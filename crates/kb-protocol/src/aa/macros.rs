//! Macros (§4.8): `15` read, `25` write.
//!
//! * Addresses 0–399: a table of 100 little-endian u32 absolute addresses,
//!   one per macro slot (0 = no macro).
//! * From address 400: one block per macro, `[n_lo n_hi 00 00]` (n = twice
//!   the number of entries) and 4-byte entries `[delay_lo delay_hi code
//!   flag]`: wait `delay` ms after the entry; `flag` bit 7 is press (1) or
//!   release (0), `B0`/`30` a key, `90`/`10` a mouse button.
//!
//! A write sends the table first with LAST = 0 on every chunk, then the
//! blocks from address 400 with LAST on the final chunk (AJAZZ's WebHID
//! driver). Macro names are not stored on the keyboard.

use super::CodecError;
use crate::sonix::keymap::is_key_usage;
use crate::sonix::macros::{MacroStep, StepKind};

/// Bytes in the address table.
pub const TABLE_LEN: usize = 400;
/// Macro slots (keymap entries `06 SLOT …` name them).
pub const MAX_MACROS: usize = 100;
/// Most macros stored at once (AJAZZ's WebHID driver refuses a 100th).
pub const MAX_STORED: usize = 99;
/// Address of the first block.
pub const BLOCKS_AT: u16 = 400;
/// Bytes of blocks the keyboard takes: AJAZZ's WebHID driver keeps this
/// keyboard to 512 (the table not counted), although firmware 769 reports
/// 3072 bytes of macro memory. Raised only once a real board shows more
/// works.
pub const CAPACITY: usize = 512;
/// Shortest pause after an entry, except the last.
pub const MIN_DELAY_MS: u16 = 5;

/// Entry flags.
pub mod flag {
    pub const PRESS: u8 = 0x80;
    pub const KEY_DOWN: u8 = 0xB0;
    pub const KEY_UP: u8 = 0x30;
    pub const MOUSE_DOWN: u8 = 0x90;
    pub const MOUSE_UP: u8 = 0x10;
}

/// Mouse buttons a macro entry can press (1 left, 2 right, 4 middle, 8
/// back, 16 forward).
pub const MOUSE_BUTTONS: [u8; 5] = [0x01, 0x02, 0x04, 0x08, 0x10];

/// One stored entry, as the keyboard keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AaMacroEntry {
    /// Pause after the entry, in milliseconds.
    pub delay_ms: u16,
    /// Keyboard usage or mouse button mask.
    pub code: u8,
    pub flag: u8,
}

impl AaMacroEntry {
    pub fn bytes(self) -> [u8; 4] {
        let [lo, hi] = self.delay_ms.to_le_bytes();
        [lo, hi, self.code, self.flag]
    }

    pub fn from_bytes(e: [u8; 4]) -> Self {
        Self { delay_ms: u16::from_le_bytes([e[0], e[1]]), code: e[2], flag: e[3] }
    }
}

/// The app's steps as entries. Pauses shorter than [`MIN_DELAY_MS`] are
/// raised to it, except after the last step.
pub fn from_steps(steps: &[MacroStep]) -> Result<Vec<AaMacroEntry>, CodecError> {
    let last = steps.len().saturating_sub(1);
    steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let ok = if s.kind.is_mouse() {
                MOUSE_BUTTONS.contains(&s.code)
            } else {
                is_key_usage(s.code) || (0xE0..=0xE7).contains(&s.code)
            };
            if !ok {
                return Err(CodecError::Invalid("macro step: unknown key or button"));
            }
            let flag = match s.kind {
                StepKind::KeyDown => flag::KEY_DOWN,
                StepKind::KeyUp => flag::KEY_UP,
                StepKind::MouseDown => flag::MOUSE_DOWN,
                StepKind::MouseUp => flag::MOUSE_UP,
            };
            let delay_ms = if i == last { s.delay_ms } else { s.delay_ms.max(MIN_DELAY_MS) };
            Ok(AaMacroEntry { delay_ms, code: s.code, flag })
        })
        .collect()
}

