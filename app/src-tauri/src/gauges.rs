//! Sistem göstergeleri (phase C, the LED live layer): processor, graphics
//! and memory load as bars on key rows, the speaker volume on the F row for
//! a moment when it changes (the knob turns it), and a key lit red while
//! the microphone is muted in Windows. RAM only (`04 20`), never the flash.
//!
//! * Readings come from `kb-winsys`, on this module's own thread: its COM
//!   and performance-counter handles must stay on one thread.
//! * Two light layers (`device::LightLayer`): `Gauges` (the bars, the other
//!   keys dark or faintly tinted) and `Alerts` (volume, microphone). The
//!   focus timer sits between them: the bars wait while it shows, the
//!   alerts stay on top (owner's decision, 2026-11-10).
//! * When the gauges are turned off or the app closes, the layers go and
//!   the keyboard returns to its saved effect by itself (measured).
//! * The page in the UI asks for readings (`watch`) even while the gauges
//!   are off, so it can show what they would look like.

use crate::device::{DeviceHandle, LightLayer, LiveFrame, Request};
use crate::dto::{Connection, GaugeBase, GaugeReadings, GaugeStyle, GaugesSettings, GaugesState, LedColor};
use crate::live::now_ms;
use crate::settings;
use kb_devices::Model;
use kb_protocol::sonix::{level_factor, Rgb};
use kb_winsys::{memory_load, Audio, CpuMeter, GpuMeter, Volume};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const EVENT_GAUGES: &str = "gauges";
const FILE: &str = "gauges.json";
const VERSION: u32 = 1;

/// Pass of the worker while the gauges run: the volume is polled and the
/// frames worked out this often (the device sends one every 50 ms).
const TICK: Duration = Duration::from_millis(100);
/// Sleep while nobody needs readings (a message wakes the worker).
const IDLE: Duration = Duration::from_secs(5);
const CPU_EVERY_MS: f64 = 500.0;
const GPU_EVERY_MS: f64 = 1_000.0;
const RAM_EVERY_MS: f64 = 2_000.0;
/// The default speakers and microphone are looked up again this often, so
/// a switch of device is followed.
const AUDIO_REFRESH_MS: f64 = 3_000.0;
/// No graphics counters: try again after this long.
const GPU_RETRY_MS: f64 = 60_000.0;
/// Opening the graphics counters takes about a second: the other readings
/// go out first.
const GPU_FIRST_MS: f64 = 300.0;
/// The page gets readings this often.
const EMIT_EVERY_MS: f64 = 250.0;
/// The volume bar shows this long after the volume changed.
const VOLUME_SHOW_MS: f64 = 1_500.0;
/// Weight of a new sample in the shown value: smooth, but quick.
const SMOOTH: f32 = 0.5;

/// The empty part of a bar: barely lit, as the timer's (owner, 2026-11-10).
const TRACK: f32 = 0.03;
/// The faint colour of the other keys (`GaugeBase::Tint`).
const TINT: f32 = 0.12;
/// Light levels change in this many steps, so frames go to the keyboard
/// only when something visible changed.
const STEPS: f32 = 32.0;

/// Key rows of the gauges (key ids of the model geometry), left to right.
const CPU_KEYS: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
const GPU_KEYS: [&str; 10] = ["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"];
const RAM_KEYS: [&str; 10] = ["a", "s", "d", "f", "g", "h", "j", "k", "l", "semicolon"];
const VOLUME_KEYS: [&str; 12] = ["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"];

const HEAT_GREEN: Rgb = [52, 199, 89];
const HEAT_AMBER: Rgb = [255, 176, 32];
const HEAT_RED: Rgb = [255, 59, 48];
const OWN_CPU: Rgb = [61, 165, 255];
const OWN_GPU: Rgb = [160, 107, 255];
const OWN_RAM: Rgb = [46, 196, 182];
const VOLUME_COLOR: Rgb = [235, 235, 235];
const MUTED_COLOR: Rgb = [255, 59, 48];
const MIC_COLOR: Rgb = [255, 40, 40];

// ----------------------------------------------------------------- frames

/// Key colours of one layer (UI colours, before brightness).
type Keyed = Vec<(u8, Rgb)>;

/// The keys of one model: every LED, and key id → LED.
#[derive(Debug, Clone, Default, PartialEq)]
struct Keys {
    all: Vec<u8>,
    by_id: HashMap<String, u8>,
}

