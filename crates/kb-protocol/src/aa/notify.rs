//! Reports the keyboard sends on its own (§2.7). The driver skips them
//! while it waits for a reply, and counts them.

use super::frame::REPLY;

/// What an unsolicited report says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// `55 FC 05`: the keyboard was reset (e.g. with a key combination):
    /// what the host knew about its state is stale.
    Reset,
    /// `55 FC 04`: the 2.4 GHz link dropped.
    Disconnected,
    /// `55 FC 06`: the keyboard went to sleep.
    Asleep,
    /// `55 FA …`: a device status notice.
    Status,
    /// `55 FB …`: calibration data (magnetic boards).
    Calibration,
}

/// Classify a report that is not the awaited reply.
pub fn notice(report: &[u8]) -> Option<Notice> {
    match report {
        [REPLY, 0xFC, 0x05, ..] => Some(Notice::Reset),
        [REPLY, 0xFC, 0x04, ..] => Some(Notice::Disconnected),
        [REPLY, 0xFC, 0x06, ..] => Some(Notice::Asleep),
        [REPLY, 0xFA, ..] => Some(Notice::Status),
        [REPLY, 0xFB, ..] => Some(Notice::Calibration),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices() {
        assert_eq!(notice(&[0x55, 0xFC, 0x05, 0]), Some(Notice::Reset));
        assert_eq!(notice(&[0x55, 0xFC, 0x04]), Some(Notice::Disconnected));
        assert_eq!(notice(&[0x55, 0xFC, 0x06]), Some(Notice::Asleep));
        assert_eq!(notice(&[0x55, 0xFA, 1]), Some(Notice::Status));
        assert_eq!(notice(&[0x55, 0xFB]), Some(Notice::Calibration));
        assert_eq!(notice(&[0x55, 0x10, 0x30]), None, "a reply");
        assert_eq!(notice(&[]), None);
    }
}
