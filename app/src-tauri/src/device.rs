//! Device actor: the only code that talks to keyboards.
//!
//! One thread owns the HID subsystem and every open device. Requests from
//! the UI arrive on a channel and run one at a time, so protocol sessions
//! (START … COMMIT) can never interleave. Between requests the actor
//!
//! * re-enumerates devices and emits `keyboards-changed`,
//! * streams the live (RAM-only) LED frame while one is set: a preview the
//!   UI shows goes on top of a background layer (the Pomodoro progress),
//! * mirrors the colours currently shown (`04 F5`) while the UI asks for it,
//! * sets the keyboard clock when it connects over USB (at most once a day).
//!
//! A screen upload (up to ~5 minutes) runs inside one request, so nothing
//! else talks to the keyboard meanwhile; progress goes out as events.
//!
//! It also keeps the app's record of each model's keymap (`keymap.json`)
//! and macros (`macros.json`): the keyboard cannot report either, so a
//! record is updated only after a write the keyboard confirmed.

use crate::dto::{Battery, KeyboardInfo, LedColor};
use crate::settings::{
    self, ClockSettings, Keymaps, MacroRecord, StoredKeymap, StoredLayer, StoredMacro, StoredMacros,
};
use kb_core::color::LedCalibration;
use kb_core::detect::BlePeer;
use kb_core::readback;
use kb_core::sonix::SonixFf13;
use kb_devices::{ConnectionKind, Model};
use kb_protocol::sonix::keymap::{KeyAction, Layer};
use kb_protocol::sonix::screen::ScreenAnimation;
use kb_protocol::sonix::{scale_to_level, EffectSettings, Rgb, MAX_LEVEL};
use kb_transport::hid::{Hid, HidChannel};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

pub const EVENT_KEYBOARDS_CHANGED: &str = "keyboards-changed";
pub const EVENT_MIRROR: &str = "lighting-mirror";
pub const EVENT_CLOCK: &str = "clock-changed";
pub const EVENT_SCREEN_PROGRESS: &str = "screen-upload-progress";
/// Longest wait for a screen upload (140 frames take ~5.4 min).
pub const SCREEN_UPLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Progress events at most this often.
const SCREEN_PROGRESS_EVERY: Duration = Duration::from_millis(150);

const ENUMERATE_EVERY: Duration = Duration::from_secs(2);
const BLE_RESCAN: Duration = Duration::from_secs(10);
const BATTERY_REFRESH: Duration = Duration::from_secs(60);
/// Live frames are re-sent continuously; the keyboard reverts to its saved
/// effect when the stream stops (verified 2026-11-10).
const LIVE_EVERY: Duration = Duration::from_millis(50);
/// Warn when this many live frames in a row failed (one second's worth).
const LIVE_FAILURES_WARN: u32 = 20;
/// `04 F5` polling rate for the on-screen mirror.
const MIRROR_EVERY: Duration = Duration::from_millis(250);
/// Wait after the keyboard appears before talking to it: all its interfaces
/// need to be ready (S2, docs/research/protokol-ff13.md §10.4).
const CLOCK_SETTLE: Duration = Duration::from_secs(2);
/// Automatic clock sync runs at most this often. Every clock packet also
/// switches the screen to its clock page, so it should not run on every
/// reconnect.
const CLOCK_SYNC_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const CLOCK_RETRY: Duration = Duration::from_secs(5);
const CLOCK_ATTEMPTS: u8 = 3;

pub type Reply<T> = Sender<Result<T, String>>;

