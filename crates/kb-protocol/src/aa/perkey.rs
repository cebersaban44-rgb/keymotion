//! The per-key colour table (§4.4, `14` read / `24` write) and the colours
//! the LEDs show right now (§4.5, `33`, read only).
//!
//! Both are 512 bytes: 128 entries `[i R G B]`, entry `i` for the key with
//! "AA" index `i`. On firmware 769 (protocol notes §F.4) the first byte of
//! an entry is a validity tag: the LED shows the entry's colour only when
//! that byte is `i`, and stays dark otherwise. The firmware neither checks
//! nor fixes it on a write, and keeps no default table: after a firmware
//! update or a HID factory reset the table is all `00` (entry 0 black, the
//! rest dark); a board never programmed may read all `FF`. Both are normal.
//!
//! Every write sends all 128 entries with their index in front: the keys'
//! colours, and black where no key is ([`blank`], [`set`]). The table
//! shows in effect modes 20, 21 and 128, while the effect colour is the
//! custom one (every effect write makes it so; a colour picked with the
//! keyboard's Fn keys shows the keyboard's own painted tables instead).

use crate::sonix::Rgb;

/// Bytes in the table.
pub const LEN: usize = 512;
/// Entries in the table.
pub const SLOTS: usize = 128;

/// Entry `slot` set to `rgb`, with its index in front.
pub fn set(table: &mut [u8], slot: u8, rgb: Rgb) {
    let at = usize::from(slot) * 4;
    let [r, g, b] = rgb;
    table[at..at + 4].copy_from_slice(&[slot, r, g, b]);
}

/// The colour of entry `slot` as stored (whether it shows or not: see
/// [`shown`]).
pub fn get(table: &[u8], slot: u8) -> Rgb {
    let at = usize::from(slot) * 4;
    [table[at + 1], table[at + 2], table[at + 3]]
}

/// The colour entry `slot` shows: its own when it starts with its index,
/// dark (black) otherwise.
pub fn shown(table: &[u8], slot: u8) -> Rgb {
    if table[usize::from(slot) * 4] == slot {
        get(table, slot)
    } else {
        [0, 0, 0]
    }
}

/// Every entry black and starting with its own index, `[i 00 00 00]`: what
/// a write starts from, and the `33` request's data.
pub fn blank() -> [u8; LEN] {
    let mut t = [0u8; LEN];
    for i in 0..SLOTS {
        t[i * 4] = i as u8;
    }
    t
}

/// The `33` request's data: every entry names itself, `[i 00 00 00]`.
pub fn live_request() -> [u8; LEN] {
    blank()
}

/// Whether every entry starts with its own index: the only tables this
/// driver writes ([`crate::aa::guard::guard_per_key`]).
pub fn is_well_formed(table: &[u8]) -> bool {
    table.len() == LEN && (0..SLOTS).all(|i| table[i * 4] == i as u8)
}

/// `table` as a write sends it: every entry with its index in front, with
/// the colour it shows ([`shown`]). An entry that does not start with its
/// index is dark on the keyboard, and stays dark as black.
pub fn well_formed(table: &[u8]) -> [u8; LEN] {
    let mut t = blank();
    for slot in 0..SLOTS as u8 {
        set(&mut t, slot, shown(table, slot));
    }
    t
}

/// The `[i R G B]` entries of a table or a `33` reply, by the index byte
/// each carries.
pub fn entries(table: &[u8]) -> impl Iterator<Item = (u8, Rgb)> + '_ {
    table.chunks_exact(4).map(|e| (e[0], [e[1], e[2], e[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_carry_their_index() {
        let mut t = blank();
        assert_eq!(&t[..8], &[0, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(&t[508..], &[127, 0, 0, 0]);
        assert_eq!(live_request(), t);
        set(&mut t, 76, [0xFF, 0x10, 0x00]);
        assert_eq!(&t[304..308], &[76, 0xFF, 0x10, 0x00]);
        assert_eq!(get(&t, 76), [0xFF, 0x10, 0x00]);
        let lit: Vec<_> = entries(&t).filter(|(_, c)| *c != [0, 0, 0]).collect();
        assert_eq!(lit, [(76, [0xFF, 0x10, 0x00])]);
        assert_eq!(entries(&t).count(), SLOTS);
        assert!(is_well_formed(&t));
    }

    #[test]
    fn the_first_byte_of_an_entry_is_a_validity_tag() {
        // What a freshly flashed or HID-reset keyboard holds: all 00. Entry
        // 0 is valid (black), entries 1–127 are dark.
        let zeros = [0u8; LEN];
        assert!(!is_well_formed(&zeros));
        assert_eq!(well_formed(&zeros), blank());
        // A board never programmed: all FF, every entry dark.
        let erased = [0xFFu8; LEN];
        assert_eq!(shown(&erased, 3), [0, 0, 0]);
        assert_eq!(get(&erased, 3), [0xFF; 3]);
        assert_eq!(well_formed(&erased), blank());
        // A colour behind a wrong tag does not show, and is not kept.
        let mut t = blank();
        set(&mut t, 10, [1, 2, 3]);
        t[40] = 11;
        assert_eq!(shown(&t, 10), [0, 0, 0]);
        let w = well_formed(&t);
        assert_eq!(&w[40..44], &[10, 0, 0, 0]);
        set(&mut t, 10, [1, 2, 3]);
        assert_eq!(&well_formed(&t)[40..44], &[10, 1, 2, 3]);
        assert!(!is_well_formed(&t[..508]));
    }
}
