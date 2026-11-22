//! Müzik modu (music mode): the key lights move with the sound the PC
//! plays. RAM only (`04 20`), never the flash; USB only (the other
//! connections take no live frames).
//!
//! * The sound comes from `kb_winsys::loopback` (Windows' own capture of
//!   the default speakers or headphones; no microphone), read on this
//!   module's thread, which owns the COM objects. Only levels are kept
//!   ([`crate::spectrum`]); nothing is recorded, saved or sent anywhere.
//! * One light layer, `device::LightLayer::Music`: over the gauges and the
//!   per-app profile, under the game's lights and the focus timer, with the
//!   alerts, reminders and nudges on top (owner's order, 2026-11-22). A
//!   game that wants the keyboard left alone hides it like every layer.
//! * Four modes, ported from the preview the owner approved (2026-11-22):
//!   Bars, Beat, Level and Wave ([`frame`]). The shapes are worked out in
//!   the AK980 PRO's key units and scaled to the model's width.
//! * Silence: the lights start when two frames in a row are louder than
//!   −50 dBFS and stop after 3 s under −60 dBFS (or with Windows' sound
//!   muted); then the keyboard's own light shows again, or every key dim
//!   or dark, as chosen ([`Silence`]).
//! * "Pause while playing a game": while a game session's game is in front
//!   the lights wait (and nothing is listened to).
//! * The page asks for frames (`watch`) even while music mode is off, so it
//!   can show what the lights would look like; the sound is read then too.
//! * 20 frames a second, the rate measured to be safe (plan §7).

use crate::device::{DeviceHandle, LightLayer, LiveFrame, Request};
use crate::dto::{
    Connection, LedColor, MusicMode, MusicPalette, MusicRest, MusicSettings, MusicSilence, MusicState, MusicStatus,
};
use crate::games::GamesHandle;
use crate::live::now_ms;
use crate::settings;
use crate::spectrum::{Analyzer, Levels};
use kb_devices::Model;
use kb_protocol::sonix::{level_factor, Rgb};
use kb_winsys::loopback::{Capture, State as Sound};
use kb_winsys::Audio;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const EVENT_MUSIC: &str = "music";
const FILE: &str = "music.json";
const VERSION: u32 = 1;

/// One frame: 20 a second.
const TICK: Duration = Duration::from_millis(50);
/// Sleep while nobody needs the lights (a message wakes the worker).
const IDLE: Duration = Duration::from_secs(5);
/// The page gets frames this often.
const EMIT_EVERY_MS: f64 = 80.0;
/// Windows' volume and mute are looked up again this often.
const AUDIO_REFRESH_MS: f64 = 3_000.0;
/// Bands of the analysis, and columns of the bars.
const COLUMNS: usize = 20;

/// Below this the sound counts as silence (dBFS)…
const QUIET_DB: f32 = -60.0;
/// …and two frames in a row above this start the lights again.
const SOUND_DB: f32 = -50.0;
/// The lights step aside after this long in silence (owner, 2026-11-22).
const QUIET_AFTER_MS: f64 = 3_000.0;

/// The preview's numbers are in the AK980 PRO's key units.
const REF_WIDTH: f32 = 20.5;
/// How a share of light shows on a key: `a^0.6`, as in the preview.
const GAMMA: f32 = 0.6;
/// The other keys when they are dim.
const DIM: f32 = 0.06;
/// Bars: a little over the analysis, so loud music reaches the top row.
const BAR_GAIN: f32 = 1.15;
/// Bars: a peak mark shows above this height.
const MARK_MIN: f32 = 0.08;
const MARK_LIGHT: f32 = 0.8;
/// Beat: the light between beats, and how much the loudness adds to it.
const BEAT_BASE: f32 = 0.12;
const BEAT_BREATH: f32 = 0.3;
/// Beat: a pulse dies away with this time constant (s)…
const PULSE_SECS: f32 = 0.25;
/// …the ring spreads this fast (key units per second) and fades so.
const RING_SPEED: f32 = 22.0;
const RING_SECS: f32 = 0.6;
/// Every fourth beat is strong (a bar of four, as in the preview), and the
/// first after this long without beats (s).
const STRONG_EVERY: u32 = 4;
const STRONG_AFTER_PAUSE: f64 = 2.0;
/// Level: the marker at the edge holds this long (s), then falls (share of
/// the half width per second).
const LEVEL_HOLD: f64 = 0.5;
const LEVEL_FALL: f32 = 0.47;
const LEVEL_MARK_LIGHT: f32 = 0.85;
/// Wave: at most this many at once; speed (key units per second), from
/// the left and from the middle; how long one lasts (time constant, s).
const MAX_WAVES: usize = 6;
const WAVE_SPEED: f32 = 24.0;
const WAVE_SPEED_CENTER: f32 = 13.0;
const WAVE_SECS: f32 = 1.2;
/// A wave older than this is gone (s).
const WAVE_GONE: f64 = 6.0;
/// Silence in "Dim colour": the palette's colour this far up, this bright.
const QUIET_DIM_AT: f32 = 0.2;
const QUIET_DIM: f32 = 0.07;

// ------------------------------------------------------------------- keys

/// One key: its LED, centre (key units) and row counted from the bottom.
#[derive(Debug, Clone, PartialEq)]
struct Key {
    led: u8,
    cx: f32,
    cy: f32,
    row: u8,
    col: usize,
}

/// The keys of one model, as the modes see them.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Keys {
    keys: Vec<Key>,
    width: f32,
    height: f32,
    rows: u8,
}

