//! Device actor: the only code that talks to keyboards.
//!
//! One thread owns the HID subsystem and every open device. Requests from
//! the UI arrive on a channel and run one at a time, so protocol sessions
//! (START … COMMIT) can never interleave. Requests already waiting are
//! taken together, and of the light frames among them only the newest per
//! layer is applied (see [`next_batch`]): frames that piled up during a
//! long request (a screen upload) don't play back seconds late. Between
//! requests the actor
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
//! Every flash write (effects, per-key colours, keymap, macros, screen) is
//! announced while it runs (`flash-write`): closing the window asks first,
//! and Windows is told why shutting down now is a bad idea.
//!
//! It also keeps the app's record of each model's keymap (`keymap.json`)
//! and macros (`macros.json`): the keyboard cannot report either, so a
//! record is updated only after a write the keyboard confirmed.
//!
//! Before anything is written to the keyboard's memory (effects, per-key
//! colours, keymap, macros, screen uploads) [`write_block`] decides whether
//! it may go: not with several keyboards on the cable, not while another
//! program talks to the keyboard, and on firmware nobody tested only after
//! the user said so in this session. The clock waits for that go-ahead too;
//! live frames never wait ([`write_gate`]).

use crate::dto::{Battery, FlashWrite, KeyboardInfo, LedColor, UnsupportedKeyboard};
use crate::settings::{
    self, ClockSettings, Keymaps, MacroRecord, StoredKeymap, StoredLayer, StoredMacro, StoredMacros,
};
use kb_core::color::LedCalibration;
use kb_core::conflicts::Conflict;
use kb_core::detect::BlePeer;
use kb_core::readback;
use kb_core::sonix::SonixFf13;
use kb_devices::{ConnectionKind, Model};
use kb_protocol::sonix::keymap::{KeyAction, Layer};
use kb_protocol::sonix::screen::ScreenAnimation;
use kb_protocol::sonix::{scale_to_level, EffectSettings, Rgb, MAX_LEVEL};
use kb_transport::hid::{Hid, HidChannel};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

pub const EVENT_KEYBOARDS_CHANGED: &str = "keyboards-changed";
/// The keyboards the app sees but can't drive yet changed; the payload is
/// the new list (`UnsupportedKeyboard`).
pub const EVENT_UNSUPPORTED_CHANGED: &str = "unsupported-keyboards-changed";
pub const EVENT_MIRROR: &str = "lighting-mirror";
pub const EVENT_CLOCK: &str = "clock-changed";
pub const EVENT_SCREEN_PROGRESS: &str = "screen-upload-progress";
/// A write was refused because the keyboard's firmware was not tested; the
/// payload is its version ("1.07"). The UI asks the user (also for writes
/// the live mode makes in the background).
pub const EVENT_FIRMWARE_UNVERIFIED: &str = "firmware-unverified";
/// A flash write started (`"screen"` | `"settings"`) or ended (`null`).
pub const EVENT_FLASH_WRITE: &str = "flash-write";
/// What Windows shows when the user shuts down during a flash write, until
/// the UI sends the text in its language.
const WRITE_REASON: &str = "Saving to the keyboard. Shutting down now can leave it half written.";
/// Longest wait for a screen upload (140 frames take ~5.4 min).
pub const SCREEN_UPLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Progress events at most this often.
const SCREEN_PROGRESS_EVERY: Duration = Duration::from_millis(150);

/// Error when a request needs the cable. English, like every backend error;
/// the UI shows its own text for it (lib/errors.ts).
const NO_USB: &str = "no keyboard on the USB cable";

/// Why a write to the keyboard's memory may not go now. The texts are the
/// backend's English errors; lib/errors.ts maps each to a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteBlock {
    /// More than one keyboard on the cable: which one would get it is up to
    /// the order the OS lists them in.
    SeveralKeyboards,
    /// A known program talks to the same channel; commands would interleave.
    Program(Conflict),
    /// Firmware not in the model's tested list, not confirmed this session.
    UnverifiedFirmware(String),
}

impl std::fmt::Display for WriteBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SeveralKeyboards => write!(f, "more than one keyboard on the USB cable"),
            Self::Program(c) => write!(f, "another program is using the keyboard ({})", c.id()),
            Self::UnverifiedFirmware(v) => write!(f, "untested keyboard firmware {v}"),
        }
    }
}

/// The wired keyboard's firmware, as far as writes are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareCheck {
    pub release: u16,
    /// In the model file's `verified_firmware` list.
    pub verified: bool,
    /// The user said to go ahead on this version in this session.
    pub confirmed: bool,
}

/// Whether a write to the keyboard's memory may go. `wired`: keyboards on
/// the cable; `running`: looks for a conflicting program (asked only when
/// the cheaper checks passed, it lists every process).
pub fn write_block(
    wired: usize,
    firmware: Option<FirmwareCheck>,
    running: impl FnOnce() -> Option<Conflict>,
) -> Option<WriteBlock> {
    if wired > 1 {
        return Some(WriteBlock::SeveralKeyboards);
    }
    if let Some(c) = running() {
        return Some(WriteBlock::Program(c));
    }
    firmware_block(firmware)
}

/// An untested firmware version the user has not said yes to in this session.
fn firmware_block(firmware: Option<FirmwareCheck>) -> Option<WriteBlock> {
    match firmware {
        Some(f) if !f.verified && !f.confirmed => {
            Some(WriteBlock::UnverifiedFirmware(kb_devices::firmware_text(f.release)))
        }
        _ => None,
    }
}

/// What a write to the keyboard changes, as far as [`write_gate`] is
/// concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteKind {
    /// The keyboard's memory: effects, per-key colours, keymap, macros and
    /// screen uploads.
    Memory,
    /// The keyboard's clock: "Sync now" and the sync when it connects.
    Clock,
    /// A live light frame (gauges, music, game, timer, previews): RAM only,
    /// gone as soon as the stream stops.
    Live,
}

/// Whether a write of `kind` may go now.
///
/// * Memory: every check of [`write_block`].
/// * Clock: only an untested firmware waits, for the user's go-ahead (the
///   same question a save asks; a yes holds for the session).
/// * Live frames never wait: they leave nothing behind on the keyboard.
pub fn write_gate(
    kind: WriteKind,
    wired: usize,
    firmware: Option<FirmwareCheck>,
    running: impl FnOnce() -> Option<Conflict>,
) -> Option<WriteBlock> {
    match kind {
        WriteKind::Memory => write_block(wired, firmware, running),
        WriteKind::Clock => firmware_block(firmware),
        WriteKind::Live => None,
    }
}

/// A known program talking to the keyboard, among those running now.
fn running_conflict() -> Option<Conflict> {
    let programs = kb_winsys::running_programs();
    let found = kb_core::conflicts::find_conflict(
        programs.iter().map(|p| (p.pid, p.name.as_str(), p.path.as_deref())),
        std::process::id(),
    );
    if let Some(c) = found {
        tracing::warn!(program = c.id(), "write refused: another program uses the keyboard");
    }
    found
}

/// Open a vendor channel only if its report layout is the "04 xx" family's
/// (kb_core::shape): the same IDs also belong to boards that speak another
/// protocol, and nothing may be sent to those.
fn open_checked(hid: &Hid, path: &str, display: bool) -> Result<HidChannel, String> {
    let iface = hid.interfaces().into_iter().find(|i| i.path == path).ok_or("keyboard disconnected")?;
    let channel = hid.open(&iface).map_err(|e| e.to_string())?;
    let desc = channel.report_descriptor().map_err(|e| e.to_string())?;
    let ok = if display { kb_core::shape::is_display_shape(&desc) } else { kb_core::shape::is_command_shape(&desc) };
    if !ok {
        tracing::warn!(
            usage_page = format_args!("{:04X}", iface.usage_page),
            shape = ?kb_core::shape::report_shape(&desc),
            "unexpected report layout; channel not used"
        );
        return Err("unsupported keyboard firmware: unexpected report layout".into());
    }
    Ok(channel)
}

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
/// need to be ready (S2; docs site: developers/protocol).
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
    /// One background light layer (see [`LightLayer`]): streamed while no
    /// UI preview (`SetLive`) is set. `None` takes the layer away.
    SetLayer(LightLayer, Option<LiveFrame>),
    /// A game wants the keyboard left alone (see `Actor::quiet`).
    SetQuiet(Quiet),
    /// The game layer is what the user asked to try ("Try on the keyboard"
    /// on the Games page): it shows even while a game wants quiet.
    SetGameTried(bool),
    /// The agents' layer still shows while a game wants the keyboard left
    /// alone (the user turned "hide while playing" off).
    SetAgentInQuiet(bool),
    /// Start (`Some`, with what the keys should show) or stop the mirror.
    SetMirror(Option<MirrorExpect>),
    /// New colour calibration; re-sends the live frame at once so sliders
    /// give immediate feedback on the keys.
    SetCalibration(LedCalibration),
    /// Set the keyboard clock to the PC's local time now.
    SyncClock(Reply<ClockSettings>),
    /// The user said to write on this firmware version ("1.07") anyway, for
    /// the rest of the session.
    ConfirmFirmware(String),
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
    /// Describe an unsupported keyboard for a support request: its ids,
    /// names, collections and their report descriptors. Read-only: the
    /// descriptors come from Windows, nothing is sent to the keyboard.
    DescribeUnsupported {
        id: String,
        reply: Reply<String>,
    },
    /// Upload an animation to the USB keyboard's GIF page (flash write).
    /// `id` only labels the progress events.
    UploadScreen {
        id: String,
        anim: Box<ScreenAnimation>,
        reply: mpsc::Sender<ScreenUploadResult>,
    },
}

