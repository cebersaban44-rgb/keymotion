//! A simulated AK980 PRO on firmware 769: both "AA" collections (FF68 and
//! FF67) over one shared state, for tests and for the app's development
//! without a keyboard (cargo feature `sim`).
//!
//! It behaves as the official 769 firmware's code does (protocol notes §F):
//!
//! * Settings live in flash. A write chunk erases its 1 KB page only when
//!   it starts on the page's first byte, then programs LEN (rounded up to
//!   4) bytes over what is there, bit by bit as AND: a chunk patched into
//!   a page that was not erased mixes old and new. LAST is ignored.
//! * Every request is answered with its own bytes (`55` first): a read
//!   with the data filled in, a write echoed whether it was done or not.
//! * Writes are dropped, echoed all the same, during the power-on
//!   animation ([`SimState::power_on`]) and the reset blink
//!   ([`Faults::reset_blink`]); real-time frames are ignored during the
//!   reset.
//! * `22` and `26` replace an all-zero entry with the layer's default
//!   (`0xB000` / `0xB400`) before programming it.
//! * `13` reports the running effect: modes 0, 20, 21 and 128 as 0, a
//!   custom colour (any effect write sets one) as `FF FF FF` with colour
//!   mode 1. An effect write with an unknown mode is ignored.
//! * The per-key table shows an entry only when its first byte is its
//!   index; the factory table is all `00` ([`SimState::factory`]; all `FF`:
//!   [`SimState::erased_per_key`]).
//! * `32` frames show in any effect mode, by the slot in each entry's
//!   first byte, and the effect shows again [`SimState::frame_hold`] (5 s)
//!   after the last one.
//! * `10` is a fixed flash block with the work mode, battery, charge state
//!   and the frame limit 146 filled in.
//! * FF67 answers everything with `55 41 00 01`.
//!
//! [`Faults`] make it misbehave further. It also checks the driver: a
//! packet the guard refuses, a keymap write the table guard refuses, a
//! table write that does not run from address 0 upwards, a screen packet
//! written before the previous answer was read, or command traffic while
//! an upload is open (until [`SimState::upload_quiet`] has passed since its
//! last packet) makes it panic, failing the test.

use super::{AaError, OutputPort, Result, UsbIdentity};
use kb_devices::Model;
use kb_protocol::aa::guard::{self, Channel};
use kb_protocol::aa::{cmd, effect, frame, info, keymap, macros, perkey, screen};
use kb_transport::TransportError;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// What the simulated keyboard gets wrong, on purpose.
#[derive(Debug, Clone, Default)]
pub struct Faults {
    /// No reply to the n-th FF68 request (1-based).
    pub drop_reply: Option<usize>,
    /// No reply to any FF68 request from the n-th on.
    pub mute_from: Option<usize>,
    /// `55 FC 05` (keyboard reset by its key combination) before the reply
    /// to the n-th request: the keymap layers go back to their defaults
    /// (per-key table, effect and macros stay), and the reset blink starts
    /// when [`Self::reset_blink`] is set.
    pub reset_before: Option<usize>,
    /// How long the reset blink lasts: writes are dropped and frames
    /// ignored meanwhile.
    pub reset_blink: Option<Duration>,
    /// A stray reply (another address) before the reply to the n-th request.
    pub stray_before: Option<usize>,
    /// `10` is never answered.
    pub silent_info: bool,
    /// Replies to this read command come back with their first data byte
    /// inverted.
    pub corrupt: Option<u8>,
    /// The n-th FF68 write fails (as a stalled write does).
    pub fail_write: Option<usize>,
    /// Writes are echoed with their first data byte changed.
    pub garble_echo: bool,
    /// Writes of this command are echoed but not done.
    pub ignore_writes: Option<u8>,
    /// Every write from the n-th FF68 request on is echoed but not done, as
    /// in a power-on animation that started meanwhile.
    pub drop_writes_from: Option<usize>,
    /// The answer to the n-th FF67 packet comes only after the next one.
    pub late_ack: Option<usize>,
    /// The n-th FF67 packet is answered twice.
    pub double_ack: Option<usize>,
    /// The n-th FF67 packet is never answered.
    pub drop_ack: Option<usize>,
    /// No FF67 packet is answered from the n-th on.
    pub mute_display_from: Option<usize>,
}

