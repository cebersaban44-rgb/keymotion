//! Driver for AJAZZ's "AA" command family: the AK980 PRO on firmware 769
//! (USB 0C45:8039). Protocol: `kb_protocol::aa` and the protocol notes.
//!
//! * Every packet runs [`kb_protocol::aa::guard::guard`] right before it is
//!   written: FF68 requests in [`exchange`], FF67 screen packets in
//!   [`screen_exchange`]. Keymap and per-key tables are also checked as a
//!   whole against the fresh read they were built from.
//! * Nothing is read before the device information (`10`) was answered,
//!   nothing at all while the keyboard waits for a firmware update, and
//!   nothing is written before a full read ([`AaKeyboard::read_all`]) passed
//!   the sanity gate ([`check_sanity`]). A reset notice (`55 FC 05`), an
//!   unfinished full read or an unfinished write closes the gate again, and
//!   a reset stops the operation it arrives in before its next write.
//! * Tables are read fresh, changed only where asked, written whole and
//!   read back; unchanged tables are not written.
//! * The effect's colour does not read back: after any effect write, by any
//!   program in any session, the keyboard reports `FF FF FF` with colour
//!   mode 1. The driver trusts a colour only when it wrote it itself or the
//!   read-back is something else ([`EffectRead::color_known`]), and never
//!   writes a colour it could not trust: the caller supplies it.
//! * After a screen upload stopped part-way, FF68 stays silent for
//!   [`UPLOAD_COOLDOWN`] and then sends one device-information request
//!   before anything else ([`AaError::UploadInterrupted`]). Keep the same
//!   [`AaKeyboard`] across a reopen of the screen channel, or the rule is
//!   lost.
//! * The app names keys by LED number; the driver translates through the
//!   model's [`AaLayout`].
//! * A failed write leaves that HID channel unusable (hidapi keeps the
//!   write pending): on [`AaError::needs_reopen`], reopen it and read
//!   everything again.

mod channel;
mod layout;
#[cfg(any(test, feature = "sim"))]
pub mod sim;
mod snapshot;
#[cfg(test)]
mod tests;

pub use channel::open_channel;
pub use kb_protocol::aa::effect::{AaEffect, EffectColor};
pub use kb_protocol::aa::info::DeviceInfo;
pub use kb_protocol::aa::keymap::Binding;
pub use kb_protocol::aa::macros::AaMacroEntry;
pub use layout::{AaLayout, LayoutError, LayoutKey, KNOB_SLOTS};
pub use snapshot::{check as check_sanity, AaSnapshot, ColorRecord, DeviceRecord, Hex, MacroBlock, READ_LEVEL_MAX};

use kb_protocol::aa::guard::{self, Blocked, Channel};
use kb_protocol::aa::{clock, cmd, effect, frame, info, keymap, macros, notify, perkey, realtime, screen, CodecError};
use kb_protocol::sonix::keymap::{KeyAction, Layer};
use kb_protocol::sonix::macros::MacroStep;
use kb_protocol::sonix::screen::ScreenAnimation;
use kb_protocol::sonix::{ClockTime, EffectSettings, Rgb};
use kb_transport::hid::HidChannel;
use kb_transport::TransportError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Tries per request before giving up.
pub const ATTEMPTS: usize = 3;

/// The default wait for a reply.
const DEFAULT_TIMEOUT_MS: u32 = 500;

/// How long to wait for a reply (AJAZZ's WebHID driver: 500 ms, longer for
/// the big writes, 50 ms per real-time chunk).
pub fn timeout_ms(c: u8) -> u32 {
    match c {
        cmd::SET_KEYMAP | cmd::SET_FN_KEYMAP => 1000,
        cmd::SET_PER_KEY | cmd::SCREEN => 2000,
        cmd::REALTIME => realtime::CHUNK_TIMEOUT_MS,
        _ => DEFAULT_TIMEOUT_MS,
    }
}

/// [`timeout_ms`] on a board reporting `frame_version`: version 1 raises
/// only the 500 ms default to 2 s (AJAZZ's WebHID driver).
pub fn timeout_ms_for(c: u8, frame_version: u8) -> u32 {
    match timeout_ms(c) {
        DEFAULT_TIMEOUT_MS if frame_version == 1 => 2000,
        t => t,
    }
}

/// How long the command channel stays silent after a screen upload stopped
/// part-way. The keyboard has no command that ends an upload, nobody has
/// measured how long it waits for the missing packets, and command traffic
/// during an upload reset a sibling board's USB: well above the 2 s the
/// WebHID driver allows a packet.
pub const UPLOAD_COOLDOWN: Duration = Duration::from_secs(5);

/// The USB device the channels belong to, as the OS reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbIdentity {
    pub pid: u16,
    /// `bcdDevice`: the firmware version.
    pub release: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum AaError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("only {written} of {expected} bytes were written")]
    ShortWrite { written: usize, expected: usize },
    #[error("no answer to command 0x{cmd:02X}")]
    Timeout { cmd: u8 },
    #[error("the keyboard echoed different data for command 0x{cmd:02X}")]
    Mismatch { cmd: u8 },
    #[error("blocked by the safety guard: {0}")]
    Blocked(#[from] Blocked),
    #[error("the keyboard is waiting for a firmware update; nothing is read or written")]
    UpdatePending,
    #[error("the keyboard does not look as expected, so nothing is written: {}", .0.join("; "))]
    NotSane(Vec<String>),
    #[error("not supported on this keyboard: {0}")]
    Unsupported(&'static str),
    #[error("live lighting needs effect mode 20, 21 or 128 first")]
    NotArmed,
    #[error(
        "{macros} macros with {entries} steps take {bytes} bytes; the keyboard takes {limit} \
         (and at most 99 macros)"
    )]
    TooLarge { macros: usize, entries: usize, bytes: usize, limit: usize },
    #[error("the {what} read back differs from what was written")]
    Readback { what: &'static str },
    #[error("invalid value: {0}")]
    Invalid(CodecError),
    #[error("LED {0} is not a key of this keyboard")]
    UnknownLed(u8),
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error("channel not usable: {0}")]
    Channel(String),
    #[error("screen upload cancelled after {done} of {total} parts")]
    Cancelled { done: usize, total: usize },
    #[error(
        "the keyboard does not report its effect colour (it reads back white after any write); \
         the colour to write has to be given"
    )]
    ColorUnknown,
    #[error("a screen upload stopped part-way; no commands for another {wait_ms} ms")]
    UploadInterrupted { wait_ms: u64 },
    /// A restore failed while writing `section`; `report` holds what was
    /// written before it (complete sections only; `section` itself may be
    /// partly written).
    #[error("the restore stopped while writing the {section} ({}): {error}", wrote(.report))]
    RestoreIncomplete { section: &'static str, report: RestoreReport, error: Box<AaError> },
}

fn wrote(report: &RestoreReport) -> String {
    match report.written.as_slice() {
        [] => "nothing written before".into(),
        done => format!("already written: {}", done.join(", ")),
    }
}

impl AaError {
    /// The channel that failed must be dropped and opened again before it
    /// is used (a write failed or stalled; hidapi keeps it pending).
    pub fn needs_reopen(&self) -> bool {
        match self {
            Self::Transport(_) | Self::ShortWrite { .. } => true,
            Self::RestoreIncomplete { error, .. } => error.needs_reopen(),
            _ => false,
        }
    }
}