/// What a frame request sets: the preview or one background layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameSlot {
    Live,
    Layer(LightLayer),
}

impl Request {
    /// The slot a `SetLive` or `SetLayer` sets (with a frame or `None`).
    fn frame_slot(&self) -> Option<FrameSlot> {
        match self {
            Self::SetLive(_) => Some(FrameSlot::Live),
            Self::SetLayer(layer, _) => Some(FrameSlot::Layer(*layer)),
            _ => None,
        }
    }

    /// Talks to a keyboard (or reads a device) and takes a while. Such a
    /// request runs in a turn of its own, after the frame set before it
    /// went out, as every request did before frames were batched. The
    /// others only change what the actor holds, so the order they are
    /// applied in within a turn is all that matters. Every variant is named
    /// so a new one has to be placed.
    fn uses_keyboard(&self) -> bool {
        match self {
            Self::ApplyEffect(..)
            | Self::ApplyPerKey { .. }
            | Self::SyncClock(_)
            | Self::ApplyKeymap { .. }
            | Self::ApplyMacros { .. }
            | Self::DescribeUnsupported { .. }
            | Self::UploadScreen { .. } => true,
            Self::Refresh
            | Self::SetLive(_)
            | Self::SetLayer(..)
            | Self::SetQuiet(_)
            | Self::SetGameTried(_)
            | Self::SetAgentInQuiet(_)
            | Self::SetMirror(_)
            | Self::SetCalibration(_)
            | Self::ConfirmFirmware(_)
            | Self::SetClockAuto(..) => false,
        }
    }
}

/// The requests to handle in one turn, from the front of `waiting`:
///
/// * a request that uses the keyboard, alone;
/// * else every request up to the next one that does, with each frame
///   request left out when a later one in the same run sets the same slot
///   (the preview, or the same layer; a `None` that takes the layer away
///   counts as a frame too).
///
/// Nothing is reordered, and only frame requests are ever left out: every
/// other request (each with its reply) is handled, in order. A frame left
/// out is one nothing would have shown: its slot is set again later in the
/// same turn, no frame goes out within a turn, and no request in the run
/// reads the slot. A run never reaches past a request that uses the
/// keyboard, so the frames set before a screen upload or a write still go
/// out before it, and the ones after it come after it.
fn next_batch(waiting: &mut VecDeque<Request>) -> Vec<Request> {
    if waiting.front().is_some_and(Request::uses_keyboard) {
        return waiting.pop_front().into_iter().collect();
    }
    let end = waiting.iter().position(Request::uses_keyboard).unwrap_or(waiting.len());
    let mut set = Vec::new();
    let mut batch: Vec<Request> = waiting
        .drain(..end)
        .rev()
        .filter(|r| match r.frame_slot() {
            Some(slot) if set.contains(&slot) => false,
            Some(slot) => {
                set.push(slot);
                true
            }
            None => true,
        })
        .collect();
    batch.reverse();
    batch
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

/// Background light layers, from the bottom up (owner's decision,
/// 2026-11-10; the game and profile layers 2026-11-16; music and the
/// agents 2026-11-22). A live frame covers the whole keyboard, so the
/// keyboard's own saved light only shows when no layer is set.
///
/// * `Profile`: the per-key design of the profile linked to the program in
///   front (per-app profiles), shown without writing the flash.
/// * `Gauges`: the system gauges (processor, graphics, memory bars) with
///   the rest of the keys dark or tinted.
/// * `Music`: the music lights, moving with the sound the PC plays, set
///   only while something plays. The gauges and the profile wait while it
///   shows.
/// * `Game`: a game's lights while a game session runs (its keys
///   highlighted, or League of Legends' health and events). The music and
///   the gauges wait while it shows.
/// * `Timer`: the focus timer. It uses the whole keyboard ("the light left
///   is the time left"), so the music and the gauges wait while it shows.
/// * `Agent`: the AI agents' and scripts' signals (`agent`), over whichever
///   of the five above shows. Only the keys it names: every key (an agent
///   waiting for the user covers the keyboard) or a few. The alerts, the
///   reminders and the nudges stay on top, so a muted microphone or a
///   meeting's key is never hidden by an agent.
/// * `Alerts`: only the keys it names (volume bar for a moment, the
///   microphone key), on top of the agents' keys, the timer or the gauges.
/// * `Reminders`: only the keys it names (a key blinking before a meeting),
///   on top of those.
/// * `Nudges`: only the keys it names (the wave of a reminder going off,
///   for a few seconds), on top of everything else.
///
/// A preview the user edits on the Lighting page (`SetLive`) goes over all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightLayer {
    Profile,
    Gauges,
    Music,
    Game,
    Timer,
    Agent,
    Alerts,
    Reminders,
    Nudges,
}

/// A game wants the keyboard left alone (the game session's quiet mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quiet {
    #[default]
    Off,
    /// Only the meetings' and reminders' lights and cards.
    ButReminders,
    /// Nothing at all.
    All,
}

/// The layers that still show while a game wants the keyboard left alone:
/// the meetings' and reminders' when the user lets them through, the game
/// layer when it is the user's own try, and the agents' when the user
/// turned their "hide while playing" off (`agent_in_quiet`).
fn quiet_layers(l: &Layers, quiet: Quiet, game_tried: bool, agent_in_quiet: bool) -> Layers {
    let reminders = quiet == Quiet::ButReminders;
    Layers {
        game: l.game.clone().filter(|_| game_tried),
        agent: l.agent.clone().filter(|_| agent_in_quiet),
        reminders: l.reminders.clone().filter(|_| reminders),
        nudges: l.nudges.clone().filter(|_| reminders),
        ..Layers::default()
    }
}

/// When a frame set now should go out: at once, unless one went out less
/// than [`LIVE_EVERY`] ago; then when that one's turn is up. A producer's
/// new frame and the loop's resend could otherwise both go in the same
/// timer step, doubling the frames the keyboard gets.
fn live_due(now: Instant, last_sent: Option<Instant>) -> Instant {
    last_sent.map_or(now, |t| now.max(t + LIVE_EVERY))
}

/// When the run loop wakes next: the next look for keyboards, the next
/// frame or mirror read (`frames`), or the clock sync. A sync waits while a
/// game wants the keyboard left alone (`SetQuiet` wakes the loop when that
/// ends), so a due one must not wake it then: it would never sleep.
fn wake_at(enumerate: Instant, frames: Option<Instant>, clock_due: Option<Instant>, quiet: Quiet) -> Instant {
    let due = clock_due.filter(|_| quiet == Quiet::Off);
    [frames, due].into_iter().flatten().fold(enumerate, Instant::min)
}

/// One LED of a composed frame: UI colour and linear brightness (0–1).
pub type LitKey = (u8, Rgb, f32);

/// The background layers, as handed in.
#[derive(Debug, Clone, Default)]
pub struct Layers {
    pub profile: Option<LiveFrame>,
    pub gauges: Option<LiveFrame>,
    pub music: Option<LiveFrame>,
    pub game: Option<LiveFrame>,
    pub timer: Option<LiveFrame>,
    pub agent: Option<LiveFrame>,
    pub alerts: Option<LiveFrame>,
    pub reminders: Option<LiveFrame>,
    pub nudges: Option<LiveFrame>,
}