/// Now as Unix time in milliseconds.
fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub enum Request {
    Refresh,
    ApplyEffect(EffectSettings, Reply<()>),
    ApplyPerKey {
        colors: Vec<(u8, Rgb)>,
        brightness: u8,
        reply: Reply<()>,
    },
    /// `Some` starts/updates the live frame, `None` stops streaming.
    SetLive(Option<LiveFrame>),
    /// Background live layer (Pomodoro progress): streamed while no UI
    /// preview (`SetLive`) is set. `None` stops it.
    SetAmbient(Option<LiveFrame>),
    /// Start (`Some`, with what the keys should show) or stop the mirror.
    SetMirror(Option<MirrorExpect>),
    /// New colour calibration; re-sends the live frame at once so sliders
    /// give immediate feedback on the keys.
    SetCalibration(LedCalibration),
    /// Set the keyboard clock to the PC's local time now.
    SyncClock(Reply<ClockSettings>),
    /// Turn automatic clock sync on or off (persisted).
    SetClockAuto(bool, Reply<ClockSettings>),
    /// Write one keymap layer of the USB keyboard (flash) and record it.
    /// Replies with the model's updated record.
    ApplyKeymap {
        layer: Layer,
        entries: Vec<(u8, KeyAction)>,
        reply: Reply<StoredKeymap>,
    },
    /// Write every macro of the USB keyboard (flash) and record them with
    /// their names. Replies with the model's updated record.
    ApplyMacros {
        slots: Vec<Option<StoredMacro>>,
        reply: Reply<StoredMacros>,
    },
    /// Upload an animation to the USB keyboard's GIF page (flash write).
    /// `id` only labels the progress events.
    UploadScreen {
        id: String,
        anim: Box<ScreenAnimation>,
        reply: mpsc::Sender<ScreenUploadResult>,
    },
}

/// How a screen upload ended.
#[derive(Debug, Clone)]
pub struct ScreenUploadResult {
    /// Model of the keyboard written to.
    pub model_id: Option<String>,
    /// Whether the keyboard's flash was touched (the session was opened),
    /// even if the upload then failed: it counts towards wear.
    pub flash_written: bool,
    pub error: Option<String>,
}

/// A RAM-only frame in UI colours (sRGB). Calibration and the linear
/// brightness `level` (0–1) are applied when the frame is sent, so a
/// calibration change shows up on the next frame.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveFrame {
    pub colors: Vec<(u8, Rgb)>,
    pub level: f32,
}

/// What the keyboard should be showing, in UI colours (sRGB). The mirror
/// uses it to pick the right red value from each read-back.
#[derive(Debug, Clone, Default)]
pub struct MirrorExpect {
    /// Brightness level (0–5) the keyboard runs at.
    pub level: u8,
    pub colors: Expected,
}

#[derive(Debug, Clone, Default)]
pub enum Expected {
    /// Rainbow effects, or a state the app did not set.
    #[default]
    Unknown,
    /// One colour on every key (single-colour firmware effects).
    Uniform(Rgb),
    /// A per-key design; keys not listed are off.
    PerKey(HashMap<u8, Rgb>),
}

#[derive(Default)]
pub struct Snapshot {
    pub keyboards: Vec<KeyboardInfo>,
    pub calibration: LedCalibration,
    pub clock: ClockSettings,
    /// What the app last wrote to each model's keymap.
    pub keymaps: Keymaps,
    /// What the app last wrote to each model's macro memory.
    pub macros: MacroRecord,
}

pub struct DeviceHandle {
    pub snapshot: Arc<Mutex<Snapshot>>,
    tx: Sender<Request>,
}

impl DeviceHandle {
    pub fn send(&self, r: Request) -> Result<(), String> {
        self.tx.send(r).map_err(|_| "device actor stopped".to_string())
    }

    /// Send a request carrying a reply channel and wait for the answer.
    pub fn call<T>(&self, make: impl FnOnce(Reply<T>) -> Request) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        self.send(make(tx))?;
        rx.recv_timeout(Duration::from_secs(20)).map_err(|_| "keyboard did not answer in time".to_string())?
    }

    /// Upload to the screen and wait (long) for the outcome.
    pub fn upload_screen(&self, id: String, anim: ScreenAnimation) -> ScreenUploadResult {
        let (tx, rx) = mpsc::channel();
        if let Err(e) = self.send(Request::UploadScreen { id, anim: Box::new(anim), reply: tx }) {
            return ScreenUploadResult { model_id: None, flash_written: false, error: Some(e) };
        }
        rx.recv_timeout(SCREEN_UPLOAD_TIMEOUT).unwrap_or_else(|_| ScreenUploadResult {
            model_id: None,
            flash_written: true,
            error: Some("keyboard did not finish the upload in time".into()),
        })
    }
}