impl Keys {
    pub(crate) fn of(model: &Model) -> Self {
        let Some(g) = &model.geometry else { return Self::default() };
        let width = g.width.max(1.0);
        // Rows: keys whose tops are within half a key of each other (the
        // arrow keys sit a little lower than their row).
        let mut tops: Vec<f32> = g.keys.iter().map(|k| k.y).collect();
        tops.sort_by(f32::total_cmp);
        let mut starts: Vec<f32> = Vec::new();
        for t in tops {
            if starts.last().is_none_or(|s| t - s > 0.5) {
                starts.push(t);
            }
        }
        let rows = starts.len().clamp(1, usize::from(u8::MAX)) as u8;
        let keys = g
            .keys
            .iter()
            .map(|k| {
                let from_top = starts.iter().rposition(|s| k.y >= s - 1e-3).unwrap_or(0) as u8;
                let cx = k.x + k.w / 2.0;
                Key {
                    led: k.led,
                    cx,
                    cy: k.y + k.h / 2.0,
                    row: rows - 1 - from_top.min(rows - 1),
                    col: ((cx / width * COLUMNS as f32) as usize).min(COLUMNS - 1),
                }
            })
            .collect();
        Self { keys, width, height: g.height.max(1.0), rows }
    }

    /// Key units per AK980 PRO key unit.
    fn unit(&self) -> f32 {
        self.width / REF_WIDTH
    }
}

// ----------------------------------------------------------------- colours

fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(360.0) / 60.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, u) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as u32 % 6 {
        0 => (v, u, p),
        1 => (q, v, p),
        2 => (p, v, u),
        3 => (p, q, v),
        4 => (u, p, v),
        _ => (v, p, q),
    };
    [r, g, b].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn lerp(a: Rgb, b: Rgb, f: f32) -> Rgb {
    let f = f.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * f).round() as u8)
}

/// The palette's colour at `f` (0–1).
fn palette(s: &MusicSettings, f: f32) -> Rgb {
    match s.palette {
        MusicPalette::Heat => hsv(120.0 - f.clamp(0.0, 1.0) * 120.0, 1.0, 1.0),
        MusicPalette::Rainbow => hsv(f.clamp(0.0, 1.0) * 300.0, 1.0, 1.0),
        MusicPalette::One => s.color,
        MusicPalette::Two => lerp(s.color, s.color2, f),
    }
}

/// `c` lit to `a` (0–1).
fn paint(c: Rgb, a: f32) -> Rgb {
    let k = a.clamp(0.0, 1.0).powf(GAMMA);
    c.map(|v| (f32::from(v) * k).round() as u8)
}

fn rest_light(s: &MusicSettings) -> f32 {
    match s.rest {
        MusicRest::Dark => 0.0,
        MusicRest::Dim => DIM,
    }
}

// ----------------------------------------------------------------- moments

/// What the modes draw from at one moment: the analysis and the beats'
/// history.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Moment {
    /// Band heights (0–1), bass first, and their peak marks.
    pub bands: Vec<f32>,
    pub peaks: Vec<f32>,
    /// Overall loudness (0–1).
    pub loud: f32,
    /// Level: how far out (0–1 of the half width) the marker is.
    pub loud_peak: f32,
    /// When the last beat and the last strong beat were (s).
    pub beat: Option<f64>,
    pub strong: Option<f64>,
    /// Beats so far (Beat's rainbow moves on with each).
    pub beats: u32,
    /// Waves under way: start (s) and hue (degrees).
    pub waves: Vec<(f64, f32)>,
}

/// Turns analyses into [`Moment`]s: counts the beats, starts the waves and
/// moves the Level marker.
#[derive(Debug, Clone, Default)]
pub(crate) struct Tracker {
    seen: Option<f64>,
    beats: u32,
    strong: Option<f64>,
    waves: VecDeque<(f64, f32)>,
    loud_peak: f32,
    loud_peak_at: f64,
    last: Option<f64>,
}

impl Tracker {
    pub(crate) fn step(&mut self, l: &Levels, now: f64) -> Moment {
        let dt = self.last.map_or(0.0, |t| (now - t).clamp(0.0, 0.5)) as f32;
        self.last = Some(now);
        if let Some(beat) = l.beat.filter(|b| Some(*b) != self.seen) {
            let after_pause = self.seen.is_none_or(|t| beat - t >= STRONG_AFTER_PAUSE);
            self.seen = Some(beat);
            self.beats = self.beats.wrapping_add(1);
            if after_pause || self.beats % STRONG_EVERY == 1 {
                self.strong = Some(beat);
            }
            let hue = (self.beats.wrapping_mul(47) % 360) as f32;
            self.waves.push_back((beat, hue));
            while self.waves.len() > MAX_WAVES {
                self.waves.pop_front();
            }
        }
        while self.waves.front().is_some_and(|w| now - w.0 > WAVE_GONE) {
            self.waves.pop_front();
        }
        if l.loud >= self.loud_peak {
            self.loud_peak = l.loud;
            self.loud_peak_at = now;
        } else if now - self.loud_peak_at > LEVEL_HOLD {
            self.loud_peak = (self.loud_peak - LEVEL_FALL * dt).max(l.loud);
        }
        Moment {
            bands: l.bands.clone(),
            peaks: l.peaks.clone(),
            loud: l.loud,
            loud_peak: self.loud_peak,
            beat: self.seen,
            strong: self.strong,
            beats: self.beats,
            waves: self.waves.iter().copied().collect(),
        }
    }
}

