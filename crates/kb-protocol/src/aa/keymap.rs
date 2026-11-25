//! Keymap tables (§4.7): `12`/`22` base layer, `16`/`26` Fn layer, `1C`
//! the factory Fn layer (read only).
//!
//! A table is 512 bytes, 128 slots of `[page p1 p2 p3]`; slot `i` is the key
//! with "AA" index `i` (the model file's `aa`, not its LED number). Writes
//! always send the whole table, so the driver builds it from a fresh read
//! and changes only the slots it was asked to ([`super::guard::guard_keymap`]
//! checks the result).

use super::CodecError;
use crate::sonix::keymap::{is_key_usage, KeyAction, MacroPlay, MouseButton, FN_USAGE};

/// Bytes in one table.
pub const LEN: usize = 512;
/// Slots in one table.
pub const SLOTS: usize = 128;
/// The last two slots: some boards keep `AA 55` in bytes 510–511 (NUT87),
/// so they are always written back as read. Firmware 769's own tables have
/// no such tail and no key there (slots 111–127 reach no key); keeping
/// them as read costs nothing. (The per-key table has no such rule.)
pub const TAIL_SLOTS: [u8; 2] = [126, 127];
/// An entry of erased flash. A keymap write that stopped part-way leaves
/// every entry it did not reach so (the page was erased by its first
/// chunk); written as `00 00 00 00` it gets the layer's factory entry back,
/// which firmware 769 fills in from its defaults (protocol notes §F.2).
pub const ERASED: [u8; 4] = [0xFF; 4];
/// "The key does nothing": a keyboard entry without a usage. Inferred from
/// the vendor's encoding (no vendor tool writes it); to be confirmed by
/// read-back on a real board.
pub const DISABLED: [u8; 4] = [0x02, 0x00, 0x00, 0x00];

/// Entry pages (first byte).
pub mod page {
    pub const DEFAULT: u8 = 0x00;
    pub const MOUSE: u8 = 0x01;
    pub const KEYBOARD: u8 = 0x02;
    pub const CONSUMER: u8 = 0x03;
    pub const SYSTEM: u8 = 0x04;
    /// Run by AJAZZ's host program, not the firmware: never written.
    pub const EXTRA_FUNCTION: u8 = 0x05;
    pub const MACRO: u8 = 0x06;
    pub const COMBO: u8 = 0x07;
    /// A firmware function, id in p1–p3 (big-endian).
    pub const FUNC: u8 = 0x0D;
    /// Pages from 0x80 up: two firmware functions (frameVersion 1 boards).
    pub const FUNC_V2: u8 = 0x80;
}

/// Pages the AK980 PRO's tables are expected to hold (§4.7): anything else
/// on a fresh read means the board is not what this driver knows.
pub fn is_known_page(p: u8) -> bool {
    matches!(
        p,
        page::DEFAULT
            | page::MOUSE
            | page::KEYBOARD
            | page::CONSUMER
            | page::EXTRA_FUNCTION
            | page::MACRO
            | page::COMBO
            | page::FUNC
    ) || p >= page::FUNC_V2
}

/// Firmware functions that reset the keyboard, enter a test or calibration
/// mode, switch USB / wireless or NKRO, or lock the driver or macros out
/// (AJAZZ's WebHID driver lists them as dangerous). Never written to a slot
/// that did not already hold them.
pub const DANGEROUS_FUNC: [u32; 23] =
    [1, 182, 4096, 113, 205, 38, 59, 60, 101, 108, 174, 221, 153, 103, 292, 49, 68, 82, 276, 187, 255, 256, 45];

/// Whether an entry is a firmware function (FUNC or FUNC_V2).
pub fn is_func(e: [u8; 4]) -> bool {
    e[0] == page::FUNC || e[0] >= page::FUNC_V2
}

/// The firmware function ids an entry names (none unless it is FUNC or
/// FUNC_V2; zero ids are left out).
pub fn func_ids(e: [u8; 4]) -> Vec<u32> {
    let ids = match e[0] {
        page::FUNC => vec![u32::from_be_bytes([0, e[1], e[2], e[3]])],
        p if p >= page::FUNC_V2 => {
            vec![(u32::from(p & 0x7F) << 8) | u32::from(e[1]), (u32::from(e[2]) << 8) | u32::from(e[3])]
        }
        _ => vec![],
    };
    ids.into_iter().filter(|&id| id != 0).collect()
}

/// One slot, as the app sees it: an action this crate encodes, or bytes it
/// does not (a firmware function, a combo, a host program...), which are
/// kept as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Binding {
    Action(KeyAction),
    Raw([u8; 4]),
}

