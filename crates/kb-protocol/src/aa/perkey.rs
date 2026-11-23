//! The per-key colour table (§4.4, `14` read / `24` write) and the colours
//! the LEDs show right now (§4.5, `33`, read only).
//!
//! Both are 512 bytes: 128 entries `[i R G B]`, entry `i` for the key with
//! "AA" index `i`. A write keeps every slot that is not a key of the layout
//! as it was read, slots 126–127 included (some boards keep `AA 55` in
//! bytes 510–511). The table shows in effect modes 20, 21 and 128.

use crate::sonix::Rgb;

/// Bytes in the table.
pub const LEN: usize = 512;
/// Entries in the table.
pub const SLOTS: usize = 128;
/// Kept as read in every write.
pub const TAIL_SLOTS: [u8; 2] = [126, 127];

/// Entry `slot` set to `rgb`, with its index in front.
pub fn set(table: &mut [u8], slot: u8, rgb: Rgb) {
    let at = usize::from(slot) * 4;
    let [r, g, b] = rgb;
    table[at..at + 4].copy_from_slice(&[slot, r, g, b]);
}

/// The colour of entry `slot`.
pub fn get(table: &[u8], slot: u8) -> Rgb {
    let at = usize::from(slot) * 4;
    [table[at + 1], table[at + 2], table[at + 3]]
}

/// The `33` request's data: every entry names itself, `[i 00 00 00]`.
pub fn live_request() -> [u8; LEN] {
    let mut t = [0u8; LEN];
    for i in 0..SLOTS {
        t[i * 4] = i as u8;
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
        let mut t = live_request();
        assert_eq!(&t[..8], &[0, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(&t[508..], &[127, 0, 0, 0]);
        set(&mut t, 76, [0xFF, 0x10, 0x00]);
        assert_eq!(&t[304..308], &[76, 0xFF, 0x10, 0x00]);
        assert_eq!(get(&t, 76), [0xFF, 0x10, 0x00]);
        let lit: Vec<_> = entries(&t).filter(|(_, c)| *c != [0, 0, 0]).collect();
        assert_eq!(lit, [(76, [0xFF, 0x10, 0x00])]);
        assert_eq!(entries(&t).count(), SLOTS);
    }
}