// ------------------------------------------------------------------ frames

/// `values` read at `p` (0–1), between the two nearest.
fn at(values: &[f32], p: f32) -> f32 {
    let Some(last) = values.len().checked_sub(1) else { return 0.0 };
    let f = p.clamp(0.0, 1.0) * last as f32;
    let i = (f.floor() as usize).min(last);
    let g = f - i as f32;
    values[i] * (1.0 - g) + values[(i + 1).min(last)] * g
}

/// Bars: each column's height and peak mark (0–1).
fn columns(m: &Moment, center: bool) -> Vec<(f32, f32)> {
    (0..COLUMNS)
        .map(|c| {
            // The column's middle, as a share of the width (in whole
            // numbers first, so the two halves match exactly).
            let (twice, n) = ((2 * c + 1) as f32, COLUMNS as f32);
            let p = if center { (twice - n).abs() / n } else { twice / (2.0 * n) };
            ((at(&m.bands, p) * BAR_GAIN).min(1.0), (at(&m.peaks, p) * BAR_GAIN).min(1.0))
        })
        .collect()
}

/// What the music lights put on every key at `now` (s): UI colours, before
/// brightness. The approved preview's look, on the model's own keys.
pub(crate) fn frame(keys: &Keys, m: &Moment, s: &MusicSettings, now: f64) -> Vec<(u8, Rgb)> {
    let rest = rest_light(s);
    let u = keys.unit();
    let (mid_x, mid_y) = (keys.width / 2.0, keys.height / 2.0);
    let rows = f32::from(keys.rows);
    let top = f32::from(keys.rows.saturating_sub(1).max(1));
    match s.mode {
        MusicMode::Bars => {
            let cols = columns(m, s.center);
            keys.keys
                .iter()
                .map(|k| {
                    let (v, peak) = cols[k.col];
                    let row = f32::from(k.row);
                    let fill = (v * rows - row).clamp(0.0, 1.0);
                    let f = if s.palette == MusicPalette::Rainbow {
                        k.col as f32 / (COLUMNS - 1) as f32
                    } else {
                        row / top
                    };
                    let c = palette(s, f);
                    let mark = ((peak * rows).floor() as u8).min(keys.rows.saturating_sub(1));
                    let a = if fill > 0.0 {
                        fill.max(rest)
                    } else if k.row == mark && peak > MARK_MIN {
                        MARK_LIGHT
                    } else {
                        rest
                    };
                    (k.led, paint(c, a))
                })
                .collect()
        }
        MusicMode::Beat => {
            let pulse = m.beat.map_or(0.0, |b| (-((now - b).max(0.0) as f32) / PULSE_SECS).exp());
            let base = BEAT_BASE + BEAT_BREATH * m.loud;
            keys.keys
                .iter()
                .map(|k| {
                    let ring = m.strong.map_or(0.0, |t0| {
                        let age = (now - t0).max(0.0) as f32;
                        let radius = age * RING_SPEED * u;
                        let d = (k.cx - mid_x).hypot((k.cy - mid_y) * 1.3);
                        (-((d - radius) / u).powi(2) / 1.6).exp() * (-age / RING_SECS).exp()
                    });
                    let c = match s.palette {
                        MusicPalette::Rainbow => hsv(m.beats as f32 * 40.0, 1.0, 1.0),
                        MusicPalette::Heat => hsv(120.0 - m.loud.clamp(0.0, 1.0) * 120.0, 1.0, 1.0),
                        MusicPalette::Two => lerp(s.color, s.color2, ring.min(1.0)),
                        MusicPalette::One => s.color,
                    };
                    (k.led, paint(c, base + 0.85 * pulse + ring * 0.8))
                })
                .collect()
        }
        MusicMode::Level => {
            let reach = mid_x + 0.35 * u;
            let (half, mark) = (m.loud.clamp(0.0, 1.0) * reach, m.loud_peak.clamp(0.0, 1.0) * reach);
            keys.keys
                .iter()
                .map(|k| {
                    let d = (k.cx - mid_x).abs();
                    let fill = ((half - d) / u + 0.5).clamp(0.0, 1.0);
                    let c = palette(s, d / mid_x);
                    let a = if fill > 0.0 {
                        fill.max(rest)
                    } else if (d - mark).abs() < 0.55 * u && mark > 0.6 * u {
                        LEVEL_MARK_LIGHT
                    } else {
                        rest
                    };
                    (k.led, paint(c, a))
                })
                .collect()
        }
        MusicMode::Wave => keys
            .keys
            .iter()
            .map(|k| {
                let pos = if s.center { (k.cx - mid_x).abs() } else { k.cx };
                let speed = if s.center { WAVE_SPEED_CENTER } else { WAVE_SPEED };
                let (light, hue) = m.waves.iter().fold((0.0f32, 0.0f32), |best, &(t0, hue)| {
                    let age = (now - t0).max(0.0) as f32;
                    let front = age * speed * u;
                    let i = (-((pos - front) / u).powi(2) / 2.9).exp() * (-age / WAVE_SECS).exp();
                    if i > best.0 {
                        (i, hue)
                    } else {
                        best
                    }
                });
                let c = match s.palette {
                    MusicPalette::Rainbow => hsv(hue, 1.0, 1.0),
                    _ => palette(s, if s.center { (k.cx - mid_x).abs() / mid_x } else { k.cx / keys.width }),
                };
                (k.led, paint(c, (light * (0.5 + m.loud)).min(1.0).max(rest)))
            })
            .collect(),
    }
}