impl Binding {
    /// The slot's bytes.
    pub fn bytes(self) -> Result<[u8; 4], CodecError> {
        match self {
            Self::Action(a) => encode(a),
            Self::Raw(e) => Ok(e),
        }
    }
}

fn mouse_mask(b: MouseButton) -> Result<u8, CodecError> {
    Ok(match b {
        MouseButton::Left => 0x01,
        MouseButton::Right => 0x02,
        MouseButton::Middle => 0x04,
        MouseButton::Back => 0x08,
        MouseButton::Forward => 0x10,
        MouseButton::DoubleClick => return Err(CodecError::Unsupported("this keyboard has no double-click key")),
    })
}

fn mouse_button(mask: u8) -> Option<MouseButton> {
    Some(match mask {
        0x01 => MouseButton::Left,
        0x02 => MouseButton::Right,
        0x04 => MouseButton::Middle,
        0x08 => MouseButton::Back,
        0x10 => MouseButton::Forward,
        _ => return None,
    })
}

/// The 4-byte entry for `action`.
pub fn encode(action: KeyAction) -> Result<[u8; 4], CodecError> {
    Ok(match action {
        KeyAction::Default => [0; 4],
        KeyAction::Disabled => DISABLED,
        KeyAction::Key { mods, code } => {
            if code == FN_USAGE {
                return Err(CodecError::Invalid("key: Fn cannot be assigned"));
            }
            let modifier = (0xE0..=0xE7).contains(&code);
            match (mods, code) {
                (0, 0) => return Err(CodecError::Invalid("key: empty")),
                (0, c) if is_key_usage(c) || modifier => [page::KEYBOARD, 0, c, 0],
                // A modifier on its own is its usage, like the vendor writes it.
                (m, 0) if m.count_ones() == 1 => [page::KEYBOARD, 0, 0xE0 + m.trailing_zeros() as u8, 0],
                (_, 0) => return Err(CodecError::Unsupported("several modifiers without a key")),
                // A shortcut: modifier bits and the key, in one entry.
                (m, c) if is_key_usage(c) => [page::KEYBOARD, m, c, 0],
                _ => return Err(CodecError::Invalid("key: not a keyboard usage")),
            }
        }
        KeyAction::Consumer(usage) => {
            if usage == 0 || usage > 0x03FF {
                return Err(CodecError::Invalid("consumer usage"));
            }
            let [lo, hi] = usage.to_le_bytes();
            [page::CONSUMER, lo, hi, 0]
        }
        KeyAction::Mouse(b) => [page::MOUSE, 0x01, mouse_mask(b)?, 0],
        KeyAction::Wheel { up } => [page::MOUSE, 0x03, if up { 0x01 } else { 0xFF }, 0],
        KeyAction::Macro { slot, play } => {
            if usize::from(slot) >= super::macros::MAX_MACROS {
                return Err(CodecError::Invalid("macro slot"));
            }
            let (mode, count) = match play {
                MacroPlay::Once => (0, 0),
                MacroPlay::Times(0) => return Err(CodecError::Invalid("macro repeat count 0")),
                MacroPlay::Times(n) => (1, n),
                MacroPlay::Toggle => (2, 0),
                // Firmware 769 has play mode 3, "repeat while the key is
                // held" (§F.8: 0 once, 1 N times, 2 until pressed again, 3
                // repeat while held, 4 step by step). Not offered yet.
                MacroPlay::WhileHeld => {
                    return Err(CodecError::Unsupported("a macro cannot repeat while the key is held"));
                }
            };
            [page::MACRO, slot, mode, count]
        }
    })
}

/// Read one slot. An entry decodes to an action only when encoding that
/// action gives back the same bytes; everything else stays [`Binding::Raw`]
/// and is written back untouched.
pub fn decode(e: [u8; 4]) -> Binding {
    let action = match e {
        [0, 0, 0, 0] => Some(KeyAction::Default),
        DISABLED => Some(KeyAction::Disabled),
        [page::KEYBOARD, 0, c @ 0xE0..=0xE7, 0] => Some(KeyAction::key(c)),
        [page::KEYBOARD, mods, code, 0] => Some(KeyAction::Key { mods, code }),
        [page::CONSUMER, lo, hi, 0] => Some(KeyAction::Consumer(u16::from_le_bytes([lo, hi]))),
        [page::MOUSE, 0x01, b, 0] => mouse_button(b).map(KeyAction::Mouse),
        [page::MOUSE, 0x03, 0x01, 0] => Some(KeyAction::Wheel { up: true }),
        [page::MOUSE, 0x03, 0xFF, 0] => Some(KeyAction::Wheel { up: false }),
        [page::MACRO, slot, 0, 0] => Some(KeyAction::Macro { slot, play: MacroPlay::Once }),
        [page::MACRO, slot, 1, n] => Some(KeyAction::Macro { slot, play: MacroPlay::Times(n) }),
        [page::MACRO, slot, 2, 0] => Some(KeyAction::Macro { slot, play: MacroPlay::Toggle }),
        _ => None,
    };
    match action {
        Some(a) if encode(a) == Ok(e) => Binding::Action(a),
        _ => Binding::Raw(e),
    }
}