impl Keys {
    fn of(model: &Model) -> Self {
        let Some(g) = &model.geometry else { return Self::default() };
        Self {
            all: g.keys.iter().map(|k| k.led).collect(),
            by_id: g.keys.iter().map(|k| (k.id.clone(), k.led)).collect(),
        }
    }

    /// LEDs of a row; keys the model does not have are left out.
    fn row(&self, ids: &[&str]) -> Vec<u8> {
        ids.iter().filter_map(|id| self.by_id.get(*id).copied()).collect()
    }
}

fn scale(c: Rgb, k: f32) -> Rgb {
    let k = (k.clamp(0.0, 1.0) * STEPS).round() / STEPS;
    c.map(|v| (f32::from(v) * k).round() as u8)
}

/// Colour of the key at `share` (0 first key, 1 last) of a heat bar.
fn heat(share: f32) -> Rgb {
    if share < 0.6 {
        HEAT_GREEN
    } else if share < 0.8 {
        HEAT_AMBER
    } else {
        HEAT_RED
    }
}

/// A bar of `value` (0–100) over `leds`: whole keys lit, the key on the
/// edge partly, the rest at `track`.
fn bar(leds: &[u8], value: f32, track: f32, color: impl Fn(f32) -> Rgb) -> Vec<(u8, Rgb)> {
    let n = leds.len();
    let fill = value.clamp(0.0, 100.0) / 100.0 * n as f32;
    leds.iter()
        .enumerate()
        .map(|(i, &led)| {
            let lit = (fill - i as f32).clamp(0.0, 1.0);
            let share = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 };
            (led, scale(color(share), lit.max(track)))
        })
        .collect()
}

/// Put `keys` over `frame`, replacing the LEDs both have.
fn overlay(frame: &mut Vec<(u8, Rgb)>, keys: Vec<(u8, Rgb)>) {
    for (led, c) in keys {
        match frame.iter_mut().find(|k| k.0 == led) {
            Some(k) => k.1 = c,
            None => frame.push((led, c)),
        }
    }
}

/// The bars, and the other keys dark or tinted.
fn gauges_frame(keys: &Keys, s: &GaugesSettings, r: &GaugeReadings) -> Vec<(u8, Rgb)> {
    let base = match s.base {
        GaugeBase::Dark => [0, 0, 0],
        GaugeBase::Tint => scale(s.tint, TINT),
    };
    let mut frame: Vec<(u8, Rgb)> = keys.all.iter().map(|&l| (l, base)).collect();
    let track = if s.track { TRACK } else { 0.0 };
    for (on, ids, value, own) in [
        (s.cpu, &CPU_KEYS[..], r.cpu, OWN_CPU),
        (s.gpu, &GPU_KEYS[..], r.gpu, OWN_GPU),
        (s.ram, &RAM_KEYS[..], r.ram, OWN_RAM),
    ] {
        if !on {
            continue;
        }
        let color = |share| match s.style {
            GaugeStyle::Heat => heat(share),
            GaugeStyle::Own => own,
        };
        overlay(&mut frame, bar(&keys.row(ids), value.unwrap_or(0.0), track, color));
    }
    frame
}

/// The volume bar (while it shows) and the microphone key (while muted).
fn alerts_frame(keys: &Keys, s: &GaugesSettings, r: &GaugeReadings, volume_showing: bool) -> Vec<(u8, Rgb)> {
    let mut frame = Vec::new();
    if s.volume && volume_showing {
        let row = keys.row(&VOLUME_KEYS);
        if r.muted == Some(true) {
            frame.extend(row.into_iter().map(|l| (l, MUTED_COLOR)));
        } else if let Some(v) = r.volume {
            let track = if s.track { TRACK } else { 0.0 };
            frame.extend(bar(&row, v, track, |_| VOLUME_COLOR));
        }
    }
    if s.mic && r.mic_muted == Some(true) {
        if let Some(&led) = keys.by_id.get(&s.mic_key) {
            overlay(&mut frame, vec![(led, MIC_COLOR)]);
        }
    }
    frame
}

/// The two layers to hand to the device. The gauges layer also comes with
/// an alert alone, so the other keys show the base the user chose.
fn layers(keys: &Keys, s: &GaugesSettings, r: &GaugeReadings, volume_showing: bool) -> (Option<Keyed>, Option<Keyed>) {
    let alerts = Some(alerts_frame(keys, s, r, volume_showing)).filter(|f| !f.is_empty());
    let bars = s.cpu || s.gpu || s.ram;
    let gauges = (bars || alerts.is_some()).then(|| gauges_frame(keys, s, r)).filter(|f| !f.is_empty());
    (gauges, alerts)
}