/// Start the actor. `config_dir` is where clock sync history and the keymap
/// record are saved; without it they last only for this run.
pub fn spawn(app: AppHandle, calibration: LedCalibration, config_dir: Option<PathBuf>) -> DeviceHandle {
    let clock = config_dir.as_deref().map(settings::load_clock).unwrap_or_default();
    let keymaps = config_dir.as_deref().map(settings::load_keymaps).unwrap_or_default();
    let macros = config_dir.as_deref().map(settings::load_macros).unwrap_or_default();
    let snapshot = Arc::new(Mutex::new(Snapshot {
        keyboards: Vec::new(),
        calibration,
        clock,
        keymaps: keymaps.clone(),
        macros: macros.clone(),
    }));
    let (tx, rx) = mpsc::channel();
    let shared = snapshot.clone();
    std::thread::Builder::new()
        .name("device-actor".into())
        .spawn(move || Actor::new(app, shared, calibration, clock, keymaps, macros, config_dir).run(rx))
        .expect("spawn device actor");
    DeviceHandle { snapshot, tx }
}

struct Command {
    path: String,
    driver: SonixFf13<HidChannel>,
}

struct Actor {
    app: AppHandle,
    shared: Arc<Mutex<Snapshot>>,
    hid: Option<Hid>,
    peers: Vec<BlePeer>,
    last_ble_scan: Option<Instant>,
    batteries: HashMap<u64, (Battery, Instant)>,
    /// Path of the wired command interface of the active keyboard, if any.
    command_path: Option<String>,
    /// Path of its display channel (screen pages), if any.
    display_path: Option<String>,
    /// Model of the keyboard behind `command_path`.
    usb_model: Option<&'static Model>,
    command: Option<Command>,
    live: Option<LiveFrame>,
    /// Background layer, shown while `live` is not set.
    ambient: Option<LiveFrame>,
    /// Live frames that failed in a row.
    live_failures: u32,
    /// `Some` while the UI shows the mirror.
    mirror: Option<MirrorExpect>,
    /// Firmware status lights of the USB keyboard: (LED, colour when lit).
    indicators: Vec<(u8, Rgb)>,
    /// UI colours are monitor sRGB; LEDs are linear. Applied to every
    /// host-provided colour (live frames, per-key tables, effect colours)
    /// and inverted for the mirror. The firmware drives all three linearly:
    /// a Static effect in #8B5CF6 reads back 137/91/244 at level 5, the
    /// value-for-value scale a per-key table gets (measured 2026-11-10).
    calibration: LedCalibration,
    clock: ClockSettings,
    keymaps: Keymaps,
    macros: MacroRecord,
    config_dir: Option<PathBuf>,
    /// When the next automatic clock sync is due, and attempts left.
    clock_due: Option<(Instant, u8)>,
    next_enumerate: Instant,
    next_live: Instant,
    next_mirror: Instant,
}

impl Actor {
    fn new(
        app: AppHandle,
        shared: Arc<Mutex<Snapshot>>,
        calibration: LedCalibration,
        clock: ClockSettings,
        keymaps: Keymaps,
        macros: MacroRecord,
        config_dir: Option<PathBuf>,
    ) -> Self {
        let hid = Hid::new().map_err(|e| tracing::error!("HID init failed: {e}")).ok();
        let now = Instant::now();
        Self {
            app,
            shared,
            hid,
            peers: Vec::new(),
            last_ble_scan: None,
            batteries: HashMap::new(),
            command_path: None,
            display_path: None,
            usb_model: None,
            command: None,
            live: None,
            ambient: None,
            live_failures: 0,
            mirror: None,
            indicators: Vec::new(),
            calibration,
            clock,
            keymaps,
            macros,
            config_dir,
            clock_due: None,
            next_enumerate: now,
            next_live: now,
            next_mirror: now,
        }
    }