impl From<CodecError> for AaError {
    fn from(e: CodecError) -> Self {
        match e {
            CodecError::Unsupported(what) => Self::Unsupported(what),
            CodecError::TooLarge { macros, entries, bytes, limit } => Self::TooLarge { macros, entries, bytes, limit },
            other => Self::Invalid(other),
        }
    }
}

pub type Result<T> = std::result::Result<T, AaError>;

/// One "AA" collection: output reports out, input reports in. Implemented
/// by the HID channel (FF68 and FF67 alike) and by the simulator.
pub trait OutputPort {
    /// Write one output report (report ID first); returns the bytes
    /// written. Never a feature report.
    fn write(&mut self, report: &[u8]) -> Result<usize>;
    /// Wait up to `timeout_ms` for one input report (64 bytes, no report
    /// ID); `None` when none came.
    fn read(&mut self, timeout_ms: u32) -> Result<Option<Vec<u8>>>;
    /// Drop input reports already queued; returns how many.
    fn drain(&mut self) -> usize {
        let mut n = 0;
        while n < 64 && matches!(self.read(0), Ok(Some(_))) {
            n += 1;
        }
        n
    }
}

impl OutputPort for HidChannel {
    fn write(&mut self, report: &[u8]) -> Result<usize> {
        Ok(self.write_output(report)?)
    }
    fn read(&mut self, timeout_ms: u32) -> Result<Option<Vec<u8>>> {
        match self.read_input(frame::WIRE_LEN, timeout_ms.min(i32::MAX as u32) as i32) {
            Ok(mut r) => {
                // Windows leaves the report ID out of unnumbered reports;
                // drop it should a backend keep it.
                if r.len() == frame::WIRE_LEN && r[0] == 0 {
                    r.remove(0);
                }
                Ok(Some(r))
            }
            Err(TransportError::Timeout(_)) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// Reports that were not the awaited reply.
#[derive(Debug, Default)]
struct Watch {
    unexpected: u64,
    /// Of those, the keyboard's own notices (`55 FA`, `55 FB`, `55 FC`).
    notices: u64,
    resets: u64,
}

impl Watch {
    fn note(&mut self, report: &[u8]) {
        self.unexpected += 1;
        let notice = notify::notice(report);
        if notice.is_some() {
            self.notices += 1;
        }
        if notice == Some(notify::Notice::Reset) {
            self.resets += 1;
        }
        tracing::debug!(?notice, head = ?&report[..report.len().min(8)], "unexpected report skipped");
    }
}

/// The reply a request waits for.
struct Expect<'a> {
    cmd: u8,
    /// The chunk's address (FF68 replies echo it).
    addr: u16,
    /// What a write's echo must repeat.
    echo: Option<&'a [u8]>,
}

fn is_reply(report: &[u8], expect: &Expect<'_>) -> bool {
    frame::parse_reply(report).is_some_and(|r| r.cmd == expect.cmd && r.addr == expect.addr)
}

/// The one way an FF68 request reaches the keyboard: guard it, write it,
/// wait for its reply (matched by command and address; anything else is
/// skipped and counted), up to [`ATTEMPTS`] times. A failed write is
/// returned at once: the channel must be reopened.
fn exchange<P: OutputPort>(
    port: &mut P,
    wire: &[u8],
    expect: &Expect<'_>,
    timeout_ms: u32,
    watch: &mut Watch,
) -> Result<Vec<u8>> {
    guard::guard(Channel::Command, wire)?;
    for attempt in 0..ATTEMPTS {
        let written = port.write(wire)?;
        if written != wire.len() {
            return Err(AaError::ShortWrite { written, expected: wire.len() });
        }
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.into());
        loop {
            let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
            let Some(r) = port.read(left)? else { break };
            if is_reply(&r, expect) {
                if let Some(sent) = expect.echo {
                    if r.get(frame::HEADER_LEN..frame::HEADER_LEN + sent.len()) != Some(sent) {
                        return Err(AaError::Mismatch { cmd: expect.cmd });
                    }
                }
                return Ok(r);
            }
            watch.note(&r);
            if Instant::now() >= deadline {
                break;
            }
        }
        tracing::debug!(cmd = expect.cmd, attempt, "no reply");
    }
    Err(AaError::Timeout { cmd: expect.cmd })
}

fn is_screen_ack(report: &[u8]) -> bool {
    report.get(..2) == Some(&screen::ACK[..2])
}

/// Wait up to `timeout_ms` for one FF67 answer.
fn wait_screen_ack<D: OutputPort>(display: &mut D, timeout_ms: u32, watch: &mut Watch) -> Result<bool> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.into());
    loop {
        let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
        let Some(r) = display.read(left)? else { return Ok(false) };
        if is_screen_ack(&r) {
            return Ok(true);
        }
        watch.note(&r);
        if Instant::now() >= deadline {
            return Ok(false);
        }
    }
}

/// Drop what FF67 has queued; returns how many of them were answers.
fn drain_screen_acks<D: OutputPort>(display: &mut D, watch: &mut Watch) -> Result<usize> {
    let mut acks = 0;
    for _ in 0..64 {
        let Some(r) = display.read(0)? else { break };
        if is_screen_ack(&r) {
            acks += 1;
        } else {
            watch.note(&r);
        }
    }
    Ok(acks)
}

/// The one way a screen packet reaches FF67. Its answer `55 41 00 01` names
/// no packet, so answers are counted, not matched: whatever was queued
/// before the packet is dropped first, a resent packet's late answers are
/// dropped before the resend, and every copy sent gets its answer consumed
/// (bounded wait) before the next packet. Returns the stale answers dropped.
fn screen_exchange<D: OutputPort>(display: &mut D, wire: &[u8], watch: &mut Watch) -> Result<usize> {
    guard::guard(Channel::Display, wire)?;
    let mut stale = drain_screen_acks(display, watch)?;
    let mut outstanding = 0usize;
    let mut answered = false;
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            let late = drain_screen_acks(display, watch)?;
            stale += late;
            outstanding = outstanding.saturating_sub(late);
        }
        let written = display.write(wire)?;
        if written != wire.len() {
            return Err(AaError::ShortWrite { written, expected: wire.len() });
        }
        outstanding += 1;
        if wait_screen_ack(display, screen::ACK_TIMEOUT_MS, watch)? {
            outstanding -= 1;
            answered = true;
            break;
        }
        tracing::debug!(attempt, "no screen answer");
    }
    if !answered {
        return Err(AaError::Timeout { cmd: cmd::SCREEN });
    }
    while outstanding > 0 && wait_screen_ack(display, screen::ACK_TIMEOUT_MS, watch)? {
        outstanding -= 1;
    }
    Ok(stale)
}

/// What a screen upload measured.
#[derive(Debug, Clone)]
pub struct AaScreenReport {
    pub packets: usize,
    /// Input reports already queued on FF67 before the upload.
    pub drained: usize,
    /// Answers dropped because they came before their packet or belonged to
    /// a resent copy.
    pub stale_acks: usize,
    /// Write-to-answer time of every packet.
    pub packet_times: Vec<Duration>,
}