/// Everything the simulated keyboard holds and saw.
#[derive(Debug, Clone)]
pub struct SimState {
    /// The device information block in flash (`0x8F80`); bytes 16, 17, 18
    /// and 22 of a reply are the four fields below.
    pub info: [u8; info::LEN],
    pub work_mode: u8,
    /// Battery in percent (a reply says 0 for 255 and more).
    pub battery: u8,
    pub charge: u8,
    /// The screen frame limit (146 on 769).
    pub tft_frames: u8,
    pub options: [u8; 56],
    pub keymap: Vec<u8>,
    /// The base layer's defaults (`0xB000`): what a slot written `00 00 00
    /// 00` becomes, and what a key-combination reset brings back.
    pub factory_keymap: Vec<u8>,
    pub fn_keymap: Vec<u8>,
    /// The Fn layer's defaults (`0xB400`, read as `1C`).
    pub default_fn: Vec<u8>,
    /// The effect block in flash; the running effect follows it.
    pub effect: [u8; effect::LEN],
    /// The effect colour is the custom one (any effect write makes it so):
    /// it reads back as `FF FF FF`, and the per-key table shows.
    pub custom_color: bool,
    /// The palette colour shown and read back while it is not custom.
    pub palette_color: [u8; 3],
    pub per_key: Vec<u8>,
    /// Macro memory (three pages).
    pub macros: Vec<u8>,
    /// The last clock block written.
    pub clock: Option<Vec<u8>>,
    /// Real-time frames received (as the driver cut them, by address), and
    /// whether they showed.
    pub frames: Vec<(Vec<u8>, bool)>,
    /// Frame colours by slot: what the LEDs show while frames run.
    pub frame_leds: [Option<[u8; 3]>; perkey::SLOTS],
    /// When the last frame came.
    pub frame_at: Option<Instant>,
    /// How long after the last frame the effect shows again.
    pub frame_hold: Duration,
    /// The power-on animation runs until then: writes are dropped.
    pub booting_until: Option<Instant>,
    /// The reset blink runs until then: writes are dropped, frames ignored.
    pub resetting_until: Option<Instant>,
    /// Writes dropped (echoed, not done), by command.
    pub dropped: Vec<u8>,
    /// FF67 packets received (report ID dropped).
    pub screen: Vec<Vec<u8>>,
    /// FF68 packets received, as written.
    pub packets: Vec<Vec<u8>>,
    /// How long after its last packet an unfinished upload counts as
    /// abandoned (command traffic before that is a violation).
    pub upload_quiet: Duration,
    pub faults: Faults,
    requests: usize,
    writes: usize,
    screen_writes: usize,
    command_replies: VecDeque<Vec<u8>>,
    display_replies: VecDeque<Vec<u8>>,
    held_ack: Option<Vec<u8>>,
    /// The table write in progress: its command, where its last chunk
    /// started and ended.
    writing: Option<(u8, usize, usize)>,
    /// A keymap write in progress: the table before it and the table asked
    /// for (checked with the table guard when its last byte comes).
    pending_keymap: Option<(Vec<u8>, Vec<u8>)>,
    upload_open: bool,
    last_screen: Option<Instant>,
}

fn put(table: &mut [u8], slot: u8, e: [u8; 4]) {
    keymap::set_entry(table, slot, e);
}

