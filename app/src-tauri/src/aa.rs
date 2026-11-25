//! The "AA" family (AK980 PRO on firmware 769) in the app: the driver arm
//! the device actor talks to ([`AaDriver`]), and what the app keeps around
//! it (plan K-3 to K-10, notes: aa-surucu-plani.md):
//!
//! * **Connect.** Opening the command channel reads everything
//!   (`kb_core::aa::AaKeyboard::read_all`). A keyboard that does not pass
//!   the sanity check, or could not be read in full, stays read-only
//!   ([`ReadOnly`]). A sane one gets a backup of everything it holds saved
//!   to the app's data folder ([`backups`]) before anything can be written;
//!   no backup, no writes.
//! * **Live lights** are real-time frames, RAM only: firmware 769 shows
//!   them in any effect mode and its own effect again about 5 s after the
//!   last one. Nothing is switched, noted, held or put back, and a
//!   read-only keyboard shows them too; only while the keyboard ignores
//!   writes (just switched on or reset) are they held back
//!   ([`LiveOff::Blocked`]).
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
use crate::dto::{AaReadBack, AaStatus, ChargeState, DeviceReport, LiveLightsOff, ReadOnlyReason};
use kb_core::aa::{
    AaEffect, AaError, AaKeyboard, AaLayout, AaMacroEntry, AaSnapshot, Binding, DeviceInfo, EffectColor, EffectRead,
    Hex, OutputPort, RestoreReport, UsbIdentity,
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

/// Why live lights are held back while the keyboard ignores writes (just
/// switched on or reset: [`kb_core::aa::SETTLE`]).
pub const LIVE_SETTLING: &str = "the keyboard was just switched on or reset and ignores live lights for a moment";
/// A full read that did not finish (no answer, a reset) is tried again in
/// the background after this long; a write tries it at once.
const REREAD_EVERY: Duration = Duration::from_secs(30);
/// Unexpected replies within [`CONFLICT_WINDOW`] that make the conflict
/// hint show.
pub const CONFLICT_THRESHOLD: u64 = 10;
pub const CONFLICT_WINDOW: Duration = Duration::from_secs(60);
/// Driver errors kept for the diagnostics report.
const ERRORS_KEPT: usize = 10;
/// Beta.1 kept a note here, in the app's data folder, while live lights had
/// switched a keyboard to its per-key mode (the effect to put back).
/// Nothing switches or puts back any more, and nothing reads it.
const BETA1_LIVE_NOTE: &str = "aa-live-armed.json";

/// Remove beta.1's live-lights note from the app's data folder, if it is
/// there: a file only, nothing is sent to a keyboard.
pub fn remove_beta1_live_note(dir: &Path) {
    match std::fs::remove_file(dir.join(BETA1_LIVE_NOTE)) {
        Ok(()) => tracing::info!("beta.1's live-lights note removed (nothing reads it any more)"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("beta.1's live-lights note not removed: {e}"),
    }
}

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
    /// The backup of its settings could not be saved.
    NoBackup(String),
}

impl fmt::Display for ReadOnly {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Grouped: one reason per table entry can be a hundred or more.
            Self::NotSane(why) => match crate::diagnostics::summarize_reasons(why) {
                summary if summary.is_empty() => {
                    write!(f, "the keyboard does not look as expected, so nothing is written")
                }
                summary => {
                    write!(f, "the keyboard does not look as expected, so nothing is written: {}", summary.join("; "))
                }
            },
            Self::NoBackup(e) => write!(f, "no backup of the keyboard could be saved, so nothing is written: {e}"),
        }
    }
}

impl ReadOnly {
    /// Which of them, for the log (the reasons go there once, with the
    /// read).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotSane(_) => "not as expected",
            Self::NoBackup(_) => "no backup",
        }
    }
}

impl From<&ReadOnly> for ReadOnlyReason {
    fn from(r: &ReadOnly) -> Self {
        match r {
            ReadOnly::NotSane(reasons) => {
                Self::NotSane { reasons: crate::diagnostics::summarize_reasons(reasons), read_failed: false }
            }
            ReadOnly::NoBackup(error) => Self::NoBackup { error: error.clone() },
        }
    }
}

/// Why live lights do not show on the keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveOff {
    /// The keyboard ignores writes for a moment ([`LIVE_SETTLING`]).
    Blocked(String),
}

impl From<&LiveOff> for LiveLightsOff {
    fn from(o: &LiveOff) -> Self {
        match o {
            LiveOff::Blocked(reason) => Self::Blocked { reason: reason.clone() },
        }
    }
}

