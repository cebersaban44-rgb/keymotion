//! Keymap tables (`04 11` base layer, `04 27` Fn layer).
//!
//! A layer is 144 four-byte entries, entry `idx` at byte `idx * 4`, with
//! `AA 55` in the last two bytes (docs/research/protokol-ff13.md §6). The
//! index is the key's LED index. Writing a layer replaces all of it: keys
//! that are not given go back to the firmware default. The keyboard cannot
//! report its keymap, so callers keep their own copy.

use super::{Invalid, Op, PAYLOAD, TRAILER};

/// Data packets in one keymap layer.
pub const KEYMAP_PACKETS: u16 = 9;
/// Bytes in one keymap layer.
pub const KEYMAP_LEN: usize = KEYMAP_PACKETS as usize * PAYLOAD;
/// Highest usable entry; the trailer overwrites the end of entry 143.
pub const KEYMAP_MAX_IDX: u8 = 142;
/// The vendor usage of the Fn key. Fn is never remapped and never offered
/// as a target: without it the Fn layer and the firmware functions are gone.
pub const FN_USAGE: u8 = 0xAF;

/// Which keymap layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// Keys pressed on their own.
    Base,
    /// Keys pressed while Fn is held.
    Fn,
}

impl Layer {
    pub fn op(self) -> Op {
        match self {
            Self::Base => Op::KeymapBase,
            Self::Fn => Op::KeymapFn,
        }
    }
}

/// HID modifier bits, as in the boot keyboard report.
pub mod mods {
    pub const LEFT_CTRL: u8 = 0x01;
    pub const LEFT_SHIFT: u8 = 0x02;
    pub const LEFT_ALT: u8 = 0x04;
    pub const LEFT_GUI: u8 = 0x08;
    pub const RIGHT_CTRL: u8 = 0x10;
    pub const RIGHT_SHIFT: u8 = 0x20;
    pub const RIGHT_ALT: u8 = 0x40;
    pub const RIGHT_GUI: u8 = 0x80;
}

/// Mouse buttons a key can send (`01 01 BTN 00`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// Side button 4 (browser back on Windows).
    Back,
    /// Side button 5 (browser forward on Windows).
    Forward,
    /// The firmware sends two left clicks.
    DoubleClick,
}

impl MouseButton {
    pub const ALL: [Self; 6] = [Self::Left, Self::Right, Self::Middle, Self::Back, Self::Forward, Self::DoubleClick];

    /// Byte value: the HID button bits, plus the vendor's 03 for a double
    /// click. Measured 2026-11-10 (fw 0x0106): 08 arrives in Windows as
    /// mouse button 4 (browser back), so the vendor table's "10 = button 4,
    /// 08 = button 5" (S1 KeyMap.cs) has the side buttons swapped.
    pub fn code(self) -> u8 {
        match self {
            Self::Left => 0x01,
            Self::Right => 0x02,
            Self::Middle => 0x04,
            Self::Back => 0x08,
            Self::Forward => 0x10,
            Self::DoubleClick => 0x03,
        }
    }

    pub fn from_code(b: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.code() == b)
    }
}

/// What one key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    /// The firmware's own function for this key (`00 00 00 00`).
    Default,
    /// The key does nothing (`05 03 00 00`).
    Disabled,
    /// A keyboard usage (page 0x07) with modifiers (`02 MODS CODE 00`).
    /// `code` 0 with modifiers set is a modifier key on its own.
    Key { mods: u8, code: u8 },
    /// A consumer-control usage such as volume up (`03 LO HI 00`).
    Consumer(u16),
    /// A mouse button (`01 01 BTN 00`).
    Mouse(MouseButton),
    /// One notch of the wheel (`01 03 01 00` up, `01 03 FF 00` down).
    Wheel { up: bool },
}

/// Keyboard usages a key may send: the standard keys of page 0x07.
/// Reserved and system usages (0xA5–0xDF, Power, …) are not accepted.
pub fn is_key_usage(code: u8) -> bool {
    matches!(code, 0x04..=0x65 | 0x68..=0x73)
}

impl KeyAction {
    /// A single key; modifier usages (E0–E7) become their modifier bit.
    pub fn key(code: u8) -> Self {
        match code {
            0xE0..=0xE7 => Self::Key { mods: 1 << (code - 0xE0), code: 0 },
            _ => Self::Key { mods: 0, code },
        }
    }

