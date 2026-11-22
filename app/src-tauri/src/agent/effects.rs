//! How the agents' lights move: one key's brightness (0–1) at a moment.
//!
//! * The five simple motions (solid, pulse, blink, wave, flash) take their
//!   cycle from `kb_agent::Effect`, so a blink or a flash never switches
//!   more than three times a second (WCAG 2.3.1); [`Motion::level`]
//!   clamps the cycle again before it draws one.
//! * The user's looks add three compound ones (owner's defaults,
//!   2026-11-22): one wave then steady (done), three quick flashes then a
//!   slow pulse (attention), two blinks then steady (error).
//! * A "flash again" ([`BURST_MS`]: three flashes at the safe rate) goes
//!   over a light for a moment: when the user comes back to the computer,
//!   or when a question has waited too long.
//! * [`FlashGuard`] watches the frames that go out, key by key, and holds
//!   a key dark rather than let it light up a fourth time within a second,
//!   whatever the looks, the flashes again and the lights taking turns on
//!   it add up to.

use crate::dto::AgentEffect;
use kb_agent::values::{clamp_flash_period, MAX_FLASHES_PER_SECOND, MIN_FLASH_PERIOD};
use kb_agent::{Direction, Effect, EffectKind};
use kb_protocol::sonix::Rgb;
use std::collections::{HashMap, VecDeque};

/// The shortest safe flash, in ms.
pub const fn flash_period_ms() -> u64 {
    MIN_FLASH_PERIOD.as_millis() as u64
}

/// How long a "flash again" lasts: three flashes at the safe rate.
pub const BURST_FLASHES: u64 = 3;
pub const BURST_MS: u64 = BURST_FLASHES * flash_period_ms();

/// The darkest a pulse gets.
const PULSE_FLOOR: f32 = 0.12;
/// Unlit keys of a progress bar.
const BAR_FLOOR: f32 = 0.08;
/// Width of a wave's band, as a share of the keys' span.
const WAVE_WIDTH: f32 = 0.16;
/// A wave lighter than this leaves its key dark.
const WAVE_CUTOFF: f32 = 0.03;

/// Where a key sits among a light's keys: `x` and `y` as shares (0–1) of
/// their span, `rank` in reading order (left to right, then top to bottom)
/// of `count`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub x: f32,
    pub y: f32,
    pub rank: usize,
    pub count: usize,
}

impl Place {
    /// A key on its own.
    #[cfg(test)]
    pub const ALONE: Place = Place { x: 0.5, y: 0.5, rank: 0, count: 1 };
}

/// One way of moving, with its cycle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    Solid,
    Pulse {
        period_ms: u64,
    },
    Blink {
        period_ms: u64,
    },
    Wave {
        period_ms: u64,
        direction: Direction,
    },
    Flash {
        period_ms: u64,
    },
    /// A progress bar: `fraction` (0–1) of the keys lit, in reading order.
    Bar {
        fraction: f32,
    },
}

impl Motion {
    /// From a checked effect (its cycle count is the light's business).
    pub fn of(effect: &Effect) -> Self {
        let period_ms = effect.period().map_or(1000, |p| p.as_millis() as u64);
        match effect.kind() {
            EffectKind::Solid => Self::Solid,
            EffectKind::Pulse => Self::Pulse { period_ms },
            EffectKind::Blink => Self::Blink { period_ms },
            EffectKind::Wave => Self::Wave { period_ms, direction: effect.direction().unwrap_or(Direction::Right) },
            EffectKind::Flash => Self::Flash { period_ms },
        }
    }

    /// `kind` at `speed` (1–5), through kb-agent's checks.
    fn at_speed(kind: EffectKind, speed: u8) -> Self {
        let speed = u32::from(speed.clamp(1, 5));
        Effect::new(kind, None, Some(speed), None).map_or(Self::Solid, |e| Self::of(&e))
    }

