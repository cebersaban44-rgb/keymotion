//! Check that a vendor HID collection has the shape of its command family
//! before anything is sent to it.
//!
//! SONiX boards share VID/PIDs across two command families. The "04 xx"
//! family has a 64-byte FF13 command collection and a 4096-byte FF68 screen
//! collection; the "AA" family puts 64-byte commands on FF68 and its screen
//! on FF67 (docs site: developers/protocol). Sending one family's commands
//! to the other can erase the screen slot, so the shape is checked when a
//! channel is opened, on top of the model file's IDs and product name.
//! The shapes of the two families never overlap.

use kb_devices::{InterfaceRole, Protocol};

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

/// The "AA" family's FF68 command collection: 64-byte output requests and
/// 64-byte input replies, no feature report, no report IDs.
pub fn is_aa_command_shape(desc: &[u8]) -> bool {
    let s = report_shape(desc);
    s.output == 64 && s.input == 64 && s.feature == 0 && !s.has_report_ids
}

/// The "AA" family's FF67 screen collection: 4104-byte output reports (an
/// 8-byte header and 4096 bytes of image) and 64-byte input acks, no report
/// IDs. It also declares a 64-byte feature report, which nothing uses and
/// kb-transport refuses: its size is not part of the check.
pub fn is_aa_display_shape(desc: &[u8]) -> bool {
    let s = report_shape(desc);
    s.output == 4104 && s.input == 64 && !s.has_report_ids
}

/// Whether a collection with this report descriptor can be `role` for a
/// keyboard of `protocol`. A standard collection (keyboard, consumer...)
/// never fits: nothing is sent on one.
pub fn fits(protocol: Protocol, role: InterfaceRole, desc: &[u8]) -> bool {
    match (protocol, role) {
        (Protocol::SonixFf13, InterfaceRole::Command) => is_command_shape(desc),
        (Protocol::SonixFf13, InterfaceRole::Display) => is_display_shape(desc),
        (Protocol::AjazzAa, InterfaceRole::Command) => is_aa_command_shape(desc),
        (Protocol::AjazzAa, InterfaceRole::Display) => is_aa_display_shape(desc),
        (_, InterfaceRole::Standard) => false,
    }
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

    // An AK980 PRO on firmware 769 (USB 0C45:8039, release 0x0301), from
    // its "Request support" report (issue #154).
    const AA_FF68: &str = "06 68 FF 09 61 A1 01 09 62 15 00 26 FF 00 75 08 95 40 81 02 09 63 15 00 26 FF 00 75 08 \
                           95 40 91 02 C0";
    const AA_FF67: &str = "06 67 FF 09 61 A1 01 09 64 15 00 26 FF 00 75 08 95 40 81 02 09 63 15 00 26 FF 00 75 08 \
                           96 08 10 91 02 09 62 15 00 26 FF 00 75 08 95 40 B1 02 C0";
    const AA_FFFF: &str = "06 FF FF 09 01 A1 01 85 05 09 01 15 00 26 FF 00 75 08 95 03 81 02 C0";
    const AA_KEYBOARD: &str = "05 01 09 06 A1 01 05 07 19 E0 29 E7 15 00 25 01 75 01 95 08 81 02 75 08 95 01 81 03 \
                               19 00 29 65 15 00 26 FF 00 75 08 95 06 81 00 05 08 19 01 29 05 15 00 25 01 75 01 95 \
                               05 91 02 75 03 95 01 91 03 C0";

    #[test]
    fn reads_the_aa_family_collections() {
        let cmd = report_shape(&hex(AA_FF68));
        assert_eq!((cmd.input, cmd.output, cmd.feature, cmd.has_report_ids), (64, 64, 0, false));
        let disp = report_shape(&hex(AA_FF67));
        assert_eq!((disp.input, disp.output, disp.feature, disp.has_report_ids), (64, 4104, 64, false));
        assert!(is_aa_command_shape(&hex(AA_FF68)));
        assert!(is_aa_display_shape(&hex(AA_FF67)));
        assert!(!is_aa_command_shape(&hex(AA_FF67)));
        assert!(!is_aa_display_shape(&hex(AA_FF68)));
    }

    #[test]
    fn each_layout_fits_one_family_and_role() {
        use InterfaceRole::{Command, Display, Standard};
        use Protocol::{AjazzAa, SonixFf13};
        let cases = [
            (AK980_FF13, Some((SonixFf13, Command))),
            (AK980_FF68, Some((SonixFf13, Display))),
            (AA_FF68, Some((AjazzAa, Command))),
            (AA_FF67, Some((AjazzAa, Display))),
            // Report IDs and standard collections fit nowhere.
            (AA_FFFF, None),
            (AA_KEYBOARD, None),
        ];
        for (desc, only) in cases {
            for protocol in [SonixFf13, AjazzAa] {
                for role in [Command, Display, Standard] {
                    assert_eq!(
                        fits(protocol, role, &hex(desc)),
                        only == Some((protocol, role)),
                        "{protocol} {role:?} on {}",
                        &desc[..8]
                    );
                }
            }
        }
        // The "04 xx" command collection has the AA command's 64/64 reports
        // and a feature report on top: still not an AA command channel.
        assert!(!is_aa_command_shape(&hex(AK980_FF13)));
        // A 4104-byte screen with a report ID in front is another layout.
        let with_id = hex("06 67 ff 09 61 a1 01 85 01 75 08 95 40 09 64 81 02 96 08 10 09 63 91 02 c0");
        assert!(!is_aa_display_shape(&with_id));
    }

    #[test]
    fn survives_truncated_and_push_pop_descriptors() {
        assert_eq!(report_shape(&hex("06 13 ff 75 08 96 00")), ReportShape::default());
        let pushed = hex("75 08 95 40 a4 75 01 95 08 81 02 b4 b1 02");
        let s = report_shape(&pushed);
        assert_eq!((s.input, s.feature), (1, 64));
    }
}