/// Slot `slot` of a table.
pub fn entry(table: &[u8], slot: u8) -> [u8; 4] {
    let at = usize::from(slot) * 4;
    [table[at], table[at + 1], table[at + 2], table[at + 3]]
}

/// Replace slot `slot` of a table.
pub fn set_entry(table: &mut [u8], slot: u8, e: [u8; 4]) {
    let at = usize::from(slot) * 4;
    table[at..at + 4].copy_from_slice(&e);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sonix::keymap::{mods, CONSUMER, USAGES};

    #[test]
    fn encodings_follow_the_vendor() {
        assert_eq!(encode(KeyAction::Default), Ok([0, 0, 0, 0]));
        assert_eq!(encode(KeyAction::key(0x04)), Ok([0x02, 0x00, 0x04, 0x00]));
        // A shortcut fits one key: modifier bits in p1 (the web hub's form).
        let ctrl_shift_a = KeyAction::Key { mods: mods::LEFT_CTRL | mods::LEFT_SHIFT, code: 0x04 };
        assert_eq!(encode(ctrl_shift_a), Ok([0x02, 0x03, 0x04, 0x00]));
        // A modifier alone is its usage (the WebHID driver's form).
        assert_eq!(encode(KeyAction::key(0xE0)), Ok([0x02, 0x00, 0xE0, 0x00]));
        assert_eq!(encode(KeyAction::key(0xE7)), Ok([0x02, 0x00, 0xE7, 0x00]));
        assert_eq!(encode(KeyAction::Key { mods: 0, code: 0xE1 }), Ok([0x02, 0x00, 0xE1, 0x00]));
        assert_eq!(encode(KeyAction::Consumer(0x00E9)), Ok([0x03, 0xE9, 0x00, 0x00]));
        assert_eq!(encode(KeyAction::Consumer(0x0192)), Ok([0x03, 0x92, 0x01, 0x00]));
        let mouse: Vec<_> = [MouseButton::Left, MouseButton::Right, MouseButton::Middle, MouseButton::Back]
            .into_iter()
            .chain([MouseButton::Forward])
            .map(|b| encode(KeyAction::Mouse(b)).unwrap())
            .collect();
        assert_eq!(mouse, [[1, 1, 1, 0], [1, 1, 2, 0], [1, 1, 4, 0], [1, 1, 8, 0], [1, 1, 16, 0]]);
        assert_eq!(encode(KeyAction::Wheel { up: true }), Ok([0x01, 0x03, 0x01, 0x00]));
        assert_eq!(encode(KeyAction::Wheel { up: false }), Ok([0x01, 0x03, 0xFF, 0x00]));
        let m = |slot, play| encode(KeyAction::Macro { slot, play }).unwrap();
        assert_eq!(m(0, MacroPlay::Once), [0x06, 0, 0, 0]);
        assert_eq!(m(2, MacroPlay::Times(5)), [0x06, 2, 1, 5]);
        assert_eq!(m(99, MacroPlay::Toggle), [0x06, 99, 2, 0]);
        assert_eq!(encode(KeyAction::Disabled), Ok([0x02, 0x00, 0x00, 0x00]));
    }

    #[test]
    fn what_the_keyboard_cannot_do_is_refused() {
        assert!(matches!(encode(KeyAction::Mouse(MouseButton::DoubleClick)), Err(CodecError::Unsupported(_))));
        let held = KeyAction::Macro { slot: 0, play: MacroPlay::WhileHeld };
        assert!(matches!(encode(held), Err(CodecError::Unsupported(_))));
        let two_mods = KeyAction::Key { mods: mods::LEFT_CTRL | mods::LEFT_ALT, code: 0 };
        assert!(matches!(encode(two_mods), Err(CodecError::Unsupported(_))));
        for bad in [
            KeyAction::key(FN_USAGE),
            KeyAction::Key { mods: 0, code: 0 },
            KeyAction::Key { mods: 0, code: 0x66 },
            KeyAction::Key { mods: mods::LEFT_CTRL, code: 0xE1 },
            KeyAction::Consumer(0),
            KeyAction::Consumer(0x0400),
            KeyAction::Macro { slot: 100, play: MacroPlay::Once },
            KeyAction::Macro { slot: 0, play: MacroPlay::Times(0) },
        ] {
            assert!(matches!(encode(bad), Err(CodecError::Invalid(_))), "{bad:?}");
        }
    }

    #[test]
    fn never_a_host_program_or_the_ff13_disable_code() {
        // Every action this crate can encode lands on pages 0, 1, 2, 3 or 6.
        let mut all: Vec<KeyAction> = USAGES.iter().map(|u| KeyAction::key(u.code)).collect();
        all.extend(CONSUMER.iter().map(|c| KeyAction::Consumer(c.code)));
        all.extend(MouseButton::ALL.map(KeyAction::Mouse));
        all.extend([KeyAction::Default, KeyAction::Disabled, KeyAction::Wheel { up: true }]);
        all.extend((0..100).map(|slot| KeyAction::Macro { slot, play: MacroPlay::Times(3) }));
        for a in all {
            let Ok(e) = encode(a) else { continue };
            assert!(matches!(e[0], 0 | 1 | 2 | 3 | 6), "{a:?}");
            assert_ne!(e, [0x05, 0x03, 0x00, 0x00]);
        }
    }

    #[test]
    fn decode_round_trips_and_keeps_the_rest_raw() {
        let mut actions: Vec<KeyAction> = USAGES.iter().map(|u| KeyAction::key(u.code)).collect();
        actions.extend(CONSUMER.iter().map(|c| KeyAction::Consumer(c.code)));
        actions.extend([
            KeyAction::Default,
            KeyAction::Disabled,
            KeyAction::Key { mods: mods::RIGHT_ALT | mods::LEFT_SHIFT, code: 0x1E },
            KeyAction::Mouse(MouseButton::Forward),
            KeyAction::Wheel { up: false },
            KeyAction::Macro { slot: 7, play: MacroPlay::Times(255) },
            KeyAction::Macro { slot: 7, play: MacroPlay::Toggle },
        ]);
        for a in actions {
            let e = encode(a).unwrap();
            assert_eq!(decode(e), Binding::Action(a), "{a:?}");
            assert_eq!(decode(e).bytes(), Ok(e));
        }
        // Bytes the encoder would write differently, or not at all, stay raw.
        for raw in [
            [0x0D, 0, 0, 1],           // FUNC 1
            [0x05, 0x03, 0, 0],        // host program / "04 xx" disable
            [0x02, 0x01, 0x00, 0x00],  // modifier bits without a usage
            [0x02, 0x00, FN_USAGE, 0], // the Fn usage
            [0x01, 0x01, 0x03, 0x00],  // two buttons
            [0x06, 0x01, 0x03, 0x00],  // play while held
            [0x00, 0x01, 0x00, 0x00],  // default with parameters
            [0x07, 0x04, 0x05, 0x06],  // combo
            [0x90, 0x00, 0x00, 0x00],  // FUNC_V2
        ] {
            assert_eq!(decode(raw), Binding::Raw(raw), "{raw:02X?}");
            assert_eq!(decode(raw).bytes(), Ok(raw));
        }
    }

    #[test]
    fn function_ids_and_pages() {
        assert_eq!(func_ids([0x0D, 0, 0, 1]), [1]);
        assert_eq!(func_ids([0x0D, 0x01, 0x00, 0x24]), [65572]);
        assert_eq!(func_ids([0x90, 0x00, 0x00, 0x00]), [4096], "FUNC_V2 factory reset");
        assert_eq!(func_ids([0x80, 0x00, 0x20, 0x00]), [8192]);
        assert!(func_ids([0x02, 0, 4, 0]).is_empty());
        assert!(is_func([0x0D, 0, 0, 0]) && is_func([0x80, 0, 0, 0]) && !is_func([0x06, 0, 0, 0]));
        for p in [0u8, 1, 2, 3, 5, 6, 7, 13, 0x80, 0xFF] {
            assert!(is_known_page(p), "{p}");
        }
        for p in [4u8, 8, 9, 12, 14, 15, 0x7F] {
            assert!(!is_known_page(p), "{p}");
        }
        let mut t = [0u8; LEN];
        set_entry(&mut t, 76, [2, 0, 0x28, 0]);
        assert_eq!(&t[304..308], &[2, 0, 0x28, 0]);
        assert_eq!(entry(&t, 76), [2, 0, 0x28, 0]);
    }
}
