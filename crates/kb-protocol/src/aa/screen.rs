//! Screen upload (§4.9): `50` packets on FF67.
//!
//! Packet (4104 bytes, written as 4105 with the report ID):
//! `AA 50 IDX_LO IDX_HI TOTAL_LO TOTAL_HI 50 06` and 4096 bytes of the blob.
//! Blob: a 256-byte header filled with `FF`, B[0] = frame count N, B[1..N]
//! the delays of frames 0..N−2 in 2 ms units, B[N] = 0; then the frames,
//! 240×135 RGB565 little-endian, no padding between them; the last packet
//! is zero-padded. Every packet is answered `55 41 00 01` on FF67, but FF67
//! answers anything that way: the answer is no proof.
//!
//! The image comes from the "04 xx" family's validated
//! [`ScreenAnimation`] (1–140 frames, 240×135, delays 1–254).

use crate::sonix::screen::{ScreenAnimation, FRAME_BYTES};

/// Blob bytes in one packet.
pub const PAYLOAD: usize = 4096;
/// Packet header bytes.
pub const HEADER_LEN: usize = 8;
/// A packet, report ID excluded.
pub const PACKET_LEN: usize = HEADER_LEN + PAYLOAD;
/// A packet as written: report ID `00` and the packet.
pub const WIRE_LEN: usize = PACKET_LEN + 1;
/// Header bytes 6–7.
pub const MARKER: [u8; 2] = [0x50, 0x06];
/// Blob header bytes.
pub const BLOB_HEADER: usize = 256;
/// Most frames ever sent: AJAZZ's tools allow 140–141, and a sibling
/// board's menu graphics were destroyed by going past its limit.
pub const MAX_FRAMES: usize = 140;
/// Packets of a 140-frame upload.
pub const MAX_PACKETS: usize = 2215;
/// The acknowledgement FF67 sends after every packet (and to anything else).
pub const ACK: [u8; 4] = [0x55, 0x41, 0x00, 0x01];
/// How long to wait for the acknowledgement (the first one can take
/// ~300 ms; AJAZZ's WebHID driver waits 2 s).
pub const ACK_TIMEOUT_MS: u32 = 2000;

/// Frames the keyboard may take: its `tft_max_frames` (0 = not reported →
/// 140), never more than 140.
pub fn frame_limit(tft_max_frames: u16) -> usize {
    match usize::from(tft_max_frames) {
        0 => MAX_FRAMES,
        n => n.min(MAX_FRAMES),
    }
}

/// The blob header: N, the delays of every frame but the last, 0, `FF`s.
pub fn blob_header(delays: &[u8]) -> [u8; BLOB_HEADER] {
    let mut h = [0xFFu8; BLOB_HEADER];
    let n = delays.len().min(BLOB_HEADER - 1);
    h[0] = n as u8;
    h[1..n].copy_from_slice(&delays[..n.saturating_sub(1)]);
    h[n] = 0;
    h
}

/// The whole blob: header and frames, unpadded.
pub fn blob(anim: &ScreenAnimation) -> Vec<u8> {
    let frames = anim.frames();
    let delays: Vec<u8> = frames.iter().map(|(_, d)| *d).collect();
    let mut out = Vec::with_capacity(BLOB_HEADER + frames.len() * FRAME_BYTES);
    out.extend_from_slice(&blob_header(&delays));
    for (frame, _) in frames {
        for &p in frame.pixels() {
            out.extend_from_slice(&p.to_le_bytes());
        }
    }
    out
}

/// Packets an upload of `frames` frames takes.
pub const fn packets_for(frames: usize) -> usize {
    (BLOB_HEADER + FRAME_BYTES * frames).div_ceil(PAYLOAD)
}

/// Packet `index` of `total` from `blob`, report ID excluded.
pub fn packet(blob: &[u8], index: usize, total: usize) -> Vec<u8> {
    let mut p = vec![0u8; PACKET_LEN];
    let [il, ih] = (index as u16).to_le_bytes();
    let [tl, th] = (total as u16).to_le_bytes();
    p[..HEADER_LEN].copy_from_slice(&[0xAA, super::cmd::SCREEN, il, ih, tl, th, MARKER[0], MARKER[1]]);
    let start = (index * PAYLOAD).min(blob.len());
    let end = (start + PAYLOAD).min(blob.len());
    p[HEADER_LEN..HEADER_LEN + (end - start)].copy_from_slice(&blob[start..end]);
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sonix::screen::{rgb565, test_pattern, Frame};

    #[test]
    fn header_matches_the_capture() {
        // S11 (AJAZZ's WebHID driver, an AK820 Pro): the first packet's blob
        // starts `84 19 19 …`: 132 frames of 50 ms.
        let h = blob_header(&[25; 132]);
        assert_eq!(&h[..3], &[0x84, 0x19, 0x19]);
        assert!(h[1..132].iter().all(|&d| d == 25), "frames 0–130");
        assert_eq!(h[132], 0, "no delay for the last frame");
        assert!(h[133..].iter().all(|&b| b == 0xFF));
        // One frame: just N and the terminator.
        assert_eq!(&blob_header(&[50])[..3], &[1, 0, 0xFF]);
    }

    #[test]
    fn one_and_140_frames() {
        let one = ScreenAnimation::still(test_pattern());
        let b = blob(&one);
        assert_eq!(b.len(), 256 + FRAME_BYTES);
        assert_eq!(packets_for(1), 16);
        let last = packet(&b, 15, 16);
        let used = b.len() - 15 * PAYLOAD;
        assert!(last[HEADER_LEN + used..].iter().all(|&x| x == 0), "the last packet is zero-padded");
        let first = packet(&b, 0, 16);
        assert_eq!(&first[..10], &[0xAA, 0x50, 0, 0, 16, 0, 0x50, 0x06, 1, 0]);
        assert_eq!(&first[HEADER_LEN + 256..HEADER_LEN + 258], &rgb565(255, 255, 255).to_le_bytes(), "border pixel");

        assert_eq!(packets_for(MAX_FRAMES), MAX_PACKETS);
        let red = Frame::filled(rgb565(255, 0, 0));
        let anim = ScreenAnimation::new(vec![(red, 50); MAX_FRAMES]).unwrap();
        let b = blob(&anim);
        assert_eq!(b.len().div_ceil(PAYLOAD), MAX_PACKETS);
        let p = packet(&b, MAX_PACKETS - 1, MAX_PACKETS);
        assert_eq!(&p[2..6], &[0xA6, 0x08, 0xA7, 0x08], "index 2214 of 2215");
        assert_eq!(&p[HEADER_LEN..HEADER_LEN + 2], &[0x00, 0xF8]);
    }

    #[test]
    fn frame_limits() {
        assert_eq!(frame_limit(0), 140, "not reported");
        assert_eq!(frame_limit(142), 140);
        assert_eq!(frame_limit(24), 24);
    }
}