    /// One cycle, in ms; `None` for steady motions.
    pub fn period_ms(&self) -> Option<u64> {
        match *self {
            Self::Solid | Self::Bar { .. } => None,
            Self::Pulse { period_ms } | Self::Wave { period_ms, .. } => Some(period_ms.max(1)),
            Self::Blink { period_ms } | Self::Flash { period_ms } => Some(safe_flash(period_ms)),
        }
    }

    /// The brightness (0–1) of the key at `place`, `t` ms into the motion.
    pub fn level(&self, t: u64, place: Place) -> f32 {
        match *self {
            Self::Solid => 1.0,
            Self::Pulse { period_ms } => {
                let phase = (t % period_ms.max(1)) as f32 / period_ms.max(1) as f32;
                // Starts bright, so a new light shows at once.
                let k = 0.5 + 0.5 * (std::f32::consts::TAU * phase).cos();
                PULSE_FLOOR + (1.0 - PULSE_FLOOR) * k
            }
            Self::Blink { period_ms } | Self::Flash { period_ms } => {
                let p = safe_flash(period_ms);
                if t % p < p / 2 {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Wave { period_ms, direction } => {
                let along = match direction {
                    Direction::Right => place.x,
                    Direction::Left => 1.0 - place.x,
                    Direction::Down => place.y,
                    Direction::Up => 1.0 - place.y,
                };
                let phase = (t % period_ms.max(1)) as f32 / period_ms.max(1) as f32;
                // The band enters before the first key and leaves after the last.
                let centre = -0.3 + 1.6 * phase;
                let a = (-((along - centre) / WAVE_WIDTH).powi(2)).exp();
                if a < WAVE_CUTOFF {
                    0.0
                } else {
                    a
                }
            }
            Self::Bar { fraction } => {
                let lit = (fraction.clamp(0.0, 1.0) * place.count as f32 - place.rank as f32).clamp(0.0, 1.0);
                BAR_FLOOR.max(lit)
            }
        }
    }
}

/// A blink's or flash's cycle, never under the safe minimum.
fn safe_flash(period_ms: u64) -> u64 {
    clamp_flash_period(std::time::Duration::from_millis(period_ms)).as_millis() as u64
}

/// How a light looks: a colour, an opening (`intro`, run `cycles` times)
/// and then its main motion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub color: Rgb,
    pub intro: Option<(Motion, u32)>,
    pub main: Motion,
}

impl Look {
    pub const fn plain(color: Rgb, main: Motion) -> Self {
        Self { color, intro: None, main }
    }

    /// A look the user chose; `None` for "no light".
    #[cfg(test)]
    pub fn from_settings(look: &crate::dto::AgentLook) -> Option<Self> {
        Self::of_effect(look.color, look.effect, look.speed)
    }

    /// `effect` at `speed` in `color`; `None` for "no light".
    pub fn of_effect(color: Rgb, effect: AgentEffect, speed: u8) -> Option<Self> {
        let at = |kind| Motion::at_speed(kind, speed);
        let quick_flash = Motion::Flash { period_ms: flash_period_ms() };
        let (intro, main) = match effect {
            AgentEffect::Off => return None,
            AgentEffect::Solid => (None, Motion::Solid),
            AgentEffect::Pulse => (None, at(EffectKind::Pulse)),
            AgentEffect::Blink => (None, at(EffectKind::Blink)),
            AgentEffect::Wave => (None, at(EffectKind::Wave)),
            AgentEffect::Flash => (None, at(EffectKind::Flash)),
            AgentEffect::WaveThenSolid => (Some((at(EffectKind::Wave), 1)), Motion::Solid),
            AgentEffect::FlashThenPulse => (Some((quick_flash, BURST_FLASHES as u32)), at(EffectKind::Pulse)),
            AgentEffect::BlinkTwiceThenSolid => (Some((at(EffectKind::Blink), 2)), Motion::Solid),
        };
        Some(Self { color, intro, main })
    }