    /// The 4-byte table entry.
    pub fn encode(self) -> Result<[u8; 4], Invalid> {
        Ok(match self {
            Self::Default => [0, 0, 0, 0],
            Self::Disabled => [0x05, 0x03, 0, 0],
            Self::Key { mods, code } => {
                if code == FN_USAGE {
                    return Err(Invalid::KeyAction("Fn cannot be assigned"));
                }
                if code == 0 && mods == 0 {
                    return Err(Invalid::KeyAction("empty key"));
                }
                if code != 0 && !is_key_usage(code) {
                    return Err(Invalid::KeyAction("not a keyboard usage"));
                }
                [0x02, mods, code, 0]
            }
            Self::Consumer(usage) => {
                if usage == 0 || usage > 0x03FF {
                    return Err(Invalid::KeyAction("consumer usage out of range"));
                }
                let [lo, hi] = usage.to_le_bytes();
                [0x03, lo, hi, 0]
            }
            Self::Mouse(b) => [0x01, 0x01, b.code(), 0],
            Self::Wheel { up } => [0x01, 0x03, if up { 0x01 } else { 0xFF }, 0],
        })
    }

    /// Read an entry back; `None` for kinds this crate does not produce
    /// (macros, combos, host programs) or malformed bytes.
    pub fn decode(e: [u8; 4]) -> Option<Self> {
        let action = match e {
            [0, 0, 0, 0] => Self::Default,
            [0x05, 0x03, 0, 0] => Self::Disabled,
            [0x02, mods, code, 0] => Self::Key { mods, code },
            [0x03, lo, hi, 0] => Self::Consumer(u16::from_le_bytes([lo, hi])),
            [0x01, 0x01, b, 0] => Self::Mouse(MouseButton::from_code(b)?),
            [0x01, 0x03, 0x01, 0] => Self::Wheel { up: true },
            [0x01, 0x03, 0xFF, 0] => Self::Wheel { up: false },
            _ => return None,
        };
        action.encode().ok().map(|_| action)
    }
}

/// Encode one keymap layer. Keys not listed get `00 00 00 00` (firmware
/// default), so the result always describes the whole layer.
pub fn keymap_table(entries: impl IntoIterator<Item = (u8, KeyAction)>) -> Result<[u8; KEYMAP_LEN], Invalid> {
    let mut t = [0u8; KEYMAP_LEN];
    for (idx, action) in entries {
        if idx == 0 || idx > KEYMAP_MAX_IDX {
            return Err(Invalid::LedIndex(idx));
        }
        let i = idx as usize * 4;
        t[i..i + 4].copy_from_slice(&action.encode()?);
    }
    t[KEYMAP_LEN - 2..].copy_from_slice(&TRAILER);
    Ok(t)
}

/// One named keyboard usage, for pickers and the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub code: u8,
    /// Stable id; equals the model key id where the key exists.
    pub id: &'static str,
    /// Keycap legend (US positions).
    pub label: &'static str,
    pub group: UsageGroup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UsageGroup {
    Letter,
    Digit,
    Function,
    Editing,
    Navigation,
    Symbol,
    Keypad,
    Modifier,
}

macro_rules! usages {
    ($($code:literal $id:literal $label:literal $group:ident),* $(,)?) => {
        /// Keyboard usages offered as targets, in display order.
        pub const USAGES: &[Usage] = &[$(Usage { code: $code, id: $id, label: $label, group: UsageGroup::$group }),*];
    };
}