/// The effect the factory state runs: static, one palette colour (not
/// white rainbow, which is what the custom-colour quirk reports).
pub const FACTORY_EFFECT: [u8; effect::LEN] = [1, 0x00, 0x80, 0xFF, 0xFF, 0, 0, 0, 0, 5, 3, 0, 0, 0, 0, 0];

/// What 769's reverse mode map reports for a running mode (§F.6).
fn mode_as_read(m: u8) -> u8 {
    if effect::READS_AS_OFF.contains(&m) {
        0
    } else {
        m
    }
}

/// Program `data` at `offset` of a flash region that starts on a page:
/// the page is erased first when `offset` is its first byte, then every
/// byte is ANDed in. Past the region is firmware code on the real board.
fn program(region: &mut [u8], offset: usize, data: &[u8]) {
    assert!(
        offset + data.len() <= region.len(),
        "a write runs past its table ({} > {}): it would erase firmware code",
        offset + data.len(),
        region.len()
    );
    if offset % frame::PAGE == 0 {
        let end = (offset + frame::PAGE).min(region.len());
        region[offset..end].fill(0xFF);
    }
    for (b, d) in region[offset..].iter_mut().zip(data) {
        *b &= d;
    }
}

impl SimState {
    /// The state of a board fresh from AJAZZ's 769 updater, for `model`
    /// (its keys need `aa`): per-key table and macros all 00, keymaps from
    /// the model's usages.
    pub fn factory(model: &Model) -> Self {
        let mut info = [0u8; info::LEN];
        // The flash block: the real reply's first 16 bytes.
        info[..16].copy_from_slice(&info::FIRMWARE_769_CABLE[..16]);
        let mut options = [0u8; 56];
        options[3] = 5;
        options[4] = 2;

        let keys = model.geometry.as_ref().map(|g| g.keys.as_slice()).unwrap_or_default();
        let mut base = vec![0u8; keymap::LEN];
        for k in keys {
            if let Some(aa) = k.aa {
                put(&mut base, aa, [keymap::page::KEYBOARD, 0, k.hid, 0]);
            }
        }
        // The knob: volume up, volume down, mute.
        put(&mut base, 13, [keymap::page::CONSUMER, 0xE9, 0, 0]);
        put(&mut base, 14, [keymap::page::CONSUMER, 0xEA, 0, 0]);
        put(&mut base, 15, [keymap::page::CONSUMER, 0xE2, 0, 0]);

        // Fn layer: media keys on F1–F12, firmware functions on a few keys,
        // factory reset (FUNC 1) on Space as 769's own default Fn layer has.
        let mut fn_layer = vec![0u8; keymap::LEN];
        let media: [u16; 12] =
            [0x0183, 0x00EA, 0x00E9, 0x00E2, 0x00B6, 0x00CD, 0x00B5, 0x018A, 0x0192, 0x0194, 0x0223, 0x0221];
        for (i, u) in media.into_iter().enumerate() {
            let [lo, hi] = u.to_le_bytes();
            put(&mut fn_layer, i as u8 + 1, [keymap::page::CONSUMER, lo, hi, 0]);
        }
        let func = |id: u32| {
            let [_, a, b, c] = id.to_be_bytes();
            [keymap::page::FUNC, a, b, c]
        };
        put(&mut fn_layer, 83, func(1)); // Space: factory reset
        put(&mut fn_layer, 92, func(7)); // Backspace: battery
        put(&mut fn_layer, 90, func(13)); // ↑: brightness up
        put(&mut fn_layer, 89, func(14)); // ↓: brightness down
        put(&mut fn_layer, 106, func(11)); // Delete: next effect

        Self {
            info,
            work_mode: 0,
            battery: 100,
            charge: 2,
            tft_frames: info::FRAME_LIMIT_769 as u8,
            options,
            factory_keymap: base.clone(),
            keymap: base,
            default_fn: fn_layer.clone(),
            fn_keymap: fn_layer,
            effect: FACTORY_EFFECT,
            custom_color: false,
            palette_color: [FACTORY_EFFECT[1], FACTORY_EFFECT[2], FACTORY_EFFECT[3]],
            per_key: vec![0u8; perkey::LEN],
            macros: vec![0u8; macros::AREA_LEN],
            clock: None,
            frames: Vec::new(),
            frame_leds: [None; perkey::SLOTS],
            frame_at: None,
            frame_hold: Duration::from_secs(5),
            booting_until: None,
            resetting_until: None,
            dropped: Vec::new(),
            screen: Vec::new(),
            packets: Vec::new(),
            upload_quiet: Duration::from_secs(1),
            faults: Faults::default(),
            requests: 0,
            writes: 0,
            screen_writes: 0,
            command_replies: VecDeque::new(),
            display_replies: VecDeque::new(),
            held_ack: None,
            writing: None,
            pending_keymap: None,
            upload_open: false,
            last_screen: None,
        }
    }