    /// How long the opening runs, in ms.
    pub fn intro_ms(&self) -> u64 {
        self.intro.map_or(0, |(m, cycles)| m.period_ms().unwrap_or(0) * u64::from(cycles))
    }

    /// The brightness of the key at `place`, `t` ms after the light came.
    pub fn level(&self, t: u64, place: Place) -> f32 {
        let intro = self.intro_ms();
        match self.intro {
            Some((motion, _)) if t < intro => motion.level(t, place),
            _ => self.main.level(t - intro.min(t), place),
        }
    }
}

/// The brightness of a "flash again" `t` ms after it started; `None` once
/// it is over.
pub fn burst_level(t: u64) -> Option<f32> {
    (t < BURST_MS)
        .then(|| Motion::Flash { period_ms: flash_period_ms() }.level(t, Place { x: 0.0, y: 0.0, rank: 0, count: 1 }))
}

/// `color` at brightness `k` (0–1).
pub fn scale(color: Rgb, k: f32) -> Rgb {
    let k = k.clamp(0.0, 1.0);
    color.map(|v| (f32::from(v) * k).round() as u8)
}

/// A key counts as lit at this brightness of its brightest channel, and
/// as dark again below [`DARK`] (the gap keeps a pulse near the line from
/// counting as flashes).
const LIT: u8 = 96;
const DARK: u8 = 48;
const WINDOW_MS: u64 = 1000;

/// The last check before a frame goes out: no key lights up more than
/// three times in any second. A key that would is held dark for that frame.
#[derive(Debug, Default)]
pub struct FlashGuard {
    keys: HashMap<u8, KeyState>,
}

#[derive(Debug, Default)]
struct KeyState {
    lit: bool,
    /// When it lit up lately, oldest first.
    ups: VecDeque<u64>,
}

impl FlashGuard {
    /// `frame` as it may go out at `now` (ms).
    pub fn check(&mut self, now: u64, frame: Vec<(u8, Rgb)>) -> Vec<(u8, Rgb)> {
        let seen: Vec<u8> = frame.iter().map(|k| k.0).collect();
        for (led, state) in &mut self.keys {
            while state.ups.front().is_some_and(|&t| now.saturating_sub(t) >= WINDOW_MS) {
                state.ups.pop_front();
            }
            // A key the frame leaves out shows what is under the agents'
            // layer: dark, as far as this layer goes.
            if !seen.contains(led) {
                state.lit = false;
            }
        }
        let out = frame
            .into_iter()
            .map(|(led, color)| {
                let state = self.keys.entry(led).or_default();
                let peak = color.into_iter().max().unwrap_or(0);
                if !state.lit && peak >= LIT {
                    if state.ups.len() >= MAX_FLASHES_PER_SECOND as usize {
                        return (led, [0, 0, 0]);
                    }
                    state.ups.push_back(now);
                    state.lit = true;
                } else if state.lit && peak < DARK {
                    state.lit = false;
                }
                (led, color)
            })
            .collect();
        self.keys.retain(|_, s| s.lit || !s.ups.is_empty());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::AgentSignalLooks;

    /// Times a key goes from dark to lit, sampled every `step` ms for `ms`.
    fn ups(level: impl Fn(u64) -> f32, step: u64, ms: u64) -> Vec<u64> {
        let mut out = Vec::new();
        let mut lit = false;
        for t in (0..ms).step_by(step as usize) {
            let k = level(t);
            if !lit && k >= 0.5 {
                out.push(t);
                lit = true;
            } else if lit && k < 0.25 {
                lit = false;
            }
        }
        out
    }

    /// The most `ups` in any one second.
    fn most_in_a_second(ups: &[u64]) -> usize {
        ups.iter().map(|&t| ups.iter().filter(|&&u| u >= t && u < t + 1000).count()).max().unwrap_or(0)
    }

    const ALL_EFFECTS: [AgentEffect; 9] = [
        AgentEffect::Solid,
        AgentEffect::Pulse,
        AgentEffect::Blink,
        AgentEffect::Wave,
        AgentEffect::Flash,
        AgentEffect::WaveThenSolid,
        AgentEffect::FlashThenPulse,
        AgentEffect::BlinkTwiceThenSolid,
        AgentEffect::Off,
    ];

    #[test]
    fn no_look_flashes_more_than_three_times_a_second() {
        for effect in ALL_EFFECTS {
            for speed in 1..=5 {
                let Some(look) = Look::of_effect([255, 0, 0], effect, speed) else {
                    assert_eq!(effect, AgentEffect::Off);
                    continue;
                };
                for place in [Place::ALONE, Place { x: 0.0, y: 0.0, rank: 0, count: 10 }] {
                    // Sampled finely and at the app's own 20 frames a second.
                    for step in [5, 50] {
                        let ups = ups(|t| look.level(t, place), step, 20_000);
                        let most = most_in_a_second(&ups);
                        assert!(most <= 3, "{effect:?} speed {speed} step {step}: {most} in a second");
                    }
                }
            }
        }
        // A flash asked for faster than is safe still gets the safe cycle.
        let fast = Motion::Flash { period_ms: 50 };
        assert_eq!(fast.period_ms(), Some(flash_period_ms()));
        assert!(most_in_a_second(&ups(|t| fast.level(t, Place::ALONE), 5, 10_000)) <= 3);
        let fast_blink = Motion::Blink { period_ms: 1 };
        assert!(most_in_a_second(&ups(|t| fast_blink.level(t, Place::ALONE), 5, 10_000)) <= 3);
        // So does a flash again.
        assert_eq!(most_in_a_second(&ups(|t| burst_level(t).unwrap_or(0.0), 5, 5_000)), 3);
        assert!(flash_period_ms() * BURST_FLASHES <= BURST_MS);
    }

    #[test]
    fn the_default_looks_open_then_settle() {
        let looks = AgentSignalLooks::default();
        let lit = |look: &Look, t| look.level(t, Place::ALONE);

        // Done: one wave (1.8 s at speed 3), then steady green.
        let done = Look::from_settings(&looks.done).unwrap();
        assert_eq!(done.color, [0, 255, 0]);
        assert_eq!(done.intro_ms(), 1800);
        assert_eq!(lit(&done, 0), 0.0, "the band starts outside the keys");
        assert!(lit(&done, 900) > 0.9, "and crosses the middle half way");
        for t in [1800, 5_000, 119_000] {
            assert_eq!(lit(&done, t), 1.0);
        }

        // Attention: three flashes at the safe rate (1 s), then a slow pulse.
        let attention = Look::from_settings(&looks.attention).unwrap();
        assert_eq!(attention.intro_ms(), 3 * flash_period_ms());
        let flashes = ups(|t| lit(&attention, t), 1, attention.intro_ms());
        assert_eq!(flashes, vec![0, 334, 668]);
        assert_eq!(lit(&attention, 200), 0.0);
        let Motion::Pulse { period_ms } = attention.main else { panic!("{:?}", attention.main) };
        assert_eq!(period_ms, 2400, "speed 2");
        assert_eq!(lit(&attention, attention.intro_ms()), 1.0, "the pulse starts bright");
        let half = attention.intro_ms() + period_ms / 2;
        assert!((lit(&attention, half) - PULSE_FLOOR).abs() < 1e-3);

        // Error: two blinks (0.9 s each at speed 3), then steady red.
        let error = Look::from_settings(&looks.error).unwrap();
        assert_eq!(error.intro_ms(), 1800);
        assert_eq!(ups(|t| lit(&error, t), 1, 1800), vec![0, 900]);
        assert_eq!(lit(&error, 500), 0.0);
        assert_eq!(lit(&error, 1800), 1.0);
        assert_eq!(lit(&error, 600_000 - 1), 1.0);

        // Busy: no light. Info: a blue wave all along.
        assert_eq!(Look::from_settings(&looks.busy), None);
        let info = Look::from_settings(&looks.info).unwrap();
        assert_eq!((info.intro, info.color), (None, [0, 0, 255]));
        assert!(matches!(info.main, Motion::Wave { period_ms: 1800, direction: Direction::Right }));
    }

    #[test]
    fn a_wave_runs_the_way_it_is_told() {
        let wave = |direction| Motion::Wave { period_ms: 1000, direction };
        let at = |x: f32, y: f32| Place { x, y, rank: 0, count: 2 };
        // Right: the left key first.
        let right = wave(Direction::Right);
        let peak = |m: Motion, p: Place| (0..1000).step_by(10).max_by(|&a, &b| m.level(a, p).total_cmp(&m.level(b, p)));
        assert!(peak(right, at(0.0, 0.0)) < peak(right, at(1.0, 0.0)));
        let left = wave(Direction::Left);
        assert!(peak(left, at(0.0, 0.0)) > peak(left, at(1.0, 0.0)));
        let down = wave(Direction::Down);
        assert!(peak(down, at(0.5, 0.0)) < peak(down, at(0.5, 1.0)));
        let up = wave(Direction::Up);
        assert!(peak(up, at(0.5, 0.0)) > peak(up, at(0.5, 1.0)));
    }

    #[test]
    fn a_progress_bar_fills_in_reading_order() {
        let key = |rank| Place { x: 0.0, y: 0.0, rank, count: 4 };
        let bar = |p: f32| (0..4).map(|r| Motion::Bar { fraction: p }.level(0, key(r))).collect::<Vec<_>>();
        assert_eq!(bar(0.0), vec![BAR_FLOOR; 4]);
        assert_eq!(bar(0.5), vec![1.0, 1.0, BAR_FLOOR, BAR_FLOOR]);
        assert_eq!(bar(0.625), vec![1.0, 1.0, 0.5, BAR_FLOOR]);
        assert_eq!(bar(1.0), vec![1.0; 4]);
    }

    #[test]
    fn the_guard_holds_a_key_dark_rather_than_a_fourth_flash() {
        let mut guard = FlashGuard::default();
        let mut ups_out = Vec::new();
        let mut lit = false;
        // Something asks for on, off, on, off… every frame (10 a second).
        for i in 0..100u64 {
            let now = i * 50;
            let color = if i % 2 == 0 { [255, 0, 0] } else { [0, 0, 0] };
            let out = guard.check(now, vec![(7, color), (8, [0, 200, 0])]);
            assert_eq!(out[1], (8, [0, 200, 0]), "a steady key is left alone");
            let peak = out[0].1.into_iter().max().unwrap();
            if !lit && peak >= LIT {
                ups_out.push(now);
                lit = true;
            } else if lit && peak < DARK {
                lit = false;
            }
        }
        assert!(most_in_a_second(&ups_out) <= 3, "{ups_out:?}");
        assert!(ups_out.len() >= 3 * 4, "it still flashes, at the safe rate: {ups_out:?}");

        // A key that leaves the frame and comes back lit counts as lighting up.
        let mut guard = FlashGuard::default();
        let mut shown = 0;
        for i in 0..40u64 {
            let frame = if i % 2 == 0 { vec![(3, [255, 255, 255])] } else { vec![] };
            if guard.check(i * 50, frame).iter().any(|k| k.1 != [0, 0, 0]) {
                shown += 1;
            }
        }
        assert_eq!(shown, 3 + 3, "three in each of the two seconds");
    }

    #[test]
    fn colours_scale_with_brightness() {
        assert_eq!(scale([200, 100, 0], 0.5), [100, 50, 0]);
        assert_eq!(scale([200, 100, 0], 2.0), [200, 100, 0]);
        assert_eq!(scale([200, 100, 0], -1.0), [0, 0, 0]);
    }
}
