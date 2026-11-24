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

use crate::aa::{backups, LiveContext, ReadOnly};
use crate::driver::{self, Driver, OpenTarget, Ports};
use crate::dto::{
    Battery, DeviceReport, FlashWrite, KeyboardBackupInfo, KeyboardBackupReason, KeyboardInfo, KeyboardRestoreResult,
    LedColor, UnsupportedKeyboard,
};
use crate::settings::{
    self, ClockSettings, FixedMacro, Keymaps, MacroRecord, StoredKeymap, StoredLayer, StoredMacro, StoredMacros,
    StoredStep,
};
use kb_core::aa::{AaEffect, EffectColor};
use kb_core::color::LedCalibration;
use kb_core::conflicts::Conflict;
use kb_core::detect::BlePeer;
use kb_devices::{ConnectionKind, ModeSpec, Model, Protocol};
use kb_protocol::sonix::keymap::{KeyAction, Layer};
use kb_protocol::sonix::screen::ScreenAnimation;
use kb_protocol::sonix::{EffectSettings, Rgb};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
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
/// An "AA" keyboard's keymap or macros, as the app's records hold them,
/// changed to follow the keyboard (device.rs `sync_aa_records`): the UI
/// reads the records again.
pub const EVENT_RECORDS_CHANGED: &str = "keyboard-records-changed";
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
/// A keyboard backup that is not of the keyboard on the cable (another
/// keyboard, another firmware, or none there).
pub const ERR_BACKUP_OTHER_KEYBOARD: &str = "the backup is not of the keyboard on the USB cable";
/// [`DeviceHandle::call`] gave up waiting: the request may still run.
pub const ERR_NO_ANSWER_IN_TIME: &str = "keyboard did not answer in time";

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
    /// An "AA" keyboard whose answers do not look as expected (the sanity
    /// check, or a full read that did not finish): read-only.
    NotSane(Vec<String>),
    /// An "AA" keyboard waiting for a firmware update: read-only.
    UpdatePending,
    /// The backup of an "AA" keyboard could not be saved: read-only.
    NoBackup(String),
}

impl std::fmt::Display for WriteBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SeveralKeyboards => write!(f, "more than one keyboard on the USB cable"),
            Self::Program(c) => write!(f, "another program is using the keyboard ({})", c.id()),
            Self::UnverifiedFirmware(v) => write!(f, "untested keyboard firmware {v}"),
            // The same texts the "AA" arm refuses with (aa.rs `ReadOnly`).
            Self::NotSane(why) => ReadOnly::NotSane(why.clone()).fmt(f),
            Self::UpdatePending => ReadOnly::UpdatePending.fmt(f),
            Self::NoBackup(e) => ReadOnly::NoBackup(e.clone()).fmt(f),
        }
    }
}

