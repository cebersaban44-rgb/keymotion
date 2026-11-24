//! The "AA" family (AK980 PRO on firmware 769) in the app: the driver arm
//! the device actor talks to ([`AaDriver`]), and what the app keeps around
//! it (plan K-3 to K-10, notes: aa-surucu-plani.md):
//!
//! * **Connect.** Opening the command channel reads everything
//!   (`kb_core::aa::AaKeyboard::read_all`). A keyboard that does not pass
//!   the sanity check, or waits for a firmware update, stays read-only
//!   ([`ReadOnly`]). A sane one gets a backup of everything it holds saved
//!   to the app's data folder ([`backups`]) before anything can be written;
//!   no backup, no writes.
//! * **Live lights** show only in the effect modes 20, 21 and 128. The first
//!   frame of a stream switches the keyboard to 128 (a flash write, through
//!   the actor's Memory gate) and notes it on disk ([`ArmedRecord`]); 30
//!   seconds after the last frame ([`LIVE_HOLD`]) the user's effect goes
//!   back and the note goes. A crash, a quit or a pulled cable leaves the
//!   note, and the next connect puts the effect back. Putting it back needs
//!   its colour, which the keyboard does not report after a write: arming
//!   waits until the driver or the app's lighting record knows it
//!   ([`LiveOff::ColorUnknown`]).
//! * **Keymaps** change only the keys that differ from a fresh read: the
//!   "AA" driver changes the slots it is given and refuses to bring back
//!   some factory functions, so a whole layer of defaults is never sent.
//! * **Conflict hint** (K-10): replies nobody asked for are counted; many
//!   in a short while mean another program (AJAZZ's web hub, which no
//!   process name gives away) talks to the keyboard.
//!
//! Nothing here talks to Tauri: the actor passes in what it knows (the
//! write gate's verdict, the app's lighting record) and shows what comes
//! out. Tests drive it against `kb_core::aa::sim`.

use crate::driver::Error;
use crate::dto::{AaStatus, ChargeState, DeviceReport, LiveLightsOff, ReadOnlyReason};
use kb_core::aa::{
    levels_in_range, AaEffect, AaError, AaKeyboard, AaLayout, AaMacroEntry, AaSnapshot, Binding, DeviceInfo,
    EffectColor, EffectRead, Hex, OutputPort, RestoreReport, UsbIdentity,
};
use kb_core::keymap::LayerMap;
use kb_devices::Model;
use kb_protocol::aa::keymap as aa_keymap;
use kb_protocol::aa::macros as aa_macros;
use kb_protocol::sonix::keymap::{KeyAction, Layer};
use kb_protocol::sonix::macros::MacroStep;
use kb_protocol::sonix::screen::ScreenAnimation;
use kb_protocol::sonix::{ClockTime, EffectSettings, Rgb};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long the keyboard stays in per-key mode after the last live frame
/// before the user's effect goes back (plan K-4): agents' lights start and
/// stop often, and every switch is a flash write.
pub const LIVE_HOLD: Duration = Duration::from_secs(30);
/// Arming that was refused (a write gate, a failure) is tried again after
/// this long while frames keep coming; a switch that keeps failing waits
/// twice as long each time, up to [`ARM_RETRY_MAX`].
const ARM_RETRY: Duration = Duration::from_secs(30);
const ARM_RETRY_MAX: Duration = Duration::from_secs(8 * 60);
/// A failed put-back of the user's effect is tried again after this long.
const DISARM_RETRY: Duration = Duration::from_secs(10);
/// A full read that did not finish (no answer, a reset) is tried again in
/// the background after this long; a write tries it at once.
const REREAD_EVERY: Duration = Duration::from_secs(30);
/// Unexpected replies within [`CONFLICT_WINDOW`] that make the conflict
/// hint show.
pub const CONFLICT_THRESHOLD: u64 = 10;
pub const CONFLICT_WINDOW: Duration = Duration::from_secs(60);
/// Driver errors kept for the diagnostics report.
const ERRORS_KEPT: usize = 10;
/// The note of an armed keyboard, in the app's data folder.
pub const ARMED_FILE: &str = "aa-live-armed.json";
const ARMED_VERSION: u32 = 2;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// An "AA" collection: the HID channel, or the simulator's in debug builds.
pub struct AaPort(Box<dyn OutputPort + Send>);

impl AaPort {
    pub fn new(port: impl OutputPort + Send + 'static) -> Self {
        Self(Box::new(port))
    }
}

impl OutputPort for AaPort {
    fn write(&mut self, report: &[u8]) -> kb_core::aa::Result<usize> {
        self.0.write(report)
    }
    fn read(&mut self, timeout_ms: u32) -> kb_core::aa::Result<Option<Vec<u8>>> {
        self.0.read(timeout_ms)
    }
    fn drain(&mut self) -> usize {
        self.0.drain()
    }
}

/// A port that remembers whether one of its operations failed: tells a
/// screen channel failure from a command channel one.
struct Watched<'a> {
    port: &'a mut AaPort,
    failed: bool,
}

impl OutputPort for Watched<'_> {
    fn write(&mut self, report: &[u8]) -> kb_core::aa::Result<usize> {
        let r = self.port.write(report);
        self.failed |= r.is_err();
        r
    }
    fn read(&mut self, timeout_ms: u32) -> kb_core::aa::Result<Option<Vec<u8>>> {
        let r = self.port.read(timeout_ms);
        self.failed |= r.is_err();
        r
    }
    fn drain(&mut self) -> usize {
        self.port.drain()
    }
}

/// Why nothing is written to the keyboard. The texts are backend errors
/// (English); lib/errors.ts maps them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOnly {
    /// Its answers do not look as expected (the sanity check), or the last
    /// full read did not finish.
    NotSane(Vec<String>),
    /// A firmware update was left unfinished: AJAZZ's tool has to finish it.
    UpdatePending,
    /// The backup of its settings could not be saved.
    NoBackup(String),
}

impl fmt::Display for ReadOnly {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSane(why) => {
                write!(f, "the keyboard does not look as expected, so nothing is written: {}", why.join("; "))
            }
            Self::UpdatePending => {
                write!(f, "the keyboard is waiting for a firmware update; nothing is read or written")
            }
            Self::NoBackup(e) => write!(f, "no backup of the keyboard could be saved, so nothing is written: {e}"),
        }
    }
}

impl From<&ReadOnly> for ReadOnlyReason {
    fn from(r: &ReadOnly) -> Self {
        match r {
            ReadOnly::NotSane(reasons) => Self::NotSane { reasons: reasons.clone(), read_failed: false },
            ReadOnly::UpdatePending => Self::UpdatePending,
            ReadOnly::NoBackup(error) => Self::NoBackup { error: error.clone() },
        }
    }
}

/// Why live lights do not show on the keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveOff {
    /// The colour of the user's effect is not known, so it could not be put
    /// back: the UI asks to set the lighting once in Keymotion.
    ColorUnknown,
    /// The keyboard is read-only ([`ReadOnly`]).
    ReadOnly,
    /// A write gate refused the switch to per-key mode (its text).
    Blocked(String),
    /// The switch failed (the error).
    Failed(String),
}

impl From<&LiveOff> for LiveLightsOff {
    fn from(o: &LiveOff) -> Self {
        match o {
            LiveOff::ColorUnknown => Self::ColorUnknown,
            LiveOff::ReadOnly => Self::ReadOnly,
            LiveOff::Blocked(reason) => Self::Blocked { reason: reason.clone() },
            LiveOff::Failed(error) => Self::Failed { error: error.clone() },
        }
    }
}

/// The Memory gate: `Err` says why a write to the keyboard's memory may not
/// go now.
pub type Gate<'a> = &'a dyn Fn() -> Result<(), String>;

/// What the actor lends the arm for the writes it makes on its own (the
/// switch to per-key mode and back). Both are asked only when needed.
#[derive(Clone, Copy, Default)]
pub struct LiveContext<'a> {
    /// The Memory gate (several keyboards, another program, untested
    /// firmware), asked right before such a write: it lists the running
    /// programs and may put the firmware question to the user. `None`:
    /// nothing is written.
    pub gate: Option<Gate<'a>>,
    /// The effect the app last stored (its lighting record, calibrated), in
    /// the keyboard's values: its colour is the effect's when the mode and
    /// levels match what the keyboard holds.
    pub app_effect: Option<&'a dyn Fn() -> Option<AaEffect>>,
    /// Told right before the switch to per-key mode is written: from then
    /// on the keyboard may need its effect back when the app closes.
    pub arming: Option<&'a dyn Fn()>,
}

impl LiveContext<'_> {
    /// The gate's verdict now; `None` when there is no gate.
    fn ask(&self) -> Option<Result<(), String>> {
        self.gate.map(|g| g())
    }

    fn app_effect(&self) -> Option<AaEffect> {
        self.app_effect.and_then(|f| f())
    }
}

/// What a switch to per-key mode changed: the user's effect to put back
/// (colour included, levels 1–5) and what Keymotion left on the keyboard
/// instead. The effect goes back only while the keyboard still shows the
/// latter: anything else is the user's own choice since.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Restore {
    back: AaEffect,
    armed: AaEffect,
}

#[derive(Debug, Clone)]
enum Live {
    /// No live lights on the keyboard; the user's effect shows.
    Idle,
    /// The keyboard is in a live mode. `restore`: what goes back (`None`:
    /// it was in a live mode already); `stopped`: when the last stream
    /// ended (the hold runs).
    Armed { restore: Option<Restore>, stopped: Option<Instant>, retry_at: Option<Instant> },
    /// Not switched for this stream; tried again at `retry_at` (`None`:
    /// with the next stream).
    Off { why: LiveOff, retry_at: Option<Instant> },
}