/// Live lights on the keyboard: frames only (RAM, any effect mode), so
/// nothing is switched before a stream or put back after it.
#[derive(Debug, Clone)]
enum Live {
    /// No frames go to the keyboard.
    Idle,
    /// A stream's frames go to the keyboard.
    Streaming,
    /// A stream runs, but its frames are held back.
    Off { why: LiveOff },
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
    /// The app's data folder (backups); `None`: nothing can
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
    /// The quiet time after a cut screen upload: nothing is tried before.
    quiet_until: Option<Instant>,
    /// The last full read failed in a way that needs the channel opened
    /// again: the next operation hands it to the actor, which drops the
    /// handle.
    reopen: Option<AaError>,
    conflict: ConflictWatch,
    /// The last errors (Unix ms, text), for the diagnostics report.
    errors: VecDeque<(u64, String)>,
    /// The last full read that failed the sanity check, as saved for the
    /// developer ([`report::reads`]): its file, or why it was not saved.
    kept_read: Option<Result<String, String>>,
    /// When a save of each kind last read back as written (the driver reads
    /// back every one, and fails it otherwise).
    read_back: AaReadBack,
}

impl AaDriver {
    /// The driver for `model` on `port`; nothing is sent yet ([`Self::connect`]).
    pub fn new(port: AaPort, model: &Model, target: AaTarget) -> Result<Self, String> {
        let layout = AaLayout::from_model(model).map_err(|e| e.to_string())?;
        let mut conflict = ConflictWatch::default();
        conflict.note(Instant::now(), 0);
        #[cfg_attr(not(test), allow(unused_mut))]
        let mut kb = AaKeyboard::new(port, layout, target.usb);
        // The app's tests do not wait for the simulated keyboard to settle.
        #[cfg(test)]
        kb.set_settle(Duration::ZERO);
        Ok(Self {
            kb,
            target,
            read: Read::NotYet,
            read_at: None,
            backup: None,
            live: Live::Idle,
            quiet_until: None,
            reopen: None,
            conflict,
            errors: VecDeque::new(),
            kept_read: None,
            read_back: AaReadBack::default(),
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
    /// a sane keyboard gets its backup (with the colour and mode
    /// `app_effect` tells, if the keyboard's do not). Nothing is written
    /// here.
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
                } else {
                    let why = self.kb.sanity_reasons();
                    self.kept_read = self
                        .target
                        .data_dir
                        .as_deref()
                        .map(|dir| report::reads::keep(dir, &self.target, &snap, &why).map_err(|e| e.to_string()));
                    let kept = match &self.kept_read {
                        Some(Ok(file)) => file.clone(),
                        Some(Err(e)) => format!("not saved: {e}"),
                        None => "not saved: no data folder".into(),
                    };
                    let summary = crate::diagnostics::summarize_reasons(&why).join("; ");
                    tracing::warn!(
                        reasons = why.len(),
                        why = %crate::diagnostics::cut(&summary, 1_000),
                        read = %kept,
                        "\"AA\" keyboard read; read-only"
                    );
                }
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
    /// gate (a reset, an unfinished write, a write that did not read back);
    /// not when the keyboard's own data failed the check (that needs a
    /// reconnect). `background`: at most every [`REREAD_EVERY`].
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
        if self.kb.is_sane() {
            return false;
        }
        match &self.read {
            Read::NotYet | Read::Failed(_) => true,
            Read::Done { sane } => *sane,
        }
    }

    /// Why nothing is written now, if so.
    pub fn read_only(&self) -> Option<ReadOnly> {
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
        let snapshot = with_app_effect(snap, app_effect);
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

    // ------------------------------------------------------------ background

    /// When [`Self::run_due`] has something to do: read the keyboard again
    /// when a read did not finish or something since closed the gate.
    pub fn next_due(&self) -> Option<Instant> {
        if !self.stale() {
            return None;
        }
        let reread = self.read_at.map_or(Instant::now(), |t| t + REREAD_EVERY);
        Some(self.quiet_until.map_or(reread, |q| reread.max(q)))
    }

    /// Do what is due ([`Self::next_due`]); `app_effect` (the app's
    /// lighting record) fills in a backup the read takes.
    pub fn run_due(&mut self, app_effect: &dyn Fn() -> Option<AaEffect>) -> Result<(), Error> {
        self.reread_if_stale(true, app_effect);
        self.reopen_needed()
    }

    /// The keyboard's USB ids, as opened.
    pub fn usb(&self) -> UsbIdentity {
        self.target.usb
    }

    // ------------------------------------------------------------ live lights

    /// A stream starts: nothing to switch on firmware 769.
    pub fn live_starting(&mut self) {
        if matches!(self.live, Live::Idle) {
            self.live = Live::Streaming;
        }
    }

    /// One frame (RAM only, any effect mode). Held back, with no error,
    /// while the keyboard ignores writes (the UI says so), in the quiet time
    /// after a cut upload, and while its device information is unknown (a
    /// silent keyboard is not asked with every frame: the background read
    /// asks at its own pace, as for the mirror).
    pub fn live_frame(&mut self, frame: Vec<(u8, Rgb)>) -> Result<(), Error> {
        if self.quiet(Instant::now()) {
            return Ok(());
        }
        self.reopen_needed()?;
        if self.kb.info().is_none() {
            return Ok(());
        }
        let r = self.kb.realtime_frame(frame);
        self.after_io();
        match r {
            Ok(()) => {
                self.live = Live::Streaming;
                Ok(())
            }
            Err(AaError::Settling { .. }) => {
                self.live_off(LiveOff::Blocked(LIVE_SETTLING.into()));
                Ok(())
            }
            Err(AaError::UploadInterrupted { wait_ms }) => {
                self.quiet_until = Some(Instant::now() + Duration::from_millis(wait_ms));
                Ok(())
            }
            r => self.check(r),
        }
    }

    /// The stream ended: the keyboard shows its own effect again by itself.
    pub fn live_stopped(&mut self) {
        self.live = Live::Idle;
    }

    fn live_off(&mut self, why: LiveOff) {
        let same = matches!(&self.live, Live::Off { why: w } if *w == why);
        if !same {
            tracing::info!(?why, "live lights held back on this keyboard");
        }
        self.live = Live::Off { why };
    }

    // ------------------------------------------------------------ writes

    /// Store the app's effect (levels mapped to 1–5). **Writes flash.**
    pub fn apply_effect(&mut self, s: &EffectSettings) -> Result<(), Error> {
        self.writable()?;
        let r = self.kb.apply_effect(s);
        self.after_io();
        self.check(r)?;
        self.read_back.effect = Some(now_ms());
        Ok(())
    }

    /// Store per-key colours, then switch to per-key mode. **Writes flash.**
    pub fn apply_per_key(&mut self, colors: Vec<(u8, Rgb)>, brightness: u8) -> Result<(), Error> {
        self.writable()?;
        let r = self.kb.apply_per_key(colors, brightness);
        self.after_io();
        self.check(r)?;
        self.read_back.per_key = Some(now_ms());
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
        // A layer a write that stopped part-way left partly erased is
        // written even when the records ask for nothing new: the driver
        // gives every erased slot its factory entry back (the Fn key and
        // the knob too, which no change names).
        if changes.is_empty() && !self.layer_erased(layer) {
            tracing::debug!(?layer, kept, "keymap layer already as asked; nothing written");
            // The fresh read just showed it holds what was asked.
            self.read_back.keymap = Some(now_ms());
            return Ok(kept);
        }
        tracing::debug!(?layer, keys = changes.len(), kept, "keymap keys to change");
        let r = self.kb.apply_keymap(layer, changes);
        self.after_io();
        self.check(r)?;
        self.read_back.keymap = Some(now_ms());
        Ok(kept)
    }

    /// Whether `layer`, as last read, holds erased slots ([`aa_keymap::ERASED`]).
    fn layer_erased(&self, layer: Layer) -> bool {
        self.kb.snapshot().is_some_and(|s| {
            let t = match layer {
                Layer::Base => &s.keymap.0,
                Layer::Fn => &s.fn_keymap.0,
            };
            t.len() == aa_keymap::LEN
                && (0..aa_keymap::SLOTS as u8).any(|i| aa_keymap::entry(t, i) == aa_keymap::ERASED)
        })
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
            // action now (a firmware function is never brought back). An
            // erased slot is always written: its factory entry, or what the
            // records ask.
            let never = matches!(view, SlotView::Fixed | SlotView::Builtin)
                || (want == KeyAction::Default && aa_keymap::is_func(f) && view != SlotView::Erased);
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
        self.check(r)?;
        self.read_back.macros = Some(now_ms());
        Ok(())
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
                    // Erased: the next save gives it its factory entry back,
                    // so the records see that.
                    SlotView::Erased if aa_keymap::is_func(f) => out.builtin.push(k.led),
                    SlotView::Default | SlotView::Erased => {}
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
    /// backup, else `color`. The state before is backed up first, with the
    /// colour and mode `app_effect` (the app's lighting record) tells, if
    /// the keyboard's do not; old backups are pruned only once something
    /// was written, and never `source` (the file being restored). **Writes
    /// flash.**
    pub fn restore(
        &mut self,
        backup: &AaSnapshot,
        color: Option<EffectColor>,
        app_effect: Option<AaEffect>,
        source: Option<&str>,
    ) -> Result<RestoreReport, Error> {
        self.writable()?;
        // A fresh full read: the gate looks at the keyboard as it is now,
        // and the state before is what gets backed up.
        self.connect(app_effect);
        self.reopen_needed()?;
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
        let before = backups::KeyboardBackup::new(
            &self.target,
            backups::Reason::BeforeRestore,
            with_app_effect(&now, app_effect),
        );
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
                    AaError::RestoreIncomplete { .. } => {
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
                return self.check(Err(e));
            }
        };
        backups::prune(&dir, self.target.usb, &protect);
        if report.effect_mode_guessed {
            tracing::info!("the backup could not tell its effect mode (it read 0); chosen from its per-key table");
        }
        self.read_back.restore = Some(now_ms());
        Ok(report)
    }

    // ------------------------------------------------------------ reads

    /// How long the keyboard still takes no write (just opened or reset:
    /// [`kb_core::aa::SETTLE`]); a write meanwhile waits for it.
    pub fn settling(&self) -> Option<Duration> {
        self.kb.settling()
    }

    /// Whether the mirror may ask now: not while the keyboard left its
    /// device information unanswered (the background read asks again at its
    /// own pace), nor in the quiet time after a cut upload.
    pub fn mirror_ready(&self) -> bool {
        self.kb.info().is_some() && !self.quiet(Instant::now())
    }

    /// The colours the keys show now (`33`), as LED values.
    pub fn mirror_colors(&mut self) -> Result<Vec<(u8, Rgb)>, Error> {
        let r = self.kb.read_live_colors();
        self.after_io();
        self.check(r)
    }

    /// The device information (`10`), as the app shows it.
    pub fn device_info(&mut self) -> Result<DeviceReport, Error> {
        let r = self.kb.device_info();
        self.after_io();
        self.check(r).map(|i| device_report(&i))
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
        let live_lights_off = match &self.live {
            Live::Off { why } => Some(why.into()),
            Live::Idle | Live::Streaming => None,
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
            other_program_suspected: self.conflict.suspected,
            backup: self.backup.as_ref().and_then(|b| b.as_ref().ok().cloned()),
            effect_color_known: self.kb.current_effect().is_some_and(|e| e.color_known),
            read_back: self.read_back,
        }
    }

    /// The diagnostics report's section about this keyboard (`report`):
    /// each section of the last full read in a line (the device
    /// information, the options and the effect whole), why writes are
    /// refused, grouped, the counters, a hex excerpt of each table the
    /// sanity check found fault with and the last errors. Never macro
    /// contents; keymaps only by their entry pages unless one failed. The
    /// most useful lines come first: the report cuts from the end.
    pub fn diagnostics(&self) -> Vec<String> {
        use crate::diagnostics::{cut, summarize_reasons};
        const LINE: usize = 300;
        let mut out = vec![format!(
            "{} on USB {:04X}, firmware {}",
            self.target.record_id,
            self.target.usb.pid,
            kb_devices::firmware_text(self.target.usb.release)
        )];
        // The test guide's shared text says so at its top (aaGuideRules.ts).
        if crate::simulated() {
            out.push("simulated keyboard (a development build with KEYMOTION_SIM), not a real one".into());
        }
        match self.kb.snapshot() {
            Some(s) => out.extend(report::section_lines(s)),
            None => match self.kb.info() {
                Some(i) => out.push(format!("device information only: {i:?}")),
                None => out.push("nothing read".into()),
            },
        }
        let read_only = self.read_only();
        let why: &[String] = match &read_only {
            Some(ReadOnly::NotSane(why)) => why,
            _ => &[],
        };
        let reasons = match why.len() {
            0 => String::new(),
            1 => " (1 reason)".into(),
            n => format!(" ({n} reasons)"),
        };
        out.push(match &self.read {
            Read::NotYet => "full read: not yet".into(),
            Read::Done { sane: true } => "full read: done, passed the sanity check".into(),
            Read::Done { sane: false } => format!("full read: done, did not pass the sanity check{reasons}"),
            Read::Failed(e) => format!("full read: failed ({}){reasons}", cut(e, LINE)),
        });
        out.extend(summarize_reasons(why).into_iter().map(|l| format!("  {l}")));
        match &self.kept_read {
            Some(Ok(file)) => out.push(format!("failed read saved in the app's data folder: {file}")),
            Some(Err(e)) => out.push(format!("failed read not saved: {}", cut(e, LINE))),
            None => {}
        }
        out.push(match &read_only {
            None => "writes: allowed".into(),
            Some(ReadOnly::NotSane(_)) => "writes: refused, the keyboard does not look as expected (above)".into(),
            Some(r) => format!("writes: refused ({})", cut(&r.to_string(), LINE)),
        });
        out.push(format!("saves read back as written since connected: {}", read_back_kinds(&self.read_back)));
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
            Live::Streaming if self.kb.info().is_none() => {
                "live lights: held back (the keyboard left its device information unanswered)".into()
            }
            Live::Streaming => "live lights: frames streaming".into(),
            Live::Off { why: LiveOff::Blocked(reason) } => format!("live lights: held back ({})", cut(reason, LINE)),
        });
        if let Some(s) = self.kb.snapshot() {
            let broken = kb_core::aa::broken_macros(s);
            if !broken.is_empty() {
                let slots: Vec<String> = broken.iter().map(|(slot, _)| slot.to_string()).collect();
                out.push(format!(
                    "macro slots read as empty (erased, or pointing into the table; the next macro save \
                     repairs them): {}",
                    slots.join(", ")
                ));
            }
        }
        out.push(match &self.backup {
            Some(Ok(f)) => format!("backup: {f}"),
            Some(Err(e)) => format!("backup: failed ({})", cut(e, LINE)),
            None => "backup: none yet".into(),
        });
        if let Some(s) = self.kb.snapshot() {
            out.extend(report::excerpts(s, why));
        }
        for (at, e) in &self.errors {
            out.push(format!("error at {at}: {}", cut(e, LINE)));
        }
        out
    }
}

