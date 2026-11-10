//! How a lighting pattern moves between two `04 F5` read-backs.
//!
//! Each frame is projected onto an axis (key centre x, y, or the angle
//! around the board's centre) as a profile of mean green and blue values.
//! Red is left out because the read-back reports it ambiguously (2·r mod
//! 256). The shift that best maps the first profile onto the second, over
//! the time between the reads, is the pattern's speed along that axis.

use std::collections::HashMap;

/// A key centre in key units (x to the right, y downwards).
#[derive(Debug, Clone, Copy)]
pub struct KeyPos {
    pub led: u8,
    pub x: f32,
    pub y: f32,
}

pub type Frame = HashMap<u8, [u8; 3]>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Left → right.
    X,
    /// Top → bottom.
    Y,
    /// Degrees around the centre, clockwise on screen (y points down).
    Angle,
}

/// Movement along one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisMotion {
    /// Units per second (key units, or degrees for [`Axis::Angle`]).
    pub speed: f32,
    /// How much better the best shift fits than no shift (1 = no evidence).
    pub confidence: f32,
}

/// Profile points `(coordinate, [g, b])`, sorted, one per group of keys
/// that share a coordinate (within `merge`).
fn profile(keys: &[KeyPos], frame: &Frame, axis: Axis, centre: (f32, f32)) -> Vec<(f32, [f32; 2])> {
    let merge = if axis == Axis::Angle { 4.0 } else { 0.3 };
    let mut pts: Vec<(f32, [f32; 2])> = keys
        .iter()
        .filter_map(|k| {
            let [_, g, b] = *frame.get(&k.led)?;
            let c = match axis {
                Axis::X => k.x,
                Axis::Y => k.y,
                Axis::Angle => (k.y - centre.1).atan2(k.x - centre.0).to_degrees().rem_euclid(360.0),
            };
            Some((c, [f32::from(g), f32::from(b)]))
        })
        .collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Average keys that share (nearly) the same coordinate.
    let mut out: Vec<(f32, [f32; 2], f32)> = Vec::new();
    for (c, v) in pts {
        match out.last_mut() {
            Some((lc, lv, n)) if c - *lc / *n <= merge => {
                *lc += c;
                lv[0] += v[0];
                lv[1] += v[1];
                *n += 1.0;
            }
            _ => out.push((c, v, 1.0)),
        }
    }
    out.into_iter().map(|(c, v, n)| (c / n, [v[0] / n, v[1] / n])).collect()
}

/// Linear interpolation of a profile at `c`; `None` outside its range.
/// `period` makes the profile wrap (angles).
fn sample(p: &[(f32, [f32; 2])], c: f32, period: Option<f32>) -> Option<[f32; 2]> {
    let c = match period {
        Some(t) => c.rem_euclid(t),
        None => c,
    };
    let i = p.partition_point(|(x, _)| *x < c);
    let lerp = |a: (f32, [f32; 2]), b: (f32, [f32; 2]), bx: f32| {
        let span = bx - a.0;
        let t = if span.abs() < f32::EPSILON { 0.0 } else { (c - a.0) / span };
        [a.1[0] + (b.1[0] - a.1[0]) * t, a.1[1] + (b.1[1] - a.1[1]) * t]
    };
    match (i.checked_sub(1).and_then(|j| p.get(j)), p.get(i)) {
        (Some(&a), Some(&b)) => Some(lerp(a, b, b.0)),
        (Some(&a), None) => period.map(|t| lerp(a, p[0], p[0].0 + t)),
        (None, Some(&b)) => {
            let last = *p.last()?;
            period.map(|t| lerp((last.0 - t, last.1), b, b.0))
        }
        (None, None) => None,
    }
}

