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
    AaEffect, AaError, AaKeyboard, AaLayout, AaSnapshot, DeviceInfo, EffectColor, EffectRead, Hex, OutputPort,
    RestoreReport, UsbIdentity,
};
use kb_core::keymap::LayerMap;
use kb_devices::Model;
use kb_protocol::aa::keymap as aa_keymap;
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
/// this long while frames keep coming.
const ARM_RETRY: Duration = Duration::from_secs(30);
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
const ARMED_VERSION: u32 = 1;

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
            ReadOnly::NotSane(reasons) => Self::NotSane { reasons: reasons.clone() },
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

#[derive(Debug, Clone)]
enum Live {
    /// No live lights on the keyboard; the user's effect shows.
    Idle,
    /// The keyboard is in a live mode. `restore`: the user's effect to put
    /// back (`None`: it was in a live mode already); `stopped`: when the
    /// last stream ended (the hold runs).
    Armed { restore: Option<AaEffect>, stopped: Option<Instant>, retry_at: Option<Instant> },
    /// Not switched for this stream; tried again at `retry_at` (`None`:
    /// with the next stream).
    Off { why: LiveOff, retry_at: Option<Instant> },
}

/// The note of an armed keyboard (crash recovery): which keyboard, and the
/// effect to put back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArmedRecord {
    record_id: String,
    pid: u16,
    release: u16,
    /// The effect block to put back, colour included.
    effect: Hex,
    at_ms: u64,
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
    /// Note the driver's count now; true when the hint changed.
    fn note(&mut self, now: Instant, count: u64) -> bool {
        self.samples.push_back((now, count));
        while self.samples.front().is_some_and(|(t, _)| now.duration_since(*t) > CONFLICT_WINDOW) {
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
    recover: Option<(AaEffect, Option<Instant>)>,
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
            Err(e) => {
                tracing::warn!("\"AA\" keyboard not read: {e}");
                self.note_error(&e.to_string());
                self.read = Read::Failed(e.to_string());
            }
        }
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
        if due {
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
                Ok(file)
            }
            Err(e) => {
                tracing::error!("keyboard backup not saved; writes refused: {e}");
                Err(e.to_string())
            }
        });
    }

    /// A note from a run that ended with the keyboard in a live mode: put
    /// the user's effect back when the keyboard still shows a live mode;
    /// otherwise the note is old news.
    fn look_at_armed_note(&mut self, snap: &AaSnapshot) {
        let Some(dir) = self.target.data_dir.clone() else { return };
        let Some(note) = read_armed(&dir) else { return };
        if (note.pid, note.release, note.record_id.as_str())
            != (self.target.usb.pid, self.target.usb.release, self.target.record_id.as_str())
        {
            return;
        }
        let effect = AaEffect::decode(&note.effect.0).ok();
        match (effect, snap.effect()) {
            (Some(back), Some(now)) if now.is_live() && !back.is_live() => {
                tracing::info!("the keyboard was left in per-key mode for live lights; its effect goes back");
                self.recover = Some((back, None));
            }
            _ => remove_armed(&dir),
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
            due.push(self.read_at.map_or(now, |t| t + REREAD_EVERY));
        }
        due.into_iter().min()
    }

    /// Do what is due ([`Self::next_due`]); `ctx.gate` is asked before a
    /// write.
    pub fn run_due(&mut self, now: Instant, ctx: &LiveContext<'_>) -> Result<(), Error> {
        self.reread_if_stale(true, || ctx.app_effect());
        if let Some((back, retry_at)) = self.recover {
            if retry_at.is_none_or(|t| now >= t) && self.read_only().is_none() {
                let later = Some((back, Some(now + DISARM_RETRY)));
                match ctx.ask() {
                    Some(Ok(())) => match self.put_back(back) {
                        Ok(()) => {
                            tracing::info!("the keyboard's effect is back after the crash");
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
                    Ok(()) => {
                        tracing::info!("live lights over; the keyboard's effect is back");
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
        let back = match (&self.live, self.recover) {
            (Live::Armed { restore: Some(back), .. }, _) => *back,
            (_, Some((back, _))) => back,
            _ => return Ok(()),
        };
        if self.read_only().is_some() {
            return Ok(());
        }
        match ctx.ask() {
            Some(Ok(())) => {
                self.put_back(back)?;
                tracing::info!("closing: the keyboard's effect is back");
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

    /// Write the user's effect back and drop the live note.
    fn put_back(&mut self, back: AaEffect) -> Result<(), Error> {
        let read = EffectRead { effect: back, color_known: true };
        let r = self.kb.disarm_live(read, Some(back.color_part()));
        self.after_io();
        self.check(r)?;
        if let Some(dir) = &self.target.data_dir {
            remove_armed(dir);
        }
        Ok(())
    }

    // ------------------------------------------------------------ live lights

    /// A stream starts (plan K-4): a hold running is called off, else the
    /// keyboard is switched to per-key mode when it is not in a live mode.
    pub fn live_starting(&mut self, ctx: &LiveContext<'_>) -> Result<(), Error> {
        match &mut self.live {
            Live::Armed { stopped, retry_at, .. } => {
                if stopped.take().is_some() {
                    tracing::debug!("live lights again; the hold is called off");
                }
                *retry_at = None;
                Ok(())
            }
            Live::Idle | Live::Off { .. } => self.arm(ctx),
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
            Live::Off { .. } => self.live = Live::Idle,
            Live::Idle => {}
        }
    }

    /// Switch to per-key mode for live lights, if allowed and the user's
    /// effect could be put back afterwards.
    fn arm(&mut self, ctx: &LiveContext<'_>) -> Result<(), Error> {
        let now = Instant::now();
        self.reread_if_stale(true, || ctx.app_effect());
        if self.read_only().is_some() {
            self.live_off(LiveOff::ReadOnly, Some(now + ARM_RETRY));
            return Ok(());
        }
        let read = self.kb.read_effect();
        self.after_io();
        let current = match self.check(read) {
            Ok(r) => r,
            Err(e) => {
                self.live_off(LiveOff::Failed(e.to_string()), Some(now + ARM_RETRY));
                return Err(e);
            }
        };
        if current.effect.is_live() {
            // Already in a live mode: frames show as they are. A note from
            // a crash names the effect to put back afterwards.
            let restore = self.recover.take().map(|(back, _)| back);
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
        let restore = current.effect.with_color(color);
        let Some(dir) = self.target.data_dir.clone() else {
            self.live_off(LiveOff::Failed("the app has no data folder to note the switch in".into()), None);
            return Ok(());
        };
        let note = ArmedRecord {
            record_id: self.target.record_id.clone(),
            pid: self.target.usb.pid,
            release: self.target.usb.release,
            effect: Hex(restore.encode().map_err(|e| Error::Aa(e.into()))?.to_vec()),
            at_ms: now_ms(),
        };
        if let Err(e) = crate::settings::save(&dir, ARMED_FILE, ARMED_VERSION, &note) {
            self.live_off(LiveOff::Failed(format!("the switch could not be noted: {e}")), Some(now + ARM_RETRY));
            return Ok(());
        }
        let armed = self.kb.arm_live();
        self.after_io();
        match self.check(armed) {
            Ok(Some(_)) => {
                tracing::info!("keyboard switched to per-key mode for live lights");
                self.live = Live::Armed { restore: Some(restore), stopped: None, retry_at: None };
                Ok(())
            }
            Ok(None) => {
                remove_armed(&dir);
                self.live = Live::Armed { restore: None, stopped: None, retry_at: None };
                Ok(())
            }
            Err(e) => {
                // Keep the note when the effect may have changed: the next
                // connect looks at what the keyboard shows.
                if !self.kb.current_effect().is_some_and(|c| c.effect.is_live()) && self.kb.is_sane() {
                    remove_armed(&dir);
                }
                self.live_off(LiveOff::Failed(e.to_string()), Some(now + ARM_RETRY));
                Err(e)
            }
        }
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
            if let Some(dir) = &self.target.data_dir {
                remove_armed(dir);
            }
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
        self.writable()?;
        let changes = self.keymap_changes(layer, map);
        let changes = self.check(changes)?;
        if changes.is_empty() {
            tracing::debug!(?layer, "keymap layer already as asked; nothing written");
            return Ok(());
        }
        tracing::debug!(?layer, keys = changes.len(), "keymap keys to change");
        let r = self.kb.apply_keymap(layer, changes);
        self.after_io();
        self.check(r)
    }

    /// The keys of `layer` whose entry differs from what `map` asks for.
    /// The Fn key is never among them (it is never reassigned).
    fn keymap_changes(&mut self, layer: Layer, map: &LayerMap) -> kb_core::aa::Result<Vec<(u8, KeyAction)>> {
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
        for (led, now) in current {
            if Some(led) == fn_led {
                continue;
            }
            let want = map.get(&led).copied().unwrap_or(KeyAction::Default);
            let now = now.bytes().ok();
            let same = match want {
                KeyAction::Default => now.is_some() && now == factory.get(&led).copied(),
                a => aa_keymap::encode(a).ok().is_some_and(|e| Some(e) == now),
            };
            if !same {
                changes.push((led, want));
            }
        }
        // Keys this keyboard does not have: the driver refuses them.
        changes.extend(map.iter().filter(|(led, _)| layout.aa(**led).is_none()).map(|(l, a)| (*l, *a)));
        Ok(changes)
    }

    /// Replace every macro. **Writes flash.**
    pub fn write_macros(&mut self, slots: &[Vec<MacroStep>]) -> Result<(), Error> {
        self.writable()?;
        let r = self.kb.write_macros(slots);
        self.after_io();
        self.check(r)
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
                self.note_error(&e.to_string());
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
    /// backup, else `color`. The state before is backed up first.
    /// **Writes flash.**
    pub fn restore(&mut self, backup: &AaSnapshot, color: Option<EffectColor>) -> Result<RestoreReport, Error> {
        self.writable()?;
        // A fresh full read: the gate looks at the keyboard as it is now,
        // and the state before is what gets backed up.
        self.connect(None);
        if let Some(r) = self.read_only() {
            return Err(Error::Refused(r.to_string()));
        }
        let now = self.kb.snapshot().cloned().ok_or_else(|| Error::Refused(ReadOnly::NotSane(vec![]).to_string()))?;
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
        let report = match r {
            Ok(report) => report,
            Err(e) => {
                match &e {
                    AaError::RestoreIncomplete { report, .. } => {
                        if report.written.contains(&"effect") {
                            self.effect_replaced();
                        }
                    }
                    // Refused before anything was written: the backup of
                    // the state before is not needed.
                    _ => {
                        if let Err(e) = std::fs::remove_file(dir.join(&file)) {
                            tracing::warn!(%file, "unneeded keyboard backup not removed: {e}");
                        }
                    }
                }
                return self.check(Err(e));
            }
        };
        if report.written.contains(&"effect") {
            self.effect_replaced();
        }
        Ok(report)
    }

    // ------------------------------------------------------------ reads

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

    fn after_io(&mut self) {
        if self.conflict.note(Instant::now(), self.kb.unexpected_reports()) {
            if self.conflict.suspected {
                tracing::warn!(
                    unexpected = self.kb.unexpected_reports(),
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

    /// A driver result as the app's, the error noted.
    fn check<T>(&mut self, r: kb_core::aa::Result<T>) -> Result<T, Error> {
        r.map_err(|e| {
            self.note_error(&e.to_string());
            Error::Aa(e)
        })
    }

    /// What the UI shows about this keyboard.
    pub fn status(&self) -> AaStatus {
        let (live_armed, live_lights_off) = match &self.live {
            Live::Armed { .. } => (true, None),
            Live::Off { why, .. } => (false, Some(why.into())),
            Live::Idle => (false, None),
        };
        AaStatus {
            read_only: self.read_only().as_ref().map(Into::into),
            live_lights_off,
            live_armed,
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

#[derive(Deserialize)]
struct StoredArmed {
    version: u32,
    #[serde(flatten)]
    value: ArmedRecord,
}

fn read_armed(dir: &Path) -> Option<ArmedRecord> {
    let text = std::fs::read_to_string(dir.join(ARMED_FILE)).ok()?;
    match serde_json::from_str::<StoredArmed>(&text) {
        Ok(s) if s.version == ARMED_VERSION => Some(s.value),
        _ => {
            tracing::warn!("{ARMED_FILE} unreadable; removed");
            remove_armed(dir);
            None
        }
    }
}

fn remove_armed(dir: &Path) {
    match std::fs::remove_file(dir.join(ARMED_FILE)) {
        Ok(()) => tracing::debug!("live note removed"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("live note not removed: {e}"),
    }
}

/// Whether the app holds a live note (tests, diagnostics).
#[cfg(test)]
fn armed_note(dir: &Path) -> Option<ArmedRecord> {
    read_armed(dir)
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

    /// The first backup of the keyboard with these ids, if any.
    pub fn first_of(dir: &Path, usb: UsbIdentity) -> Option<String> {
        list(dir).into_iter().find(|(_, b)| b.usb() == usb).map(|(name, _)| name)
    }

    /// Save a backup; returns its file name. Older ones of the same
    /// keyboard beyond [`KEPT`] go (never the first).
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
        prune(dir, backup.usb());
        Ok(name)
    }

    fn prune(dir: &Path, usb: UsbIdentity) {
        let mine: Vec<String> = list(dir).into_iter().filter(|(_, b)| b.usb() == usb).map(|(n, _)| n).collect();
        if mine.len() <= KEPT {
            return;
        }
        // Oldest first: keep the first, drop the oldest of the others.
        for name in &mine[1..mine.len() - (KEPT - 1)] {
            match std::fs::remove_file(dir.join(name)) {
                Ok(()) => tracing::info!(file = %name, "old keyboard backup removed"),
                Err(e) => tracing::warn!(file = %name, "old keyboard backup not removed: {e}"),
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
