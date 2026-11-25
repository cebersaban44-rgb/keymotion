//! Report lengths from a HID report descriptor.
//!
//! A channel takes output reports no longer than its collection declares:
//! [`crate::hid::Hid::open`] sets each channel's output limit from its
//! descriptor ([`output_limit`]), so a 64-byte command collection is never
//! sent a 4 KiB report.

/// Byte lengths of the reports a collection declares (report IDs excluded).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReportShape {
    pub input: usize,
    pub output: usize,
    pub feature: usize,
    pub has_report_ids: bool,
}

/// Read the report lengths from a HID report descriptor. Unknown or long
/// items are skipped; a truncated descriptor yields what was read so far.
/// With report IDs the lengths are the sums over all reports.
pub fn report_shape(desc: &[u8]) -> ReportShape {
    #[derive(Clone, Copy, Default)]
    struct Globals {
        size: u32,
        count: u32,
    }
    let (mut g, mut stack) = (Globals::default(), Vec::new());
    let mut bits = [0u64; 3]; // input, output, feature
    let mut has_report_ids = false;
    let mut i = 0;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix == 0xFE {
            // Long item: size byte, tag byte, data.
            let len = desc.get(i + 1).copied().unwrap_or(0) as usize;
            i += 3 + len;
            continue;
        }
        let len = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        let Some(data) = desc.get(i + 1..i + 1 + len) else { break };
        let value = data.iter().rev().fold(0u32, |v, b| (v << 8) | u32::from(*b));
        match (prefix >> 2) & 0x03 {
            // Main items.
            0 => {
                let slot = match prefix >> 4 {
                    0x8 => Some(0),
                    0x9 => Some(1),
                    0xB => Some(2),
                    _ => None,
                };
                if let Some(s) = slot {
                    bits[s] += u64::from(g.size) * u64::from(g.count);
                }
            }
            // Global items.
            1 => match prefix >> 4 {
                0x7 => g.size = value,
                0x8 => has_report_ids = true,
                0x9 => g.count = value,
                0xA => stack.push(g),
                0xB => g = stack.pop().unwrap_or_default(),
                _ => {}
            },
            _ => {}
        }
        i += 1 + len;
    }
    let bytes = |b: u64| b.div_ceil(8) as usize;
    ReportShape { input: bytes(bits[0]), output: bytes(bits[1]), feature: bytes(bits[2]), has_report_ids }
}

/// The longest output report a channel with this descriptor sends, report
/// ID byte included (written as `00` when the collection uses none), at
/// most [`crate::hid::MAX_REPORT`]. A collection without output reports
/// takes none at all (0). With report IDs, the sum of all output reports
/// bounds the longest one.
pub fn output_limit(desc: &[u8]) -> usize {
    match report_shape(desc).output {
        0 => 0,
        n => (n + 1).min(crate::hid::MAX_REPORT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    #[test]
    fn survives_truncated_and_push_pop_descriptors() {
        assert_eq!(report_shape(&hex("06 13 ff 75 08 96 00")), ReportShape::default());
        let pushed = hex("75 08 95 40 a4 75 01 95 08 81 02 b4 b1 02");
        let s = report_shape(&pushed);
        assert_eq!((s.input, s.feature), (1, 64));
    }

    #[test]
    fn output_limits_of_both_families() {
        // "04 xx" family (the owner's AK980 PRO, fw 0x0106): the FF13
        // command collection (64-byte reports) and the FF68 screen (4096).
        let ff13 = hex("06 13 ff 09 01 a1 01 09 02 15 00 26 ff 00 75 08 95 40 81 02 09 03 15 00 26 ff 00 75 08 \
             95 40 91 02 0a 00 ff 15 00 26 ff 00 75 08 95 40 b1 02 c0");
        let ff68 = hex("06 68 ff 09 61 a1 01 09 62 15 00 26 ff 00 75 08 95 40 81 02 09 63 15 00 26 ff 00 75 08 \
             96 00 10 91 02 c0");
        assert_eq!(output_limit(&ff13), 65);
        assert_eq!(output_limit(&ff68), 4097, "what every channel had before");
        // "AA" family (firmware 769, issue #154): FF68 commands, FF67 screen.
        let aa_ff68 = hex("06 68 FF 09 61 A1 01 09 62 15 00 26 FF 00 75 08 95 40 81 02 09 63 15 00 26 FF 00 75 08 \
             95 40 91 02 C0");
        let aa_ff67 = hex("06 67 FF 09 61 A1 01 09 64 15 00 26 FF 00 75 08 95 40 81 02 09 63 15 00 26 FF 00 75 08 \
             96 08 10 91 02 09 62 15 00 26 FF 00 75 08 95 40 B1 02 C0");
        assert_eq!(output_limit(&aa_ff68), 65);
        assert_eq!(output_limit(&aa_ff67), 4105);
        // No output report: nothing may be written. Never above MAX_REPORT.
        assert_eq!(output_limit(&hex("06 ff ff 09 01 a1 01 85 05 75 08 95 03 81 02 c0")), 0);
        assert_eq!(output_limit(&hex("75 08 96 00 20 91 02")), crate::hid::MAX_REPORT);
        assert_eq!(output_limit(&[]), 0);
    }
}
