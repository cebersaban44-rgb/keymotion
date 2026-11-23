//! Driver for AJAZZ's "AA" command family: the AK980 PRO on firmware 769
//! (USB 0C45:8039). Protocol: `kb_protocol::aa` and the protocol notes.
//!
//! * Every packet, on both channels, goes through one function
//!   ([`exchange`]) that runs [`kb_protocol::aa::guard::guard`] first. Keymap
//!   and per-key tables are also checked as a whole against the fresh read
//!   they were built from.
//! * Nothing is written before a full read ([`AaKeyboard::read_all`]) passed
//!   the sanity gate ([`check_sanity`]), and nothing at all is read or
//!   written while the keyboard waits for a firmware update.
//! * Tables are read fresh, changed only where asked, written whole and
//!   read back. The effect's colour does not read back reliably, so only its
//!   mode and levels are compared; the driver remembers the colour it wrote.
//! * The app names keys by LED number; the driver translates through the
//!   model's [`AaLayout`].
//! * A failed write leaves the HID channel unusable (hidapi keeps the write
//!   pending): on [`AaError::needs_reopen`], drop the driver, reopen the
//!   channel and read everything again.

mod channel;
mod layout;
#[cfg(any(test, feature = "sim"))]
pub mod sim;
mod snapshot;
#[cfg(test)]
mod tests;

pub use channel::open_channel;
pub use kb_protocol::aa::effect::AaEffect;
pub use kb_protocol::aa::info::DeviceInfo;
pub use kb_protocol::aa::keymap::Binding;
pub use kb_protocol::aa::macros::AaMacroEntry;
pub use layout::{AaLayout, LayoutError, LayoutKey, KNOB_SLOTS};
pub use snapshot::{check as check_sanity, AaSnapshot, DeviceRecord, Hex, MacroBlock};

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

/// How long to wait for a reply (AJAZZ's WebHID driver: 500 ms, longer for
/// the big writes, 50 ms per real-time chunk).
pub fn timeout_ms(c: u8) -> u32 {
    match c {
        cmd::SET_KEYMAP | cmd::SET_FN_KEYMAP => 1000,
        cmd::SET_PER_KEY | cmd::SCREEN => 2000,
        cmd::REALTIME => realtime::CHUNK_TIMEOUT_MS,
        _ => 500,
    }
}

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
}