    /// A board whose per-key page was never programmed: all `FF`.
    pub fn erased_per_key(&mut self) {
        self.per_key.fill(0xFF);
    }

    /// The keyboard was just switched on: its power-on animation runs for
    /// `animation`, and writes meanwhile are dropped.
    pub fn power_on(&mut self, animation: Duration) {
        self.booting_until = Some(Instant::now() + animation);
    }

    /// FF68 packets whose command is `c`.
    pub fn sent(&self, c: u8) -> Vec<&Vec<u8>> {
        self.packets.iter().filter(|p| p[2] == c).collect()
    }

    /// Store an effect as another program would (the colour then reads back
    /// as the custom-colour quirk, like after any write).
    pub fn set_effect_by_another_tool(&mut self, e: [u8; effect::LEN]) {
        self.store_effect(&e);
    }

    /// The effect as a read shows it.
    pub fn effect_read(&self) -> [u8; effect::LEN] {
        let mut e = self.effect;
        e[0] = mode_as_read(e[0]);
        if self.custom_color {
            e[1..4].copy_from_slice(&[0xFF; 3]);
            e[8] = 1;
        } else {
            e[1..4].copy_from_slice(&self.palette_color);
            e[8] = 0;
        }
        e
    }

    fn store_effect(&mut self, data: &[u8]) {
        if !effect::is_valid_mode(data[0]) {
            // An unknown mode is ignored: the old effect keeps running.
            return;
        }
        program(&mut self.effect, 0, data);
        self.custom_color = true;
    }

    /// The device information as a read shows it.
    pub fn info_read(&self) -> [u8; info::LEN] {
        let mut i = self.info;
        i[16] = self.work_mode;
        i[17] = if self.battery < 0xFF { self.battery } else { 0 };
        i[18] = self.charge;
        i[22] = self.tft_frames;
        i
    }

    fn frames_running(&self) -> bool {
        self.frame_at.is_some_and(|t| t.elapsed() < self.frame_hold)
    }

    fn resetting(&self) -> bool {
        self.resetting_until.is_some_and(|t| Instant::now() < t)
    }

    fn drops_writes(&self, n: usize) -> bool {
        let now = Instant::now();
        self.booting_until.is_some_and(|t| now < t)
            || self.resetting()
            || self.faults.drop_writes_from.is_some_and(|m| n >= m)
    }

    /// What the LED of `slot` shows now.
    fn live_color(&self, slot: u8) -> [u8; 3] {
        if self.frames_running() {
            if let Some(c) = self.frame_leds.get(usize::from(slot)).copied().flatten() {
                return c;
            }
        }
        let mode = self.effect[0];
        if mode == 0 {
            [0, 0, 0]
        } else if effect::PER_KEY_MODES.contains(&mode) {
            if self.custom_color {
                perkey::shown(&self.per_key, slot)
            } else {
                // The keyboard's own painted tables; one colour here.
                self.palette_color
            }
        } else if self.custom_color {
            [self.effect[1], self.effect[2], self.effect[3]]
        } else {
            self.palette_color
        }
    }