usages! {
    0x04 "a" "A" Letter, 0x05 "b" "B" Letter, 0x06 "c" "C" Letter, 0x07 "d" "D" Letter,
    0x08 "e" "E" Letter, 0x09 "f" "F" Letter, 0x0A "g" "G" Letter, 0x0B "h" "H" Letter,
    0x0C "i" "I" Letter, 0x0D "j" "J" Letter, 0x0E "k" "K" Letter, 0x0F "l" "L" Letter,
    0x10 "m" "M" Letter, 0x11 "n" "N" Letter, 0x12 "o" "O" Letter, 0x13 "p" "P" Letter,
    0x14 "q" "Q" Letter, 0x15 "r" "R" Letter, 0x16 "s" "S" Letter, 0x17 "t" "T" Letter,
    0x18 "u" "U" Letter, 0x19 "v" "V" Letter, 0x1A "w" "W" Letter, 0x1B "x" "X" Letter,
    0x1C "y" "Y" Letter, 0x1D "z" "Z" Letter,
    0x1E "1" "1" Digit, 0x1F "2" "2" Digit, 0x20 "3" "3" Digit, 0x21 "4" "4" Digit,
    0x22 "5" "5" Digit, 0x23 "6" "6" Digit, 0x24 "7" "7" Digit, 0x25 "8" "8" Digit,
    0x26 "9" "9" Digit, 0x27 "0" "0" Digit,
    0x3A "f1" "F1" Function, 0x3B "f2" "F2" Function, 0x3C "f3" "F3" Function, 0x3D "f4" "F4" Function,
    0x3E "f5" "F5" Function, 0x3F "f6" "F6" Function, 0x40 "f7" "F7" Function, 0x41 "f8" "F8" Function,
    0x42 "f9" "F9" Function, 0x43 "f10" "F10" Function, 0x44 "f11" "F11" Function, 0x45 "f12" "F12" Function,
    0x68 "f13" "F13" Function, 0x69 "f14" "F14" Function, 0x6A "f15" "F15" Function, 0x6B "f16" "F16" Function,
    0x6C "f17" "F17" Function, 0x6D "f18" "F18" Function, 0x6E "f19" "F19" Function, 0x6F "f20" "F20" Function,
    0x70 "f21" "F21" Function, 0x71 "f22" "F22" Function, 0x72 "f23" "F23" Function, 0x73 "f24" "F24" Function,
    0x29 "esc" "Esc" Editing, 0x2B "tab" "Tab" Editing, 0x39 "caps" "Caps Lock" Editing,
    0x28 "enter" "Enter" Editing, 0x2A "backspace" "Backspace" Editing, 0x2C "space" "Space" Editing,
    0x65 "menu" "Menu" Editing, 0x46 "print-screen" "Print Screen" Editing,
    0x47 "scroll-lock" "Scroll Lock" Editing, 0x48 "pause" "Pause" Editing,
    0x49 "insert" "Insert" Navigation, 0x4C "delete" "Delete" Navigation,
    0x4A "home" "Home" Navigation, 0x4D "end" "End" Navigation,
    0x4B "pgup" "Page Up" Navigation, 0x4E "pgdn" "Page Down" Navigation,
    0x52 "up" "↑" Navigation, 0x51 "down" "↓" Navigation, 0x50 "left" "←" Navigation, 0x4F "right" "→" Navigation,
    0x35 "grave" "`" Symbol, 0x2D "minus" "-" Symbol, 0x2E "equal" "=" Symbol,
    0x2F "lbracket" "[" Symbol, 0x30 "rbracket" "]" Symbol, 0x31 "backslash" "\\" Symbol,
    0x33 "semicolon" ";" Symbol, 0x34 "quote" "'" Symbol, 0x36 "comma" "," Symbol,
    0x37 "period" "." Symbol, 0x38 "slash" "/" Symbol,
    0x32 "iso-hash" "ISO #" Symbol, 0x64 "iso-backslash" "ISO \\" Symbol,
    0x53 "numlock" "Num Lock" Keypad, 0x54 "kp-slash" "Num /" Keypad, 0x55 "kp-star" "Num *" Keypad,
    0x56 "kp-minus" "Num −" Keypad, 0x57 "kp-plus" "Num +" Keypad, 0x58 "kp-enter" "Num Enter" Keypad,
    0x59 "kp1" "Num 1" Keypad, 0x5A "kp2" "Num 2" Keypad, 0x5B "kp3" "Num 3" Keypad,
    0x5C "kp4" "Num 4" Keypad, 0x5D "kp5" "Num 5" Keypad, 0x5E "kp6" "Num 6" Keypad,
    0x5F "kp7" "Num 7" Keypad, 0x60 "kp8" "Num 8" Keypad, 0x61 "kp9" "Num 9" Keypad,
    0x62 "kp0" "Num 0" Keypad, 0x63 "kp-dot" "Num ." Keypad,
    0xE0 "lctrl" "L Ctrl" Modifier, 0xE1 "lshift" "L Shift" Modifier,
    0xE2 "lalt" "L Alt" Modifier, 0xE3 "lwin" "L Win" Modifier,
    0xE4 "rctrl" "R Ctrl" Modifier, 0xE5 "rshift" "R Shift" Modifier,
    0xE6 "ralt" "R Alt" Modifier, 0xE7 "rwin" "R Win" Modifier,
}

