//! SONiX "FF13" family: wired AJAZZ AK980 PRO (0C45:8009) and relatives.
//!
//! Wire format (docs/research/protokol-ff13.md §2):
//! * Commands travel as 65-byte HID feature reports: report ID `00`, then a
//!   64-byte payload. The payload's leading `04` is data, not a report ID.
//! * Header packet: `04 OP ARG 00 00 00 00 00 CNT_LO CNT_HI 00…`, where CNT
//!   is the number of raw 64-byte data packets that follow.
//! * The reply to a header (read with GET_FEATURE) echoes it with a status in
//!   payload byte 3: `01` accepted, `FF` refused/busy.

/// Payload bytes per command packet.
pub const PAYLOAD: usize = 64;
/// Feature report length on the wire (report ID + payload).
pub const REPORT_LEN: usize = PAYLOAD + 1;

/// First payload byte of every command header.
pub const PREFIX: u8 = 0x04;

/// Status byte values in a header echo.
pub const STATUS_OK: u8 = 0x01;
pub const STATUS_REFUSED: u8 = 0xFF;

/// LED slots addressable by the real-time stream (`[idx R G B]` × 128).
pub const REALTIME_SLOTS: usize = 128;
/// Data packets in one real-time frame (128 × 4 bytes / 64).
pub const REALTIME_PACKETS: u16 = 8;
/// Data packets returned by a colour read-back (`[idx R G B]` × 144).
pub const COLOR_TABLE_PACKETS: usize = 9;

/// Command opcodes this project is allowed to send.
///
/// Anything else, including opcodes the firmware accepts but the official
/// driver never sends, is rejected by [`guard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    /// Open a configuration session.
    Begin = 0x18,
    /// Close the data phase ("save"); also ends a real-time frame.
    Commit = 0x02,
    /// Apply lighting/keymap after commit.
    Apply = 0xF0,
    /// Lighting effect (flash write).
    Lighting = 0x13,
    /// Per-key colour table (flash write).
    PerKeyColors = 0x23,
    /// Read back the colours currently shown (read-only).
    ReadColors = 0xF5,
    /// Options: sleep, Win lock, … (flash write).
    Options = 0x17,
    /// Base-layer keymap (flash write).
    KeymapBase = 0x11,
    /// Fn-layer keymap (flash write).
    KeymapFn = 0x27,
    /// Open a macro session.
    MacroBegin = 0x19,
    /// Macro data (flash write).
    MacroData = 0x15,
    /// Set the keyboard clock.
    Clock = 0x28,
    /// Screen image upload announcement (flash write on the display channel).
    ScreenUpload = 0x72,
    /// Real-time LED frame (RAM only, no flash write).
    Realtime = 0x20,
}

impl Op {
    pub fn from_byte(b: u8) -> Option<Self> {
        use Op::*;
        Some(match b {
            0x18 => Begin,
            0x02 => Commit,
            0xF0 => Apply,
            0x13 => Lighting,
            0x23 => PerKeyColors,
            0xF5 => ReadColors,
            0x17 => Options,
            0x11 => KeymapBase,
            0x27 => KeymapFn,
            0x19 => MacroBegin,
            0x15 => MacroData,
            0x28 => Clock,
            0x72 => ScreenUpload,
            0x20 => Realtime,
            _ => return None,
        })
    }
}

/// Byte sequences that switch the MCU into firmware-update/ISP mode.
/// Never sent, in any position of the protocol.
const BOOTLOADER_MAGIC: &[&[u8]] =
    &[&[0xAA, 0x42, 0x89, 0x5A, 0xFF, 0x71, 0x62, 0xCC], &[0xAA, 0x55, 0xA5, 0x5A, 0xFF, 0x00, 0x33, 0xCC]];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Blocked {
    #[error("packet matches a bootloader/firmware-update sequence")]
    BootloaderMagic,
    #[error("SN32 ISP command pattern")]
    IspPattern,
    #[error("opcode 0x{0:02X} is not on the allowlist")]
    Opcode(u8),
    #[error("packet is {0} bytes; expected at most {PAYLOAD}")]
    TooLong(usize),
}

/// What kind of packet the caller believes it is sending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketKind {
    /// A `04 OP …` header.
    Header,
    /// Raw data announced by a preceding header.
    Data,
}

