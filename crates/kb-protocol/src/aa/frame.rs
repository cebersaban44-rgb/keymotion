//! Request and reply framing on the FF68 command channel (§2).

/// Bytes in one request or reply, report ID excluded.
pub const REPORT_LEN: usize = 64;
/// A request as written: report ID `00` and the 64-byte request.
pub const WIRE_LEN: usize = REPORT_LEN + 1;
/// Header bytes in front of the data.
pub const HEADER_LEN: usize = 8;
/// Data bytes in one request or reply.
pub const CHUNK: usize = REPORT_LEN - HEADER_LEN;
/// First byte of every request.
pub const REQUEST: u8 = 0xAA;
/// First byte of every reply.
pub const REPLY: u8 = 0x55;
/// Bytes in one flash page. The keyboard keeps its settings straight in
/// flash and erases a page only for a write chunk that starts exactly on
/// the page's first byte; any other chunk is programmed over what the page
/// holds (protocol notes §F.2). Every table write therefore starts at
/// address 0 and goes up ([`paged_chunks`]).
pub const PAGE: usize = 1024;

/// One chunk of a block: where its data starts in the block and what the
/// header says about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    /// Offset of the chunk's data in the caller's buffer.
    pub offset: usize,
    /// Header ADDR: the chunk's byte address on the keyboard.
    pub addr: u16,
    /// Header LEN: data bytes in this chunk (1–56).
    pub len: u8,
    /// Header LAST.
    pub last: bool,
}

/// Cut a block of `total` bytes starting at keyboard address `base` into
/// 56-byte chunks. LAST is set on the final chunk when `last_flag` is true
/// (every block but the macro table, which is written with LAST = 0 on all
/// chunks). A block of 0 bytes has no chunks.
pub fn chunks(total: usize, base: u16, last_flag: bool) -> Vec<Chunk> {
    let n = total.div_ceil(CHUNK);
    (0..n)
        .map(|t| {
            let offset = t * CHUNK;
            Chunk {
                offset,
                addr: base.wrapping_add(offset as u16),
                len: (total - offset).min(CHUNK) as u8,
                last: last_flag && t == n - 1,
            }
        })
        .collect()
}

/// Cut a block of `total` bytes written from address 0 into chunks of at
/// most 56 bytes that never cross a page boundary, so that one chunk starts
/// exactly on every page the block reaches (0, 1024, 2048 …) and the
/// firmware erases each page before it programs it. Ascending, each byte
/// once, LAST on the final chunk. For a block within one page this is
/// [`chunks`]`(total, 0, true)`.
pub fn paged_chunks(total: usize) -> Vec<Chunk> {
    let mut out = Vec::with_capacity(total.div_ceil(CHUNK) + total / PAGE);
    let mut offset = 0;
    while offset < total {
        let to_page = PAGE - offset % PAGE;
        let len = (total - offset).min(CHUNK).min(to_page);
        out.push(Chunk { offset, addr: offset as u16, len: len as u8, last: false });
        offset += len;
    }
    if let Some(c) = out.last_mut() {
        c.last = true;
    }
    out
}

/// Build a 64-byte request: `AA CMD LEN ADDR_LO ADDR_HI R5 LAST 00` and
/// the first LEN bytes of `data` (the rest is zero; a read passes none).
/// `r5` is the real-time frame's amplitude, 0 for the AK980 PRO and in
/// every other request.
pub fn request(cmd: u8, chunk: &Chunk, r5: u8, data: &[u8]) -> [u8; REPORT_LEN] {
    let mut r = [0u8; REPORT_LEN];
    let [lo, hi] = chunk.addr.to_le_bytes();
    r[..HEADER_LEN].copy_from_slice(&[REQUEST, cmd, chunk.len, lo, hi, r5, u8::from(chunk.last), 0]);
    let n = data.len().min(chunk.len as usize);
    r[HEADER_LEN..HEADER_LEN + n].copy_from_slice(&data[..n]);
    r
}

/// Prepend the report ID: what is written to the channel.
pub fn wire(request: &[u8; REPORT_LEN]) -> [u8; WIRE_LEN] {
    let mut w = [0u8; WIRE_LEN];
    w[1..].copy_from_slice(request);
    w
}

/// A parsed reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub cmd: u8,
    pub len: u8,
    pub addr: u16,
    pub last: bool,
    /// The 56 data bytes (fewer if the report was short).
    pub data: Vec<u8>,
}