    fn receive(&mut self, channel: Channel, wire: &[u8]) -> Result<usize> {
        if let Err(b) = guard::guard(channel, wire) {
            panic!("the keyboard received a packet the guard refuses ({b}): {:02X?}", &wire[..wire.len().min(12)]);
        }
        match channel {
            Channel::Command => {
                if self.upload_open {
                    let quiet = self.last_screen.is_some_and(|t| t.elapsed() >= self.upload_quiet);
                    assert!(quiet, "command traffic during a screen upload");
                    self.upload_open = false;
                }
                self.writes += 1;
                if self.faults.fail_write == Some(self.writes) {
                    return Err(AaError::Transport(TransportError::Refused("simulated write failure")));
                }
                self.packets.push(wire.to_vec());
                self.requests += 1;
                let n = self.requests;
                let req = &wire[1..];
                if self.faults.reset_before == Some(n) {
                    self.command_replies.push_back(report(&[0x55, 0xFC, 0x05]));
                    self.reset_by_keys();
                }
                if self.faults.stray_before == Some(n) {
                    let mut stray = req.to_vec();
                    stray[0] = frame::REPLY;
                    stray[3] = stray[3].wrapping_add(56);
                    self.command_replies.push_back(stray);
                }
                let reply = self.handle(req, n);
                let muted = self.faults.mute_from.is_some_and(|m| n >= m);
                if self.faults.drop_reply != Some(n) && !muted {
                    self.command_replies.extend(reply);
                }
            }
            Channel::Display => {
                assert!(
                    self.display_replies.is_empty(),
                    "a screen packet was written before the previous answer was read"
                );
                self.screen_writes += 1;
                let n = self.screen_writes;
                let p = &wire[1..];
                let index = usize::from(u16::from_le_bytes([p[2], p[3]]));
                let total = usize::from(u16::from_le_bytes([p[4], p[5]]));
                self.upload_open = index + 1 < total;
                self.last_screen = Some(Instant::now());
                self.screen.push(p.to_vec());
                if let Some(held) = self.held_ack.take() {
                    self.display_replies.push_back(held);
                }
                let f = &self.faults;
                let ack = report(&screen::ACK);
                if f.mute_display_from.is_some_and(|m| n >= m) || f.drop_ack == Some(n) {
                    // No answer.
                } else if f.late_ack == Some(n) {
                    self.held_ack = Some(ack);
                } else if f.double_ack == Some(n) {
                    self.display_replies.extend([ack.clone(), ack]);
                } else {
                    self.display_replies.push_back(ack);
                }
            }
        }
        Ok(wire.len())
    }

    /// A key-combination reset (Fn+Space held): the keymap layers go back
    /// to their defaults; the per-key table, the effect and the macros stay
    /// (§F.4). The reset blink follows.
    fn reset_by_keys(&mut self) {
        self.keymap = self.factory_keymap.clone();
        self.fn_keymap = self.default_fn.clone();
        if let Some(blink) = self.faults.reset_blink {
            self.resetting_until = Some(Instant::now() + blink);
        }
    }

