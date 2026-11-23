//! A simulated AK980 PRO on firmware 769: both "AA" collections (FF68 and
//! FF67) over one shared state, for tests and for the app's development
//! without a keyboard (cargo feature `sim`).
//!
//! It answers like the protocol notes describe: device information from a
//! real 769 reply, reads from its tables, writes echoed and stored, the
//! effect colour reading back `FF FF FF` after a write, FF67 answering
//! everything with `55 41 00 01`, real-time frames shown only in effect
//! modes 20, 21 and 128. [`Faults`] make it misbehave.
//!
//! It also checks the driver: a packet the guard refuses, a keymap write
//! that introduces a firmware function, a macro table chunk with LAST set
//! or command traffic during a screen upload makes it panic, failing the
//! test.

use super::{AaError, OutputPort, Result, UsbIdentity};
use kb_devices::Model;
use kb_protocol::aa::guard::{self, Channel};
use kb_protocol::aa::{cmd, effect, frame, info, keymap, macros, perkey, screen};
use kb_transport::TransportError;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

/// What the simulated keyboard gets wrong, on purpose.
#[derive(Debug, Clone, Default)]
pub struct Faults {
    /// No reply to the n-th FF68 request (1-based).
    pub drop_reply: Option<usize>,
    /// `55 FC 05` (keyboard reset) before the reply to the n-th request.
    pub reset_before: Option<usize>,
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
    /// Writes of this command are echoed but not stored.
    pub ignore_writes: Option<u8>,
}

/// Everything the simulated keyboard holds and saw.
#[derive(Debug, Clone)]
pub struct SimState {
    pub info: [u8; info::LEN],
    pub options: [u8; 56],
    pub keymap: Vec<u8>,
    pub fn_keymap: Vec<u8>,
    pub default_fn: Vec<u8>,
    pub effect: [u8; effect::LEN],
    /// An effect was written since power-up: its colour reads back wrong.
    pub effect_written: bool,
    pub per_key: Vec<u8>,
    /// Macro memory (`macro_space_size` bytes).
    pub macros: Vec<u8>,
    /// The last clock block written.
    pub clock: Option<Vec<u8>>,
    /// Real-time frames received, and whether they showed.
    pub frames: Vec<(Vec<u8>, bool)>,
    /// FF67 packets received (report ID dropped).
    pub screen: Vec<Vec<u8>>,
    /// FF68 packets received, as written.
    pub packets: Vec<Vec<u8>>,
    pub faults: Faults,
    requests: usize,
    writes: usize,
    command_replies: VecDeque<Vec<u8>>,
    display_replies: VecDeque<Vec<u8>>,
    pending_table: Option<(u8, Vec<u8>)>,
    pending_frame: Vec<u8>,
    upload_open: bool,
}

fn put(table: &mut [u8], slot: u8, e: [u8; 4]) {
    keymap::set_entry(table, slot, e);
}

impl SimState {
    /// A plausible factory state for `model` (its keys need `aa`).
    pub fn factory(model: &Model) -> Self {
        let mut info = [0u8; info::LEN];
        info[..info::FIRMWARE_769_CABLE.len()].copy_from_slice(&info::FIRMWARE_769_CABLE);
        info[22..24].copy_from_slice(&140u16.to_le_bytes()); // tftMaxFrames
        let mut options = [0u8; 56];
        options[4] = 3; // key response level (WebHID default)

        let keys = model.geometry.as_ref().map(|g| g.keys.as_slice()).unwrap_or_default();
        let tail = |t: &mut Vec<u8>| t[510..].copy_from_slice(&[0xAA, 0x55]);
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
        tail(&mut base);

        // Fn layer: media keys on F1–F12, firmware functions on a few keys,
        // factory reset (FUNC 1) on Space as on the MK87.
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
        tail(&mut fn_layer);

        let mut per_key = perkey::live_request().to_vec();
        per_key[510..].copy_from_slice(&[0xAA, 0x55]);

        Self {
            info,
            options,
            keymap: base,
            default_fn: fn_layer.clone(),
            fn_keymap: fn_layer,
            effect: [1, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 1, 5, 3, 0, 0, 0, 0, 0],
            effect_written: false,
            per_key,
            macros: vec![0u8; usize::from(u16::from_le_bytes([info[2], info[3]]))],
            clock: None,
            frames: Vec::new(),
            screen: Vec::new(),
            packets: Vec::new(),
            faults: Faults::default(),
            requests: 0,
            writes: 0,
            command_replies: VecDeque::new(),
            display_replies: VecDeque::new(),
            pending_table: None,
            pending_frame: Vec::new(),
            upload_open: false,
        }
    }

    /// FF68 packets whose command is `c`.
    pub fn sent(&self, c: u8) -> Vec<&Vec<u8>> {
        self.packets.iter().filter(|p| p[2] == c).collect()
    }

