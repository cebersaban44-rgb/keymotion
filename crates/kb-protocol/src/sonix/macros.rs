//! Macro storage (`04 19` → `04 15`).
//!
//! The keyboard keeps every macro in one image, written as a whole
//! (docs site: developers/protocol; S1 Macro.cs, S2 macros.rs):
//!
//! * a directory of 100 × 4 bytes: slot `i` holds `OFS_LO OFS_HI 00 00`,
//!   the absolute offset of its record; `FF FF FF FF` marks a slot without
//!   steps below the last used one, and `00 00 00 00` follows the last;
//! * from byte 400, one record per used slot: `[event count LE16][6 × 00]`
//!   then the events. Every step is two 4-byte events: the action
//!   `00 00 CODE TYPE` and the pause after it `MS_LO MS_HI 00 50`.
//!
//! The image is sent as whole 64-byte pages with `AA 55` in the last two
//! bytes. Keys play a macro through a keymap entry `06 SLOT MODE COUNT`
//! ([`super::keymap::KeyAction::Macro`]); slots keep their number, so a
//! macro can be removed without rewriting the keymap.

use super::keymap::is_key_usage;
use super::{Invalid, PAYLOAD, TRAILER};

/// Macro slots the keyboard offers.
pub const MAX_MACROS: usize = 100;
/// Directory size; records start here.
pub const DIRECTORY_LEN: usize = MAX_MACROS * 4;
/// Shortest pause after a step the firmware accepts.
pub const MIN_DELAY_MS: u16 = 10;
/// Vendor memory limit: `(steps + slots) × 8` bytes (S1 Macro.cs:125-139).
pub const BUDGET: usize = 0xA8C;

const EMPTY_SLOT: [u8; 4] = [0xFF; 4];
const DELAY_TYPE: u8 = 0x50;

/// Mouse button masks a macro step can press.
pub mod mouse {
    pub const LEFT: u8 = 0x01;
    pub const RIGHT: u8 = 0x02;
    pub const MIDDLE: u8 = 0x04;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepKind {
    KeyDown,
    KeyUp,
    MouseDown,
    MouseUp,
}

impl StepKind {
    fn type_byte(self) -> u8 {
        match self {
            Self::KeyDown => 0xB0,
            Self::KeyUp => 0x30,
            Self::MouseDown => 0x90,
            Self::MouseUp => 0x10,
        }
    }

    fn from_type(b: u8) -> Option<Self> {
        [Self::KeyDown, Self::KeyUp, Self::MouseDown, Self::MouseUp].into_iter().find(|k| k.type_byte() == b)
    }

    pub fn is_mouse(self) -> bool {
        matches!(self, Self::MouseDown | Self::MouseUp)
    }
}

/// One step: press or release a key or mouse button, then wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacroStep {
    pub kind: StepKind,
    /// Keyboard usage (page 0x07, modifiers as E0–E7) or mouse mask.
    pub code: u8,
    /// Pause after the step; values below [`MIN_DELAY_MS`] are raised to it.
    pub delay_ms: u16,
}

impl MacroStep {
    pub fn new(kind: StepKind, code: u8, delay_ms: u16) -> Self {
        Self { kind, code, delay_ms }
    }

    fn check(&self) -> Result<(), Invalid> {
        let ok = if self.kind.is_mouse() {
            matches!(self.code, mouse::LEFT | mouse::RIGHT | mouse::MIDDLE)
        } else {
            is_key_usage(self.code) || (0xE0..=0xE7).contains(&self.code)
        };
        if ok {
            Ok(())
        } else {
            Err(Invalid::Macro("step has an unknown key or button"))
        }
    }
}

/// Used slots: everything up to the last slot with steps.
fn used(slots: &[Vec<MacroStep>]) -> &[Vec<MacroStep>] {
    let n = slots.iter().rposition(|s| !s.is_empty()).map_or(0, |i| i + 1);
    &slots[..n]
}

