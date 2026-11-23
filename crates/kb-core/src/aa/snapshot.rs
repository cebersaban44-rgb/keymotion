//! Everything an "AA" keyboard lets the host read, as one backup
//! ([`AaSnapshot`]), and the sanity gate over it ([`check`]): the driver
//! writes nothing until a full read looks like the keyboard it knows.

use super::UsbIdentity;
use kb_protocol::aa::effect::{self, AaEffect, EffectColor};
use kb_protocol::aa::info::{self, DeviceInfo};
use kb_protocol::aa::macros::{self, AaMacroEntry};
use kb_protocol::aa::{keymap, perkey};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Bytes stored as a lowercase hex string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hex(pub Vec<u8>);

impl Serialize for Hex {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = String::with_capacity(self.0.len() * 2);
        for b in &self.0 {
            out.push_str(&format!("{b:02x}"));
        }
        s.serialize_str(&out)
    }
}

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s.len() % 2 != 0 || !s.is_ascii() {
            return Err(serde::de::Error::custom("hex string of odd length"));
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(serde::de::Error::custom))
            .collect::<Result<Vec<u8>, _>>()
            .map(Hex)
    }
}

/// The device information, field by field (for people reading a backup).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub rom_size: u8,
    pub macro_space_size: u16,
    pub vid: u16,
    pub pid: u16,
    pub version: u16,
    pub sensor: u16,
    pub manufacturer: u16,
    pub product: u16,
    pub work_mode: u8,
    pub battery_level: u8,
    pub charge_status: u8,
    pub current_profile: u8,
    pub axis_info: u16,
    pub tft_max_frames: u16,
    pub gif_max_frames: u16,
    pub led_max_frames: u16,
    pub tft_direction: u8,
    pub rt_precision: u8,
    pub frame_version: u8,
    pub lighting_version: u8,
    pub firmware_status: u8,
    pub max_profile: u8,
    pub tft_color_byte_order: u8,
}

impl From<DeviceInfo> for DeviceRecord {
    fn from(i: DeviceInfo) -> Self {
        Self {
            rom_size: i.rom_size,
            macro_space_size: i.macro_space_size,
            vid: i.vid,
            pid: i.pid,
            version: i.version,
            sensor: i.sensor,
            manufacturer: i.manufacturer,
            product: i.product,
            work_mode: i.work_mode,
            battery_level: i.battery_level,
            charge_status: i.charge_status,
            current_profile: i.current_profile,
            axis_info: i.axis_info,
            tft_max_frames: i.tft_max_frames,
            gif_max_frames: i.gif_max_frames,
            led_max_frames: i.led_max_frames,
            tft_direction: i.tft_direction,
            rt_precision: i.rt_precision,
            frame_version: i.frame_version,
            lighting_version: i.lighting_version,
            firmware_status: i.firmware_status,
            max_profile: i.max_profile,
            tft_color_byte_order: i.tft_color_byte_order,
        }
    }
}

/// An effect colour and colour mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorRecord {
    pub color: [u8; 3],
    pub color_mode: u8,
}

impl From<EffectColor> for ColorRecord {
    fn from(c: EffectColor) -> Self {
        Self { color: c.color, color_mode: c.color_mode }
    }
}

impl From<ColorRecord> for EffectColor {
    fn from(c: ColorRecord) -> Self {
        Self { color: c.color, color_mode: c.color_mode }
    }
}

/// One stored macro: its slot, its address and its bytes (header and
/// entries).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroBlock {
    pub slot: u8,
    pub addr: u32,
    pub bytes: Hex,
}

/// A full read of the keyboard: device information, options, both keymap
/// layers and the factory Fn layer, the effect, the per-key table and the
/// macros, byte for byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AaSnapshot {
    /// [`AaSnapshot::FORMAT`].
    pub format: u32,
    /// `10`: 48 bytes.
    pub info: Hex,
    /// The same, parsed.
    pub device: DeviceRecord,
    /// `11`: 56 bytes (never written back).
    pub options: Hex,
    /// `12`: 512 bytes.
    pub keymap: Hex,
    /// `16`: 512 bytes.
    pub fn_keymap: Hex,
    /// `1C`: 512 bytes (read only).
    pub default_fn: Hex,
    /// `13`: 16 bytes, as read back. The colour and colour mode in them
    /// are not reliable ([`effect::READBACK_QUIRK`]).
    pub effect: Hex,
    /// The effect's colour and colour mode, when the driver knew them: it
    /// had written them itself, or the read-back was not the quirk. `None`
    /// = unknown; a restore then needs the colour from the caller.
    #[serde(default)]
    pub effect_color: Option<ColorRecord>,
    /// `14`: 512 bytes.
    pub per_key: Hex,
    /// `15` addresses 0–399.
    pub macro_table: Hex,
    /// Every block the table points to.
    pub macro_blocks: Vec<MacroBlock>,
}

impl AaSnapshot {
    pub const FORMAT: u32 = 1;

    pub fn device_info(&self) -> Option<DeviceInfo> {
        DeviceInfo::parse(&self.info.0).ok()
    }

    /// The effect as read back (see [`Self::effect_color`]).
    pub fn effect(&self) -> Option<AaEffect> {
        AaEffect::decode(&self.effect.0).ok()
    }

    /// The stored macros, slot by slot (100 slots; empty = none).
    pub fn macros(&self) -> Vec<Vec<AaMacroEntry>> {
        let mut slots = vec![Vec::new(); macros::MAX_MACROS];
        for b in &self.macro_blocks {
            if let Some(s) = slots.get_mut(usize::from(b.slot)) {
                *s = macros::decode_entries(b.bytes.0.get(4..).unwrap_or_default());
            }
        }
        slots
    }
}