    /// The effect as a read shows it.
    fn effect_read(&self) -> [u8; effect::LEN] {
        let mut e = self.effect;
        if self.effect_written {
            e[1..4].copy_from_slice(&[0xFF; 3]);
            e[8] = 1;
        }
        e
    }

    fn live_color(&self, slot: u8) -> [u8; 3] {
        if effect::LIVE_MODES.contains(&self.effect[0]) {
            if let Some((f, true)) = self.frames.last() {
                if let Some(e) = f.chunks_exact(4).find(|e| e[0] == slot) {
                    return [e[1], e[2], e[3]];
                }
            }
            perkey::get(&self.per_key, slot)
        } else if self.effect[0] == 0 {
            [0, 0, 0]
        } else {
            [self.effect[1], self.effect[2], self.effect[3]]
        }
    }

    fn receive(&mut self, channel: Channel, wire: &[u8]) -> Result<usize> {
        if let Err(b) = guard::guard(channel, wire) {
            panic!("the keyboard received a packet the guard refuses ({b}): {:02X?}", &wire[..wire.len().min(12)]);
        }
        match channel {
            Channel::Command => {
                assert!(!self.upload_open, "command traffic during a screen upload");
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
                }
                if self.faults.stray_before == Some(n) {
                    let mut stray = req.to_vec();
                    stray[0] = frame::REPLY;
                    stray[3] = stray[3].wrapping_add(56);
                    self.command_replies.push_back(stray);
                }
                let reply = self.handle(req);
                if self.faults.drop_reply != Some(n) {
                    self.command_replies.extend(reply);
                }
            }
            Channel::Display => {
                let p = &wire[1..];
                let index = usize::from(u16::from_le_bytes([p[2], p[3]]));
                let total = usize::from(u16::from_le_bytes([p[4], p[5]]));
                self.upload_open = index + 1 < total;
                self.screen.push(p.to_vec());
                self.display_replies.push_back(report(&screen::ACK));
            }
        }
        Ok(wire.len())
    }

    /// Answer one FF68 request.
    fn handle(&mut self, req: &[u8]) -> Option<Vec<u8>> {
        let (c, len, addr, last) =
            (req[1], usize::from(req[2]), usize::from(u16::from_le_bytes([req[3], req[4]])), req[6]);
        let data = req[frame::HEADER_LEN..frame::HEADER_LEN + len].to_vec();
        let mut reply = req.to_vec();
        reply[0] = frame::REPLY;
        let region = |buf: &[u8]| -> Vec<u8> { (addr..addr + len).map(|i| buf.get(i).copied().unwrap_or(0)).collect() };
        let read = match c {
            cmd::DEVICE_INFO if self.faults.silent_info => return None,
            cmd::DEVICE_INFO => Some(region(&self.info)),
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
        // A write: store it (unless told not to) and echo it.
        if self.faults.ignore_writes != Some(c) {
            self.store(c, addr, last == 1, &data);
        }
        if self.faults.garble_echo {
            reply[frame::HEADER_LEN] ^= 1;
        }
        Some(reply)
    }

    fn store(&mut self, c: u8, addr: usize, last: bool, data: &[u8]) {
        let write = |buf: &mut [u8]| buf[addr..addr + data.len()].copy_from_slice(data);
        match c {
            cmd::SET_EFFECT => {
                write(&mut self.effect);
                self.effect_written = true;
            }
            cmd::SET_PER_KEY => write(&mut self.per_key),
            cmd::SET_KEYMAP | cmd::SET_FN_KEYMAP => {
                let current = if c == cmd::SET_KEYMAP { &self.keymap } else { &self.fn_keymap };
                if addr == 0 {
                    self.pending_table = Some((c, current.clone()));
                }
                let Some((pc, table)) = self.pending_table.as_mut() else { panic!("keymap chunk without its start") };
                assert_eq!(*pc, c, "keymap chunks of two layers mixed");
                table[addr..addr + data.len()].copy_from_slice(data);
                if last {
                    let (_, table) = self.pending_table.take().expect("pending");
                    let current = if c == cmd::SET_KEYMAP { &mut self.keymap } else { &mut self.fn_keymap };
                    if let Err(b) = guard::guard_keymap(current, &table) {
                        panic!("the keyboard received a keymap the table guard refuses: {b}");
                    }
                    *current = table;
                }
            }
            cmd::SET_MACROS => {
                assert!(!(addr < macros::TABLE_LEN && last), "a macro table chunk carries LAST");
                write(&mut self.macros);
            }
            cmd::CLOCK => self.clock = Some(data.to_vec()),
            cmd::REALTIME => {
                if addr == 0 {
                    self.pending_frame.clear();
                }
                self.pending_frame.extend_from_slice(data);
                if last {
                    let visible = effect::LIVE_MODES.contains(&self.effect[0]);
                    self.frames.push((std::mem::take(&mut self.pending_frame), visible));
                }
            }
            other => panic!("write command 0x{other:02X} has no handler"),
        }
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