/// What the keys show once it is quiet: `None` gives the keyboard its own
/// light back.
fn quiet_frame(keys: &Keys, s: &MusicSettings) -> Option<Vec<(u8, Rgb)>> {
    let c = match s.silence {
        MusicSilence::Saved => return None,
        MusicSilence::Dim => paint(palette(s, QUIET_DIM_AT), QUIET_DIM),
        MusicSilence::Dark => [0, 0, 0],
    };
    Some(keys.keys.iter().map(|k| (k.led, c)).collect())
}

// ----------------------------------------------------------------- silence

/// Whether the music lights show: sound starts them, silence stops them
/// after [`QUIET_AFTER_MS`]. Between [`QUIET_DB`] and [`SOUND_DB`] nothing
/// changes, so a quiet passage neither starts nor stops them.
#[derive(Debug, Clone, Default)]
struct Silence {
    showing: bool,
    loud_frames: u8,
    quiet_since: Option<f64>,
}

impl Silence {
    /// One frame's level (dBFS; `None`: nothing to hear: no stream, Windows'
    /// sound muted). True while the lights show.
    fn step(&mut self, db: Option<f32>, now: f64) -> bool {
        let db = db.unwrap_or(f32::NEG_INFINITY);
        self.loud_frames = if db > SOUND_DB { self.loud_frames.saturating_add(1) } else { 0 };
        if db >= QUIET_DB {
            self.quiet_since = None;
        } else if self.quiet_since.is_none() {
            self.quiet_since = Some(now);
        }
        if !self.showing && self.loud_frames >= 2 {
            self.showing = true;
        } else if self.showing && self.quiet_since.is_some_and(|t| now - t >= QUIET_AFTER_MS) {
            self.showing = false;
        }
        self.showing
    }
}

/// Where the lights stand, from what the worker knows.
fn status_of(s: &MusicSettings, usb: bool, sound: Sound, game: bool, showing: bool) -> MusicStatus {
    match sound {
        _ if !s.enabled => MusicStatus::Off,
        _ if !usb => MusicStatus::NoUsb,
        _ if game => MusicStatus::Game,
        Sound::Unavailable => MusicStatus::Unavailable,
        Sound::NoOutput => MusicStatus::NoOutput,
        _ if showing => MusicStatus::Playing,
        _ => MusicStatus::Waiting,
    }
}

// ------------------------------------------------------------------ worker

/// The sound readers: on the worker thread only (COM).
#[derive(Default)]
struct Ears {
    capture: Option<Capture>,
    audio: Option<Audio>,
    analyzer: Option<Analyzer>,
    samples: Vec<f32>,
    next_refresh: f64,
    was_open: bool,
}

/// What one listen got.
struct Heard {
    sound: Sound,
    levels: Levels,
    /// Windows' sound is muted or at zero.
    muted: bool,
    /// How long until listening again is worth it.
    wait: Duration,
}

impl Ears {
    fn listen(&mut self, now: f64, sensitivity: u16) -> Heard {
        let capture = self.capture.get_or_insert_with(|| {
            tracing::debug!("music mode listens to the sound output");
            Capture::new()
        });
        if now >= self.next_refresh {
            self.next_refresh = now + AUDIO_REFRESH_MS;
            match &mut self.audio {
                Some(a) => a.refresh(),
                None => self.audio = Audio::new(),
            }
        }
        self.samples.clear();
        let poll = capture.poll(&mut self.samples);
        let open = poll.state == Sound::Open;
        if open && self.analyzer.as_ref().is_none_or(|a| a.rate() != poll.rate) {
            self.analyzer = Some(Analyzer::new(poll.rate, COLUMNS));
        }
        let analyzer = self.analyzer.get_or_insert_with(|| Analyzer::new(48_000, COLUMNS));
        analyzer.set_sensitivity(sensitivity);
        if poll.reset || (self.was_open && !open) {
            analyzer.reset();
        }
        self.was_open = open;
        if open {
            if poll.frames == 0 {
                // Nothing playing gives no packets: that is silence, not
                // the last window again.
                let silence = (f64::from(poll.rate) * TICK.as_secs_f64()) as usize;
                self.samples.resize(silence, 0.0);
            }
            analyzer.push(&self.samples);
        }
        let levels = analyzer.analyze(now / 1000.0);
        let muted = self.audio.as_mut().and_then(Audio::speakers).is_some_and(|v| v.muted || v.level <= 0.0);
        Heard { sound: poll.state, levels, muted, wait: if open { TICK } else { capture.wait() } }
    }

    /// Stop listening (the stream, the meter and COM go).
    fn close(&mut self) {
        if self.capture.is_some() {
            tracing::debug!("music mode stopped listening");
        }
        *self = Self::default();
    }
}

type Frame = Option<(Vec<(u8, Rgb)>, u8)>;

struct Inner {
    settings: MusicSettings,
    /// The page is open and wants frames.
    watched: bool,
    status: MusicStatus,
    /// Last frame handed to the device, to send only changes.
    sent: Frame,
    /// The frame for the page.
    shown: Vec<LedColor>,
    next_emit: f64,
    model: Option<(String, Arc<Keys>)>,
    silence: Silence,
    tracker: Tracker,
}

enum Msg {
    Wake,
}

/// Handle to the music worker (managed Tauri state).
pub struct MusicHandle {
    tx: Sender<Msg>,
    inner: Arc<Mutex<Inner>>,
    app: AppHandle,
    config_dir: Option<PathBuf>,
}