/// Last line of defence before bytes reach the device.
pub fn guard(packet: &[u8], kind: PacketKind) -> Result<(), Blocked> {
    if packet.len() > PAYLOAD {
        return Err(Blocked::TooLong(packet.len()));
    }
    if BOOTLOADER_MAGIC.iter().any(|m| packet.starts_with(m)) {
        return Err(Blocked::BootloaderMagic);
    }
    // SN32 ISP: 01/05/07 AA 55 …
    if packet.len() >= 3 && matches!(packet[0], 0x01 | 0x05 | 0x07) && packet[1] == 0xAA && packet[2] == 0x55 {
        return Err(Blocked::IspPattern);
    }
    if kind == PacketKind::Header {
        let op = packet.get(1).copied().unwrap_or(0);
        if packet.first() != Some(&PREFIX) || Op::from_byte(op).is_none() {
            return Err(Blocked::Opcode(op));
        }
    }
    Ok(())
}

/// Build a command header.
pub fn header(op: Op, arg: u8, count: u16) -> [u8; PAYLOAD] {
    let mut p = [0u8; PAYLOAD];
    p[0] = PREFIX;
    p[1] = op as u8;
    p[2] = arg;
    p[8] = (count & 0xFF) as u8;
    p[9] = (count >> 8) as u8;
    p
}

/// Wrap a payload into a feature report (report ID 0).
pub fn report(payload: &[u8; PAYLOAD]) -> [u8; REPORT_LEN] {
    let mut r = [0u8; REPORT_LEN];
    r[1..].copy_from_slice(payload);
    r
}

/// Parsed echo of a header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ack {
    pub op: u8,
    pub arg: u8,
    pub status: u8,
    /// For `04 02` with status 01: LE16 byte sum of the data received since
    /// the announcing header (measured on the AK980 PRO, fw 0x0106).
    pub sum: Option<u16>,
}

impl Ack {
    pub fn is_ok(&self) -> bool {
        self.status == STATUS_OK
    }
}

/// Parse a GET_FEATURE reply (with or without the leading report ID byte).
pub fn parse_ack(reply: &[u8]) -> Option<Ack> {
    let p = payload_of(reply);
    if p.len() < 4 || p[0] != PREFIX {
        return None;
    }
    let sum = (p[1] == Op::Commit as u8 && p[3] == STATUS_OK && p.len() >= 6).then(|| u16::from_le_bytes([p[4], p[5]]));
    Some(Ack { op: p[1], arg: p[2], status: p[3], sum })
}

/// Strip the report-ID byte that Windows prepends to feature replies.
pub fn payload_of(reply: &[u8]) -> &[u8] {
    if reply.len() == REPORT_LEN && reply[0] == 0 {
        &reply[1..]
    } else {
        reply
    }
}

/// An RGB colour.
pub type Rgb = [u8; 3];

/// Encode one real-time frame: entry `idx` lives at byte `idx * 4` as
/// `[idx R G B]`. Indices ≥ 128 are ignored.
///
/// Every slot from 1 up carries its own index, so LEDs that are not given
/// are sent as black. The firmware applies entries by their index byte: an
/// all-zero slot changes nothing, so a sparse frame left keys showing their
/// previous colour and an empty frame did not clear the keyboard (measured
/// with `kb live-probe` on 2026-11-10).
pub fn realtime_table(leds: impl IntoIterator<Item = (u8, Rgb)>) -> [u8; REALTIME_SLOTS * 4] {
    let mut t = [0u8; REALTIME_SLOTS * 4];
    for i in 1..REALTIME_SLOTS {
        t[i * 4] = i as u8;
    }
    for (idx, [r, g, b]) in leds {
        let i = idx as usize;
        if i < REALTIME_SLOTS {
            t[i * 4..i * 4 + 4].copy_from_slice(&[idx, r, g, b]);
        }
    }
    t
}

// ---------------------------------------------------------------- lighting

/// Trailer at payload bytes 14–15 of lighting packets and at the end of the
/// per-key table. `55 AA` is acknowledged but silently ignored.
pub const TRAILER: [u8; 2] = [0xAA, 0x55];
/// Highest brightness/speed level.
pub const MAX_LEVEL: u8 = 5;
/// The firmware's brightness scale: at level `n` a colour value `v` is
/// driven as `v * LEVEL_SCALE[n] >> 8`. Measured 2026-11-10 with the
/// Static effect and `04 F5`: white read back as 47, 95, 142, 190 and 253
/// at levels 1–5, and #8B5CF6 at level 5 as 137, 91, 244. Close to linear,
/// but not `n / 5`.
pub const LEVEL_SCALE: [u16; MAX_LEVEL as usize + 1] = [0, 48, 96, 143, 191, 254];