impl Layers {
    fn slot(&mut self, layer: LightLayer) -> &mut Option<LiveFrame> {
        match layer {
            LightLayer::Profile => &mut self.profile,
            LightLayer::Gauges => &mut self.gauges,
            LightLayer::Music => &mut self.music,
            LightLayer::Game => &mut self.game,
            LightLayer::Timer => &mut self.timer,
            LightLayer::Agent => &mut self.agent,
            LightLayer::Alerts => &mut self.alerts,
            LightLayer::Reminders => &mut self.reminders,
            LightLayer::Nudges => &mut self.nudges,
        }
    }

    fn any(&self) -> bool {
        self.profile.is_some()
            || self.gauges.is_some()
            || self.music.is_some()
            || self.game.is_some()
            || self.timer.is_some()
            || self.agent.is_some()
            || self.alerts.is_some()
            || self.reminders.is_some()
            || self.nudges.is_some()
    }
}

/// What the keys show: the preview alone, or the timer (else the game,
/// else the music, else the gauges, else the profile) with the agents',
/// the alerts', the reminders' and then the nudges' keys on top. `None`
/// when nothing is set.
pub fn compose(live: Option<&LiveFrame>, l: &Layers) -> Option<Vec<LitKey>> {
    let keys = |f: &LiveFrame| f.colors.iter().map(|&(l, c)| (l, c, f.level)).collect::<Vec<_>>();
    if let Some(live) = live {
        return Some(keys(live));
    }
    let base = [&l.timer, &l.game, &l.music, &l.gauges, &l.profile];
    let mut out = base.into_iter().find_map(Option::as_ref).map(keys);
    let over = [l.agent.as_ref(), l.alerts.as_ref(), l.reminders.as_ref(), l.nudges.as_ref()];
    for over in over.into_iter().flatten() {
        let base = out.get_or_insert_with(Vec::new);
        for (led, c, level) in keys(over) {
            match base.iter_mut().find(|k| k.0 == led) {
                Some(k) => *k = (led, c, level),
                None => base.push((led, c, level)),
            }
        }
    }
    out
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
    /// Keyboards that look like SONiX or AJAZZ ones but match no model.
    pub unsupported: Vec<UnsupportedKeyboard>,
    pub calibration: LedCalibration,
    pub clock: ClockSettings,
    /// What the app last wrote to each model's keymap.
    pub keymaps: Keymaps,
    /// What the app last wrote to each model's macro memory.
    pub macros: MacroRecord,
    /// The untested firmware version ("1.07") the clock sync on connect
    /// waits on until the user says yes. Kept for a window that opens after
    /// the question went out (`firmware-unverified`).
    pub clock_waits_for: Option<String>,
    /// Whether a game wants the keyboard left alone now.
    pub quiet: Quiet,
}

/// One step of a screen upload, in order (uploads run one at a time). The
/// screen library uses it to keep an "incomplete" mark on disk while the
/// GIF page is being written, so a crash or a quit midway shows up on the
/// next start.
pub enum ScreenWriteStep<'a> {
    /// The upload session is about to open; nothing is written yet.
    /// `label` is the request's id (a library item, or `live:<source>`).
    Starting { label: &'a str, model_id: Option<&'a str> },
    /// It ended: `ok` finished, else failed after (`flash_written`) or
    /// before touching the flash.
    Ended { flash_written: bool, ok: bool },
}

pub type ScreenWriteHook = Box<dyn Fn(ScreenWriteStep<'_>) + Send>;

/// The flash write in progress, shared by the actor and the close guard.
/// While one runs the UI is told (`flash-write`) and Windows is asked to
/// hold a shutdown with a reason.
#[derive(Clone)]
pub struct FlashState {
    /// 0: none, else a [`FlashWrite`] code.
    kind: Arc<AtomicU8>,
    reason: Arc<Mutex<String>>,
    app: AppHandle,
}

impl FlashState {
    fn new(app: AppHandle) -> Self {
        Self { kind: Arc::new(AtomicU8::new(0)), reason: Arc::new(Mutex::new(WRITE_REASON.into())), app }
    }

    pub fn current(&self) -> Option<FlashWrite> {
        match self.kind.load(Ordering::SeqCst) {
            1 => Some(FlashWrite::Screen),
            2 => Some(FlashWrite::Settings),
            _ => None,
        }
    }

    /// The text Windows shows during a write, in the UI language.
    pub fn set_reason(&self, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).take(200).collect();
        if let (false, Ok(mut r)) = (text.trim().is_empty(), self.reason.lock()) {
            *r = text;
        }
    }

    /// A write starts; it ends when the guard drops. The event goes out
    /// before the flag is set: a close request that sees the flag finds the
    /// UI already told.
    fn begin(&self, kind: FlashWrite) -> FlashWriteGuard {
        let _ = self.app.emit(EVENT_FLASH_WRITE, Some(kind));
        self.kind.store(if kind == FlashWrite::Screen { 1 } else { 2 }, Ordering::SeqCst);
        let reason = self.reason.lock().map(|r| r.clone()).unwrap_or_else(|_| WRITE_REASON.into());
        self.hold_shutdown(Some(reason));
        FlashWriteGuard(self.clone())
    }

    fn end(&self) {
        self.kind.store(0, Ordering::SeqCst);
        let _ = self.app.emit(EVENT_FLASH_WRITE, None::<FlashWrite>);
        self.hold_shutdown(None);
    }

    /// `ShutdownBlockReasonCreate` (with a reason) or `…Destroy` on the
    /// main window. Windows takes these only from the window's own thread.
    fn hold_shutdown(&self, reason: Option<String>) {
        #[cfg(windows)]
        {
            use tauri::Manager;
            let app = self.app.clone();
            let r = self.app.run_on_main_thread(move || {
                let Some(hwnd) = app.get_webview_window("main").and_then(|w| w.hwnd().ok()) else { return };
                let hwnd = hwnd.0 as isize;
                let ok = match &reason {
                    Some(text) => kb_winsys::block_shutdown(hwnd, text),
                    None => kb_winsys::unblock_shutdown(hwnd),
                };
                if !ok {
                    tracing::debug!(hold = reason.is_some(), "shutdown hold not changed");
                }
            });
            if let Err(e) = r {
                tracing::warn!("shutdown hold not set: {e}");
            }
        }
        #[cfg(not(windows))]
        let _ = reason;
    }
}

/// Ends the flash write when dropped (also on an early return).
struct FlashWriteGuard(FlashState);

impl Drop for FlashWriteGuard {
    fn drop(&mut self) {
        self.0.end();
    }
}

