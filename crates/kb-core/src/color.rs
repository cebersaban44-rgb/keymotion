//! Colour pipeline from UI colours (sRGB, as seen on a monitor) to LED PWM.
//!
//! LEDs respond linearly to the PWM value while monitors apply a gamma
//! curve, so mid values (e.g. the 0x5C green in violet #8B5CF6) look far
//! too bright on the keys and colours wash out. Pure colours (0/255 per
//! channel) are unaffected, which is why they looked right in tests.

use kb_protocol::sonix::Rgb;

/// Per-channel gamma and gain applied before colours are sent to LEDs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LedCalibration {
    /// Exponent per channel (2.2 ≈ monitor gamma; 1.0 = raw).
    pub gamma: [f32; 3],
    /// Gain per channel, 0.0–1.0 (white balance).
    pub gain: [f32; 3],
}

impl LedCalibration {
    /// Raw values: no correction.
    pub const RAW: Self = Self { gamma: [1.0; 3], gain: [1.0; 3] };
    /// Monitor-like default: gamma 2.2 on every channel, no gain change.
    pub const DEFAULT: Self = Self { gamma: [2.2; 3], gain: [1.0; 3] };

    pub fn apply(&self, c: Rgb) -> Rgb {
        [0, 1, 2].map(|i| {
            let v = (c[i] as f32 / 255.0).powf(self.gamma[i].clamp(0.5, 4.0)) * self.gain[i].clamp(0.0, 1.0);
            (v * 255.0).round().clamp(0.0, 255.0) as u8
        })
    }
}

impl Default for LedCalibration {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_colours_are_unchanged() {
        let c = LedCalibration::DEFAULT;
        for rgb in [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255], [0, 0, 0]] {
            assert_eq!(c.apply(rgb), rgb);
        }
    }

    #[test]
    fn mid_values_are_darkened() {
        let c = LedCalibration::DEFAULT;
        // violet #8B5CF6
        assert_eq!(c.apply([0x8B, 0x5C, 0xF6]), [67, 27, 236]);
        assert_eq!(LedCalibration::RAW.apply([0x8B, 0x5C, 0xF6]), [0x8B, 0x5C, 0xF6]);
    }

    #[test]
    fn gain_scales_channels() {
        let c = LedCalibration { gamma: [1.0; 3], gain: [0.78, 1.0, 0.95] };
        assert_eq!(c.apply([255, 255, 255]), [199, 255, 242]);
    }
}
