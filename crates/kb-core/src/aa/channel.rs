//! Opening an "AA" keyboard's vendor collections.

use super::{AaError, Result};
use crate::shape;
use kb_devices::{InterfaceRole, Protocol};
use kb_protocol::aa::guard::Channel;
use kb_transport::hid::{Hid, HidChannel, HidInterface};

/// Usage page of the command collection (64-byte requests and replies).
pub const COMMAND_USAGE_PAGE: u16 = 0xFF68;
/// Usage page of the screen collection (4104-byte packets).
pub const DISPLAY_USAGE_PAGE: u16 = 0xFF67;

/// Open the command (FF68) or screen (FF67) collection of an "AA"
/// keyboard, after checking that it names itself (a board in its
/// bootloader keeps the IDs and drops the name), that its report layout is
/// the family's, and that the longest output report its descriptor declares
/// is exactly the one the protocol uses (65 or 4105 bytes, report ID
/// included). The channel's output limit is set from that descriptor, so
/// nothing longer can be written to it. Nothing is sent to the keyboard.
pub fn open_channel(hid: &Hid, iface: &HidInterface, role: InterfaceRole) -> Result<HidChannel> {
    let (page, channel) = match role {
        InterfaceRole::Command => (COMMAND_USAGE_PAGE, Channel::Command),
        InterfaceRole::Display => (DISPLAY_USAGE_PAGE, Channel::Display),
        InterfaceRole::Standard => return Err(AaError::Channel("a standard collection takes no commands".into())),
    };
    if iface.usage_page != page {
        return Err(AaError::Channel(format!("usage page {:04X}, expected {page:04X}", iface.usage_page)));
    }
    if iface.product.as_deref().is_none_or(|p| p.trim().is_empty()) {
        return Err(AaError::Channel("the keyboard reports no name (bootloader?); nothing is sent".into()));
    }
    let mut ch = hid.open(iface)?;
    let desc = ch.report_descriptor()?;
    check_descriptor(&desc, role, channel)?;
    let limit = shape::report_shape(&desc).output + 1;
    ch.set_output_limit(limit)?;
    if ch.output_limit() != channel.wire_len() {
        return Err(AaError::Channel(format!("output limit {} bytes", ch.output_limit())));
    }
    Ok(ch)
}

/// The descriptor checks of [`open_channel`].
fn check_descriptor(desc: &[u8], role: InterfaceRole, channel: Channel) -> Result<()> {
    let s = shape::report_shape(desc);
    if !shape::fits(Protocol::AjazzAa, role, desc) {
        return Err(AaError::Channel(format!("report layout {s:?} is not the \"AA\" family's")));
    }
    let limit = s.output + 1;
    if limit != channel.wire_len() {
        return Err(AaError::Channel(format!(
            "the descriptor declares {limit}-byte output reports, expected {}",
            channel.wire_len()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    // Firmware 769 (issue #154), as in shape.rs.
    const FF68: &str = "06 68 FF 09 61 A1 01 09 62 15 00 26 FF 00 75 08 95 40 81 02 09 63 15 00 26 FF 00 75 08 \
                        95 40 91 02 C0";
    const FF67: &str = "06 67 FF 09 61 A1 01 09 64 15 00 26 FF 00 75 08 95 40 81 02 09 63 15 00 26 FF 00 75 08 \
                        96 08 10 91 02 09 62 15 00 26 FF 00 75 08 95 40 B1 02 C0";
    // The "04 xx" family's FF68 screen (4096-byte pages).
    const FF13_FF68: &str = "06 68 ff 09 61 a1 01 09 62 15 00 26 ff 00 75 08 95 40 81 02 09 63 15 00 26 ff 00 75 08 \
                             96 00 10 91 02 c0";

    #[test]
    fn descriptors_set_and_confirm_the_limits() {
        assert!(check_descriptor(&hex(FF68), InterfaceRole::Command, Channel::Command).is_ok());
        assert!(check_descriptor(&hex(FF67), InterfaceRole::Display, Channel::Display).is_ok());
        assert_eq!(kb_transport::descriptor::output_limit(&hex(FF68)), Channel::Command.wire_len());
        assert_eq!(kb_transport::descriptor::output_limit(&hex(FF67)), Channel::Display.wire_len());
        // The other family's FF68, a role swap: refused before anything is sent.
        for (desc, role, ch) in [
            (FF13_FF68, InterfaceRole::Command, Channel::Command),
            (FF67, InterfaceRole::Command, Channel::Command),
            (FF68, InterfaceRole::Display, Channel::Display),
        ] {
            assert!(matches!(check_descriptor(&hex(desc), role, ch), Err(AaError::Channel(_))), "{role:?}");
        }
    }
}