/// The kinds of save that read back as written, for the diagnostics report
/// ("none yet" before the first).
fn read_back_kinds(r: &AaReadBack) -> String {
    let kinds: Vec<&str> = [
        (r.effect, "effect"),
        (r.per_key, "per-key colours"),
        (r.keymap, "keymap"),
        (r.macros, "macros"),
        (r.restore, "restore"),
    ]
    .into_iter()
    .filter_map(|(at, kind)| at.map(|_| kind))
    .collect();
    if kinds.is_empty() {
        "none yet".into()
    } else {
        kinds.join(", ")
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
    /// Erased flash a keymap write that stopped part-way left: any save of
    /// the layer writes it (its factory entry, or what the records ask).
    Erased,
}

fn slot_view(now: [u8; 4], factory: [u8; 4]) -> SlotView {
    if now == aa_keymap::ERASED {
        return SlotView::Erased;
    }
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
    app.filter(|a| a.matches_readback(&effect)).map(|a| a.color_part())
}

/// The colour a backup's effect had, from the app's record.
fn app_color(s: &AaSnapshot, app: Option<AaEffect>) -> Option<EffectColor> {
    s.effect().and_then(|e| matching_color(e, app))
}

/// `snap` for a backup: the effect's colour and mode the keyboard does not
/// report, from the app's lighting record when its effect reads back as
/// the keyboard's.
fn with_app_effect(snap: &AaSnapshot, app_effect: Option<AaEffect>) -> AaSnapshot {
    let mut snapshot = snap.clone();
    snapshot.effect_color = snapshot.effect_color.or_else(|| app_color(&snapshot, app_effect).map(Into::into));
    if !snapshot.effect_mode_known() {
        snapshot.effect_mode = app_mode(&snapshot, app_effect);
    }
    snapshot
}

/// The mode behind a backup's read-back of 0 (off or a per-key mode), from
/// the app's record when its effect reads back as the backup's.
fn app_mode(s: &AaSnapshot, app: Option<AaEffect>) -> Option<u8> {
    let read = s.effect()?;
    app.filter(|a| read.mode == 0 && a.matches_readback(&read)).map(|a| a.mode)
}

/// The device information as the app shows it. Only `1` is taken as
/// "charging": what the other charge values mean is not known. Firmware
/// 769 has no "waiting for an update" field (protocol notes §F.7).
pub fn device_report(i: &DeviceInfo) -> DeviceReport {
    DeviceReport {
        battery: (i.battery_level <= 100).then_some(i.battery_level),
        charge: (i.charge_status == 1).then_some(ChargeState::Charging),
        screen_max_frames: (i.tft_max_frames > 0).then_some(u32::from(i.tft_max_frames)),
    }
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

mod report;

#[cfg(test)]
pub(crate) mod tests;