/// Mean squared difference between `b` and `a` shifted by `shift`.
fn mismatch(a: &[(f32, [f32; 2])], b: &[(f32, [f32; 2])], shift: f32, period: Option<f32>) -> Option<f32> {
    let (mut sum, mut n) = (0.0, 0usize);
    for &(c, v) in b {
        if let Some(w) = sample(a, c - shift, period) {
            sum += (v[0] - w[0]).powi(2) + (v[1] - w[1]).powi(2);
            n += 1;
        }
    }
    // Shifts that leave too few points overlapping are not comparable.
    (n * 2 >= b.len()).then(|| sum / n as f32)
}

/// Movement along `axis` between two frames taken `dt` seconds apart.
/// `max_shift` bounds the search (key units, or degrees).
pub fn axis_motion(keys: &[KeyPos], f1: &Frame, f2: &Frame, dt: f32, axis: Axis, max_shift: f32) -> Option<AxisMotion> {
    let centre = centre(keys);
    let a = profile(keys, f1, axis, centre);
    let b = profile(keys, f2, axis, centre);
    if a.len() < 3 || b.len() < 3 || dt <= 0.0 {
        return None;
    }
    let (period, step) = if axis == Axis::Angle { (Some(360.0), 1.0) } else { (None, 0.05) };
    let still = mismatch(&a, &b, 0.0, period)?;
    let steps = (max_shift / step).round() as i32;
    let (best_shift, best) = (-steps..=steps)
        .filter_map(|i| {
            let s = i as f32 * step;
            mismatch(&a, &b, s, period).map(|e| (s, e))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))?;
    Some(AxisMotion { speed: best_shift / dt, confidence: (still + 1.0) / (best + 1.0) })
}

/// Movement in two dimensions at once: the shift `(dx, dy)` that best maps
/// the first frame onto the second, values between key centres taken from
/// a Gaussian blend of nearby keys. Works for diagonal and sparse patterns
/// that projections average away. Returns `(vx, vy, confidence)`.
pub fn motion_2d(keys: &[KeyPos], f1: &Frame, f2: &Frame, dt: f32, max_shift: f32) -> Option<(f32, f32, f32)> {
    const STEP: f32 = 0.25;
    const SIGMA: f32 = 0.45;
    const REACH: f32 = 1.2;
    let pts1: Vec<(f32, f32, [f32; 2])> =
        keys.iter().filter_map(|k| f1.get(&k.led).map(|c| (k.x, k.y, [f32::from(c[1]), f32::from(c[2])]))).collect();
    let at = |x: f32, y: f32| -> Option<[f32; 2]> {
        let (mut w, mut g, mut b) = (0.0, 0.0, 0.0);
        for &(px, py, v) in &pts1 {
            let d2 = (px - x).powi(2) + (py - y).powi(2);
            if d2 <= REACH * REACH {
                let wi = (-d2 / (2.0 * SIGMA * SIGMA)).exp();
                w += wi;
                g += wi * v[0];
                b += wi * v[1];
            }
        }
        (w > 0.3).then(|| [g / w, b / w])
    };
    let mismatch = |dx: f32, dy: f32| -> Option<f32> {
        let (mut sum, mut n) = (0.0, 0usize);
        for k in keys {
            let Some(c) = f2.get(&k.led) else { continue };
            if let Some(p) = at(k.x - dx, k.y - dy) {
                sum += (f32::from(c[1]) - p[0]).powi(2) + (f32::from(c[2]) - p[1]).powi(2);
                n += 1;
            }
        }
        (n * 2 >= keys.len()).then(|| sum / n as f32)
    };
    if dt <= 0.0 {
        return None;
    }
    let still = mismatch(0.0, 0.0)?;
    let steps = (max_shift / STEP).round() as i32;
    let mut best = (0.0, 0.0, still);
    for i in -steps..=steps {
        for j in -steps..=steps {
            let (dx, dy) = (i as f32 * STEP, j as f32 * STEP);
            if let Some(e) = mismatch(dx, dy) {
                if e < best.2 {
                    best = (dx, dy, e);
                }
            }
        }
    }
    Some((best.0 / dt, best.1 / dt, (still + 1.0) / (best.2 + 1.0)))
}