/// `v` at brightness `level`, as the firmware drives it.
pub fn scale_to_level(v: u8, level: u8) -> u8 {
    let m = LEVEL_SCALE[level.min(MAX_LEVEL) as usize];
    ((u16::from(v) * m) >> 8) as u8
}

/// Brightness `level` as a linear factor (0–1), for host-side dimming that
/// should match the firmware's own scale.
pub fn level_factor(level: u8) -> f32 {
    f32::from(LEVEL_SCALE[level.min(MAX_LEVEL) as usize]) / 256.0
}
/// Mode byte that selects the per-key colour table.
pub const MODE_PER_KEY: u8 = 0x80;
/// Data packets in the per-key colour table (`[idx R G B]` × 144).
pub const PER_KEY_PACKETS: u16 = 9;
/// Highest LED index the per-key table can address (entry 143 holds the trailer).
pub const PER_KEY_MAX_IDX: u8 = 142;

/// What an effect responds to (vendor `config_func` bits).
pub mod caps {
    pub const BRIGHTNESS: u8 = 0x01;
    pub const SPEED: u8 = 0x02;
    /// Two directions (0/1).
    pub const DIRECTION2: u8 = 0x04;
    /// Four directions (0–3).
    pub const DIRECTION4: u8 = 0x08;
    pub const COLOR: u8 = 0x10;
    pub const RAINBOW: u8 = 0x20;
}

/// A firmware lighting effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    pub id: u8,
    /// English name from the vendor language file; the UI translates it.
    pub name: &'static str,
    pub caps: u8,
}

impl Effect {
    pub fn has(&self, cap: u8) -> bool {
        self.caps & cap != 0
    }
}

/// Firmware effects in firmware order (docs/research/protokol-ff13.md §4.1).
/// `0 = Off` is listed last; it is known to block Fn+X on a sibling board.
pub const EFFECTS: [Effect; 20] = [
    Effect { id: 1, name: "Static", caps: 0x31 },
    Effect { id: 2, name: "Single on", caps: 0x33 },
    Effect { id: 3, name: "Single off", caps: 0x33 },
    Effect { id: 4, name: "Glittering", caps: 0x33 },
    Effect { id: 5, name: "Fluttering", caps: 0x33 },
    Effect { id: 6, name: "Colourful", caps: 0x03 },
    Effect { id: 7, name: "Breath", caps: 0x33 },
    Effect { id: 8, name: "Spectrum", caps: 0x03 },
    Effect { id: 9, name: "Outward", caps: 0x33 },
    Effect { id: 10, name: "Scrolling", caps: 0x3B },
    Effect { id: 11, name: "Rolling", caps: 0x37 },
    Effect { id: 12, name: "Rotating", caps: 0x37 },
    Effect { id: 13, name: "Explode", caps: 0x33 },
    Effect { id: 14, name: "Launch", caps: 0x33 },
    Effect { id: 15, name: "Ripples", caps: 0x33 },
    Effect { id: 16, name: "Flowing", caps: 0x37 },
    Effect { id: 17, name: "Pulsating", caps: 0x33 },
    Effect { id: 18, name: "Tilt", caps: 0x37 },
    Effect { id: 19, name: "Shuttle", caps: 0x33 },
    Effect { id: 0, name: "Off", caps: 0x00 },
];

/// Factory default effect (vendor config.xml `default_mode="11"`).
pub const FACTORY_EFFECT: u8 = 11;

pub fn effect(id: u8) -> Option<Effect> {
    EFFECTS.iter().copied().find(|e| e.id == id)
}

/// Settings of a firmware effect, validated before encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectSettings {
    pub mode: u8,
    pub color: Rgb,
    pub rainbow: bool,
    pub brightness: u8,
    pub speed: u8,
    pub direction: u8,
}