/// Highest macro address the driver reads: the memory the keyboard reports,
/// at least the table and the 512 bytes AJAZZ's WebHID driver writes. What
/// the keyboard holds there may be read and backed up; writes stay within
/// [`macros::CAPACITY`].
pub(crate) fn macro_limit(info: &DeviceInfo) -> u64 {
    u64::from(info.macro_space_size).max((macros::TABLE_LEN + macros::CAPACITY) as u64)
}

/// Whether a block of `len` bytes at `addr` lies inside the macro memory
/// (no arithmetic can overflow).
pub(crate) fn block_fits(addr: u32, len: usize, limit: u64) -> bool {
    u64::from(addr) >= u64::from(macros::BLOCKS_AT) && u64::from(addr) + len as u64 <= limit
}

/// Highest brightness and speed a read may show (written: 1–5 only).
pub const READ_LEVEL_MAX: u8 = 6;

/// The sanity gate: does this read look like the keyboard the driver
/// knows, on the cable with these USB ids? `Err` lists every reason not to
/// write to it.
pub fn check(s: &AaSnapshot, usb: UsbIdentity) -> Result<(), Vec<String>> {
    let mut why = Vec::new();
    let lengths = [
        ("device information", s.info.0.len(), info::LEN),
        ("options", s.options.0.len(), 56),
        ("base layer", s.keymap.0.len(), keymap::LEN),
        ("Fn layer", s.fn_keymap.0.len(), keymap::LEN),
        ("factory Fn layer", s.default_fn.0.len(), keymap::LEN),
        ("effect", s.effect.0.len(), effect::LEN),
        ("per-key table", s.per_key.0.len(), perkey::LEN),
        ("macro table", s.macro_table.0.len(), macros::TABLE_LEN),
    ];
    for (what, got, want) in lengths {
        if got != want {
            why.push(format!("the {what} read is {got} bytes, expected {want}"));
        }
    }
    if s.format != AaSnapshot::FORMAT {
        why.push(format!("backup format {} is not {}", s.format, AaSnapshot::FORMAT));
    }
    if !why.is_empty() {
        return Err(why);
    }

    let i = DeviceInfo::parse(&s.info.0).expect("length checked");
    if DeviceRecord::from(i) != s.device {
        why.push("the parsed device information does not match its bytes".into());
    }
    if i.vid != crate::detect::SONIX_VID {
        why.push(format!("vendor id {:04X}, expected {:04X}", i.vid, crate::detect::SONIX_VID));
    }
    if i.pid != usb.pid {
        why.push(format!("product id {:04X}, but the USB device is {:04X}", i.pid, usb.pid));
    }
    if i.version != usb.release {
        why.push(format!("firmware version {:04X}, but the USB device reports {:04X}", i.version, usb.release));
    }
    if i.firmware_status != 0 {
        why.push(format!("firmware status {} (1 = waiting for an update)", i.firmware_status));
    }

    for (name, table) in [("base layer", &s.keymap), ("Fn layer", &s.fn_keymap), ("factory Fn layer", &s.default_fn)] {
        for slot in 0..keymap::SLOTS as u8 {
            let page = keymap::entry(&table.0, slot)[0];
            if !keymap::is_known_page(page) {
                why.push(format!("{name} slot {slot}: unknown entry page 0x{page:02X}"));
            }
        }
    }

    let e = AaEffect::decode(&s.effect.0).expect("length checked");
    if !effect::is_valid_mode(e.mode) {
        why.push(format!("unknown effect mode {}", e.mode));
    }
    // Levels other tools leave on this family (0–5 in the web hub and
    // AK980-Manager, 6 on some siblings) are plausible to read; writes
    // only ever send 1–5. Off (mode 0) may hold anything.
    let plausible = 0..=READ_LEVEL_MAX;
    if e.mode != 0 && (!plausible.contains(&e.brightness) || !plausible.contains(&e.speed)) {
        why.push(format!("effect brightness {} / speed {} outside 0–{READ_LEVEL_MAX}", e.brightness, e.speed));
    }
    if e.direction > effect::MAX_DIRECTION || e.color_mode > 1 {
        why.push(format!("effect direction {} / colour mode {}", e.direction, e.color_mode));
    }
    if s.effect_color.is_some_and(|c| c.color_mode > 1) {
        why.push("the remembered effect colour mode is not 0 or 1".into());
    }

    for slot in 0..perkey::SLOTS {
        if s.per_key.0[slot * 4] != slot as u8 {
            why.push(format!("per-key entry {slot} starts with {}", s.per_key.0[slot * 4]));
        }
    }

    let limit = macro_limit(&i);
    for (slot, &p) in macros::pointers(&s.macro_table.0).iter().enumerate() {
        if macros::is_none(p) {
            continue;
        }
        let block = s.macro_blocks.iter().find(|b| usize::from(b.slot) == slot && b.addr == p);
        let ok = block.is_some_and(|b| {
            b.bytes.0.len() >= 4
                && b.bytes.0.len() == 4 + 4 * macros::header_entries(&b.bytes.0)
                && block_fits(p, b.bytes.0.len(), limit)
        });
        if !ok {
            why.push(format!("macro {slot} at address {p} is outside the macro memory or unreadable"));
        }
    }
    let pointers = macros::pointers(&s.macro_table.0);
    for b in &s.macro_blocks {
        if pointers.get(usize::from(b.slot)) != Some(&b.addr) {
            why.push(format!("a stored block for macro {} is not in the table", b.slot));
        }
    }

    if why.is_empty() {
        Ok(())
    } else {
        Err(why)
    }
}
