//! Sound to light levels, for the music lights: how high each frequency
//! band is, the peak marks above them, the overall loudness and the beats.
//!
//! Pure arithmetic: mono samples in ([`Analyzer::push`], from
//! `kb_winsys::loopback`), numbers out ([`Analyzer::analyze`], once per
//! light frame); no I/O, no clock of its own (the caller passes `now`, in
//! seconds), so every behaviour is tested with made-up sound.
//!
//! * Spectrum: the last ~43 ms of samples ([`fft_len`]), Hann window, real
//!   FFT (`realfft`). Bands spaced evenly in octaves from [`LOW_HZ`] to
//!   [`HIGH_HZ`] ([`band_edges`]), bass first, each at least one FFT bin.
//!   Treble is weaker in music, so each octave above 1 kHz gets 3 dB more
//!   (and each below 3 dB less): the right-hand bars don't stay dim.
//! * Heights: dB below the automatic gain's reference, over
//!   [`BAND_RANGE_DB`]; below [`GATE_DB`] a band is silent. The reference
//!   follows the loudest band up at once and a quieter sound down by
//!   halving the gap every [`AGC_HALF_LIFE`] s, so the bars look full at
//!   any volume. Sensitivity (50–200 %) moves it.
//! * Motion: a bar rises at once and falls from full to empty in
//!   [`RELEASE_SECS`]; its peak mark stays [`PEAK_HOLD_SECS`], then falls.
//! * Beats: the bass (40–150 Hz) level against its average over the last
//!   0.4 s; a beat is a rise over an adaptive threshold (the recent rises'
//!   mean plus a multiple of their spread, and never less than a few dB),
//!   at most one per [`MIN_BEAT_GAP`]. An energy version of the spectral
//!   flux of Bello et al., "A Tutorial on Onset Detection in Music
//!   Signals" (2005), steadier on noise with the few bass bins a short
//!   window has.
//!
//! Only levels leave this module; the samples are overwritten as new ones
//! come in.

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::collections::VecDeque;
use std::sync::Arc;

/// The bands span this range (Hz).
pub const LOW_HZ: f32 = 40.0;
pub const HIGH_HZ: f32 = 16_000.0;
/// Beats are looked for in this range (Hz).
pub const BASS_HZ: (f32, f32) = (40.0, 150.0);
/// Analysis window, about this long (s).
const WINDOW_SECS: f64 = 0.043;
/// A band, the loudness or the bass below this is silence (dBFS).
pub const GATE_DB: f32 = -60.0;
/// The bass must be at least this loud for a beat (dBFS).
const BEAT_GATE_DB: f32 = -50.0;
/// Per octave away from [`TILT_REF_HZ`] (dB).
const TILT_DB_PER_OCTAVE: f32 = 3.0;
const TILT_REF_HZ: f32 = 1_000.0;
/// A bar spans this many dB below the reference.
pub const BAND_RANGE_DB: f32 = 40.0;
/// The loudness spans this many dB below its reference.
pub const LOUD_RANGE_DB: f32 = 24.0;
/// The automatic gain halves the gap to a quieter sound every this many
/// seconds.
pub const AGC_HALF_LIFE: f64 = 3.0;
/// The reference never goes below this (dBFS): quiet is not turned up
/// into noise.
const AGC_FLOOR_DB: f32 = -45.0;
/// Each doubling of the sensitivity (percent; 100 is the middle) moves the
/// reference this much (dB): higher is fuller.
const SENSITIVITY_DB_PER_DOUBLING: f32 = 12.0;
pub const DEFAULT_SENSITIVITY: u16 = 100;
pub const MIN_SENSITIVITY: u16 = 50;
pub const MAX_SENSITIVITY: u16 = 200;
/// A bar falls from full to empty in this long (s).
pub const RELEASE_SECS: f32 = 0.25;
/// A peak mark stays this long (s)…
pub const PEAK_HOLD_SECS: f64 = 0.4;
/// …then falls this much per second.
pub const PEAK_FALL_PER_SEC: f32 = 1.25;
/// The bass is compared with its average over this long (s).
const ONSET_WINDOW: f64 = 0.4;
/// The threshold comes from the rises of this long (s).
const THRESHOLD_WINDOW: f64 = 1.5;
/// At most one beat per this long (s): one drum hit is one beat.
pub const MIN_BEAT_GAP: f64 = 0.15;
/// A longer gap between two analyses is taken as this long (s).
const MAX_STEP: f64 = 0.5;
/// What silence reads as (dB).
const SILENT_DB: f32 = -120.0;