pub struct DeviceHandle {
    pub snapshot: Arc<Mutex<Snapshot>>,
    /// The flash write in progress (close guard, shutdown hold).
    pub flash: FlashState,
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
/// record are saved; without it they last only for this run. `screen_hook`
/// hears each screen upload start and end.
pub fn spawn(
    app: AppHandle,
    calibration: LedCalibration,
    config_dir: Option<PathBuf>,
    screen_hook: ScreenWriteHook,
) -> DeviceHandle {
    let flash = FlashState::new(app.clone());
    let clock = config_dir.as_deref().map(settings::load_clock).unwrap_or_default();
    let keymaps = config_dir.as_deref().map(settings::load_keymaps).unwrap_or_default();
    let macros = config_dir.as_deref().map(settings::load_macros).unwrap_or_default();
    let snapshot = Arc::new(Mutex::new(Snapshot {
        keyboards: Vec::new(),
        unsupported: Vec::new(),
        calibration,
        clock,
        keymaps: keymaps.clone(),
        macros: macros.clone(),
        clock_waits_for: None,
        quiet: Quiet::Off,
    }));
    let (tx, rx) = mpsc::channel();
    let shared = snapshot.clone();
    let actor_flash = flash.clone();
    std::thread::Builder::new()
        .name("device-actor".into())
        .spawn(move || {
            let mut actor = Actor::new(app, shared, calibration, clock, keymaps, macros, config_dir);
            actor.flash = actor_flash;
            actor.screen_hook = screen_hook;
            actor.run(rx)
        })
        .expect("spawn device actor");
    DeviceHandle { snapshot, flash, tx }
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
    /// Keyboards seen that no model matches, or whose vendor channels don't
    /// have the model's report layout (for support requests).
    unknown: Vec<kb_core::UnknownKeyboard>,
    /// Report descriptors of the vendor channels on the cable, by path and
    /// firmware version (kb_core::classify checks their layout).
    layouts: HashMap<(String, u16), Vec<u8>>,
    last_ble_scan: Option<Instant>,
    batteries: HashMap<u64, (Battery, Instant)>,
    /// Last battery reading per model, kept on disk: shown over USB, where
    /// the keyboard cannot report it.
    last_battery: std::collections::BTreeMap<String, Battery>,
    /// Path of the wired command interface of the active keyboard, if any.
    command_path: Option<String>,
    /// Path of its display channel (screen pages), if any.
    display_path: Option<String>,
    /// Model of the keyboard behind `command_path`.
    usb_model: Option<&'static Model>,
    /// Its firmware version (USB `bcdDevice`).
    usb_firmware: Option<u16>,
    /// Keyboards on the cable, identical ones counted separately.
    wired: usize,
    /// Firmware version the user confirmed writes on in this session.
    firmware_confirmed: Option<u16>,
    command: Option<Command>,
    live: Option<LiveFrame>,
    /// Background layers, shown while `live` is not set.
    layers: Layers,
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
    /// The automatic clock sync found an untested firmware and waits for
    /// the user's go-ahead (`ConfirmFirmware`) while this keyboard stays.
    clock_waits: bool,
    /// A game wants the keyboard left alone: no light layer (or only the
    /// meetings' and reminders'), no automatic clock sync (a preview the
    /// user runs still shows).
    quiet: Quiet,
    /// The game layer is the user's own try: it shows even while quiet.
    game_tried: bool,
    /// The agents' layer shows even while quiet ("hide while playing" off).
    agent_in_quiet: bool,
    next_enumerate: Instant,
    next_live: Instant,
    /// When the last live frame went out.
    last_live: Option<Instant>,
    next_mirror: Instant,
    /// The flash write in progress (shared with [`DeviceHandle`]).
    flash: FlashState,
    screen_hook: ScreenWriteHook,
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
            flash: FlashState::new(app.clone()),
            screen_hook: Box::new(|_: ScreenWriteStep<'_>| {}),
            app,
            shared,
            hid,
            peers: Vec::new(),
            unknown: Vec::new(),
            layouts: HashMap::new(),
            last_ble_scan: None,
            batteries: HashMap::new(),
            last_battery: config_dir.as_deref().map(settings::load_batteries).unwrap_or_default(),
            command_path: None,
            display_path: None,
            usb_model: None,
            usb_firmware: None,
            wired: 0,
            firmware_confirmed: None,
            command: None,
            live: None,
            layers: Layers::default(),
            live_failures: 0,
            mirror: None,
            indicators: Vec::new(),
            calibration,
            clock,
            keymaps,
            macros,
            config_dir,
            clock_due: None,
            clock_waits: false,
            quiet: Quiet::Off,
            game_tried: false,
            agent_in_quiet: false,
            next_enumerate: now,
            next_live: now,
            last_live: None,
            next_mirror: now,
        }
    }

