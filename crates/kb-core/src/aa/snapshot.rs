//! Everything an "AA" keyboard lets the host read, as one backup
//! ([`AaSnapshot`]), and the sanity gate over it ([`check`]): the driver
//! writes nothing until a full read looks like the keyboard it knows.
//!
//! The gate looks at what firmware 769 really holds (protocol notes §F):
//! the per-key table may be all `00` (after a firmware update or a HID
//! reset) or all `FF` (never programmed), so its entries are not checked;
//! byte 32 of the device information is no field of that firmware, so it
//! is not either.

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
    /// are not reliable ([`effect::READBACK_QUIRK`]), nor is a mode of 0
    /// (modes 0, 20, 21 and 128 all read back so: [`Self::effect_mode`]).
    pub effect: Hex,
    /// The effect's colour and colour mode, when the driver knew them: it
    /// had written them itself, or the read-back was not the quirk. `None`
    /// = unknown; a restore then needs the colour from the caller.
    #[serde(default)]
    pub effect_color: Option<ColorRecord>,
    /// The effect mode when the read-back says 0 and the driver knew which
    /// mode that is (it wrote it, or the app's record said so): 0, 20, 21
    /// or 128. `None` with a read-back of 0: the keyboard was off or in one
    /// of the per-key modes, and nothing tells which.
    #[serde(default)]
    pub effect_mode: Option<u8>,
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

    /// The effect as read back, with its mode as known
    /// ([`Self::effect_mode`]; the colour: [`Self::effect_color`]).
    pub fn effect(&self) -> Option<AaEffect> {
        let e = AaEffect::decode(&self.effect.0).ok()?;
        Some(AaEffect { mode: self.effect_mode.unwrap_or(e.mode), ..e })
    }

    /// Whether [`Self::effect`]'s mode is the keyboard's: the read-back was
    /// not 0, or the mode behind the 0 is known.
    pub fn effect_mode_known(&self) -> bool {
        self.effect_mode.is_some() || self.effect.0.first().is_some_and(|&m| m != 0)
    }

    /// The stored macros, slot by slot (100 slots; empty = none). A slot
    /// whose pointer leads nowhere ([`broken_macros`]) is empty.
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
/// at least the table and the 512 bytes AJAZZ's WebHID driver writes, at
/// most the three pages of macro memory ([`macros::AREA_LEN`]). Writes
/// stay within [`macros::capacity`].
pub(crate) fn macro_limit(info: &DeviceInfo) -> u64 {
    let reported = usize::from(info.macro_space_size).max(macros::TABLE_LEN + macros::CAPACITY);
    reported.min(macros::AREA_LEN) as u64
}

/// Whether a block of `len` bytes at `addr` lies inside the macro memory
/// (no arithmetic can overflow).
pub(crate) fn block_fits(addr: u32, len: usize, limit: u64) -> bool {
    u64::from(addr) >= u64::from(macros::BLOCKS_AT) && u64::from(addr) + len as u64 <= limit
}

/// Macro slots (slot, address) a macro write that stopped part-way can
/// leave (the table goes first): a pointer into the table itself (below
/// [`macros::BLOCKS_AT`]) or to erased flash (a block header of `FF FF`).
/// They read as empty slots and are no reason to refuse writes: the next
/// macro save writes the whole image and repairs the table. Shown in the
/// log and the report.
///
/// A pointer past the macro memory is not among them: no write of the
/// driver leaves one (its pointers stay inside, and programming over a page
/// that was not erased only lowers a value), so it comes from another
/// format or a misread. It stays a reason ([`check`]): read as empty, the
/// first macro save would wipe macros that may be real.
pub fn broken_macros(s: &AaSnapshot) -> Vec<(u8, u32)> {
    let pointers = macros::pointers(&s.macro_table.0);
    pointers
        .iter()
        .enumerate()
        .filter(|&(_, &p)| !macros::is_none(p))
        .filter(|&(slot, &p)| {
            if p < u32::from(macros::BLOCKS_AT) {
                return true;
            }
            let block = s.macro_blocks.iter().find(|b| usize::from(b.slot) == slot && b.addr == p);
            block.is_some_and(|b| b.bytes.0.get(..2) == Some(&[0xFF, 0xFF][..]))
        })
        .map(|(slot, &p)| (slot as u8, p))
        .collect()
}