/// The effect as read: mode and levels always; the colour and colour mode
/// only when [`Self::color_known`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectRead {
    pub effect: AaEffect,
    /// False when the read-back shows the post-write quirk (`FF FF FF`,
    /// colour mode 1) and the driver did not write the colour itself: the
    /// keyboard may show anything.
    pub color_known: bool,
}

impl EffectRead {
    /// The colour and colour mode, when known.
    pub fn color(&self) -> Option<EffectColor> {
        self.color_known.then(|| self.effect.color_part())
    }

    /// The effect as the app describes it; its colour and rainbow flag mean
    /// nothing unless [`Self::color_known`].
    pub fn settings(&self) -> EffectSettings {
        self.effect.to_settings()
    }
}

/// A slot a restore left as the keyboard has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptSlot {
    pub layer: Layer,
    pub slot: u8,
    /// The key's LED, when the slot is a key of the layout.
    pub led: Option<u8>,
    /// What the backup holds there.
    pub backup: [u8; 4],
    /// Why it was not written.
    pub reason: String,
}

/// What a restore did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreReport {
    /// Backup entries that were not written (a firmware function, a host
    /// program, anything this driver never writes, or a factory firmware
    /// function a cleared slot would bring back): the keyboard keeps its own.
    pub kept: Vec<KeptSlot>,
    /// The sections written (unchanged ones are not).
    pub written: Vec<&'static str>,
}

const RESET_REASON: &str = "the keyboard reported a reset (55 FC 05); read it again before writing";
const NOTHING_READ: &str = "nothing has been read from the keyboard yet";
const INFO_UNANSWERED: &str = "the device information (10) went unanswered; read it again before writing";

/// The colour stored with per-key mode when the real one is unknown: white,
/// one colour, the web hub's reset default. Per-key mode does not show it;
/// it is what a later switch to a one-colour mode starts from, so it is a
/// visible one rather than black.
pub const NEUTRAL_COLOR: EffectColor = EffectColor { color: [0xFF, 0xFF, 0xFF], color_mode: 0 };
const UNFINISHED_READ: &str = "the last full read of the keyboard did not finish";
const UNFINISHED_WRITE: &str = "a write to the keyboard did not finish; read it again before writing";

/// Levels in range for a write. Reads may show 0 or 6 (other tools,
/// sibling boards); the keyboard is only ever sent 1–5.
pub fn levels_in_range(e: AaEffect) -> AaEffect {
    let fit = |v: u8| v.clamp(effect::MIN_LEVEL, effect::MAX_LEVEL);
    AaEffect { brightness: fit(e.brightness), speed: fit(e.speed), ..e }
}

fn layer_commands(layer: Layer) -> (u8, u8) {
    match layer {
        Layer::Base => (cmd::KEYMAP, cmd::SET_KEYMAP),
        Layer::Fn => (cmd::FN_KEYMAP, cmd::SET_FN_KEYMAP),
    }
}

/// Whether a table read back as `table` was written: a slot written as
/// `00 00 00 00` may read back as its factory entry (the firmware fills it
/// in; measured on the MK87).
fn keymap_matches(table: &[u8], back: &[u8], factory: &[u8]) -> bool {
    back.len() == table.len()
        && (0..keymap::SLOTS as u8).all(|s| {
            let (t, b) = (keymap::entry(table, s), keymap::entry(back, s));
            t == b || (t == [0; 4] && b == keymap::entry(factory, s))
        })
}

/// Note a restore section as written, or turn its failure into
/// [`AaError::RestoreIncomplete`] with what was written before it.
fn record(report: &mut RestoreReport, section: &'static str, r: Result<()>) -> Result<()> {
    match r {
        Ok(()) => {
            report.written.push(section);
            Ok(())
        }
        Err(error) => {
            Err(AaError::RestoreIncomplete { section, report: std::mem::take(report), error: Box::new(error) })
        }
    }
}

/// An "AA" keyboard on its FF68 command channel.
pub struct AaKeyboard<P: OutputPort> {
    port: P,
    layout: AaLayout,
    usb: UsbIdentity,
    watch: Watch,
    resets_seen: u64,
    info: Option<DeviceInfo>,
    snapshot: Option<AaSnapshot>,
    /// The sanity gate's verdict on the last full read.
    sanity: Option<std::result::Result<(), Vec<String>>>,
    update_pending: bool,
    /// The effect the keyboard holds, as last read or written.
    effect: Option<EffectRead>,
    /// The last effect written (its colour is the only trusted one).
    written: Option<AaEffect>,
    /// When a screen upload stopped part-way.
    upload_interrupted: Option<Instant>,
    upload_cooldown: Duration,
}

impl<P: OutputPort> AaKeyboard<P> {
    pub fn new(port: P, layout: AaLayout, usb: UsbIdentity) -> Self {
        Self {
            port,
            layout,
            usb,
            watch: Watch::default(),
            resets_seen: 0,
            info: None,
            snapshot: None,
            sanity: None,
            update_pending: false,
            effect: None,
            written: None,
            upload_interrupted: None,
            upload_cooldown: UPLOAD_COOLDOWN,
        }
    }

    /// A shorter [`UPLOAD_COOLDOWN`], for tests and the simulator.
    #[cfg(any(test, feature = "sim"))]
    pub fn set_upload_cooldown(&mut self, cooldown: Duration) {
        self.upload_cooldown = cooldown;
    }

    pub fn layout(&self) -> &AaLayout {
        &self.layout
    }

    /// Reports that arrived while a reply was awaited and were not it. More
    /// than a few means another program talks to the keyboard (AJAZZ's web
    /// tools cannot be found by process name).
    pub fn unexpected_reports(&self) -> u64 {
        self.watch.unexpected
    }

    /// Of [`Self::unexpected_reports`], the keyboard's own notices (a reset,
    /// sleep, a status or calibration report): what is left are replies to
    /// requests this driver did not make.
    pub fn notices_reported(&self) -> u64 {
        self.watch.notices
    }

    /// How often the keyboard reported a reset (`55 FC 05`).
    pub fn resets_reported(&self) -> u64 {
        self.watch.resets
    }

    /// The device information last read (`None` after a failed attempt).
    pub fn info(&self) -> Option<&DeviceInfo> {
        self.info.as_ref()
    }

    /// The last full read, kept up to date by every write.
    pub fn snapshot(&self) -> Option<&AaSnapshot> {
        self.snapshot.as_ref()
    }

    /// The effect the keyboard holds, as far as the driver knows.
    pub fn current_effect(&self) -> Option<EffectRead> {
        self.effect
    }

    /// Whether writes are allowed: a full read passed the sanity gate,
    /// nothing has invalidated it since, and the last device-information
    /// request was answered.
    pub fn is_sane(&self) -> bool {
        self.check_writable().is_ok()
    }

    /// Why writes are refused (empty when they are allowed).
    pub fn sanity_reasons(&self) -> Vec<String> {
        match self.check_writable() {
            Ok(()) => Vec::new(),
            Err(AaError::NotSane(why)) => why,
            Err(e) => vec![e.to_string()],
        }
    }

    pub fn into_inner(self) -> P {
        self.port
    }

    // ---------------------------------------------------------------- gates

    /// Reads need an answered device information first: a keyboard waiting
    /// for a firmware update gets nothing else.
    fn ensure_readable(&mut self) -> Result<()> {
        if self.update_pending {
            return Err(AaError::UpdatePending);
        }
        if self.info.is_none() {
            self.device_info()?;
        }
        Ok(())
    }