pub fn usage(code: u8) -> Option<&'static Usage> {
    USAGES.iter().find(|u| u.code == code)
}

pub fn usage_by_id(id: &str) -> Option<&'static Usage> {
    USAGES.iter().find(|u| u.id.eq_ignore_ascii_case(id))
}

/// One consumer-control usage offered as a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsumerUsage {
    pub code: u16,
    pub id: &'static str,
}

/// The consumer usages the stock driver offers (S1 KeyMap.cs:398-415),
/// in display order.
pub const CONSUMER: &[ConsumerUsage] = &[
    ConsumerUsage { code: 0x00CD, id: "play-pause" },
    ConsumerUsage { code: 0x00B5, id: "next" },
    ConsumerUsage { code: 0x00B6, id: "previous" },
    ConsumerUsage { code: 0x00B7, id: "stop" },
    ConsumerUsage { code: 0x00E9, id: "volume-up" },
    ConsumerUsage { code: 0x00EA, id: "volume-down" },
    ConsumerUsage { code: 0x00E2, id: "mute" },
    ConsumerUsage { code: 0x0183, id: "media-player" },
    ConsumerUsage { code: 0x018A, id: "mail" },
    ConsumerUsage { code: 0x0192, id: "calculator" },
    ConsumerUsage { code: 0x0194, id: "my-computer" },
    ConsumerUsage { code: 0x0223, id: "browser-home" },
    ConsumerUsage { code: 0x0224, id: "browser-back" },
    ConsumerUsage { code: 0x0225, id: "browser-forward" },
    ConsumerUsage { code: 0x0226, id: "browser-stop" },
    ConsumerUsage { code: 0x0227, id: "browser-refresh" },
    ConsumerUsage { code: 0x022A, id: "browser-favorites" },
    ConsumerUsage { code: 0x0221, id: "browser-search" },
];

pub fn consumer_by_id(id: &str) -> Option<&'static ConsumerUsage> {
    CONSUMER.iter().find(|c| c.id.eq_ignore_ascii_case(id))
}

#[cfg(test)]
mod tests {
    use super::super::{guard, PacketKind};
    use super::*;

    #[test]
    fn entries_match_the_documented_encoding() {
        // docs/research/protokol-ff13.md §6.2
        let ctrl_shift_a = KeyAction::Key { mods: mods::LEFT_CTRL | mods::LEFT_SHIFT, code: 0x04 };
        assert_eq!(ctrl_shift_a.encode().unwrap(), [0x02, 0x03, 0x04, 0x00]);
        let win_l = KeyAction::Key { mods: mods::LEFT_GUI, code: 0x0F };
        assert_eq!(win_l.encode().unwrap(), [0x02, 0x08, 0x0F, 0x00]);
        assert_eq!(KeyAction::key(0xE0).encode().unwrap(), [0x02, 0x01, 0x00, 0x00], "Ctrl on its own");
        assert_eq!(KeyAction::key(0xE7), KeyAction::Key { mods: mods::RIGHT_GUI, code: 0 });
        assert_eq!(KeyAction::Default.encode().unwrap(), [0; 4]);
        assert_eq!(KeyAction::Disabled.encode().unwrap(), [0x05, 0x03, 0, 0]);
        assert_eq!(KeyAction::Consumer(0x0192).encode().unwrap(), [0x03, 0x92, 0x01, 0x00]);
        assert_eq!(KeyAction::Consumer(0xE9).encode().unwrap(), [0x03, 0xE9, 0x00, 0x00]);
        assert_eq!(KeyAction::Mouse(MouseButton::Middle).encode().unwrap(), [0x01, 0x01, 0x04, 0x00]);
        assert_eq!(KeyAction::Wheel { up: false }.encode().unwrap(), [0x01, 0x03, 0xFF, 0x00]);
    }