/// Notices volume changes: the bar shows for a moment after each one. The
/// first reading and a switch of device are not changes.
#[derive(Debug, Default)]
struct VolumeWatch {
    last: Option<Volume>,
    until: f64,
}

impl VolumeWatch {
    fn update(&mut self, now: f64, v: Option<Volume>) {
        if let (Some(old), Some(new)) = (&self.last, &v) {
            if old.device == new.device && ((old.level - new.level).abs() >= 0.005 || old.muted != new.muted) {
                self.until = now + VOLUME_SHOW_MS;
            }
        }
        if v.is_some() {
            self.last = v;
        }
    }

    fn showing(&self, now: f64) -> bool {
        now < self.until
    }
}

fn smooth(old: Option<f32>, new: Option<f32>) -> Option<f32> {
    match (old, new) {
        (Some(o), Some(n)) => Some(o + (n - o) * SMOOTH),
        (_, n) => n,
    }
}

// ----------------------------------------------------------------- worker

/// Readers of this PC and their latest readings; they live on the worker
/// thread only and are read without holding the shared state's lock (the
/// graphics counters take about a second to open).
struct Sensors {
    readings: GaugeReadings,
    volume: VolumeWatch,
    cpu: CpuMeter,
    gpu: Option<GpuMeter>,
    audio: Option<Audio>,
    next_cpu: f64,
    next_gpu: f64,
    next_ram: f64,
    next_audio_refresh: f64,
    started: bool,
}

impl Sensors {
    fn new() -> Self {
        Self {
            readings: GaugeReadings::default(),
            volume: VolumeWatch::default(),
            cpu: CpuMeter::new(),
            gpu: None,
            audio: None,
            next_cpu: 0.0,
            next_gpu: 0.0,
            next_ram: 0.0,
            next_audio_refresh: 0.0,
            started: false,
        }
    }

    /// Take the readings that are due.
    fn read(&mut self, now: f64) {
        let r = &mut self.readings;
        if !self.started {
            self.started = true;
            self.next_gpu = now + GPU_FIRST_MS;
            self.audio = Audio::new();
            if self.audio.is_none() {
                tracing::info!("no Core Audio: volume and microphone gauges unavailable");
            }
            self.next_audio_refresh = now + AUDIO_REFRESH_MS;
        }
        if now >= self.next_cpu {
            self.next_cpu = now + CPU_EVERY_MS;
            r.cpu = smooth(r.cpu, self.cpu.sample());
        }
        if now >= self.next_gpu {
            self.next_gpu = now + GPU_EVERY_MS;
            if self.gpu.is_none() {
                self.gpu = GpuMeter::new();
                if self.gpu.is_none() {
                    tracing::info!("no graphics performance counters; trying again later");
                    self.next_gpu = now + GPU_RETRY_MS;
                }
            } else {
                r.gpu = smooth(r.gpu, self.gpu.as_mut().and_then(GpuMeter::sample));
            }
        }
        if now >= self.next_ram {
            self.next_ram = now + RAM_EVERY_MS;
            r.ram = memory_load();
        }
        if let Some(audio) = &mut self.audio {
            if now >= self.next_audio_refresh {
                self.next_audio_refresh = now + AUDIO_REFRESH_MS;
                audio.refresh();
            }
            let v = audio.speakers();
            r.volume = v.as_ref().map(|v| v.level * 100.0);
            r.muted = v.as_ref().map(|v| v.muted);
            r.mic_muted = audio.mic_muted();
            self.volume.update(now, v);
        }
    }
}

type Frame = Option<(Vec<(u8, Rgb)>, u8)>;

struct Inner {
    settings: GaugesSettings,
    /// Copy of the worker's latest readings.
    readings: GaugeReadings,
    /// The volume bar shows until then.
    volume_until: f64,
    /// The page is open and wants readings.
    watched: bool,
    /// Last frames handed to the device (gauges, alerts), to send only changes.
    sent: (Frame, Frame),
    /// Merged frame for the page.
    shown: Vec<LedColor>,
    next_emit: f64,
    model: Option<(String, Arc<Keys>)>,
}

enum Msg {
    Wake,
}