/// The note of an armed keyboard (crash recovery): which keyboard, the
/// effect to put back and the one Keymotion left on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArmedRecord {
    record_id: String,
    pid: u16,
    release: u16,
    /// The effect block to put back, colour included.
    effect: Hex,
    /// The effect block the keyboard showed after the switch.
    armed: Hex,
    /// What the switch does not change ([`fingerprint`]): tells this
    /// keyboard from another of the same model and firmware.
    fingerprint: String,
    at_ms: u64,
}

impl ArmedRecord {
    /// The effects it names, when both read as such.
    fn restore(&self) -> Option<Restore> {
        let back = AaEffect::decode(&self.effect.0).ok().filter(|e| !e.is_live())?;
        let armed = AaEffect::decode(&self.armed.0).ok().filter(AaEffect::is_live)?;
        Some(Restore { back, armed })
    }
}

/// What a switch to per-key mode leaves as it was (both keymaps, the
/// per-key table, the macro table), hashed: another keyboard of the same
/// model and firmware almost surely differs. FNV-1a, which stays the same
/// across builds (the note outlives the run that wrote it).
///
/// Not verified on a real 769 yet (beta checks): two boards nobody
/// customised hash the same (their tables are the factory ones), so a note
/// can be taken for the other board's; and if its per-key read returns the
/// colours live frames put in RAM rather than the stored table, the hash
/// changes while live lights run and the note would not be recognised.
fn fingerprint(s: &AaSnapshot) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in [&s.keymap.0, &s.fn_keymap.0, &s.per_key.0, &s.macro_table.0] {
        let len = (part.len() as u64).to_le_bytes();
        for b in len.iter().chain(part.iter()) {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

/// The last full read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Read {
    NotYet,
    /// Finished; `sane`: it passed the sanity check.
    Done {
        sane: bool,
    },
    Failed(String),
}

/// Unexpected replies over time.
#[derive(Debug, Default)]
struct ConflictWatch {
    samples: VecDeque<(Instant, u64)>,
    suspected: bool,
}

impl ConflictWatch {
    /// Note the driver's count now; true when the hint changed. The newest
    /// sample from before the window stays as the base: an idle keyboard is
    /// asked something only every minute or so, and the replies another
    /// program caused meanwhile all arrive with that one exchange.
    fn note(&mut self, now: Instant, count: u64) -> bool {
        self.samples.push_back((now, count));
        while self.samples.get(1).is_some_and(|(t, _)| now.duration_since(*t) >= CONFLICT_WINDOW) {
            self.samples.pop_front();
        }
        let first = self.samples.front().map_or(count, |(_, c)| *c);
        let suspected = count.saturating_sub(first) >= CONFLICT_THRESHOLD;
        let changed = suspected != self.suspected;
        self.suspected = suspected;
        changed
    }
}

/// Where the app keeps an "AA" keyboard's files, and which one it is.
#[derive(Debug, Clone)]
pub struct AaTarget {
    pub record_id: String,
    pub usb: UsbIdentity,
    /// The app's data folder (backups, the live note); `None`: nothing can
    /// be kept, so nothing is written.
    pub data_dir: Option<PathBuf>,
}

/// One "AA" keyboard on its FF68 command channel, as the app drives it.
pub struct AaDriver {
    kb: AaKeyboard<AaPort>,
    target: AaTarget,
    read: Read,
    read_at: Option<Instant>,
    /// The backup taken before the first write: its file, or why it failed.
    backup: Option<Result<String, String>>,
    live: Live,
    /// The user's effect to put back after a crash left the keyboard in a
    /// live mode (from [`ArmedRecord`]), and when to try again after a
    /// refusal.
    recover: Option<(Restore, Option<Instant>)>,
    /// Switches to per-key mode that failed in a row (each waits longer).
    arm_failures: u32,
    /// The quiet time after a cut screen upload: nothing is tried before.
    quiet_until: Option<Instant>,
    /// The app is closing: nothing is switched to per-key mode any more.
    closed: bool,
    /// The fingerprint this keyboard's live note was saved under.
    noted: Option<String>,
    /// The last full read failed in a way that needs the channel opened
    /// again: the next operation hands it to the actor, which drops the
    /// handle.
    reopen: Option<AaError>,
    conflict: ConflictWatch,
    /// The last errors (Unix ms, text), for the diagnostics report.
    errors: VecDeque<(u64, String)>,
}

impl AaDriver {
    /// The driver for `model` on `port`; nothing is sent yet ([`Self::connect`]).
    pub fn new(port: AaPort, model: &Model, target: AaTarget) -> Result<Self, String> {
        let layout = AaLayout::from_model(model).map_err(|e| e.to_string())?;
        let mut conflict = ConflictWatch::default();
        conflict.note(Instant::now(), 0);
        Ok(Self {
            kb: AaKeyboard::new(port, layout, target.usb),
            target,
            read: Read::NotYet,
            read_at: None,
            backup: None,
            live: Live::Idle,
            recover: None,
            arm_failures: 0,
            quiet_until: None,
            closed: false,
            noted: None,
            reopen: None,
            conflict,
            errors: VecDeque::new(),
        })
    }

    /// The driver itself (tests: a shorter cooldown).
    #[cfg(test)]
    pub fn keyboard_mut(&mut self) -> &mut AaKeyboard<AaPort> {
        &mut self.kb
    }

    pub fn record_id(&self) -> &str {
        &self.target.record_id
    }

    /// The effect the keyboard holds, from the last read or write.
    pub fn effect_now(&self) -> Option<EffectRead> {
        self.kb.current_effect()
    }

    // ------------------------------------------------------------ connect

    /// Read everything (device information first) and run the sanity check;
    /// a sane keyboard gets its backup (with the colour `app_effect` tells,
    /// if the keyboard's does not) and a live note from a crash is looked
    /// at. Nothing is written here.
    pub fn connect(&mut self, app_effect: Option<AaEffect>) {
        self.read_at = Some(Instant::now());
        let result = self.kb.read_all();
        self.after_io();
        match result {
            Ok(snap) => {
                let sane = self.kb.is_sane();
                self.read = Read::Done { sane };
                if sane {
                    tracing::info!(record = %self.target.record_id, "\"AA\" keyboard read; writes allowed");
                    self.ensure_backup(&snap, app_effect);
                    self.look_at_armed_note(&snap);
                } else {
                    tracing::warn!(why = ?self.kb.sanity_reasons(), "\"AA\" keyboard read; read-only");
                }
            }
            Err(AaError::UpdatePending) => {
                self.read = Read::Done { sane: false };
                tracing::warn!("\"AA\" keyboard waits for a firmware update; read-only");
            }
            // The quiet time after a cut upload: nothing was sent, nothing
            // changes; read again once it is over.
            Err(AaError::UploadInterrupted { wait_ms }) => {
                tracing::debug!(wait_ms, "full read waits for the quiet time after a cut upload");
                self.read_at = None;
                self.quiet_until = Some(Instant::now() + Duration::from_millis(wait_ms));
            }
            Err(e) => {
                tracing::warn!("\"AA\" keyboard not read: {e}");
                self.note_error(&e.to_string());
                self.read = Read::Failed(e.to_string());
                if e.needs_reopen() {
                    self.reopen = Some(e);
                }
            }
        }
    }

    /// The last full read failed in a way that needs the channel opened
    /// again ([`AaError::needs_reopen`]): the error, once.
    pub fn take_reopen(&mut self) -> Option<AaError> {
        self.reopen.take()
    }

    /// [`Self::take_reopen`] as the error that makes the actor drop the
    /// handle.
    fn reopen_needed(&mut self) -> Result<(), Error> {
        match self.reopen.take() {
            Some(e) => Err(Error::Aa(e)),
            None => Ok(()),
        }
    }

    /// Nothing reaches the keyboard now (the quiet time after a cut upload).
    fn quiet(&self, now: Instant) -> bool {
        self.quiet_until.is_some_and(|t| now < t)
    }

    /// The quiet time after a cut upload runs: every command is refused
    /// on purpose (no failure).
    pub fn in_quiet_time(&self) -> bool {
        self.quiet(Instant::now())
    }

    /// Read again when a read did not finish or something since closed the
    /// gate (a reset, an unfinished write); not when the keyboard's own
    /// data failed the check (that needs a reconnect) or it waits for an
    /// update. `background`: at most every [`REREAD_EVERY`].
    fn reread_if_stale(&mut self, background: bool, app_effect: impl FnOnce() -> Option<AaEffect>) {
        if !self.stale() {
            return;
        }
        let due = !background || self.read_at.is_none_or(|t| t.elapsed() >= REREAD_EVERY);
        if due && !self.quiet(Instant::now()) {
            tracing::info!("\"AA\" keyboard read again");
            self.connect(app_effect());
        }
    }

    /// Writes are closed for a reason a new full read may clear: a read
    /// that did not finish, or a reset or an unfinished write since a read
    /// that passed.
    fn stale(&self) -> bool {
        if self.kb.is_sane() || self.update_pending() {
            return false;
        }
        match &self.read {
            Read::NotYet | Read::Failed(_) => true,
            Read::Done { sane } => *sane,
        }
    }

    fn update_pending(&self) -> bool {
        self.kb.info().is_some_and(DeviceInfo::update_pending)
    }

    /// Why nothing is written now, if so.
    pub fn read_only(&self) -> Option<ReadOnly> {
        if self.update_pending() {
            return Some(ReadOnly::UpdatePending);
        }
        if !self.kb.is_sane() {
            let mut why = Vec::new();
            if let Read::Failed(e) = &self.read {
                why.push(e.clone());
            }
            why.extend(self.kb.sanity_reasons());
            return Some(ReadOnly::NotSane(why));
        }
        match &self.backup {
            Some(Ok(_)) => None,
            Some(Err(e)) => Some(ReadOnly::NoBackup(e.clone())),
            None => Some(ReadOnly::NoBackup("not saved yet".into())),
        }
    }

    fn writable(&mut self) -> Result<(), Error> {
        self.reread_if_stale(false, || None);
        self.reopen_needed()?;
        match self.read_only() {
            Some(r) => Err(Error::Refused(r.to_string())),
            None => Ok(()),
        }
    }

    /// The backup of the keyboard as it was before Keymotion wrote anything:
    /// saved once per keyboard (USB product id and firmware version).
    fn ensure_backup(&mut self, snap: &AaSnapshot, app_effect: Option<AaEffect>) {
        if matches!(self.backup, Some(Ok(_))) {
            return;
        }
        let Some(dir) = self.target.data_dir.as_deref().map(backups::dir) else {
            self.backup = Some(Err("the app has no data folder".into()));
            return;
        };
        if let Some(file) = backups::first_of(&dir, self.target.usb) {
            self.backup = Some(Ok(file));
            return;
        }
        let mut snapshot = snap.clone();
        snapshot.effect_color = snapshot.effect_color.or_else(|| app_color(&snapshot, app_effect).map(Into::into));
        let backup = backups::KeyboardBackup::new(&self.target, backups::Reason::FirstConnect, snapshot);
        self.backup = Some(match backups::save(&dir, &backup) {
            Ok(file) => {
                tracing::info!(%file, "keyboard backed up before the first write");
                backups::prune(&dir, self.target.usb, &[]);
                Ok(file)
            }
            Err(e) => {
                tracing::error!("keyboard backup not saved; writes refused: {e}");
                Err(e.to_string())
            }
        });
    }

    /// A note from a run that ended with the keyboard in per-key mode: the
    /// user's effect goes back when it is this keyboard's note and the
    /// keyboard still shows what Keymotion left. Another keyboard's note is
    /// left alone; this keyboard showing something else means the user
    /// chose it since, and the note goes. While live lights of this run
    /// hold the keyboard, the put-back is theirs.
    fn look_at_armed_note(&mut self, snap: &AaSnapshot) {
        let Some(note) = self.my_note(snap) else { return };
        let Some(noted) = note.restore() else {
            self.drop_note();
            return;
        };
        self.noted = Some(note.fingerprint.clone());
        let shows_armed = snap.effect().is_some_and(|now| now.same_settings(&noted.armed));
        if let Live::Armed { restore, .. } = &mut self.live {
            if restore.is_none() && shows_armed {
                *restore = Some(noted);
            }
            return;
        }
        if shows_armed {
            tracing::info!("the keyboard was left in per-key mode for live lights; its effect goes back");
            self.recover = Some((noted, None));
        } else {
            tracing::info!("the keyboard shows another effect than live lights left; the live note goes");
            self.drop_note();
        }
    }

    /// Whether `note` is this keyboard's: same model and USB ids, and the
    /// fingerprint it has now ([`fingerprint`] of `snap`) or the one this
    /// driver noted it under.
    fn is_mine(&self, note: &ArmedRecord, snap: Option<&AaSnapshot>) -> bool {
        let ids = (note.pid, note.release, note.record_id.as_str())
            == (self.target.usb.pid, self.target.usb.release, self.target.record_id.as_str());
        ids && (snap.is_some_and(|s| note.fingerprint == fingerprint(s))
            || self.noted.as_deref() == Some(note.fingerprint.as_str()))
    }

    /// This keyboard's note, if there is one.
    fn my_note(&self, snap: &AaSnapshot) -> Option<ArmedRecord> {
        let dir = self.target.data_dir.as_deref()?;
        armed_notes(dir).into_iter().find(|n| self.is_mine(n, Some(snap)))
    }

    /// What this keyboard's note says goes back, when the keyboard shows
    /// `now`, what Keymotion left (a switch whose read-back differed, a
    /// crash note the full read did not take).
    fn noted_restore(&self, now: &AaEffect) -> Option<Restore> {
        let note = self.my_note(self.kb.snapshot()?)?;
        note.restore().filter(|r| r.armed.same_settings(now))
    }

    /// Note a switch to per-key mode before it goes (crash recovery):
    /// `armed` is the effect block the keyboard shows after it. Other
    /// keyboards' notes stay.
    fn save_note(&mut self, r: Restore, armed: Vec<u8>) -> Result<(), String> {
        let dir = self.target.data_dir.clone().ok_or("the app has no data folder to note the switch in")?;
        let snap = self.kb.snapshot().ok_or("the keyboard was not read")?;
        let note = ArmedRecord {
            record_id: self.target.record_id.clone(),
            pid: self.target.usb.pid,
            release: self.target.usb.release,
            effect: Hex(r.back.encode().map_err(|e| e.to_string())?.to_vec()),
            armed: Hex(armed),
            fingerprint: fingerprint(snap),
            at_ms: now_ms(),
        };
        let mut notes = armed_notes(&dir);
        notes.retain(|n| !self.is_mine(n, Some(snap)));
        self.noted = Some(note.fingerprint.clone());
        notes.push(note);
        cap_notes(&mut notes);
        save_armed_notes(&dir, &notes).map_err(|e| e.to_string())
    }

    /// The note again after a write the switch does not undo (a keymap,
    /// macros): what tells this keyboard apart changed with it.
    fn renote(&mut self) {
        let (Some(dir), Some(snap)) = (self.target.data_dir.clone(), self.kb.snapshot()) else { return };
        let now = fingerprint(snap);
        let mut notes = armed_notes(&dir);
        let Some(note) = notes.iter_mut().find(|n| self.is_mine(n, None)) else { return };
        if note.fingerprint == now {
            return;
        }
        note.fingerprint = now.clone();
        self.noted = Some(now);
        if let Err(e) = save_armed_notes(&dir, &notes) {
            tracing::warn!("live note not updated: {e}");
        }
    }

    /// Drop this keyboard's live note (the effect is back, or is the
    /// user's own); other keyboards' notes stay.
    fn drop_note(&mut self) {
        let Some(dir) = self.target.data_dir.clone() else { return };
        let snap = self.kb.snapshot().cloned();
        let mut notes = armed_notes(&dir);
        let before = notes.len();
        notes.retain(|n| !self.is_mine(n, snap.as_ref()));
        self.noted = None;
        if notes.len() != before {
            if let Err(e) = save_armed_notes(&dir, &notes) {
                tracing::warn!("live note not removed: {e}");
            } else {
                tracing::debug!("live note removed");
            }
        }
    }

    // ------------------------------------------------------------ background

    /// When [`Self::run_due`] has something to do: put the user's effect
    /// back (after the hold, after a crash), or read the keyboard again.
    pub fn next_due(&self) -> Option<Instant> {
        let now = Instant::now();
        let mut due: Vec<Instant> = Vec::new();
        let stale = self.stale();
        // A read-only keyboard takes no write: nothing is due until a read
        // clears it.
        let writable = !stale && self.read_only().is_none();
        if let (Some((_, retry_at)), true) = (self.recover, writable) {
            due.push(retry_at.unwrap_or(now));
        }
        if let Live::Armed { stopped: Some(t), retry_at, .. } = &self.live {
            due.push(retry_at.unwrap_or(*t + LIVE_HOLD));
        }
        if stale {
            let reread = self.read_at.map_or(now, |t| t + REREAD_EVERY);
            due.push(self.quiet_until.map_or(reread, |q| reread.max(q)));
        }
        due.into_iter().min()
    }

    /// Do what is due ([`Self::next_due`]); `ctx.gate` is asked before a
    /// write.
    pub fn run_due(&mut self, now: Instant, ctx: &LiveContext<'_>) -> Result<(), Error> {
        self.reread_if_stale(true, || ctx.app_effect());
        self.reopen_needed()?;
        if let Some((back, retry_at)) = self.recover {
            if retry_at.is_none_or(|t| now >= t) && self.read_only().is_none() {
                let later = Some((back, Some(now + DISARM_RETRY)));
                match ctx.ask() {
                    Some(Ok(())) => match self.put_back(back) {
                        Ok(written) => {
                            if written {
                                tracing::info!("the keyboard's effect is back after the crash");
                            }
                            self.recover = None;
                        }
                        Err(e) => {
                            tracing::warn!("effect not put back after the crash: {e}");
                            self.recover = later;
                            return Err(e);
                        }
                    },
                    Some(Err(why)) => {
                        tracing::debug!("effect waits to be put back: {why}");
                        self.recover = later;
                    }
                    None => {}
                }
            }
        }
        if let Live::Armed { restore, stopped: Some(t), retry_at } = self.live.clone() {
            if now < retry_at.unwrap_or(t + LIVE_HOLD) {
                return Ok(());
            }
            let Some(back) = restore else {
                // It was in a live mode before: nothing to put back.
                self.live = Live::Idle;
                return Ok(());
            };
            match ctx.ask() {
                Some(Ok(())) => match self.put_back(back) {
                    Ok(written) => {
                        if written {
                            tracing::info!("live lights over; the keyboard's effect is back");
                        }
                        self.live = Live::Idle;
                    }
                    Err(e) => {
                        tracing::warn!("effect not put back after live lights: {e}");
                        self.live = Live::Armed { restore, stopped: Some(t), retry_at: Some(now + DISARM_RETRY) };
                        return Err(e);
                    }
                },
                Some(Err(why)) => {
                    tracing::debug!("effect waits to be put back: {why}");
                    self.live = Live::Armed { restore, stopped: Some(t), retry_at: Some(now + DISARM_RETRY) };
                }
                None => {}
            }
        }
        Ok(())
    }

    /// The app is closing: the user's effect goes back now if live lights
    /// hold the keyboard in per-key mode (the hold is not waited). Without
    /// the gate's go-ahead the note stays, and the next connect does it.
    pub fn closing(&mut self, ctx: &LiveContext<'_>) -> Result<(), Error> {
        // From now on nothing is switched to per-key mode.
        self.closed = true;
        let back = match (&self.live, self.recover) {
            (Live::Armed { restore: Some(back), .. }, _) => *back,
            (_, Some((back, _))) => back,
            _ => {
                self.live = Live::Idle;
                return Ok(());
            }
        };
        if self.read_only().is_some() {
            return Ok(());
        }
        match ctx.ask() {
            Some(Ok(())) => {
                if self.put_back(back)? {
                    tracing::info!("closing: the keyboard's effect is back");
                }
                self.live = Live::Idle;
                self.recover = None;
                Ok(())
            }
            _ => {
                tracing::info!("closing: the effect goes back at the next connect");
                Ok(())
            }
        }
    }

    /// The user said yes to writes on this firmware (or something else
    /// that held arming back changed): live lights try again now.
    pub fn retry_live(&mut self) {
        // A refusal by a gate (the untested firmware question) is over: the
        // UI stops saying live lights wait for it, and the next frame asks
        // the gate again.
        if matches!(self.live, Live::Off { why: LiveOff::Blocked(_), .. }) {
            self.live = Live::Idle;
        }
        if let Live::Off { retry_at, .. } = &mut self.live {
            *retry_at = Some(Instant::now());
        }
        if let Some((_, retry_at)) = &mut self.recover {
            *retry_at = None;
        }
        if let Live::Armed { stopped: Some(_), retry_at, .. } = &mut self.live {
            *retry_at = None;
        }
    }

    /// The keyboard's USB ids, as opened.
    pub fn usb(&self) -> UsbIdentity {
        self.target.usb
    }

    /// Live lights hold the keyboard in per-key mode, or a crash left it
    /// there: something goes back when the app closes.
    pub fn holds_keyboard(&self) -> bool {
        matches!(self.live, Live::Armed { restore: Some(_), .. }) || self.recover.is_some()
    }

    /// Write the user's effect back and drop the live note, if the keyboard
    /// still shows what Keymotion left (one read first): an effect chosen
    /// since on the keyboard or in another program stays. `Ok(false)`:
    /// nothing written.
    fn put_back(&mut self, r: Restore) -> Result<bool, Error> {
        let read = self.kb.read_effect();
        self.after_io();
        let now = self.check(read)?;
        if !now.effect.same_settings(&r.armed) {
            tracing::info!("the keyboard shows another effect than live lights left; it stays");
            self.drop_note();
            return Ok(false);
        }
        let read = EffectRead { effect: r.back, color_known: true };
        let written = self.kb.disarm_live(read, Some(r.back.color_part()));
        self.after_io();
        self.check(written)?;
        self.drop_note();
        Ok(true)
    }

    // ------------------------------------------------------------ live lights

    /// A stream starts (plan K-4): a hold running is called off, else the
    /// keyboard is switched to per-key mode when it is not in a live mode.
    pub fn live_starting(&mut self, ctx: &LiveContext<'_>) -> Result<(), Error> {
        let now = Instant::now();
        match &mut self.live {
            Live::Armed { stopped, retry_at, .. } => {
                if stopped.take().is_some() {
                    tracing::debug!("live lights again; the hold is called off");
                }
                *retry_at = None;
                Ok(())
            }
            Live::Idle => self.arm(ctx),
            // A refusal or failure waits its time, whoever asks.
            Live::Off { retry_at: Some(t), .. } if now >= *t => self.arm(ctx),
            Live::Off { .. } => Ok(()),
        }
    }

    /// One frame: shown when the keyboard is in a live mode, else it is
    /// switched first (or tried again when the last try is old enough);
    /// while it may not be, frames are dropped (the UI says why).
    pub fn live_frame(&mut self, frame: Vec<(u8, Rgb)>, ctx: &LiveContext<'_>) -> Result<(), Error> {
        let now = Instant::now();
        match &self.live {
            Live::Armed { .. } => {}
            Live::Idle => self.arm(ctx)?,
            Live::Off { retry_at: Some(t), .. } if now >= *t => self.arm(ctx)?,
            Live::Off { .. } => return Ok(()),
        }
        if !matches!(self.live, Live::Armed { .. }) {
            return Ok(());
        }
        let r = self.kb.realtime_frame(frame);
        self.after_io();
        match r {
            // The effect changed under the stream (another program, a
            // reset): switch again with the next frame.
            Err(AaError::NotArmed) => {
                tracing::debug!("the keyboard left per-key mode during live lights");
                self.live = Live::Idle;
                Ok(())
            }
            r => self.check(r),
        }
    }

    /// The stream ended: the hold starts ([`LIVE_HOLD`]).
    pub fn live_stopped(&mut self) {
        match &mut self.live {
            Live::Armed { stopped, .. } => *stopped = Some(Instant::now()),
            // A refusal or failure keeps its retry time across streams
            // (agents' lights start and stop often; every try is a flash
            // write). One that waits for nothing goes with the stream.
            Live::Off { retry_at: Some(_), .. } => {}
            Live::Off { retry_at: None, .. } => self.live = Live::Idle,
            Live::Idle => {}
        }
    }

    /// Switch to per-key mode for live lights, if allowed and the user's
    /// effect could be put back afterwards.
    fn arm(&mut self, ctx: &LiveContext<'_>) -> Result<(), Error> {
        let now = Instant::now();
        // Closing, or the quiet time after a cut upload (tried again once
        // it is over, nothing shown meanwhile).
        if self.closed || self.quiet(now) {
            return Ok(());
        }
        self.reread_if_stale(true, || ctx.app_effect());
        self.reopen_needed()?;
        if self.read_only().is_some() {
            self.live_off(LiveOff::ReadOnly, Some(now + ARM_RETRY));
            return Ok(());
        }
        let read = self.kb.read_effect();
        self.after_io();
        let current = match self.check(read) {
            Ok(r) => r,
            Err(e) if e.is_noise() => return Ok(()),
            Err(e) => {
                self.live_off(LiveOff::Failed(e.to_string()), Some(now + self.arm_retry()));
                return Err(e);
            }
        };
        if current.effect.is_live() {
            // Already in a live mode: frames show as they are. What goes
            // back afterwards: a note from a crash, or from a switch whose
            // read-back differed.
            let restore = self.recover.take().map(|(back, _)| back).or_else(|| self.noted_restore(&current.effect));
            self.live = Live::Armed { restore, stopped: None, retry_at: None };
            return Ok(());
        }
        let Some(color) = current.color().or_else(|| matching_color(current.effect, ctx.app_effect())) else {
            self.live_off(LiveOff::ColorUnknown, None);
            return Ok(());
        };
        // Asked only now that a write would go: a keyboard in a live mode
        // already, or one whose colour is unknown, asks nothing.
        match ctx.ask() {
            Some(Ok(())) => {}
            Some(Err(why)) => {
                self.live_off(LiveOff::Blocked(why), Some(now + ARM_RETRY));
                return Ok(());
            }
            // No gate: nothing is written; the next frame tries again.
            None => return Ok(()),
        }
        // Both as the keyboard is ever sent them (levels 1–5): the driver
        // writes them so, and the note must hold what goes back.
        let restore = Restore {
            back: levels_in_range(current.effect.with_color(color)),
            armed: levels_in_range(current.effect.per_key()),
        };
        // Noted before the switch goes: a crash, or a switch that lands and
        // then reads back otherwise, still finds what goes back. Whoever
        // waits for the app to close hears it now.
        if let Some(arming) = ctx.arming {
            arming();
        }
        let armed_block = restore.armed.encode().map(|b| b.to_vec()).map_err(|e| e.to_string());
        if let Err(e) = armed_block.and_then(|b| self.save_note(restore, b)) {
            self.live_off(LiveOff::Failed(format!("the switch could not be noted: {e}")), Some(now + ARM_RETRY));
            return Ok(());
        }
        let armed = self.kb.arm_live();
        self.after_io();
        match self.check(armed) {
            Ok(Some(_)) => {
                tracing::info!("keyboard switched to per-key mode for live lights");
                self.arm_failures = 0;
                self.live = Live::Armed { restore: Some(restore), stopped: None, retry_at: None };
                Ok(())
            }
            Ok(None) => {
                self.drop_note();
                self.live = Live::Armed { restore: None, stopped: None, retry_at: None };
                Ok(())
            }
            Err(e) => self.arm_failed(restore, e, now),
        }
    }

    /// The switch to per-key mode failed. If it landed (the keyboard reads
    /// back a live mode, only not exactly what was written), the keyboard
    /// is armed: frames show, and the note now says what it shows, so the
    /// effect still goes back. If the keyboard may have changed but cannot
    /// tell (the gate closed), the note stays for the next connect.
    /// Otherwise the note goes and the next try waits longer.
    fn arm_failed(&mut self, restore: Restore, e: Error, now: Instant) -> Result<(), Error> {
        // A read-back that differed was seen; any other failure (no echo,
        // no read-back, a transport error) leaves open whether the switch
        // landed: one read tells.
        let seen = matches!(&e, Error::Aa(AaError::Readback { .. }));
        let shows = if seen {
            self.kb.current_effect().map(|c| c.effect)
        } else {
            let read = self.kb.read_effect();
            self.after_io();
            match read {
                Ok(r) => Some(r.effect),
                Err(read_error) => {
                    // Still unknown: the note stays, and the background
                    // looks again and puts the effect back if the keyboard
                    // shows per-key mode.
                    tracing::warn!(
                        "switch to per-key mode not confirmed ({e}); its effect may need to go back: {read_error}"
                    );
                    self.recover = Some((restore, Some(now + DISARM_RETRY)));
                    self.live_off(LiveOff::Failed(e.to_string()), Some(now + self.arm_retry()));
                    self.arm_failures += 1;
                    return Err(e);
                }
            }
        };
        match shows {
            Some(live) if live.is_live() && self.kb.is_sane() => {
                tracing::warn!("keyboard switched to per-key mode, but it reads back otherwise: {e}");
                let restore = Restore { armed: live, ..restore };
                // The block the keyboard shows: the read-back's (kept in the
                // snapshot) or the one just read.
                let raw = match (seen, self.kb.snapshot()) {
                    (true, Some(s)) => s.effect.0.clone(),
                    _ => live
                        .encode()
                        .or_else(|_| levels_in_range(live).encode())
                        .map(|b| b.to_vec())
                        .unwrap_or_default(),
                };
                if let Err(e) = self.save_note(restore, raw) {
                    tracing::warn!("live note not updated: {e}");
                }
                self.arm_failures = 0;
                self.live = Live::Armed { restore: Some(restore), stopped: None, retry_at: None };
                Ok(())
            }
            Some(live) if live.is_live() => {
                self.live_off(LiveOff::Failed(e.to_string()), Some(now + self.arm_retry()));
                self.arm_failures += 1;
                Err(e)
            }
            _ => {
                if self.kb.is_sane() {
                    self.drop_note();
                }
                self.live_off(LiveOff::Failed(e.to_string()), Some(now + self.arm_retry()));
                self.arm_failures += 1;
                Err(e)
            }
        }
    }

    /// How long a failed switch waits before the next try: [`ARM_RETRY`],
    /// doubled for each failure in a row, at most [`ARM_RETRY_MAX`].
    fn arm_retry(&self) -> Duration {
        (ARM_RETRY * 2u32.saturating_pow(self.arm_failures.min(8))).min(ARM_RETRY_MAX)
    }

    fn live_off(&mut self, why: LiveOff, retry_at: Option<Instant>) {
        let same = matches!(&self.live, Live::Off { why: w, .. } if *w == why);
        if !same {
            tracing::info!(?why, "live lights not shown on this keyboard");
        }
        self.live = Live::Off { why, retry_at };
    }

    /// The user's own write replaced the effect: nothing to put back any
    /// more (frames that keep coming switch again).
    fn effect_replaced(&mut self) {
        if !matches!(self.live, Live::Idle) || self.recover.is_some() {
            self.drop_note();
        }
        self.recover = None;
        self.live = Live::Idle;
    }

    // ------------------------------------------------------------ writes

    /// Store the app's effect (levels mapped to 1–5). **Writes flash.**
    pub fn apply_effect(&mut self, s: &EffectSettings) -> Result<(), Error> {
        self.writable()?;
        let r = self.kb.apply_effect(s);
        self.after_io();
        self.check(r)?;
        self.effect_replaced();
        Ok(())
    }

    /// Store per-key colours and switch to per-key mode. **Writes flash.**
    pub fn apply_per_key(&mut self, colors: Vec<(u8, Rgb)>, brightness: u8) -> Result<(), Error> {
        self.writable()?;
        let r = self.kb.apply_per_key(colors, brightness);
        self.after_io();
        self.check(r)?;
        self.effect_replaced();
        Ok(())
    }

    /// Make one layer what `map` says (keys not in it: their default), by
    /// changing only the keys that differ from a fresh read. **Writes
    /// flash** (nothing when nothing differs).
    pub fn apply_keymap(&mut self, layer: Layer, map: &LayerMap) -> Result<(), Error> {
        self.apply_keymap_from(layer, map, None).map(|_| ())
    }

    /// [`Self::apply_keymap`], never over a key set in another program or
    /// one of the keyboard's own functions ([`slot_view`]): those keep what
    /// they hold, and how many keys `map` asked to change that way is the
    /// answer. `base`: the [`AaContents::keys_source`] the change was made
    /// against (the Key mapping page's); when the fresh read differs (the
    /// keys changed in another program meanwhile), nothing is written and
    /// the snapshot, now as read, tells the records. **Writes flash.**
    pub fn apply_keymap_from(&mut self, layer: Layer, map: &LayerMap, base: Option<&str>) -> Result<usize, Error> {
        self.writable()?;
        if base.is_some() {
            // The source covers both layers: the other one is read fresh too,
            // so a change made elsewhere on it refuses this save before
            // anything is written (a save of both layers starts with this
            // one).
            let other = match layer {
                Layer::Base => Layer::Fn,
                Layer::Fn => Layer::Base,
            };
            let read = self.kb.read_keymap(other);
            self.after_io();
            self.check(read)?;
        }
        let changes = self.keymap_changes(layer, map);
        self.after_io();
        let (changes, kept) = self.check(changes)?;
        if base.is_some_and(|b| self.keys_source().as_deref() != Some(b)) {
            tracing::info!(?layer, "keys changed on the keyboard since the page read them; nothing written");
            return Err(Error::Refused(ERR_KEYS_CHANGED.into()));
        }
        if changes.is_empty() {
            tracing::debug!(?layer, kept, "keymap layer already as asked; nothing written");
            return Ok(kept);
        }
        tracing::debug!(?layer, keys = changes.len(), kept, "keymap keys to change");
        let r = self.kb.apply_keymap(layer, changes);
        self.after_io();
        let r = self.check(r);
        self.renote();
        r.map(|()| kept)
    }

    /// The keys of `layer` whose entry differs from what `map` asks for, and
    /// how many keys asked for something are kept as they are instead: set
    /// in another program, one of the keyboard's own functions, or a key
    /// whose factory function is one of those, which never comes back. The
    /// Fn key is never among the changes (it is never reassigned).
    fn keymap_changes(&mut self, layer: Layer, map: &LayerMap) -> kb_core::aa::Result<(Vec<(u8, KeyAction)>, usize)> {
        let current = self.kb.read_keymap(layer)?;
        let layout = self.kb.layout().clone();
        let factory: HashMap<u8, [u8; 4]> = match layer {
            Layer::Base => {
                layout.keys().iter().map(|k| (k.led, aa_keymap::entry(layout.factory_base(), k.aa))).collect()
            }
            Layer::Fn => {
                let table = match self.kb.snapshot().map(|s| s.default_fn.0.clone()) {
                    Some(t) if t.len() == aa_keymap::LEN => t,
                    _ => {
                        let read = self.kb.read_default_fn()?;
                        let mut t = vec![0u8; aa_keymap::LEN];
                        for (led, b) in read {
                            if let (Some(aa), Ok(e)) = (layout.aa(led), b.bytes()) {
                                aa_keymap::set_entry(&mut t, aa, e);
                            }
                        }
                        t
                    }
                };
                layout.keys().iter().map(|k| (k.led, aa_keymap::entry(&table, k.aa))).collect()
            }
        };
        let fn_led = layout.fn_slot().and_then(|s| layout.led(s));
        let mut changes = Vec::new();
        let mut kept = 0;
        for (led, now) in current {
            if Some(led) == fn_led {
                continue;
            }
            let (Ok(now), Some(f)) = (now.bytes(), factory.get(&led).copied()) else { continue };
            let want = map.get(&led).copied().unwrap_or(KeyAction::Default);
            let view = slot_view(now, f);
            let same = match (&view, want) {
                (SlotView::Default, KeyAction::Default) => true,
                (SlotView::Action(_), w) => aa_keymap::encode(w).ok() == Some(now),
                _ => false,
            };
            if same {
                continue;
            }
            // Never written over: set in another program, the keyboard's own
            // function, or the factory function of a key that holds another
            // action now (a firmware function is never brought back).
            let never = matches!(view, SlotView::Fixed | SlotView::Builtin)
                || (want == KeyAction::Default && aa_keymap::is_func(f));
            if never {
                if map.contains_key(&led) || want == KeyAction::Default && matches!(view, SlotView::Action(_)) {
                    kept += 1;
                }
                continue;
            }
            changes.push((led, want));
        }
        // Keys this keyboard does not have: the driver refuses them.
        changes.extend(map.iter().filter(|(led, _)| layout.aa(**led).is_none()).map(|(l, a)| (*l, *a)));
        Ok((changes, kept))
    }

    /// Replace every macro with `slots` (an empty one is none), except a
    /// macro the keyboard holds that the app's steps cannot express: where
    /// the app leaves its slot empty, it stays as it is (the Macros page
    /// shows it as kept). **Writes flash.**
    pub fn write_macros(&mut self, slots: &[Vec<MacroStep>]) -> Result<(), Error> {
        self.write_macros_from(slots, None)
    }

    /// [`Self::write_macros`]: a macro made in another program is never
    /// replaced (a slot that asks to is refused, nothing written). `base`:
    /// the [`AaContents::macros_source`] the change was made against; when
    /// the fresh read differs, nothing is written. **Writes flash.**
    pub fn write_macros_from(&mut self, slots: &[Vec<MacroStep>], base: Option<&str>) -> Result<(), Error> {
        self.writable()?;
        let fresh = self.kb.read_macros();
        self.after_io();
        let fresh = self.check(fresh)?;
        if base.is_some_and(|b| self.macros_source().as_deref() != Some(b)) {
            tracing::info!("macros changed on the keyboard since the page read them; nothing written");
            return Err(Error::Refused(ERR_MACROS_CHANGED.into()));
        }
        let made_elsewhere = |m: &Vec<AaMacroEntry>| !m.is_empty() && lossless_steps(m).is_none();
        if let Some(i) = (0..slots.len()).find(|&i| !slots[i].is_empty() && fresh.get(i).is_some_and(made_elsewhere)) {
            return Err(Error::Refused(macro_elsewhere(i)));
        }
        let len = slots.len().max(fresh.iter().rposition(|m| !m.is_empty()).map_or(0, |i| i + 1));
        let mut entries: Vec<Vec<AaMacroEntry>> = Vec::with_capacity(len);
        for i in 0..len {
            let wanted = slots.get(i).filter(|s| !s.is_empty());
            let e = match (wanted, fresh.get(i)) {
                (Some(steps), _) => self.check(aa_macros::from_steps(steps).map_err(AaError::from))?,
                (None, Some(kept)) if made_elsewhere(kept) => kept.clone(),
                _ => Vec::new(),
            };
            entries.push(e);
        }
        while entries.last().is_some_and(Vec::is_empty) {
            entries.pop();
        }
        let r = self.kb.write_macro_entries(&entries);
        self.after_io();
        let r = self.check(r);
        self.renote();
        r
    }

    /// [`AaContents::keys_source`] of the last read (writes and fresh reads
    /// update it).
    pub fn keys_source(&self) -> Option<String> {
        self.kb.snapshot().map(|s| source(&[&s.keymap.0, &s.fn_keymap.0]))
    }

    /// [`AaContents::macros_source`] of the last read.
    pub fn macros_source(&self) -> Option<String> {
        self.kb.snapshot().map(|s| {
            let blocks: Vec<u8> = s.macro_blocks.iter().flat_map(|b| b.bytes.0.iter().copied()).collect();
            source(&[&s.macro_table.0, &blocks])
        })
    }

    /// What the keyboard holds in its keymap layers and macros, as the
    /// app's records keep them, from the last read (writes update it). On
    /// this family the keyboard, not the app's record, is the source of
    /// truth: the Key mapping and Macros pages start from this. `None`
    /// before a read that passed the sanity check.
    pub fn contents(&self) -> Option<AaContents> {
        if !self.kb.is_sane() {
            return None;
        }
        let s = self.kb.snapshot()?;
        let layout = self.kb.layout();
        let full = |t: &Hex| t.0.len() == aa_keymap::LEN;
        if !(full(&s.keymap) && full(&s.fn_keymap) && full(&s.default_fn)) {
            return None;
        }
        let layer = |table: &[u8], factory: &[u8]| {
            let mut out = LayerContents::default();
            for k in layout.keys() {
                if layout.is_protected(k.aa) {
                    continue;
                }
                let (now, f) = (aa_keymap::entry(table, k.aa), aa_keymap::entry(factory, k.aa));
                match slot_view(now, f) {
                    SlotView::Default => {}
                    SlotView::Action(a) => {
                        out.actions.insert(k.led, a);
                        // Its factory function is a firmware function, which
                        // never comes back: no write gives it its default.
                        if aa_keymap::is_func(f) {
                            out.pinned.push(k.led);
                        }
                    }
                    SlotView::Builtin => out.builtin.push(k.led),
                    SlotView::Fixed => out.fixed.push(k.led),
                }
            }
            out
        };
        let macros = s
            .macros()
            .into_iter()
            .map(|e| match (e.is_empty(), lossless_steps(&e)) {
                (true, _) => MacroContents::Empty,
                (false, Some(steps)) => MacroContents::Steps(steps),
                (false, None) => MacroContents::Fixed(e.len()),
            })
            .collect();
        let (keys_source, macros_source) = (self.keys_source()?, self.macros_source()?);
        Some(AaContents {
            base: layer(&s.keymap.0, layout.factory_base()),
            fn_layer: layer(&s.fn_keymap.0, &s.default_fn.0),
            macros,
            fingerprint: format!("{keys_source}:{macros_source}"),
            keys_source,
            macros_source,
        })
    }

    /// Set the keyboard clock.
    pub fn set_clock(&mut self, t: &ClockTime) -> Result<(), Error> {
        self.writable()?;
        let r = self.kb.set_clock(t);
        self.after_io();
        self.check(r)
    }

    /// Upload to the screen over `display` (FF67), `cancel` stopping it
    /// before the next part. **Writes flash**, and may replace the
    /// keyboard's clock page (the caller asked the user; plan K-13).
    pub fn upload_screen(
        &mut self,
        display: &mut AaPort,
        anim: &ScreenAnimation,
        progress: impl FnMut(usize, usize),
        cancel: &AtomicBool,
    ) -> Result<usize, Error> {
        self.writable()?;
        let mut watched = Watched { port: display, failed: false };
        let r = self.kb.upload_screen(&mut watched, anim, progress, cancel);
        self.after_io();
        match r {
            Ok(report) => Ok(report.packets),
            Err(e) => {
                self.noted(&e);
                // The screen channel is dropped after every upload; the
                // command channel (and its cooldown) stays.
                if watched.failed && e.needs_reopen() {
                    Err(Error::AaScreen(e))
                } else {
                    Err(Error::Aa(e))
                }
            }
        }
    }

    /// Write `backup` back (the keyboard's own entries where it cannot be,
    /// listed in the report). The colour of its effect comes from the
    /// backup, else `color`. The state before is backed up first; old
    /// backups are pruned only once something was written, and never
    /// `source` (the file being restored). **Writes flash.**
    pub fn restore(
        &mut self,
        backup: &AaSnapshot,
        color: Option<EffectColor>,
        source: Option<&str>,
    ) -> Result<RestoreReport, Error> {
        self.writable()?;
        // A fresh full read: the gate looks at the keyboard as it is now,
        // and the state before is what gets backed up.
        self.connect(None);
        self.reopen_needed()?;
        if let Some(r) = self.read_only() {
            return Err(Error::Refused(r.to_string()));
        }
        let mut now =
            self.kb.snapshot().cloned().ok_or_else(|| Error::Refused(ReadOnly::NotSane(vec![]).to_string()))?;
        // Live lights hold the keyboard in per-key mode for now: the state
        // before is the user's effect, not that.
        if let Some(r) = self.pending_restore() {
            if let Ok(block) = r.back.encode() {
                now.effect = Hex(block.to_vec());
                now.effect_color = Some(r.back.color_part().into());
            }
        }
        let dir = self
            .target
            .data_dir
            .as_deref()
            .map(backups::dir)
            .ok_or_else(|| Error::Refused(ReadOnly::NoBackup("the app has no data folder".into()).to_string()))?;
        let before = backups::KeyboardBackup::new(&self.target, backups::Reason::BeforeRestore, now);
        let file = backups::save(&dir, &before).map_err(|e| {
            Error::Refused(format!(
                "the keyboard's settings could not be backed up first, so nothing was restored: {e}"
            ))
        })?;
        tracing::info!(%file, "keyboard backed up before a restore");
        let r = self.kb.restore(backup, color);
        self.after_io();
        let protect: Vec<&str> = source.into_iter().chain([file.as_str()]).collect();
        let report = match r {
            Ok(report) => report,
            Err(e) => {
                match &e {
                    // Stopped part-way: the section it stopped in may be
                    // partly written, so the backup from before stays (and
                    // old ones are pruned, as after any restore).
                    AaError::RestoreIncomplete { report, .. } => {
                        if report.written.contains(&"effect") {
                            self.effect_replaced();
                        }
                        backups::prune(&dir, self.target.usb, &protect);
                    }
                    // Refused before anything was written: the backup of
                    // the state before is not needed, and nothing else goes.
                    _ => {
                        if let Err(e) = std::fs::remove_file(dir.join(&file)) {
                            tracing::warn!(%file, "unneeded keyboard backup not removed: {e}");
                        }
                    }
                }
                self.renote();
                return self.check(Err(e));
            }
        };
        backups::prune(&dir, self.target.usb, &protect);
        if report.written.contains(&"effect") {
            self.effect_replaced();
        } else {
            self.renote();
        }
        Ok(report)
    }

    /// The user's effect live lights or a crash note will put back, if any.
    fn pending_restore(&self) -> Option<Restore> {
        match (&self.live, self.recover) {
            (Live::Armed { restore: Some(r), .. }, _) => Some(*r),
            (_, Some((r, _))) => Some(r),
            _ => None,
        }
    }

    // ------------------------------------------------------------ reads

    /// Whether the mirror may ask now: not while the keyboard waits for an
    /// update or left its device information unanswered (the background
    /// read asks again at its own pace), nor in the quiet time after a cut
    /// upload.
    pub fn mirror_ready(&self) -> bool {
        !self.update_pending() && self.kb.info().is_some() && !self.quiet(Instant::now())
    }

    /// The colours the keys show now (`33`), as LED values.
    pub fn mirror_colors(&mut self) -> Result<Vec<(u8, Rgb)>, Error> {
        let r = self.kb.read_live_colors();
        self.after_io();
        self.check(r)
    }

    /// The device information (`10`), as the app shows it. A keyboard
    /// waiting for a firmware update reports that, not an error.
    pub fn device_info(&mut self) -> Result<DeviceReport, Error> {
        let r = self.kb.device_info();
        self.after_io();
        match r {
            Ok(i) => Ok(device_report(&i)),
            Err(AaError::UpdatePending) => Ok(self.kb.info().map(device_report).unwrap_or_default()),
            Err(e) => self.check(Err(e)),
        }
    }

    // ------------------------------------------------------------ state

    /// Replies to requests this driver did not make: the keyboard's own
    /// notices (a reset, sleep, a status report) are not counted.
    fn stray_replies(&self) -> u64 {
        self.kb.unexpected_reports().saturating_sub(self.kb.notices_reported())
    }

    fn after_io(&mut self) {
        if self.conflict.note(Instant::now(), self.stray_replies()) {
            if self.conflict.suspected {
                tracing::warn!(
                    unexpected = self.stray_replies(),
                    "another program seems to be talking to the keyboard (replies nobody asked for)"
                );
            } else {
                tracing::info!("no more unexpected replies from the keyboard");
            }
        }
    }

    fn note_error(&mut self, e: &str) {
        if self.errors.len() == ERRORS_KEPT {
            self.errors.pop_front();
        }
        self.errors.push_back((now_ms(), e.to_string()));
    }

    /// A driver result as the app's, the error noted. The quiet time after
    /// a cut upload is no error to note (it would push the upload's own out
    /// of the list): it is remembered, and nothing is tried before its end.
    fn check<T>(&mut self, r: kb_core::aa::Result<T>) -> Result<T, Error> {
        r.map_err(|e| {
            self.noted(&e);
            Error::Aa(e)
        })
    }

    fn noted(&mut self, e: &AaError) {
        match e {
            AaError::UploadInterrupted { wait_ms } => {
                self.quiet_until = Some(Instant::now() + Duration::from_millis(*wait_ms));
            }
            e => self.note_error(&e.to_string()),
        }
    }

    /// What the UI shows about this keyboard.
    pub fn status(&self) -> AaStatus {
        let (live_armed, live_lights_off) = match &self.live {
            Live::Armed { .. } => (true, None),
            Live::Off { why, .. } => (false, Some(why.into())),
            Live::Idle => (false, None),
        };
        AaStatus {
            read_only: self.read_only().as_ref().map(|r| {
                let mut shown = ReadOnlyReason::from(r);
                // A read that did not finish (another program holding the
                // keyboard?) is tried again; data that failed the check is
                // worth a report.
                if let ReadOnlyReason::NotSane { read_failed, .. } = &mut shown {
                    *read_failed = matches!(self.read, Read::Failed(_));
                }
                shown
            }),
            live_lights_off,
            live_armed,
            // Armed on a keyboard that was in a live mode already (per-key
            // lighting saved from Keymotion): nothing goes back.
            live_restores: matches!(self.live, Live::Armed { restore: Some(_), .. }),
            other_program_suspected: self.conflict.suspected,
            backup: self.backup.as_ref().and_then(|b| b.as_ref().ok().cloned()),
            effect_color_known: self.kb.current_effect().is_some_and(|e| e.color_known),
        }
    }

    /// The diagnostics report's section about this keyboard: device
    /// information, the sanity check, the counters and the last errors. No
    /// keymap, macro or colour content.
    pub fn diagnostics(&self) -> Vec<String> {
        let mut out = vec![format!(
            "{} on USB {:04X}, firmware {}",
            self.target.record_id,
            self.target.usb.pid,
            kb_devices::firmware_text(self.target.usb.release)
        )];
        match self.kb.snapshot() {
            Some(s) => {
                out.push(format!("device information: {}", hex(&s.info.0)));
                let d = &s.device;
                out.push(format!(
                    "parsed: vid {:04X} pid {:04X} version {:04X} product {:04X} work mode {} battery {} charge {} \
                     macro space {} tft frames {} direction {} frame version {} lighting version {} firmware \
                     status {} profile {}/{}",
                    d.vid,
                    d.pid,
                    d.version,
                    d.product,
                    d.work_mode,
                    d.battery_level,
                    d.charge_status,
                    d.macro_space_size,
                    d.tft_max_frames,
                    d.tft_direction,
                    d.frame_version,
                    d.lighting_version,
                    d.firmware_status,
                    d.current_profile,
                    d.max_profile
                ));
                out.push(format!(
                    "sections read: device information, options, base layer, Fn layer, factory Fn layer, effect, \
                     per-key table, macro table and {} macro block(s)",
                    s.macro_blocks.len()
                ));
            }
            None => match self.kb.info() {
                Some(i) => out.push(format!("device information only: {i:?}")),
                None => out.push("nothing read".into()),
            },
        }
        out.push(match &self.read {
            Read::NotYet => "full read: not yet".into(),
            Read::Done { sane: true } => "full read: done, passed the sanity check".into(),
            Read::Done { sane: false } => "full read: done, did not pass the sanity check".into(),
            Read::Failed(e) => format!("full read: failed ({e})"),
        });
        out.push(match self.read_only() {
            None => "writes: allowed".into(),
            Some(r) => format!("writes: refused ({r})"),
        });
        out.push(format!(
            "effect colour known: {}",
            if self.kb.current_effect().is_some_and(|e| e.color_known) { "yes" } else { "no" }
        ));
        out.push(format!(
            "unexpected replies: {}, resets reported: {}, another program suspected: {}",
            self.kb.unexpected_reports(),
            self.kb.resets_reported(),
            self.conflict.suspected
        ));
        out.push(match &self.live {
            Live::Idle => "live lights: none".into(),
            Live::Armed { restore, stopped, .. } => format!(
                "live lights: per-key mode{}{}",
                if restore.is_some() { ", effect to put back" } else { "" },
                if stopped.is_some() { ", hold running" } else { "" }
            ),
            Live::Off { why, .. } => format!("live lights: not shown ({why:?})"),
        });
        if self.recover.is_some() {
            out.push("an effect waits to be put back after a crash".into());
        }
        out.push(match &self.backup {
            Some(Ok(f)) => format!("backup: {f}"),
            Some(Err(e)) => format!("backup: failed ({e})"),
            None => "backup: none yet".into(),
        });
        for (at, e) in &self.errors {
            out.push(format!("error at {at}: {e}"));
        }
        out
    }
}

/// The colour of `effect` as the app stored it: the app's effect, when its
/// mode and levels are the keyboard's.
/// One keymap layer as the keyboard holds it, for the app's records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayerContents {
    /// Keys not at their factory function whose action the app's records
    /// hold.
    pub actions: LayerMap,
    /// Keys set to something the app can neither show nor write (a
    /// firmware function, a combo, an entry for AJAZZ's host program...):
    /// every keymap write keeps them as they are.
    pub fixed: Vec<u8>,
    /// Keys at one of the keyboard's own functions in this layer (a
    /// factory reset, a mode switch: plan K-15): shown as such, never
    /// written.
    pub builtin: Vec<u8>,
    /// Keys in `actions` whose factory function in this layer is one of
    /// the keyboard's own: they take another action, never their default
    /// back (a write asking for it keeps them as they are).
    pub pinned: Vec<u8>,
}