/// Bytes counted against [`BUDGET`].
pub fn budget_used(slots: &[Vec<MacroStep>]) -> usize {
    let slots = used(slots);
    (slots.iter().map(Vec::len).sum::<usize>() + slots.len()) * 8
}

/// Encode the whole macro image; slot `i` of `slots` is macro `i`. The
/// result is whole pages, ready to send after `04 15`. An empty list
/// clears every macro.
pub fn macro_image(slots: &[Vec<MacroStep>]) -> Result<Vec<u8>, Invalid> {
    let slots = used(slots);
    if slots.len() > MAX_MACROS {
        return Err(Invalid::Macro("more than 100 macros"));
    }
    if budget_used(slots) > BUDGET {
        return Err(Invalid::Macro("macros do not fit in the keyboard's memory"));
    }
    let mut img = vec![0u8; DIRECTORY_LEN];
    for (i, steps) in slots.iter().enumerate() {
        if steps.is_empty() {
            img[i * 4..i * 4 + 4].copy_from_slice(&EMPTY_SLOT);
            continue;
        }
        let offset = img.len() as u16;
        img[i * 4..i * 4 + 2].copy_from_slice(&offset.to_le_bytes());
        let events = (steps.len() * 2) as u16;
        img.extend_from_slice(&events.to_le_bytes());
        img.extend_from_slice(&[0; 6]);
        for s in steps {
            s.check()?;
            let [lo, hi] = s.delay_ms.max(MIN_DELAY_MS).to_le_bytes();
            img.extend_from_slice(&[0, 0, s.code, s.kind.type_byte(), lo, hi, 0, DELAY_TYPE]);
        }
    }
    // The stock sender always leaves room after the data: one extra page
    // when it ends on a page boundary, otherwise the partial page plus one.
    let pages = img.len() / PAYLOAD + if img.len() % PAYLOAD == 0 { 1 } else { 2 };
    img.resize(pages * PAYLOAD, 0);
    let n = img.len();
    img[n - 2..].copy_from_slice(&TRAILER);
    Ok(img)
}

/// Read an image back (tests and diagnostics). `None` when it is malformed.
pub fn parse_macro_image(data: &[u8]) -> Option<Vec<Vec<MacroStep>>> {
    if data.len() < DIRECTORY_LEN {
        return None;
    }
    let mut slots = Vec::new();
    for i in 0..MAX_MACROS {
        let e = &data[i * 4..i * 4 + 4];
        if e == [0, 0, 0, 0] {
            break;
        }
        if e == EMPTY_SLOT {
            slots.push(Vec::new());
            continue;
        }
        let at = u16::from_le_bytes([e[0], e[1]]) as usize;
        let events = u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]) as usize;
        let mut steps = Vec::with_capacity(events / 2);
        for k in 0..events / 2 {
            let p = at + 8 + k * 8;
            let ev = data.get(p..p + 8)?;
            let kind = StepKind::from_type(ev[3])?;
            if ev[7] != DELAY_TYPE {
                return None;
            }
            steps.push(MacroStep { kind, code: ev[2], delay_ms: u16::from_le_bytes([ev[4], ev[5]]) });
        }
        slots.push(steps);
    }
    Some(slots)
}

/// Steps that type `text` on a US layout: each character is pressed and
/// released, with Shift around shifted ones. Characters without a key are
/// skipped. On other layouts letters mostly match, symbols may not.
pub fn text_steps(text: &str, delay_ms: u16) -> Vec<MacroStep> {
    const SHIFT: u8 = 0xE1;
    let mut steps = Vec::new();
    for ch in text.chars() {
        let Some((shift, code)) = us_key(ch) else { continue };
        if shift {
            steps.push(MacroStep::new(StepKind::KeyDown, SHIFT, delay_ms));
        }
        steps.push(MacroStep::new(StepKind::KeyDown, code, delay_ms));
        steps.push(MacroStep::new(StepKind::KeyUp, code, delay_ms));
        if shift {
            steps.push(MacroStep::new(StepKind::KeyUp, SHIFT, delay_ms));
        }
    }
    steps
}