/// Entries as the app's steps; `None` when one has no step kind (a wheel
/// turn, which AJAZZ writes as mouse button 255, or an unknown flag).
pub fn to_steps(entries: &[AaMacroEntry]) -> Option<Vec<MacroStep>> {
    entries
        .iter()
        .map(|e| {
            let kind = match e.flag {
                flag::KEY_DOWN => StepKind::KeyDown,
                flag::KEY_UP => StepKind::KeyUp,
                flag::MOUSE_DOWN => StepKind::MouseDown,
                flag::MOUSE_UP => StepKind::MouseUp,
                _ => return None,
            };
            if kind.is_mouse() && !MOUSE_BUTTONS.contains(&e.code) {
                return None;
            }
            Some(MacroStep::new(kind, e.code, e.delay_ms))
        })
        .collect()
}

/// What a write sends: the 400-byte table and the blocks from address 400.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroImage {
    pub table: Vec<u8>,
    pub blocks: Vec<u8>,
}

/// Bytes `macros` macros with `entries` entries in total take (headers and
/// entries; the table is not counted).
pub fn size(macros: usize, entries: usize) -> usize {
    4 * macros + 4 * entries
}

/// Encode every macro: `slots[i]` is macro `i`, an empty one is no macro.
pub fn encode(slots: &[Vec<AaMacroEntry>]) -> Result<MacroImage, CodecError> {
    if slots.len() > MAX_MACROS {
        return Err(CodecError::Invalid("more than 100 macro slots"));
    }
    let macros = slots.iter().filter(|s| !s.is_empty()).count();
    let entries: usize = slots.iter().map(Vec::len).sum();
    let bytes = size(macros, entries);
    if macros > MAX_STORED || bytes > CAPACITY {
        return Err(CodecError::TooLarge { macros, entries, bytes, limit: CAPACITY });
    }
    let mut table = vec![0u8; TABLE_LEN];
    let mut blocks = Vec::with_capacity(bytes);
    for (slot, list) in slots.iter().enumerate() {
        if list.is_empty() {
            continue;
        }
        let addr = u32::from(BLOCKS_AT) + blocks.len() as u32;
        table[slot * 4..slot * 4 + 4].copy_from_slice(&addr.to_le_bytes());
        let n = (list.len() * 2) as u16;
        blocks.extend_from_slice(&n.to_le_bytes());
        blocks.extend_from_slice(&[0, 0]);
        for e in list {
            blocks.extend_from_slice(&e.bytes());
        }
    }
    Ok(MacroImage { table, blocks })
}

/// Whether a table entry names no macro: 0, or erased flash (`FF FF FF FF`).
pub fn is_none(pointer: u32) -> bool {
    pointer == 0 || pointer == u32::MAX
}

/// The address of every slot's block (0 = none; see [`is_none`]).
pub fn pointers(table: &[u8]) -> Vec<u32> {
    table.chunks_exact(4).take(MAX_MACROS).map(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]])).collect()
}

/// Entries in a block, from its 4-byte header.
pub fn header_entries(header: &[u8]) -> usize {
    usize::from(u16::from_le_bytes([header[0], header[1]])) / 2
}