    /// The gate's verdict, without any I/O; the one source for
    /// [`Self::is_sane`], [`Self::sanity_reasons`] and every write. A
    /// device-information request that went unanswered after the full read
    /// (it clears what was known) closes the gate until one is answered.
    fn check_writable(&self) -> Result<()> {
        if self.update_pending {
            return Err(AaError::UpdatePending);
        }
        match &self.sanity {
            None => Err(AaError::NotSane(vec![NOTHING_READ.into()])),
            Some(Err(why)) => Err(AaError::NotSane(why.clone())),
            Some(Ok(())) if self.info.is_none() => Err(AaError::NotSane(vec![INFO_UNANSWERED.into()])),
            Some(Ok(())) => Ok(()),
        }
    }

    /// Writes need the gate open (which needs a full read, device
    /// information included); refused without sending anything.
    fn ensure_writable(&mut self) -> Result<()> {
        self.check_writable()
    }

    /// After a screen upload stopped part-way: nothing on FF68 until the
    /// cooldown is over, then one device-information request.
    fn ensure_idle(&mut self) -> Result<()> {
        let Some(since) = self.upload_interrupted else { return Ok(()) };
        let waited = since.elapsed();
        if waited < self.upload_cooldown {
            return Err(AaError::UploadInterrupted { wait_ms: (self.upload_cooldown - waited).as_millis() as u64 });
        }
        self.upload_interrupted = None;
        let probe = self.device_info();
        if let Err(e) = &probe {
            if !matches!(e, AaError::UpdatePending) {
                self.upload_interrupted = Some(Instant::now());
            }
        }
        probe.map(|_| ())
    }

    /// How long to wait for command `c`'s reply on this board.
    fn timeout(&self, c: u8) -> u32 {
        timeout_ms_for(c, self.info.map_or(0, |i| i.frame_version))
    }

    /// A reset reported since the last look makes everything known stale.
    fn after_io(&mut self) {
        if self.watch.resets != self.resets_seen {
            self.resets_seen = self.watch.resets;
            self.effect = None;
            self.written = None;
            self.sanity = Some(Err(vec![RESET_REASON.into()]));
        }
    }

    // ---------------------------------------------------------------- I/O

    /// Read `len` bytes of block `c` from address `base`. `request` is the
    /// data each chunk carries (only `33` sends any).
    fn read_block(&mut self, c: u8, len: usize, base: u16, request: Option<&[u8]>) -> Result<Vec<u8>> {
        self.ensure_idle()?;
        let mut out = Vec::with_capacity(len);
        let timeout = self.timeout(c);
        let result = (|| -> Result<()> {
            for ch in frame::chunks(len, base, true) {
                let data = request.map_or(&[][..], |r| &r[ch.offset..]);
                let wire = frame::wire(&frame::request(c, &ch, 0, data));
                let expect = Expect { cmd: c, addr: ch.addr, echo: None };
                let r = exchange(&mut self.port, &wire, &expect, timeout, &mut self.watch)?;
                let data = r
                    .get(frame::HEADER_LEN..frame::HEADER_LEN + usize::from(ch.len))
                    .ok_or(AaError::Invalid(CodecError::Malformed("reply")))?;
                out.extend_from_slice(data);
            }
            Ok(())
        })();
        self.after_io();
        result.map(|()| out)
    }

    /// Write `data` as block `c` from address `base`, every chunk echoed.
    /// Refused unless the gate is open, and stopped before the next chunk
    /// when the keyboard reports a reset. A flash write that fails part-way
    /// closes the gate: the keyboard holds something between old and new.
    fn write_block(&mut self, c: u8, data: &[u8], base: u16, last_flag: bool) -> Result<()> {
        self.ensure_idle()?;
        self.after_io();
        self.check_writable()?;
        let timeout = self.timeout(c);
        let resets = self.watch.resets;
        let result = (|| -> Result<()> {
            for ch in frame::chunks(data.len(), base, last_flag) {
                if self.watch.resets != resets {
                    return Err(AaError::NotSane(vec![RESET_REASON.into()]));
                }
                let part = &data[ch.offset..ch.offset + usize::from(ch.len)];
                let wire = frame::wire(&frame::request(c, &ch, 0, part));
                let expect = Expect { cmd: c, addr: ch.addr, echo: Some(part) };
                exchange(&mut self.port, &wire, &expect, timeout, &mut self.watch)?;
            }
            Ok(())
        })();
        self.after_io();
        if result.is_err() && !matches!(c, cmd::REALTIME | cmd::CLOCK) && self.watch.resets == resets {
            self.sanity = Some(Err(vec![UNFINISHED_WRITE.into()]));
        }
        result
    }

    fn patch_snapshot(&mut self, f: impl FnOnce(&mut AaSnapshot)) {
        if let Some(s) = self.snapshot.as_mut() {
            f(s);
        }
    }

    // ---------------------------------------------------------------- reading

    /// Read the device information (`10`) only: the first and safest
    /// request. A keyboard waiting for a firmware update gets nothing more.
    pub fn device_info(&mut self) -> Result<DeviceInfo> {
        if self.upload_interrupted.is_some() {
            // The probe after an interrupted upload is this very request.
            self.ensure_idle()?;
            if let Some(i) = self.info {
                return Ok(i);
            }
        }
        self.info = None;
        let raw = self.read_block(cmd::DEVICE_INFO, info::LEN, 0, None)?;
        let i = DeviceInfo::parse(&raw)?;
        self.info = Some(i);
        self.update_pending = i.update_pending();
        if self.update_pending {
            return Err(AaError::UpdatePending);
        }
        Ok(i)
    }

    /// Read everything the keyboard lets the host read and run the sanity
    /// gate over it. The snapshot is returned even when the gate fails (the
    /// app can keep it as a backup); writes are allowed only when it passed
    /// ([`Self::is_sane`]).
    pub fn read_all(&mut self) -> Result<AaSnapshot> {
        self.ensure_idle()?;
        let resets = self.watch.resets;
        // Until this read is through, the last verdict no longer holds.
        self.sanity = Some(Err(vec![UNFINISHED_READ.into()]));
        self.info = None;
        let raw_info = self.read_block(cmd::DEVICE_INFO, info::LEN, 0, None)?;
        let i = DeviceInfo::parse(&raw_info)?;
        self.info = Some(i);
        self.update_pending = i.update_pending();
        if self.update_pending {
            self.sanity = None;
            return Err(AaError::UpdatePending);
        }
        let options = self.read_block(cmd::OPTIONS, 56, 0, None)?;
        let keymap = self.read_block(cmd::KEYMAP, keymap::LEN, 0, None)?;
        let fn_keymap = self.read_block(cmd::FN_KEYMAP, keymap::LEN, 0, None)?;
        let default_fn = self.read_block(cmd::DEFAULT_FN, keymap::LEN, 0, None)?;
        let effect = self.read_block(cmd::EFFECT, effect::LEN, 0, None)?;
        let per_key = self.read_block(cmd::PER_KEY, perkey::LEN, 0, None)?;
        let macro_table = self.read_block(cmd::MACROS, macros::TABLE_LEN, 0, None)?;
        let macro_blocks = self.read_macro_blocks(&macro_table, &i)?;
        let read = AaEffect::decode(&effect).ok().map(|e| self.remember(e));
        let snap = AaSnapshot {
            format: AaSnapshot::FORMAT,
            info: Hex(raw_info),
            device: i.into(),
            options: Hex(options),
            keymap: Hex(keymap),
            fn_keymap: Hex(fn_keymap),
            default_fn: Hex(default_fn),
            effect: Hex(effect),
            effect_color: read.and_then(|r| r.color()).map(Into::into),
            per_key: Hex(per_key),
            macro_table: Hex(macro_table),
            macro_blocks,
        };
        let mut verdict = check_sanity(&snap, self.usb);
        if self.watch.resets != resets {
            let mut why = verdict.err().unwrap_or_default();
            why.push(RESET_REASON.into());
            verdict = Err(why);
        }
        if let Err(why) = &verdict {
            tracing::warn!(?why, "the keyboard did not pass the sanity gate; no writes");
        }
        self.sanity = Some(verdict);
        self.snapshot = Some(snap.clone());
        Ok(snap)
    }

