//! The colours the keys show, from `04 F5` read-backs.
//!
//! The read-back reports red as `2·r mod 256`, so every red byte stands for
//! two values (see [`readback_red_candidates`]); green and blue are exact.
//! The host picks the red value that fits what it expects the keys to show
//! (the saved design, the effect colour, an indicator's colour) and falls
//! back to clues in the frame itself when it expects nothing.

use kb_protocol::sonix::{readback_red_candidates, Rgb};
use std::cmp::Ordering;

/// Distance from the frame's peak that still counts as "at the peak".
const PEAK_TOLERANCE: u8 = 8;
/// How far green and blue may stray from an expectation before it is
/// ignored (the keyboard shows something else, e.g. an effect chosen with
/// Fn keys).
const MATCH_TOLERANCE: f32 = 12.0;

/// Resolve the red channel of every LED in a read-back.
///
/// `expected(led)` is the LED value the host expects there (after
/// calibration and the brightness scale), if it knows one. `fallback_peak`
/// is the brightest value the keyboard should reach, used when no LED in
/// the frame has green or blue to measure it from.
pub fn decode_frame(raw: &[(u8, Rgb)], expected: impl Fn(u8) -> Option<Rgb>, fallback_peak: u8) -> Vec<(u8, Rgb)> {
    let frame_peak = raw.iter().map(|(_, [_, g, b])| *g.max(b)).max().unwrap_or(0);
    let peak = if frame_peak > 0 { frame_peak } else { fallback_peak };
    raw.iter().map(|&(led, c)| (led, decode(c, expected(led), peak))).collect()
}

/// Resolve the red channel of one read-back colour. See [`decode_frame`].
pub fn decode(raw: Rgb, expected: Option<Rgb>, peak: u8) -> Rgb {
    if raw == [0, 0, 0] {
        // Off. A pure red of exactly 128 reads the same, but is rare.
        return raw;
    }
    let [byte, g, b] = raw;
    let [lo, hi] = readback_red_candidates(byte);

    if let Some(want) = expected.and_then(|e| predicted_red(e, g, b)) {
        let r = if (f32::from(lo) - want).abs() <= (f32::from(hi) - want).abs() { lo } else { hi };
        return [r, g, b];
    }
    if byte == 0 {
        // A red of exactly 128 also reads 0; unexpected, dark is far likelier.
        return [0, g, b];
    }

    // Nothing usable expected. A lit LED's brightest channel usually sits at
    // the frame's peak, so prefer the red that makes that true.
    let top = g.max(b);
    let miss = |r: u8| r.max(top).abs_diff(peak);
    let r = match miss(lo).cmp(&miss(hi)) {
        Ordering::Less => lo,
        Ordering::Greater => hi,
        // Green or blue already sets the brightest channel. Only a whitish
        // colour (both at the peak) points to a bright red as well.
        Ordering::Equal if g.min(b).saturating_add(PEAK_TOLERANCE) >= peak => hi,
        Ordering::Equal => lo,
    };
    [r, g, b]
}

/// The red that expectation `e` predicts at the observed intensity, or
/// `None` when green and blue show that the key is not showing `e`.
fn predicted_red(e: Rgb, g: u8, b: u8) -> Option<f32> {
    let [er, eg, eb] = e.map(f32::from);
    let (g, b) = (f32::from(g), f32::from(b));
    let want = eg + eb;
    let seen = g + b;
    if want == 0.0 {
        // Pure red (or black) expected: green and blue must be dark. Its
        // intensity cannot be measured, so a dimmed pure red (Breath) may
        // pick the wrong half; static colours resolve exactly.
        return (er > 0.0 && seen == 0.0).then_some(er);
    }
    if seen == 0.0 {
        return None;
    }
    // Effects such as Breath dim the colour, so compare at the same intensity.
    let scale = seen / want;
    let fits = (g - eg * scale).abs() <= MATCH_TOLERANCE && (b - eb * scale).abs() <= MATCH_TOLERANCE;
    fits.then_some(er * scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `04 F5` returns for a driven colour.
    fn read([r, g, b]: Rgb) -> Rgb {
        [r.wrapping_mul(2), g, b]
    }

    #[test]
    fn expected_colours_resolve_red() {
        for c in [[137, 91, 244], [253, 253, 253], [127, 127, 127], [63, 63, 63], [253, 59, 47], [56, 182, 253]] {
            assert_eq!(decode(read(c), Some(c), 253), c, "{c:?}");
        }
    }

    #[test]
    fn dimmed_effect_still_matches_its_colour() {
        // Breath at half intensity: the expectation is the full colour.
        let full = [200, 100, 50];
        let half = [100, 50, 25];
        assert_eq!(decode(read(half), Some(full), 253), half);
    }

    #[test]
    fn pure_red_expected() {
        assert_eq!(decode(read([255, 0, 0]), Some([255, 0, 0]), 253), [255, 0, 0]);
        assert_eq!(decode(read([100, 0, 0]), Some([100, 0, 0]), 253), [100, 0, 0]);
    }

    #[test]
    fn mismatching_expectation_falls_back() {
        // The design says black but the Fn indicator glows red.
        assert_eq!(decode([0xFE, 0, 0], Some([0, 0, 0]), 253), [255, 0, 0]);
        // The design says blue but the keyboard shows orange.
        assert_eq!(decode(read([253, 120, 0]), Some([0, 0, 253]), 253), [253, 120, 0]);
    }

    #[test]
    fn fallback_follows_the_peak() {
        // Rainbow frame at full brightness: red is the brightest channel…
        assert_eq!(decode(read([253, 40, 0]), None, 253), [253, 40, 0]);
        // …or red is dark…
        assert_eq!(decode(read([0, 253, 120]), None, 253), [0, 253, 120]);
        // …and white stays white (Num Lock indicator).
        assert_eq!(decode([0xFE, 0xFF, 0xFF], None, 255), [255, 255, 255]);
        // When green already peaks, the lower red is taken.
        assert_eq!(decode(read([90, 253, 0]), None, 253), [90, 253, 0]);
    }

    #[test]
    fn frame_peak_comes_from_green_and_blue() {
        // A dim rainbow frame (Breath, low phase): peak 120, not the level's 253.
        let frame = [(1, read([120, 30, 0])), (2, read([0, 120, 60])), (3, read([10, 0, 120]))];
        let out = decode_frame(&frame, |_| None, 253);
        assert_eq!(out, vec![(1, [120, 30, 0]), (2, [0, 120, 60]), (3, [10, 0, 120])]);
        // Only red in the frame: the fallback peak decides.
        assert_eq!(decode_frame(&[(96, [0xFE, 0, 0])], |_| None, 253), vec![(96, [255, 0, 0])]);
    }

    #[test]
    fn black_stays_black() {
        assert_eq!(decode([0, 0, 0], None, 253), [0, 0, 0]);
        assert_eq!(decode([0, 0, 0], Some([0, 0, 0]), 253), [0, 0, 0]);
        assert_eq!(decode([0, 0, 0], Some([200, 0, 0]), 253), [0, 0, 0]);
    }

    #[test]
    fn red_128_reads_zero() {
        // Live grey #808080 read back as 00 80 80 (2026-11-10).
        assert_eq!(decode([0x00, 0x80, 0x80], Some([0x80, 0x80, 0x80]), 253), [0x80, 0x80, 0x80]);
        assert_eq!(decode([0x00, 0x80, 0x80], None, 253), [0, 0x80, 0x80]);
    }
}
