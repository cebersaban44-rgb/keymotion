//! Check that a vendor HID collection has the shape of the "04 xx" protocol
//! family before anything is sent to it.
//!
//! SONiX boards share VID/PIDs across two command families. The "04 xx"
//! family (this app's) has a 64-byte FF13 command collection and a 4096-byte
//! FF68 screen collection; the "AA" family puts 64-byte commands on FF68 and
//! adds FF67 (docs site: developers/protocol). Sending one family's
//! commands to the other can erase the screen slot, so the shape is checked
//! when a channel is opened, on top of the model file's IDs and product name.

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

/// The FF13 command collection: 64-byte feature reports, no report IDs.
pub fn is_command_shape(desc: &[u8]) -> bool {
    let s = report_shape(desc);
    s.feature == 64 && s.output == 64 && !s.has_report_ids
}

/// The FF68 screen collection: 4096-byte output reports, no report IDs.
pub fn is_display_shape(desc: &[u8]) -> bool {
    let s = report_shape(desc);
    s.output == 4096 && !s.has_report_ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    // Read from the user's AK980 PRO (fw 0x0106) with `kb descriptors`.
    const AK980_FF13: &str = "06 13 ff 09 01 a1 01 09 02 15 00 26 ff 00 75 08 95 40 81 02 09 03 15 00 26 ff 00 75 08 \
                              95 40 91 02 0a 00 ff 15 00 26 ff 00 75 08 95 40 b1 02 c0";
    const AK980_FF68: &str = "06 68 ff 09 61 a1 01 09 62 15 00 26 ff 00 75 08 95 40 81 02 09 63 15 00 26 ff 00 75 08 \
                              96 00 10 91 02 c0";

    #[test]
    fn reads_the_ak980_pro_collections() {
        let cmd = report_shape(&hex(AK980_FF13));
        assert_eq!((cmd.input, cmd.output, cmd.feature, cmd.has_report_ids), (64, 64, 64, false));
        let disp = report_shape(&hex(AK980_FF68));
        assert_eq!((disp.input, disp.output, disp.feature), (64, 4096, 0));
        assert!(is_command_shape(&hex(AK980_FF13)));
        assert!(is_display_shape(&hex(AK980_FF68)));
        assert!(!is_command_shape(&hex(AK980_FF68)));
        assert!(!is_display_shape(&hex(AK980_FF13)));
    }

    #[test]
    fn rejects_the_aa_family_shape() {
        // "AA" family: FF68 carries 64-byte commands, not 4096-byte pages.
        let aa_ff68 = hex("06 68 ff 09 61 a1 01 15 00 26 ff 00 75 08 95 40 09 62 81 02 95 40 09 63 91 02 c0");
        assert!(!is_display_shape(&aa_ff68));
        // A report ID in front of the command report is another layout too.
        let with_id = hex("06 13 ff 09 01 a1 01 85 01 75 08 95 40 09 02 81 02 09 03 91 02 09 04 b1 02 c0");
        assert!(!is_command_shape(&with_id));
    }

    #[test]
    fn survives_truncated_and_push_pop_descriptors() {
        assert_eq!(report_shape(&hex("06 13 ff 75 08 96 00")), ReportShape::default());
        let pushed = hex("75 08 95 40 a4 75 01 95 08 81 02 b4 b1 02");
        let s = report_shape(&pushed);
        assert_eq!((s.input, s.feature), (1, 64));
    }
}