/// One analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct Levels {
    /// Each band's height (0–1), bass first.
    pub bands: Vec<f32>,
    /// Each band's peak mark (0–1), never under the band.
    pub peaks: Vec<f32>,
    /// Overall loudness (0–1), with its own automatic gain.
    pub loud: f32,
    /// When the last beat was (the `now` of that analysis).
    pub beat: Option<f64>,
    /// The window's RMS level (dBFS; −120 for silence), for "is anything
    /// playing".
    pub db: f32,
}

/// Window length for a sample rate: the power of two nearest ~43 ms
/// (2048 at 44.1 and 48 kHz, 4096 at 96 kHz, 8192 at 192 kHz).
pub fn fft_len(rate: u32) -> usize {
    let target = (f64::from(rate) * WINDOW_SECS).max(1.0);
    let below = 1usize << (target.log2().floor() as u32);
    let len = if target / below as f64 > std::f64::consts::SQRT_2 { below * 2 } else { below };
    len.clamp(256, 16_384)
}

/// The FFT bin of a frequency.
fn bin(hz: f32, rate: u32, fft_len: usize) -> usize {
    (hz * fft_len as f32 / rate as f32).round().max(0.0) as usize
}

/// `n` bands from [`LOW_HZ`] to [`HIGH_HZ`] (or just under half the rate),
/// evenly spaced in octaves, as `n + 1` FFT bin edges: band `i` is bins
/// `edges[i]..edges[i + 1]`. Every band has at least one bin; narrow bass
/// bands are widened (and the ones above moved up) to get it.
pub fn band_edges(rate: u32, fft_len: usize, n: usize) -> Vec<usize> {
    let n = n.max(1);
    // Exclusive end: the last bin is the one at half the rate.
    let end = fft_len / 2 + 1;
    let high = HIGH_HZ.min(rate as f32 * 0.475);
    let low = LOW_HZ.min(high / 2.0);
    let mut edges: Vec<usize> = (0..=n)
        .map(|i| {
            let hz = low * (high / low).powf(i as f32 / n as f32);
            bin(hz, rate, fft_len).max(1)
        })
        .collect();
    for i in 1..=n {
        edges[i] = edges[i].max(edges[i - 1] + 1);
    }
    // Pushed past the end (tiny windows only): back down from the top.
    edges[n] = edges[n].min(end);
    for i in (0..n).rev() {
        edges[i] = edges[i].min(edges[i + 1].saturating_sub(1));
    }
    edges
}

/// Doublings of the sensitivity from the middle: −1 at 50 %, 1 at 200 %.
fn doublings(sensitivity: u16) -> f32 {
    (f32::from(sensitivity.clamp(MIN_SENSITIVITY, MAX_SENSITIVITY)) / f32::from(DEFAULT_SENSITIVITY)).log2()
}

fn to_db(power: f32) -> f32 {
    if power > 1e-12 && power.is_finite() {
        10.0 * power.log10()
    } else {
        SILENT_DB
    }
}

/// The automatic gain's reference (dB): up at once, down by halving the
/// gap every [`AGC_HALF_LIFE`].
#[derive(Debug, Clone, Copy)]
struct Agc(f32);

