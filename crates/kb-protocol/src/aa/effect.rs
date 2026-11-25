//! The lighting effect (§4.3): `13` read, `23` write, 16 bytes.
//!
//! `MODE R G B FF 00 00 00 COLORMODE BRIGHTNESS SPEED DIRECTION 00 00 AA 55`.
//! Mode numbers are the "04 xx" family's (0 off, 1 static … 19 shuttle,
//! 128 per-key), plus 20 and 21 (which also show the per-key table).
//! Brightness and speed are 1–5 here (0–5 in the "04 xx" family): the app's
//! levels go through one table, [`LEVELS`], and nothing outside 1–5 is ever
//! written.
//!
//! A read does not give back what was written (protocol notes §F.6): it
//! reports the effect the keyboard runs, through a map that turns modes 0,
//! 20, 21 and 128 all into 0 ([`READS_AS_OFF`]), and a custom colour (any
//! colour an effect write sets) as `FF FF FF` with colour mode 1
//! ([`READBACK_QUIRK`]). So a read-back confirms levels and direction, and
//! the mode only up to that ambiguity ([`AaEffect::matches_readback`]); the
//! colour never.

use super::{range, CodecError};
use crate::sonix::{EffectSettings, Rgb};

/// Bytes in the effect block.
pub const LEN: usize = 16;
/// Per-key mode: the per-key table is shown.
pub const MODE_PER_KEY: u8 = 128;
/// Modes that show the per-key table (20 and 21 fall through to the same
/// renderer as 128 on firmware 769).
pub const PER_KEY_MODES: [u8; 3] = [20, 21, MODE_PER_KEY];
/// Modes a read reports as 0: off and the three per-key modes. A read of 0
/// is one of them; which one, the keyboard does not tell.
pub const READS_AS_OFF: [u8; 4] = [0, 20, 21, MODE_PER_KEY];
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

/// Whether a read that reports mode `read` can follow a write of mode
/// `written`: the same mode, or 0 for any mode in [`READS_AS_OFF`].
pub fn mode_reads_as(written: u8, read: u8) -> bool {
    written == read || (read == 0 && READS_AS_OFF.contains(&written))
}

/// Whether the effect colour shows in mode `m` (not off, not a per-key
/// mode): only then does a write need it.
pub fn shows_color(m: u8) -> bool {
    !READS_AS_OFF.contains(&m)
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

/// What a read shows while the effect colour is a custom one, which every
/// effect write (by any program, in any session) makes it: white and
/// colour mode 1, whatever the keyboard shows. Only a colour picked with
/// the keyboard's own Fn keys (one of its palette) reads back as itself.
pub const READBACK_QUIRK: EffectColor = EffectColor { color: [0xFF, 0xFF, 0xFF], color_mode: 1 };

/// The colour part of an effect: the colour and the colour mode (1 =
/// rainbow, 0 = one colour). A read-back cannot always tell it
/// ([`READBACK_QUIRK`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectColor {
    pub color: Rgb,
    pub color_mode: u8,
}

impl EffectColor {
    /// The app's colour and rainbow flag.
    pub fn from_settings(s: &EffectSettings) -> Self {
        Self { color: s.color, color_mode: u8::from(s.rainbow) }
    }
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

    /// The same mode, levels and direction (the colour aside): two effects
    /// both known, such as one written and one remembered.
    pub fn same_settings(&self, other: &Self) -> bool {
        (self.mode, self.brightness, self.speed, self.direction)
            == (other.mode, other.brightness, other.speed, other.direction)
    }

    /// Whether `read`, an effect as the keyboard reports it, is what this
    /// effect (written, or known otherwise) reads back as: levels and
    /// direction equal, and the mode equal up to [`READS_AS_OFF`]. The
    /// colour is not compared (it does not read back).
    pub fn matches_readback(&self, read: &Self) -> bool {
        mode_reads_as(self.mode, read.mode)
            && (self.brightness, self.speed, self.direction) == (read.brightness, read.speed, read.direction)
    }

    /// Whether this mode shows the per-key table (20, 21, 128).
    pub fn shows_per_key(&self) -> bool {
        PER_KEY_MODES.contains(&self.mode)
    }

    /// The colour and colour mode.
    pub fn color_part(&self) -> EffectColor {
        EffectColor { color: self.color, color_mode: self.color_mode }
    }

    /// This effect with another colour and colour mode.
    pub fn with_color(self, c: EffectColor) -> Self {
        Self { color: c.color, color_mode: c.color_mode, ..self }
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
        assert!(e.shows_per_key());
        let rolling = AaEffect::from_settings(&EffectSettings::factory()).unwrap();
        assert!(!rolling.shows_per_key());
        let pk = rolling.per_key();
        assert_eq!((pk.mode, pk.color_mode, pk.brightness, pk.speed), (128, 0, 5, 3));
    }

    #[test]
    fn readback_compares_mode_and_levels_only() {
        let written = AaEffect::from_settings(&EffectSettings::factory()).unwrap();
        // The colour comes back FF FF FF with colour mode 1 after a write.
        let read = AaEffect { color: [0xFF; 3], color_mode: 1, ..written };
        assert!(written.same_settings(&read));
        assert_eq!(read.color_part(), READBACK_QUIRK);
        assert_eq!(read.with_color(written.color_part()), written);
        let pink = EffectSettings { color: [0xFF, 0x69, 0xB4], rainbow: false, ..EffectSettings::factory() };
        assert_eq!(EffectColor::from_settings(&pink), EffectColor { color: [0xFF, 0x69, 0xB4], color_mode: 0 });
        assert!(!written.same_settings(&AaEffect { brightness: 4, ..written }));
        assert!(AaEffect::decode(&[0; 15]).is_err());
    }

    #[test]
    fn off_and_the_per_key_modes_all_read_back_as_0() {
        // Firmware 769 reports the running effect through a reverse map that
        // sends 0, 20, 21 and 128 to 0 (§F.6).
        let base = AaEffect::from_settings(&EffectSettings::factory()).unwrap();
        for m in READS_AS_OFF {
            let written = AaEffect { mode: m, ..base };
            let read = AaEffect { mode: 0, color: [0xFF; 3], color_mode: 1, ..base };
            assert!(written.matches_readback(&read), "{m}");
            assert!(mode_reads_as(m, 0) && mode_reads_as(m, m));
            assert!(!shows_color(m));
            assert_eq!(written.same_settings(&read), m == 0, "{m}: not the same as known");
        }
        // Any other mode reads back as itself.
        for m in (1..=19).chain([22]) {
            assert!(!mode_reads_as(m, 0), "{m}");
            assert!(shows_color(m));
        }
        let static_one = AaEffect { mode: 1, ..base };
        assert!(static_one.matches_readback(&AaEffect { mode: 1, color: [0xFF; 3], ..base }));
        assert!(!static_one.matches_readback(&AaEffect { mode: 0, ..base }));
        // Levels and direction still count.
        let pk = AaEffect { mode: 128, ..base };
        assert!(!pk.matches_readback(&AaEffect { mode: 0, brightness: 1, ..base }));
        assert!(!pk.matches_readback(&AaEffect { mode: 0, direction: 2, ..base }));
    }
}
