//! Real-time frames (§4.6): `32`, shown from RAM, nothing stored.
//!
//! One frame is `[i R G B]` for every key of the layout, in layout order
//! (not the 128-slot table), in 56-byte chunks; R5 (amplitude) is 0 for the
//! AK980 PRO and LAST is set on the final chunk. Every chunk is echoed.
//!
//! On firmware 769 (protocol notes §F.5) a frame shows in any effect mode:
//! the first byte of each entry names the slot (ADDR and LAST are
//! ignored), an LED the frame leaves out keeps what it showed, and about
//! 5 s after the last frame the keyboard shows its own effect again by
//! itself. Nothing is written to flash, so nothing has to be put back. A
//! frame sent while the keyboard runs a key-combination reset is ignored.

use super::frame::CHUNK;
use super::guard::find_forbidden;
use crate::sonix::Rgb;

/// Wait for each chunk's echo (AJAZZ's WebHID driver).
pub const CHUNK_TIMEOUT_MS: u32 = 50;

/// One frame: `[i R G B]` for each index in `order`, coloured by `color`.
pub fn frame(order: &[u8], mut color: impl FnMut(u8) -> Rgb) -> Vec<u8> {
    let mut f = Vec::with_capacity(order.len() * 4);
    for &i in order {
        let [r, g, b] = color(i);
        f.extend_from_slice(&[i, r, g, b]);
    }
    defuse(&mut f);
    f
}

/// Change colour bytes by one step where a chunk would otherwise carry a
/// bootloader sequence (the guard would refuse it). Only possible for rare
/// colours next to particular keys; one step of one channel is invisible.
pub fn defuse(frame: &mut [u8]) {
    for chunk in frame.chunks_mut(CHUNK) {
        for _ in 0..chunk.len() {
            let Some(at) = find_forbidden(chunk) else { break };
            // Chunks start on an entry, so offsets that are not a multiple
            // of 4 are colour bytes.
            if let Some(p) = (at..at + 8).find(|p| p % 4 != 0) {
                chunk[p] ^= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aa::guard::FORBIDDEN;

    #[test]
    fn entries_follow_the_layout_order() {
        let f = frame(&[0, 76, 85], |i| if i == 76 { [1, 2, 3] } else { [0, 0, 0] });
        assert_eq!(f, [0, 0, 0, 0, 76, 1, 2, 3, 85, 0, 0, 0]);
    }

    #[test]
    fn bootloader_sequences_are_defused() {
        // `AA 55 A5 5A FF 00 33 CC` across two entries: blue AA, then the
        // entry of key 0x55 coloured A5 5A FF, then key 0 coloured 33 CC ...
        let order = [1u8, 0x55, 0x00];
        let colors = |i: u8| match i {
            1 => [0, 0, 0xAA],
            0x55 => [0xA5, 0x5A, 0xFF],
            _ => [0x33, 0xCC, 0],
        };
        let raw: Vec<u8> = order.iter().flat_map(|&i| std::iter::once(i).chain(colors(i))).collect();
        assert_eq!(find_forbidden(&raw), Some(3), "the sequence is really there");
        let f = frame(&order, colors);
        assert_eq!(find_forbidden(&f), None);
        let changed: Vec<usize> = (0..f.len()).filter(|&i| f[i] != raw[i]).collect();
        assert_eq!(changed, [3], "one colour byte, one step");
        assert_eq!(f[3], 0xAB);
        // Index bytes are never touched.
        let mut all = FORBIDDEN[0].to_vec();
        all.extend_from_slice(&FORBIDDEN[1]);
        defuse(&mut all);
        assert_eq!((all[0], all[4], all[8], all[12]), (0xAA, 0xFF, 0xAA, 0xFF));
        assert_eq!(find_forbidden(&all), None);
    }
}