/// Centre of the keys (mean key centre).
pub fn centre(keys: &[KeyPos]) -> (f32, f32) {
    let n = keys.len().max(1) as f32;
    (keys.iter().map(|k| k.x).sum::<f32>() / n, keys.iter().map(|k| k.y).sum::<f32>() / n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 21 × 6 grid of keys, one unit apart.
    fn grid() -> Vec<KeyPos> {
        let mut keys = Vec::new();
        for row in 0..6 {
            for col in 0..21 {
                keys.push(KeyPos { led: (row * 21 + col) as u8, x: col as f32 + 0.5, y: row as f32 + 0.5 });
            }
        }
        keys
    }

    /// A smooth periodic pattern in G and B.
    fn wave(phase: f32) -> [u8; 3] {
        let g = 128.0 + 120.0 * phase.sin();
        let b = 128.0 + 120.0 * (phase + 2.0).sin();
        [0, g as u8, b as u8]
    }

    fn frame(keys: &[KeyPos], f: impl Fn(&KeyPos) -> f32) -> Frame {
        keys.iter().map(|k| (k.led, wave(f(k)))).collect()
    }

    #[test]
    fn horizontal_wave_moving_right() {
        let keys = grid();
        let k = std::f32::consts::TAU / 14.0; // period 14 units
        let f1 = frame(&keys, |p| k * p.x);
        let f2 = frame(&keys, |p| k * (p.x - 1.0)); // moved 1 unit right
        let m = axis_motion(&keys, &f1, &f2, 0.5, Axis::X, 3.0).unwrap();
        assert!((m.speed - 2.0).abs() < 0.15, "{m:?}");
        assert!(m.confidence > 5.0, "{m:?}");
        // Nothing moves vertically: every row is the same.
        let v = axis_motion(&keys, &f1, &f2, 0.5, Axis::Y, 3.0).unwrap();
        assert!(v.confidence < 1.2, "{v:?}");
    }

    #[test]
    fn vertical_wave_moving_up() {
        let keys = grid();
        let k = std::f32::consts::TAU / 9.0;
        let f1 = frame(&keys, |p| k * p.y);
        let f2 = frame(&keys, |p| k * (p.y + 0.6)); // moved 0.6 units up
        let m = axis_motion(&keys, &f1, &f2, 0.2, Axis::Y, 2.0).unwrap();
        assert!((m.speed + 3.0).abs() < 0.3, "{m:?}");
    }

    #[test]
    fn diagonal_wave_moving_up_right() {
        let keys = grid();
        let k = std::f32::consts::TAU / 8.0;
        // Stripes along the anti-diagonal, moving 1 unit right and 1 unit up.
        let f1 = frame(&keys, |p| k * (p.x - p.y));
        let f2 = frame(&keys, |p| k * ((p.x - 1.0) - (p.y + 1.0)));
        let (vx, vy, conf) = motion_2d(&keys, &f1, &f2, 1.0, 2.0).unwrap();
        // Only the component across the stripes is observable: (1, -1).
        assert!(vx > 0.5 && vy < -0.5, "({vx}, {vy})");
        assert!(conf > 2.0, "{conf}");
    }

    #[test]
    fn rotation_clockwise() {
        let keys = grid();
        let c = centre(&keys);
        let angle = |p: &KeyPos| (p.y - c.1).atan2(p.x - c.0);
        let f1 = frame(&keys, |p| angle(p));
        let f2 = frame(&keys, |p| angle(p) - 20f32.to_radians()); // turned 20° clockwise
        let m = axis_motion(&keys, &f1, &f2, 1.0, Axis::Angle, 60.0).unwrap();
        assert!((m.speed - 20.0).abs() < 3.0, "{m:?}");
    }
}