    /// Answer one FF68 request (the `n`-th): the request itself, `55` first,
    /// with a read's data filled in.
    fn handle(&mut self, req: &[u8], n: usize) -> Option<Vec<u8>> {
        let (c, len, addr) = (req[1], usize::from(req[2]), usize::from(u16::from_le_bytes([req[3], req[4]])));
        let data = req[frame::HEADER_LEN..frame::HEADER_LEN + len].to_vec();
        let mut reply = req.to_vec();
        reply[0] = frame::REPLY;
        let region = |buf: &[u8]| -> Vec<u8> { (addr..addr + len).map(|i| buf.get(i).copied().unwrap_or(0)).collect() };
        let read = match c {
            cmd::DEVICE_INFO if self.faults.silent_info => return None,
            cmd::DEVICE_INFO => Some(region(&self.info_read())),
            cmd::OPTIONS => Some(region(&self.options)),
            cmd::KEYMAP => Some(region(&self.keymap)),
            cmd::FN_KEYMAP => Some(region(&self.fn_keymap)),
            cmd::DEFAULT_FN => Some(region(&self.default_fn)),
            cmd::EFFECT => Some(region(&self.effect_read())),
            cmd::PER_KEY => Some(region(&self.per_key)),
            cmd::MACROS => Some(region(&self.macros)),
            cmd::LIVE_COLORS => Some(
                data.chunks(4)
                    .flat_map(|e| {
                        let [r, g, b] = self.live_color(e[0]);
                        [e[0], r, g, b]
                    })
                    .collect(),
            ),
            _ => None,
        };
        if let Some(mut d) = read {
            if self.faults.corrupt == Some(c) {
                d[0] ^= 0xFF;
            }
            reply[frame::HEADER_LEN..frame::HEADER_LEN + len].copy_from_slice(&d);
            return Some(reply);
        }
        // A write: done unless dropped, and echoed either way. The firmware
        // programs whole words (the guard keeps LEN a multiple of 4).
        let words = &req[frame::HEADER_LEN..frame::HEADER_LEN + len.div_ceil(4) * 4];
        if c == cmd::REALTIME {
            self.frame(addr, words);
        } else if c == cmd::CLOCK {
            // Its own handler, not the flash writes': kept in RAM.
            self.clock = Some(data.clone());
        } else {
            let resend = self.check_sequence(c, addr, len);
            if self.drops_writes(n) || self.faults.ignore_writes == Some(c) {
                self.dropped.push(c);
            } else {
                self.store(c, addr, words, resend);
            }
        }
        if self.faults.garble_echo {
            reply[frame::HEADER_LEN] ^= 1;
        }
        Some(reply)
    }

    /// The driver's side of the flash rule: a table is written from address
    /// 0 upwards, each chunk where the last one ended (or the last one
    /// again, when its echo was lost), never patched in the middle. True for
    /// the last chunk again.
    fn check_sequence(&mut self, c: u8, addr: usize, len: usize) -> bool {
        let resend = self.writing.is_some_and(|(wc, start, _)| wc == c && addr == start);
        let ok = addr == 0 || resend || self.writing.is_some_and(|(wc, _, end)| wc == c && addr == end);
        assert!(
            ok,
            "write 0x{c:02X} at {addr} does not continue a table write from address 0 \
             (its page would not be erased)"
        );
        self.writing = Some((c, addr, addr + len));
        resend
    }

    fn store(&mut self, c: u8, addr: usize, words: &[u8], resend: bool) {
        match c {
            cmd::SET_EFFECT => self.store_effect(words),
            cmd::SET_PER_KEY => program(&mut self.per_key, addr, words),
            cmd::SET_MACROS => program(&mut self.macros, addr, words),
            cmd::SET_KEYMAP | cmd::SET_FN_KEYMAP => {
                let base = c == cmd::SET_KEYMAP;
                if addr == 0 && !resend {
                    let now = if base { self.keymap.clone() } else { self.fn_keymap.clone() };
                    self.pending_keymap = Some((now.clone(), now));
                }
                let Some((before, asked)) = self.pending_keymap.as_mut() else {
                    panic!("keymap chunk without its start")
                };
                asked[addr..addr + words.len()].copy_from_slice(words);
                if addr + words.len() == keymap::LEN {
                    let factory = if base { &self.factory_keymap } else { &self.default_fn };
                    if let Err(b) = guard::guard_keymap(before, asked, Some(factory)) {
                        panic!("the keyboard received a keymap the table guard refuses: {b}");
                    }
                }
                // An all-zero entry becomes the layer's default.
                let defaults = if base { &self.factory_keymap } else { &self.default_fn };
                let mut fixed = words.to_vec();
                for (i, e) in fixed.chunks_exact_mut(4).enumerate() {
                    if e == [0; 4] {
                        e.copy_from_slice(&defaults[addr + 4 * i..addr + 4 * i + 4]);
                    }
                }
                let table = if base { &mut self.keymap } else { &mut self.fn_keymap };
                program(table, addr, &fixed);
            }
            other => panic!("write command 0x{other:02X} has no handler"),
        }
    }