impl MusicHandle {
    pub fn spawn(
        app: AppHandle,
        device: Arc<DeviceHandle>,
        games: Arc<GamesHandle>,
        config_dir: Option<PathBuf>,
    ) -> Arc<Self> {
        let settings = config_dir
            .as_deref()
            .and_then(|d| settings::load::<MusicSettings>(d, FILE, VERSION))
            .unwrap_or_default()
            .sanitized();
        if settings.enabled {
            tracing::info!("music mode on");
        }
        let inner = Arc::new(Mutex::new(Inner {
            settings,
            watched: false,
            status: MusicStatus::Off,
            sent: None,
            shown: Vec::new(),
            next_emit: 0.0,
            model: None,
            silence: Silence::default(),
            tracker: Tracker::default(),
        }));
        let (tx, rx) = mpsc::channel();
        let handle = Arc::new(Self { tx, inner, app, config_dir });
        let worker = handle.clone();
        std::thread::Builder::new()
            .name("music".into())
            .spawn(move || {
                let mut ears = Ears::default();
                loop {
                    let wait = worker.tick(&device, &games, &mut ears);
                    match rx.recv_timeout(wait) {
                        Ok(Msg::Wake) | Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("spawn music thread");
        handle
    }

    fn wake(&self) {
        let _ = self.tx.send(Msg::Wake);
    }

    pub fn state(&self) -> Result<MusicState, String> {
        let inner = self.inner.lock().map_err(|_| "music mode unavailable")?;
        Ok(Self::state_of(&inner))
    }

    fn state_of(inner: &Inner) -> MusicState {
        MusicState { settings: inner.settings.clone(), status: inner.status, frame: inner.shown.clone() }
    }

    pub fn configure(&self, settings: MusicSettings) -> Result<MusicState, String> {
        let settings = settings.sanitized();
        {
            let mut inner = self.inner.lock().map_err(|_| "music mode unavailable")?;
            if inner.settings.enabled != settings.enabled {
                tracing::info!(on = settings.enabled, "music mode");
            }
            inner.status = status_of(&settings, false, Sound::Quiet, false, false);
            inner.settings = settings.clone();
            inner.next_emit = 0.0;
        }
        if let Some(dir) = &self.config_dir {
            settings::save(dir, FILE, VERSION, settings).map_err(|e| e.to_string())?;
        }
        self.wake();
        self.state()
    }

    /// The page opened (`true`) or closed: frames are wanted while it is open.
    pub fn watch(&self, on: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.watched = on;
            inner.next_emit = 0.0;
        }
        self.wake();
    }

    /// One pass of the worker; returns how long to sleep.
    fn tick(&self, device: &DeviceHandle, games: &GamesHandle, ears: &mut Ears) -> Duration {
        let now = now_ms();
        let (in_front, quiet) = (games.playing_in_front(), games.quiet_now());
        let (watched, settings, usb) = match self.inner.lock() {
            Ok(mut i) => (i.watched, i.settings.clone(), self.usb_keys(&mut i, device)),
            Err(_) => return IDLE,
        };
        let game = (settings.pause_in_games && in_front) || quiet;
        let on_keys = settings.enabled && usb.is_some() && !game;
        let heard = (on_keys || watched).then(|| ears.listen(now, settings.sensitivity));
        if heard.is_none() {
            ears.close();
        }

        let Ok(mut guard) = self.inner.lock() else { return IDLE };
        let inner = &mut *guard;
        // Settings may have changed while listening: take them as they are now.
        let s = inner.settings.clone();
        let on_keys = on_keys && s.enabled;
        let (lights, wait) = match &heard {
            Some(h) => {
                let db = (h.sound == Sound::Open && !h.muted).then_some(h.levels.db);
                let showing = inner.silence.step(db, now);
                let moment = inner.tracker.step(&h.levels, now / 1000.0);
                let keys = usb.clone().or_else(fallback_keys);
                let lights = match keys.as_deref() {
                    _ if game => None,
                    Some(k) if showing => Some(frame(k, &moment, &s, now / 1000.0)),
                    Some(k) => quiet_frame(k, &s),
                    None => None,
                };
                inner.status = status_of(&s, usb.is_some(), h.sound, game, showing);
                (lights, if showing { TICK } else { h.wait })
            }
            None => {
                inner.silence = Silence::default();
                inner.tracker = Tracker::default();
                inner.status = status_of(&s, usb.is_some(), Sound::Quiet, game, false);
                (None, IDLE)
            }
        };
        inner.shown = lights.iter().flatten().map(|&(led, [r, g, b])| LedColor { led, r, g, b }).collect();

        let send: Frame = lights.filter(|_| on_keys).map(|f| (f, s.brightness));
        if send != inner.sent {
            let _ = device.send(Request::SetLayer(
                LightLayer::Music,
                send.clone().map(|(colors, b)| LiveFrame { colors, level: level_factor(b) }),
            ));
            inner.sent = send;
        }
        if inner.watched && now >= inner.next_emit {
            inner.next_emit = now + EMIT_EVERY_MS;
            let _ = self.app.emit(EVENT_MUSIC, Self::state_of(inner));
        }
        if inner.watched {
            wait.min(Duration::from_millis(EMIT_EVERY_MS as u64))
        } else {
            wait
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

    fn model() -> &'static Model {
        kb_devices::catalogue().unwrap().iter().find(|m| m.id.contains("ak980")).unwrap()
    }

    fn keys() -> Keys {
        Keys::of(model())
    }

    fn led(id: &str) -> u8 {
        model().geometry.as_ref().unwrap().keys.iter().find(|k| k.id == id).unwrap().led
    }

    fn color(f: &[(u8, Rgb)], id: &str) -> Rgb {
        f.iter().find(|k| k.0 == led(id)).unwrap().1
    }

    fn moment(bands: &[f32], loud: f32) -> Moment {
        let mut b = bands.to_vec();
        b.resize(COLUMNS, 0.0);
        Moment { peaks: b.clone(), bands: b, loud, ..Moment::default() }
    }

    fn settings(mode: MusicMode) -> MusicSettings {
        MusicSettings { enabled: true, mode, ..MusicSettings::default() }
    }

    fn brightness(c: Rgb) -> u32 {
        c.iter().map(|&v| u32::from(v)).sum()
    }

    #[test]
    fn the_ak980_has_six_rows_and_every_led_once_in_every_mode() {
        let k = keys();
        assert_eq!(k.rows, 6);
        let row = |id: &str| k.keys.iter().find(|x| x.led == led(id)).unwrap().row;
        assert_eq!((row("esc"), row("grave"), row("tab"), row("caps"), row("space")), (5, 4, 3, 2, 0));
        // The arrow keys sit lower than their row but belong to it; a tall
        // key counts in the row it starts in.
        assert_eq!((row("up"), row("left"), row("kp-plus"), row("kp-enter")), (1, 0, 3, 1));
        let all = model().geometry.as_ref().unwrap().keys.len();
        let mut m = moment(&[0.7; COLUMNS], 0.5);
        m.beat = Some(1.0);
        m.strong = Some(1.0);
        m.waves = vec![(1.0, 90.0)];
        for mode in [MusicMode::Bars, MusicMode::Beat, MusicMode::Level, MusicMode::Wave] {
            let f = frame(&k, &m, &settings(mode), 1.1);
            let mut leds: Vec<u8> = f.iter().map(|x| x.0).collect();
            leds.sort_unstable();
            leds.dedup();
            assert_eq!((f.len(), leds.len()), (all, all), "{mode:?}");
        }
    }

    #[test]
    fn bars_fill_their_column_from_the_bottom() {
        let k = keys();
        let s = settings(MusicMode::Bars);
        // Loud bass: the left column, Esc down to Ctrl, all lit.
        let f = frame(&k, &moment(&[1.0, 1.0], 0.5), &s, 0.0);
        for id in ["esc", "grave", "tab", "caps", "lctrl"] {
            assert_ne!(color(&f, id), [0, 0, 0], "{id}");
        }
        // Green at the bottom, red at the top.
        assert_eq!(color(&f, "lctrl"), [0, 255, 0]);
        assert_eq!(color(&f, "esc"), [255, 0, 0]);
        // Nothing else plays: the right side is dark.
        for id in ["kp-plus", "kp0", "delete", "enter", "right"] {
            assert_eq!(color(&f, id), [0, 0, 0], "{id}");
        }
        // Half height: the bottom rows only.
        let f = frame(&k, &moment(&[0.45; COLUMNS], 0.5), &s, 0.0);
        assert_ne!(color(&f, "lctrl"), [0, 0, 0]);
        assert_eq!(color(&f, "esc"), [0, 0, 0]);
        // Dim: the other keys faintly in their colour.
        let dim = MusicSettings { rest: MusicRest::Dim, ..s.clone() };
        let f = frame(&k, &moment(&[], 0.0), &dim, 0.0);
        assert_eq!(color(&f, "esc"), paint([255, 0, 0], DIM));
        assert_ne!(color(&f, "esc"), [0, 0, 0]);
    }

    #[test]
    fn a_peak_mark_shows_above_a_fallen_bar() {
        let k = keys();
        let mut m = moment(&[], 0.0);
        m.peaks = vec![1.0; COLUMNS];
        let f = frame(&k, &m, &settings(MusicMode::Bars), 0.0);
        // The top row shows the mark; the rows under it are dark.
        assert_eq!(color(&f, "f5"), paint(palette(&settings(MusicMode::Bars), 1.0), MARK_LIGHT));
        assert_eq!(color(&f, "t"), [0, 0, 0]);
    }

    #[test]
    fn centre_out_bars_are_symmetric() {
        let m = moment(&[1.0, 0.8, 0.6, 0.4, 0.3, 0.2, 0.1, 0.05], 0.5);
        let cols = columns(&m, true);
        for c in 0..COLUMNS / 2 {
            assert_eq!(cols[c], cols[COLUMNS - 1 - c], "column {c}");
        }
        // Bass in the middle, treble at the ends.
        assert!(cols[COLUMNS / 2].0 > cols[0].0);
        let left = columns(&m, false);
        assert!(left[0].0 > left[COLUMNS - 1].0);
    }

    #[test]
    fn a_beat_flashes_and_dies_away() {
        let k = keys();
        let s = MusicSettings { palette: MusicPalette::One, color: [255, 255, 255], ..settings(MusicMode::Beat) };
        let mut m = moment(&[], 0.0);
        m.beat = Some(10.0);
        let at = |t: f64| brightness(color(&frame(&k, &m, &s, t), "g"));
        let (now, later, after) = (at(10.0), at(10.25), at(11.0));
        assert!(now > later && later > after, "{now} {later} {after}");
        assert!(now >= 3 * 245, "brightest right on the beat: {now}");
        // A second later only the base light is left.
        assert_eq!(after, brightness(paint([255; 3], BEAT_BASE + 0.85 * (-4.0f32).exp())));
        // Louder music breathes brighter between beats.
        m.loud = 1.0;
        assert!(brightness(color(&frame(&k, &m, &s, 11.0), "g")) > after);
    }

    #[test]
    fn a_strong_beat_sends_a_ring_from_the_middle() {
        let k = keys();
        let s = MusicSettings { palette: MusicPalette::One, color: [255, 255, 255], ..settings(MusicMode::Beat) };
        let mut m = moment(&[], 0.0);
        m.strong = Some(0.0);
        // 0.2 s on, the ring is 4.4 keys out (G on the left, the
        // backslash on the right), past the middle (L).
        let f = frame(&k, &m, &s, 0.2);
        let middle = brightness(color(&f, "l"));
        assert!(brightness(color(&f, "g")) > middle && brightness(color(&f, "backslash")) > middle);
        // Without a strong beat there is no ring.
        let f = frame(&k, &moment(&[], 0.0), &s, 0.2);
        assert_eq!(brightness(color(&f, "g")), brightness(color(&f, "l")));
    }

    #[test]
    fn level_opens_from_the_middle() {
        let k = keys();
        let s = settings(MusicMode::Level);
        let lit = |loud: f32| {
            let f = frame(&k, &Moment { loud, ..moment(&[], loud) }, &s, 0.0);
            f.iter().filter(|x| x.1 != [0, 0, 0]).count() as f32 / f.len() as f32
        };
        // Silent, only the very middle glows (the silence rule takes the
        // lights away soon).
        assert!(lit(0.0) < 0.05, "{}", lit(0.0));
        assert!((0.35..0.65).contains(&lit(0.5)), "{}", lit(0.5));
        assert!(lit(1.0) > 0.95);
        // Green in the middle, red at the ends.
        let f = frame(&k, &moment(&[], 1.0), &s, 0.0);
        let (mid, end) = (color(&f, "b"), color(&f, "esc"));
        assert!(mid[1] > mid[0] && end[0] > end[1], "{mid:?} {end:?}");
        // The marker stays out where the loudness was: at Esc, 9.75 keys
        // from the middle; the keys between are dark.
        let marked = Moment { loud: 0.2, loud_peak: 0.9, ..moment(&[], 0.2) };
        let f = frame(&k, &marked, &s, 0.0);
        assert_eq!(color(&f, "esc"), paint(palette(&s, 9.75 / 10.25), LEVEL_MARK_LIGHT));
        assert_eq!(color(&f, "1"), [0, 0, 0]);
    }

    #[test]
    fn a_wave_crosses_from_the_left_or_from_the_middle() {
        let k = keys();
        let s = settings(MusicMode::Wave);
        let mut m = moment(&[], 0.5);
        m.waves = vec![(0.0, 0.0)];
        // A quarter second on, the front is six keys in.
        let f = frame(&k, &m, &s, 0.25);
        let (front, far) = (brightness(color(&f, "5")), brightness(color(&f, "kp9")));
        assert!(front > 0 && far == 0, "{front} {far}");
        let centred = MusicSettings { center: true, ..s };
        let f = frame(&k, &m, &centred, 0.1);
        assert!(brightness(color(&f, "g")) > brightness(color(&f, "esc")));
        // Without waves only the rest shows.
        assert!(frame(&k, &moment(&[], 0.5), &centred, 1.0).iter().all(|x| x.1 == [0, 0, 0]));
    }

    #[test]
    fn palettes_give_their_colours() {
        let s = |palette| MusicSettings {
            palette,
            color: [10, 20, 30],
            color2: [210, 220, 230],
            ..MusicSettings::default()
        };
        assert_eq!(palette(&s(MusicPalette::Heat), 0.0), [0, 255, 0]);
        assert_eq!(palette(&s(MusicPalette::Heat), 0.5), [255, 255, 0]);
        assert_eq!(palette(&s(MusicPalette::Heat), 1.0), [255, 0, 0]);
        assert_eq!(palette(&s(MusicPalette::Rainbow), 0.0), [255, 0, 0]);
        assert_eq!(palette(&s(MusicPalette::One), 0.7), [10, 20, 30]);
        assert_eq!(palette(&s(MusicPalette::Two), 0.5), [110, 120, 130]);
        assert_eq!(paint([200, 100, 0], 1.0), [200, 100, 0]);
        assert_eq!(paint([200, 100, 0], 0.0), [0, 0, 0]);
    }

    #[test]
    fn quiet_gives_the_saved_light_or_dim_or_dark_keys() {
        let k = keys();
        let s = MusicSettings::default();
        assert_eq!(quiet_frame(&k, &s), None);
        let dim = quiet_frame(&k, &MusicSettings { silence: MusicSilence::Dim, ..s.clone() }).unwrap();
        assert_eq!(dim.len(), k.keys.len());
        assert!(dim.iter().all(|x| x.1 != [0, 0, 0] && brightness(x.1) < 3 * 80));
        let dark = quiet_frame(&k, &MusicSettings { silence: MusicSilence::Dark, ..s }).unwrap();
        assert!(dark.iter().all(|x| x.1 == [0, 0, 0]));
    }

    #[test]
    fn the_lights_start_on_sound_and_step_aside_after_three_quiet_seconds() {
        let mut q = Silence::default();
        assert!(!q.step(Some(-30.0), 0.0), "one loud frame is not enough");
        assert!(q.step(Some(-30.0), 50.0));
        // A quiet passage between the two thresholds keeps them.
        assert!(q.step(Some(-55.0), 100.0));
        assert!(q.step(Some(-55.0), 10_000.0));
        // Silence: still showing for three seconds, then gone.
        assert!(q.step(Some(-70.0), 10_050.0));
        assert!(q.step(None, 13_000.0));
        assert!(!q.step(Some(-80.0), 13_050.0));
        // Between the thresholds does not start them again…
        assert!(!q.step(Some(-55.0), 13_100.0));
        assert!(!q.step(Some(-55.0), 13_150.0));
        // …two loud frames do.
        assert!(!q.step(Some(-40.0), 13_200.0));
        assert!(q.step(Some(-40.0), 13_250.0));
        // A muted Windows (`None`) is silence.
        let mut q = Silence::default();
        assert!(!q.step(None, 0.0) && !q.step(None, 50.0));
    }

    #[test]
    fn beats_are_counted_strong_ones_and_waves_kept() {
        let mut t = Tracker::default();
        let levels = |beat: Option<f64>, loud: f32| Levels {
            bands: vec![0.0; COLUMNS],
            peaks: vec![0.0; COLUMNS],
            loud,
            beat,
            db: -20.0,
        };
        let m = t.step(&levels(None, 0.0), 0.0);
        assert_eq!((m.beats, m.beat, m.strong), (0, None, None));
        // The first beat is strong; the same beat seen again counts once.
        let m = t.step(&levels(Some(0.5), 0.0), 0.5);
        assert_eq!((m.beats, m.strong), (1, Some(0.5)));
        let m = t.step(&levels(Some(0.5), 0.0), 0.55);
        assert_eq!(m.beats, 1);
        for (i, at) in [1.0, 1.5, 2.0, 2.5].into_iter().enumerate() {
            let m = t.step(&levels(Some(at), 0.0), at);
            assert_eq!(m.beats, i as u32 + 2);
            // Every fourth beat is strong: the fifth here.
            assert_eq!(m.strong, Some(if at == 2.5 { 2.5 } else { 0.5 }), "{at}");
        }
        t.step(&levels(Some(3.0), 0.0), 3.0);
        let m = t.step(&levels(Some(3.5), 0.0), 3.5);
        assert_eq!(m.beats, 7);
        assert_eq!(m.waves.len(), MAX_WAVES, "the oldest wave went");
        assert_eq!(m.waves[0].0, 1.0);
        // After a pause the first beat is strong again.
        let m = t.step(&levels(Some(6.0), 0.0), 6.0);
        assert_eq!(m.strong, Some(6.0));
        assert!(m.waves.iter().all(|w| 6.0 - w.0 <= WAVE_GONE));
    }

    #[test]
    fn the_level_marker_holds_then_falls() {
        let mut t = Tracker::default();
        let levels = |loud: f32| Levels { bands: vec![], peaks: vec![], loud, beat: None, db: -20.0 };
        assert_eq!(t.step(&levels(0.9), 0.0).loud_peak, 0.9);
        assert_eq!(t.step(&levels(0.2), 0.3).loud_peak, 0.9, "held");
        let falling = t.step(&levels(0.2), 0.8).loud_peak;
        assert!(falling < 0.9 && falling > 0.2, "{falling}");
        let mut last = falling;
        for i in 1..=60 {
            last = t.step(&levels(0.2), 0.8 + f64::from(i) * 0.05).loud_peak;
        }
        assert_eq!(last, 0.2, "never under the loudness");
    }

    #[test]
    fn the_status_says_what_holds_the_lights_back() {
        let on = MusicSettings { enabled: true, ..MusicSettings::default() };
        let off = MusicSettings::default();
        assert_eq!(status_of(&off, true, Sound::Open, false, true), MusicStatus::Off);
        assert_eq!(status_of(&on, false, Sound::Open, false, true), MusicStatus::NoUsb);
        assert_eq!(status_of(&on, true, Sound::Open, true, true), MusicStatus::Game);
        assert_eq!(status_of(&on, true, Sound::Unavailable, false, false), MusicStatus::Unavailable);
        assert_eq!(status_of(&on, true, Sound::NoOutput, false, false), MusicStatus::NoOutput);
        assert_eq!(status_of(&on, true, Sound::Quiet, false, false), MusicStatus::Waiting);
        assert_eq!(status_of(&on, true, Sound::Open, false, true), MusicStatus::Playing);
    }

    #[test]
    fn settings_are_kept_in_range() {
        let s = MusicSettings { sensitivity: 400, brightness: 0, ..MusicSettings::default() }.sanitized();
        assert_eq!((s.sensitivity, s.brightness), (200, 1));
        let s = MusicSettings { sensitivity: 10, ..MusicSettings::default() }.sanitized();
        assert_eq!(s.sensitivity, 50);
        let d = MusicSettings::default();
        assert_eq!((d.mode, d.palette, d.center, d.sensitivity), (MusicMode::Bars, MusicPalette::Heat, false, 100));
        assert_eq!((d.rest, d.silence, d.pause_in_games), (MusicRest::Dark, MusicSilence::Saved, true));
        // An old or partial file fills in the rest.
        let partial: MusicSettings = serde_json::from_str(r#"{"enabled":true,"mode":"wave"}"#).unwrap();
        assert_eq!(partial, MusicSettings { enabled: true, mode: MusicMode::Wave, ..MusicSettings::default() });
    }
}