fn us_key(ch: char) -> Option<(bool, u8)> {
    const PLAIN: &[(char, u8)] = &[
        ('\n', 0x28),
        ('\t', 0x2B),
        (' ', 0x2C),
        ('-', 0x2D),
        ('=', 0x2E),
        ('[', 0x2F),
        (']', 0x30),
        ('\\', 0x31),
        (';', 0x33),
        ('\'', 0x34),
        ('`', 0x35),
        (',', 0x36),
        ('.', 0x37),
        ('/', 0x38),
    ];
    const SHIFTED: &[(char, u8)] = &[
        ('!', 0x1E),
        ('@', 0x1F),
        ('#', 0x20),
        ('$', 0x21),
        ('%', 0x22),
        ('^', 0x23),
        ('&', 0x24),
        ('*', 0x25),
        ('(', 0x26),
        (')', 0x27),
        ('_', 0x2D),
        ('+', 0x2E),
        ('{', 0x2F),
        ('}', 0x30),
        ('|', 0x31),
        (':', 0x33),
        ('"', 0x34),
        ('~', 0x35),
        ('<', 0x36),
        ('>', 0x37),
        ('?', 0x38),
    ];
    match ch {
        'a'..='z' => Some((false, 0x04 + (ch as u8 - b'a'))),
        'A'..='Z' => Some((true, 0x04 + (ch as u8 - b'A'))),
        '1'..='9' => Some((false, 0x1E + (ch as u8 - b'1'))),
        '0' => Some((false, 0x27)),
        _ => PLAIN
            .iter()
            .find(|(c, _)| *c == ch)
            .map(|&(_, k)| (false, k))
            .or_else(|| SHIFTED.iter().find(|(c, _)| *c == ch).map(|&(_, k)| (true, k))),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{guard, PacketKind};
    use super::*;

    fn tap(code: u8, delay: u16) -> [MacroStep; 2] {
        [MacroStep::new(StepKind::KeyDown, code, delay), MacroStep::new(StepKind::KeyUp, code, delay)]
    }

    #[test]
    fn empty_list_clears_every_macro() {
        // 400-byte directory → 6 full pages + 16 bytes → 8 pages, as the stock
        // "no macros" image (S4:584): zeros with AA 55 at 510–511.
        let img = macro_image(&[]).unwrap();
        assert_eq!(img.len(), 512);
        assert!(img[..510].iter().all(|&b| b == 0));
        assert_eq!(&img[510..], &[0xAA, 0x55]);
    }

    #[test]
    fn layout_matches_the_documented_format() {
        let ok = text_steps("ok", 25);
        let img = macro_image(&[ok.clone(), vec![], tap(0x04, 10).to_vec()]).unwrap();
        // Directory: slot 0 at 400, slot 1 empty, slot 2 after slot 0's record.
        assert_eq!(&img[0..4], &[0x90, 0x01, 0, 0]);
        assert_eq!(&img[4..8], &[0xFF; 4]);
        let second = (400 + 8 + ok.len() * 8) as u16;
        assert_eq!(&img[8..10], &second.to_le_bytes());
        assert_eq!(&img[10..12], &[0, 0]);
        assert_eq!(&img[12..16], &[0; 4], "after the last macro");
        // Record: 4 steps → 8 events, then O down with its 25 ms pause.
        assert_eq!(&img[400..408], &[8, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&img[408..416], &[0, 0, 0x12, 0xB0, 25, 0, 0, 0x50]);
        assert_eq!(&img[416..424], &[0, 0, 0x12, 0x30, 25, 0, 0, 0x50]);
        // Data ends at 400 + 40 + 24 = 464 → 7 pages + 16 → 9 pages.
        assert_eq!(img.len(), 9 * 64);
        assert_eq!(&img[img.len() - 2..], &[0xAA, 0x55]);
    }

    #[test]
    fn page_count_follows_the_stock_sender() {
        // 400 + 8 + 8 × 27 = 624 = 9 full pages + 48 → 11 pages.
        let steps: Vec<_> = (0..27).map(|_| MacroStep::new(StepKind::KeyDown, 0x04, 10)).collect();
        assert_eq!(macro_image(&[steps]).unwrap().len(), 11 * 64);
        // 400 + 8 + 8 × 29 = 640 = exactly 10 pages → 11 pages.
        let steps: Vec<_> = (0..29).map(|_| MacroStep::new(StepKind::KeyDown, 0x04, 10)).collect();
        assert_eq!(macro_image(&[steps]).unwrap().len(), 11 * 64);
    }

    #[test]
    fn round_trip_and_trailing_empty_slots() {
        let slots = vec![
            text_steps("Hi!", 10),
            vec![],
            vec![
                MacroStep::new(StepKind::MouseDown, mouse::LEFT, 40),
                MacroStep::new(StepKind::MouseUp, mouse::LEFT, 9),
            ],
            vec![],
        ];
        let img = macro_image(&slots).unwrap();
        let back = parse_macro_image(&img).unwrap();
        assert_eq!(back.len(), 3, "trailing empty slots are not stored");
        assert_eq!(back[0], slots[0]);
        assert!(back[1].is_empty());
        assert_eq!(back[2][1].delay_ms, MIN_DELAY_MS, "short pauses are raised to the minimum");
    }

    #[test]
    fn limits_are_enforced() {
        let too_many: Vec<_> = (0..101).map(|_| tap(0x04, 10).to_vec()).collect();
        assert!(macro_image(&too_many).is_err());
        // (336 steps + 1 macro) × 8 = 2696 fits; one more step does not.
        let fits: Vec<_> = (0..336).map(|_| MacroStep::new(StepKind::KeyDown, 0x04, 10)).collect();
        assert!(macro_image(std::slice::from_ref(&fits)).is_ok());
        let mut over = fits;
        over.push(MacroStep::new(StepKind::KeyUp, 0x04, 10));
        assert!(macro_image(&[over]).is_err());
        assert!(macro_image(&[vec![MacroStep::new(StepKind::KeyDown, 0xAF, 10)]]).is_err(), "Fn");
        assert!(macro_image(&[vec![MacroStep::new(StepKind::MouseDown, 0x08, 10)]]).is_err());
    }

    #[test]
    fn text_uses_shift_for_capitals_and_symbols() {
        let s = text_steps("aB!", 10);
        let codes: Vec<(StepKind, u8)> = s.iter().map(|x| (x.kind, x.code)).collect();
        use StepKind::*;
        assert_eq!(
            codes,
            vec![
                (KeyDown, 0x04),
                (KeyUp, 0x04),
                (KeyDown, 0xE1),
                (KeyDown, 0x05),
                (KeyUp, 0x05),
                (KeyUp, 0xE1),
                (KeyDown, 0xE1),
                (KeyDown, 0x1E),
                (KeyUp, 0x1E),
                (KeyUp, 0xE1),
            ]
        );
        assert!(text_steps("ğ", 10).is_empty(), "no US key");
    }

    /// No image the encoder accepts can start a page with a bootloader or
    /// ISP pattern: every page starts on a 4-byte entry whose third byte is
    /// zero, or on an action event starting with zero.
    #[test]
    fn pages_pass_the_guard() {
        let mut slots = Vec::new();
        for code in [0x04u8, 0x55, 0x29, 0x68, 0x73] {
            for delay in [10u16, 0x55AA, 0xAA05, 0x0701, 0xFFFF] {
                slots.push(vec![
                    MacroStep::new(StepKind::KeyDown, code, delay),
                    MacroStep::new(StepKind::MouseUp, mouse::RIGHT, delay),
                    MacroStep::new(StepKind::KeyUp, 0xE7, delay),
                ]);
            }
        }
        slots.insert(3, vec![]);
        let img = macro_image(&slots).unwrap();
        for page in img.chunks_exact(PAYLOAD) {
            assert!(guard(page, PacketKind::Data).is_ok(), "{:02X?}", &page[..4]);
        }
    }
}