impl From<ReadOnly> for WriteBlock {
    fn from(r: ReadOnly) -> Self {
        match r {
            ReadOnly::NotSane(why) => Self::NotSane(why),
            ReadOnly::UpdatePending => Self::UpdatePending,
            ReadOnly::NoBackup(e) => Self::NoBackup(e),
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

/// [`write_gate`] for a keyboard that may be read-only (an "AA" keyboard,
/// aa.rs `ReadOnly`): such a keyboard takes no memory write and no clock,
/// whatever else holds. Several keyboards on the cable are still reported
/// first; live frames are the "AA" arm's own business (it does not switch
/// a read-only keyboard to per-key mode, and says so).
pub fn write_gate_for(
    kind: WriteKind,
    wired: usize,
    read_only: Option<ReadOnly>,
    firmware: Option<FirmwareCheck>,
    running: impl FnOnce() -> Option<Conflict>,
) -> Option<WriteBlock> {
    if kind == WriteKind::Memory && wired > 1 {
        return Some(WriteBlock::SeveralKeyboards);
    }
    match read_only {
        Some(r) if kind != WriteKind::Live => Some(r.into()),
        _ => write_gate(kind, wired, firmware, running),
    }
}

/// Log a refused write, and for an untested firmware tell the UI, so it can
/// ask the user even when the write came from the background (the live
/// mode, the clock sync on connect, live lights on an "AA" keyboard).
fn note_block(app: &Ui, kind: WriteKind, block: &WriteBlock, wired: usize) {
    match block {
        WriteBlock::UnverifiedFirmware(v) => {
            tracing::warn!(firmware = %v, ?kind, "write waits for the user: firmware not tested");
            let _ = app.emit(EVENT_FIRMWARE_UNVERIFIED, v);
        }
        WriteBlock::SeveralKeyboards => tracing::warn!(keyboards = wired, "write refused: several keyboards"),
        WriteBlock::Program(_) => {}
        WriteBlock::NotSane(_) | WriteBlock::UpdatePending | WriteBlock::NoBackup(_) => {
            tracing::info!(?kind, "write refused: {block}");
        }
    }
}

/// The effect the Lighting page's record (`lighting.json` in `dir`) says
/// the app last stored, calibrated, in an "AA" keyboard's values.
/// An "AA" keyboard's macros as the app's record keeps them: its steps,
/// with the name the record had for the same steps in the same slot (the
/// keyboard stores no names); the macros the steps cannot express as kept.
fn macros_from_keyboard(slots: &[crate::aa::MacroContents], old: &StoredMacros, now: u64) -> StoredMacros {
    use crate::aa::MacroContents;
    use kb_protocol::aa::macros::from_steps;
    let mut out = StoredMacros { written_ms: old.written_ms, read_ms: Some(now), ..Default::default() };
    for (i, slot) in slots.iter().enumerate() {
        let m = match slot {
            MacroContents::Steps(steps) => {
                let before = old.slots.get(i).and_then(Option::as_ref);
                let same = before.is_some_and(|b| {
                    let steps_before: Vec<_> = b.steps.iter().map(|s| s.0).collect();
                    from_steps(&steps_before).ok() == from_steps(steps).ok()
                });
                Some(StoredMacro {
                    name: if same { before.map(|b| b.name.clone()).unwrap_or_default() } else { String::new() },
                    steps: steps.iter().map(|s| StoredStep(*s)).collect(),
                })
            }
            MacroContents::Fixed(entries) => {
                out.fixed.push(FixedMacro { slot: i as u8, entries: (*entries).min(u16::MAX as usize) as u16 });
                None
            }
            MacroContents::Empty => None,
        };
        out.slots.push(m);
    }
    while out.slots.last().is_some_and(Option::is_none) {
        out.slots.pop();
    }
    out
}

fn app_effect_in(dir: &std::path::Path, calibration: LedCalibration) -> Option<AaEffect> {
    let state = crate::lighting::load(dir)?;
    let request: crate::dto::EffectRequest = serde_json::from_value(state.get("appliedEffect")?.clone()).ok()?;
    let mut settings = request.to_settings().ok()?;
    settings.color = calibration.apply(settings.color);
    AaEffect::from_settings(&settings).ok()
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
    /// Replies with the model's updated record. `base`: the record's
    /// `source` the change was made against ("AA": refused when the keys
    /// changed on the keyboard since; `None`: not checked).
    ApplyKeymap {
        layer: Layer,
        entries: Vec<(u8, KeyAction)>,
        base: Option<String>,
        reply: Reply<StoredKeymap>,
    },
    /// Write every macro of the USB keyboard (flash) and record them with
    /// their names. Replies with the model's updated record. `base`: as
    /// for [`Request::ApplyKeymap`].
    ApplyMacros {
        slots: Vec<Option<StoredMacro>>,
        base: Option<String>,
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
    /// Whether a write to the keyboard's memory would be refused now, and
    /// why ([`write_gate`]), for a caller that answers with the reason (an
    /// agent's `profile apply`). Reads only what the actor holds.
    WriteBlocked(Reply<Option<WriteBlock>>),
    /// Write a keyboard backup back to the "AA" keyboard on the cable
    /// (flash). `color`: the effect's colour when the backup does not know
    /// it (UI colour and rainbow flag).
    RestoreKeyboard {
        file: String,
        color: Option<(Rgb, bool)>,
        reply: Reply<KeyboardRestoreResult>,
    },
    /// The app is closing: live lights give the keyboard its own effect
    /// back now ("AA"); replies when done.
    Closing(mpsc::Sender<()>),
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
            | Self::UploadScreen { .. }
            | Self::RestoreKeyboard { .. }
            | Self::Closing(_) => true,
            Self::Refresh
            | Self::SetLive(_)
            | Self::SetLayer(..)
            | Self::SetQuiet(_)
            | Self::SetGameTried(_)
            | Self::SetAgentInQuiet(_)
            | Self::SetMirror(_)
            | Self::SetCalibration(_)
            | Self::ConfirmFirmware(_)
            | Self::SetClockAuto(..)
            | Self::WriteBlocked(_) => false,
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
    /// Record id of the keyboard written to (`DetectedKeyboard::record_id`).
    pub record_id: Option<String>,
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
pub(crate) fn quiet_layers(l: &Layers, quiet: Quiet, game_tried: bool, agent_in_quiet: bool) -> Layers {
    let reminders = quiet == Quiet::ButReminders;
    Layers {
        game: l.game.clone().filter(|_| game_tried),
        agent: l.agent.clone().filter(|_| agent_in_quiet),
        reminders: l.reminders.clone().filter(|_| reminders),
        nudges: l.nudges.clone().filter(|_| reminders),
        ..Layers::default()
    }
}

/// Whether a layer shows: any while no game wants quiet, else one of those
/// [`quiet_layers`] lets through (without copying them: the run loop asks
/// often). The actor streams frames while one does.
fn shows_a_layer(l: &Layers, quiet: Quiet, game_tried: bool, agent_in_quiet: bool) -> bool {
    let reminders = quiet == Quiet::ButReminders && (l.reminders.is_some() || l.nudges.is_some());
    match quiet {
        Quiet::Off => l.any(),
        _ => reminders || (game_tried && l.game.is_some()) || (agent_in_quiet && l.agent.is_some()),
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
    /// The diagnostics report's section about the last "AA" keyboard on
    /// the cable (aa.rs `AaDriver::diagnostics`); empty when none was.
    pub aa_diagnostics: Vec<String>,
    /// The USB ids of the "AA" keyboard on the cable: which backups match
    /// it ([`list_keyboard_backups`], read without asking the actor).
    pub aa_usb: Option<kb_core::aa::UsbIdentity>,
}

/// The "AA" keyboard backups in `dir`, oldest first; those of the keyboard
/// with `usb` marked as restorable. Only the files are read.
pub fn list_keyboard_backups(dir: &std::path::Path, usb: Option<kb_core::aa::UsbIdentity>) -> Vec<KeyboardBackupInfo> {
    backups::list(dir)
        .into_iter()
        .map(|(file, b)| KeyboardBackupInfo {
            file,
            saved_at_ms: b.saved_at_ms,
            reason: match b.reason {
                backups::Reason::FirstConnect => KeyboardBackupReason::FirstConnect,
                backups::Reason::BeforeRestore => KeyboardBackupReason::BeforeRestore,
            },
            record_id: b.record_id.clone(),
            firmware: kb_devices::firmware_text(b.release),
            matches: usb == Some(b.usb()),
            color_known: b.snapshot.effect_color.is_some(),
        })
        .collect()
}

/// One step of a screen upload, in order (uploads run one at a time). The
/// screen library uses it to keep an "incomplete" mark on disk while the
/// GIF page is being written, so a crash or a quit midway shows up on the
/// next start, and to refuse an upload that would cost the keyboard its
/// clock page unasked (plan K-13): every upload passes here, whoever asked
/// for it (a library item, a live card, the screen going back to the own
/// picture).
pub enum ScreenWriteStep<'a> {
    /// The upload session is about to open; nothing is written yet, and an
    /// `Err` refuses it (nothing is written, no [`Self::Ended`] follows).
    /// `label` is the request's id (a library item, or `live:<source>`);
    /// `record_id` the keyboard's record id.
    Starting { label: &'a str, record_id: Option<&'a str> },
    /// It ended: `ok` finished, else failed after (`flash_written`) or
    /// before touching the flash.
    Ended { flash_written: bool, ok: bool },
}

pub type ScreenWriteHook = Box<dyn Fn(ScreenWriteStep<'_>) -> Result<(), String> + Send>;

/// The flash write in progress, shared by the actor and the close guard.
/// While one runs the UI is told (`flash-write`) and Windows is asked to
/// hold a shutdown with a reason.
#[derive(Clone)]
pub struct FlashState {
    /// 0: none, else a [`FlashWrite`] code.
    kind: Arc<AtomicU8>,
    reason: Arc<Mutex<String>>,
    app: Ui,
}

/// Where the actor's events go: the app's windows, or nowhere (the tests
/// run the actor without a window). Behind a trait object, so the tests'
/// binary does not link Tauri's window code (which would need the app's
/// Windows manifest to start).
#[derive(Clone)]
struct Ui(Option<Arc<dyn UiSink>>);

/// What the actor does with the app's windows.
trait UiSink: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String>;
    /// `ShutdownBlockReasonCreate` (with a reason) or `…Destroy` on the
    /// main window.
    fn hold_shutdown(&self, reason: Option<String>);
}

impl UiSink for AppHandle {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        Emitter::emit(self, event, payload).map_err(|e| e.to_string())
    }

    /// Windows takes these only from the window's own thread.
    fn hold_shutdown(&self, reason: Option<String>) {
        #[cfg(windows)]
        {
            use tauri::Manager;
            let app = self.clone();
            let r = self.run_on_main_thread(move || {
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

impl Ui {
    fn app(app: AppHandle) -> Self {
        Self(Some(Arc::new(app)))
    }

    fn emit<S: serde::Serialize>(&self, event: &str, payload: S) -> Result<(), String> {
        match &self.0 {
            Some(sink) => sink.emit(event, serde_json::to_value(payload).map_err(|e| e.to_string())?),
            None => Ok(()),
        }
    }
}

impl FlashState {
    fn new(app: Ui) -> Self {
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

    fn hold_shutdown(&self, reason: Option<String>) {
        if let Some(sink) = &self.app.0 {
            sink.hold_shutdown(reason);
        }
    }
}

/// Ends the flash write when dropped (also on an early return).
struct FlashWriteGuard(FlashState);

impl Drop for FlashWriteGuard {
    fn drop(&mut self) {
        self.0.end();
    }
}

/// Cancelling screen uploads, each by its id: the one that runs stops
/// before its next part ("AA"; "04 xx" cannot stop and finishes), one still
/// waiting its turn does not start. A cancel never reaches another upload.
#[derive(Default)]
pub struct UploadCancels(Mutex<UploadQueue>);

#[derive(Default)]
struct UploadQueue {
    /// The upload that runs, and its stop flag.
    running: Option<(String, Arc<AtomicBool>)>,
    /// Uploads sent to the actor and not started yet; `true`: cancelled.
    waiting: Vec<(String, bool)>,
}

impl UploadCancels {
    fn lock(&self) -> std::sync::MutexGuard<'_, UploadQueue> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// An upload with this id was sent to the actor.
    fn queued(&self, id: &str) {
        self.lock().waiting.push((id.to_string(), false));
    }

    /// It will not start after all (the actor did not take it).
    fn unqueued(&self, id: &str) {
        let mut q = self.lock();
        if let Some(i) = q.waiting.iter().position(|(w, _)| w == id) {
            q.waiting.remove(i);
        }
    }

    /// Stop the upload with this id: the one that runs, else the first one
    /// waiting. Nothing else.
    pub fn cancel(&self, id: &str) -> bool {
        let mut q = self.lock();
        if let Some((running, stop)) = &q.running {
            if running == id {
                stop.store(true, Ordering::SeqCst);
                return true;
            }
        }
        match q.waiting.iter_mut().find(|(w, cancelled)| w == id && !*cancelled) {
            Some((_, cancelled)) => {
                *cancelled = true;
                true
            }
            None => false,
        }
    }

    /// The actor starts the upload with this id: its stop flag, or `None`
    /// when it was cancelled while it waited.
    fn start(&self, id: &str) -> Option<Arc<AtomicBool>> {
        let mut q = self.lock();
        let cancelled = match q.waiting.iter().position(|(w, _)| w == id) {
            Some(i) => q.waiting.remove(i).1,
            None => false,
        };
        if cancelled {
            return None;
        }
        let stop = Arc::new(AtomicBool::new(false));
        q.running = Some((id.to_string(), stop.clone()));
        Some(stop)
    }

    fn ended(&self) {
        self.lock().running = None;
    }
}

/// A screen upload stopped before it began.
pub const ERR_UPLOAD_CANCELLED: &str = "screen upload cancelled before it started";

/// How often background work (live frames, the mirror, device reads) opens
/// an "AA" keyboard again after its channel failed: every open reads the
/// whole keyboard, and a stalling one would keep the actor busy.
const AA_REOPEN_BACKOFF: Duration = Duration::from_secs(30);

/// Background work waits before opening a failed "AA" channel again.
const ERR_AA_REOPEN_LATER: &str = "the keyboard did not answer when it was opened; it is tried again later";

/// A restore found no keyboard it could read on the cable.
pub const ERR_KEYBOARD_NOT_READ: &str = "the keyboard on the USB cable could not be read";

pub struct DeviceHandle {
    pub snapshot: Arc<Mutex<Snapshot>>,
    /// The flash write in progress (close guard, shutdown hold).
    pub flash: FlashState,
    tx: Sender<Request>,
    uploads: Arc<UploadCancels>,
    /// An "AA" keyboard is held in per-key mode by live lights (or a crash
    /// note waits): closing has something to put back.
    aa_hold: Arc<AtomicBool>,
    /// The data folder (keyboard backups).
    config_dir: Option<std::path::PathBuf>,
}

/// How long a keyboard restore may wait for its turn and run: behind a
/// screen upload it waits minutes, and it is not reported as failed while
/// it still may run.
pub const RESTORE_WAIT: Duration = Duration::from_secs(15 * 60);

impl DeviceHandle {
    /// The "AA" keyboard backups in the data folder, oldest first, read
    /// without the actor (which may be busy with an upload for minutes):
    /// `matches` from the keyboard the last look found on the cable.
    pub fn keyboard_backups(&self) -> Vec<KeyboardBackupInfo> {
        let Some(dir) = self.config_dir.as_deref().map(backups::dir) else { return Vec::new() };
        let usb = self.snapshot.lock().ok().and_then(|s| s.aa_usb);
        list_keyboard_backups(&dir, usb)
    }

    pub fn send(&self, r: Request) -> Result<(), String> {
        self.tx.send(r).map_err(|_| "device actor stopped".to_string())
    }

    /// Stop the screen upload with this id, running or waiting; whether
    /// there was one.
    pub fn cancel_upload(&self, id: &str) -> bool {
        self.uploads.cancel(id)
    }

    /// The record id of the keyboard on the cable that takes writes: the
    /// one a screen upload goes to.
    pub fn usb_record(&self) -> Option<String> {
        let snap = self.snapshot.lock().ok()?;
        snap.keyboards
            .iter()
            .find(|k| k.connection == crate::dto::Connection::Usb && k.has_command_channel)
            .map(|k| k.record_id.clone())
    }

    /// The app is closing: give the keyboard its own effect back if live
    /// lights hold it ("AA"), waiting at most `wait`. Nothing to put back
    /// ("04 xx", no keyboard, nothing held): returns at once.
    pub fn closing(&self, wait: Duration) {
        if !self.aa_hold.load(Ordering::SeqCst) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        if self.send(Request::Closing(tx)).is_ok() && rx.recv_timeout(wait).is_err() {
            tracing::warn!("closing: the keyboard did not finish in time");
        }
    }

    /// Send a request carrying a reply channel and wait for the answer.
    pub fn call<T>(&self, make: impl FnOnce(Reply<T>) -> Request) -> Result<T, String> {
        self.call_waiting(Duration::from_secs(20), make)
    }

    /// [`Self::call`], waiting up to `wait`: for a request that may queue
    /// behind a screen upload (minutes) and must not be reported as failed
    /// while it still waits its turn (a keyboard restore).
    pub fn call_waiting<T>(&self, wait: Duration, make: impl FnOnce(Reply<T>) -> Request) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        self.send(make(tx))?;
        rx.recv_timeout(wait).map_err(|_| ERR_NO_ANSWER_IN_TIME.to_string())?
    }

    /// Upload to the screen and wait (long) for the outcome.
    pub fn upload_screen(&self, id: String, anim: ScreenAnimation) -> ScreenUploadResult {
        let (tx, rx) = mpsc::channel();
        self.uploads.queued(&id);
        let label = id.clone();
        if let Err(e) = self.send(Request::UploadScreen { id, anim: Box::new(anim), reply: tx }) {
            self.uploads.unqueued(&label);
            return ScreenUploadResult { record_id: None, flash_written: false, error: Some(e) };
        }
        rx.recv_timeout(SCREEN_UPLOAD_TIMEOUT).unwrap_or_else(|_| ScreenUploadResult {
            record_id: None,
            flash_written: true,
            error: Some("keyboard did not finish the upload in time".into()),
        })
    }
}

/// Start the actor. `config_dir` is where clock sync history and the keymap
/// record are saved; without it they last only for this run. `screen_hook`
/// hears each screen upload start and end. `ports` makes, on the actor's
/// thread, where keyboards come from (the HID subsystem, or the simulator
/// in a debug build).
pub fn spawn(
    app: AppHandle,
    calibration: LedCalibration,
    config_dir: Option<PathBuf>,
    screen_hook: ScreenWriteHook,
    ports: Box<dyn FnOnce() -> Ports + Send>,
) -> DeviceHandle {
    let flash = FlashState::new(Ui::app(app.clone()));
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
        aa_diagnostics: Vec::new(),
        aa_usb: None,
    }));
    let (tx, rx) = mpsc::channel();
    let shared = snapshot.clone();
    let actor_flash = flash.clone();
    let uploads = Arc::new(UploadCancels::default());
    let aa_hold = Arc::new(AtomicBool::new(false));
    let (actor_uploads, actor_hold, actor_dir) = (uploads.clone(), aa_hold.clone(), config_dir.clone());
    std::thread::Builder::new()
        .name("device-actor".into())
        .spawn(move || {
            let mut actor = Actor::new(Ui::app(app), shared, calibration, clock, keymaps, macros, actor_dir);
            actor.ports = ports();
            if actor.ports.simulated() {
                tracing::warn!("simulator mode: no HID device is listed or opened, no radio is scanned");
            }
            actor.flash = actor_flash;
            actor.screen_hook = screen_hook;
            actor.uploads = actor_uploads;
            actor.aa_hold = actor_hold;
            actor.run(rx)
        })
        .expect("spawn device actor");
    DeviceHandle { snapshot, flash, tx, uploads, aa_hold, config_dir }
}

/// The open command channel of the keyboard on the cable.
struct Command {
    path: String,
    driver: Driver,
    /// A live stream runs on this handle: `Driver::live_starting` went out
    /// and `Driver::live_stopped` has not yet.
    live: bool,
}

/// The keyboard on the cable that takes writes, as detected: its model, the
/// model file's mode it matched (which names its command family) and the
/// id its records are kept under.
#[derive(Debug, Clone)]
struct UsbKeyboard {
    model: &'static Model,
    mode: &'static ModeSpec,
    record_id: String,
}

impl UsbKeyboard {
    fn protocol(&self) -> Protocol {
        self.model.protocol_of(self.mode)
    }
}

struct Actor {
    app: Ui,
    shared: Arc<Mutex<Snapshot>>,
    /// Where keyboards come from: the HID subsystem, or the simulator.
    ports: Ports,
    /// Cancels of screen uploads (DeviceHandle::cancel_upload).
    uploads: Arc<UploadCancels>,
    /// Told to DeviceHandle: an "AA" keyboard has something to put back
    /// when the app closes.
    aa_hold: Arc<AtomicBool>,
    /// The app is closing (`Request::Closing`): no live frame, mirror,
    /// clock sync or driver work any more.
    closed: bool,
    /// An "AA" channel on this path failed: background work opens it again
    /// only after this ([`AA_REOPEN_BACKOFF`]).
    aa_reopen_after: Option<(String, Instant)>,
    /// The driver's own work ([`Driver::next_due`]) is not looked at again
    /// before this (it had nothing it could do).
    driver_rest_until: Option<Instant>,
    /// Looks for a program that talks to the keyboard ([`running_conflict`];
    /// the tests' own).
    programs: fn() -> Option<Conflict>,
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
    /// The keyboard behind `command_path`.
    usb: Option<UsbKeyboard>,
    /// What it last reported about itself, and when (Unix ms; families that
    /// report, driver.rs `family::reports_device`).
    device_report: Option<(DeviceReport, u64)>,
    /// When it was last asked.
    device_read: Option<Instant>,
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
    /// The "AA" keyboard contents the records were last taken from (record
    /// id and [`crate::aa::AaContents::fingerprint`]).
    aa_synced: Option<(String, String)>,
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
        app: Ui,
        shared: Arc<Mutex<Snapshot>>,
        calibration: LedCalibration,
        clock: ClockSettings,
        keymaps: Keymaps,
        macros: MacroRecord,
        config_dir: Option<PathBuf>,
    ) -> Self {
        let now = Instant::now();
        Self {
            flash: FlashState::new(app.clone()),
            screen_hook: Box::new(|_: ScreenWriteStep<'_>| Ok(())),
            app,
            shared,
            // Set by `spawn`.
            ports: Ports::Hid(None),
            uploads: Arc::new(UploadCancels::default()),
            aa_hold: Arc::new(AtomicBool::new(false)),
            closed: false,
            aa_reopen_after: None,
            driver_rest_until: None,
            programs: running_conflict,
            peers: Vec::new(),
            unknown: Vec::new(),
            layouts: HashMap::new(),
            last_ble_scan: None,
            batteries: HashMap::new(),
            last_battery: config_dir.as_deref().map(settings::load_batteries).unwrap_or_default(),
            command_path: None,
            display_path: None,
            usb: None,
            device_report: None,
            device_read: None,
            usb_firmware: None,
            wired: 0,
            firmware_confirmed: None,
            command: None,
            live: None,
            layers: Layers::default(),
            live_failures: 0,
            aa_synced: None,
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
            } else if !self.streaming() {
                self.end_live();
            }
            if self.mirror.is_some() && !self.closed && !self.streaming() && now >= self.next_mirror {
                self.send_mirror();
                self.next_mirror = Instant::now() + MIRROR_EVERY;
            }
            if self.driver_due().is_some_and(|due| now >= due) {
                self.run_driver_due();
            }
            if let Some((due, attempts)) = self.clock_due {
                // Not during a game that wants the keyboard left alone.
                if now >= due && self.quiet == Quiet::Off && !self.closed {
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

            // Before waiting: what this turn did (a switch by a frame, a
            // request's write) is known to the handle while the actor sleeps.
            self.note_hold();
            if waiting.is_empty() {
                let frames = if self.streaming() {
                    Some(self.next_live)
                } else {
                    self.mirror.as_ref().map(|_| self.next_mirror)
                };
                let wake = wake_at(self.next_enumerate, frames, self.clock_due.map(|(due, _)| due), self.quiet);
                let wake = self.driver_due().map_or(wake, |due| wake.min(due));
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

    /// Tell [`DeviceHandle::closing`] whether an "AA" keyboard has
    /// something to put back.
    fn note_hold(&self) {
        let holds = self.command.as_ref().and_then(|c| c.driver.aa()).is_some_and(crate::aa::AaDriver::holds_keyboard);
        self.aa_hold.store(holds, Ordering::SeqCst);
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
                let was_streaming = self.streaming();
                let slot = self.layers.slot(layer);
                match (&frame, &slot) {
                    (None, Some(_)) => tracing::debug!(?layer, "light layer stopped"),
                    (Some(f), None) => tracing::debug!(?layer, leds = f.colors.len(), "light layer started"),
                    _ => {}
                }
                *slot = frame;
                // Once, when the last one goes: a layer a game hides may still
                // get frames 20 times a second.
                if was_streaming && !self.streaming() {
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
                        // Live lights held back by the question try again.
                        if let Some(d) = self.command.as_mut().and_then(|c| c.driver.aa_mut()) {
                            d.retry_live();
                            // The UI stops saying live lights wait for it.
                            self.next_enumerate = Instant::now();
                        }
                        self.driver_rest_until = None;
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
            Request::ApplyKeymap { layer, entries, base, reply } => {
                let _write = self.flash.begin(FlashWrite::Settings);
                let _ = reply.send(self.apply_keymap(layer, entries, base));
            }
            Request::ApplyMacros { slots, base, reply } => {
                let _write = self.flash.begin(FlashWrite::Settings);
                let _ = reply.send(self.apply_macros(slots, base));
            }
            Request::DescribeUnsupported { id, reply } => {
                let _ = reply.send(self.describe_unsupported(&id));
            }
            Request::WriteBlocked(reply) => {
                let _ = reply.send(Ok(self.gate_block(WriteKind::Memory)));
            }
            Request::UploadScreen { id, anim, reply } => {
                let _write = self.flash.begin(FlashWrite::Screen);
                let _ = reply.send(self.upload_screen(&id, &anim));
            }
            Request::RestoreKeyboard { file, color, reply } => {
                let _write = self.flash.begin(FlashWrite::Settings);
                let _ = reply.send(self.restore_keyboard(&file, color));
            }
            Request::Closing(reply) => {
                self.closing();
                let _ = reply.send(());
            }
        }
    }

    /// The "AA" driver of the keyboard on the cable, when its channel is
    /// open: a handle left from another keyboard (another path, another
    /// family) is not this keyboard's, and its state says nothing about it.
    fn aa_driver(&self) -> Option<&crate::aa::AaDriver> {
        let c = self.command.as_ref()?;
        let current = Some(&c.path) == self.command_path.as_ref()
            && self.usb.as_ref().is_some_and(|u| u.protocol() == c.driver.protocol());
        current.then(|| c.driver.aa()).flatten()
    }

    /// Why the keyboard on the cable is read-only, if it is ("AA").
    fn read_only(&self) -> Option<ReadOnly> {
        self.aa_driver().and_then(crate::aa::AaDriver::read_only)
    }

    /// The effect the app last stored (the Lighting page's record),
    /// calibrated, in an "AA" keyboard's values: where the colour of the
    /// keyboard's effect comes from when the keyboard does not report it.
    fn app_effect(&self) -> Option<AaEffect> {
        app_effect_in(self.config_dir.as_deref()?, self.calibration)
    }

    /// [`Self::app_effect`] as a closure that borrows nothing of the actor,
    /// for the "AA" arm to read only when it needs it.
    fn app_effect_source(&self) -> impl Fn() -> Option<AaEffect> + 'static {
        let (dir, calibration) = (self.config_dir.clone(), self.calibration);
        move || app_effect_in(dir.as_deref()?, calibration)
    }

    /// The Memory gate ([`write_gate`]) as a closure that borrows nothing of
    /// the actor, for the "AA" arm to ask right before a write it makes on
    /// its own (live lights, putting the effect back): it lists the running
    /// programs and, on untested firmware, puts the question to the user,
    /// so it is asked only when a write would go. The read-only check is
    /// the arm's own.
    fn memory_gate(&self) -> impl Fn() -> Result<(), String> + 'static {
        let (wired, firmware, programs, app) = (self.wired, self.firmware_check(), self.programs, self.app.clone());
        move || match write_gate(WriteKind::Memory, wired, firmware, programs) {
            None => Ok(()),
            Some(block) => {
                note_block(&app, WriteKind::Memory, &block, wired);
                Err(block.to_string())
            }
        }
    }

    /// When the open driver has work of its own ("AA": the effect back
    /// after live lights or a crash, a read again).
    fn driver_due(&self) -> Option<Instant> {
        if self.closed {
            return None;
        }
        let due = self.command.as_ref()?.driver.next_due()?;
        Some(self.driver_rest_until.map_or(due, |rest| due.max(rest)))
    }

    fn run_driver_due(&mut self) {
        let now = Instant::now();
        let (gate, app_effect) = (self.memory_gate(), self.app_effect_source());
        let ctx = LiveContext { gate: Some(&gate), app_effect: Some(&app_effect), arming: None };
        let before = self.aa_driver().map(crate::aa::AaDriver::status);
        if let Err(e) = self.with_open_background(|c| c.driver.run_due(now, &ctx)) {
            tracing::debug!("driver work not done: {e}");
        }
        // Nothing it could do now: look again in a second at the soonest.
        let still = self.command.as_ref().and_then(|c| c.driver.next_due()).is_some_and(|d| d <= Instant::now());
        self.driver_rest_until = still.then(|| Instant::now() + Duration::from_secs(1));
        if self.aa_driver().map(crate::aa::AaDriver::status) != before {
            self.next_enumerate = Instant::now();
        }
    }

    /// The app is closing: for the rest of the run nothing streams, mirrors
    /// or syncs any more, and the "AA" arm gives the keyboard its own effect
    /// back now if live lights hold it.
    fn closing(&mut self) {
        self.closed = true;
        self.live = None;
        self.layers = Layers::default();
        self.mirror = None;
        self.clock_due = None;
        if self.aa_driver().is_none() {
            return;
        }
        let (gate, app_effect) = (self.memory_gate(), self.app_effect_source());
        let ctx = LiveContext { gate: Some(&gate), app_effect: Some(&app_effect), arming: None };
        if let Err(e) = self.with_open(|c| c.driver.closing(&ctx)) {
            tracing::warn!("closing: the keyboard's effect did not go back: {e}");
        }
    }

    /// The "AA" keyboard backups in the data folder, the ones of the
    /// keyboard on the cable marked.
    /// What [`DeviceHandle::keyboard_backups`] lists: from the snapshot,
    /// not the actor.
    #[cfg(test)]
    fn keyboard_backups(&self) -> Vec<KeyboardBackupInfo> {
        let Some(dir) = self.config_dir.as_deref().map(backups::dir) else { return Vec::new() };
        list_keyboard_backups(&dir, self.shared.lock().unwrap().aa_usb)
    }

    /// The USB ids of the "AA" keyboard on the cable: the open driver's, or
    /// the detected mode's and firmware.
    fn aa_usb(&self) -> Option<kb_core::aa::UsbIdentity> {
        self.aa_driver().map(crate::aa::AaDriver::usb).or_else(|| {
            let k = self.usb.as_ref().filter(|k| k.protocol() == Protocol::AjazzAa)?;
            Some(kb_core::aa::UsbIdentity { pid: k.mode.pid, release: self.usb_firmware? })
        })
    }

    /// On an "AA" keyboard the keyboard, not the app, holds the truth about
    /// keys and macros: the records follow its last read (writes update
    /// it), macro names kept where the steps are the same, so the Key
    /// mapping and Macros pages and the profiles compare against what the
    /// keyboard has. The UI hears when they changed. Nothing happens while
    /// the read is the one the records came from, or before a read passed
    /// the sanity check. `wrote`: Keymotion just wrote to the keyboard (a
    /// save, a restore), so the records go to disk even when only their
    /// names or write times changed.
    fn sync_aa_records(&mut self, wrote: bool) {
        let Some((record, contents)) = self.aa_driver().and_then(|d| Some((d.record_id().to_string(), d.contents()?)))
        else {
            return;
        };
        let key = (record.clone(), contents.fingerprint.clone());
        if self.aa_synced.as_ref() == Some(&key) {
            return;
        }
        self.aa_synced = Some(key);
        let now = now_ms();
        let old_keymap = self.keymaps.models.get(&record).cloned().unwrap_or_default();
        let layer = |c: &crate::aa::LayerContents, old: &StoredLayer| {
            let mut l = StoredLayer::new(&c.actions, 0);
            l.written_ms = old.written_ms;
            l.fixed = c.fixed.clone();
            l.builtin = c.builtin.clone();
            l.pinned = c.pinned.clone();
            l
        };
        let keymap = StoredKeymap {
            base: layer(&contents.base, &old_keymap.base),
            fn_layer: layer(&contents.fn_layer, &old_keymap.fn_layer),
            read_ms: Some(now),
            source: Some(contents.keys_source.clone()),
        };
        let old_macros = self.macros.models.get(&record).cloned().unwrap_or_default();
        let mut macros = macros_from_keyboard(&contents.macros, &old_macros, now);
        macros.source = Some(contents.macros_source.clone());
        let same_keymap = StoredKeymap { read_ms: old_keymap.read_ms, ..keymap.clone() } == old_keymap;
        let same_macros = StoredMacros { read_ms: old_macros.read_ms, ..macros.clone() } == old_macros;
        self.keymaps.models.insert(record.clone(), keymap);
        self.macros.models.insert(record.clone(), macros);
        if let Ok(mut s) = self.shared.lock() {
            s.keymaps = self.keymaps.clone();
            s.macros = self.macros.clone();
        }
        if !wrote && same_keymap && same_macros && old_keymap.read_ms.is_some() {
            return;
        }
        tracing::info!(record = %record, "keymap and macro records follow the \"AA\" keyboard");
        if let Some(dir) = &self.config_dir {
            if let Err(e) = settings::save_keymaps(dir, &self.keymaps) {
                tracing::warn!("keymap record not saved: {e}");
            }
            if let Err(e) = settings::save_macros(dir, &self.macros) {
                tracing::warn!("macro record not saved: {e}");
            }
        }
        let _ = self.app.emit(EVENT_RECORDS_CHANGED, &record);
    }

    /// Write a keyboard backup back. The effect's colour: the backup's,
    /// else `color` (a UI colour, calibrated here), else the app's record
    /// when its effect is the backup's.
    fn restore_keyboard(&mut self, file: &str, color: Option<(Rgb, bool)>) -> Result<KeyboardRestoreResult, String> {
        let dir = self.config_dir.as_deref().map(backups::dir).ok_or("the app has no data folder")?;
        let backup = backups::load(&dir, file)?;
        if self.command_path.is_none() {
            return Err(NO_USB.into());
        }
        if self.aa_driver().is_none() {
            // Opens (and reads) the keyboard on the cable; one that cannot
            // be read says so, not that the backup is another keyboard's.
            self.with_open(|_| Ok(())).map_err(|e| format!("{ERR_KEYBOARD_NOT_READ}: {e}"))?;
        }
        let usb = self.aa_driver().map(crate::aa::AaDriver::usb);
        if usb != Some(backup.usb()) {
            return Err(ERR_BACKUP_OTHER_KEYBOARD.into());
        }
        self.may_write()?;
        let color = color
            .map(|(c, rainbow)| EffectColor { color: self.calibration.apply(c), color_mode: u8::from(rainbow) })
            .or_else(|| {
                let saved = backup.snapshot.effect()?;
                self.app_effect().filter(|a| a.same_settings(&saved)).map(|a| a.color_part())
            });
        let report = self.with_command(|d| d.restore(&backup.snapshot, color, Some(file)))?;
        tracing::info!(written = ?report.written, kept = report.kept.len(), %file, "keyboard backup restored");
        self.next_enumerate = Instant::now();
        // The records follow what was put back; so does the Lighting
        // page's, through the UI (`effect`).
        self.sync_aa_records(true);
        let mut result = KeyboardRestoreResult::from(report);
        result.effect = self.aa_driver().and_then(|d| d.effect_now()).and_then(|e| self.effect_request(&e));
        Ok(result)
    }

    /// An effect the keyboard holds, as the Lighting page's record keeps
    /// one (monitor colour), when its colour is known and it is not the
    /// per-key mode.
    fn effect_request(&self, read: &kb_core::aa::EffectRead) -> Option<crate::dto::EffectRequest> {
        if read.effect.is_live() || !read.color_known {
            return None;
        }
        let s = read.settings();
        let [r, g, b] = self.calibration.inverse(s.color);
        Some(crate::dto::EffectRequest {
            mode: s.mode,
            color: format!("#{r:02x}{g:02x}{b:02x}"),
            rainbow: s.rainbow,
            brightness: s.brightness,
            speed: s.speed,
            direction: s.direction,
        })
    }

    /// Upload to the GIF page: command channel for the session, display
    /// channel (opened just for this) for the pages. Progress is emitted as
    /// `screen-upload-progress`.
    ///
    /// Nothing else reaches the keyboard meanwhile: the actor handles one
    /// request at a time ([`next_batch`] gives an upload a turn of its own),
    /// so live frames, the mirror, the clock sync and device reads wait for
    /// it, and the progress callback gets only numbers.
    fn upload_screen(&mut self, id: &str, anim: &ScreenAnimation) -> ScreenUploadResult {
        let record_id = self.usb.as_ref().map(|k| k.record_id.clone());
        // Cancelled while it waited its turn: it does not start.
        let Some(cancel) = self.uploads.start(id) else {
            tracing::info!(label = id, "screen upload cancelled before it started");
            return ScreenUploadResult { record_id, flash_written: false, error: Some(ERR_UPLOAD_CANCELLED.into()) };
        };
        let r = self.upload_screen_now(id, anim, record_id, &cancel);
        self.uploads.ended();
        r
    }

    fn upload_screen_now(
        &mut self,
        id: &str,
        anim: &ScreenAnimation,
        record_id: Option<String>,
        cancel: &AtomicBool,
    ) -> ScreenUploadResult {
        let fail = |record_id: Option<String>, flash_written: bool, e: String| ScreenUploadResult {
            record_id,
            flash_written,
            error: Some(e),
        };
        let (Some(path), Some(protocol)) = (self.display_path.clone(), self.usb.as_ref().map(UsbKeyboard::protocol))
        else {
            return fail(record_id, false, NO_USB.into());
        };
        if let Err(e) = self.may_write() {
            return fail(record_id, false, e);
        }
        // The clock page guard and the "incomplete" mark (lib.rs
        // `screen_journal`): every upload passes here, and may stop here.
        if let Err(e) = (self.screen_hook)(ScreenWriteStep::Starting { label: id, record_id: record_id.as_deref() }) {
            tracing::info!(label = id, "screen upload refused: {e}");
            return fail(record_id, false, e);
        }
        let mut display = match self.ports.open_display(&path, protocol) {
            Ok(d) => d,
            Err(e) => {
                let _ = (self.screen_hook)(ScreenWriteStep::Ended { flash_written: false, ok: false });
                return fail(record_id, false, e);
            }
        };
        let app = self.app.clone();
        let label = id.to_string();
        let mut last = Instant::now() - SCREEN_PROGRESS_EVERY;
        let mut started = false;
        let started_at = Instant::now();
        let r = self.with_command(|d| {
            d.upload_screen(
                &mut display,
                anim,
                |done, total| {
                    started = true;
                    if done == 1 || done == total || last.elapsed() >= SCREEN_PROGRESS_EVERY {
                        last = Instant::now();
                        let p =
                            crate::dto::ScreenProgress { id: label.clone(), done: done as u32, total: total as u32 };
                        let _ = app.emit(EVENT_SCREEN_PROGRESS, p);
                    }
                },
                cancel,
            )
        });
        let _ = (self.screen_hook)(ScreenWriteStep::Ended { flash_written: r.is_ok() || started, ok: r.is_ok() });
        match r {
            Ok(report) => {
                tracing::info!(
                    pages = report.pages,
                    secs = started_at.elapsed().as_secs(),
                    commit = ?report.commit_echo,
                    "screen upload done"
                );
                ScreenUploadResult { record_id, flash_written: true, error: None }
            }
            // A failure after the first page left the GIF page half written.
            Err(e) => fail(record_id, started, e),
        }
    }

    /// Write every macro, then record them (names included). Like the
    /// keymap, the record changes only after the keyboard confirmed the data.
    fn apply_macros(
        &mut self,
        mut slots: Vec<Option<StoredMacro>>,
        base: Option<String>,
    ) -> Result<StoredMacros, String> {
        let record = self.usb.as_ref().map(|k| k.record_id.clone()).ok_or_else(|| NO_USB.to_string())?;
        self.may_write()?;
        while slots.last().is_some_and(Option::is_none) {
            slots.pop();
        }
        let mut stored = StoredMacros { slots, ..Default::default() };
        let image = stored.image_slots();
        let written = self.with_command(|d| match d.aa_mut() {
            Some(aa) => aa.write_macros_from(&image, base.as_deref()),
            None => d.write_macros(&image),
        });
        if self.aa_driver().is_some() {
            // The keyboard is the record ("AA"): what it holds now, with the
            // names the user just gave. A refused save still brings the
            // record up to the fresh read (and the UI hears it).
            if written.is_ok() {
                tracing::info!(slots = image.len(), record = %record, "macros written");
                stored.written_ms = Some(now_ms());
                self.macros.models.insert(record.clone(), stored.clone());
            }
            self.aa_synced = None;
            self.sync_aa_records(true);
            written?;
            return Ok(self.macros.models.get(&record).cloned().unwrap_or(stored));
        }
        written?;
        tracing::info!(slots = image.len(), record = %record, "macros written");
        stored.written_ms = Some(now_ms());
        self.macros.models.insert(record, stored.clone());
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
    fn apply_keymap(
        &mut self,
        layer: Layer,
        entries: Vec<(u8, KeyAction)>,
        base: Option<String>,
    ) -> Result<StoredKeymap, String> {
        let usb = self.usb.clone().ok_or_else(|| NO_USB.to_string())?;
        let map = kb_core::keymap::canonical(usb.model, layer, entries).map_err(|e| e.to_string())?;
        self.may_write()?;
        let written = self.with_command(|d| match d.aa_mut() {
            Some(aa) => aa.apply_keymap_from(layer, &map, base.as_deref()),
            None => d.apply_keymap(layer, &map).map(|()| 0),
        });
        if self.aa_driver().is_some() {
            // The keyboard is the record ("AA"): what it holds now. A refused
            // save still brings the record up to the fresh read (and the UI
            // hears it).
            if let Ok(kept) = &written {
                tracing::info!(?layer, keys = map.len(), kept, record = %usb.record_id, "keymap layer written");
                if let Some(l) = self.keymaps.models.get_mut(&usb.record_id) {
                    l.layer_mut(layer).written_ms = Some(now_ms());
                }
            }
            self.aa_synced = None;
            self.sync_aa_records(true);
            let kept = written?;
            if let Some(k) = self.keymaps.models.get(&usb.record_id) {
                return Ok(k.clone());
            }
            tracing::debug!(kept, "no keymap record to answer with");
        } else {
            written?;
        }
        tracing::info!(?layer, keys = map.len(), record = %usb.record_id, "keymap layer written");
        let stored = self.keymaps.models.entry(usb.record_id).or_default();
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

    /// The wired keyboard's firmware: tested in the detected mode, or said
    /// yes to this session.
    fn firmware_check(&self) -> Option<FirmwareCheck> {
        self.usb.as_ref().zip(self.usb_firmware).map(|(k, release)| FirmwareCheck {
            release,
            verified: k.mode.firmware_verified(release),
            confirmed: self.firmware_confirmed == Some(release),
        })
    }

    /// Whether a write of `kind` may go now ([`write_gate`]). An untested
    /// firmware also tells the UI, so it can ask the user even when the
    /// write came from the background (the live mode, the clock sync on
    /// connect).
    fn gate(&self, kind: WriteKind) -> Result<(), String> {
        match self.gate_block(kind) {
            Some(block) => Err(block.to_string()),
            None => Ok(()),
        }
    }

    /// [`Self::gate`]'s reason, as it is.
    fn gate_block(&self, kind: WriteKind) -> Option<WriteBlock> {
        // Without a keyboard on the cable the write fails on its own.
        self.command_path.as_ref()?;
        let block = write_gate_for(kind, self.wired, self.read_only(), self.firmware_check(), self.programs)?;
        note_block(&self.app, kind, &block, self.wired);
        Some(block)
    }

    /// Run `f` against the wired command channel's driver, opening it on
    /// demand ([`Self::with_open`]).
    fn with_command<T>(&mut self, f: impl FnOnce(&mut Driver) -> driver::Result<T>) -> Result<T, String> {
        self.with_open(|c| f(&mut c.driver))
    }

    /// Run `f` against the wired command channel, opening it on demand in
    /// the detected keyboard's family (an "AA" keyboard is read whole when
    /// it opens). Any transport error drops the handle so the next call
    /// reopens it. For the user's own requests: an "AA" keyboard whose
    /// channel failed is tried again at once.
    fn with_open<T>(&mut self, f: impl FnOnce(&mut Command) -> driver::Result<T>) -> Result<T, String> {
        self.with_open_as(false, f)
    }

    /// [`Self::with_open`] for the actor's own work (live frames, the
    /// mirror, device reads, the driver's due work): an "AA" keyboard whose
    /// channel failed is opened again at most every [`AA_REOPEN_BACKOFF`].
    fn with_open_background<T>(&mut self, f: impl FnOnce(&mut Command) -> driver::Result<T>) -> Result<T, String> {
        self.with_open_as(true, f)
    }

    fn with_command_background<T>(&mut self, f: impl FnOnce(&mut Driver) -> driver::Result<T>) -> Result<T, String> {
        self.with_open_background(|c| f(&mut c.driver))
    }

    fn with_open_as<T>(
        &mut self,
        background: bool,
        f: impl FnOnce(&mut Command) -> driver::Result<T>,
    ) -> Result<T, String> {
        let path = self.command_path.clone().ok_or_else(|| NO_USB.to_string())?;
        let usb = self.usb.clone().ok_or_else(|| NO_USB.to_string())?;
        let protocol = usb.protocol();
        let aa = protocol == Protocol::AjazzAa;
        if self.command.as_ref().is_none_or(|c| c.path != path || c.driver.protocol() != protocol) {
            let waiting = self.aa_reopen_after.as_ref().is_some_and(|(p, until)| *p == path && Instant::now() < *until);
            if background && aa && waiting {
                return Err(ERR_AA_REOPEN_LATER.into());
            }
            let target = OpenTarget {
                protocol,
                model: usb.model,
                record_id: usb.record_id.clone(),
                data_dir: self.config_dir.clone(),
                app_effect: if protocol == Protocol::AjazzAa { self.app_effect() } else { None },
            };
            // A handle being replaced takes its stream with it.
            self.command = None;
            let driver = match self.ports.open_command(&path, &target) {
                Ok(driver) => driver,
                Err(e) => {
                    if aa {
                        self.aa_reopen_after = Some((path, Instant::now() + AA_REOPEN_BACKOFF));
                    }
                    return Err(e);
                }
            };
            self.aa_reopen_after = None;
            if let Some(d) = driver.aa() {
                tracing::info!(record = %usb.record_id, read_only = ?d.read_only(), "\"AA\" keyboard opened");
                // Show what the read found at once.
                self.next_enumerate = Instant::now();
            }
            self.driver_rest_until = None;
            self.command = Some(Command { path: path.clone(), driver, live: false });
        }
        let cmd = self.command.as_mut().expect("opened above");
        match f(cmd) {
            Ok(v) => Ok(v),
            Err(e) => {
                // The keyboard now and then refuses one real-time frame
                // (`FF`, about 1 in 100); the next one goes 50 ms later.
                // `send_live` warns when they keep failing.
                if e.is_noise() {
                    tracing::debug!("command failed: {e}");
                } else {
                    tracing::warn!("command failed: {e}");
                }
                if e.is_transport() {
                    self.command = None;
                    if aa {
                        self.aa_reopen_after = Some((path, Instant::now() + AA_REOPEN_BACKOFF));
                    }
                }
                Err(e.to_string())
            }
        }
    }

    /// A live frame is being streamed (preview or background layer); never
    /// once the app is closing.
    fn streaming(&self) -> bool {
        !self.closed && (self.live.is_some() || self.has_layer())
    }

    fn has_layer(&self) -> bool {
        shows_a_layer(&self.layers, self.quiet, self.game_tried, self.agent_in_quiet)
    }

    fn send_live(&mut self) {
        if self.command_path.is_none() || self.closed {
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
        // What an "AA" keyboard needs to switch to per-key mode, asked only
        // when it would ("04 xx": never).
        let (gate, app_effect) = (self.memory_gate(), self.app_effect_source());
        // A switch to per-key mode about to be written: closing the app
        // waits for it (and puts the effect back) from now on.
        let hold = self.aa_hold.clone();
        let arming = move || hold.store(true, Ordering::SeqCst);
        let ctx = LiveContext { gate: Some(&gate), app_effect: Some(&app_effect), arming: Some(&arming) };
        let before = self.aa_driver().map(crate::aa::AaDriver::status);
        // The first frame on this handle starts the stream in the family's
        // driver ("04 xx": nothing to do).
        let sent = self.with_open_background(|c| {
            if !c.live {
                c.driver.live_starting(&ctx)?;
                c.live = true;
            }
            c.driver.live_frame(frame, &ctx)
        });
        if self.aa_driver().map(crate::aa::AaDriver::status) != before {
            // Armed, or why not: the UI hears it at once.
            self.next_enumerate = Instant::now();
        }
        match sent {
            Ok(()) => self.live_failures = 0,
            // Refused on purpose during the quiet time after a cut upload.
            Err(e) if self.aa_driver().is_some_and(crate::aa::AaDriver::in_quiet_time) => {
                tracing::debug!("live frame waits for the quiet time: {e}");
            }
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

    /// The live stream ended: tell the family's driver, once, on the handle
    /// the stream ran on ("04 xx": nothing to do). A handle dropped
    /// meanwhile took the stream with it.
    fn end_live(&mut self) {
        let Some(cmd) = self.command.as_mut().filter(|c| c.live) else { return };
        cmd.live = false;
        if let Err(e) = cmd.driver.live_stopped() {
            tracing::warn!("live stream end failed: {e}");
            if e.is_transport() {
                self.command = None;
            }
        }
    }

    fn send_mirror(&mut self) {
        if self.command_path.is_none() {
            return;
        }
        // An "AA" keyboard waiting for an update, silent or in its quiet
        // time after a cut upload is not asked four times a second; its
        // background read asks at its own pace.
        if self.aa_driver().is_some_and(|d| !d.mirror_ready()) {
            return;
        }
        let cal = self.calibration;
        let expect = self.mirror.clone().unwrap_or_default();
        let indicators = self.indicators.clone();
        match self.with_command_background(|d| d.mirror_colors(&expect, cal, &indicators)) {
            Ok(colors) => {
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

    /// What the keyboard on the cable reports about itself, and when it was
    /// read (Unix ms): asked again at most every [`BATTERY_REFRESH`], only
    /// in a family that reports ([`driver::family::reports_device`]; never
    /// "04 xx", so nothing is sent to those). Runs on the actor like every
    /// other request, never during a screen upload.
    fn refresh_device_report(&mut self) -> Option<(DeviceReport, u64)> {
        let protocol = self.usb.as_ref().map(UsbKeyboard::protocol)?;
        if !driver::family::reports_device(protocol) {
            return None;
        }
        if self.device_read.is_none_or(|at| at.elapsed() >= BATTERY_REFRESH) {
            self.device_read = Some(Instant::now());
            match self.with_command_background(|d| d.device_info()) {
                Ok(report) => self.device_report = report.map(|r| (r, now_ms())),
                Err(e) => tracing::debug!("device report not read: {e}"),
            }
        }
        self.device_report.clone()
    }

    fn enumerate(&mut self) {
        // BLE enumeration is slow; skip it while streaming so frames stay smooth.
        let busy = self.streaming() || self.mirror.is_some();
        // The simulator stands for every device: no radio is scanned.
        let radio = !self.ports.simulated();
        if radio && !busy && self.last_ble_scan.is_none_or(|t| t.elapsed() >= BLE_RESCAN) {
            self.peers = scan_ble();
            self.last_ble_scan = Some(Instant::now());
        }

        let interfaces = self.ports.interfaces();
        // The report layouts of the vendor channels on the cable, read once
        // per collection and firmware version (from Windows; nothing is sent
        // to the keyboard). A failed read is tried again next time.
        self.layouts.retain(|(path, _), _| interfaces.iter().any(|i| &i.path == path));
        let (ports, layouts) = (&self.ports, &mut self.layouts);
        let found = kb_core::classify(&interfaces, &self.peers, |iface| {
            let key = (iface.path.clone(), iface.release);
            if let Some(desc) = layouts.get(&key) {
                return Some(desc.clone());
            }
            match ports.report_descriptor_at(&iface.path) {
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
        let usb_keyboard = usb.filter(|k| k.command.is_some()).map(|k| UsbKeyboard {
            model: k.model,
            mode: k.mode,
            record_id: k.record_id(),
        });
        if usb_keyboard.as_ref().map(|k| &k.record_id) != self.usb.as_ref().map(|k| &k.record_id) {
            self.device_report = None;
            self.device_read = None;
        }
        self.usb = usb_keyboard;
        self.usb_firmware = usb.and_then(|k| k.firmware);
        let wired = kb_core::wired_count(&detected);
        if wired != self.wired && wired > 1 {
            tracing::warn!(keyboards = wired, "several keyboards on the cable; writes are refused");
        }
        self.wired = wired;
        self.indicators =
            usb.map(|k| k.model.indicators.iter().map(|i| (i.led, i.color)).collect()).unwrap_or_default();
        // A handle on a path that is gone (another keyboard took the cable,
        // or this one came back on another path) goes with it.
        if self.command.as_ref().is_some_and(|c| Some(&c.path) != self.command_path.as_ref()) {
            self.command = None;
        }
        let device = self.refresh_device_report();

        let mut infos = Vec::with_capacity(detected.len());
        for k in &detected {
            // A keyboard that reports its battery on the cable shows only
            // that there: an older Bluetooth reading is not a cable reading.
            let own_report = k.connection == ConnectionKind::Usb && driver::family::battery_over_usb(k.protocol());
            let battery = match (&k.ble, k.connection) {
                (Some(peer), ConnectionKind::Bluetooth) if radio && peer.connected && !busy => {
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
            };
            let battery =
                if own_report { None } else { battery.or_else(|| self.last_battery.get(&k.model.id).cloned()) };
            // The keyboard on the cable may report about itself (not "04 xx").
            let device = device
                .as_ref()
                .filter(|_| k.connection == ConnectionKind::Usb)
                .filter(|_| self.usb.as_ref().is_some_and(|u| u.record_id == k.record_id()));
            let battery = device.and_then(|(d, at)| Some(Battery { percent: d.battery?, read_at_ms: *at })).or(battery);
            let mut info = KeyboardInfo::from_detected(k, battery, device.map(|(d, _)| d.clone()));
            if k.connection == ConnectionKind::Usb {
                info.aa_status =
                    self.aa_driver().filter(|d| d.record_id() == k.record_id()).map(crate::aa::AaDriver::status);
            }
            infos.push(info);
        }
        if let Some(d) = self.aa_driver() {
            let lines = d.diagnostics();
            if let Ok(mut s) = self.shared.lock() {
                s.aa_diagnostics = lines;
            }
        }
        let aa_usb = self.aa_usb();
        if let Ok(mut s) = self.shared.lock() {
            s.aa_usb = aa_usb;
        }
        self.sync_aa_records(false);

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
        let descriptors: Vec<Result<Vec<u8>, String>> =
            k.paths.iter().map(|p| self.ports.report_descriptor_at(p)).collect();
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

    #[test]
    fn a_layer_shows_exactly_when_quiet_mode_lets_one_through() {
        let f = || Some(frame(&[(1, 9)], 1.0));
        let all = [
            LightLayer::Profile,
            LightLayer::Gauges,
            LightLayer::Music,
            LightLayer::Game,
            LightLayer::Timer,
            LightLayer::Agent,
            LightLayer::Alerts,
            LightLayer::Reminders,
            LightLayer::Nudges,
        ];
        // Every set of layers, every quiet mode and both flags: the actor
        // streams exactly when something would be on the keys.
        for mask in 0..(1u32 << all.len()) {
            let mut layers = Layers::default();
            for (i, l) in all.iter().enumerate() {
                if mask & (1 << i) != 0 {
                    *layers.slot(*l) = f();
                }
            }
            for quiet in [Quiet::Off, Quiet::ButReminders, Quiet::All] {
                for (tried, agent) in [(false, false), (true, false), (false, true), (true, true)] {
                    let shown = if quiet == Quiet::Off {
                        compose(None, &layers)
                    } else {
                        compose(None, &quiet_layers(&layers, quiet, tried, agent))
                    };
                    assert_eq!(
                        shows_a_layer(&layers, quiet, tried, agent),
                        shown.is_some(),
                        "layers {mask:09b}, {quiet:?}, tried {tried}, agent {agent}"
                    );
                }
            }
        }
        // The cases the agents' layer adds, by name.
        let agent = Layers { agent: f(), ..Layers::default() };
        assert!(shows_a_layer(&agent, Quiet::Off, false, false));
        assert!(!shows_a_layer(&agent, Quiet::All, false, false), "hidden while playing");
        assert!(!shows_a_layer(&agent, Quiet::ButReminders, false, false));
        assert!(shows_a_layer(&agent, Quiet::All, false, true), "the user lets it show");
        assert!(shows_a_layer(&agent, Quiet::ButReminders, false, true));
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

    #[test]
    fn a_read_only_keyboard_takes_no_memory_write_and_no_clock() {
        let insane = || Some(ReadOnly::NotSane(vec!["unknown effect mode 99".into()]));
        let tested = Some(FirmwareCheck { release: 0x0301, verified: true, confirmed: false });
        for kind in [WriteKind::Memory, WriteKind::Clock] {
            let block = write_gate_for(kind, 1, insane(), tested, || panic!("programs listed"));
            assert_eq!(block, Some(WriteBlock::NotSane(vec!["unknown effect mode 99".into()])), "{kind:?}");
        }
        assert_eq!(
            WriteBlock::NotSane(vec!["a".into(), "b".into()]).to_string(),
            "the keyboard does not look as expected, so nothing is written: a; b"
        );
        assert_eq!(
            write_gate_for(WriteKind::Clock, 1, Some(ReadOnly::UpdatePending), tested, || None)
                .map(|b| b.to_string())
                .as_deref(),
            Some("the keyboard is waiting for a firmware update; nothing is read or written")
        );
        assert_eq!(
            WriteBlock::NoBackup("disk full".into()).to_string(),
            "no backup of the keyboard could be saved, so nothing is written: disk full"
        );
        // Live frames are the "AA" arm's business (it does not switch a
        // read-only keyboard); several keyboards still come first.
        assert_eq!(write_gate_for(WriteKind::Live, 1, insane(), tested, || None), None);
        assert_eq!(write_gate_for(WriteKind::Memory, 2, insane(), tested, || None), Some(WriteBlock::SeveralKeyboards));
        // A keyboard that is not read-only: the gate as before.
        let untested = Some(FirmwareCheck { release: 0x0301, verified: false, confirmed: false });
        for kind in [WriteKind::Memory, WriteKind::Clock, WriteKind::Live] {
            assert_eq!(
                write_gate_for(kind, 1, None, untested, || None),
                write_gate(kind, 1, untested, || None),
                "{kind:?}"
            );
        }
    }

    /// The actor with the simulated keyboard: everything but the windows.
    mod with_the_simulator {
        use super::*;
        use crate::aa::tests::TempDir;
        use crate::sim::SimBoard;
        use kb_core::aa::sim::Sim;
        use kb_protocol::aa::cmd;
        use std::path::Path;

        const RECORD: &str = "ajazz-ak980-pro.aa";

        fn actor(dir: &Path, faults: &str) -> (Actor, Sim) {
            actor_with(dir, faults, |_| {})
        }

        /// [`actor`], the simulated keyboard set up by `prepare` first (as
        /// another program left it).
        fn actor_with(dir: &Path, faults: &str, prepare: impl FnOnce(&Sim)) -> (Actor, Sim) {
            let board = SimBoard::new("aa769", faults).unwrap();
            let sim = board.sim().clone();
            prepare(&sim);
            let shared = Arc::new(Mutex::new(Snapshot::default()));
            let mut a = Actor::new(
                Ui(None),
                shared,
                LedCalibration::RAW,
                ClockSettings::default(),
                Keymaps::default(),
                MacroRecord::default(),
                Some(dir.to_path_buf()),
            );
            a.ports = Ports::Sim(board);
            a.programs = || None;
            a.enumerate();
            (a, sim)
        }

        fn call<T>(a: &mut Actor, make: impl FnOnce(Reply<T>) -> Request) -> Result<T, String> {
            let (tx, rx) = mpsc::channel();
            a.handle(make(tx));
            rx.try_recv().expect("answered")
        }

        fn upload(a: &mut Actor, id: &str) -> ScreenUploadResult {
            let (tx, rx) = mpsc::channel();
            let anim = ScreenAnimation::still(kb_protocol::sonix::screen::test_pattern());
            a.handle(Request::UploadScreen { id: id.into(), anim: Box::new(anim), reply: tx });
            rx.try_recv().expect("answered")
        }

        fn info(a: &Actor) -> KeyboardInfo {
            let snap = a.shared.lock().unwrap();
            assert_eq!(snap.keyboards.len(), 1, "the simulated keyboard alone");
            snap.keyboards[0].clone()
        }

        fn writes(sim: &Sim) -> usize {
            let changes = [cmd::SET_KEYMAP, cmd::SET_EFFECT, cmd::SET_PER_KEY, cmd::SET_MACROS, cmd::SET_FN_KEYMAP];
            sim.state().packets.iter().filter(|p| changes.contains(&p[2]) || p[2] == cmd::CLOCK).count()
        }

        fn settings() -> EffectSettings {
            EffectSettings { mode: 2, color: [255, 0, 0], rainbow: false, brightness: 4, speed: 2, direction: 0 }
        }

        #[test]
        fn the_keyboard_is_listed_read_and_backed_up_with_its_own_battery() {
            let dir = TempDir::new("actor-listed");
            let (mut a, sim) = actor(&dir.0, "");
            let k = info(&a);
            assert_eq!((k.record_id.as_str(), k.firmware.as_deref()), (RECORD, Some("3.01")));
            let status = k.aa_status.clone().expect("read when it appeared");
            assert_eq!(status.read_only, None);
            assert!(status.backup.as_deref().is_some_and(|f| f.starts_with("aa-8039-0301-")));
            assert_eq!(k.device.as_ref().and_then(|d| d.battery), Some(100));
            assert_eq!(k.battery.as_ref().map(|b| b.percent), Some(100), "the keyboard's own report");
            assert_eq!(writes(&sim), 0, "nothing written by connecting");
            assert!(a.shared.lock().unwrap().aa_diagnostics.iter().any(|l| l == "writes: allowed"));
            let json = serde_json::to_value(&k).unwrap();
            assert_eq!(json["aaStatus"]["readOnly"], serde_json::Value::Null);
            assert_eq!(json["aaStatus"]["liveArmed"], false);
            assert_eq!(json["aaStatus"]["otherProgramSuspected"], false);

            // An older Bluetooth reading never stands in on the cable.
            a.last_battery.insert("ajazz-ak980-pro".into(), Battery { percent: 12, read_at_ms: 1 });
            a.enumerate();
            assert_eq!(info(&a).battery.map(|b| b.percent), Some(100));
            let dir = TempDir::new("actor-silent");
            let (mut a, _) = actor(&dir.0, "silent");
            a.last_battery.insert("ajazz-ak980-pro".into(), Battery { percent: 12, read_at_ms: 1 });
            a.enumerate();
            let k = info(&a);
            assert_eq!(k.battery, None, "no report: no battery, not the old reading");
            assert!(matches!(k.aa_status.unwrap().read_only, Some(crate::dto::ReadOnlyReason::NotSane { .. })));
        }

        #[test]
        fn every_write_asks_the_firmware_question_then_goes() {
            let dir = TempDir::new("actor-writes");
            let (mut a, sim) = actor(&dir.0, "");
            let e = call(&mut a, |r| Request::ApplyEffect(settings(), r)).unwrap_err();
            assert_eq!(e, "untested keyboard firmware 3.01");
            assert_eq!(writes(&sim), 0);
            a.handle(Request::ConfirmFirmware("3.01".into()));

            call(&mut a, |r| Request::ApplyEffect(settings(), r)).unwrap();
            assert_eq!(sim.state().effect[0], 2);
            let esc = Sim::model().geometry.as_ref().unwrap().keys[0].led;
            call(&mut a, |reply| Request::ApplyPerKey { colors: vec![(esc, [7, 8, 9])], brightness: 5, reply })
                .unwrap();
            assert_eq!(sim.state().effect[0], kb_protocol::aa::effect::MODE_PER_KEY);
            let a_key = Sim::model().geometry.as_ref().unwrap().keys.iter().find(|k| k.id == "a").unwrap().led;
            let stored = call(&mut a, |reply| Request::ApplyKeymap {
                layer: Layer::Base,
                entries: vec![(a_key, KeyAction::key(0x05))],
                base: None,
                reply,
            })
            .unwrap();
            assert_eq!(stored.base.entries.len(), 1, "the record of the {RECORD} keyboard");
            assert!(a.keymaps.models.contains_key(RECORD));
            use kb_protocol::sonix::macros::{MacroStep, StepKind};
            let steps = vec![
                crate::settings::StoredStep(MacroStep::new(StepKind::KeyDown, 4, 20)),
                crate::settings::StoredStep(MacroStep::new(StepKind::KeyUp, 4, 20)),
            ];
            let slots = vec![Some(StoredMacro { name: "x".into(), steps })];
            call(&mut a, |reply| Request::ApplyMacros { slots, base: None, reply }).unwrap();
            assert!(!sim.state().sent(cmd::SET_MACROS).is_empty());
            call(&mut a, Request::SyncClock).unwrap();
            assert!(sim.state().clock.is_some());
            assert!(call(&mut a, Request::WriteBlocked).unwrap().is_none());
        }

        #[test]
        fn a_read_only_keyboard_refuses_every_write_path() {
            for (faults, text) in [
                ("insane", "the keyboard does not look as expected, so nothing is written"),
                ("update-pending", "the keyboard is waiting for a firmware update; nothing is read or written"),
            ] {
                let dir = TempDir::new(&format!("actor-{faults}"));
                let (mut a, sim) = actor(&dir.0, faults);
                a.handle(Request::ConfirmFirmware("3.01".into()));
                assert!(info(&a).aa_status.unwrap().read_only.is_some(), "{faults}");
                let refused = [
                    call(&mut a, |r| Request::ApplyEffect(settings(), r)).unwrap_err(),
                    call(&mut a, |reply| Request::ApplyPerKey { colors: vec![], brightness: 5, reply }).unwrap_err(),
                    call(&mut a, |reply| Request::ApplyKeymap { layer: Layer::Fn, entries: vec![], base: None, reply })
                        .unwrap_err(),
                    call(&mut a, |reply| Request::ApplyMacros { slots: vec![], base: None, reply }).unwrap_err(),
                    call(&mut a, Request::SyncClock).unwrap_err(),
                    upload(&mut a, "0123456789abcdef").error.unwrap(),
                ];
                for e in refused {
                    assert!(e.starts_with(text), "{faults}: {e}");
                }
                let block = call(&mut a, Request::WriteBlocked).unwrap().unwrap();
                assert!(block.to_string().starts_with(text), "{faults}");
                // Live lights: nothing switched, the UI hears why.
                a.handle(Request::SetLayer(LightLayer::Gauges, Some(frame(&[(1, 9)], 1.0))));
                a.send_live();
                a.enumerate();
                let off = info(&a).aa_status.unwrap().live_lights_off;
                assert_eq!(off, Some(crate::dto::LiveLightsOff::ReadOnly), "{faults}");
                assert_eq!(writes(&sim), 0, "{faults}: nothing written");
                assert!(sim.state().sent(cmd::REALTIME).is_empty() && sim.state().screen.is_empty());
            }
        }

        #[test]
        fn every_upload_path_stops_at_the_clock_page_until_the_user_says_yes() {
            let dir = TempDir::new("actor-clock-page");
            let (mut a, sim) = actor(&dir.0, "");
            let library = Arc::new(Mutex::new(crate::screen_lib::ScreenLibrary::open(Some(&dir.0))));
            a.screen_hook = crate::screen_journal(&library);
            a.handle(Request::ConfirmFirmware("3.01".into()));
            assert!(library.lock().unwrap().clock_page_at_risk(RECORD));
            // A library item (`screen_upload`), a live card, the live
            // screen going back to the own picture: one path, one guard.
            for id in ["0123456789abcdef", "live:music", "fedcba9876543210"] {
                let r = upload(&mut a, id);
                assert_eq!(r.error.as_deref(), Some(crate::ERR_CLOCK_PAGE), "{id}");
                assert!(!r.flash_written, "{id}");
                assert_eq!(r.record_id.as_deref(), Some(RECORD));
            }
            assert!(sim.state().screen.is_empty(), "nothing reached the screen channel");
            assert!(library.lock().unwrap().state(RECORD).incomplete.is_none(), "no upload began");
            // The user said yes.
            library.lock().unwrap().ack_clock_page(RECORD).unwrap();
            let r = upload(&mut a, "live:music");
            assert_eq!(r.error, None);
            assert!(r.flash_written && !sim.state().screen.is_empty());
            // A "04 xx" keyboard's record is never at risk.
            assert!(!library.lock().unwrap().clock_page_at_risk("ajazz-ak980-pro"));
        }

        #[test]
        fn live_lights_switch_through_the_actor_and_come_back() {
            let dir = TempDir::new("actor-live");
            let (mut a, sim) = actor(&dir.0, "");
            // Untested firmware not confirmed: not switched, and the UI
            // hears why.
            a.handle(Request::SetLayer(LightLayer::Gauges, Some(frame(&[(1, 9)], 1.0))));
            a.send_live();
            a.enumerate();
            assert_eq!(
                info(&a).aa_status.unwrap().live_lights_off,
                Some(crate::dto::LiveLightsOff::Blocked { reason: "untested keyboard firmware 3.01".into() })
            );
            assert_eq!(sim.state().effect[0], 1);
            // Confirmed: the note goes at once (the UI hears it on the next
            // look, due now), and the next frame switches and shows.
            a.next_enumerate = Instant::now() + Duration::from_secs(60);
            a.handle(Request::ConfirmFirmware("3.01".into()));
            assert!(a.next_enumerate <= Instant::now(), "the UI hears it at once");
            a.enumerate();
            assert_eq!(info(&a).aa_status.unwrap().live_lights_off, None);
            a.send_live();
            assert_eq!(sim.state().effect[0], kb_protocol::aa::effect::MODE_PER_KEY);
            assert!(sim.state().frames.last().is_some_and(|f| f.1), "shown");
            a.enumerate();
            let status = info(&a).aa_status.unwrap();
            assert!(status.live_armed && status.live_lights_off.is_none());
            assert!(status.live_restores, "its own effect comes back after the hold");
            // The layer goes: the hold starts, the actor wakes for it.
            a.handle(Request::SetLayer(LightLayer::Gauges, None));
            a.end_live();
            let due = a.driver_due().expect("the effect goes back later");
            assert!(due > Instant::now() + Duration::from_secs(25));
            // Closing does not wait for it.
            let (tx, rx) = mpsc::channel();
            a.handle(Request::Closing(tx));
            rx.try_recv().unwrap();
            assert_eq!(sim.state().effect[0], 1);
            assert_eq!(a.driver_due(), None);
        }

        #[test]
        fn backups_are_listed_and_restored_through_the_actor() {
            let dir = TempDir::new("actor-restore");
            let (mut a, sim) = actor(&dir.0, "");
            a.handle(Request::ConfirmFirmware("3.01".into()));
            let list = a.keyboard_backups();
            assert_eq!(list.len(), 1);
            let first = &list[0];
            assert!(first.matches && first.color_known);
            assert_eq!(first.reason, KeyboardBackupReason::FirstConnect);
            assert_eq!((first.record_id.as_str(), first.firmware.as_str()), (RECORD, "3.01"));
            call(&mut a, |r| Request::ApplyEffect(settings(), r)).unwrap();
            let file = first.file.clone();
            let result =
                call(&mut a, |reply| Request::RestoreKeyboard { file: file.clone(), color: None, reply }).unwrap();
            assert_eq!(result.written, ["effect"]);
            assert!(result.kept.is_empty());
            // The Lighting page's record follows: the effect that went back,
            // its colour known.
            let e = result.effect.as_ref().expect("its colour is known");
            assert_eq!((e.mode, e.color.as_str(), e.rainbow), (1, "#0080ff", false));
            assert_eq!(sim.state().effect[0], 1);
            let list = a.keyboard_backups();
            assert_eq!(list.len(), 2);
            assert_eq!(list[1].reason, KeyboardBackupReason::BeforeRestore);
            // Only a backup of the keyboard on the cable, and only a backup.
            for bad in ["../keymap.json", "aa-8009-0301-1.json"] {
                assert!(
                    call(&mut a, |reply| Request::RestoreKeyboard { file: bad.into(), color: None, reply }).is_err()
                );
            }
            let other = crate::aa::backups::KeyboardBackup {
                pid: 0x8009,
                ..crate::aa::backups::load(&crate::aa::backups::dir(&dir.0), &file).unwrap()
            };
            let name = crate::aa::backups::save(&crate::aa::backups::dir(&dir.0), &other).unwrap();
            let e = call(&mut a, |reply| Request::RestoreKeyboard { file: name, color: None, reply }).unwrap_err();
            assert_eq!(e, ERR_BACKUP_OTHER_KEYBOARD);
        }

        // ------------------------------------------------------------ review of #195

        fn errors_noted(a: &Actor) -> usize {
            a.aa_driver().map_or(0, |d| d.diagnostics().iter().filter(|l| l.starts_with("error at")).count())
        }

        #[test]
        fn a_handle_left_from_another_keyboard_says_nothing_about_the_one_on_the_cable() {
            let dir = TempDir::new("actor-stale-handle");
            let (mut a, _) = actor(&dir.0, "update-pending");
            assert_eq!(a.read_only(), Some(ReadOnly::UpdatePending));
            // The "04 xx" AK980 PRO takes the cable before a listing in
            // between drops the "AA" handle.
            let model = Sim::model();
            let ff13 = model.modes.iter().find(|m| model.protocol_of(m) == Protocol::SonixFf13).unwrap();
            a.usb = Some(UsbKeyboard { model, mode: ff13, record_id: "ajazz-ak980-pro".into() });
            a.command_path = Some("ff13-command".into());
            assert_eq!(a.read_only(), None);
            a.firmware_confirmed = a.usb_firmware;
            let block = a.gate_block(WriteKind::Memory);
            assert!(
                !matches!(block, Some(WriteBlock::NotSane(_) | WriteBlock::UpdatePending | WriteBlock::NoBackup(_))),
                "{block:?}"
            );

            // A listing drops a handle whose path is gone.
            let dir = TempDir::new("actor-gone-path");
            let (mut a, _) = actor(&dir.0, "");
            a.command.as_mut().expect("opened when listed").path = "gone".into();
            a.enumerate();
            assert!(a.command.as_ref().is_none_or(|c| c.path != "gone"));
        }

        #[test]
        fn closing_waits_only_when_an_aa_keyboard_is_held() {
            let (tx, _rx) = mpsc::channel();
            let h = DeviceHandle {
                snapshot: Arc::default(),
                flash: FlashState::new(Ui(None)),
                tx,
                uploads: Arc::default(),
                aa_hold: Arc::new(AtomicBool::new(false)),
                config_dir: None,
            };
            let t = Instant::now();
            h.closing(Duration::from_secs(2));
            assert!(t.elapsed() < Duration::from_millis(500), "nothing held: no wait");
            h.aa_hold.store(true, Ordering::SeqCst);
            let t = Instant::now();
            h.closing(Duration::from_millis(300));
            assert!(t.elapsed() >= Duration::from_millis(300), "held: it waits for the actor");

            // The actor says when it holds one.
            let dir = TempDir::new("actor-hold");
            let (mut a, _) = actor(&dir.0, "");
            a.handle(Request::ConfirmFirmware("3.01".into()));
            a.note_hold();
            assert!(!a.aa_hold.load(Ordering::SeqCst));
            a.handle(Request::SetLayer(LightLayer::Gauges, Some(frame(&[(1, 9)], 1.0))));
            a.send_live();
            a.note_hold();
            assert!(a.aa_hold.load(Ordering::SeqCst));
            let (tx, rx) = mpsc::channel();
            a.handle(Request::Closing(tx));
            rx.try_recv().unwrap();
            a.note_hold();
            assert!(!a.aa_hold.load(Ordering::SeqCst));
        }

        #[test]
        fn closing_ends_the_stream_for_good() {
            let dir = TempDir::new("actor-closed");
            let (mut a, sim) = actor(&dir.0, "");
            a.handle(Request::ConfirmFirmware("3.01".into()));
            a.handle(Request::SetLayer(LightLayer::Gauges, Some(frame(&[(1, 9)], 1.0))));
            a.send_live();
            assert_eq!(sim.state().effect[0], kb_protocol::aa::effect::MODE_PER_KEY);
            let (tx, rx) = mpsc::channel();
            a.handle(Request::Closing(tx));
            rx.try_recv().unwrap();
            assert_eq!(sim.state().effect[0], 1);
            let writes_before = sim.state().sent(cmd::SET_EFFECT).len();
            // A layer that still comes, and the next turns of the loop.
            a.handle(Request::SetLayer(LightLayer::Gauges, Some(frame(&[(1, 7)], 1.0))));
            assert!(!a.streaming());
            a.send_live();
            a.handle(Request::SetLive(Some(frame(&[(1, 5)], 1.0))));
            a.send_live();
            assert_eq!(sim.state().effect[0], 1);
            assert_eq!(sim.state().sent(cmd::SET_EFFECT).len(), writes_before);
            assert_eq!(a.driver_due(), None);
        }

        #[test]
        fn the_key_and_macro_records_follow_the_keyboard() {
            use kb_core::keymap::LayerMap;
            use kb_protocol::aa::keymap as aa_keymap;
            use kb_protocol::sonix::macros::{MacroStep, StepKind};
            let dir = TempDir::new("actor-records");
            let key = |id: &str| {
                let k = Sim::model().geometry.as_ref().unwrap().keys.iter().find(|k| k.id == id).unwrap();
                (k.led, k.aa.unwrap())
            };
            let ((b_led, b_aa), (d_led, d_aa), (a_led, _)) = (key("b"), key("d"), key("a"));
            let func = [aa_keymap::page::FUNC, 0, 0, 2];
            // Set in AJAZZ's tool: B runs a firmware function, D types "b".
            let (mut a, sim) = actor_with(&dir.0, "", |sim| {
                let mut st = sim.state();
                aa_keymap::set_entry(&mut st.keymap, b_aa, func);
                aa_keymap::set_entry(&mut st.keymap, d_aa, [aa_keymap::page::KEYBOARD, 0, 0x05, 0]);
            });
            // Taken when it connected: what the Key mapping page starts
            // from, on disk and in what the UI reads.
            let k = a.keymaps.models[RECORD].clone();
            assert!(k.read_ms.is_some());
            assert_eq!(k.base.actions(), LayerMap::from([(d_led, KeyAction::key(0x05))]));
            assert_eq!(k.base.fixed, [b_led]);
            assert_eq!(settings::load_keymaps(&dir.0).models[RECORD], k);
            assert_eq!(a.shared.lock().unwrap().keymaps.models[RECORD], k);
            let m = &a.macros.models[RECORD];
            assert!(m.read_ms.is_some() && m.slots.is_empty() && m.fixed.is_empty());

            // A save of the page as shown (D kept, A added): B stays, and
            // the record is what the keyboard holds.
            a.handle(Request::ConfirmFirmware("3.01".into()));
            let stored = call(&mut a, |reply| Request::ApplyKeymap {
                layer: Layer::Base,
                entries: vec![(d_led, KeyAction::key(0x05)), (a_led, KeyAction::key(0x06))],
                base: None,
                reply,
            })
            .unwrap();
            assert_eq!(aa_keymap::entry(&sim.state().keymap, b_aa), func);
            assert_eq!(stored.base.actions().len(), 2);
            assert_eq!(stored.base.fixed, [b_led]);
            assert!(stored.base.written_ms.is_some());
            assert_eq!(a.keymaps.models[RECORD], stored);

            // Macros keep the names given, against the keyboard's steps.
            let steps = vec![
                crate::settings::StoredStep(MacroStep::new(StepKind::KeyDown, 4, 20)),
                crate::settings::StoredStep(MacroStep::new(StepKind::KeyUp, 4, 20)),
            ];
            let slots = vec![Some(StoredMacro { name: "x".into(), steps: steps.clone() })];
            let stored = call(&mut a, |reply| Request::ApplyMacros { slots, base: None, reply }).unwrap();
            assert_eq!(stored.slots[0].as_ref().map(|m| (m.name.as_str(), m.steps.clone())), Some(("x", steps)));
            assert!(stored.written_ms.is_some() && stored.read_ms.is_some());
            assert_eq!(
                settings::load_macros(&dir.0).models[RECORD],
                stored,
                "the names and the write time on disk too"
            );
            assert_eq!(settings::load_keymaps(&dir.0).models[RECORD], a.keymaps.models[RECORD]);

            // A restore of the backup from the first connection: the records
            // follow what went back (A and the macro gone, B and D there).
            let file = a.keyboard_backups()[0].file.clone();
            call(&mut a, |reply| Request::RestoreKeyboard { file, color: None, reply }).unwrap();
            let k = &a.keymaps.models[RECORD];
            assert_eq!(k.base.actions(), LayerMap::from([(d_led, KeyAction::key(0x05))]));
            assert_eq!(k.base.fixed, [b_led]);
            assert!(a.macros.models[RECORD].slots.iter().all(Option::is_none));
            assert_eq!(settings::load_macros(&dir.0).models[RECORD], a.macros.models[RECORD]);
        }

        #[test]
        fn a_stale_save_is_refused_and_the_records_catch_up() {
            use kb_core::keymap::LayerMap;
            use kb_protocol::aa::keymap as aa_keymap;
            let dir = TempDir::new("actor-stale");
            let (mut a, sim) = actor(&dir.0, "");
            a.handle(Request::ConfirmFirmware("3.01".into()));
            let key = |id: &str| {
                let k = Sim::model().geometry.as_ref().unwrap().keys.iter().find(|k| k.id == id).unwrap();
                (k.led, k.aa.unwrap())
            };
            let ((a_led, _), (d_led, d_aa)) = (key("a"), key("d"));
            let source = a.keymaps.models[RECORD].source.clone();
            assert!(source.is_some(), "the page gets it with the record");
            // AJAZZ's tool sets D while the page shows the old keys.
            aa_keymap::set_entry(&mut sim.state().keymap, d_aa, [aa_keymap::page::KEYBOARD, 0, 0x05, 0]);
            let before = sim.state().keymap.clone();
            let e = call(&mut a, |reply| Request::ApplyKeymap {
                layer: Layer::Base,
                entries: vec![(a_led, KeyAction::key(0x06))],
                base: source.clone(),
                reply,
            })
            .unwrap_err();
            assert_eq!(e, crate::aa::ERR_KEYS_CHANGED);
            assert_eq!(sim.state().keymap, before, "nothing written");
            // The records (and the file) show D now, with a new source.
            let k = a.keymaps.models[RECORD].clone();
            assert_eq!(k.base.actions(), LayerMap::from([(d_led, KeyAction::key(0x05))]));
            assert_ne!(k.source, source);
            assert_eq!(settings::load_keymaps(&dir.0).models[RECORD], k);
            // Against it, the save goes.
            call(&mut a, |reply| Request::ApplyKeymap {
                layer: Layer::Base,
                entries: vec![(a_led, KeyAction::key(0x06)), (d_led, KeyAction::key(0x05))],
                base: k.source.clone(),
                reply,
            })
            .unwrap();
            // Macros: one made meanwhile is not deleted by a stale save.
            let m = a.macros.models[RECORD].source.clone();
            let steps = vec![
                crate::settings::StoredStep(kb_protocol::sonix::macros::MacroStep::new(
                    kb_protocol::sonix::macros::StepKind::KeyDown,
                    4,
                    20,
                )),
                crate::settings::StoredStep(kb_protocol::sonix::macros::MacroStep::new(
                    kb_protocol::sonix::macros::StepKind::KeyUp,
                    4,
                    20,
                )),
            ];
            let slots = vec![Some(StoredMacro { name: "x".into(), steps })];
            call(&mut a, |reply| Request::ApplyMacros { slots: slots.clone(), base: m.clone(), reply }).unwrap();
            let m = a.macros.models[RECORD].source.clone();
            let e = call(&mut a, |reply| Request::ApplyMacros { slots: vec![], base: Some("0".into()), reply });
            assert_eq!(e.unwrap_err(), crate::aa::ERR_MACROS_CHANGED);
            assert_eq!(a.macros.models[RECORD].slots.len(), 1, "still there");
            assert_eq!(a.macros.models[RECORD].source, m);
        }

        #[test]
        fn live_frames_refused_in_the_quiet_time_are_no_failures() {
            let dir = TempDir::new("actor-quiet-live");
            let (mut a, _sim) = actor(&dir.0, "");
            a.handle(Request::ConfirmFirmware("3.01".into()));
            a.handle(Request::SetLayer(LightLayer::Gauges, Some(frame(&[(1, 9)], 1.0))));
            a.send_live();
            assert_eq!(a.live_failures, 0);
            // An upload cut part-way (after its first part): the keyboard
            // takes nothing for a while.
            struct CutAfterFirstPart(Arc<UploadCancels>);
            impl UiSink for CutAfterFirstPart {
                fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
                    if event == EVENT_SCREEN_PROGRESS && payload["done"] == 1 {
                        self.0.cancel(payload["id"].as_str().unwrap_or_default());
                    }
                    Ok(())
                }
                fn hold_shutdown(&self, _: Option<String>) {}
            }
            a.app = Ui(Some(Arc::new(CutAfterFirstPart(a.uploads.clone()))));
            let r = upload(&mut a, "cut");
            assert!(r.error.as_deref().is_some_and(|e| e.starts_with("screen upload cancelled")), "{r:?}");
            for _ in 0..=LIVE_FAILURES_WARN {
                a.send_live();
            }
            assert!(a.aa_driver().is_some_and(crate::aa::AaDriver::in_quiet_time));
            assert_eq!(a.live_failures, 0, "refused on purpose: no failure, no warning");
        }

        #[test]
        fn a_cancel_reaches_only_its_own_upload() {
            let dir = TempDir::new("actor-cancel");
            let (mut a, sim) = actor(&dir.0, "");
            a.handle(Request::ConfirmFirmware("3.01".into()));
            // Two uploads wait their turn; the second is cancelled.
            a.uploads.queued("first");
            a.uploads.queued("second");
            assert!(a.uploads.cancel("second"));
            assert!(!a.uploads.cancel("never-sent"));
            let r = upload(&mut a, "first");
            assert_eq!(r.error, None, "the one not cancelled runs to its end");
            let parts = sim.state().screen.len();
            let r = upload(&mut a, "second");
            assert_eq!(r.error.as_deref(), Some(ERR_UPLOAD_CANCELLED));
            assert!(!r.flash_written);
            assert_eq!(sim.state().screen.len(), parts, "it did not start");
            // The one that runs stops before its next part.
            let uploads = a.uploads.clone();
            a.screen_hook = Box::new(move |step: ScreenWriteStep<'_>| {
                if let ScreenWriteStep::Starting { label, .. } = step {
                    uploads.cancel(label);
                }
                Ok(())
            });
            let r = upload(&mut a, "third");
            assert!(r.error.as_deref().is_some_and(|e| e.starts_with("screen upload cancelled after 0 of")), "{r:?}");
            // Nothing left waiting or running.
            assert!(!a.uploads.cancel("first") && !a.uploads.cancel("third"));
        }

        #[test]
        fn the_mirror_leaves_a_silent_or_waiting_keyboard_alone() {
            for faults in ["silent", "update-pending"] {
                let dir = TempDir::new(&format!("actor-mirror-{faults}"));
                let (mut a, sim) = actor(&dir.0, faults);
                a.handle(Request::SetMirror(Some(MirrorExpect::default())));
                let sent = sim.state().packets.len();
                let errors = errors_noted(&a);
                for _ in 0..3 {
                    a.send_mirror();
                }
                assert_eq!(sim.state().packets.len(), sent, "{faults}: nothing asked");
                assert_eq!(errors_noted(&a), errors, "{faults}: nothing noted");
            }
            // A keyboard that answers is mirrored.
            let dir = TempDir::new("actor-mirror-ok");
            let (mut a, sim) = actor(&dir.0, "");
            a.handle(Request::SetMirror(Some(MirrorExpect::default())));
            let sent = sim.state().packets.len();
            a.send_mirror();
            assert!(sim.state().packets.len() > sent);
        }

        #[test]
        fn the_running_actor_tells_the_handle_when_it_holds_a_keyboard() {
            let dir = TempDir::new("actor-run-hold");
            let path = dir.0.clone();
            let hold = Arc::new(AtomicBool::new(false));
            let flag = hold.clone();
            let (tx, rx) = mpsc::channel();
            // The actor holds the HID subsystem type, which stays on its own
            // thread: made there, as `spawn` does.
            let running = std::thread::spawn(move || {
                let (mut a, _) = actor(&path, "");
                a.aa_hold = flag;
                a.run(rx);
            });
            let wait_for = |want: bool| {
                let since = Instant::now();
                while hold.load(Ordering::SeqCst) != want && since.elapsed() < Duration::from_secs(5) {
                    std::thread::sleep(Duration::from_millis(20));
                }
                hold.load(Ordering::SeqCst) == want
            };
            tx.send(Request::ConfirmFirmware("3.01".into())).unwrap();
            tx.send(Request::SetLayer(LightLayer::Gauges, Some(frame(&[(1, 9)], 1.0)))).unwrap();
            // Frames switch it; no further request comes.
            assert!(wait_for(true), "the handle hears it while frames stream");
            let (done, answered) = mpsc::channel();
            tx.send(Request::Closing(done)).unwrap();
            answered.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(wait_for(false), "nothing held after closing");
            drop(tx);
            running.join().unwrap();
        }

        #[test]
        fn a_failing_aa_channel_is_not_opened_again_with_every_frame() {
            let dir = TempDir::new("actor-reopen");
            let (mut a, sim) = actor(&dir.0, "");
            a.handle(Request::ConfirmFirmware("3.01".into()));
            let first = a.keyboard_backups()[0].file.clone();
            a.handle(Request::SetLayer(LightLayer::Gauges, Some(frame(&[(1, 9)], 1.0))));
            a.send_live();
            // The switch itself told the handle, before any later loop turn.
            assert!(a.aa_hold.load(Ordering::SeqCst));
            let Ports::Sim(board) = &a.ports else { unreachable!() };
            let (dead, opened) = (board.dead.clone(), board.opened.clone());
            // The keyboard stays listed, but its channel dies: the next frame
            // fails, and every open after it fails in the full read.
            dead.store(true, Ordering::SeqCst);
            let next = sim.state().packets.len() + 1;
            sim.state().faults.fail_write = Some(next);
            a.send_live();
            assert!(a.command.is_none(), "the failed handle went");
            let opens = opened.load(Ordering::SeqCst);
            a.handle(Request::SetMirror(Some(MirrorExpect::default())));
            for _ in 0..10 {
                a.send_live();
                a.send_mirror();
            }
            assert_eq!(opened.load(Ordering::SeqCst), opens, "frames and the mirror wait");
            // The user's own write tries once, and says why it failed.
            let e = call(&mut a, |r| Request::ApplyEffect(settings(), r)).unwrap_err();
            assert!(e.contains("the handle died"), "{e}");
            assert_eq!(opened.load(Ordering::SeqCst), opens + 1);
            a.send_live();
            assert_eq!(opened.load(Ordering::SeqCst), opens + 1, "and frames wait again");
            // A restore says no keyboard could be read, not that the backup
            // is another keyboard's.
            let e = call(&mut a, |reply| Request::RestoreKeyboard { file: first, color: None, reply }).unwrap_err();
            assert!(e.starts_with(ERR_KEYBOARD_NOT_READ), "{e}");
            // Once the wait is over, a frame opens it again.
            let path = a.command_path.clone().unwrap();
            a.aa_reopen_after = Some((path, Instant::now()));
            a.send_live();
            assert_eq!(opened.load(Ordering::SeqCst), opens + 3);
        }
    }
}