/// One macro slot as the keyboard holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroContents {
    Empty,
    /// Steps that go back to the very same bytes.
    Steps(Vec<MacroStep>),
    /// Entries the app's steps cannot express (a wheel turn, a pause under
    /// 5 ms, an unknown flag), and how many: every macro write keeps them.
    Fixed(usize),
}

/// What the keyboard holds in its keymap layers and macros
/// ([`AaDriver::contents`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AaContents {
    pub base: LayerContents,
    pub fn_layer: LayerContents,
    /// 100 slots.
    pub macros: Vec<MacroContents>,
    /// Changes whenever any of the above does.
    pub fingerprint: String,
    /// Tells the keymap tables apart: a keymap save made against another
    /// one is refused ([`AaDriver::apply_keymap_from`]).
    pub keys_source: String,
    /// The same for the macros ([`AaDriver::write_macros_from`]).
    pub macros_source: String,
}

/// How the app's records see a keymap slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotView {
    /// Its factory function (`00 00 00 00` is the same).
    Default,
    /// An action the records hold and the app writes.
    Action(KeyAction),
    /// One of the keyboard's own functions (a firmware function), at its
    /// factory place: never written (plan K-15).
    Builtin,
    /// Something the app can neither show nor write (set in another
    /// program): never written.
    Fixed,
}

