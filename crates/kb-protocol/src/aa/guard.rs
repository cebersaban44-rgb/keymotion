//! The last line of defence before bytes reach an "AA" keyboard.
//!
//! [`guard`] checks every packet, whatever the caller meant to send: the
//! report length of its channel, the `AA` request marker, a command on the
//! channel's allow-list, sane header and data bytes, and never one of the
//! byte sequences that switch the MCU into its bootloader. Every write
//! stays inside its table ([`write_limit`]) and programs whole 4-byte
//! words: firmware 769 checks neither, rounds LEN up to 4 and erases its
//! own code when a write runs past the macro memory or the Fn layer
//! (protocol notes §F.2). Keymap tables are also checked as a whole
//! against the fresh read they were built from ([`guard_keymap`]): a FUNC
//! entry (a firmware function such as "factory reset") is never
//! introduced, only kept where it was. Per-key tables always carry every
//! entry's own index ([`guard_per_key`]).
//!
//! Allow-list (protocol notes Ek B, tightened): FF68 reads `10 11 12 13 14
//! 15 16 1C 33`, FF68 writes `22 23 24 25 26 34 32`, FF67 only `50`. Not
//! `21` (the options are read-only here), never `0F` (factory reset),
//! `40–42`, `4F`, `60–6A`, `80–86` or anything else.

use super::{clock, cmd, effect, frame, keymap, macros, perkey, screen};

/// Which of the keyboard's two vendor collections a packet goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// FF68: 64-byte requests and replies.
    Command,
    /// FF67: 4104-byte screen packets.
    Display,
}

impl Channel {
    /// The only packet length (report ID included) the channel takes.
    pub fn wire_len(self) -> usize {
        match self {
            Self::Command => frame::WIRE_LEN,
            Self::Display => screen::WIRE_LEN,
        }
    }
}

/// FF68 commands that only read.
pub const READS: [u8; 9] = [
    cmd::DEVICE_INFO,
    cmd::OPTIONS,
    cmd::KEYMAP,
    cmd::EFFECT,
    cmd::PER_KEY,
    cmd::MACROS,
    cmd::FN_KEYMAP,
    cmd::DEFAULT_FN,
    cmd::LIVE_COLORS,
];
/// FF68 commands that write.
pub const WRITES: [u8; 7] = [
    cmd::SET_KEYMAP,
    cmd::SET_EFFECT,
    cmd::SET_PER_KEY,
    cmd::SET_MACROS,
    cmd::SET_FN_KEYMAP,
    cmd::CLOCK,
    cmd::REALTIME,
];
/// FF67 commands.
pub const DISPLAY: [u8; 1] = [cmd::SCREEN];

/// Bytes a write command may reach (ADDR + LEN at most this), or `None` for
/// a command that writes nothing. The keymap and per-key tables are 512
/// bytes, the effect 16, the macro memory three pages, the clock block 24;
/// a real-time frame names at most the 128 slots.
pub fn write_limit(c: u8) -> Option<usize> {
    match c {
        cmd::SET_KEYMAP | cmd::SET_FN_KEYMAP => Some(keymap::LEN),
        cmd::SET_PER_KEY | cmd::REALTIME => Some(perkey::LEN),
        cmd::SET_EFFECT => Some(effect::LEN),
        cmd::SET_MACROS => Some(macros::AREA_LEN),
        cmd::CLOCK => Some(clock::LEN),
        _ => None,
    }
}