    /// Read the block every macro slot points to. A slot holding 0 or
    /// `FF FF FF FF` (erased flash) has none; addresses outside the macro
    /// memory are not read (the sanity gate reports them).
    fn read_macro_blocks(&mut self, table: &[u8], i: &DeviceInfo) -> Result<Vec<MacroBlock>> {
        let limit = snapshot::macro_limit(i);
        let mut blocks = Vec::new();
        for (slot, p) in macros::pointers(table).into_iter().enumerate() {
            if macros::is_none(p) || !snapshot::block_fits(p, 4, limit) {
                continue;
            }
            // Inside the macro memory, so below 0xFFFF.
            let mut bytes = self.read_block(cmd::MACROS, 4, p as u16, None)?;
            let body = 4 * macros::header_entries(&bytes);
            if body > 0 && snapshot::block_fits(p, 4 + body, limit) {
                bytes.extend(self.read_block(cmd::MACROS, body, (p + 4) as u16, None)?);
            }
            blocks.push(MacroBlock { slot: slot as u8, addr: p, bytes: Hex(bytes) });
        }
        Ok(blocks)
    }

    /// The macros the keyboard holds, slot by slot (100 slots; empty = none).
    fn read_macro_slots(&mut self) -> Result<Vec<Vec<AaMacroEntry>>> {
        let i = self.info.ok_or_else(|| AaError::NotSane(vec![NOTHING_READ.into()]))?;
        let table = self.read_block(cmd::MACROS, macros::TABLE_LEN, 0, None)?;
        let mut slots = vec![Vec::new(); macros::MAX_MACROS];
        for b in self.read_macro_blocks(&table, &i)? {
            slots[usize::from(b.slot)] = macros::decode_entries(b.bytes.0.get(4..).unwrap_or_default());
        }
        Ok(slots)
    }

    /// What a read of the effect tells: the colour only when this driver
    /// wrote it (and mode and levels still match), or when the read-back is
    /// not the post-write quirk.
    ///
    /// Any read-back other than `FF FF FF` / colour mode 1 is taken as the
    /// colour the keyboard holds. That rests on the captures (the quirk
    /// appears after a write; before one, the factory colour reads back)
    /// and is not verified on firmware 769 beyond them. The colour this
    /// driver wrote is remembered for one session only: a reset notice
    /// clears it ([`Self::after_io`]) and a reopened channel is a new
    /// driver, which starts without it.
    fn interpret(&self, read: AaEffect) -> EffectRead {
        if let Some(w) = self.written.filter(|w| w.same_settings(&read)) {
            return EffectRead { effect: read.with_color(w.color_part()), color_known: true };
        }
        EffectRead { effect: read, color_known: read.color_part() != effect::READBACK_QUIRK }
    }

    fn remember(&mut self, read: AaEffect) -> EffectRead {
        let e = self.interpret(read);
        self.effect = Some(e);
        e
    }

    /// Read the effect (`13`).
    pub fn read_effect(&mut self) -> Result<EffectRead> {
        self.ensure_readable()?;
        let raw = self.read_block(cmd::EFFECT, effect::LEN, 0, None)?;
        let e = AaEffect::decode(&raw)?;
        Ok(self.remember(e))
    }

    /// The effect as the app describes it, and whether its colour and
    /// rainbow flag are known ([`EffectRead::color_known`]).
    pub fn effect_settings(&mut self) -> Result<(EffectSettings, bool)> {
        let e = self.read_effect()?;
        Ok((e.settings(), e.color_known))
    }

    /// The per-key table's colours, by LED, in layout order.
    pub fn read_per_key(&mut self) -> Result<Vec<(u8, Rgb)>> {
        self.ensure_readable()?;
        let t = self.read_block(cmd::PER_KEY, perkey::LEN, 0, None)?;
        Ok(self.layout.keys().iter().map(|k| (k.led, perkey::get(&t, k.aa))).collect())
    }

    /// The colours the keys show right now (`33`), by LED, in layout order.
    pub fn read_live_colors(&mut self) -> Result<Vec<(u8, Rgb)>> {
        self.ensure_readable()?;
        let request = perkey::live_request();
        let t = self.read_block(cmd::LIVE_COLORS, perkey::LEN, 0, Some(&request[..]))?;
        Ok(self.layout.keys().iter().map(|k| (k.led, perkey::get(&t, k.aa))).collect())
    }

    /// One keymap layer, by LED, in layout order.
    pub fn read_keymap(&mut self, layer: Layer) -> Result<Vec<(u8, Binding)>> {
        self.ensure_readable()?;
        let t = self.read_block(layer_commands(layer).0, keymap::LEN, 0, None)?;
        Ok(self.bindings(&t))
    }

    /// The factory Fn layer (`1C`), by LED, in layout order.
    pub fn read_default_fn(&mut self) -> Result<Vec<(u8, Binding)>> {
        self.ensure_readable()?;
        let t = self.read_block(cmd::DEFAULT_FN, keymap::LEN, 0, None)?;
        Ok(self.bindings(&t))
    }

    fn bindings(&self, table: &[u8]) -> Vec<(u8, Binding)> {
        self.layout.keys().iter().map(|k| (k.led, keymap::decode(keymap::entry(table, k.aa)))).collect()
    }

    /// Every macro slot's entries (100 slots; empty = no macro).
    pub fn read_macros(&mut self) -> Result<Vec<Vec<AaMacroEntry>>> {
        self.ensure_readable()?;
        self.read_macro_slots()
    }

    // ---------------------------------------------------------------- writing

    /// Write an effect and read it back (mode and levels only: the colour
    /// does not read back).
    fn put_effect(&mut self, e: &AaEffect) -> Result<()> {
        let data = e.encode()?;
        self.write_block(cmd::SET_EFFECT, &data, 0, true)?;
        self.written = Some(*e);
        let raw = self.read_block(cmd::EFFECT, effect::LEN, 0, None)?;
        let back = AaEffect::decode(&raw)?;
        let read = self.remember(back);
        let ok = e.same_settings(&back);
        self.patch_snapshot(|s| {
            s.effect = Hex(raw);
            s.effect_color = if ok { read.color().map(Into::into) } else { None };
        });
        if !ok {
            return Err(AaError::Readback { what: "effect" });
        }
        Ok(())
    }