    fn run(mut self, rx: mpsc::Receiver<Request>) {
        // Taken from the channel, not handled yet.
        let mut waiting = VecDeque::new();
        loop {
            let now = Instant::now();
            if now >= self.next_enumerate {
                self.enumerate();
                self.next_enumerate = Instant::now() + ENUMERATE_EVERY;
            }
            if self.streaming() && now >= self.next_live {
                self.send_live();
                self.last_live = Some(Instant::now());
                self.next_live = Instant::now() + LIVE_EVERY;
            } else if self.mirror.is_some() && !self.streaming() && now >= self.next_mirror {
                self.send_mirror();
                self.next_mirror = Instant::now() + MIRROR_EVERY;
            }
            if let Some((due, attempts)) = self.clock_due {
                // Not during a game that wants the keyboard left alone.
                if now >= due && self.quiet == Quiet::Off {
                    self.clock_due = None;
                    match self.sync_clock() {
                        Ok(()) => {}
                        // The user was asked; a yes starts it (ConfirmFirmware).
                        Err(_) if firmware_block(self.firmware_check()).is_some() => self.wait_clock(true),
                        Err(e) => {
                            tracing::warn!(attempts_left = attempts - 1, "automatic clock sync failed: {e}");
                            if attempts > 1 && self.command_path.is_some() {
                                self.clock_due = Some((Instant::now() + CLOCK_RETRY, attempts - 1));
                            }
                        }
                    }
                }
            }

            if waiting.is_empty() {
                let frames = if self.streaming() {
                    Some(self.next_live)
                } else {
                    self.mirror.as_ref().map(|_| self.next_mirror)
                };
                let wake = wake_at(self.next_enumerate, frames, self.clock_due.map(|(due, _)| due), self.quiet);
                let timeout = wake.saturating_duration_since(Instant::now());
                match rx.recv_timeout(timeout) {
                    Ok(req) => waiting.push_back(req),
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            self.handle_waiting(&rx, &mut waiting);
        }
    }

    /// Handle one turn's requests ([`next_batch`]) out of those waiting
    /// now. After one that used the keyboard (a screen upload can take
    /// seconds), what came in meanwhile is taken too, so the next frame
    /// sent is the newest and not one from before the upload.
    fn handle_waiting(&mut self, rx: &mpsc::Receiver<Request>, waiting: &mut VecDeque<Request>) {
        waiting.extend(rx.try_iter());
        let used_keyboard = waiting.front().is_some_and(Request::uses_keyboard);
        self.handle_batch(waiting);
        if used_keyboard {
            waiting.extend(rx.try_iter());
            if waiting.front().is_some_and(|r| !r.uses_keyboard()) {
                self.handle_batch(waiting);
            }
        }
    }

    fn handle_batch(&mut self, waiting: &mut VecDeque<Request>) {
        let before = waiting.len();
        let batch = next_batch(waiting);
        let skipped = before - waiting.len() - batch.len();
        if skipped > 0 {
            tracing::debug!(skipped, "older light frames skipped");
        }
        for req in batch {
            self.handle(req);
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
                let _write = self.flash.begin(FlashWrite::Settings);
                settings.color = self.calibration.apply(settings.color);
                let r = self.may_write().and_then(|()| self.with_command(|d| d.apply_effect(&settings)));
                let _ = reply.send(r);
            }
            Request::ApplyPerKey { colors, brightness, reply } => {
                let _write = self.flash.begin(FlashWrite::Settings);
                let cal = self.calibration;
                let colors: Vec<(u8, Rgb)> = colors.into_iter().map(|(l, c)| (l, cal.apply(c))).collect();
                let r = self.may_write().and_then(|()| self.with_command(|d| d.apply_per_key(colors, brightness)));
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
                    (None, Some(_)) if self.has_layer() => {
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
            Request::SetLayer(layer, frame) => {
                let slot = self.layers.slot(layer);
                match (&frame, &slot) {
                    (None, Some(_)) => tracing::debug!(?layer, "light layer stopped"),
                    (Some(f), None) => tracing::debug!(?layer, leds = f.colors.len(), "light layer started"),
                    _ => {}
                }
                *slot = frame;
                if !self.streaming() {
                    tracing::debug!("no light layer left; keyboard reverts to its saved effect");
                }
                if self.live.is_none() {
                    self.next_live = live_due(Instant::now(), self.last_live);
                }
            }
            Request::SetQuiet(quiet) => {
                if self.quiet != quiet {
                    tracing::info!(?quiet, "keyboard left alone for a game");
                    self.quiet = quiet;
                    if let Ok(mut s) = self.shared.lock() {
                        s.quiet = quiet;
                    }
                    self.next_live = Instant::now();
                }
            }
            Request::SetGameTried(on) => {
                self.game_tried = on;
                self.next_live = Instant::now();
            }
            Request::SetAgentInQuiet(on) => {
                if self.agent_in_quiet != on {
                    tracing::debug!(on, "agent lights while a game wants quiet");
                    self.agent_in_quiet = on;
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
            Request::ConfirmFirmware(version) => {
                match self.usb_firmware.filter(|&r| kb_devices::firmware_text(r) == version) {
                    Some(release) => {
                        tracing::info!(firmware = %version, "writes on untested firmware confirmed for this session");
                        self.firmware_confirmed = Some(release);
                        if self.clock_waits {
                            tracing::debug!("automatic clock sync goes ahead");
                            self.wait_clock(false);
                            self.clock_due = Some((Instant::now(), CLOCK_ATTEMPTS));
                        }
                    }
                    // The keyboard changed while the user was asked.
                    None => tracing::warn!(firmware = %version, "confirmed firmware is not the connected one"),
                }
            }
            Request::SetClockAuto(on, reply) => {
                self.clock.auto_sync = on;
                if !on {
                    self.clock_due = None;
                    self.wait_clock(false);
                }
                let _ = reply.send(self.store_clock().map(|_| self.clock));
            }
            Request::ApplyKeymap { layer, entries, reply } => {
                let _write = self.flash.begin(FlashWrite::Settings);
                let _ = reply.send(self.apply_keymap(layer, entries));
            }
            Request::ApplyMacros { slots, reply } => {
                let _write = self.flash.begin(FlashWrite::Settings);
                let _ = reply.send(self.apply_macros(slots));
            }
            Request::DescribeUnsupported { id, reply } => {
                let _ = reply.send(self.describe_unsupported(&id));
            }
            Request::UploadScreen { id, anim, reply } => {
                let _write = self.flash.begin(FlashWrite::Screen);
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
        let Some(path) = self.display_path.clone() else { return fail(model_id, false, NO_USB.into()) };
        if let Err(e) = self.may_write() {
            return fail(model_id, false, e);
        }
        let display = self
            .hid
            .as_ref()
            .ok_or_else(|| "HID unavailable".to_string())
            .and_then(|hid| open_checked(hid, &path, true));
        let mut display = match display {
            Ok(d) => d,
            Err(e) => return fail(model_id, false, e),
        };
        let app = self.app.clone();
        let label = id.to_string();
        let mut last = Instant::now() - SCREEN_PROGRESS_EVERY;
        let mut started = false;
        let started_at = Instant::now();
        (self.screen_hook)(ScreenWriteStep::Starting { label: id, model_id: model_id.as_deref() });
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
        (self.screen_hook)(ScreenWriteStep::Ended { flash_written: r.is_ok() || started, ok: r.is_ok() });
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
        let model = self.usb_model.ok_or_else(|| NO_USB.to_string())?;
        self.may_write()?;
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
        let model = self.usb_model.ok_or_else(|| NO_USB.to_string())?;
        let map = kb_core::keymap::canonical(model, layer, entries).map_err(|e| e.to_string())?;
        self.may_write()?;
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

    /// Set the keyboard clock to the PC's local time, then record it. On an
    /// untested firmware only after the user's go-ahead ([`write_gate`]).
    fn sync_clock(&mut self) -> Result<(), String> {
        self.gate(WriteKind::Clock)?;
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

    /// The automatic clock sync starts (or stops) waiting for the user's
    /// go-ahead on an untested firmware. The version it waits on is kept
    /// for the UI, which may open after the question went out.
    fn wait_clock(&mut self, waits: bool) {
        if waits {
            tracing::info!("automatic clock sync waits for the user: firmware not tested");
        }
        self.clock_waits = waits;
        let version = self.usb_firmware.filter(|_| waits).map(kb_devices::firmware_text);
        if let Ok(mut s) = self.shared.lock() {
            s.clock_waits_for = version;
        }
    }

    /// Whether a write to the keyboard's memory may go now (see
    /// [`write_block`]).
    fn may_write(&self) -> Result<(), String> {
        self.gate(WriteKind::Memory)
    }

    /// The wired keyboard's firmware: tested, or said yes to this session.
    fn firmware_check(&self) -> Option<FirmwareCheck> {
        self.usb_model.zip(self.usb_firmware).map(|(model, release)| FirmwareCheck {
            release,
            verified: model.mode(ConnectionKind::Usb).is_some_and(|mode| mode.firmware_verified(release)),
            confirmed: self.firmware_confirmed == Some(release),
        })
    }

    /// Whether a write of `kind` may go now ([`write_gate`]). An untested
    /// firmware also tells the UI, so it can ask the user even when the
    /// write came from the background (the live mode, the clock sync on
    /// connect).
    fn gate(&self, kind: WriteKind) -> Result<(), String> {
        // Without a keyboard on the cable the write fails on its own.
        if self.command_path.is_none() {
            return Ok(());
        }
        let Some(block) = write_gate(kind, self.wired, self.firmware_check(), running_conflict) else {
            return Ok(());
        };
        match &block {
            WriteBlock::UnverifiedFirmware(v) => {
                tracing::warn!(firmware = %v, ?kind, "write waits for the user: firmware not tested");
                let _ = self.app.emit(EVENT_FIRMWARE_UNVERIFIED, v);
            }
            WriteBlock::SeveralKeyboards => tracing::warn!(keyboards = self.wired, "write refused: several keyboards"),
            WriteBlock::Program(_) => {}
        }
        Err(block.to_string())
    }

    /// Run `f` against the wired command channel, opening it on demand.
    /// Any transport error drops the handle so the next call reopens it.
    fn with_command<T>(
        &mut self,
        f: impl FnOnce(&mut SonixFf13<HidChannel>) -> kb_core::sonix::Result<T>,
    ) -> Result<T, String> {
        let path = self.command_path.clone().ok_or_else(|| NO_USB.to_string())?;
        if self.command.as_ref().is_none_or(|c| c.path != path) {
            let hid = self.hid.as_ref().ok_or("HID unavailable")?;
            let channel = open_checked(hid, &path, false)?;
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
        self.live.is_some() || self.has_layer()
    }

    fn has_layer(&self) -> bool {
        let tried = self.game_tried && self.layers.game.is_some();
        let agent = self.agent_in_quiet && self.layers.agent.is_some();
        match self.quiet {
            Quiet::Off => self.layers.any(),
            Quiet::ButReminders => self.layers.reminders.is_some() || self.layers.nudges.is_some() || tried || agent,
            Quiet::All => tried || agent,
        }
    }

    fn send_live(&mut self) {
        if self.command_path.is_none() {
            return;
        }
        // Live frames leave nothing on the keyboard: they never wait, not
        // even on an untested firmware (write_gate).
        if let Err(e) = self.gate(WriteKind::Live) {
            tracing::debug!("live frame held: {e}");
            return;
        }
        let heard;
        let layers = if self.quiet == Quiet::Off {
            &self.layers
        } else {
            heard = quiet_layers(&self.layers, self.quiet, self.game_tried, self.agent_in_quiet);
            &heard
        };
        let Some(keys) = compose(self.live.as_ref(), layers) else {
            return;
        };
        let cal = self.calibration;
        let frame: Vec<(u8, Rgb)> = keys.into_iter().map(|(l, c, level)| (l, cal.apply_dimmed(c, level))).collect();
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
        // The report layouts of the vendor channels on the cable, read once
        // per collection and firmware version (from Windows; nothing is sent
        // to the keyboard). A failed read is tried again next time.
        self.layouts.retain(|(path, _), _| interfaces.iter().any(|i| &i.path == path));
        let (hid, layouts) = (self.hid.as_ref(), &mut self.layouts);
        let found = kb_core::classify(&interfaces, &self.peers, |iface| {
            let key = (iface.path.clone(), iface.release);
            if let Some(desc) = layouts.get(&key) {
                return Some(desc.clone());
            }
            match hid?.report_descriptor_at(&iface.path) {
                Ok(desc) => {
                    layouts.insert(key, desc.clone());
                    Some(desc)
                }
                Err(e) => {
                    tracing::debug!(
                        usage_page = format_args!("{:04X}", iface.usage_page),
                        "report layout not read: {e}"
                    );
                    None
                }
            }
        });
        let detected = found.keyboards;
        self.unknown = found.unknown;
        let unsupported: Vec<UnsupportedKeyboard> =
            self.unknown.iter().map(UnsupportedKeyboard::from_unknown).collect();

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
            if self.clock_waits {
                self.wait_clock(false);
            }
        }
        self.usb_model = usb.filter(|k| k.command.is_some()).map(|k| k.model);
        self.usb_firmware = usb.and_then(|k| k.firmware);
        let wired = kb_core::wired_count(&detected);
        if wired != self.wired && wired > 1 {
            tracing::warn!(keyboards = wired, "several keyboards on the cable; writes are refused");
        }
        self.wired = wired;
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
                            self.remember_battery(&k.model.id, &b);
                            self.batteries.insert(peer.address, (b, Instant::now()));
                        }
                    }
                    self.batteries.get(&peer.address).map(|(b, _)| b.clone())
                }
                (Some(peer), _) => self.batteries.get(&peer.address).map(|(b, _)| b.clone()),
                _ => None,
            }
            .or_else(|| self.last_battery.get(&k.model.id).cloned());
            infos.push(KeyboardInfo::from_detected(k, battery));
        }

        let (changed, unsupported_changed) = {
            let mut snap = self.shared.lock().expect("snapshot lock");
            let changed = snap.keyboards != infos;
            if changed {
                snap.keyboards = infos.clone();
            }
            let unsupported_changed = snap.unsupported != unsupported;
            if unsupported_changed {
                snap.unsupported = unsupported.clone();
            }
            (changed, unsupported_changed)
        };
        if unsupported_changed {
            let ids: Vec<&str> = unsupported.iter().map(|k| k.id.as_str()).collect();
            tracing::info!(?ids, "unsupported keyboards changed");
            if let Err(e) = self.app.emit(EVENT_UNSUPPORTED_CHANGED, &unsupported) {
                tracing::warn!("emit failed: {e}");
            }
        }
        if changed {
            tracing::info!(count = infos.len(), "keyboards changed");
            if let Err(e) = self.app.emit(EVENT_KEYBOARDS_CHANGED, &infos) {
                tracing::warn!("emit failed: {e}");
            }
        }
    }
}

impl Actor {
    /// The support request text for the unsupported keyboard `id`.
    fn describe_unsupported(&self, id: &str) -> Result<String, String> {
        let k = self.unknown.iter().find(|k| k.key() == id).ok_or("unsupported keyboard not found")?;
        let hid = self.hid.as_ref().ok_or("no HID access")?;
        let descriptors: Vec<Result<Vec<u8>, String>> =
            k.paths.iter().map(|p| hid.report_descriptor_at(p).map_err(|e| e.to_string())).collect();
        Ok(crate::diagnostics::keyboard_details(k, &descriptors))
    }

    /// Keep a battery reading for when the keyboard is on the cable. Written
    /// to disk when the level changes or every half hour.
    fn remember_battery(&mut self, model_id: &str, b: &Battery) {
        let write = self
            .last_battery
            .get(model_id)
            .is_none_or(|old| old.percent != b.percent || b.read_at_ms.saturating_sub(old.read_at_ms) >= 30 * 60_000);
        self.last_battery.insert(model_id.to_string(), b.clone());
        if let (true, Some(dir)) = (write, &self.config_dir) {
            if let Err(e) = settings::save_batteries(dir, &self.last_battery) {
                tracing::warn!("battery reading not saved: {e}");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(keys: &[(u8, u8)], level: f32) -> LiveFrame {
        LiveFrame { colors: keys.iter().map(|&(l, v)| (l, [v, v, v])).collect(), level }
    }

    #[test]
    fn the_timer_hides_the_gauges_and_alerts_go_on_top() {
        let gauges = frame(&[(1, 10), (2, 10), (3, 10)], 0.5);
        let timer = frame(&[(1, 20), (2, 20), (3, 20)], 0.8);
        let alerts = frame(&[(2, 99), (9, 99)], 1.0);
        let only_gauges = Layers { gauges: Some(gauges.clone()), ..Layers::default() };
        assert_eq!(compose(None, &only_gauges), Some(vec![(1, [10; 3], 0.5), (2, [10; 3], 0.5), (3, [10; 3], 0.5)]));
        let all = Layers {
            profile: None,
            gauges: Some(gauges),
            music: None,
            game: None,
            timer: Some(timer),
            agent: None,
            alerts: Some(alerts.clone()),
            reminders: Some(frame(&[(9, 7)], 1.0)),
            nudges: Some(frame(&[(3, 50)], 1.0)),
        };
        // A reminder's wave goes over everything; a meeting's key over the alerts.
        assert_eq!(
            compose(None, &all),
            Some(vec![(1, [20; 3], 0.8), (2, [99; 3], 1.0), (3, [50; 3], 1.0), (9, [7; 3], 1.0)])
        );
        // Alerts alone light only their keys.
        let only_alerts = Layers { alerts: Some(alerts), ..Layers::default() };
        assert_eq!(compose(None, &only_alerts), Some(vec![(2, [99; 3], 1.0), (9, [99; 3], 1.0)]));
        assert_eq!(compose(None, &Layers::default()), None);
    }

    #[test]
    fn a_game_hides_the_gauges_and_the_profile_lies_under_everything() {
        let profile = frame(&[(1, 5), (2, 5)], 1.0);
        let gauges = frame(&[(1, 10), (2, 10)], 0.5);
        let game = frame(&[(1, 30), (2, 30)], 1.0);
        let mic = frame(&[(2, 99)], 1.0);
        let only_profile = Layers { profile: Some(profile.clone()), ..Layers::default() };
        assert_eq!(compose(None, &only_profile), Some(vec![(1, [5; 3], 1.0), (2, [5; 3], 1.0)]));
        let gauges_over_profile =
            Layers { profile: Some(profile.clone()), gauges: Some(gauges.clone()), ..Layers::default() };
        assert_eq!(compose(None, &gauges_over_profile), Some(vec![(1, [10; 3], 0.5), (2, [10; 3], 0.5)]));
        let in_game = Layers {
            profile: Some(profile),
            gauges: Some(gauges),
            game: Some(game),
            alerts: Some(mic),
            ..Layers::default()
        };
        // The muted microphone stays on top of the game.
        assert_eq!(compose(None, &in_game), Some(vec![(1, [30; 3], 1.0), (2, [99; 3], 1.0)]));
    }

    #[test]
    fn several_keyboards_refuse_before_anything_else() {
        let untested = Some(FirmwareCheck { release: 0x0107, verified: false, confirmed: false });
        let scanned = std::cell::Cell::new(false);
        let scan = || {
            scanned.set(true);
            Some(Conflict::SignalRgb)
        };
        assert_eq!(write_block(2, untested, scan), Some(WriteBlock::SeveralKeyboards));
        assert!(!scanned.get(), "no process scan when the answer is already no");
        assert_eq!(WriteBlock::SeveralKeyboards.to_string(), "more than one keyboard on the USB cable");
        assert_eq!(write_block(1, None, || None), None);
        assert_eq!(write_block(0, None, || None), None, "no keyboard: the write fails on its own");
    }

    #[test]
    fn a_program_on_the_channel_refuses_writes() {
        let tested = Some(FirmwareCheck { release: 0x0106, verified: true, confirmed: false });
        let block = write_block(1, tested, || Some(Conflict::AjazzDriver));
        assert_eq!(block, Some(WriteBlock::Program(Conflict::AjazzDriver)));
        assert_eq!(block.unwrap().to_string(), "another program is using the keyboard (ajazz-driver)");
    }

    #[test]
    fn untested_firmware_needs_the_users_go_ahead() {
        let check = |verified, confirmed| Some(FirmwareCheck { release: 0x0107, verified, confirmed });
        let block = write_block(1, check(false, false), || None);
        assert_eq!(block, Some(WriteBlock::UnverifiedFirmware("1.07".into())));
        assert_eq!(block.unwrap().to_string(), "untested keyboard firmware 1.07");
        assert_eq!(write_block(1, check(false, true), || None), None, "confirmed this session");
        assert_eq!(write_block(1, check(true, false), || None), None, "tested");
        // A program on the channel is reported first: asking about the
        // firmware would not help while it runs.
        assert_eq!(
            write_block(1, check(false, false), || Some(Conflict::OpenRgb)),
            Some(WriteBlock::Program(Conflict::OpenRgb))
        );
    }

    #[test]
    fn on_untested_firmware_the_clock_waits_and_live_frames_do_not() {
        let untested = Some(FirmwareCheck { release: 0x0107, verified: false, confirmed: false });
        let asked = Some(WriteBlock::UnverifiedFirmware("1.07".into()));
        // The clock sync (on connect or "Sync now") waits for the same
        // go-ahead a save does; live frames (RAM only) go on.
        assert_eq!(write_gate(WriteKind::Memory, 1, untested, || None), asked);
        assert_eq!(write_gate(WriteKind::Clock, 1, untested, || None), asked);
        assert_eq!(write_gate(WriteKind::Live, 1, untested, || None), None);
        let clock = write_gate(WriteKind::Clock, 1, untested, || None).map(|b| b.to_string());
        assert_eq!(clock.as_deref(), Some("untested keyboard firmware 1.07"), "the UI asks on this text");

        // A yes this session, or a tested version: everything goes.
        let confirmed = Some(FirmwareCheck { release: 0x0107, verified: false, confirmed: true });
        let tested = Some(FirmwareCheck { release: 0x0106, verified: true, confirmed: false });
        for kind in [WriteKind::Memory, WriteKind::Clock, WriteKind::Live] {
            assert_eq!(write_gate(kind, 1, confirmed, || None), None, "{kind:?} confirmed");
            assert_eq!(write_gate(kind, 1, tested, || None), None, "{kind:?} tested");
            assert_eq!(write_gate(kind, 1, None, || None), None, "{kind:?} version unknown");
        }

        // Only saves stop for several keyboards or another program; the
        // clock never looked for those, and it does not list programs.
        assert_eq!(write_gate(WriteKind::Memory, 2, tested, || None), Some(WriteBlock::SeveralKeyboards));
        assert_eq!(write_gate(WriteKind::Clock, 2, tested, || panic!("programs listed")), None);
        assert_eq!(write_gate(WriteKind::Live, 2, untested, || panic!("programs listed")), None);
    }

    #[test]
    fn a_new_layer_frame_waits_for_the_last_ones_turn() {
        let now = Instant::now();
        // Nothing sent lately: at once.
        assert_eq!(live_due(now, None), now);
        assert_eq!(live_due(now, Some(now - LIVE_EVERY * 2)), now);
        // The resend went out 10 ms ago: the new frame takes the next turn,
        // not a second slot in the same 50 ms.
        let sent = now - Duration::from_millis(10);
        assert_eq!(live_due(now, Some(sent)), sent + LIVE_EVERY);
    }

    #[test]
    fn a_clock_sync_due_while_quiet_lets_the_loop_sleep() {
        let now = Instant::now();
        let enumerate = now + Duration::from_secs(5);
        // Overdue by the time the loop looks.
        let due = Some(now);
        assert_eq!(wake_at(enumerate, None, due, Quiet::Off), now);
        assert_eq!(wake_at(enumerate, None, due, Quiet::All), enumerate, "no busy loop while quiet");
        assert_eq!(wake_at(enumerate, None, due, Quiet::ButReminders), enumerate);
        // Frames still go on time.
        let frame = now + Duration::from_millis(50);
        assert_eq!(wake_at(enumerate, Some(frame), due, Quiet::ButReminders), frame);
        assert_eq!(wake_at(enumerate, None, None, Quiet::Off), enumerate);
    }

    #[test]
    fn the_users_try_shows_while_a_game_wants_quiet() {
        let layers = Layers {
            gauges: Some(frame(&[(1, 10)], 1.0)),
            game: Some(frame(&[(1, 30), (2, 30)], 1.0)),
            reminders: Some(frame(&[(2, 7)], 1.0)),
            ..Layers::default()
        };
        // The game's own lights wait; the meeting's key goes through.
        assert_eq!(
            compose(None, &quiet_layers(&layers, Quiet::ButReminders, false, false)),
            Some(vec![(2, [7; 3], 1.0)])
        );
        assert_eq!(compose(None, &quiet_layers(&layers, Quiet::All, false, false)), None);
        // "Try on the keyboard" shows, under the meeting's key.
        assert_eq!(
            compose(None, &quiet_layers(&layers, Quiet::ButReminders, true, false)),
            Some(vec![(1, [30; 3], 1.0), (2, [7; 3], 1.0)])
        );
        assert_eq!(
            compose(None, &quiet_layers(&layers, Quiet::All, true, false)),
            Some(vec![(1, [30; 3], 1.0), (2, [30; 3], 1.0)])
        );
    }

    #[test]
    fn the_lighting_preview_goes_over_every_layer() {
        let live = frame(&[(5, 50)], 1.0);
        let timer = frame(&[(1, 20)], 0.8);
        let alerts = frame(&[(2, 99)], 1.0);
        let layers = Layers { timer: Some(timer), alerts: Some(alerts), ..Layers::default() };
        assert_eq!(compose(Some(&live), &layers), Some(vec![(5, [50; 3], 1.0)]));
    }

    #[test]
    fn music_lies_between_the_gauges_and_the_game() {
        let with = |l: &Layers| compose(None, l);
        let playing = Layers {
            profile: Some(frame(&[(1, 5), (2, 5)], 1.0)),
            gauges: Some(frame(&[(1, 10), (2, 10)], 0.5)),
            music: Some(frame(&[(1, 40), (2, 40)], 0.8)),
            alerts: Some(frame(&[(2, 99)], 1.0)),
            ..Layers::default()
        };
        // Over the gauges and the profile; the volume bar over the music.
        assert_eq!(with(&playing), Some(vec![(1, [40; 3], 0.8), (2, [99; 3], 1.0)]));
        // The game's lights and the focus timer cover it.
        let in_game = Layers { game: Some(frame(&[(1, 30), (2, 30)], 1.0)), ..playing.clone() };
        assert_eq!(with(&in_game), Some(vec![(1, [30; 3], 1.0), (2, [99; 3], 1.0)]));
        let timer = Layers { timer: Some(frame(&[(1, 20), (2, 20)], 0.8)), ..playing.clone() };
        assert_eq!(with(&timer), Some(vec![(1, [20; 3], 0.8), (2, [99; 3], 1.0)]));
        // A reminder's wave goes over it too.
        let wave = Layers { nudges: Some(frame(&[(1, 77)], 1.0)), ..playing.clone() };
        assert_eq!(with(&wave), Some(vec![(1, [77; 3], 1.0), (2, [99; 3], 1.0)]));
        // When the music stops, the gauges show again.
        let stopped = Layers { music: None, alerts: None, ..playing.clone() };
        assert_eq!(with(&stopped), Some(vec![(1, [10; 3], 0.5), (2, [10; 3], 0.5)]));
        // Alone, it is the whole frame.
        let alone = Layers { music: playing.music.clone(), ..Layers::default() };
        assert!(alone.any());
        assert_eq!(with(&alone), Some(vec![(1, [40; 3], 0.8), (2, [40; 3], 0.8)]));
    }

    #[test]
    fn a_game_that_wants_quiet_hides_the_music() {
        let mut layers = Layers::default();
        *layers.slot(LightLayer::Music) = Some(frame(&[(1, 40)], 1.0));
        assert_eq!(compose(None, &layers), Some(vec![(1, [40; 3], 1.0)]));
        assert_eq!(compose(None, &quiet_layers(&layers, Quiet::All, false, false)), None);
        assert_eq!(compose(None, &quiet_layers(&layers, Quiet::ButReminders, false, false)), None);
        // The user's try of the game's lights is not the music.
        assert_eq!(compose(None, &quiet_layers(&layers, Quiet::All, true, false)), None);
    }

    #[test]
    fn the_agents_lie_over_every_base_and_under_the_alerts() {
        let agent_all = frame(&[(1, 60), (2, 60), (3, 60)], 1.0);
        let agent_some = frame(&[(3, 60)], 1.0);
        // Over each base layer alone: the whole keyboard, or only its keys.
        for base in [LightLayer::Profile, LightLayer::Gauges, LightLayer::Music, LightLayer::Game, LightLayer::Timer] {
            let mut layers = Layers::default();
            *layers.slot(base) = Some(frame(&[(1, 10), (2, 10), (3, 10)], 0.5));
            *layers.slot(LightLayer::Agent) = Some(agent_all.clone());
            assert_eq!(
                compose(None, &layers),
                Some(vec![(1, [60; 3], 1.0), (2, [60; 3], 1.0), (3, [60; 3], 1.0)]),
                "{base:?}"
            );
            *layers.slot(LightLayer::Agent) = Some(agent_some.clone());
            assert_eq!(
                compose(None, &layers),
                Some(vec![(1, [10; 3], 0.5), (2, [10; 3], 0.5), (3, [60; 3], 1.0)]),
                "{base:?}"
            );
        }
        // The muted microphone, a meeting's key and a reminder's wave stay on
        // top of an agent that covers the keyboard.
        let layers = Layers {
            timer: Some(frame(&[(1, 20), (2, 20), (3, 20), (4, 20)], 0.8)),
            agent: Some(frame(&[(1, 60), (2, 60), (3, 60), (4, 60)], 1.0)),
            alerts: Some(frame(&[(2, 99)], 1.0)),
            reminders: Some(frame(&[(3, 7)], 1.0)),
            nudges: Some(frame(&[(4, 50)], 1.0)),
            ..Layers::default()
        };
        assert_eq!(
            compose(None, &layers),
            Some(vec![(1, [60; 3], 1.0), (2, [99; 3], 1.0), (3, [7; 3], 1.0), (4, [50; 3], 1.0)])
        );
        // Alone it lights only its keys; the Lighting page's preview covers it.
        let alone = Layers { agent: Some(agent_some.clone()), ..Layers::default() };
        assert!(alone.any());
        assert_eq!(compose(None, &alone), Some(vec![(3, [60; 3], 1.0)]));
        assert_eq!(compose(Some(&frame(&[(5, 1)], 1.0)), &alone), Some(vec![(5, [1; 3], 1.0)]));
    }

    #[test]
    fn a_game_that_wants_quiet_hides_the_agents_unless_the_user_lets_them_show() {
        let layers = Layers {
            game: Some(frame(&[(1, 30), (2, 30)], 1.0)),
            agent: Some(frame(&[(1, 60), (2, 60)], 1.0)),
            reminders: Some(frame(&[(2, 7)], 1.0)),
            ..Layers::default()
        };
        let shown = |quiet, agent_in_quiet| compose(None, &quiet_layers(&layers, quiet, false, agent_in_quiet));
        // "Hide while playing" on (the default): gone, the meeting's key stays.
        assert_eq!(shown(Quiet::All, false), None);
        assert_eq!(shown(Quiet::ButReminders, false), Some(vec![(2, [7; 3], 1.0)]));
        // Off: the agent shows (the game's own lights still wait), under the
        // meeting's key.
        assert_eq!(shown(Quiet::All, true), Some(vec![(1, [60; 3], 1.0), (2, [60; 3], 1.0)]));
        assert_eq!(shown(Quiet::ButReminders, true), Some(vec![(1, [60; 3], 1.0), (2, [7; 3], 1.0)]));
        // Not quiet: the flag changes nothing.
        assert_eq!(compose(None, &layers), Some(vec![(1, [60; 3], 1.0), (2, [7; 3], 1.0)]));
    }

    fn layer(layer: LightLayer, v: u8) -> Request {
        Request::SetLayer(layer, Some(frame(&[(1, v)], 1.0)))
    }

    fn upload(id: &str) -> Request {
        let pages = vec![(kb_protocol::sonix::screen::Frame::filled(0), 10)];
        let anim = ScreenAnimation::new(pages).expect("one frame");
        Request::UploadScreen { id: id.into(), anim: Box::new(anim), reply: mpsc::channel().0 }
    }

    fn per_key(brightness: u8) -> Request {
        Request::ApplyPerKey { colors: Vec::new(), brightness, reply: mpsc::channel().0 }
    }

    /// What a request is, to compare queues (requests hold replies).
    fn label(r: &Request) -> String {
        match r {
            Request::SetLayer(l, Some(f)) => format!("{l:?}={}", f.colors[0].1[0]),
            Request::SetLayer(l, None) => format!("{l:?}=off"),
            Request::SetLive(Some(f)) => format!("live={}", f.colors[0].1[0]),
            Request::SetLive(None) => "live=off".into(),
            Request::SetQuiet(q) => format!("quiet={q:?}"),
            Request::SetGameTried(on) => format!("tried={on}"),
            Request::SetAgentInQuiet(on) => format!("agent-in-quiet={on}"),
            Request::SetClockAuto(on, _) => format!("clock-auto={on}"),
            Request::SyncClock(_) => "sync-clock".into(),
            Request::UploadScreen { id, .. } => format!("upload {id}"),
            Request::ApplyPerKey { brightness, .. } => format!("per-key {brightness}"),
            _ => "other".into(),
        }
    }

    /// The turns `requests` are handled in, if they were all waiting.
    fn turns(requests: Vec<Request>) -> Vec<Vec<String>> {
        let mut waiting: VecDeque<Request> = requests.into();
        let mut out = Vec::new();
        while !waiting.is_empty() {
            out.push(next_batch(&mut waiting).iter().map(label).collect());
        }
        out
    }

    #[test]
    fn piled_up_frames_apply_only_the_newest() {
        // Music frames that came in during a screen upload, and a game
        // asking for quiet in the middle of them: one turn, and the quiet
        // is not lost.
        let mut queue: Vec<Request> = (0..100).map(|v| layer(LightLayer::Music, v)).collect();
        queue.insert(50, Request::SetQuiet(Quiet::All));
        assert_eq!(turns(queue), [["quiet=All", "Music=99"]]);
    }

    #[test]
    fn each_layer_and_the_preview_keep_their_newest_in_order() {
        let preview = |v| Request::SetLive(Some(frame(&[(1, v)], 1.0)));
        let queue = vec![
            layer(LightLayer::Music, 1),
            layer(LightLayer::Game, 1),
            preview(1),
            layer(LightLayer::Music, 2),
            Request::SetGameTried(true),
            layer(LightLayer::Game, 2),
            Request::SetLive(None),
            Request::SetClockAuto(true, mpsc::channel().0),
            layer(LightLayer::Gauges, 1),
        ];
        assert_eq!(turns(queue), [["Music=2", "tried=true", "Game=2", "live=off", "clock-auto=true", "Gauges=1"]]);
    }

    #[test]
    fn taking_a_layer_away_is_a_frame_too() {
        use LightLayer::{Game, Music};
        // The newest wins, whether it sets a frame or takes the layer away:
        // a frame from before the clear never lands after it.
        let queue = vec![layer(Music, 1), layer(Music, 2), Request::SetLayer(Music, None)];
        assert_eq!(turns(queue), [["Music=off"]]);
        let queue = vec![Request::SetLayer(Music, None), layer(Music, 3)];
        assert_eq!(turns(queue), [["Music=3"]]);
        // Another layer's clear leaves this one's frames alone.
        let queue = vec![layer(Music, 1), Request::SetLayer(Game, None), layer(Music, 2)];
        assert_eq!(turns(queue), [["Game=off", "Music=2"]]);
    }

    #[test]
    fn frames_never_cross_a_request_that_uses_the_keyboard() {
        use LightLayer::Music;
        let queue = vec![
            layer(Music, 1),
            layer(Music, 2),
            upload("song card"),
            layer(Music, 3),
            layer(Music, 4),
            Request::SetLayer(Music, None),
            Request::SyncClock(mpsc::channel().0),
            per_key(3),
            layer(Music, 5),
            per_key(4),
        ];
        assert_eq!(
            turns(queue),
            vec![
                vec!["Music=2"],
                vec!["upload song card"],
                vec!["Music=off"],
                vec!["sync-clock"],
                vec!["per-key 3"],
                vec!["Music=5"],
                vec!["per-key 4"],
            ]
        );
    }

    /// The state requests set, as the actor holds it.
    #[derive(Default)]
    struct Held {
        live: Option<LiveFrame>,
        layers: Layers,
        quiet: Quiet,
        tried: bool,
        agent_in_quiet: bool,
    }

    impl Held {
        fn apply(&mut self, r: &Request) {
            match r {
                Request::SetLive(f) => self.live = f.clone(),
                Request::SetLayer(l, f) => *self.layers.slot(*l) = f.clone(),
                Request::SetQuiet(q) => self.quiet = *q,
                Request::SetGameTried(on) => self.tried = *on,
                Request::SetAgentInQuiet(on) => self.agent_in_quiet = *on,
                _ => {}
            }
        }

        /// What the keys show (as `send_live` composes it).
        fn shown(&self) -> Option<Vec<LitKey>> {
            if self.quiet == Quiet::Off {
                compose(self.live.as_ref(), &self.layers)
            } else {
                compose(self.live.as_ref(), &quiet_layers(&self.layers, self.quiet, self.tried, self.agent_in_quiet))
            }
        }
    }

    /// A made-up stream of requests, mostly music frames (the same for the
    /// same seed).
    fn random_requests(seed: u64, count: usize) -> Vec<Request> {
        use LightLayer::{Agent, Game, Gauges, Music, Timer};
        let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut next = move |n: u64| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x % n
        };
        (0..count)
            .map(|i| {
                let v = (i % 250) as u8;
                let pick = [Music, Game, Gauges, Timer, Agent][next(5) as usize];
                match next(17) {
                    0..=7 => layer(Music, v),
                    8 | 9 => layer(pick, v),
                    10 => Request::SetLayer(pick, None),
                    11 => Request::SetLive((next(2) == 0).then(|| frame(&[(1, v)], 1.0))),
                    12 => Request::SetQuiet([Quiet::Off, Quiet::ButReminders, Quiet::All][next(3) as usize]),
                    13 => Request::SetGameTried(next(2) == 0),
                    14 => Request::SyncClock(mpsc::channel().0),
                    15 => Request::SetAgentInQuiet(next(2) == 0),
                    _ => upload("x"),
                }
            })
            .collect()
    }

    #[test]
    fn batched_turns_show_what_one_at_a_time_would() {
        // What the keys show going into each request that uses the
        // keyboard, and at the end: handled one at a time, and in turns.
        for seed in 1..=25 {
            let mut held = Held::default();
            let mut one_at_a_time = Vec::new();
            for r in random_requests(seed, 400) {
                if r.uses_keyboard() {
                    one_at_a_time.push(held.shown());
                }
                held.apply(&r);
            }
            one_at_a_time.push(held.shown());

            let mut held = Held::default();
            let mut batched = Vec::new();
            let mut handled = 0;
            let mut waiting: VecDeque<Request> = random_requests(seed, 400).into();
            while !waiting.is_empty() {
                for r in next_batch(&mut waiting) {
                    handled += 1;
                    if r.uses_keyboard() {
                        batched.push(held.shown());
                    }
                    held.apply(&r);
                }
            }
            batched.push(held.shown());
            assert_eq!(one_at_a_time, batched, "seed {seed}");
            assert!(handled < 350, "seed {seed}: {handled} of 400 handled");
        }
    }
}