/// Parse a 64-byte input report (no report ID). `None` unless it starts
/// with `55` and holds a full header.
pub fn parse_reply(report: &[u8]) -> Option<Reply> {
    if report.len() < HEADER_LEN || report[0] != REPLY {
        return None;
    }
    Some(Reply {
        cmd: report[1],
        len: report[2],
        addr: u16::from_le_bytes([report[3], report[4]]),
        last: report[6] != 0,
        data: report[HEADER_LEN..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keymap_read_matches_the_capture() {
        // S11 capture (AK820 Pro fw 1.16, AJAZZ's WebHID driver): a 512-byte
        // keymap read is 9 × `AA 12 38 ADDR` and `AA 12 08 F8 01 00 01 00`.
        let c = chunks(512, 0, true);
        assert_eq!(c.len(), 10);
        let reqs: Vec<[u8; 64]> = c.iter().map(|c| request(0x12, c, 0, &[])).collect();
        assert_eq!(&reqs[0][..8], &[0xAA, 0x12, 0x38, 0x00, 0x00, 0, 0, 0]);
        assert_eq!(&reqs[1][..8], &[0xAA, 0x12, 0x38, 0x38, 0x00, 0, 0, 0]);
        assert_eq!(&reqs[8][..8], &[0xAA, 0x12, 0x38, 0xC0, 0x01, 0, 0, 0]);
        assert_eq!(&reqs[9][..8], &[0xAA, 0x12, 0x08, 0xF8, 0x01, 0x00, 0x01, 0x00]);
        assert!(reqs.iter().all(|r| r[8..].iter().all(|&b| b == 0)), "reads carry no data");
    }

    #[test]
    fn single_chunk_reads_match_the_captures() {
        // Device info (S11, and firmware 769 in #966): `AA 10 30 00 00 00 01 00`.
        assert_eq!(&request(0x10, &chunks(48, 0, true)[0], 0, &[])[..8], &[0xAA, 0x10, 0x30, 0, 0, 0, 1, 0]);
        // Options and effect (S11).
        assert_eq!(&request(0x11, &chunks(56, 0, true)[0], 0, &[])[..8], &[0xAA, 0x11, 0x38, 0, 0, 0, 1, 0]);
        assert_eq!(&request(0x13, &chunks(16, 0, true)[0], 0, &[])[..8], &[0xAA, 0x13, 0x10, 0, 0, 0, 1, 0]);
        // The 400-byte macro table read (S11): 7 × 56 and `AA 15 08 88 01 00 01 00`.
        let t = chunks(400, 0, true);
        assert_eq!(t.len(), 8);
        assert_eq!(&request(0x15, &t[7], 0, &[])[..8], &[0xAA, 0x15, 0x08, 0x88, 0x01, 0, 1, 0]);
    }

    #[test]
    fn the_macro_table_write_never_sets_last() {
        let t = chunks(400, 0, false);
        assert!(t.iter().all(|c| !c.last));
        // Blocks start at 400 and end with LAST.
        let b = chunks(60, 400, true);
        assert_eq!((b[0].addr, b[0].len, b[0].last), (400, 56, false));
        assert_eq!((b[1].addr, b[1].len, b[1].last), (456, 4, true));
        assert!(chunks(0, 0, true).is_empty());
    }

    #[test]
    fn paged_chunks_start_on_every_page() {
        // Within one page: the plain 56-byte cut.
        assert_eq!(paged_chunks(512), chunks(512, 0, true));
        assert!(paged_chunks(0).is_empty());
        // The macro memory (3 pages): 56-byte chunks never land on 1024
        // (1008 → 1064), so a 16-byte chunk closes each page.
        let c = paged_chunks(3072);
        let starts: Vec<usize> = c.iter().map(|c| c.offset).collect();
        assert!(starts.contains(&0) && starts.contains(&1024) && starts.contains(&2048));
        let at_1008 = c.iter().find(|c| c.offset == 1008).unwrap();
        assert_eq!(at_1008.len, 16);
        // Contiguous, ascending, every byte once, none crossing a page.
        let mut next = 0;
        for ch in &c {
            assert_eq!(ch.offset, next);
            assert_eq!(usize::from(ch.addr), ch.offset);
            assert!((1..=56).contains(&usize::from(ch.len)));
            assert_eq!(ch.offset / PAGE, (ch.offset + usize::from(ch.len) - 1) / PAGE, "{ch:?} crosses a page");
            assert_eq!(ch.len % 4, 0);
            next += usize::from(ch.len);
        }
        assert_eq!(next, 3072);
        assert!(c[..c.len() - 1].iter().all(|ch| !ch.last) && c.last().unwrap().last);
        // A block ending just past a page boundary.
        let d = paged_chunks(1028);
        assert_eq!((d.last().unwrap().offset, d.last().unwrap().len), (1024, 4));
    }

    #[test]
    fn requests_and_replies() {
        let c = Chunk { offset: 0, addr: 0x0138, len: 3, last: true };
        let r = request(0x32, &c, 7, &[1, 2, 3]);
        assert_eq!(&r[..11], &[0xAA, 0x32, 3, 0x38, 0x01, 7, 1, 0, 1, 2, 3]);
        let w = wire(&r);
        assert_eq!((w.len(), w[0], w[1]), (65, 0, 0xAA));

        // Firmware 769's real reply header (#966).
        let mut rep = [0u8; 64];
        rep[..8].copy_from_slice(&[0x55, 0x10, 0x30, 0, 0, 0, 1, 0]);
        rep[8] = 0x40;
        let p = parse_reply(&rep).unwrap();
        assert_eq!((p.cmd, p.len, p.addr, p.last, p.data[0], p.data.len()), (0x10, 0x30, 0, true, 0x40, 56));
        assert_eq!(parse_reply(&[0xAA, 0x10, 0, 0, 0, 0, 0, 0]), None, "a request is no reply");
        assert_eq!(parse_reply(&[0x55, 0x10]), None, "too short");
    }
}