    /// Store an effect given in the keyboard's own values. **Writes flash.**
    pub fn write_effect(&mut self, e: &AaEffect) -> Result<()> {
        self.ensure_writable()?;
        self.put_effect(e)
    }

    /// Store the app's effect (levels mapped to 1–5). **Writes flash.**
    pub fn apply_effect(&mut self, s: &EffectSettings) -> Result<()> {
        let e = AaEffect::from_settings(s)?;
        self.write_effect(&e)
    }

    /// `current` in per-key mode (at `level` when given), its other fields
    /// kept. A colour the driver cannot trust is not written back: per-key
    /// mode shows none, so [`NEUTRAL_COLOR`] goes in its place.
    fn per_key_effect(current: EffectRead, level: Option<u8>) -> AaEffect {
        let mut e = current.effect.per_key();
        if !current.color_known {
            e = e.with_color(NEUTRAL_COLOR);
        }
        if let Some(level) = level {
            e.brightness = level;
        }
        levels_in_range(e)
    }

    /// Store per-key colours by LED: every key of the layout gets its colour
    /// (black when not given), every other slot keeps what was read.
    /// **Writes flash.** Shown in effect modes 20, 21 and 128.
    pub fn write_per_key(&mut self, colors: impl IntoIterator<Item = (u8, Rgb)>) -> Result<()> {
        self.ensure_writable()?;
        let mut want = [None; perkey::SLOTS];
        for (led, rgb) in colors {
            let aa = self.layout.aa(led).ok_or(AaError::UnknownLed(led))?;
            want[usize::from(aa)] = Some(rgb);
        }
        let fresh = self.read_block(cmd::PER_KEY, perkey::LEN, 0, None)?;
        let mut table = fresh.clone();
        for k in self.layout.keys() {
            perkey::set(&mut table, k.aa, want[usize::from(k.aa)].unwrap_or([0, 0, 0]));
        }
        self.put_per_key(&fresh, &table)
    }

    /// Write a per-key table built from `fresh` and read it back; nothing
    /// is written when it is unchanged.
    fn put_per_key(&mut self, fresh: &[u8], table: &[u8]) -> Result<()> {
        guard::guard_per_key(fresh, table)?;
        if fresh == table {
            return Ok(());
        }
        self.write_block(cmd::SET_PER_KEY, table, 0, true)?;
        let back = self.read_block(cmd::PER_KEY, perkey::LEN, 0, None)?;
        let ok = back == table;
        self.patch_snapshot(|s| s.per_key = Hex(back));
        if !ok {
            return Err(AaError::Readback { what: "per-key table" });
        }
        Ok(())
    }

    /// Switch to per-key mode at the app's `brightness` (0–5), everything
    /// else of the effect kept, then store the colours as
    /// [`Self::write_per_key`] does. **Writes flash.**
    pub fn apply_per_key(&mut self, colors: impl IntoIterator<Item = (u8, Rgb)>, brightness: u8) -> Result<()> {
        self.ensure_writable()?;
        let colors: Vec<(u8, Rgb)> = colors.into_iter().collect();
        if let Some(&(led, _)) = colors.iter().find(|(led, _)| self.layout.aa(*led).is_none()) {
            return Err(AaError::UnknownLed(led));
        }
        let level = effect::level(brightness)?;
        let current = self.read_effect()?;
        if current.effect.mode != effect::MODE_PER_KEY || current.effect.brightness != level {
            self.put_effect(&Self::per_key_effect(current, Some(level)))?;
        }
        self.write_per_key(colors)
    }

    /// The factory table of a layer: `1C` for the Fn layer, the model's
    /// own usages for the base layer.
    fn factory_table(&mut self, layer: Layer) -> Result<Vec<u8>> {
        Ok(match layer {
            Layer::Base => self.layout.factory_base().to_vec(),
            Layer::Fn => match self.snapshot.as_ref().map(|s| s.default_fn.0.clone()) {
                Some(t) if t.len() == keymap::LEN => t,
                _ => self.read_block(cmd::DEFAULT_FN, keymap::LEN, 0, None)?,
            },
        })
    }

    /// Change keys of one layer, by LED: only the slots asked for change,
    /// every other slot keeps what was read. `KeyAction::Default` gives a
    /// key its factory function back (the slot is written `00 00 00 00`
    /// and reads back as the factory entry); that is refused where the
    /// factory function in this layer is a firmware function (such as the
    /// factory reset on Fn+Space) the key does not hold now. The Fn key and
    /// the knob are never reassigned. **Writes flash.**
    pub fn apply_keymap(&mut self, layer: Layer, changes: impl IntoIterator<Item = (u8, KeyAction)>) -> Result<()> {
        self.ensure_writable()?;
        let mut wanted = Vec::new();
        for (led, action) in changes {
            let aa = self.layout.aa(led).ok_or(AaError::UnknownLed(led))?;
            wanted.push((aa, keymap::encode(action)?));
        }
        let fresh = self.read_block(layer_commands(layer).0, keymap::LEN, 0, None)?;
        let factory = self.factory_table(layer)?;
        let mut table = fresh.clone();
        for (aa, e) in wanted {
            let now = keymap::entry(&fresh, aa);
            if e == [0; 4] && now == keymap::entry(&factory, aa) {
                // Already its factory function: nothing to write.
                keymap::set_entry(&mut table, aa, now);
                continue;
            }
            if self.layout.is_protected(aa) && now != e {
                return Err(AaError::Unsupported("the Fn key and the knob are not reassigned"));
            }
            if e == [0; 4] && keymap::is_func(keymap::entry(&factory, aa)) {
                return Err(AaError::Unsupported(
                    "this key's factory function in this layer is a firmware function (such as a factory \
                     reset); it is not brought back",
                ));
            }
            keymap::set_entry(&mut table, aa, e);
        }
        self.put_keymap(layer, &fresh, &table, &factory)
    }

    /// Write a layer built from `fresh` and read it back; nothing is
    /// written when it is unchanged.
    fn put_keymap(&mut self, layer: Layer, fresh: &[u8], table: &[u8], factory: &[u8]) -> Result<()> {
        guard::guard_keymap(fresh, table, Some(factory))?;
        if fresh == table {
            return Ok(());
        }
        let (read, write) = layer_commands(layer);
        self.write_block(write, table, 0, true)?;
        let back = self.read_block(read, keymap::LEN, 0, None)?;
        let ok = keymap_matches(table, &back, factory);
        self.patch_snapshot(|s| match layer {
            Layer::Base => s.keymap = Hex(back),
            Layer::Fn => s.fn_keymap = Hex(back),
        });
        if !ok {
            return Err(AaError::Readback { what: "keymap" });
        }
        Ok(())
    }

    /// Replace every macro: `slots[i]` is macro `i`, an empty one is none.
    /// **Writes flash.**
    pub fn write_macros(&mut self, slots: &[Vec<MacroStep>]) -> Result<()> {
        self.ensure_writable()?;
        let entries = slots.iter().map(|s| macros::from_steps(s)).collect::<std::result::Result<Vec<_>, _>>()?;
        let img = macros::encode(&entries)?;
        self.put_macros(&img)
    }