impl AaError {
    /// The channel must be dropped and opened again before it is used
    /// (a write failed or stalled; hidapi keeps it pending).
    pub fn needs_reopen(&self) -> bool {
        matches!(self, Self::Transport(_) | Self::ShortWrite { .. })
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
    resets: u64,
}

impl Watch {
    fn note(&mut self, report: &[u8]) {
        self.unexpected += 1;
        let notice = notify::notice(report);
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
    addr: Option<u16>,
    /// What a write's echo must repeat.
    echo: Option<&'a [u8]>,
}

fn is_reply(report: &[u8], expect: &Expect<'_>) -> bool {
    frame::parse_reply(report).is_some_and(|r| r.cmd == expect.cmd && expect.addr.is_none_or(|a| r.addr == a))
}

/// The one way a packet reaches a keyboard: guard it, write it, wait for
/// its reply (skipping and counting anything else), up to [`ATTEMPTS`]
/// times. A failed write is returned at once: the channel must be reopened.
fn exchange<P: OutputPort>(
    port: &mut P,
    channel: Channel,
    wire: &[u8],
    expect: &Expect<'_>,
    timeout_ms: u32,
    watch: &mut Watch,
) -> Result<Vec<u8>> {
    guard::guard(channel, wire)?;
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

/// What a screen upload measured.
#[derive(Debug, Clone)]
pub struct AaScreenReport {
    pub packets: usize,
    /// Input reports already queued on FF67 before the upload.
    pub drained: usize,
    /// Write-to-answer time of every packet.
    pub packet_times: Vec<Duration>,
}

const RESET_REASON: &str = "the keyboard reported a reset (55 FC 05); read it again before writing";
const NOTHING_READ: &str = "nothing has been read from the keyboard yet";
const UNFINISHED: &str = "the last full read of the keyboard did not finish";

/// Levels in range for a write: off (mode 0) may hold anything, which is
/// not shown; everything else passed the sanity gate already.
fn levels_in_range(e: AaEffect) -> AaEffect {
    let fit = |v: u8| v.clamp(effect::MIN_LEVEL, effect::MAX_LEVEL);
    AaEffect { brightness: fit(e.brightness), speed: fit(e.speed), ..e }
}

fn layer_commands(layer: Layer) -> (u8, u8) {
    match layer {
        Layer::Base => (cmd::KEYMAP, cmd::SET_KEYMAP),
        Layer::Fn => (cmd::FN_KEYMAP, cmd::SET_FN_KEYMAP),
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
    /// The effect the keyboard holds, as last read or written (colour from
    /// the last write when the read-back cannot tell).
    effect: Option<AaEffect>,
    /// The last effect written.
    written: Option<AaEffect>,
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
        }
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

    /// How often the keyboard reported a reset (`55 FC 05`).
    pub fn resets_reported(&self) -> u64 {
        self.watch.resets
    }

    /// The device information last read.
    pub fn info(&self) -> Option<&DeviceInfo> {
        self.info.as_ref()
    }

    /// The last full read, kept up to date by every write.
    pub fn snapshot(&self) -> Option<&AaSnapshot> {
        self.snapshot.as_ref()
    }

    /// The effect the keyboard holds, as far as the driver knows.
    pub fn current_effect(&self) -> Option<AaEffect> {
        self.effect
    }

    /// Whether writes are allowed: a full read passed the sanity gate and
    /// nothing has invalidated it since.
    pub fn is_sane(&self) -> bool {
        !self.update_pending && matches!(self.sanity, Some(Ok(())))
    }

    /// Why writes are refused (empty when they are allowed).
    pub fn sanity_reasons(&self) -> Vec<String> {
        match &self.sanity {
            _ if self.update_pending => vec![AaError::UpdatePending.to_string()],
            None => vec![NOTHING_READ.into()],
            Some(Ok(())) => Vec::new(),
            Some(Err(why)) => why.clone(),
        }
    }

    pub fn into_inner(self) -> P {
        self.port
    }

    // ---------------------------------------------------------------- I/O

    fn ensure_readable(&self) -> Result<()> {
        if self.update_pending {
            return Err(AaError::UpdatePending);
        }
        Ok(())
    }

    fn ensure_writable(&self) -> Result<()> {
        self.ensure_readable()?;
        match &self.sanity {
            Some(Ok(())) => Ok(()),
            Some(Err(why)) => Err(AaError::NotSane(why.clone())),
            None => Err(AaError::NotSane(vec![NOTHING_READ.into()])),
        }
    }

    /// How long to wait for command `c`'s reply: [`timeout_ms`], or 2 s on
    /// a board reporting frame version 1 (as AJAZZ's WebHID driver does).
    fn timeout(&self, c: u8) -> u32 {
        if self.info.is_some_and(|i| i.frame_version == 1) {
            2000
        } else {
            timeout_ms(c)
        }
    }

    /// A reset reported since the last look makes everything known stale.
    fn after_io(&mut self) {
        if self.watch.resets != self.resets_seen {
            self.resets_seen = self.watch.resets;
            self.effect = None;
            self.sanity = Some(Err(vec![RESET_REASON.into()]));
        }
    }

    /// Read `len` bytes of block `c` from address `base`. `request` is the
    /// data each chunk carries (only `33` sends any).
    fn read_block(&mut self, c: u8, len: usize, base: u16, request: Option<&[u8]>) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        let timeout = self.timeout(c);
        let result = (|| -> Result<()> {
            for ch in frame::chunks(len, base, true) {
                let data = request.map_or(&[][..], |r| &r[ch.offset..]);
                let wire = frame::wire(&frame::request(c, &ch, 0, data));
                let expect = Expect { cmd: c, addr: Some(ch.addr), echo: None };
                let r = exchange(&mut self.port, Channel::Command, &wire, &expect, timeout, &mut self.watch)?;
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
    fn write_block(&mut self, c: u8, data: &[u8], base: u16, last_flag: bool) -> Result<()> {
        let timeout = self.timeout(c);
        let result = (|| -> Result<()> {
            for ch in frame::chunks(data.len(), base, last_flag) {
                let part = &data[ch.offset..ch.offset + usize::from(ch.len)];
                let wire = frame::wire(&frame::request(c, &ch, 0, part));
                let expect = Expect { cmd: c, addr: Some(ch.addr), echo: Some(part) };
                exchange(&mut self.port, Channel::Command, &wire, &expect, timeout, &mut self.watch)?;
            }
            Ok(())
        })();
        self.after_io();
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
        let resets = self.watch.resets;
        // Until this read is through, the last verdict no longer holds.
        self.sanity = Some(Err(vec![UNFINISHED.into()]));
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
        let snap = AaSnapshot {
            format: AaSnapshot::FORMAT,
            info: Hex(raw_info),
            device: i.into(),
            options: Hex(options),
            keymap: Hex(keymap),
            fn_keymap: Hex(fn_keymap),
            default_fn: Hex(default_fn),
            effect: Hex(effect.clone()),
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
        if let Ok(e) = AaEffect::decode(&effect) {
            self.remember(e);
        }
        self.snapshot = Some(snap.clone());
        Ok(snap)
    }

    /// Read the block every non-empty macro slot points to. Addresses
    /// outside the macro memory are not read (the sanity gate reports them).
    fn read_macro_blocks(&mut self, table: &[u8], i: &DeviceInfo) -> Result<Vec<MacroBlock>> {
        let limit = snapshot::macro_limit(i);
        let mut blocks = Vec::new();
        for (slot, p) in macros::pointers(table).into_iter().enumerate() {
            if p < u32::from(macros::BLOCKS_AT) || p + 4 > limit {
                continue;
            }
            let mut bytes = self.read_block(cmd::MACROS, 4, p as u16, None)?;
            let body = 4 * macros::header_entries(&bytes);
            if body > 0 && p + 4 + body as u32 <= limit {
                bytes.extend(self.read_block(cmd::MACROS, body, (p + 4) as u16, None)?);
            }
            blocks.push(MacroBlock { slot: slot as u8, addr: p, bytes: Hex(bytes) });
        }
        Ok(blocks)
    }

    /// The effect read back, with the colour this driver wrote when the
    /// read-back cannot tell it.
    fn remember(&mut self, read: AaEffect) -> AaEffect {
        let e = match self.written {
            Some(w) if w.same_settings(&read) => AaEffect { color: w.color, color_mode: w.color_mode, ..read },
            _ => read,
        };
        self.effect = Some(e);
        e
    }

    /// Read the effect (`13`).
    pub fn read_effect(&mut self) -> Result<AaEffect> {
        self.ensure_readable()?;
        let raw = self.read_block(cmd::EFFECT, effect::LEN, 0, None)?;
        let e = AaEffect::decode(&raw)?;
        Ok(self.remember(e))
    }

    /// The effect as the app describes it.
    pub fn effect_settings(&mut self) -> Result<EffectSettings> {
        Ok(self.read_effect()?.to_settings())
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
        let i = match self.info {
            Some(i) => i,
            None => self.device_info()?,
        };
        let table = self.read_block(cmd::MACROS, macros::TABLE_LEN, 0, None)?;
        let mut slots = vec![Vec::new(); macros::MAX_MACROS];
        for b in self.read_macro_blocks(&table, &i)? {
            slots[usize::from(b.slot)] = macros::decode_entries(b.bytes.0.get(4..).unwrap_or_default());
        }
        Ok(slots)
    }

    // ---------------------------------------------------------------- writing

    /// Write an effect and read it back (mode and levels only).
    fn put_effect(&mut self, e: &AaEffect) -> Result<()> {
        let data = e.encode()?;
        self.write_block(cmd::SET_EFFECT, &data, 0, true)?;
        self.written = Some(*e);
        let raw = self.read_block(cmd::EFFECT, effect::LEN, 0, None)?;
        let back = AaEffect::decode(&raw)?;
        self.remember(back);
        self.patch_snapshot(|s| s.effect = Hex(raw));
        if !e.same_settings(&back) {
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
        if current.mode != effect::MODE_PER_KEY || current.brightness != level {
            self.put_effect(&levels_in_range(AaEffect { brightness: level, ..current.per_key() }))?;
        }
        self.write_per_key(colors)
    }

    /// Change keys of one layer, by LED: only the slots asked for change,
    /// every other slot keeps what was read. `KeyAction::Default` gives a
    /// key its factory function back. The Fn key and the knob are never
    /// reassigned. **Writes flash.**
    pub fn apply_keymap(&mut self, layer: Layer, changes: impl IntoIterator<Item = (u8, KeyAction)>) -> Result<()> {
        self.ensure_writable()?;
        let mut wanted = Vec::new();
        for (led, action) in changes {
            let aa = self.layout.aa(led).ok_or(AaError::UnknownLed(led))?;
            wanted.push((aa, keymap::encode(action)?));
        }
        let fresh = self.read_block(layer_commands(layer).0, keymap::LEN, 0, None)?;
        let mut table = fresh.clone();
        for (aa, e) in wanted {
            if self.layout.is_protected(aa) && keymap::entry(&fresh, aa) != e {
                return Err(AaError::Unsupported("the Fn key and the knob are not reassigned"));
            }
            keymap::set_entry(&mut table, aa, e);
        }
        self.put_keymap(layer, &fresh, &table)
    }

    /// Write a layer built from `fresh` and read it back; nothing is
    /// written when it is unchanged.
    fn put_keymap(&mut self, layer: Layer, fresh: &[u8], table: &[u8]) -> Result<()> {
        guard::guard_keymap(fresh, table)?;
        if fresh == table {
            return Ok(());
        }
        let (read, write) = layer_commands(layer);
        self.write_block(write, table, 0, true)?;
        let back = self.read_block(read, keymap::LEN, 0, None)?;
        let ok = back == table;
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
        self.put_macros(&entries)
    }

    fn put_macros(&mut self, entries: &[Vec<AaMacroEntry>]) -> Result<()> {
        let img = macros::encode(entries)?;
        // The table first, LAST on no chunk; then the blocks from 400.
        self.write_block(cmd::SET_MACROS, &img.table, 0, false)?;
        if !img.blocks.is_empty() {
            self.write_block(cmd::SET_MACROS, &img.blocks, macros::BLOCKS_AT, true)?;
        }
        let table = self.read_block(cmd::MACROS, macros::TABLE_LEN, 0, None)?;
        let blocks = match img.blocks.len() {
            0 => Vec::new(),
            n => self.read_block(cmd::MACROS, n, macros::BLOCKS_AT, None)?,
        };
        if table != img.table || blocks != img.blocks {
            return Err(AaError::Readback { what: "macros" });
        }
        let stored: Vec<MacroBlock> = macros::pointers(&img.table)
            .into_iter()
            .enumerate()
            .filter(|(_, p)| *p != 0)
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

    /// Write a backup back: both keymap layers, the effect, the per-key
    /// table and the macros, each through the normal checks (not the
    /// options, not the screen). The backup must pass the sanity gate for
    /// this keyboard; the Fn key, the knob and the table tails keep what
    /// the keyboard holds now. **Writes flash.**
    pub fn restore(&mut self, backup: &AaSnapshot) -> Result<()> {
        self.ensure_writable()?;
        check_sanity(backup, self.usb).map_err(AaError::NotSane)?;
        let (Some(now), Some(then)) = (self.info, backup.device_info()) else {
            return Err(AaError::NotSane(vec![NOTHING_READ.into()]));
        };
        if (now.vid, now.pid, now.product) != (then.vid, then.pid, then.product) {
            return Err(AaError::NotSane(vec!["the backup is from another keyboard".into()]));
        }
        for (layer, saved) in [(Layer::Base, &backup.keymap), (Layer::Fn, &backup.fn_keymap)] {
            let fresh = self.read_block(layer_commands(layer).0, keymap::LEN, 0, None)?;
            let mut table = saved.0.clone();
            let kept = self.layout.fn_slot().into_iter().chain(KNOB_SLOTS).chain(keymap::TAIL_SLOTS);
            for slot in kept {
                keymap::set_entry(&mut table, slot, keymap::entry(&fresh, slot));
            }
            self.put_keymap(layer, &fresh, &table)?;
        }
        let e = backup.effect().ok_or(AaError::Invalid(CodecError::Malformed("effect")))?;
        self.put_effect(&levels_in_range(e))?;
        let fresh = self.read_block(cmd::PER_KEY, perkey::LEN, 0, None)?;
        let mut table = backup.per_key.0.clone();
        for slot in perkey::TAIL_SLOTS {
            let at = usize::from(slot) * 4;
            table[at..at + 4].copy_from_slice(&fresh[at..at + 4]);
        }
        self.put_per_key(&fresh, &table)?;
        self.put_macros(&backup.macros())
    }

    // ---------------------------------------------------------------- live lighting

    /// Make real-time frames visible: unless the effect is already in mode
    /// 20, 21 or 128, store it in mode 128 with everything else kept and
    /// return the effect it replaced (for [`Self::disarm_live`]). When to
    /// arm is the caller's decision. **Writes flash.**
    pub fn arm_live(&mut self) -> Result<Option<AaEffect>> {
        self.ensure_writable()?;
        let current = self.read_effect()?;
        if current.is_live() {
            return Ok(None);
        }
        self.put_effect(&levels_in_range(current.per_key()))?;
        Ok(Some(current))
    }

    /// Put back the effect [`Self::arm_live`] replaced. **Writes flash.**
    pub fn disarm_live(&mut self, previous: AaEffect) -> Result<()> {
        self.ensure_writable()?;
        self.put_effect(&levels_in_range(previous))
    }

    /// Show one real-time frame (`32`, RAM only): every key of the layout,
    /// black unless given; LEDs that are no key are ignored. Refused with
    /// [`AaError::NotArmed`] unless the effect is in mode 20, 21 or 128.
    pub fn realtime_frame(&mut self, leds: impl IntoIterator<Item = (u8, Rgb)>) -> Result<()> {
        self.ensure_writable()?;
        if !self.effect.is_some_and(|e| e.is_live()) {
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
    /// is sent; `cancel` stops before the next one (the screen is then
    /// incomplete until the next upload). **Writes the keyboard's flash**,
    /// and on firmware 769 replaces the factory clock screen, which only a
    /// full factory reset brings back.
    pub fn upload_screen<D: OutputPort>(
        &mut self,
        display: &mut D,
        anim: &ScreenAnimation,
        mut progress: impl FnMut(usize, usize),
        cancel: &AtomicBool,
    ) -> Result<AaScreenReport> {
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
        let mut packet_times = Vec::with_capacity(total);
        for (i, p) in packets.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(AaError::Cancelled { done: i, total });
            }
            let t = Instant::now();
            let expect = Expect { cmd: cmd::SCREEN_ACK, addr: None, echo: None };
            exchange(display, Channel::Display, p, &expect, screen::ACK_TIMEOUT_MS, &mut self.watch)?;
            packet_times.push(t.elapsed());
            progress(i + 1, total);
        }
        self.after_io();
        Ok(AaScreenReport { packets: total, drained, packet_times })
    }
}