/// Highest brightness and speed a read may show (written: 1–5 only).
pub const READ_LEVEL_MAX: u8 = 6;

/// One reason the sanity gate gives: what is wrong, where (a section, a
/// slot, a macro) and the value found. [`Reason::kind`] names the kind, so
/// a report can group them; the text is [`std::fmt::Display`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// A section read is not the length it should be.
    Length {
        section: &'static str,
        got: usize,
        want: usize,
    },
    /// A backup of another format.
    Format {
        got: u32,
    },
    /// The parsed device information differs from its bytes (a backup).
    DeviceRecord,
    VendorId {
        got: u16,
    },
    ProductId {
        got: u16,
        usb: u16,
    },
    Version {
        got: u16,
        usb: u16,
    },
    /// A keymap slot holds a page this driver does not know.
    UnknownPage {
        layer: &'static str,
        slot: u8,
        page: u8,
    },
    EffectMode {
        mode: u8,
    },
    EffectLevels {
        brightness: u8,
        speed: u8,
    },
    EffectDirection {
        direction: u8,
        color_mode: u8,
    },
    /// The remembered effect colour mode is not 0 or 1 (a backup).
    RememberedColorMode {
        color_mode: u8,
    },
    /// The remembered mode behind a read-back of 0 is not one of those
    /// that read so, or the read-back is not 0 (a backup).
    RememberedMode {
        mode: u8,
        read: u8,
    },
    /// A macro block that does not hold what its header says, or a pointer
    /// past the macro memory; a pointer into the table or to erased flash
    /// is no reason ([`broken_macros`]).
    MacroBlock {
        slot: u8,
        addr: u32,
    },
    /// A stored block no table entry names (a backup).
    StrayBlock {
        slot: u8,
    },
}

impl Reason {
    /// The kind, for grouping (`"unknown-page"`, `"macro-block"` …).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Length { .. } => "length",
            Self::Format { .. } => "format",
            Self::DeviceRecord => "device-record",
            Self::VendorId { .. } => "vendor-id",
            Self::ProductId { .. } => "product-id",
            Self::Version { .. } => "version",
            Self::UnknownPage { .. } => "unknown-page",
            Self::EffectMode { .. } => "effect-mode",
            Self::EffectLevels { .. } => "effect-levels",
            Self::EffectDirection { .. } => "effect-direction",
            Self::RememberedColorMode { .. } => "remembered-colour-mode",
            Self::RememberedMode { .. } => "remembered-mode",
            Self::MacroBlock { .. } => "macro-block",
            Self::StrayBlock { .. } => "stray-block",
        }
    }
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Length { section, got, want } => write!(f, "the {section} read is {got} bytes, expected {want}"),
            Self::Format { got } => write!(f, "backup format {got} is not {}", AaSnapshot::FORMAT),
            Self::DeviceRecord => write!(f, "the parsed device information does not match its bytes"),
            Self::VendorId { got } => write!(f, "vendor id {got:04X}, expected {:04X}", crate::detect::SONIX_VID),
            Self::ProductId { got, usb } => write!(f, "product id {got:04X}, but the USB device is {usb:04X}"),
            Self::Version { got, usb } => {
                write!(f, "firmware version {got:04X}, but the USB device reports {usb:04X}")
            }
            Self::UnknownPage { layer, slot, page } => {
                write!(f, "{layer} slot {slot}: unknown entry page 0x{page:02X}")
            }
            Self::EffectMode { mode } => write!(f, "unknown effect mode {mode}"),
            Self::EffectLevels { brightness, speed } => {
                write!(f, "effect brightness {brightness} / speed {speed} outside 0–{READ_LEVEL_MAX}")
            }
            Self::EffectDirection { direction, color_mode } => {
                write!(f, "effect direction {direction} / colour mode {color_mode}")
            }
            Self::RememberedColorMode { color_mode } => {
                write!(f, "the remembered effect colour mode {color_mode} is not 0 or 1")
            }
            Self::RememberedMode { mode, read } => {
                write!(f, "the remembered effect mode {mode} does not read back as {read}")
            }
            Self::MacroBlock { slot, addr } => {
                write!(f, "macro slot {slot} at address {addr} does not hold what its header says")
            }
            Self::StrayBlock { slot } => write!(f, "macro slot {slot}: a stored block is not in the table"),
        }
    }
}