impl EffectSettings {
    /// Factory defaults: Rolling, rainbow, full brightness, speed 3.
    pub fn factory() -> Self {
        Self { mode: FACTORY_EFFECT, color: [0xFF, 0x00, 0x00], rainbow: true, brightness: 5, speed: 3, direction: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Invalid {
    #[error("unknown effect 0x{0:02X}")]
    Mode(u8),
    #[error("{field} {value} is above {max}")]
    Range { field: &'static str, value: u8, max: u8 },
    #[error("LED index {0} is above {PER_KEY_MAX_IDX}")]
    LedIndex(u8),
}

/// Encode the data packet that follows `04 13`.
pub fn lighting_data(s: &EffectSettings) -> Result<[u8; PAYLOAD], Invalid> {
    let fx = effect(s.mode).ok_or(Invalid::Mode(s.mode))?;
    let check = |field, value: u8, max| if value > max { Err(Invalid::Range { field, value, max }) } else { Ok(()) };
    check("brightness", s.brightness, MAX_LEVEL)?;
    check("speed", s.speed, MAX_LEVEL)?;
    check("direction", s.direction, 3)?;
    let mut p = [0u8; PAYLOAD];
    p[0] = fx.id;
    if fx.id != 0 {
        p[1..4].copy_from_slice(&s.color);
        // Modes 1/5/7 followed by colour AA 55 would look like an SN32 ISP
        // command to `guard`. Nudge green by one step (invisible) instead.
        if matches!(p[0], 0x01 | 0x05 | 0x07) && p[1] == 0xAA && p[2] == 0x55 {
            p[2] = 0x56;
        }
        p[8] = s.rainbow as u8;
        p[9] = s.brightness;
        p[10] = s.speed;
        p[11] = s.direction;
    }
    p[14..16].copy_from_slice(&TRAILER);
    Ok(p)
}

/// The lighting packet that switches to per-key mode at `brightness`.
pub fn per_key_mode_data(brightness: u8) -> Result<[u8; PAYLOAD], Invalid> {
    if brightness > MAX_LEVEL {
        return Err(Invalid::Range { field: "brightness", value: brightness, max: MAX_LEVEL });
    }
    let mut p = [0u8; PAYLOAD];
    p[0] = MODE_PER_KEY;
    p[9] = brightness;
    p[14..16].copy_from_slice(&TRAILER);
    Ok(p)
}

/// Encode the 576-byte per-key table: entry `idx` at `idx * 4`, `AA 55` in
/// the last two bytes. Like [`realtime_table`], every entry 1–142 carries
/// its index so keys that are not given are stored as black instead of
/// keeping an earlier colour.
pub fn per_key_table(
    colors: impl IntoIterator<Item = (u8, Rgb)>,
) -> Result<[u8; PER_KEY_PACKETS as usize * PAYLOAD], Invalid> {
    let mut t = [0u8; PER_KEY_PACKETS as usize * PAYLOAD];
    for i in 1..=PER_KEY_MAX_IDX as usize {
        t[i * 4] = i as u8;
    }
    for (idx, rgb) in colors {
        if idx > PER_KEY_MAX_IDX {
            return Err(Invalid::LedIndex(idx));
        }
        let i = idx as usize * 4;
        t[i] = idx;
        t[i + 1..i + 4].copy_from_slice(&rgb);
    }
    let n = t.len();
    t[n - 2..].copy_from_slice(&TRAILER);
    Ok(t)
}

/// Decode a colour read-back: `[idx R G B]` entries, skipping empty ones.
///
/// The red byte is not the red value: see [`readback_red_candidates`].
pub fn parse_color_table(bytes: &[u8]) -> Vec<(u8, Rgb)> {
    bytes.chunks_exact(4).filter(|e| e != &[0, 0, 0, 0]).map(|e| (e[0], [e[1], e[2], e[3]])).collect()
}

/// The two red values a read-back red byte can stand for.
///
/// `04 F5` reports red as `2·r mod 256`, where `r` is the red value the
/// LED is driven with; green and blue are reported as they are. Measured
/// 2026-11-10 on firmware 0x0106: static white at level 5 (r = 253) reads
/// `FA FD FD`, #808080 (r = 127) reads `FE 7F 7F`, and the red Fn
/// indicator (r = 255) reads `FE 00 00`. The top bit of red is lost, so a
/// byte fits both `b / 2` and `b / 2 + 128`.
pub fn readback_red_candidates(byte: u8) -> [u8; 2] {
    let lo = byte >> 1;
    [lo, lo | 0x80]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_layout() {
        let h = header(Op::Realtime, 0, 8);
        assert_eq!(&h[..10], &[0x04, 0x20, 0, 0, 0, 0, 0, 0, 8, 0]);
        let h = header(Op::ScreenUpload, 1, 0x0110);
        assert_eq!((h[2], h[8], h[9]), (1, 0x10, 0x01));
    }

    #[test]
    fn report_prepends_id_zero() {
        let r = report(&header(Op::Commit, 0, 0));
        assert_eq!(r.len(), 65);
        assert_eq!(&r[..3], &[0, 0x04, 0x02]);
    }

    #[test]
    fn guard_allows_known_headers_and_data() {
        assert!(guard(&header(Op::Realtime, 0, 8), PacketKind::Header).is_ok());
        assert!(guard(&[1, 255, 0, 0], PacketKind::Data).is_ok());
    }

    #[test]
    fn guard_blocks_unknown_opcodes() {
        let mut p = header(Op::Begin, 0, 0);
        for op in [0x38u8, 0x05, 0x10, 0x12, 0x22, 0x26, 0x50, 0xAB, 0xE0, 0xF1, 0xF6] {
            p[1] = op;
            assert_eq!(guard(&p, PacketKind::Header), Err(Blocked::Opcode(op)), "op {op:02X}");
        }
        // A header must start with 04.
        assert!(guard(&[0x20, 0x01], PacketKind::Header).is_err());
    }

    #[test]
    fn guard_blocks_bootloader_everywhere() {
        let magic = [0xAA, 0x42, 0x89, 0x5A, 0xFF, 0x71, 0x62, 0xCC, 0, 0];
        assert_eq!(guard(&magic, PacketKind::Data), Err(Blocked::BootloaderMagic));
        assert_eq!(guard(&magic, PacketKind::Header), Err(Blocked::BootloaderMagic));
        assert_eq!(guard(&[0x05, 0xAA, 0x55, 0], PacketKind::Data), Err(Blocked::IspPattern));
        assert_eq!(guard(&[0u8; 65], PacketKind::Data), Err(Blocked::TooLong(65)));
    }

    #[test]
    fn ack_parsing_with_and_without_report_id() {
        let mut wire = [0u8; 65];
        wire[1..5].copy_from_slice(&[0x04, 0x20, 0x00, 0x01]);
        assert_eq!(parse_ack(&wire), Some(Ack { op: 0x20, arg: 0, status: 1, sum: None }));
        // Real commit echo captured on the AK980 PRO after the factory effect.
        assert_eq!(parse_ack(&[0x04, 0x02, 0x00, 0x01, 0x12, 0x02]).unwrap().sum, Some(0x0212));
        assert!(parse_ack(&wire).unwrap().is_ok());
        assert_eq!(parse_ack(&[0x04, 0x18, 0x00, 0xFF]).unwrap().status, STATUS_REFUSED);
        assert_eq!(parse_ack(&[0u8; 65]), None);
    }

    #[test]
    fn realtime_table_places_entries_by_index() {
        let t = realtime_table([(1, [255, 0, 0]), (123, [0, 0, 255]), (200, [9, 9, 9])]);
        assert_eq!(&t[4..8], &[1, 255, 0, 0]);
        assert_eq!(&t[492..496], &[123, 0, 0, 255]);
        assert_eq!(&t[..4], &[0, 0, 0, 0]);
        assert_eq!(t.len(), 512);
    }

    #[test]
    fn missing_leds_are_sent_as_black() {
        // An empty frame must clear the keyboard: every slot names its LED.
        let t = realtime_table([]);
        for i in 1..REALTIME_SLOTS {
            assert_eq!(&t[i * 4..i * 4 + 4], &[i as u8, 0, 0, 0], "slot {i}");
        }
        let p = per_key_table([(2, [9, 9, 9])]).unwrap();
        assert_eq!(&p[4..8], &[1, 0, 0, 0]);
        assert_eq!(&p[8..12], &[2, 9, 9, 9]);
        assert_eq!(&p[568..572], &[142, 0, 0, 0]);
        assert_eq!(&p[572..576], &[0, 0, 0xAA, 0x55], "entry 143 holds only the trailer");
        // Packets still never start like an ISP command.
        for chunk in p.chunks_exact(PAYLOAD).chain(t.chunks_exact(PAYLOAD)) {
            assert!(guard(chunk, PacketKind::Data).is_ok());
        }
    }

    #[test]
    fn lighting_packet_matches_hardware_golden() {
        // S2 golden.json:467, verified on the AK980: mode 2, FFD200, bright 4, speed 1, dir 2.
        let s = EffectSettings {
            mode: 2,
            color: [0xFF, 0xD2, 0x00],
            rainbow: false,
            brightness: 4,
            speed: 1,
            direction: 2,
        };
        let p = lighting_data(&s).unwrap();
        assert_eq!(&p[..16], &[0x02, 0xFF, 0xD2, 0x00, 0, 0, 0, 0, 0x00, 0x04, 0x01, 0x02, 0, 0, 0xAA, 0x55]);
        assert!(p[16..].iter().all(|&b| b == 0));
    }

    #[test]
    fn off_carries_only_mode_and_trailer() {
        let s = EffectSettings { mode: 0, ..EffectSettings::factory() };
        let p = lighting_data(&s).unwrap();
        assert_eq!(&p[..16], &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xAA, 0x55]);
    }

    #[test]
    fn lighting_validation() {
        let ok = EffectSettings::factory();
        assert_eq!(lighting_data(&EffectSettings { mode: 0x14, ..ok }), Err(Invalid::Mode(0x14)));
        assert_eq!(
            lighting_data(&EffectSettings { mode: 0x20, ..ok }),
            Err(Invalid::Mode(0x20)),
            "side light needs its own flag"
        );
        assert!(matches!(lighting_data(&EffectSettings { brightness: 6, ..ok }), Err(Invalid::Range { .. })));
        assert!(matches!(lighting_data(&EffectSettings { direction: 4, ..ok }), Err(Invalid::Range { .. })));
        assert_eq!(EFFECTS.len(), 20);
        assert!(effect(FACTORY_EFFECT).unwrap().has(caps::RAINBOW));
    }

    #[test]
    fn lighting_never_trips_the_isp_guard() {
        for mode in [1u8, 5, 7] {
            let s = EffectSettings { mode, color: [0xAA, 0x55, 0x10], ..EffectSettings::factory() };
            let p = lighting_data(&s).unwrap();
            assert!(guard(&p, PacketKind::Data).is_ok(), "mode {mode}");
            assert_eq!(&p[1..4], &[0xAA, 0x56, 0x10]);
        }
    }

    #[test]
    fn per_key_encoding() {
        let m = per_key_mode_data(5).unwrap();
        assert_eq!(&m[..16], &[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0xAA, 0x55]);
        let t = per_key_table([(1, [0, 0xFF, 0]), (39, [0xFF, 0, 0])]).unwrap();
        assert_eq!(t.len(), 576);
        assert_eq!(&t[4..8], &[1, 0, 0xFF, 0]);
        assert_eq!(&t[156..160], &[39, 0xFF, 0, 0]);
        assert_eq!(&t[574..], &[0xAA, 0x55]);
        assert_eq!(per_key_table([(143, [1, 1, 1])]), Err(Invalid::LedIndex(143)));
    }

    #[test]
    fn color_table_round_trip() {
        let t = realtime_table([(1, [1, 2, 3]), (19, [4, 5, 6])]);
        let lit: Vec<_> = parse_color_table(&t).into_iter().filter(|(_, c)| c != &[0, 0, 0]).collect();
        assert_eq!(lit, vec![(1, [1, 2, 3]), (19, [4, 5, 6])]);
        assert_eq!(parse_color_table(&t).len(), REALTIME_SLOTS - 1, "black LEDs are listed too");
    }

    #[test]
    fn level_scale_matches_measurements() {
        // Static white at levels 1–5 and #8B5CF6 at level 5, read back on 2026-11-10.
        assert_eq!((1..=5).map(|l| scale_to_level(255, l)).collect::<Vec<_>>(), [47, 95, 142, 190, 253]);
        assert_eq!([0x8B, 0x5C, 0xF6].map(|v| scale_to_level(v, 5)), [137, 91, 244]);
        assert_eq!(scale_to_level(255, 0), 0);
        assert_eq!(scale_to_level(255, 9), 253, "levels above the maximum are clamped");
        assert!((level_factor(5) - 0.992).abs() < 0.001);
    }

    #[test]
    fn red_readback_drops_the_top_bit() {
        // (red driven, red byte read back), measured on 2026-11-10.
        let seen = [(253u8, 0xFAu8), (127, 0xFE), (63, 0x7E), (137, 0x12), (142, 0x1C), (47, 0x5E), (255, 0xFE)];
        for (driven, byte) in seen {
            assert_eq!(driven.wrapping_mul(2), byte);
            assert!(readback_red_candidates(byte).contains(&driven), "{driven} from {byte:#04X}");
        }
        assert_eq!(readback_red_candidates(0xFE), [127, 255]);
        assert_eq!(readback_red_candidates(0), [0, 128]);
    }
}