    #[test]
    fn invalid_actions_are_rejected() {
        assert!(KeyAction::key(FN_USAGE).encode().is_err(), "Fn is never a target");
        assert!(KeyAction::Key { mods: 0, code: 0 }.encode().is_err());
        assert!(KeyAction::Key { mods: 0, code: 0xAA }.encode().is_err(), "reserved usage");
        assert!(KeyAction::Key { mods: 0, code: 0x66 }.encode().is_err(), "Power is not offered");
        assert!(KeyAction::Consumer(0).encode().is_err());
    }

    #[test]
    fn decode_round_trips() {
        let all = [
            KeyAction::Default,
            KeyAction::Disabled,
            KeyAction::key(0x68),
            KeyAction::Key { mods: mods::LEFT_CTRL, code: 0x06 },
            KeyAction::key(0xE5),
            KeyAction::Consumer(0x00CD),
            KeyAction::Wheel { up: true },
            KeyAction::Wheel { up: false },
        ]
        .into_iter()
        .chain(MouseButton::ALL.map(KeyAction::Mouse));
        for a in all {
            assert_eq!(KeyAction::decode(a.encode().unwrap()), Some(a), "{a:?}");
        }
        assert_eq!(KeyAction::decode([0x06, 0x02, 0x01, 0x05]), None, "macros are not decoded yet");
        assert_eq!(KeyAction::decode([0x02, 0x00, FN_USAGE, 0x00]), None);
    }

    #[test]
    fn table_layout() {
        let t = keymap_table([(33, KeyAction::key(0x04)), (34, KeyAction::Consumer(0xE9))]).unwrap();
        assert_eq!(t.len(), 576);
        assert_eq!(&t[132..136], &[0x02, 0x00, 0x04, 0x00]);
        assert_eq!(&t[136..140], &[0x03, 0xE9, 0x00, 0x00]);
        assert_eq!(&t[574..], &[0xAA, 0x55]);
        // Everything else is the firmware default.
        let set: usize = t.iter().filter(|&&b| b != 0).count();
        assert_eq!(set, 2 + 2 + 2);
        assert_eq!(keymap_table([]).unwrap()[..574], [0u8; 574], "reset = all default");
        assert_eq!(keymap_table([(0, KeyAction::Disabled)]), Err(Invalid::LedIndex(0)));
        assert_eq!(keymap_table([(143, KeyAction::Disabled)]), Err(Invalid::LedIndex(143)));
    }

    /// No valid entry can start a data packet with a bootloader or ISP
    /// pattern, so `guard` never blocks a keymap the encoder accepts.
    #[test]
    fn packets_pass_the_guard() {
        let mut actions: Vec<KeyAction> = USAGES.iter().map(|u| KeyAction::key(u.code)).collect();
        actions.extend(CONSUMER.iter().map(|c| KeyAction::Consumer(c.code)));
        actions.extend(MouseButton::ALL.map(KeyAction::Mouse));
        actions.extend([KeyAction::Disabled, KeyAction::Wheel { up: true }, KeyAction::Wheel { up: false }]);
        for a in actions {
            // Packets start at entries 0, 16, 32, …: put the action there.
            let t = keymap_table((1..=8).map(|p| (p * 16, a))).unwrap();
            for chunk in t.chunks_exact(PAYLOAD) {
                assert!(guard(chunk, PacketKind::Data).is_ok(), "{a:?}");
            }
        }
    }

    #[test]
    fn catalogue_is_consistent() {
        let mut ids = std::collections::HashSet::new();
        let mut codes = std::collections::HashSet::new();
        for u in USAGES {
            assert!(ids.insert(u.id), "duplicate id {}", u.id);
            assert!(codes.insert(u.code), "duplicate code {:02X}", u.code);
            assert!(KeyAction::key(u.code).encode().is_ok(), "{}", u.id);
        }
        for c in CONSUMER {
            assert!(ids.insert(c.id), "duplicate id {}", c.id);
        }
        assert_eq!(usage_by_id("F13").map(|u| u.code), Some(0x68));
        assert_eq!(consumer_by_id("volume-up").map(|c| c.code), Some(0xE9));
    }
}