    fn run(mut self, rx: mpsc::Receiver<Request>) {
        loop {
            let now = Instant::now();
            if now >= self.next_enumerate {
                self.enumerate();
                self.next_enumerate = Instant::now() + ENUMERATE_EVERY;
            }
            if self.streaming() && now >= self.next_live {
                self.send_live();
                self.next_live = Instant::now() + LIVE_EVERY;
            } else if self.mirror.is_some() && !self.streaming() && now >= self.next_mirror {
                self.send_mirror();
                self.next_mirror = Instant::now() + MIRROR_EVERY;
            }
            if let Some((due, attempts)) = self.clock_due {
                if now >= due {
                    self.clock_due = None;
                    if let Err(e) = self.sync_clock() {
                        tracing::warn!(attempts_left = attempts - 1, "automatic clock sync failed: {e}");
                        if attempts > 1 && self.command_path.is_some() {
                            self.clock_due = Some((Instant::now() + CLOCK_RETRY, attempts - 1));
                        }
                    }
                }
            }

            let mut wake = self.next_enumerate;
            if self.streaming() {
                wake = wake.min(self.next_live);
            } else if self.mirror.is_some() {
                wake = wake.min(self.next_mirror);
            }
            if let Some((due, _)) = self.clock_due {
                wake = wake.min(due);
            }
            let timeout = wake.saturating_duration_since(Instant::now());
            match rx.recv_timeout(timeout) {
                Ok(req) => self.handle(req),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn handle(&mut self, req: Request) {
        match req {
            Request::Refresh => {
                self.last_ble_scan = None;
                self.batteries.clear();
                self.next_enumerate = Instant::now();
            }
            Request::ApplyEffect(mut settings, reply) => {
                settings.color = self.calibration.apply(settings.color);
                let r = self.with_command(|d| d.apply_effect(&settings));
                let _ = reply.send(r);
            }
            Request::ApplyPerKey { colors, brightness, reply } => {
                let cal = self.calibration;
                let colors: Vec<(u8, Rgb)> = colors.into_iter().map(|(l, c)| (l, cal.apply(c))).collect();
                let r = self.with_command(|d| d.apply_per_key(colors, brightness));
                let _ = reply.send(r);
            }
            Request::SetLive(frame) => {
                if let Some(f) = &frame {
                    let lit: Vec<String> = f
                        .colors
                        .iter()
                        .filter(|(_, c)| *c != [0, 0, 0])
                        .map(|(l, [r, g, b])| format!("{l}=#{r:02X}{g:02X}{b:02X}"))
                        .collect();
                    tracing::trace!(lit = lit.len(), sample = ?&lit[..lit.len().min(6)], level = f.level, "live frame");
                }
                match (&frame, &self.live) {
                    (None, Some(_)) if self.ambient.is_some() => {
                        tracing::debug!("live preview stopped; the background layer shows again")
                    }
                    (None, Some(_)) => {
                        tracing::debug!("live stream stopped; keyboard reverts to its saved effect")
                    }
                    (Some(f), None) => tracing::debug!(leds = f.colors.len(), level = f.level, "live stream started"),
                    _ => {}
                }
                self.live = frame;
                self.next_live = Instant::now();
            }
            Request::SetAmbient(frame) => {
                match (&frame, &self.ambient) {
                    (None, Some(_)) => tracing::debug!("background light layer stopped"),
                    (Some(f), None) => tracing::debug!(leds = f.colors.len(), "background light layer started"),
                    _ => {}
                }
                self.ambient = frame;
                if self.live.is_none() {
                    self.next_live = Instant::now();
                }
            }
            Request::SetMirror(expect) => {
                self.mirror = expect;
                self.next_mirror = Instant::now();
            }
            Request::SetCalibration(cal) => {
                tracing::debug!(?cal, "calibration changed");
                self.calibration = cal.sanitized();
                self.next_live = Instant::now();
            }
            Request::SyncClock(reply) => {
                let r = self.sync_clock().map(|_| self.clock);
                let _ = reply.send(r);
            }
            Request::SetClockAuto(on, reply) => {
                self.clock.auto_sync = on;
                if !on {
                    self.clock_due = None;
                }
                let _ = reply.send(self.store_clock().map(|_| self.clock));
            }
            Request::ApplyKeymap { layer, entries, reply } => {
                let _ = reply.send(self.apply_keymap(layer, entries));
            }
            Request::ApplyMacros { slots, reply } => {
                let _ = reply.send(self.apply_macros(slots));
            }
            Request::UploadScreen { id, anim, reply } => {
                let _ = reply.send(self.upload_screen(&id, &anim));
            }
        }
    }

    /// Upload to the GIF page: command channel for the session, display
    /// channel (opened just for this) for the pages. Progress is emitted as
    /// `screen-upload-progress`.
    fn upload_screen(&mut self, id: &str, anim: &ScreenAnimation) -> ScreenUploadResult {
        let model_id = self.usb_model.map(|m| m.id.clone());
        let fail = |model_id: Option<String>, flash_written: bool, e: String| ScreenUploadResult {
            model_id,
            flash_written,
            error: Some(e),
        };
        const NO_USB: &str = "Klavye USB modunda değil. Kabloyu takıp anahtarı ortaya (USB) alın.";
        let Some(path) = self.display_path.clone() else { return fail(model_id, false, NO_USB.into()) };
        let display = self.hid.as_ref().ok_or_else(|| "HID unavailable".to_string()).and_then(|hid| {
            let iface = hid.interfaces().into_iter().find(|i| i.path == path).ok_or("display channel disappeared")?;
            hid.open(&iface).map_err(|e| e.to_string())
        });
        let mut display = match display {
            Ok(d) => d,
            Err(e) => return fail(model_id, false, e),
        };
        let app = self.app.clone();
        let label = id.to_string();
        let mut last = Instant::now() - SCREEN_PROGRESS_EVERY;
        let mut started = false;
        let started_at = Instant::now();
        let r = self.with_command(|d| {
            d.upload_screen(&mut display, anim, |done, total| {
                started = true;
                if done == 1 || done == total || last.elapsed() >= SCREEN_PROGRESS_EVERY {
                    last = Instant::now();
                    let p = crate::dto::ScreenProgress { id: label.clone(), done: done as u32, total: total as u32 };
                    let _ = app.emit(EVENT_SCREEN_PROGRESS, p);
                }
            })
        });
        match r {
            Ok(report) => {
                tracing::info!(
                    pages = report.pages,
                    secs = started_at.elapsed().as_secs(),
                    commit = ?report.commit_echo,
                    "screen upload done"
                );
                ScreenUploadResult { model_id, flash_written: true, error: None }
            }
            // A failure after the first page left the GIF page half written.
            Err(e) => fail(model_id, started, e),
        }
    }

    /// Write every macro, then record them (names included). Like the
    /// keymap, the record changes only after the keyboard confirmed the data.
    fn apply_macros(&mut self, mut slots: Vec<Option<StoredMacro>>) -> Result<StoredMacros, String> {
        let model = self
            .usb_model
            .ok_or_else(|| "Klavye USB modunda değil. Kabloyu takıp anahtarı ortaya (USB) alın.".to_string())?;
        while slots.last().is_some_and(Option::is_none) {
            slots.pop();
        }
        let mut stored = StoredMacros { slots, written_ms: None };
        let image = stored.image_slots();
        self.with_command(|d| d.write_macros(&image))?;
        tracing::info!(slots = image.len(), model = %model.id, "macros written");
        stored.written_ms = Some(now_ms());
        self.macros.models.insert(model.id.clone(), stored.clone());
        if let Ok(mut s) = self.shared.lock() {
            s.macros = self.macros.clone();
        }
        if let Some(dir) = &self.config_dir {
            if let Err(e) = settings::save_macros(dir, &self.macros) {
                // The keyboard has the macros; only the record on disk is stale.
                tracing::warn!("macro record not saved: {e}");
            }
        }
        Ok(stored)
    }

    /// Write a keymap layer, then record it. The record changes only after
    /// the keyboard confirmed the data (checksum), so it never claims a
    /// layout the keyboard did not get.
    fn apply_keymap(&mut self, layer: Layer, entries: Vec<(u8, KeyAction)>) -> Result<StoredKeymap, String> {
        let model = self
            .usb_model
            .ok_or_else(|| "Klavye USB modunda değil. Kabloyu takıp anahtarı ortaya (USB) alın.".to_string())?;
        let map = kb_core::keymap::canonical(model, layer, entries).map_err(|e| e.to_string())?;
        self.with_command(|d| d.apply_keymap(layer, map.iter().map(|(&l, &a)| (l, a))))?;
        tracing::info!(?layer, keys = map.len(), model = %model.id, "keymap layer written");
        let stored = self.keymaps.models.entry(model.id.clone()).or_default();
        *stored.layer_mut(layer) = StoredLayer::new(&map, now_ms());
        let result = stored.clone();
        if let Ok(mut s) = self.shared.lock() {
            s.keymaps = self.keymaps.clone();
        }
        if let Some(dir) = &self.config_dir {
            if let Err(e) = settings::save_keymaps(dir, &self.keymaps) {
                // The keyboard has the layer; only the record on disk is stale.
                tracing::warn!("keymap record not saved: {e}");
            }
        }
        Ok(result)
    }

    /// Set the keyboard clock to the PC's local time, then record it.
    fn sync_clock(&mut self) -> Result<(), String> {
        let time = kb_core::clock::now();
        self.with_command(|d| d.set_clock(&time))?;
        tracing::info!(?time, "keyboard clock set");
        self.clock.last_sync_ms = Some(now_ms());
        if let Err(e) = self.store_clock() {
            // The clock is set; only the history is lost.
            tracing::warn!("{e}");
        }
        Ok(())
    }

    /// Persist the clock settings and tell the UI.
    fn store_clock(&mut self) -> Result<(), String> {
        if let Ok(mut s) = self.shared.lock() {
            s.clock = self.clock;
        }
        let _ = self.app.emit(EVENT_CLOCK, crate::dto::ClockStatus::from(self.clock));
        match &self.config_dir {
            Some(dir) => settings::save_clock(dir, self.clock).map_err(|e| format!("clock settings not saved: {e}")),
            None => Ok(()),
        }
    }

    /// Whether an automatic sync is due when the keyboard connects.
    fn clock_sync_due(&self) -> bool {
        if !self.clock.auto_sync {
            return false;
        }
        let Some(last) = self.clock.last_sync_ms else { return true };
        let last = UNIX_EPOCH + Duration::from_millis(last);
        // A clock set in the future (wrong PC time earlier) also counts as due.
        SystemTime::now().duration_since(last).map_or(true, |age| age >= CLOCK_SYNC_EVERY)
    }

    /// Run `f` against the wired command channel, opening it on demand.
    /// Any transport error drops the handle so the next call reopens it.
    fn with_command<T>(
        &mut self,
        f: impl FnOnce(&mut SonixFf13<HidChannel>) -> kb_core::sonix::Result<T>,
    ) -> Result<T, String> {
        let path = self
            .command_path
            .clone()
            .ok_or_else(|| "Klavye USB modunda değil. Kabloyu takıp anahtarı ortaya (USB) alın.".to_string())?;
        if self.command.as_ref().is_none_or(|c| c.path != path) {
            let hid = self.hid.as_ref().ok_or("HID unavailable")?;
            let iface = hid.interfaces().into_iter().find(|i| i.path == path).ok_or("command interface disappeared")?;
            let channel = hid.open(&iface).map_err(|e| e.to_string())?;
            self.command = Some(Command { path, driver: SonixFf13::new(channel) });
        }
        let cmd = self.command.as_mut().expect("opened above");
        match f(&mut cmd.driver) {
            Ok(v) => Ok(v),
            Err(e) => {
                // The keyboard now and then refuses one real-time frame
                // (`FF`, about 1 in 100); the next one goes 50 ms later.
                // `send_live` warns when they keep failing.
                if matches!(e, kb_core::sonix::DriverError::Refused { op: 0x20, .. }) {
                    tracing::debug!("command failed: {e}");
                } else {
                    tracing::warn!("command failed: {e}");
                }
                if matches!(e, kb_core::sonix::DriverError::Transport(_)) {
                    self.command = None;
                }
                Err(e.to_string())
            }
        }
    }

    /// A live frame is being streamed (preview or background layer).
    fn streaming(&self) -> bool {
        self.live.is_some() || self.ambient.is_some()
    }

    fn send_live(&mut self) {
        let Some(live) = self.live.as_ref().or(self.ambient.as_ref()) else { return };
        if self.command_path.is_none() {
            return;
        }
        let cal = self.calibration;
        let frame: Vec<(u8, Rgb)> = live.colors.iter().map(|&(l, c)| (l, cal.apply_dimmed(c, live.level))).collect();
        match self.with_command(|d| d.realtime_frame(frame)) {
            Ok(()) => self.live_failures = 0,
            Err(e) => {
                self.live_failures += 1;
                if self.live_failures == LIVE_FAILURES_WARN {
                    tracing::warn!(in_a_row = self.live_failures, "live frames keep failing: {e}");
                } else {
                    tracing::debug!("live frame skipped: {e}");
                }
            }
        }
    }

    fn send_mirror(&mut self) {
        if self.command_path.is_none() {
            return;
        }
        match self.with_command(|d| d.read_colors()) {
            Ok(raw) => {
                let cal = self.calibration;
                let expect = self.mirror.clone().unwrap_or_default();
                let level = expect.level.min(MAX_LEVEL);
                // The LED value a UI colour is driven with at this level.
                let driven = |c: Rgb| cal.apply(c).map(|v| scale_to_level(v, level));
                let expected = |led: u8| {
                    if let Some((_, c)) = self.indicators.iter().find(|(l, _)| *l == led) {
                        return Some(*c);
                    }
                    match &expect.colors {
                        Expected::Unknown => None,
                        Expected::Uniform(c) => Some(driven(*c)),
                        Expected::PerKey(keys) => Some(keys.get(&led).map_or([0, 0, 0], |c| driven(*c))),
                    }
                };
                let peak = scale_to_level(255, if level == 0 { MAX_LEVEL } else { level });
                let colors = readback::decode_frame(&raw, expected, peak);
                // Show what the keys look like, in monitor colours.
                let payload: Vec<LedColor> = colors
                    .into_iter()
                    .map(|(led, c)| {
                        let [r, g, b] = cal.inverse(c);
                        LedColor { led, r, g, b }
                    })
                    .collect();
                let _ = self.app.emit(EVENT_MIRROR, payload);
            }
            Err(e) => tracing::debug!("mirror read skipped: {e}"),
        }
    }

    fn enumerate(&mut self) {
        // BLE enumeration is slow; skip it while streaming so frames stay smooth.
        let busy = self.streaming() || self.mirror.is_some();
        if !busy && self.last_ble_scan.is_none_or(|t| t.elapsed() >= BLE_RESCAN) {
            self.peers = scan_ble();
            self.last_ble_scan = Some(Instant::now());
        }

        let interfaces = match self.hid.as_mut() {
            Some(h) => {
                if let Err(e) = h.refresh() {
                    tracing::warn!("HID refresh failed: {e}");
                }
                h.interfaces()
            }
            None => Vec::new(),
        };
        let detected = kb_core::detect(&interfaces, &self.peers);

        let usb = detected.iter().find(|k| k.connection == ConnectionKind::Usb);
        let was_connected = self.command_path.is_some();
        self.command_path = usb.and_then(|k| k.command.as_ref()).map(|i| i.path.clone());
        self.display_path = usb.and_then(|k| k.display.as_ref()).map(|i| i.path.clone());
        let clock_capable = usb.is_some_and(|k| k.capabilities().contains(&kb_devices::Capability::Clock));
        if !was_connected && self.command_path.is_some() && clock_capable && self.clock_sync_due() {
            tracing::debug!("keyboard connected; clock sync due");
            self.clock_due = Some((Instant::now() + CLOCK_SETTLE, CLOCK_ATTEMPTS));
        } else if self.command_path.is_none() {
            self.clock_due = None;
        }
        self.usb_model = usb.filter(|k| k.command.is_some()).map(|k| k.model);
        self.indicators =
            usb.map(|k| k.model.indicators.iter().map(|i| (i.led, i.color)).collect()).unwrap_or_default();
        if self.command_path.is_none() {
            self.command = None;
        }

        let mut infos = Vec::with_capacity(detected.len());
        for k in &detected {
            let battery = match (&k.ble, k.connection) {
                (Some(peer), ConnectionKind::Bluetooth) if peer.connected && !busy => {
                    let stale = self.batteries.get(&peer.address).is_none_or(|(_, at)| at.elapsed() >= BATTERY_REFRESH);
                    if stale {
                        if let Some(b) = read_battery(peer.address) {
                            self.batteries.insert(peer.address, (b, Instant::now()));
                        }
                    }
                    self.batteries.get(&peer.address).map(|(b, _)| b.clone())
                }
                (Some(peer), _) => self.batteries.get(&peer.address).map(|(b, _)| b.clone()),
                _ => None,
            };
            infos.push(KeyboardInfo::from_detected(k, battery));
        }

        let changed = {
            let mut snap = self.shared.lock().expect("snapshot lock");
            if snap.keyboards != infos {
                snap.keyboards = infos.clone();
                true
            } else {
                false
            }
        };
        if changed {
            tracing::info!(count = infos.len(), "keyboards changed");
            if let Err(e) = self.app.emit(EVENT_KEYBOARDS_CHANGED, &infos) {
                tracing::warn!("emit failed: {e}");
            }
        }
    }
}

#[cfg(windows)]
fn scan_ble() -> Vec<BlePeer> {
    match kb_transport::ble::paired_devices() {
        Ok(list) => list
            .into_iter()
            .map(|d| BlePeer {
                address_text: d.address_string(),
                name: d.name,
                address: d.address,
                connected: d.connected,
            })
            .collect(),
        Err(e) => {
            tracing::warn!("BLE scan failed: {e}");
            Vec::new()
        }
    }
}

#[cfg(not(windows))]
fn scan_ble() -> Vec<BlePeer> {
    Vec::new()
}

#[cfg(windows)]
fn read_battery(address: u64) -> Option<Battery> {
    match kb_transport::ble::read_battery(address) {
        Ok(percent) => Some(Battery {
            percent,
            read_at_ms: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or_default(),
        }),
        Err(e) => {
            tracing::warn!("battery read failed: {e}");
            None
        }
    }
}

#[cfg(not(windows))]
fn read_battery(_address: u64) -> Option<Battery> {
    let _ = (SystemTime::now(), UNIX_EPOCH);
    None
}