/// Handle to the gauges worker (managed Tauri state).
pub struct GaugesHandle {
    tx: Sender<Msg>,
    inner: Arc<Mutex<Inner>>,
    app: AppHandle,
    config_dir: Option<PathBuf>,
}

impl GaugesHandle {
    pub fn spawn(app: AppHandle, device: Arc<DeviceHandle>, config_dir: Option<PathBuf>) -> Arc<Self> {
        let settings = config_dir
            .as_deref()
            .and_then(|d| settings::load::<GaugesSettings>(d, FILE, VERSION))
            .unwrap_or_default()
            .sanitized();
        if settings.enabled {
            tracing::info!("system gauges on");
        }
        let inner = Arc::new(Mutex::new(Inner {
            settings,
            readings: GaugeReadings::default(),
            volume_until: 0.0,
            watched: false,
            sent: (None, None),
            shown: Vec::new(),
            next_emit: 0.0,
            model: None,
        }));
        let (tx, rx) = mpsc::channel();
        let handle = Arc::new(Self { tx, inner, app, config_dir });
        let worker = handle.clone();
        std::thread::Builder::new()
            .name("gauges".into())
            .spawn(move || {
                let mut sensors = Sensors::new();
                loop {
                    let wait = worker.tick(&device, &mut sensors);
                    match rx.recv_timeout(wait) {
                        Ok(Msg::Wake) | Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("spawn gauges thread");
        handle
    }

    fn wake(&self) {
        let _ = self.tx.send(Msg::Wake);
    }

    pub fn state(&self) -> Result<GaugesState, String> {
        let inner = self.inner.lock().map_err(|_| "gauges unavailable")?;
        Ok(Self::state_of(&inner, now_ms()))
    }

    fn state_of(inner: &Inner, now: f64) -> GaugesState {
        GaugesState {
            settings: inner.settings.clone(),
            readings: inner.readings,
            frame: inner.shown.clone(),
            volume_showing: now < inner.volume_until,
        }
    }

    pub fn configure(&self, settings: GaugesSettings) -> Result<GaugesState, String> {
        let settings = settings.sanitized();
        {
            let mut inner = self.inner.lock().map_err(|_| "gauges unavailable")?;
            if inner.settings.enabled != settings.enabled {
                tracing::info!(on = settings.enabled, "system gauges");
            }
            inner.settings = settings.clone();
            inner.next_emit = 0.0;
        }
        if let Some(dir) = &self.config_dir {
            settings::save(dir, FILE, VERSION, settings).map_err(|e| e.to_string())?;
        }
        self.wake();
        self.state()
    }

    /// The page opened (`true`) or closed: readings are wanted while it is open.
    pub fn watch(&self, on: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.watched = on;
            inner.next_emit = 0.0;
        }
        self.wake();
    }

    /// One pass of the worker; returns how long to sleep.
    fn tick(&self, device: &DeviceHandle, sensors: &mut Sensors) -> Duration {
        let now = now_ms();
        let Some(active) = self.inner.lock().ok().map(|i| i.settings.enabled || i.watched) else { return IDLE };
        if active {
            sensors.read(now);
        }
        let showing = sensors.volume.showing(now);
        let Ok(mut guard) = self.inner.lock() else { return IDLE };
        let inner = &mut *guard;
        inner.readings = sensors.readings;
        inner.volume_until = sensors.volume.until;
        let s = inner.settings.clone();
        // Settings may have changed while reading: take them as they are now.
        let active = s.enabled || inner.watched;

        // The keyboard on the cable gets the layers; without one, the page
        // still shows what they would look like on the catalogue's model.
        let usb_keys = self.usb_keys(inner, device);
        let keys = usb_keys.clone().or_else(fallback_keys);
        let (gauges, alerts) = match &keys {
            Some(k) if active => layers(k, &s, &inner.readings, showing),
            _ => (None, None),
        };

        let mut shown = gauges.clone().unwrap_or_default();
        if let Some(a) = &alerts {
            overlay(&mut shown, a.clone());
        }
        inner.shown = shown.into_iter().map(|(led, [r, g, b])| LedColor { led, r, g, b }).collect();

        let on_keys = s.enabled && usb_keys.is_some();
        let send = |f: Option<Vec<(u8, Rgb)>>| f.filter(|_| on_keys).map(|f| (f, s.brightness));
        let (gauges, alerts) = (send(gauges), send(alerts));
        for (layer, frame, sent) in
            [(LightLayer::Gauges, gauges, &mut inner.sent.0), (LightLayer::Alerts, alerts, &mut inner.sent.1)]
        {
            if frame != *sent {
                let _ = device.send(Request::SetLayer(
                    layer,
                    frame.clone().map(|(colors, b)| LiveFrame { colors, level: level_factor(b) }),
                ));
                *sent = frame;
            }
        }

        if inner.watched && now >= inner.next_emit {
            inner.next_emit = now + EMIT_EVERY_MS;
            let _ = self.app.emit(EVENT_GAUGES, Self::state_of(inner, now));
        }
        if active {
            TICK
        } else {
            IDLE
        }
    }

    /// Keys of the keyboard on the USB cable (the only connection that takes
    /// live frames), cached per model.
    fn usb_keys(&self, inner: &mut Inner, device: &DeviceHandle) -> Option<Arc<Keys>> {
        let model_id = device.snapshot.lock().ok().and_then(|s| {
            s.keyboards
                .iter()
                .find(|k| k.connection == Connection::Usb && k.has_command_channel)
                .map(|k| k.model_id.clone())
        })?;
        if inner.model.as_ref().is_none_or(|(id, _)| *id != model_id) {
            let model = kb_devices::catalogue().ok()?.iter().find(|m| m.id == model_id)?;
            inner.model = Some((model_id, Arc::new(Keys::of(model))));
        }
        inner.model.as_ref().map(|(_, k)| k.clone())
    }
}

/// The catalogue's first model, for the page's preview without a keyboard.
fn fallback_keys() -> Option<Arc<Keys>> {
    static KEYS: OnceLock<Option<Arc<Keys>>> = OnceLock::new();
    KEYS.get_or_init(|| kb_devices::catalogue().ok().and_then(|c| c.first()).map(|m| Arc::new(Keys::of(m)))).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Keys {
        let model = kb_devices::catalogue().unwrap().iter().find(|m| m.id.contains("ak980")).unwrap();
        Keys::of(model)
    }

    fn led(k: &Keys, id: &str) -> u8 {
        k.by_id[id]
    }

    fn color(frame: &[(u8, Rgb)], led: u8) -> Option<Rgb> {
        frame.iter().find(|k| k.0 == led).map(|k| k.1)
    }

    #[test]
    fn a_bar_lights_whole_keys_and_part_of_the_edge_key() {
        let leds: Vec<u8> = (1..=10).collect();
        let f = bar(&leds, 35.0, 0.0, |_| [200, 200, 200]);
        assert_eq!(f[0].1, [200; 3]);
        assert_eq!(f[2].1, [200; 3]);
        // Half of the fourth key: 0.5 → 100.
        assert_eq!(f[3].1, [100; 3]);
        assert_eq!(f[4].1, [0; 3]);
        // With a track the empty keys are barely lit.
        let f = bar(&leds, 0.0, TRACK, |_| [255, 255, 255]);
        assert_eq!(f[9].1, scale([255; 3], TRACK));
        assert_ne!(f[9].1, [0; 3]);
    }

    #[test]
    fn heat_goes_green_yellow_red_along_the_row() {
        let leds: Vec<u8> = (1..=10).collect();
        let f = bar(&leds, 100.0, 0.0, heat);
        assert_eq!(f[0].1, HEAT_GREEN);
        assert_eq!(f[5].1, HEAT_GREEN);
        assert_eq!(f[6].1, HEAT_AMBER);
        assert_eq!(f[7].1, HEAT_AMBER);
        assert_eq!(f[8].1, HEAT_RED);
        assert_eq!(f[9].1, HEAT_RED);
    }

    #[test]
    fn gauges_sit_on_their_rows_and_the_rest_is_the_base() {
        let k = keys();
        let s = GaugesSettings { ram: true, ..GaugesSettings::default() };
        let r = GaugeReadings { cpu: Some(100.0), gpu: Some(0.0), ram: Some(50.0), ..Default::default() };
        let f = gauges_frame(&k, &s, &r);
        assert_eq!(f.len(), k.all.len());
        assert_eq!(color(&f, led(&k, "1")), Some(HEAT_GREEN));
        assert_eq!(color(&f, led(&k, "0")), Some(HEAT_RED));
        assert_eq!(color(&f, led(&k, "q")), Some(scale(HEAT_GREEN, TRACK)));
        assert_eq!(color(&f, led(&k, "g")), Some(HEAT_GREEN));
        assert_eq!(color(&f, led(&k, "h")), Some(scale(HEAT_GREEN, TRACK)));
        assert_eq!(color(&f, led(&k, "space")), Some([0, 0, 0]));
        let tinted = GaugesSettings { base: GaugeBase::Tint, style: GaugeStyle::Own, ..s };
        let f = gauges_frame(&k, &tinted, &r);
        assert_eq!(color(&f, led(&k, "space")), Some(scale(tinted.tint, TINT)));
        assert_eq!(color(&f, led(&k, "1")), Some(OWN_CPU));
    }

    #[test]
    fn alerts_show_the_volume_for_a_moment_and_the_muted_microphone() {
        let k = keys();
        let s = GaugesSettings::default();
        let r = GaugeReadings { volume: Some(50.0), muted: Some(false), mic_muted: Some(true), ..Default::default() };
        let f = alerts_frame(&k, &s, &r, true);
        assert_eq!(color(&f, led(&k, "f1")), Some(VOLUME_COLOR));
        assert_eq!(color(&f, led(&k, "f6")), Some(VOLUME_COLOR));
        assert_eq!(color(&f, led(&k, "f7")), Some(scale(VOLUME_COLOR, TRACK)));
        assert_eq!(color(&f, led(&k, "delete")), Some(MIC_COLOR));
        // After the moment only the microphone key is left.
        assert_eq!(alerts_frame(&k, &s, &r, false), vec![(led(&k, "delete"), MIC_COLOR)]);
        // Muted speakers: the whole F row red.
        let muted = GaugeReadings { muted: Some(true), mic_muted: Some(false), ..r };
        let f = alerts_frame(&k, &s, &muted, true);
        assert!(VOLUME_KEYS.iter().all(|id| color(&f, led(&k, id)) == Some(MUTED_COLOR)));
    }

    #[test]
    fn layers_only_when_something_shows() {
        let k = keys();
        let r = GaugeReadings { cpu: Some(10.0), mic_muted: Some(false), ..Default::default() };
        let only_alerts = GaugesSettings { cpu: false, gpu: false, ram: false, ..GaugesSettings::default() };
        assert_eq!(layers(&k, &only_alerts, &r, false), (None, None));
        // An alert alone brings the base along, so the other keys stay as chosen.
        let muted = GaugeReadings { mic_muted: Some(true), ..r };
        let (g, a) = layers(&k, &only_alerts, &muted, false);
        assert_eq!(g.map(|f| f.len()), Some(k.all.len()));
        assert_eq!(a, Some(vec![(led(&k, "delete"), MIC_COLOR)]));
        let (g, a) = layers(&k, &GaugesSettings::default(), &r, false);
        assert!(g.is_some() && a.is_none());
    }

    #[test]
    fn the_volume_bar_follows_changes_not_the_first_reading_or_a_device_switch() {
        let v = |level: f32, muted: bool, device: &str| Some(Volume { level, muted, device: device.into() });
        let mut w = VolumeWatch::default();
        w.update(0.0, v(0.4, false, "a"));
        assert!(!w.showing(0.0));
        w.update(100.0, v(0.4, false, "a"));
        assert!(!w.showing(100.0));
        w.update(200.0, v(0.46, false, "a"));
        assert!(w.showing(200.0) && w.showing(1_699.0) && !w.showing(1_700.0));
        w.update(3_000.0, v(0.46, true, "a"));
        assert!(w.showing(3_000.0));
        w.update(9_000.0, v(0.9, false, "b"));
        assert!(!w.showing(9_000.0));
        // A failed read keeps the last value: the next good one is compared to it.
        w.update(9_100.0, None);
        w.update(9_200.0, v(0.9, false, "b"));
        assert!(!w.showing(9_200.0));
    }

    #[test]
    fn readings_are_smoothed_but_follow() {
        assert_eq!(smooth(None, Some(40.0)), Some(40.0));
        assert_eq!(smooth(Some(40.0), Some(80.0)), Some(60.0));
        assert_eq!(smooth(Some(40.0), None), None);
    }

    #[test]
    fn settings_repair_a_bad_microphone_key() {
        let s = GaugesSettings { mic_key: "../x".into(), brightness: 9, ..GaugesSettings::default() }.sanitized();
        assert_eq!((s.mic_key.as_str(), s.brightness), ("delete", 5));
        let s = GaugesSettings { mic_key: "kp-minus".into(), ..GaugesSettings::default() }.sanitized();
        assert_eq!(s.mic_key, "kp-minus");
    }
}