fn slot_view(now: [u8; 4], factory: [u8; 4]) -> SlotView {
    // A firmware that keeps zeros in a table means the factory function.
    let now = if now == [0; 4] { factory } else { now };
    if now == factory {
        return if aa_keymap::is_func(factory) { SlotView::Builtin } else { SlotView::Default };
    }
    match aa_keymap::decode(now) {
        Binding::Action(a) if a != KeyAction::Default && a.encode().is_ok() && !aa_keymap::is_func(now) => {
            SlotView::Action(a)
        }
        _ => SlotView::Fixed,
    }
}

/// A short hex tag of `parts` (FNV-1a over their lengths and bytes).
fn source(parts: &[&[u8]]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        let len = (part.len() as u64).to_le_bytes();
        for b in len.iter().chain(part.iter()) {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

/// A keymap save made against keys the keyboard no longer holds.
pub const ERR_KEYS_CHANGED: &str =
    "the keyboard's keys changed in another program since Keymotion read them; nothing was written";
/// A macro save made against macros the keyboard no longer holds.
pub const ERR_MACROS_CHANGED: &str =
    "the keyboard's macros changed in another program since Keymotion read them; nothing was written";

/// A write that would replace a macro made in another program (slot `i`).
pub fn macro_elsewhere(i: usize) -> String {
    format!("macro {} on the keyboard was made in another program and is not replaced; nothing was written", i + 1)
}

/// A macro's entries as the app's steps, when the steps give back the very
/// same entries.
fn lossless_steps(entries: &[AaMacroEntry]) -> Option<Vec<MacroStep>> {
    let steps = aa_macros::to_steps(entries)?;
    (aa_macros::from_steps(&steps).ok()? == entries).then_some(steps)
}

fn matching_color(effect: AaEffect, app: Option<AaEffect>) -> Option<EffectColor> {
    app.filter(|a| a.same_settings(&effect)).map(|a| a.color_part())
}

/// The colour a backup's effect had, from the app's record.
fn app_color(s: &AaSnapshot, app: Option<AaEffect>) -> Option<EffectColor> {
    s.effect().and_then(|e| matching_color(e, app))
}

/// The device information as the app shows it. Only `1` is taken as
/// "charging": what the other charge values mean is not known.
pub fn device_report(i: &DeviceInfo) -> DeviceReport {
    DeviceReport {
        battery: (i.battery_level <= 100).then_some(i.battery_level),
        charge: (i.charge_status == 1).then_some(ChargeState::Charging),
        screen_max_frames: (i.tft_max_frames > 0).then_some(u32::from(i.tft_max_frames)),
        update_pending: i.update_pending(),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// The live notes file: one note per keyboard left in per-key mode.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ArmedNotes {
    notes: Vec<ArmedRecord>,
}

/// Notes kept per model and USB ids: one whose keyboard changed while the
/// app was not running (its fingerprint with it) is never recognised again,
/// so the oldest go.
const NOTES_KEPT: usize = 4;

/// Keep the newest [`NOTES_KEPT`] notes of each model and USB ids (the list
/// is oldest first).
fn cap_notes(notes: &mut Vec<ArmedRecord>) {
    let mut seen: HashMap<(String, u16, u16), usize> = HashMap::new();
    let mut keep = vec![false; notes.len()];
    for (i, n) in notes.iter().enumerate().rev() {
        let count = seen.entry((n.record_id.clone(), n.pid, n.release)).or_default();
        *count += 1;
        keep[i] = *count <= NOTES_KEPT;
    }
    let mut keep = keep.into_iter();
    notes.retain(|_| keep.next().unwrap_or(true));
}

/// Every keyboard's live note (an unreadable file is kept aside by
/// `settings::load` and reads as none).
fn armed_notes(dir: &Path) -> Vec<ArmedRecord> {
    crate::settings::load::<ArmedNotes>(dir, ARMED_FILE, ARMED_VERSION).map(|n| n.notes).unwrap_or_default()
}

fn save_armed_notes(dir: &Path, notes: &[ArmedRecord]) -> std::io::Result<()> {
    if notes.is_empty() {
        return match std::fs::remove_file(dir.join(ARMED_FILE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    crate::settings::save(dir, ARMED_FILE, ARMED_VERSION, ArmedNotes { notes: notes.to_vec() })
}

/// The live note of the simulated keyboard, if any (tests).
#[cfg(test)]
fn armed_note(dir: &Path) -> Option<ArmedRecord> {
    armed_notes(dir).into_iter().next()
}

/// Backups of "AA" keyboards in the app's data folder
/// (`keyboard-backups/`): one before Keymotion first writes to a keyboard
/// (per USB product id and firmware version), one before every restore.
/// At most [`KEPT`] per keyboard: the first is always kept, then the
/// newest.
pub mod backups {
    use super::{AaTarget, Deserialize, Path, PathBuf, Serialize};
    use kb_core::aa::{AaSnapshot, UsbIdentity};
    use std::io;

    pub const DIR: &str = "keyboard-backups";
    const VERSION: u32 = 1;
    /// Backups kept per keyboard.
    pub const KEPT: usize = 5;

    pub fn dir(data_dir: &Path) -> PathBuf {
        data_dir.join(DIR)
    }

    /// Why a backup was taken.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum Reason {
        /// Before Keymotion first wrote to the keyboard.
        FirstConnect,
        /// Before a restore overwrote what the keyboard held.
        BeforeRestore,
    }

    /// One backup file.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct KeyboardBackup {
        pub saved_at_ms: u64,
        pub reason: Reason,
        pub record_id: String,
        pub pid: u16,
        /// Firmware version (USB `bcdDevice`).
        pub release: u16,
        pub snapshot: AaSnapshot,
    }

    impl KeyboardBackup {
        pub fn new(target: &AaTarget, reason: Reason, snapshot: AaSnapshot) -> Self {
            Self {
                saved_at_ms: super::now_ms(),
                reason,
                record_id: target.record_id.clone(),
                pid: target.usb.pid,
                release: target.usb.release,
                snapshot,
            }
        }

        pub fn usb(&self) -> UsbIdentity {
            UsbIdentity { pid: self.pid, release: self.release }
        }
    }

    #[derive(Deserialize)]
    struct Stored {
        version: u32,
        #[serde(flatten)]
        value: KeyboardBackup,
    }

    /// A backup file's name: `aa-<pid>-<release>-<ms>.json`. Anything else
    /// is not opened (path safety).
    fn valid_name(name: &str) -> bool {
        let Some(stem) = name.strip_prefix("aa-").and_then(|n| n.strip_suffix(".json")) else { return false };
        let parts: Vec<&str> = stem.split('-').collect();
        matches!(parts.as_slice(), [pid, release, at]
            if pid.len() == 4 && release.len() == 4
                && pid.bytes().chain(release.bytes()).all(|b| b.is_ascii_hexdigit())
                && !at.is_empty() && at.bytes().all(|b| b.is_ascii_digit()))
    }

    /// Read one backup by file name.
    pub fn load(dir: &Path, name: &str) -> Result<KeyboardBackup, String> {
        if !valid_name(name) {
            return Err("not a keyboard backup".into());
        }
        let text = std::fs::read_to_string(dir.join(name)).map_err(|e| format!("backup not read: {e}"))?;
        match serde_json::from_str::<Stored>(&text) {
            Ok(s) if s.version == VERSION => Ok(s.value),
            Ok(s) => Err(format!("backup format {} is not known", s.version)),
            Err(e) => Err(format!("backup damaged: {e}")),
        }
    }

    /// Every readable backup, oldest first.
    pub fn list(dir: &Path) -> Vec<(String, KeyboardBackup)> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut out: Vec<(String, KeyboardBackup)> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if !valid_name(&name) {
                    return None;
                }
                match load(dir, &name) {
                    Ok(b) => Some((name, b)),
                    Err(e) => {
                        tracing::warn!(file = %name, "{e}");
                        None
                    }
                }
            })
            .collect();
        out.sort_by_key(|(name, b)| (b.saved_at_ms, name.clone()));
        out
    }

    /// The first backup of the keyboard with these ids (the oldest taken
    /// before Keymotion first wrote to it, whatever the clock said), if any.
    pub fn first_of(dir: &Path, usb: UsbIdentity) -> Option<String> {
        list(dir).into_iter().find(|(_, b)| b.usb() == usb && b.reason == Reason::FirstConnect).map(|(name, _)| name)
    }

    /// Save a backup; returns its file name. Nothing is pruned here
    /// ([`prune`]).
    pub fn save(dir: &Path, backup: &KeyboardBackup) -> io::Result<String> {
        let mut at = backup.saved_at_ms;
        let name = loop {
            let name = format!("aa-{:04x}-{:04x}-{at}.json", backup.pid, backup.release);
            if !dir.join(&name).exists() {
                break name;
            }
            at += 1;
        };
        crate::settings::save(dir, &name, VERSION, backup)?;
        Ok(name)
    }

    /// Remove the oldest backups of the keyboard with these ids beyond
    /// [`KEPT`]; the first ([`first_of`]) and `protect` always stay.
    pub fn prune(dir: &Path, usb: UsbIdentity, protect: &[&str]) {
        let first = first_of(dir, usb);
        let mine: Vec<String> = list(dir).into_iter().filter(|(_, b)| b.usb() == usb).map(|(n, _)| n).collect();
        let mut left = mine.len();
        // Oldest first.
        for name in &mine {
            if left <= KEPT {
                break;
            }
            if first.as_deref() == Some(name.as_str()) || protect.contains(&name.as_str()) {
                continue;
            }
            match std::fs::remove_file(dir.join(name)) {
                Ok(()) => {
                    tracing::info!(file = %name, "old keyboard backup removed");
                    left -= 1;
                }
                Err(e) => tracing::warn!(file = %name, "old keyboard backup not removed: {e}"),
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