impl Agc {
    fn follow(&mut self, level: f32, dt: f64) -> f32 {
        let target = level.max(AGC_FLOOR_DB);
        if target >= self.0 {
            self.0 = target;
        } else {
            let keep = 0.5f64.powf(dt / AGC_HALF_LIFE) as f32;
            self.0 = target + (self.0 - target) * keep;
        }
        self.0
    }
}

/// A falling bar or mark this close to where it is going is there (so an
/// empty bar is exactly 0, whatever the rounding of the time steps).
const SETTLE: f32 = 1e-4;

/// Down from `current` by `fall`, not below `target`.
fn fall_to(current: f32, target: f32, fall: f32) -> f32 {
    let v = current - fall;
    if v - target < SETTLE {
        target
    } else {
        v
    }
}

/// A bar: rises at once, falls at a steady rate.
fn smooth(current: f32, target: f32, dt: f32) -> f32 {
    if target >= current {
        target
    } else {
        fall_to(current, target, dt / RELEASE_SECS)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Peak {
    level: f32,
    hold_until: f64,
}

impl Peak {
    fn step(&mut self, bar: f32, now: f64, dt: f32) -> f32 {
        if bar >= self.level {
            self.level = bar;
            self.hold_until = now + PEAK_HOLD_SECS;
        } else if now >= self.hold_until {
            self.level = fall_to(self.level, bar, PEAK_FALL_PER_SEC * dt);
        }
        self.level
    }
}

/// The beat finder (see the module notes).
#[derive(Debug, Clone, Default)]
struct Beats {
    /// (time, bass power) of the analyses in the last [`ONSET_WINDOW`].
    recent: VecDeque<(f64, f32)>,
    /// (time, rise in dB) in the last [`THRESHOLD_WINDOW`].
    rises: VecDeque<(f64, f32)>,
    /// The history starts here (no beat until it is [`ONSET_WINDOW`] long).
    since: Option<f64>,
    last: Option<f64>,
}

impl Beats {
    /// Forget the history (a gap in the sound): what comes next is not
    /// compared with what was before, so the gap is no beat.
    fn reset(&mut self) {
        self.recent.clear();
        self.rises.clear();
        self.since = None;
    }

    /// One analysis' bass level (dBFS). True on a beat.
    fn step(&mut self, bass_db: f32, now: f64, sensitivity: u16) -> bool {
        if self.since.is_some_and(|s| now < s) || self.last.is_some_and(|t| now < t) {
            // The clock went back: start over.
            self.reset();
            self.last = None;
        }
        let since = *self.since.get_or_insert(now);
        while self.recent.front().is_some_and(|&(t, _)| now - t > ONSET_WINDOW) {
            self.recent.pop_front();
        }
        while self.rises.front().is_some_and(|&(t, _)| now - t > THRESHOLD_WINDOW) {
            self.rises.pop_front();
        }
        let level = bass_db.max(GATE_DB);
        let rise = match self.recent.len() {
            0 => 0.0,
            len => (level - to_db(self.recent.iter().map(|r| r.1).sum::<f32>() / len as f32)).max(0.0),
        };
        let (mean, spread) = match self.rises.len() {
            0 => (0.0, 0.0),
            len => {
                let mean = self.rises.iter().map(|r| r.1).sum::<f32>() / len as f32;
                let var = self.rises.iter().map(|r| (r.1 - mean).powi(2)).sum::<f32>() / len as f32;
                (mean, var.sqrt())
            }
        };
        // Higher sensitivity: a smaller rise counts (6.5 dB and 1.5 spreads
        // at 100 %; 4 dB and 1 spread at 200 %; 9 dB and 2 spreads at 50 %).
        let s = doublings(sensitivity);
        let threshold = (6.5 - 2.5 * s).max(mean + (1.5 - 0.5 * s) * spread);
        let beat = now - since >= ONSET_WINDOW
            && bass_db >= BEAT_GATE_DB
            && rise >= threshold
            && self.last.is_none_or(|t| now - t >= MIN_BEAT_GAP);
        self.recent.push_back((now, 10f32.powf(level / 10.0)));
        self.rises.push_back((now, rise));
        if beat {
            self.last = Some(now);
        }
        beat
    }
}

/// Turns samples into [`Levels`]. Make a new one when the sample rate
/// changes.
pub struct Analyzer {
    rate: u32,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    /// `4 / (n · Σw²)`: a full-scale sine's bins sum to 1 (0 dB) with it.
    power_scale: f32,
    /// The last `fft_len` samples; `pos` is the oldest.
    ring: Vec<f32>,
    pos: usize,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    edges: Vec<usize>,
    /// Per band (dB).
    tilt: Vec<f32>,
    bass: (usize, usize),
    sensitivity: u16,
    band_ref: Agc,
    loud_ref: Agc,
    bands: Vec<f32>,
    peaks: Vec<Peak>,
    loud: f32,
    beats: Beats,
    last: Option<f64>,
}

impl Analyzer {
    /// For `rate` Hz and `bands` bands (the keyboard's columns).
    pub fn new(rate: u32, bands: usize) -> Self {
        let rate = rate.max(1);
        let len = fft_len(rate);
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(len);
        let window: Vec<f32> = (0..len)
            .map(|i| {
                let x = std::f32::consts::TAU * i as f32 / len as f32;
                0.5 - 0.5 * x.cos()
            })
            .collect();
        let energy: f32 = window.iter().map(|w| w * w).sum();
        let edges = band_edges(rate, len, bands);
        let hz = |b: usize| b as f32 * rate as f32 / len as f32;
        let tilt = edges
            .windows(2)
            .map(|e| {
                let center = (hz(e[0]).max(1.0) * hz(e[1]).max(1.0)).sqrt();
                TILT_DB_PER_OCTAVE * (center / TILT_REF_HZ).log2()
            })
            .collect();
        let bass_lo = bin(BASS_HZ.0, rate, len).max(1);
        let bass = (bass_lo, bin(BASS_HZ.1, rate, len).max(bass_lo + 1).min(len / 2 + 1));
        let n = edges.len() - 1;
        Self {
            rate,
            input: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            window,
            power_scale: 4.0 / (len as f32 * energy),
            ring: vec![0.0; len],
            pos: 0,
            edges,
            tilt,
            bass,
            sensitivity: DEFAULT_SENSITIVITY,
            band_ref: Agc(AGC_FLOOR_DB),
            loud_ref: Agc(AGC_FLOOR_DB),
            bands: vec![0.0; n],
            peaks: vec![Peak::default(); n],
            loud: 0.0,
            beats: Beats::default(),
            last: None,
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// In percent, 50–200; 100 is the middle.
    pub fn set_sensitivity(&mut self, sensitivity: u16) {
        self.sensitivity = sensitivity.clamp(MIN_SENSITIVITY, MAX_SENSITIVITY);
    }

    /// New samples (mono, −1…1). A broken value counts as silence.
    pub fn push(&mut self, samples: &[f32]) {
        let len = self.ring.len();
        let skip = samples.len().saturating_sub(len);
        for &s in &samples[skip..] {
            self.ring[self.pos] = if s.is_finite() { s } else { 0.0 };
            self.pos = (self.pos + 1) % len;
        }
    }

    /// A gap in the sound (a new stream, another device, a glitch): the
    /// samples and the beat history are dropped; the bars and the gain
    /// carry on, so the lights don't jump.
    pub fn reset(&mut self) {
        self.ring.fill(0.0);
        self.pos = 0;
        self.beats.reset();
    }

    /// The levels now (`now` in seconds, from any fixed start; it should
    /// not go back).
    pub fn analyze(&mut self, now: f64) -> Levels {
        let dt = match self.last {
            Some(t) if now >= t => (now - t).min(MAX_STEP),
            _ => 0.0,
        };
        self.last = Some(now);
        let len = self.ring.len();
        // Oldest first: from `pos` to the end, then the start up to `pos`.
        let (newest, oldest) = self.ring.split_at(self.pos);
        for ((x, s), w) in self.input.iter_mut().zip(oldest.iter().chain(newest)).zip(&self.window) {
            *x = s * w;
        }
        let mean_square = self.ring.iter().map(|s| s * s).sum::<f32>() / len as f32;
        let db = to_db(mean_square);
        if self.fft.process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch).is_err() {
            self.spectrum.fill(Complex::default());
        }
        let power = |lo: usize, hi: usize| {
            self.spectrum[lo.min(hi)..hi.min(self.spectrum.len())].iter().map(|c| c.norm_sqr()).sum::<f32>()
                * self.power_scale
        };
        let raw: Vec<f32> = self.edges.windows(2).map(|e| to_db(power(e[0], e[1]))).collect();
        let tilted: Vec<f32> = raw.iter().zip(&self.tilt).map(|(r, t)| r + t).collect();
        let top = raw.iter().zip(&tilted).filter(|(r, _)| **r >= GATE_DB).map(|(_, t)| *t).fold(SILENT_DB, f32::max);
        let more = doublings(self.sensitivity) * SENSITIVITY_DB_PER_DOUBLING;
        let reference = self.band_ref.follow(top, dt) - more;
        let step = dt as f32;
        for (((bar, peak), raw), tilted) in self.bands.iter_mut().zip(&mut self.peaks).zip(&raw).zip(&tilted) {
            let target =
                if *raw < GATE_DB { 0.0 } else { (1.0 + (tilted - reference) / BAND_RANGE_DB).clamp(0.0, 1.0) };
            *bar = smooth(*bar, target, step);
            peak.step(*bar, now, step);
        }
        let loud_ref = self.loud_ref.follow(if db >= GATE_DB { db } else { SILENT_DB }, dt) - more;
        let loud = if db < GATE_DB { 0.0 } else { (1.0 + (db - loud_ref) / LOUD_RANGE_DB).clamp(0.0, 1.0) };
        self.loud = smooth(self.loud, loud, step);
        let bass_db = to_db(power(self.bass.0, self.bass.1));
        self.beats.step(bass_db, now, self.sensitivity);
        Levels {
            bands: self.bands.clone(),
            peaks: self.peaks.iter().map(|p| p.level).collect(),
            loud: self.loud,
            beat: self.beats.last,
            db,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    const RATE: u32 = 48_000;
    /// The music lights' frame time.
    const TICK: f64 = 0.05;

    /// Feed `signal(t)` for `secs` from `from`, analysing once per tick.
    fn feed(a: &mut Analyzer, from: f64, secs: f64, signal: impl Fn(f64) -> f32) -> Vec<(f64, Levels)> {
        let rate = f64::from(a.rate());
        let per = (rate * TICK).round() as usize;
        let ticks = (secs / TICK).round() as usize;
        let mut buf = Vec::with_capacity(per);
        (0..ticks)
            .map(|k| {
                let start = from + k as f64 * TICK;
                buf.clear();
                buf.extend((0..per).map(|i| signal(start + i as f64 / rate)));
                a.push(&buf);
                let now = start + TICK;
                (now, a.analyze(now))
            })
            .collect()
    }

    fn sine(hz: f64, amp: f64) -> impl Fn(f64) -> f32 {
        move |t| (amp * (TAU * hz * t).sin()) as f32
    }

    fn silence(_: f64) -> f32 {
        0.0
    }

    /// The band holding `hz`.
    fn band_of(a: &Analyzer, hz: f32) -> usize {
        let b = bin(hz, a.rate, a.ring.len());
        a.edges.windows(2).position(|e| e[0] <= b && b < e[1]).expect("in range")
    }

    fn argmax(v: &[f32]) -> usize {
        v.iter().enumerate().fold(0, |m, (i, x)| if *x > v[m] { i } else { m })
    }

    /// A kick drum: a 55 Hz tone dying away in ~80 ms, from each of `hits`.
    fn kicks(hits: Vec<f64>) -> impl Fn(f64) -> f32 {
        move |t| {
            hits.iter()
                .filter(|&&h| t >= h)
                .map(|&h| 0.8 * (-(t - h) / 0.08).exp() * (TAU * 55.0 * (t - h)).sin())
                .sum::<f64>() as f32
        }
    }

    /// The times a new beat was reported.
    fn beats(levels: &[(f64, Levels)]) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        for (now, l) in levels {
            if l.beat == Some(*now) {
                out.push(*now);
            }
        }
        out
    }

    #[test]
    fn the_window_is_about_43_ms() {
        assert_eq!(fft_len(44_100), 2048);
        assert_eq!(fft_len(48_000), 2048);
        assert_eq!(fft_len(96_000), 4096);
        assert_eq!(fft_len(192_000), 8192);
        assert_eq!(fft_len(8_000), 256);
        assert_eq!(fft_len(0), 256);
    }

    #[test]
    fn bands_rise_cover_the_range_and_have_a_bin_each() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let len = fft_len(rate);
            let width = rate as f32 / len as f32;
            for n in [1, 16, 18, 19, 32] {
                let e = band_edges(rate, len, n);
                assert_eq!(e.len(), n + 1, "{rate} Hz, {n} bands");
                assert!(e.windows(2).all(|w| w[0] < w[1]), "{rate} Hz, {n} bands: {e:?}");
                let hz = |b: usize| b as f32 * width;
                assert!((hz(e[0]) - LOW_HZ).abs() <= width, "{rate} Hz starts at {}", hz(e[0]));
                assert!((hz(e[n]) - HIGH_HZ).abs() <= width, "{rate} Hz ends at {}", hz(e[n]));
                assert!(e[n] <= len / 2 + 1);
            }
        }
        // Even more bands than the bass has bins: every band still gets one.
        let e = band_edges(8_000, 256, 64);
        assert!(e.windows(2).all(|w| w[0] < w[1]) && e[64] <= 129, "{e:?}");
    }

    #[test]
    fn a_1_khz_tone_lights_its_own_band() {
        let mut a = Analyzer::new(RATE, 16);
        let levels = feed(&mut a, 0.0, 1.0, sine(1_000.0, 0.5));
        let last = &levels.last().unwrap().1;
        let own = band_of(&a, 1_000.0);
        assert_eq!(argmax(&last.bands), own, "{:?}", last.bands);
        assert!(last.bands[own] > 0.95, "{:?}", last.bands);
        for (i, b) in last.bands.iter().enumerate() {
            if i.abs_diff(own) >= 2 {
                assert!(*b < 0.3, "band {i}: {:?}", last.bands);
            }
        }
        // At 96 kHz too.
        let mut a = Analyzer::new(96_000, 18);
        let last = feed(&mut a, 0.0, 0.5, sine(1_000.0, 0.5)).pop().unwrap().1;
        assert_eq!(argmax(&last.bands), band_of(&a, 1_000.0));
    }

    #[test]
    fn silence_gives_zeros_and_no_nan() {
        let mut a = Analyzer::new(RATE, 16);
        for (_, l) in feed(&mut a, 0.0, 1.0, silence) {
            assert!(l.bands.iter().chain(&l.peaks).all(|v| *v == 0.0), "{l:?}");
            assert_eq!((l.loud, l.beat, l.db), (0.0, None, SILENT_DB));
        }
        // Broken samples are silence too.
        a.push(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY]);
        let l = a.analyze(1.1);
        assert!(l.bands.iter().chain(&l.peaks).chain([&l.loud, &l.db]).all(|v| v.is_finite()), "{l:?}");
        assert_eq!(l.loud, 0.0);
    }

    #[test]
    fn the_gain_fills_the_bars_without_going_over() {
        let mut a = Analyzer::new(RATE, 16);
        let own = band_of(&a, 1_000.0);
        let loud = feed(&mut a, 0.0, 3.0, sine(1_000.0, 0.9));
        assert!(loud.iter().all(|(_, l)| l.bands.iter().all(|b| *b <= 1.0)));
        assert!(loud.last().unwrap().1.bands[own] > 0.99);
        // 20 dB quieter: half height at first, nearly full again in seconds.
        let quiet = feed(&mut a, 3.0, 6.0, sine(1_000.0, 0.09));
        let at = |secs: f64| quiet[(secs / TICK).round() as usize - 1].1.bands[own];
        assert!((0.4..0.7).contains(&at(0.3)), "{}", at(0.3));
        assert!(at(3.0) > at(0.3));
        assert!(at(6.0) > 0.8, "{}", at(6.0));
        assert!(quiet.iter().all(|(_, l)| l.bands.iter().all(|b| *b <= 1.0)));
        // A very quiet sound still shows.
        let mut a = Analyzer::new(RATE, 16);
        let faint = feed(&mut a, 0.0, 1.0, sine(1_000.0, 0.003)).pop().unwrap().1;
        assert!(faint.bands[own] > 0.8, "{:?}", faint.bands);
        // The most sensitive setting never goes over either.
        let mut a = Analyzer::new(RATE, 16);
        a.set_sensitivity(MAX_SENSITIVITY);
        assert!(feed(&mut a, 0.0, 1.0, sine(1_000.0, 0.9)).iter().all(|(_, l)| l.bands.iter().all(|b| *b <= 1.0)));
    }

    #[test]
    fn higher_sensitivity_fills_more() {
        // A loud tone and a 30 dB weaker one: the weaker band's height
        // depends on the setting.
        let two = |t: f64| sine(1_000.0, 0.5)(t) + sine(4_000.0, 0.016)(t);
        let height = |sensitivity| {
            let mut a = Analyzer::new(RATE, 16);
            a.set_sensitivity(sensitivity);
            let band = band_of(&a, 4_000.0);
            feed(&mut a, 0.0, 1.0, two).pop().unwrap().1.bands[band]
        };
        assert!(
            height(200) > height(100) && height(100) > height(50),
            "{} {} {}",
            height(50),
            height(100),
            height(200)
        );
    }

    #[test]
    fn bars_rise_at_once_and_fall_in_a_quarter_second() {
        let mut a = Analyzer::new(RATE, 16);
        let own = band_of(&a, 1_000.0);
        let on = feed(&mut a, 0.0, 1.0, sine(1_000.0, 0.5));
        assert!(on[0].1.bands[own] > 0.95, "first frame: {:?}", on[0].1.bands);
        let off = feed(&mut a, 1.0, 1.0, silence);
        let heights: Vec<f32> = off.iter().map(|(_, l)| l.bands[own]).collect();
        assert!(heights.windows(2).all(|w| w[1] <= w[0]), "{heights:?}");
        assert!((0.5..0.7).contains(&heights[1]), "after 0.1 s: {heights:?}");
        assert!(heights[3] > 0.0 && heights[4] == 0.0, "empty after 0.25 s: {heights:?}");
    }

    #[test]
    fn peak_marks_hold_then_fall() {
        let mut a = Analyzer::new(RATE, 16);
        let own = band_of(&a, 1_000.0);
        feed(&mut a, 0.0, 1.0, sine(1_000.0, 0.5));
        let off = feed(&mut a, 1.0, 1.5, silence);
        let peak = |secs: f64| off[(secs / TICK).round() as usize - 1].1.peaks[own];
        assert_eq!(peak(0.35), 1.0, "held");
        assert!(peak(0.6) < 1.0 && peak(0.6) > 0.0, "falling: {}", peak(0.6));
        assert_eq!(peak(1.5), 0.0);
        assert!(off.iter().all(|(_, l)| l.peaks.iter().zip(&l.bands).all(|(p, b)| p >= b)));
    }

    #[test]
    fn loudness_follows_the_sound() {
        let mut a = Analyzer::new(RATE, 16);
        let loud = feed(&mut a, 0.0, 2.0, sine(500.0, 0.5)).pop().unwrap().1;
        assert!(loud.loud > 0.95 && (loud.db + 9.0).abs() < 0.5, "{loud:?}");
        // 12 dB quieter: about half at first.
        let quieter = feed(&mut a, 2.0, 0.3, sine(500.0, 0.125)).pop().unwrap().1;
        assert!((0.35..0.7).contains(&quieter.loud), "{quieter:?}");
        let gone = feed(&mut a, 2.3, 0.5, silence).pop().unwrap().1;
        assert_eq!((gone.loud, gone.db), (0.0, SILENT_DB));
    }

    #[test]
    fn a_120_bpm_kick_gives_a_beat_every_half_second() {
        // Hits off the tick grid, 16 of them.
        let hits: Vec<f64> = (0..16).map(|k| 0.013 + 0.5 * k as f64).collect();
        let mut a = Analyzer::new(RATE, 16);
        let found = beats(&feed(&mut a, 0.0, 8.1, kicks(hits)));
        // The first hit comes before there is any history to compare with.
        assert!(found.len() >= 14, "{found:?}");
        for w in found.windows(2) {
            assert!((w[1] - w[0] - 0.5).abs() <= TICK + 1e-9, "{found:?}");
        }
    }

    #[test]
    fn steady_noise_gives_no_beats() {
        // xorshift: the same noise every run.
        let state = std::cell::Cell::new(0x2545_F491_4F6C_DD1D_u64);
        let noise = |_: f64| {
            let mut x = state.get();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            state.set(x);
            (x >> 40) as f32 / (1u64 << 24) as f32 - 0.5
        };
        let mut a = Analyzer::new(RATE, 16);
        let found = beats(&feed(&mut a, 0.0, 10.0, noise));
        assert!(found.len() <= 1, "{found:?}");
        // Nor does a steady tone.
        let mut a = Analyzer::new(RATE, 16);
        assert_eq!(beats(&feed(&mut a, 0.0, 5.0, sine(60.0, 0.5))), Vec::<f64>::new());
    }

    #[test]
    fn two_hits_closer_than_the_gap_are_one_beat() {
        let mut a = Analyzer::new(RATE, 16);
        let found = beats(&feed(&mut a, 0.0, 1.5, kicks(vec![0.613, 0.713])));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!((found[0] - 0.65).abs() < 1e-9, "{found:?}");
    }

    #[test]
    fn a_reset_is_no_beat() {
        // A tone after silence is a beat (something started)…
        let mut a = Analyzer::new(RATE, 16);
        feed(&mut a, 0.0, 1.0, silence);
        assert_eq!(beats(&feed(&mut a, 1.0, 1.0, sine(60.0, 0.5))).len(), 1);
        // …but not after a gap in the sound: the history is gone.
        let mut a = Analyzer::new(RATE, 16);
        feed(&mut a, 0.0, 1.0, silence);
        a.reset();
        assert_eq!(beats(&feed(&mut a, 1.0, 1.0, sine(60.0, 0.5))), Vec::<f64>::new());
        // A clock going back starts over instead of panicking.
        assert!(a.analyze(0.5).bands.iter().all(|b| b.is_finite()));
    }

    /// By hand: `cargo test -p keymotion -- --ignored --nocapture spectrum::tests::time`.
    #[test]
    #[ignore]
    fn time_one_analysis() {
        let mut a = Analyzer::new(RATE, 18);
        let signal: Vec<f32> = (0..2400).map(|i| sine(440.0, 0.5)(i as f64 / f64::from(RATE))).collect();
        let start = std::time::Instant::now();
        for k in 0..1000 {
            a.push(&signal);
            a.analyze(k as f64 * TICK);
        }
        println!("{:?} per push and analysis", start.elapsed() / 1000);
    }
}