    /// A real-time chunk: each `[slot R G B]` entry lights its slot, in any
    /// effect mode, unless a reset runs. The frames list keeps the chunks
    /// as the driver cut them (a frame starts at address 0).
    fn frame(&mut self, addr: usize, data: &[u8]) {
        let visible = !self.resetting();
        if visible {
            for e in data.chunks_exact(4) {
                if let Some(led) = self.frame_leds.get_mut(usize::from(e[0])) {
                    *led = Some([e[1], e[2], e[3]]);
                }
            }
            self.frame_at = Some(Instant::now());
        }
        // A frame starts at address 0, unless that chunk is sent again.
        let resend = self.writing.is_some_and(|(c, start, _)| c == cmd::REALTIME && start == addr);
        self.writing = Some((cmd::REALTIME, addr, addr + data.len()));
        if (addr == 0 && !resend) || self.frames.is_empty() {
            self.frames.push((Vec::new(), visible));
        }
        let (f, shown) = self.frames.last_mut().expect("a frame");
        if f.len() < addr + data.len() {
            f.resize(addr + data.len(), 0);
        }
        f[addr..addr + data.len()].copy_from_slice(data);
        *shown &= visible;
    }
}

/// A 64-byte input report starting with `head`.
fn report(head: &[u8]) -> Vec<u8> {
    let mut r = vec![0u8; frame::REPORT_LEN];
    r[..head.len()].copy_from_slice(head);
    r
}

/// A simulated keyboard; clones share it.
#[derive(Debug, Clone)]
pub struct Sim(Arc<Mutex<SimState>>);

impl Sim {
    pub fn new(model: &Model) -> Self {
        Self(Arc::new(Mutex::new(SimState::factory(model))))
    }

    /// The AK980 PRO from the model catalogue.
    pub fn ak980() -> Self {
        Self::new(Self::model())
    }

    /// The AK980 PRO's model file.
    pub fn model() -> &'static Model {
        kb_devices::catalogue()
            .expect("catalogue")
            .iter()
            .find(|m| m.id == "ajazz-ak980-pro")
            .expect("the AK980 PRO model")
    }

    /// The USB ids firmware 769 reports.
    pub fn usb() -> UsbIdentity {
        UsbIdentity { pid: 0x8039, release: 0x0301 }
    }

    pub fn with_faults(self, faults: Faults) -> Self {
        self.state().faults = faults;
        self
    }

    pub fn state(&self) -> MutexGuard<'_, SimState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The FF68 command collection.
    pub fn command(&self) -> SimPort {
        SimPort { sim: self.clone(), channel: Channel::Command }
    }

    /// The FF67 screen collection.
    pub fn display(&self) -> SimPort {
        SimPort { sim: self.clone(), channel: Channel::Display }
    }
}

/// One collection of a [`Sim`].
#[derive(Debug)]
pub struct SimPort {
    sim: Sim,
    channel: Channel,
}

impl OutputPort for SimPort {
    fn write(&mut self, report: &[u8]) -> Result<usize> {
        self.sim.state().receive(self.channel, report)
    }

    fn read(&mut self, _timeout_ms: u32) -> Result<Option<Vec<u8>>> {
        let mut s = self.sim.state();
        Ok(match self.channel {
            Channel::Command => s.command_replies.pop_front(),
            Channel::Display => s.display_replies.pop_front(),
        })
    }
}
