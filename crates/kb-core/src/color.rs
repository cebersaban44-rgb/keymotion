//! Colour pipeline from UI colours (sRGB, as seen on a monitor) to LED PWM.
//!
//! LEDs respond linearly to the PWM value while monitors apply a gamma
//! curve, so mid values (e.g. the 0x5C green in violet #8B5CF6) look far
//! too bright on the keys and colours wash out. Pure colours (0/255 per
//! channel) are unaffected, which is why they looked right in tests.

use kb_protocol::sonix::Rgb;
use serde::{Deserialize, Serialize};

/// Supported gamma range. 1.0 sends raw values; 2.2 matched the UI palette
/// on the AK980 PRO (verified by eye on 2026-11-10).
pub const GAMMA_MIN: f32 = 1.0;
pub const GAMMA_MAX: f32 = 3.0;

/// Per-channel gamma and gain applied before colours are sent to LEDs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

    /// Clamp every value into its supported range; non-finite values fall
    /// back to the default so a damaged settings file cannot break colours.
    pub fn sanitized(self) -> Self {
        let fix = |v: f32, fallback: f32, lo: f32, hi: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            }
        };
        Self {
            gamma: [0, 1, 2].map(|i| fix(self.gamma[i], Self::DEFAULT.gamma[i], GAMMA_MIN, GAMMA_MAX)),
            gain: [0, 1, 2].map(|i| fix(self.gain[i], 1.0, 0.0, 1.0)),
        }
    }

    /// Monitor colour → LED value.
    pub fn apply(&self, c: Rgb) -> Rgb {
        self.apply_dimmed(c, 1.0)
    }

    /// Monitor colour → LED value, then scaled by a linear `level` (0–1).
    ///
    /// Brightness belongs after the gamma curve: dimming in sRGB first and
    /// then applying gamma would darken by `level^gamma` instead of `level`,
    /// so a live preview at level 3/5 would look far darker than the same
    /// design stored with the firmware's brightness setting.
    pub fn apply_dimmed(&self, c: Rgb, level: f32) -> Rgb {
        let s = self.sanitized();
        let level = if level.is_finite() { level.clamp(0.0, 1.0) } else { 1.0 };
        [0, 1, 2].map(|i| {
            let v = (c[i] as f32 / 255.0).powf(s.gamma[i]) * s.gain[i] * level;
            to_byte(v)
        })
    }

    /// LED value → monitor colour: how a read-back colour looks on the keys.
    /// Used by the mirror so per-key designs read back as they were painted.
    pub fn inverse(&self, c: Rgb) -> Rgb {
        let s = self.sanitized();
        [0, 1, 2].map(|i| {
            if s.gain[i] <= f32::EPSILON {
                return 0;
            }
            let v = ((c[i] as f32 / 255.0) / s.gain[i]).min(1.0).powf(1.0 / s.gamma[i]);
            to_byte(v)
        })
    }
}

fn to_byte(v: f32) -> u8 {
    (v * 255.0).round().clamp(0.0, 255.0) as u8
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
            assert_eq!(c.inverse(rgb), rgb);
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

    #[test]
    fn brightness_is_linear_after_gamma() {
        let c = LedCalibration::DEFAULT;
        // White at level 3/5 drives the LEDs at 60 %, not 0.6^2.2 ≈ 32 %.
        assert_eq!(c.apply_dimmed([255, 255, 255], 0.6), [153, 153, 153]);
        assert_eq!(c.apply_dimmed([0x8B, 0x5C, 0xF6], 1.0), c.apply([0x8B, 0x5C, 0xF6]));
        assert_eq!(c.apply_dimmed([255, 0, 0], 0.0), [0, 0, 0]);
    }

    #[test]
    fn inverse_round_trips_mid_tones() {
        let cals = [LedCalibration::DEFAULT, LedCalibration { gamma: [2.0, 2.4, 1.8], gain: [0.85, 1.0, 0.9] }];
        for cal in cals {
            // Low values are crushed by the curve (8-bit), so test mid tones.
            for rgb in [[0x8B, 0x5C, 0xF6], [0xFF, 0x8A, 0x3D], [0x80, 0x80, 0x80], [0xC0, 0x60, 0xA0]] {
                let back = cal.inverse(cal.apply(rgb));
                for i in 0..3 {
                    assert!(
                        (back[i] as i16 - rgb[i] as i16).abs() <= 3,
                        "{cal:?}: {rgb:?} → {:?} → {back:?}",
                        cal.apply(rgb)
                    );
                }
            }
        }
    }

    #[test]
    fn sanitized_repairs_bad_values() {
        let bad = LedCalibration { gamma: [f32::NAN, 9.0, 0.1], gain: [f32::INFINITY, -1.0, 2.0] };
        let s = bad.sanitized();
        assert_eq!(s.gamma, [2.2, GAMMA_MAX, GAMMA_MIN]);
        assert_eq!(s.gain, [1.0, 0.0, 1.0]);
        // A zero gain must not divide by zero in the inverse.
        assert_eq!(LedCalibration { gamma: [2.2; 3], gain: [0.0, 1.0, 1.0] }.inverse([200, 0, 0]), [0, 0, 0]);
    }
}