    /// Write a macro image so that no table entry ever names a block that is
    /// not completely written: an all-zero table first (no macros), then the
    /// blocks from 400 (LAST on the final chunk), then the table, then the
    /// final block chunk once more, unchanged, with LAST. A failure part-way
    /// leaves the old macros, none, or the new ones, never a pointer into
    /// half-written blocks, and closes the gate.
    ///
    /// AJAZZ's WebHID driver writes the table (no LAST) and then the blocks,
    /// so its last packet is always the final block chunk with LAST, never a
    /// table chunk. If the firmware commits or reloads the macro area on
    /// LAST, a table written after the blocks would be left out; resending
    /// that chunk (the very bytes the WebHID driver ends with) makes the
    /// sequence end the same way. Beta check on a real AK980 PRO: write a
    /// macro, unplug and replug, read it back, press it.
    fn put_macros(&mut self, img: &macros::MacroImage) -> Result<()> {
        let empty = vec![0u8; macros::TABLE_LEN];
        self.write_block(cmd::SET_MACROS, &empty, 0, false)?;
        if !img.blocks.is_empty() {
            self.write_block(cmd::SET_MACROS, &img.blocks, macros::BLOCKS_AT, true)?;
            self.write_block(cmd::SET_MACROS, &img.table, 0, false)?;
            if let Some(last) = frame::chunks(img.blocks.len(), macros::BLOCKS_AT, true).pop() {
                self.write_block(cmd::SET_MACROS, &img.blocks[last.offset..], last.addr, true)?;
            }
        }
        let table = self.read_block(cmd::MACROS, macros::TABLE_LEN, 0, None)?;
        let blocks = match img.blocks.len() {
            0 => Vec::new(),
            n => self.read_block(cmd::MACROS, n, macros::BLOCKS_AT, None)?,
        };
        if table != img.table || blocks != img.blocks {
            self.sanity = Some(Err(vec![UNFINISHED_WRITE.into()]));
            return Err(AaError::Readback { what: "macros" });
        }
        let stored: Vec<MacroBlock> = macros::pointers(&img.table)
            .into_iter()
            .enumerate()
            .filter(|(_, p)| !macros::is_none(*p))
            .map(|(slot, p)| {
                let at = (p - u32::from(macros::BLOCKS_AT)) as usize;
                let len = 4 + 4 * macros::header_entries(&img.blocks[at..]);
                MacroBlock { slot: slot as u8, addr: p, bytes: Hex(img.blocks[at..at + len].to_vec()) }
            })
            .collect();
        self.patch_snapshot(|s| {
            s.macro_table = Hex(table);
            s.macro_blocks = stored;
        });
        Ok(())
    }

    /// Set the keyboard clock (`34`; AJAZZ's WebHID driver sends it on every
    /// connection).
    pub fn set_clock(&mut self, t: &ClockTime) -> Result<()> {
        self.ensure_writable()?;
        let data = clock::encode(t)?;
        self.write_block(cmd::CLOCK, &data, 0, true)
    }

    // ---------------------------------------------------------------- restore

    /// Build `layer` from `backup`, keeping the keyboard's entry where the
    /// backup's could not be written: the Fn key, the knob and the table
    /// tail always, and any slot the table guard refuses on its own. Every
    /// slot kept with a backup entry that differs is reported in `kept`.
    fn plan_layer(
        &self,
        layer: Layer,
        fresh: &[u8],
        backup: &[u8],
        factory: &[u8],
        kept: &mut Vec<KeptSlot>,
    ) -> Vec<u8> {
        let mut table = backup.to_vec();
        let fixed = self
            .layout
            .fn_slot()
            .into_iter()
            .map(|s| (s, "the Fn key is never reassigned".to_string()))
            .chain(KNOB_SLOTS.map(|s| (s, "the knob is never reassigned".to_string())))
            .chain(keymap::TAIL_SLOTS.map(|s| (s, Blocked::Tail(s).to_string())));
        for (slot, reason) in fixed {
            let (want, now) = (keymap::entry(backup, slot), keymap::entry(fresh, slot));
            if want != now {
                keymap::set_entry(&mut table, slot, now);
                kept.push(KeptSlot { layer, slot, led: self.layout.led(slot), backup: want, reason });
            }
        }
        for slot in 0..keymap::SLOTS as u8 {
            let (want, now) = (keymap::entry(&table, slot), keymap::entry(fresh, slot));
            if want == now {
                continue;
            }
            let mut probe = fresh.to_vec();
            keymap::set_entry(&mut probe, slot, want);
            if let Err(b) = guard::guard_keymap(fresh, &probe, Some(factory)) {
                keymap::set_entry(&mut table, slot, now);
                kept.push(KeptSlot { layer, slot, led: self.layout.led(slot), backup: want, reason: b.to_string() });
            }
        }
        table
    }

    /// Write a backup back: both keymap layers, the effect, the per-key
    /// table and the macros (not the options, not the screen).
    ///
    /// Everything is planned before anything is sent: fresh reads of every
    /// section, every table built and guarded, the effect and the macros
    /// encoded. Any refusal returns with nothing written. Backup entries
    /// the guard would refuse on their own (a firmware function the slot no
    /// longer holds, say) keep the keyboard's entry and are listed in the
    /// report; so are the Fn key, the knob and the table tails.
    ///
    /// The backup's effect colour is used when the backup knew it
    /// ([`AaSnapshot::effect_color`]), else `effect_color`. With neither,
    /// the effect is left alone when its mode and levels already match; a
    /// backup in per-key mode (128, which shows no effect colour) gets the
    /// colour [`Self::arm_live`] would give it ([`NEUTRAL_COLOR`]); any
    /// other restore is refused ([`AaError::ColorUnknown`]).
    ///
    /// Unchanged sections are not written. A write that fails returns
    /// [`AaError::RestoreIncomplete`] with the sections written before it.
    /// **Writes flash.**
    pub fn restore(&mut self, backup: &AaSnapshot, effect_color: Option<EffectColor>) -> Result<RestoreReport> {
        self.ensure_writable()?;
        check_sanity(backup, self.usb).map_err(AaError::NotSane)?;
        let (Some(now), Some(then)) = (self.info, backup.device_info()) else {
            return Err(AaError::NotSane(vec![NOTHING_READ.into()]));
        };
        if (now.vid, now.pid, now.product) != (then.vid, then.pid, then.product) {
            return Err(AaError::NotSane(vec!["the backup is from another keyboard".into()]));
        }

        // 1. Fresh reads.
        let fresh_base = self.read_block(cmd::KEYMAP, keymap::LEN, 0, None)?;
        let fresh_fn = self.read_block(cmd::FN_KEYMAP, keymap::LEN, 0, None)?;
        let factory_fn = self.read_block(cmd::DEFAULT_FN, keymap::LEN, 0, None)?;
        let fresh_effect = self.read_effect()?;
        let fresh_per_key = self.read_block(cmd::PER_KEY, perkey::LEN, 0, None)?;
        let fresh_macros = self.read_macro_slots()?;
        self.check_writable()?;

        // 2. Plan and check everything.
        let mut report = RestoreReport::default();
        let factory_base = self.layout.factory_base().to_vec();
        let base = self.plan_layer(Layer::Base, &fresh_base, &backup.keymap.0, &factory_base, &mut report.kept);
        let fn_layer = self.plan_layer(Layer::Fn, &fresh_fn, &backup.fn_keymap.0, &factory_fn, &mut report.kept);
        guard::guard_keymap(&fresh_base, &base, Some(&factory_base))?;
        guard::guard_keymap(&fresh_fn, &fn_layer, Some(&factory_fn))?;

        let saved = backup.effect().ok_or(AaError::Invalid(CodecError::Malformed("effect")))?;
        let color = backup.effect_color.map(EffectColor::from).or(effect_color);
        let same_levels = fresh_effect.effect.same_settings(&saved);
        let effect = match color {
            Some(c) if same_levels && fresh_effect.color() == Some(c) => None,
            Some(c) => {
                let e = levels_in_range(saved.with_color(c));
                e.encode()?;
                Some(e)
            }
            None if same_levels => None,
            None if saved.mode == effect::MODE_PER_KEY => {
                let e = Self::per_key_effect(EffectRead { effect: saved, color_known: false }, None);
                e.encode()?;
                Some(e)
            }
            None => return Err(AaError::ColorUnknown),
        };

        let mut per_key = backup.per_key.0.clone();
        for slot in perkey::TAIL_SLOTS {
            let at = usize::from(slot) * 4;
            per_key[at..at + 4].copy_from_slice(&fresh_per_key[at..at + 4]);
        }
        guard::guard_per_key(&fresh_per_key, &per_key)?;

        let saved_macros = backup.macros();
        let macro_image = if saved_macros == fresh_macros { None } else { Some(macros::encode(&saved_macros)?) };

        // 3. Write; a failure says what was written before it.
        if base != fresh_base {
            let r = self.put_keymap(Layer::Base, &fresh_base, &base, &factory_base);
            record(&mut report, "base layer", r)?;
        }
        if fn_layer != fresh_fn {
            let r = self.put_keymap(Layer::Fn, &fresh_fn, &fn_layer, &factory_fn);
            record(&mut report, "Fn layer", r)?;
        }
        if let Some(e) = effect {
            let r = self.put_effect(&e);
            record(&mut report, "effect", r)?;
        }
        if per_key != fresh_per_key {
            let r = self.put_per_key(&fresh_per_key, &per_key);
            record(&mut report, "per-key table", r)?;
        }
        if let Some(img) = macro_image {
            let r = self.put_macros(&img);
            record(&mut report, "macros", r)?;
        }
        Ok(report)
    }