/// Byte sequences that put a SONiX MCU into its bootloader (ISP mode):
/// the 769 updater's "CheckDeviceCmd" (its constant sits in the 769
/// firmware) and the generic SONiX one. Never sent, at any offset.
pub const FORBIDDEN: [[u8; 8]; 2] =
    [[0xAA, 0x42, 0x89, 0x5A, 0xFF, 0x71, 0x62, 0xCC], [0xAA, 0x55, 0xA5, 0x5A, 0xFF, 0x00, 0x33, 0xCC]];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Blocked {
    #[error("packet is {len} bytes; this channel takes exactly {expected}")]
    Length { len: usize, expected: usize },
    #[error("report ID 0x{0:02X}; the keyboard uses none (00)")]
    ReportId(u8),
    #[error("a bootloader (ISP) byte sequence at offset {offset}")]
    Forbidden { offset: usize },
    #[error("first byte 0x{0:02X} is not the AA request marker")]
    NotARequest(u8),
    #[error("command 0x{cmd:02X} is not allowed on {channel:?}")]
    Command { cmd: u8, channel: Channel },
    #[error("malformed header: {0}")]
    Header(&'static str),
    #[error("read command 0x{0:02X} carries data")]
    ReadData(u8),
    #[error("effect out of range: {0}")]
    Effect(&'static str),
    #[error("screen packet: {0}")]
    Screen(&'static str),
    #[error("keymap slot {slot}: page 0x{page:02X} entries are never written (only kept where they are)")]
    NewEntry { slot: u8, page: u8 },
    #[error("keymap slot {slot}: firmware function {id} (reset, test or mode switch) is never written")]
    DangerousFunc { slot: u8, id: u32 },
    #[error("keymap slot {slot}: its factory function is firmware function {id}, which is never brought back")]
    FactoryFunc { slot: u8, id: u32 },
    #[error("slot {0} (end of the table) must stay as read")]
    Tail(u8),
    #[error("per-key slot {0} must start with its own index")]
    PerKeyIndex(u8),
    #[error("write 0x{cmd:02X} reaches byte {end}; its table ends at {limit}")]
    PastTable { cmd: u8, end: usize, limit: usize },
    #[error("table is {0} bytes; expected 512")]
    TableLength(usize),
}

/// Offset of the first forbidden sequence in `bytes`.
pub fn find_forbidden(bytes: &[u8]) -> Option<usize> {
    bytes.windows(8).position(|w| FORBIDDEN.iter().any(|f| f == w))
}

/// Check one packet exactly as it will be written (report ID included).
pub fn guard(channel: Channel, wire: &[u8]) -> Result<(), Blocked> {
    let expected = channel.wire_len();
    if wire.len() != expected {
        return Err(Blocked::Length { len: wire.len(), expected });
    }
    if wire[0] != 0 {
        return Err(Blocked::ReportId(wire[0]));
    }
    if let Some(offset) = find_forbidden(wire) {
        return Err(Blocked::Forbidden { offset });
    }
    if wire[1] != frame::REQUEST {
        return Err(Blocked::NotARequest(wire[1]));
    }
    let c = wire[2];
    let allowed = match channel {
        Channel::Command => READS.contains(&c) || WRITES.contains(&c),
        Channel::Display => DISPLAY.contains(&c),
    };
    if !allowed {
        return Err(Blocked::Command { cmd: c, channel });
    }
    match channel {
        Channel::Command => command_packet(&wire[1..]),
        Channel::Display => screen_packet(&wire[1..]),
    }
}

fn command_packet(r: &[u8]) -> Result<(), Blocked> {
    let (c, len, addr, r5, last) = (r[1], r[2] as usize, u16::from_le_bytes([r[3], r[4]]), r[5], r[6]);
    if !(1..=frame::CHUNK).contains(&len) {
        return Err(Blocked::Header("LEN must be 1–56"));
    }
    if r5 != 0 || r[7] != 0 {
        return Err(Blocked::Header("bytes 5 and 7 must be 0"));
    }
    if last > 1 {
        return Err(Blocked::Header("LAST must be 0 or 1"));
    }
    let data = &r[frame::HEADER_LEN..];
    if data[len..].iter().any(|&b| b != 0) {
        return Err(Blocked::Header("bytes after LEN must be 0"));
    }
    if let Some(limit) = write_limit(c) {
        if len % 4 != 0 {
            return Err(Blocked::Header("a write's LEN must be a multiple of 4"));
        }
        let end = usize::from(addr) + len;
        if end > limit {
            return Err(Blocked::PastTable { cmd: c, end, limit });
        }
    }
    match c {
        cmd::LIVE_COLORS => {
            // The request names each entry: `[i 00 00 00]`.
            if data.chunks(4).any(|e| e[1..].iter().any(|&b| b != 0)) {
                return Err(Blocked::ReadData(c));
            }
        }
        _ if READS.contains(&c) => {
            if data.iter().any(|&b| b != 0) {
                return Err(Blocked::ReadData(c));
            }
        }
        cmd::SET_EFFECT => {
            if len != effect::LEN || addr != 0 || last != 1 {
                return Err(Blocked::Effect("one 16-byte chunk at address 0"));
            }
            if !effect::is_valid_mode(data[0]) {
                return Err(Blocked::Effect("unknown mode"));
            }
            if data[4] != effect::DRIVER_SETTING || data[8] > 1 {
                return Err(Blocked::Effect("bytes 4 and 8"));
            }
            let level = effect::MIN_LEVEL..=effect::MAX_LEVEL;
            if !level.contains(&data[9]) || !level.contains(&data[10]) {
                return Err(Blocked::Effect("brightness and speed must be 1–5"));
            }
            if data[11] > effect::MAX_DIRECTION {
                return Err(Blocked::Effect("direction must be 0–3"));
            }
        }
        cmd::CLOCK if addr != 0 || data[..3] != super::clock::MARKER => {
            return Err(Blocked::Header("clock data starts 5A 01 5A at address 0"));
        }
        _ => {}
    }
    Ok(())
}

fn screen_packet(p: &[u8]) -> Result<(), Blocked> {
    let index = u16::from_le_bytes([p[2], p[3]]) as usize;
    let total = u16::from_le_bytes([p[4], p[5]]) as usize;
    if p[6..8] != screen::MARKER {
        return Err(Blocked::Screen("header bytes 6–7 must be 50 06"));
    }
    if total == 0 || total > screen::MAX_PACKETS || index >= total {
        return Err(Blocked::Screen("packet index or count out of range"));
    }
    if index == 0 {
        let frames = p[screen::HEADER_LEN] as usize;
        if !(1..=screen::MAX_FRAMES).contains(&frames) {
            return Err(Blocked::Screen("frame count must be 1–140"));
        }
    }
    Ok(())
}

/// A keymap write may change a slot only to an entry this crate encodes
/// itself ([`keymap::decode`] gives an action): the factory function
/// `00 00 00 00`, "disabled", and well-formed keyboard, consumer, mouse
/// and macro entries. FUNC (0x0D), FUNC_V2 (≥ 0x80), host-program (5) and
/// combo (7) entries, any other page, and parameters the encoder would
/// refuse are only ever kept where the fresh read had them. Slots 126–127
/// (where some boards keep an `AA 55` tail) never change.
///
/// `factory` is the layer's factory table when known (`1C` for the Fn
/// layer). A slot set to `00 00 00 00` gets its factory function back, so
/// that is refused where the factory entry is a firmware function.
pub fn guard_keymap(fresh: &[u8], new: &[u8], factory: Option<&[u8]>) -> Result<(), Blocked> {
    tables(fresh, new)?;
    if let Some(f) = factory {
        tables(f, new)?;
    }
    for slot in 0..keymap::SLOTS {
        let at = slot * 4;
        let (f, n) = (&fresh[at..at + 4], &new[at..at + 4]);
        if f == n {
            continue;
        }
        let slot = slot as u8;
        if keymap::TAIL_SLOTS.contains(&slot) {
            return Err(Blocked::Tail(slot));
        }
        let entry = [n[0], n[1], n[2], n[3]];
        if let Some(id) = keymap::func_ids(entry).into_iter().find(|id| keymap::DANGEROUS_FUNC.contains(id)) {
            return Err(Blocked::DangerousFunc { slot, id });
        }
        if !matches!(keymap::decode(entry), keymap::Binding::Action(_)) {
            return Err(Blocked::NewEntry { slot, page: n[0] });
        }
        if entry == [0; 4] {
            if let Some(back) = factory.map(|f| keymap::entry(f, slot)).filter(|&e| keymap::is_func(e)) {
                let id = keymap::func_ids(back).first().copied().unwrap_or(0);
                return Err(Blocked::FactoryFunc { slot, id });
            }
        }
    }
    Ok(())
}

/// A per-key table is written whole, every one of its 128 entries starting
/// with its own index (the slots without a key black): whatever the
/// keyboard held before, all `00` and all `FF` included.
pub fn guard_per_key(table: &[u8]) -> Result<(), Blocked> {
    if table.len() != perkey::LEN {
        return Err(Blocked::TableLength(table.len()));
    }
    match (0..perkey::SLOTS).find(|&i| table[i * 4] != i as u8) {
        Some(slot) => Err(Blocked::PerKeyIndex(slot as u8)),
        None => Ok(()),
    }
}

fn tables(fresh: &[u8], new: &[u8]) -> Result<(), Blocked> {
    for t in [fresh, new] {
        if t.len() != keymap::LEN {
            return Err(Blocked::TableLength(t.len()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aa::frame::{chunks, request, wire};

    fn packet(c: u8, len: usize, data: &[u8]) -> [u8; 65] {
        wire(&request(c, &chunks(len, 0, true)[0], 0, data))
    }

    fn effect_data() -> [u8; 16] {
        [1, 0xFF, 0, 0, 0xFF, 0, 0, 0, 0, 5, 3, 0, 0, 0, 0xAA, 0x55]
    }

    #[test]
    fn allowed_packets_pass() {
        for c in READS.into_iter().filter(|&c| c != cmd::LIVE_COLORS) {
            assert_eq!(guard(Channel::Command, &packet(c, 56, &[])), Ok(()), "{c:02X}");
        }
        assert_eq!(guard(Channel::Command, &packet(cmd::LIVE_COLORS, 56, &[4, 0, 0, 0, 5, 0, 0, 0])), Ok(()));
        assert_eq!(guard(Channel::Command, &packet(cmd::SET_EFFECT, 16, &effect_data())), Ok(()));
        assert_eq!(guard(Channel::Command, &packet(cmd::SET_KEYMAP, 56, &[2, 0, 4, 0])), Ok(()));
        assert_eq!(guard(Channel::Command, &packet(cmd::CLOCK, 24, &[0x5A, 1, 0x5A, 26])), Ok(()));
    }

    /// A write of `len` bytes of command `c` at `addr`.
    fn write_at(c: u8, addr: u16, len: usize) -> [u8; 65] {
        let mut data = vec![0u8; len];
        match c {
            cmd::SET_EFFECT => data.copy_from_slice(&effect_data()[..len]),
            cmd::CLOCK => data[..3].copy_from_slice(&clock::MARKER),
            _ => {}
        }
        let ch = frame::Chunk { offset: 0, addr, len: len as u8, last: true };
        wire(&request(c, &ch, 0, &data))
    }

    #[test]
    fn every_write_stays_inside_its_table() {
        // A write past the macro memory, the Fn layer or the per-key table
        // would erase firmware code on 769: each write command is held to
        // its own table, up to its last byte and not one word further.
        for (c, limit) in [
            (cmd::SET_KEYMAP, 512usize),
            (cmd::SET_FN_KEYMAP, 512),
            (cmd::SET_PER_KEY, 512),
            (cmd::SET_MACROS, 0x0C00),
            (cmd::REALTIME, 512),
        ] {
            assert_eq!(write_limit(c), Some(limit), "{c:02X}");
            let last = (limit - 56) as u16;
            assert_eq!(guard(Channel::Command, &write_at(c, last, 56)), Ok(()), "{c:02X} last chunk");
            assert_eq!(
                guard(Channel::Command, &write_at(c, last + 4, 56)),
                Err(Blocked::PastTable { cmd: c, end: limit + 4, limit }),
                "{c:02X} one word past"
            );
            assert!(guard(Channel::Command, &write_at(c, 0xFFF0, 16)).is_err(), "{c:02X} far past");
            // LEN must cover whole words (the firmware rounds it up to 4).
            for len in [1usize, 2, 3, 6, 55] {
                assert_eq!(
                    guard(Channel::Command, &write_at(c, 0, len)),
                    Err(Blocked::Header("a write's LEN must be a multiple of 4")),
                    "{c:02X} LEN {len}"
                );
            }
        }
        // The effect: one 16-byte chunk at 0. The clock: 24 bytes at 0.
        assert_eq!(write_limit(cmd::SET_EFFECT), Some(16));
        assert_eq!(guard(Channel::Command, &write_at(cmd::SET_EFFECT, 0, 16)), Ok(()));
        assert!(guard(Channel::Command, &write_at(cmd::SET_EFFECT, 4, 16)).is_err());
        assert!(guard(Channel::Command, &write_at(cmd::SET_EFFECT, 0, 12)).is_err());
        assert_eq!(write_limit(cmd::CLOCK), Some(24));
        assert_eq!(guard(Channel::Command, &write_at(cmd::CLOCK, 0, 24)), Ok(()));
        assert!(guard(Channel::Command, &write_at(cmd::CLOCK, 0, 28)).is_err());
        assert!(guard(Channel::Command, &write_at(cmd::CLOCK, 0, 22)).is_err());
        // Reads write nothing: no limit (their LEN is still 1–56).
        for c in READS {
            assert_eq!(write_limit(c), None, "{c:02X}");
        }
        assert_eq!(guard(Channel::Command, &packet(cmd::DEVICE_INFO, 48, &[])), Ok(()));
        // Every write command has a limit.
        for c in WRITES {
            assert!(write_limit(c).is_some(), "{c:02X}");
        }
    }

    #[test]
    fn every_other_command_is_blocked() {
        for c in 0..=255u8 {
            if READS.contains(&c) || WRITES.contains(&c) {
                continue;
            }
            assert_eq!(
                guard(Channel::Command, &packet(c, 8, &[])),
                Err(Blocked::Command { cmd: c, channel: Channel::Command }),
                "{c:02X}"
            );
        }
        // Explicitly: options write, factory reset, OTA, flash, LED
        // animations, calibration, profiles, session start/end.
        for c in [0x21u8, 0x0F, 0x82, 0x86, 0x4F, 0x40, 0x41, 0x42, 0x64, 0x6A, 0x90, 0x01, 0x02, 0x31, 0x1F] {
            assert!(guard(Channel::Command, &packet(c, 8, &[])).is_err(), "{c:02X}");
        }
    }

    #[test]
    fn the_screen_channel_takes_only_screen_packets() {
        let mut p = vec![0u8; screen::WIRE_LEN];
        p[1..9].copy_from_slice(&[0xAA, 0x50, 0, 0, 16, 0, 0x50, 0x06]);
        p[9] = 1;
        assert_eq!(guard(Channel::Display, &p), Ok(()));
        // An FF68 command on FF67 (it would be "acked" there too).
        for c in READS.into_iter().chain(WRITES) {
            let mut q = p.clone();
            q[2] = c;
            assert_eq!(guard(Channel::Display, &q), Err(Blocked::Command { cmd: c, channel: Channel::Display }));
        }
        // A screen packet on FF68, or a short one on FF67.
        assert!(matches!(guard(Channel::Command, &p), Err(Blocked::Length { .. })));
        assert!(matches!(guard(Channel::Display, &packet(cmd::SCREEN, 8, &[])), Err(Blocked::Length { .. })));
        // Header: 50 06, index below total, total ≤ 2215, 1–140 frames.
        let mut bad = p.clone();
        bad[8] = 0x07;
        assert!(matches!(guard(Channel::Display, &bad), Err(Blocked::Screen(_))));
        let mut bad = p.clone();
        bad[3] = 16;
        assert!(matches!(guard(Channel::Display, &bad), Err(Blocked::Screen(_))), "index = total");
        let mut bad = p.clone();
        bad[5..7].copy_from_slice(&2216u16.to_le_bytes());
        assert!(matches!(guard(Channel::Display, &bad), Err(Blocked::Screen(_))), "too many packets");
        let mut bad = p.clone();
        bad[9] = 141;
        assert!(matches!(guard(Channel::Display, &bad), Err(Blocked::Screen(_))), "141 frames");
        bad[9] = 0;
        assert!(matches!(guard(Channel::Display, &bad), Err(Blocked::Screen(_))), "no frame");
    }

    #[test]
    fn forbidden_sequences_are_blocked_at_any_offset() {
        for seq in FORBIDDEN {
            // Inside the data of an allowed write, at every offset.
            for at in 0..=(56 - 8) {
                let mut data = [0u8; 56];
                data[0] = 2;
                data[at..at + 8].copy_from_slice(&seq);
                let p = packet(cmd::SET_KEYMAP, 56, &data);
                assert_eq!(guard(Channel::Command, &p), Err(Blocked::Forbidden { offset: 9 + at }), "{at}");
            }
            // As the packet itself, and inside a screen packet.
            let mut p = [0u8; 65];
            p[1..9].copy_from_slice(&seq);
            assert_eq!(guard(Channel::Command, &p), Err(Blocked::Forbidden { offset: 1 }));
            let mut s = vec![0u8; screen::WIRE_LEN];
            s[1..9].copy_from_slice(&[0xAA, 0x50, 0, 0, 16, 0, 0x50, 0x06]);
            s[9] = 1;
            s[4000..4008].copy_from_slice(&seq);
            assert_eq!(guard(Channel::Display, &s), Err(Blocked::Forbidden { offset: 4000 }));
        }
        assert_eq!(find_forbidden(&[0xAA, 0x42, 0x89]), None);
    }

    #[test]
    fn wrong_lengths_and_markers_are_blocked() {
        assert_eq!(guard(Channel::Command, &[0u8; 64]), Err(Blocked::Length { len: 64, expected: 65 }));
        assert_eq!(guard(Channel::Command, &[0u8; 66]), Err(Blocked::Length { len: 66, expected: 65 }));
        let mut p = packet(cmd::KEYMAP, 56, &[]);
        p[0] = 1;
        assert_eq!(guard(Channel::Command, &p), Err(Blocked::ReportId(1)));
        let mut p = packet(cmd::KEYMAP, 56, &[]);
        p[1] = 0x04;
        assert_eq!(guard(Channel::Command, &p), Err(Blocked::NotARequest(0x04)), "an FF13 command");
        let mut p = packet(cmd::KEYMAP, 56, &[]);
        p[3] = 57;
        assert!(matches!(guard(Channel::Command, &p), Err(Blocked::Header(_))));
        let mut p = packet(cmd::KEYMAP, 56, &[]);
        p[6] = 1;
        assert!(matches!(guard(Channel::Command, &p), Err(Blocked::Header(_))), "amplitude");
        // Data after LEN, and data in a read.
        let p = packet(cmd::SET_KEYMAP, 4, &[2, 0, 4, 0, 9]);
        assert_eq!(p[13], 0, "request() copies only LEN bytes");
        let mut p = packet(cmd::SET_KEYMAP, 4, &[2, 0, 4, 0]);
        p[20] = 1;
        assert!(matches!(guard(Channel::Command, &p), Err(Blocked::Header(_))));
        assert_eq!(guard(Channel::Command, &packet(cmd::KEYMAP, 56, &[1])), Err(Blocked::ReadData(cmd::KEYMAP)));
        assert_eq!(
            guard(Channel::Command, &packet(cmd::LIVE_COLORS, 56, &[4, 1, 0, 0])),
            Err(Blocked::ReadData(cmd::LIVE_COLORS))
        );
    }

    #[test]
    fn effects_outside_the_aa_ranges_are_blocked() {
        let ok = effect_data();
        for (i, v) in [(0usize, 22u8), (0, 127), (4, 0), (8, 2), (9, 0), (9, 6), (10, 0), (10, 6), (11, 4)] {
            let mut d = ok;
            d[i] = v;
            assert!(matches!(guard(Channel::Command, &packet(cmd::SET_EFFECT, 16, &d)), Err(Blocked::Effect(_))));
        }
        let mut d = ok;
        d[0] = 128;
        assert_eq!(guard(Channel::Command, &packet(cmd::SET_EFFECT, 16, &d)), Ok(()));
        // The effect is one chunk of 16 bytes.
        assert!(guard(Channel::Command, &packet(cmd::SET_EFFECT, 12, &ok[..12])).is_err());
    }

    fn table() -> [u8; 512] {
        let mut t = [0u8; 512];
        t[83 * 4..83 * 4 + 4].copy_from_slice(&[0x0D, 0, 0, 1]); // FUNC 1 (factory reset) on Space
        t[34 * 4..34 * 4 + 4].copy_from_slice(&[0x0D, 0, 0, 7]); // FUNC 7
        t[510..].copy_from_slice(&[0xAA, 0x55]);
        t
    }

    #[test]
    fn keymap_tables_keep_func_entries_where_they_were() {
        let fresh = table();
        // Changing ordinary keys, keeping the FUNC entries: fine.
        let mut new = fresh;
        new[0..4].copy_from_slice(&[2, 0, 4, 0]);
        new[4..8].copy_from_slice(&[3, 0xE9, 0, 0]);
        new[8..12].copy_from_slice(&[6, 1, 1, 3]);
        new[12..16].copy_from_slice(&[1, 1, 2, 0]);
        new[34 * 4..34 * 4 + 4].copy_from_slice(&[0, 0, 0, 0]); // a FUNC slot back to default
        assert_eq!(guard_keymap(&fresh, &new, None), Ok(()));
        // A FUNC introduced, moved or changed.
        let mut new = fresh;
        new[0..4].copy_from_slice(&[0x0D, 0, 0, 7]);
        assert_eq!(guard_keymap(&fresh, &new, None), Err(Blocked::NewEntry { slot: 0, page: 0x0D }));
        let mut new = fresh;
        new[0..4].copy_from_slice(&[0x0D, 0, 0, 1]);
        assert_eq!(guard_keymap(&fresh, &new, None), Err(Blocked::DangerousFunc { slot: 0, id: 1 }));
        for id in keymap::DANGEROUS_FUNC {
            let mut new = fresh;
            let [_, a, b, c] = id.to_be_bytes();
            new[40..44].copy_from_slice(&[0x0D, a, b, c]);
            assert_eq!(guard_keymap(&fresh, &new, None), Err(Blocked::DangerousFunc { slot: 10, id }), "{id}");
        }
        // FUNC_V2 4096 (factory reset) and other FUNC_V2 entries.
        let mut new = fresh;
        new[40..44].copy_from_slice(&[0x90, 0x00, 0, 0]);
        assert_eq!(guard_keymap(&fresh, &new, None), Err(Blocked::DangerousFunc { slot: 10, id: 4096 }));
        let mut new = fresh;
        new[40..44].copy_from_slice(&[0x80, 0x00, 0x20, 0x00]);
        assert_eq!(guard_keymap(&fresh, &new, None), Err(Blocked::NewEntry { slot: 10, page: 0x80 }));
        // Host programs (page 5, including the "04 xx" disable code), combos.
        for page in [5u8, 7, 4, 8, 14] {
            let mut new = fresh;
            new[40..44].copy_from_slice(&[page, 3, 0, 0]);
            assert_eq!(guard_keymap(&fresh, &new, None), Err(Blocked::NewEntry { slot: 10, page }));
        }
        // Parameters the encoder refuses on pages it produces.
        for bad in [
            [6u8, 200, 0, 0],
            [6, 1, 7, 0],
            [6, 1, 0, 3],
            [1, 5, 0xFF, 0xFF],
            [1, 1, 3, 0],
            [2, 0, 4, 0x7F],
            [3, 0, 0, 0],
        ] {
            let mut new = fresh;
            new[40..44].copy_from_slice(&bad);
            assert_eq!(
                guard_keymap(&fresh, &new, None),
                Err(Blocked::NewEntry { slot: 10, page: bad[0] }),
                "{bad:02X?}"
            );
        }
        // A non-zero default entry, the tail slots, wrong lengths.
        let mut new = fresh;
        new[40..44].copy_from_slice(&[0, 1, 0, 0]);
        assert!(guard_keymap(&fresh, &new, None).is_err());
        let mut new = fresh;
        new[510] = 0;
        assert_eq!(guard_keymap(&fresh, &new, None), Err(Blocked::Tail(127)));
        assert_eq!(guard_keymap(&fresh[..508], &fresh, None), Err(Blocked::TableLength(508)));
    }

    #[test]
    fn the_factory_function_of_a_slot_is_not_brought_back() {
        // Fn layer: factory reset on Space, battery on Backspace; the user
        // has put ordinary keys there.
        let factory = table();
        let mut fresh = factory;
        fresh[83 * 4..83 * 4 + 4].copy_from_slice(&[2, 0, 0x2C, 0]);
        fresh[34 * 4..34 * 4 + 4].copy_from_slice(&[2, 0, 0x1A, 0]);
        fresh[0..4].copy_from_slice(&[2, 0, 0x29, 0]);
        for (slot, id) in [(83u8, 1u32), (34, 7)] {
            let mut new = fresh;
            keymap::set_entry(&mut new, slot, [0; 4]);
            assert_eq!(guard_keymap(&fresh, &new, Some(&factory)), Err(Blocked::FactoryFunc { slot, id }));
            assert_eq!(guard_keymap(&fresh, &new, None), Ok(()), "without the factory table it cannot know");
        }
        // A slot whose factory entry is an ordinary key may go back to it.
        let mut new = fresh;
        new[0..4].copy_from_slice(&[0; 4]);
        assert_eq!(guard_keymap(&fresh, &new, Some(&factory)), Ok(()));
        assert_eq!(guard_keymap(&fresh, &new, Some(&factory[..100])), Err(Blocked::TableLength(100)));
    }

    #[test]
    fn per_key_tables_carry_every_index() {
        let mut t = perkey::blank();
        t[76 * 4 + 1] = 0xFF;
        // Slots 126–127 are written like any other (no `AA 55` tail on 769).
        t[127 * 4 + 1] = 0x10;
        assert_eq!(guard_per_key(&t), Ok(()));
        t[76 * 4] = 75;
        assert_eq!(guard_per_key(&t), Err(Blocked::PerKeyIndex(76)));
        // What a flashed (all 00) or never programmed (all FF) keyboard
        // holds is never written back as it is: its entries 1–127 are dark.
        assert_eq!(guard_per_key(&[0u8; 512]), Err(Blocked::PerKeyIndex(1)));
        assert_eq!(guard_per_key(&[0xFFu8; 512]), Err(Blocked::PerKeyIndex(0)));
        let mut tail = perkey::blank();
        tail[510..].copy_from_slice(&[0xAA, 0x55]);
        assert_eq!(guard_per_key(&tail), Ok(()), "a colour, not a tail");
        assert_eq!(guard_per_key(&t[..508]), Err(Blocked::TableLength(508)));
    }
}
