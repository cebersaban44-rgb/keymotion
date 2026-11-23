//! The lighting effect (§4.3): `13` read, `23` write, 16 bytes.
//!
//! `MODE R G B FF 00 00 00 COLORMODE BRIGHTNESS SPEED DIRECTION 00 00 AA 55`.
//! Mode numbers are the "04 xx" family's (0 off, 1 static … 19 shuttle,
//! 128 per-key), plus 20 and 21 (which also show the per-key table).
//! Brightness and speed are 1–5 here (0–5 in the "04 xx" family): the app's
//! levels go through one table, [`LEVELS`], and nothing outside 1–5 is ever
//! written. The colour read back after a write is not reliable (it comes
//! back `FF FF FF`), so only mode and levels are compared after a write.

use super::{range, CodecError};
use crate::sonix::{EffectSettings, Rgb};

/// Bytes in the effect block.
pub const LEN: usize = 16;
/// Per-key mode: the per-key table is shown.
pub const MODE_PER_KEY: u8 = 128;
/// Modes in which real-time frames are visible (and the per-key table is shown).
pub const LIVE_MODES: [u8; 3] = [20, 21, MODE_PER_KEY];
/// Byte 4, as both AJAZZ tools write it.
pub const DRIVER_SETTING: u8 = 0xFF;
/// Bytes 14–15, as AJAZZ's WebHID driver writes them for this keyboard.
pub const TAIL: [u8; 2] = [0xAA, 0x55];
/// Lowest and highest brightness and speed level the keyboard takes.
pub const MIN_LEVEL: u8 = 1;
pub const MAX_LEVEL: u8 = 5;
/// Highest direction code.
pub const MAX_DIRECTION: u8 = 3;
/// The app's level (0–5, the "04 xx" range) → the keyboard's (1–5). Level
/// 0 has no counterpart and becomes 1; whether 1 is dim or off on this
/// keyboard is not measured yet.
pub const LEVELS: [u8; 6] = [1, 1, 2, 3, 4, 5];

/// Whether the keyboard has effect mode `m`.
pub fn is_valid_mode(m: u8) -> bool {
    m <= 21 || m == MODE_PER_KEY
}

/// The keyboard's level for an app level (0–5).
pub fn level(app: u8) -> Result<u8, CodecError> {
    LEVELS.get(usize::from(app)).copied().ok_or(CodecError::Range {
        field: "level",
        value: u32::from(app),
        min: 0,
        max: 5,
    })
}

/// The effect block's fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AaEffect {
    pub mode: u8,
    pub color: Rgb,
    /// 1 = rainbow, 0 = one colour (always 0 in per-key mode).
    pub color_mode: u8,
    /// 1–5.
    pub brightness: u8,
    /// 1–5.
    pub speed: u8,
    /// 0–3, written as given.
    pub direction: u8,
}

impl AaEffect {
    /// The app's effect, with its levels mapped through [`LEVELS`].
    pub fn from_settings(s: &EffectSettings) -> Result<Self, CodecError> {
        if !is_valid_mode(s.mode) {
            return Err(CodecError::Mode(s.mode));
        }
        range("direction", u32::from(s.direction), 0, u32::from(MAX_DIRECTION))?;
        let e = Self {
            mode: s.mode,
            color: s.color,
            color_mode: u8::from(s.rainbow && s.mode != MODE_PER_KEY),
            brightness: level(s.brightness)?,
            speed: level(s.speed)?,
            direction: s.direction,
        };
        e.validate()?;
        Ok(e)
    }

    /// The effect as the app describes it. Levels above 5 read as 5.
    pub fn to_settings(&self) -> EffectSettings {
        EffectSettings {
            mode: self.mode,
            color: self.color,
            rainbow: self.color_mode == 1,
            brightness: self.brightness.min(MAX_LEVEL),
            speed: self.speed.min(MAX_LEVEL),
            direction: self.direction,
        }
    }

    /// Check every field is in the keyboard's range.
    pub fn validate(&self) -> Result<(), CodecError> {
        if !is_valid_mode(self.mode) {
            return Err(CodecError::Mode(self.mode));
        }
        range("colour mode", u32::from(self.color_mode), 0, 1)?;
        range("brightness", u32::from(self.brightness), u32::from(MIN_LEVEL), u32::from(MAX_LEVEL))?;
        range("speed", u32::from(self.speed), u32::from(MIN_LEVEL), u32::from(MAX_LEVEL))?;
        range("direction", u32::from(self.direction), 0, u32::from(MAX_DIRECTION))
    }

    /// The block to write. Per-key mode always goes with colour mode 0.
    pub fn encode(&self) -> Result<[u8; LEN], CodecError> {
        self.validate()?;
        let [r, g, b] = self.color;
        let color_mode = if self.mode == MODE_PER_KEY { 0 } else { self.color_mode };
        Ok([
            self.mode,
            r,
            g,
            b,
            DRIVER_SETTING,
            0,
            0,
            0,
            color_mode,
            self.brightness,
            self.speed,
            self.direction,
            0,
            0,
            TAIL[0],
            TAIL[1],
        ])
    }