    // ---------------------------------------------------------------- live lighting

    /// Make real-time frames visible: unless the effect is already in mode
    /// 20, 21 or 128, store it in mode 128 with everything else kept and
    /// return the effect it replaced (for [`Self::disarm_live`]; its colour
    /// may be unknown). A colour the driver cannot trust is not written: it
    /// goes in as [`NEUTRAL_COLOR`], which per-key mode does not show. When
    /// to arm is the caller's decision. **Writes flash.**
    pub fn arm_live(&mut self) -> Result<Option<EffectRead>> {
        self.ensure_writable()?;
        let current = self.read_effect()?;
        if current.effect.is_live() {
            return Ok(None);
        }
        self.put_effect(&Self::per_key_effect(current, None))?;
        Ok(Some(current))
    }

    /// Put back the effect [`Self::arm_live`] replaced. Its colour and
    /// colour mode come from `color` (what the app wrote or saved), else
    /// from `previous` when that one is known; otherwise nothing is written
    /// ([`AaError::ColorUnknown`]). **Writes flash.**
    pub fn disarm_live(&mut self, previous: EffectRead, color: Option<EffectColor>) -> Result<()> {
        self.ensure_writable()?;
        let c = color.or(previous.color()).ok_or(AaError::ColorUnknown)?;
        self.put_effect(&levels_in_range(previous.effect.with_color(c)))
    }

    /// Show one real-time frame (`32`, RAM only): every key of the layout,
    /// black unless given; LEDs that are no key are ignored. Refused with
    /// [`AaError::NotArmed`] unless the effect is in mode 20, 21 or 128.
    pub fn realtime_frame(&mut self, leds: impl IntoIterator<Item = (u8, Rgb)>) -> Result<()> {
        self.ensure_writable()?;
        if !self.effect.is_some_and(|e| e.effect.is_live()) {
            return Err(AaError::NotArmed);
        }
        let mut colors = [[0u8; 3]; perkey::SLOTS];
        for (led, rgb) in leds {
            if let Some(aa) = self.layout.aa(led) {
                colors[usize::from(aa)] = rgb;
            }
        }
        let f = realtime::frame(&self.layout.order(), |aa| colors[usize::from(aa)]);
        self.write_block(cmd::REALTIME, &f, 0, true)
    }

    // ---------------------------------------------------------------- screen

    /// Upload an image or animation to the screen over FF67 (`display`),
    /// one packet at a time, each waiting for FF67's answer, which is no
    /// proof (FF67 answers anything). Holding `self` keeps the command
    /// channel silent meanwhile. Every packet is checked before the first
    /// is sent; `cancel` stops before the next one. Once a packet has gone
    /// out, stopping part-way (cancel or error) leaves the screen incomplete
    /// until the next upload and silences the command channel for
    /// [`UPLOAD_COOLDOWN`] ([`AaError::UploadInterrupted`]). **Writes the
    /// keyboard's flash**, and on firmware 769 replaces the factory clock
    /// screen, which only a full factory reset brings back.
    pub fn upload_screen<D: OutputPort>(
        &mut self,
        display: &mut D,
        anim: &ScreenAnimation,
        mut progress: impl FnMut(usize, usize),
        cancel: &AtomicBool,
    ) -> Result<AaScreenReport> {
        self.ensure_idle()?;
        self.ensure_writable()?;
        let info = self.info.ok_or_else(|| AaError::NotSane(vec![NOTHING_READ.into()]))?;
        if !info.has_screen() {
            return Err(AaError::Unsupported("the keyboard reports no screen"));
        }
        let limit = screen::frame_limit(info.tft_max_frames);
        let frames = anim.frames().len();
        if frames > limit {
            return Err(AaError::Invalid(CodecError::Range {
                field: "frames",
                value: frames as u32,
                min: 1,
                max: limit as u32,
            }));
        }
        let blob = screen::blob(anim);
        let total = blob.len().div_ceil(screen::PAYLOAD);
        let packets: Vec<Vec<u8>> = (0..total)
            .map(|i| {
                let mut w = vec![0u8; screen::WIRE_LEN];
                w[1..].copy_from_slice(&screen::packet(&blob, i, total));
                w
            })
            .collect();
        for p in &packets {
            guard::guard(Channel::Display, p)?;
        }
        let drained = display.drain();
        let mut report =
            AaScreenReport { packets: total, drained, stale_acks: 0, packet_times: Vec::with_capacity(total) };
        for (i, p) in packets.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                if i > 0 {
                    self.upload_interrupted = Some(Instant::now());
                }
                return Err(AaError::Cancelled { done: i, total });
            }
            let t = Instant::now();
            match screen_exchange(display, p, &mut self.watch) {
                Ok(stale) => report.stale_acks += stale,
                Err(e) => {
                    self.upload_interrupted = Some(Instant::now());
                    return Err(e);
                }
            }
            report.packet_times.push(t.elapsed());
            progress(i + 1, total);
        }
        self.after_io();
        Ok(report)
    }
}