/// The entries of a block body.
pub fn decode_entries(body: &[u8]) -> Vec<AaMacroEntry> {
    body.chunks_exact(4).map(|e| AaMacroEntry::from_bytes([e[0], e[1], e[2], e[3]])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(code: u8, delay: u16) -> [MacroStep; 2] {
        [MacroStep::new(StepKind::KeyDown, code, delay), MacroStep::new(StepKind::KeyUp, code, delay)]
    }

    #[test]
    fn the_a_macro_matches_the_hardware_capture() {
        // S12 §9.7 (AK980 MAX, read back on hardware): "a" pressed and
        // released, 10 ms, the last pause 0: pointer 400, header `04 00 00
        // 00`, entries `0A 00 04 B0` and `00 00 04 30`.
        let steps = [MacroStep::new(StepKind::KeyDown, 0x04, 10), MacroStep::new(StepKind::KeyUp, 0x04, 0)];
        let img = encode(&[from_steps(&steps).unwrap()]).unwrap();
        assert_eq!(&img.table[..4], &[0x90, 0x01, 0x00, 0x00]);
        assert!(img.table[4..].iter().all(|&b| b == 0));
        assert_eq!(img.blocks, [0x04, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x04, 0xB0, 0x00, 0x00, 0x04, 0x30]);
        assert_eq!(pointers(&img.table)[0], 400);
        assert_eq!(header_entries(&img.blocks[..4]), 2);
        assert!(is_none(0) && is_none(u32::MAX) && !is_none(400));
        let back = decode_entries(&img.blocks[4..]);
        assert_eq!(to_steps(&back).unwrap(), steps);
    }

    #[test]
    fn layout_of_several_macros() {
        let a = from_steps(&tap(0x04, 20)).unwrap();
        let b = from_steps(&[MacroStep::new(StepKind::MouseDown, 0x10, 1)]).unwrap();
        let img = encode(&[a.clone(), vec![], b.clone()]).unwrap();
        let p = pointers(&img.table);
        assert_eq!((p[0], p[1], p[2]), (400, 0, 412));
        assert_eq!(&img.blocks[12..20], &[0x02, 0, 0, 0, 0x01, 0x00, 0x10, 0x90], "the last pause stays 1 ms");
        assert_eq!(img.blocks.len(), size(2, 3));
        // Every block decodes back to its entries through the table.
        for (slot, want) in [(0usize, &a), (2, &b)] {
            let at = (p[slot] - u32::from(BLOCKS_AT)) as usize;
            let n = header_entries(&img.blocks[at..]);
            assert_eq!(&decode_entries(&img.blocks[at + 4..at + 4 + 4 * n]), want);
        }
        // No macros: an empty table and no blocks.
        let none = encode(&[]).unwrap();
        assert_eq!((none.table, none.blocks.len()), (vec![0u8; 400], 0));
    }

    #[test]
    fn pauses_are_raised_but_the_last_one() {
        let steps = [
            MacroStep::new(StepKind::KeyDown, 0xE1, 0),
            MacroStep::new(StepKind::KeyDown, 0x04, 4),
            MacroStep::new(StepKind::KeyUp, 0x04, 65535),
            MacroStep::new(StepKind::KeyUp, 0xE1, 2),
        ];
        let e = from_steps(&steps).unwrap();
        let delays: Vec<u16> = e.iter().map(|e| e.delay_ms).collect();
        assert_eq!(delays, [5, 5, 65535, 2]);
        assert_eq!(e[0].bytes(), [5, 0, 0xE1, 0xB0]);
        assert_eq!(e[2].bytes(), [0xFF, 0xFF, 0x04, 0x30]);
    }

    #[test]
    fn steps_the_keyboard_cannot_store_are_refused() {
        assert!(from_steps(&[MacroStep::new(StepKind::KeyDown, 0xAF, 10)]).is_err(), "Fn");
        assert!(from_steps(&[MacroStep::new(StepKind::MouseDown, 0x03, 10)]).is_err(), "two buttons");
        assert!(from_steps(&[MacroStep::new(StepKind::MouseDown, 0x08, 10)]).is_ok(), "back");
        // A wheel turn (button 255) or an unknown flag has no step kind.
        let wheel = [AaMacroEntry { delay_ms: 10, code: 0xFF, flag: flag::MOUSE_DOWN }];
        assert_eq!(to_steps(&wheel), None);
        assert_eq!(to_steps(&[AaMacroEntry { delay_ms: 10, code: 4, flag: 0xA0 }]), None);
    }

    #[test]
    fn capacity_edges() {
        let entry = AaMacroEntry { delay_ms: 10, code: 4, flag: flag::KEY_DOWN };
        // One macro of 127 entries: 4 + 508 = 512 bytes fit.
        assert!(encode(&[vec![entry; 127]]).is_ok());
        assert_eq!(
            encode(&[vec![entry; 128]]),
            Err(CodecError::TooLarge { macros: 1, entries: 128, bytes: 516, limit: 512 })
        );
        // 64 one-entry macros fit exactly; a 65th does not.
        assert!(encode(&vec![vec![entry]; 64]).is_ok());
        assert!(matches!(encode(&vec![vec![entry]; 65]), Err(CodecError::TooLarge { macros: 65, .. })));
        // More than 100 slots is not a size question.
        assert!(matches!(encode(&vec![vec![]; 101]), Err(CodecError::Invalid(_))));
        assert!(encode(&vec![vec![]; 100]).is_ok());
    }
}