    /// Read a block as it is (no range check: see [`Self::validate`]).
    pub fn decode(b: &[u8]) -> Result<Self, CodecError> {
        if b.len() < LEN {
            return Err(CodecError::Malformed("effect block"));
        }
        Ok(Self {
            mode: b[0],
            color: [b[1], b[2], b[3]],
            color_mode: b[8],
            brightness: b[9],
            speed: b[10],
            direction: b[11],
        })
    }

    /// What a read-back can confirm: mode and levels (not the colour).
    pub fn same_settings(&self, other: &Self) -> bool {
        (self.mode, self.brightness, self.speed, self.direction)
            == (other.mode, other.brightness, other.speed, other.direction)
    }

    /// Whether real-time frames show in this mode.
    pub fn is_live(&self) -> bool {
        LIVE_MODES.contains(&self.mode)
    }

    /// This effect switched to per-key mode, everything else kept.
    pub fn per_key(&self) -> Self {
        Self { mode: MODE_PER_KEY, color_mode: 0, ..*self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_ak980_max_capture() {
        // S12 §9.7 (AK980 MAX, written and read back on hardware):
        // `09 FF B0 B0 FF 00 00 00 00 06 03 00 00 00 AA 55`. Brightness 6 is
        // outside this keyboard's range, so the encoder refuses it ...
        let golden = [0x09, 0xFF, 0xB0, 0xB0, 0xFF, 0, 0, 0, 0, 0x06, 0x03, 0, 0, 0, 0xAA, 0x55];
        let e = AaEffect::decode(&golden).unwrap();
        assert_eq!((e.mode, e.color, e.color_mode, e.brightness, e.speed), (9, [0xFF, 0xB0, 0xB0], 0, 6, 3));
        assert!(e.encode().is_err());
        // ... and at 5 the bytes are the capture's but for that one.
        let mut want = golden;
        want[9] = 5;
        assert_eq!(AaEffect { brightness: 5, ..e }.encode(), Ok(want));
    }

    #[test]
    fn hub_default_and_app_levels() {
        // The web hub's reset effect: static, white, rainbow, brightness 5,
        // speed 3 (`01 FF FF FF FF 00 00 00 01 05 03 00`).
        let s = EffectSettings { mode: 1, color: [0xFF; 3], rainbow: true, brightness: 5, speed: 3, direction: 0 };
        let b = AaEffect::from_settings(&s).unwrap().encode().unwrap();
        assert_eq!(b, [1, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 1, 5, 3, 0, 0, 0, 0xAA, 0x55]);
        assert_eq!(AaEffect::decode(&b).unwrap().to_settings(), s);
        // Every app level lands in 1–5; level 0 becomes 1.
        let levels: Vec<u8> = (0..=5).map(|l| level(l).unwrap()).collect();
        assert_eq!(levels, [1, 1, 2, 3, 4, 5]);
        assert!(level(6).is_err());
        let dim = AaEffect::from_settings(&EffectSettings { brightness: 0, speed: 0, ..s }).unwrap();
        assert_eq!((dim.brightness, dim.speed), (1, 1));
        // Direction goes as given (0–3).
        let d = AaEffect::from_settings(&EffectSettings { direction: 3, ..s }).unwrap();
        assert_eq!(d.encode().unwrap()[11], 3);
        assert!(AaEffect::from_settings(&EffectSettings { direction: 4, ..s }).is_err());
    }

    #[test]
    fn modes() {
        for m in (0..=21).chain([128]) {
            assert!(is_valid_mode(m), "{m}");
        }
        for m in [22u8, 100, 127, 129, 255] {
            assert!(!is_valid_mode(m), "{m}");
            let s = EffectSettings { mode: m, ..EffectSettings::factory() };
            assert_eq!(AaEffect::from_settings(&s), Err(CodecError::Mode(m)));
        }
        // Per-key mode drops the rainbow flag.
        let s = EffectSettings { mode: 128, rainbow: true, ..EffectSettings::factory() };
        let e = AaEffect::from_settings(&s).unwrap();
        assert_eq!(e.encode().unwrap()[8], 0);
        assert!(e.is_live());
        let rolling = AaEffect::from_settings(&EffectSettings::factory()).unwrap();
        assert!(!rolling.is_live());
        let pk = rolling.per_key();
        assert_eq!((pk.mode, pk.color_mode, pk.brightness, pk.speed), (128, 0, 5, 3));
    }

    #[test]
    fn readback_compares_mode_and_levels_only() {
        let written = AaEffect::from_settings(&EffectSettings::factory()).unwrap();
        // The colour comes back FF FF FF with colour mode 1 after a write.
        let read = AaEffect { color: [0xFF; 3], color_mode: 1, ..written };
        assert!(written.same_settings(&read));
        assert!(!written.same_settings(&AaEffect { brightness: 4, ..written }));
        assert!(AaEffect::decode(&[0; 15]).is_err());
    }
}