/// The sanity gate: does this read look like the keyboard the driver
/// knows, on the cable with these USB ids? `Err` lists every reason not to
/// write to it ([`check_reasons`], as text).
pub fn check(s: &AaSnapshot, usb: UsbIdentity) -> Result<(), Vec<String>> {
    let why = check_reasons(s, usb);
    if why.is_empty() {
        Ok(())
    } else {
        Err(why.iter().map(ToString::to_string).collect())
    }
}

/// Every reason the sanity gate finds not to write to the keyboard this
/// read came from; none: writes are allowed.
pub fn check_reasons(s: &AaSnapshot, usb: UsbIdentity) -> Vec<Reason> {
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
    for (section, got, want) in lengths {
        if got != want {
            why.push(Reason::Length { section, got, want });
        }
    }
    if s.format != AaSnapshot::FORMAT {
        why.push(Reason::Format { got: s.format });
    }
    if !why.is_empty() {
        return why;
    }

    let i = DeviceInfo::parse(&s.info.0).expect("length checked");
    if DeviceRecord::from(i) != s.device {
        why.push(Reason::DeviceRecord);
    }
    if i.vid != crate::detect::SONIX_VID {
        why.push(Reason::VendorId { got: i.vid });
    }
    if i.pid != usb.pid {
        why.push(Reason::ProductId { got: i.pid, usb: usb.pid });
    }
    if i.version != usb.release {
        why.push(Reason::Version { got: i.version, usb: usb.release });
    }

    for (layer, table) in [("base layer", &s.keymap), ("Fn layer", &s.fn_keymap), ("factory Fn layer", &s.default_fn)] {
        for slot in 0..keymap::SLOTS as u8 {
            let page = keymap::entry(&table.0, slot)[0];
            if !keymap::is_known_page(page) {
                why.push(Reason::UnknownPage { layer, slot, page });
            }
        }
    }

    let e = AaEffect::decode(&s.effect.0).expect("length checked");
    if !effect::is_valid_mode(e.mode) {
        why.push(Reason::EffectMode { mode: e.mode });
    }
    // Levels other tools leave on this family (0–5 in the web hub and
    // AK980-Manager, 6 on some siblings) are plausible to read; writes
    // only ever send 1–5. Off (mode 0) may hold anything.
    let plausible = 0..=READ_LEVEL_MAX;
    if e.mode != 0 && (!plausible.contains(&e.brightness) || !plausible.contains(&e.speed)) {
        why.push(Reason::EffectLevels { brightness: e.brightness, speed: e.speed });
    }
    if e.direction > effect::MAX_DIRECTION || e.color_mode > 1 {
        why.push(Reason::EffectDirection { direction: e.direction, color_mode: e.color_mode });
    }
    if let Some(c) = s.effect_color.filter(|c| c.color_mode > 1) {
        why.push(Reason::RememberedColorMode { color_mode: c.color_mode });
    }
    if let Some(m) = s.effect_mode.filter(|&m| !effect::READS_AS_OFF.contains(&m) || e.mode != 0) {
        why.push(Reason::RememberedMode { mode: m, read: e.mode });
    }

    // The per-key table is not checked: all 00 and all FF are normal, and
    // an entry without its own index only leaves its LED dark.

    let limit = macro_limit(&i);
    let broken = broken_macros(s);
    for (slot, &p) in macros::pointers(&s.macro_table.0).iter().enumerate() {
        if macros::is_none(p) || broken.contains(&(slot as u8, p)) {
            continue;
        }
        let block = s.macro_blocks.iter().find(|b| usize::from(b.slot) == slot && b.addr == p);
        let ok = block.is_some_and(|b| {
            b.bytes.0.len() >= 4
                && b.bytes.0.len() == 4 + 4 * macros::header_entries(&b.bytes.0)
                && block_fits(p, b.bytes.0.len(), limit)
        });
        if !ok {
            why.push(Reason::MacroBlock { slot: slot as u8, addr: p });
        }
    }
    let pointers = macros::pointers(&s.macro_table.0);
    for b in &s.macro_blocks {
        if pointers.get(usize::from(b.slot)) != Some(&b.addr) {
            why.push(Reason::StrayBlock { slot: b.slot });
        }
    }
    why
}
