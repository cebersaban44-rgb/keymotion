//! Device information (§4.1): `10`, 48 bytes, read only.
//!
//! Field offsets from AJAZZ's WebHID driver (`getDeviceInfo`); the values
//! of firmware 769 from a real reply (issue #966 of another project) and
//! from its code (protocol notes §F.7): the reply is a fixed block of flash
//! (`40 00 | 00 0C | 45 0C | 39 80 | 01 03 | FF FF 01 00 33 00`, zeros
//! after) with four bytes filled in on the fly: the work mode (16), the
//! battery (17), the charge state (18) and the screen frame limit (22,
//! always 146). There is no "waiting for an update" field: a keyboard
//! goes into its updater only on the updater's own command, and byte 32
//! ([`DeviceInfo::firmware_status`]) is just flash. It is logged, never
//! acted on.

use super::CodecError;

/// Bytes requested (the WebHID driver's LEN; the web hub asks for 56).
pub const LEN: usize = 48;
/// The screen frame limit firmware 769 reports (byte 22).
pub const FRAME_LIMIT_769: u16 = 146;
/// `tft_direction` of a keyboard without a screen.
pub const NO_SCREEN: u8 = 255;

/// The first 19 data bytes of a real reply from an AK980 PRO on firmware
/// 769, on the cable (#966 of another project; the rest was not shared).
pub const FIRMWARE_769_CABLE: [u8; 19] =
    [0x40, 0x00, 0x00, 0x0C, 0x45, 0x0C, 0x39, 0x80, 0x01, 0x03, 0xFF, 0xFF, 0x01, 0x00, 0x33, 0x00, 0x00, 0x64, 0x02];

/// Every field of the reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceInfo {
    pub rom_size: u8,
    /// Bytes of macro memory the firmware reports (3072 on 769). The driver
    /// keeps to the WebHID driver's 512 for this keyboard.
    pub macro_space_size: u16,
    pub vid: u16,
    pub pid: u16,
    /// Firmware version, as the USB `bcdDevice` (`0x0301` = 769).
    pub version: u16,
    pub sensor: u16,
    pub manufacturer: u16,
    /// Model code (`0x33` for the AK980 PRO on 769).
    pub product: u16,
    /// 0 cable, 2 receiver.
    pub work_mode: u8,
    /// Battery in percent, on the cable too.
    pub battery_level: u8,
    /// 2 on the cable, 0 over the receiver on 769; meaning not known.
    pub charge_status: u8,
    pub current_profile: u8,
    pub axis_info: u16,
    /// Most screen frames the firmware takes (0 = not reported).
    pub tft_max_frames: u16,
    pub gif_max_frames: u16,
    pub led_max_frames: u16,
    /// 255 = no screen.
    pub tft_direction: u8,
    pub rt_precision: u8,
    /// 1 = longer timeouts and FUNC_V2 entries (not this keyboard).
    pub frame_version: u8,
    pub lighting_version: u8,
    /// The WebHID driver's "firmware status" (1 = waiting for an update).
    /// Firmware 769 has no such field: this is a byte of its fixed flash
    /// block (0). Shown in reports, never acted on.
    pub firmware_status: u8,
    pub max_profile: u8,
    /// 0 = RGB565 little-endian.
    pub tft_color_byte_order: u8,
}

impl DeviceInfo {
    /// Parse the first 48 data bytes of the reply.
    pub fn parse(d: &[u8]) -> Result<Self, CodecError> {
        if d.len() < LEN {
            return Err(CodecError::Malformed("device information"));
        }
        let u16_at = |i: usize| u16::from_le_bytes([d[i], d[i + 1]]);
        Ok(Self {
            rom_size: d[0],
            macro_space_size: u16_at(2),
            vid: u16_at(4),
            pid: u16_at(6),
            version: u16_at(8),
            sensor: u16_at(10),
            manufacturer: u16_at(12),
            product: u16_at(14),
            work_mode: d[16],
            battery_level: d[17],
            charge_status: d[18],
            current_profile: d[19],
            axis_info: u16_at(20),
            tft_max_frames: u16_at(22),
            gif_max_frames: u16_at(24),
            led_max_frames: u16_at(26),
            tft_direction: d[28],
            rt_precision: d[29],
            frame_version: d[30],
            lighting_version: d[31],
            firmware_status: d[32],
            max_profile: d[33],
            tft_color_byte_order: d[35],
        })
    }

    pub fn has_screen(&self) -> bool {
        self.tft_direction != NO_SCREEN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_real_769_reply() {
        let mut d = [0u8; LEN];
        d[..FIRMWARE_769_CABLE.len()].copy_from_slice(&FIRMWARE_769_CABLE);
        let i = DeviceInfo::parse(&d).unwrap();
        assert_eq!(i.rom_size, 0x40);
        assert_eq!(i.macro_space_size, 3072);
        assert_eq!((i.vid, i.pid), (0x0C45, 0x8039));
        assert_eq!(i.version, 0x0301, "769, the USB bcdDevice");
        assert_eq!((i.sensor, i.manufacturer, i.product), (0xFFFF, 1, 0x33));
        assert_eq!((i.work_mode, i.battery_level, i.charge_status), (0, 100, 2));
        assert_eq!((i.tft_max_frames, i.firmware_status), (0, 0));
        assert!(i.has_screen());
    }

    #[test]
    fn parses_the_real_3_02_reply_with_its_frame_limit() {
        // The whole reply of a patched 769 ("770", 0x0302) from issue #154:
        // the frame limit 146 (0x92) the firmware writes into byte 22.
        let mut d = [0u8; LEN];
        d[..23].copy_from_slice(&[
            0x40, 0x00, 0x00, 0x0C, 0x45, 0x0C, 0x39, 0x80, 0x02, 0x03, 0xFF, 0xFF, 0x01, 0x00, 0x33, 0x00, 0x00, 0x64,
            0x02, 0x00, 0x00, 0x00, 0x92,
        ]);
        let i = DeviceInfo::parse(&d).unwrap();
        assert_eq!((i.version, i.tft_max_frames, i.firmware_status), (0x0302, FRAME_LIMIT_769, 0));
        // Byte 12 is always 1 (the manufacturer's low byte): not a status.
        assert_eq!(i.manufacturer, 1);
    }

    #[test]
    fn every_field_has_its_offset() {
        let d: Vec<u8> = (0..LEN as u8).collect();
        let i = DeviceInfo::parse(&d).unwrap();
        assert_eq!((i.rom_size, i.macro_space_size, i.vid, i.pid), (0, 0x0302, 0x0504, 0x0706));
        assert_eq!((i.version, i.sensor, i.manufacturer, i.product), (0x0908, 0x0B0A, 0x0D0C, 0x0F0E));
        assert_eq!((i.work_mode, i.battery_level, i.charge_status, i.current_profile), (16, 17, 18, 19));
        assert_eq!(
            (i.axis_info, i.tft_max_frames, i.gif_max_frames, i.led_max_frames),
            (0x1514, 0x1716, 0x1918, 0x1B1A)
        );
        assert_eq!((i.tft_direction, i.rt_precision, i.frame_version, i.lighting_version), (28, 29, 30, 31));
        assert_eq!((i.firmware_status, i.max_profile, i.tft_color_byte_order), (32, 33, 35));
        assert!(DeviceInfo::parse(&d[..47]).is_err());
        let mut none = [0u8; LEN];
        none[28] = NO_SCREEN;
        assert!(!DeviceInfo::parse(&none).unwrap().has_screen());
    }
}
