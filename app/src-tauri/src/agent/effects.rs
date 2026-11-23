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
//! * [`FlashGuard`] watches the frames that go out, key by key, and holds a
//!   key's colour rather than let it change more than six times (three
//!   flashes) within a second, whatever the looks, the flashes again, the
//!   lights taking turns on it or an agent switching colours add up to.

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
            _ => self.main.level(t - intro.min(t) + self.main_from(), place),
        }
    }

    /// Where the main motion starts, in ms of its cycle. A pulse after an
    /// opening starts from its dark side: the flashes end dark, and a pulse
    /// that lit up at once and faded again would make a fourth flash.
    fn main_from(&self) -> u64 {
        match (self.intro, self.main) {
            (Some(_), Motion::Pulse { period_ms }) => period_ms / 2,
            _ => 0,
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

/// A swing this large in a channel (of 255) is a change of the key's light:
/// brighter or darker, or another colour (red to blue swings two channels).
const SWING: i16 = 40;
/// Changes a key may make in any second: three flashes, each on and off.
pub const MAX_CHANGES: usize = 2 * MAX_FLASHES_PER_SECOND as usize;
const WINDOW_MS: u64 = 1000;

/// One channel's last turn: where it was and which way it went since.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Swing {
    /// The highest value since it last went up, or the lowest since it last
    /// went down.
    extreme: u8,
    /// 1 up, -1 down, 0 not yet moved.
    way: i8,
}

impl Swing {
    /// The channel moves to `v`: whether that completes a change, and how
    /// it stands after.
    fn to(self, v: u8) -> (bool, Swing) {
        let (e, v16) = (i16::from(self.extreme), i16::from(v));
        match self.way {
            1 if v16 >= e => (false, Swing { extreme: v, way: 1 }),
            -1 if v16 <= e => (false, Swing { extreme: v, way: -1 }),
            1 if e - v16 >= SWING => (true, Swing { extreme: v, way: -1 }),
            -1 if v16 - e >= SWING => (true, Swing { extreme: v, way: 1 }),
            0 if (v16 - e).abs() >= SWING => (true, Swing { extreme: v, way: if v16 > e { 1 } else { -1 } }),
            _ => (false, self),
        }
    }
}

/// The last check before a frame goes out: no key changes its light more
/// than [`MAX_CHANGES`] times in any second (three flashes, WCAG 2.3.1),
/// whatever asks for it: a look, a flash again, lights taking turns on a
/// key, or an agent switching a light between two colours. Each swing of
/// a channel by [`SWING`] or more counts once, however many frames it
/// takes, so pulses and waves pass untouched; channels swinging the same
/// way are one change even when they cross the line on different frames
/// (an orange dimming), channels swinging opposite ways (red to blue) are
/// a change every time. A key the agents' layer starts or stops covering is
/// a change too, whatever its colour: where the layer leaves a key, the
/// layers under it show (a profile, the music, the keyboard's own light),
/// so dark or dim against nothing is no smaller a change than red against
/// blue. A key that would change once more keeps what it shows.
#[derive(Debug, Default)]
pub struct FlashGuard {
    keys: HashMap<u8, KeyState>,
}

#[derive(Debug, Default)]
struct KeyState {
    channels: [Swing; 3],
    /// The way its last change went: 1 brighter, -1 darker, 0 neither (a
    /// change of colour, or none yet).
    way: i8,
    /// What it shows now: `None` when the frame leaves it to the layers
    /// under the agents' (dark, as far as this layer goes).
    shown: Option<Rgb>,
    /// When it changed lately, oldest first.
    changes: VecDeque<u64>,
}

impl FlashGuard {
    /// `frame` as it may go out at `now` (ms).
    pub fn check(&mut self, now: u64, frame: Vec<(u8, Rgb)>) -> Vec<(u8, Rgb)> {
        let mut wanted: HashMap<u8, Rgb> = HashMap::with_capacity(frame.len());
        let mut order: Vec<u8> = Vec::with_capacity(frame.len());
        for (led, color) in frame {
            if wanted.insert(led, color).is_none() {
                order.push(led);
            }
        }
        // Keys shown before and left out now come last, in LED order.
        let mut gone: Vec<u8> = self.keys.keys().copied().filter(|led| !wanted.contains_key(led)).collect();
        gone.sort_unstable();
        order.extend(gone);
        let mut out = Vec::with_capacity(order.len());
        for led in order {
            let want = wanted.get(&led).copied();
            let state = self.keys.entry(led).or_default();
            while state.changes.front().is_some_and(|&t| now.saturating_sub(t) >= WINDOW_MS) {
                state.changes.pop_front();
            }
            let target = want.unwrap_or([0, 0, 0]);
            let (mut up, mut down) = (false, false);
            let mut next = state.channels;
            for (c, &v) in next.iter_mut().zip(&target) {
                let (done, swing) = c.to(v);
                up |= done && swing.way > 0;
                down |= done && swing.way < 0;
                *c = swing;
            }
            let way = match (up, down) {
                (true, false) => 1,
                (false, true) => -1,
                _ => 0,
            };
            // Another channel going the way the last change went is the same
            // change, a little later. Covering a key or leaving it to the
            // layers under is a change of its own.
            let covers = state.shown.is_some() != want.is_some();
            let changed = covers || ((up || down) && !(way != 0 && way == state.way));
            let shows = if changed && state.changes.len() >= MAX_CHANGES {
                // Held: the key keeps what it shows.
                state.shown
            } else {
                if changed {
                    state.changes.push_back(now);
                    state.way = way;
                }
                state.channels = next;
                state.shown = want;
                want
            };
            if let Some(color) = shows {
                out.push((led, color));
            }
        }
        self.keys.retain(|_, s| s.shown.is_some() || !s.changes.is_empty());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::AgentSignalLooks;

    /// How often key 1 changes in any second, asked to switch every 50 ms
    /// between `a` and `b` (`None`: left to the layers under) for 3 s.
    fn most_changes(a: Option<Rgb>, b: Option<Rgb>) -> (usize, usize) {
        let mut guard = FlashGuard::default();
        let mut last = None;
        let mut changes = Vec::new();
        for i in 0..60u64 {
            let want = if i % 2 == 0 { a } else { b };
            let out = guard.check(i * 50, want.map(|c| vec![(1, c)]).unwrap_or_default());
            let now = out.iter().find(|k| k.0 == 1).map(|k| k.1);
            if i > 0 && now != last {
                changes.push(i * 50);
            }
            last = now;
        }
        let most = changes.iter().map(|&t| changes.iter().filter(|&&u| u >= t && u < t + 1000).count()).max();
        (most.unwrap_or(0), changes.len())
    }

    #[test]
    fn dark_or_dim_against_the_layers_under_counts_as_a_change() {
        for (a, b) in
            [(Some([0, 0, 0]), None), (Some([32, 32, 32]), None), (None, Some([0, 0, 0])), (Some([255, 0, 0]), None)]
        {
            let (most, all) = most_changes(a, b);
            assert!(most <= MAX_CHANGES, "{a:?}/{b:?}: {most}");
            assert!(all >= 6, "{a:?}/{b:?}: it still changes at the safe rate ({all})");
        }
        // A steady key is untouched; a key that stays uncovered too.
        assert_eq!(most_changes(Some([0, 0, 0]), Some([0, 0, 0])), (0, 0));
        assert_eq!(most_changes(None, None), (0, 0));
    }

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
        // The pulse starts dark, where the flashes ended, and is bright half
        // a cycle later.
        assert!((lit(&attention, attention.intro_ms()) - PULSE_FLOOR).abs() < 1e-3);
        let half = attention.intro_ms() + period_ms / 2;
        assert!((lit(&attention, half) - 1.0).abs() < 1e-3);

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

    /// The times `led` changes in the guard's output: its colour differs
    /// from the frame before.
    fn changes_out(frames: &[(u64, Vec<(u8, Rgb)>)], led: u8) -> Vec<u64> {
        let mut last: Option<Option<Rgb>> = None;
        let mut out = Vec::new();
        for (t, frame) in frames {
            let now = frame.iter().find(|k| k.0 == led).map(|k| k.1);
            if last.is_some_and(|l| l != now) {
                out.push(*t);
            }
            last = Some(now);
        }
        out
    }

    /// Runs `ask` (the frame wanted at each time) through a guard, at 20
    /// frames a second for `ms`.
    fn guarded(ms: u64, ask: impl Fn(u64) -> Vec<(u8, Rgb)>) -> Vec<(u64, Vec<(u8, Rgb)>)> {
        let mut guard = FlashGuard::default();
        (0..ms).step_by(50).map(|t| (t, guard.check(t, ask(t)))).collect()
    }

    fn most_changes_in_a_second(changes: &[u64]) -> usize {
        changes.iter().map(|&t| changes.iter().filter(|&&u| u >= t && u < t + 1000).count()).max().unwrap_or(0)
    }

    #[test]
    fn the_guard_holds_a_key_rather_than_let_it_flash_too_fast() {
        // An agent switching a light between two colours 20 times a second:
        // bright and grey, two colours, on and off, two of the same brightness.
        let pairs: [([u8; 3], [u8; 3]); 4] = [
            ([255, 255, 255], [0x40, 0x40, 0x40]),
            ([255, 0, 0], [0, 0, 255]),
            ([255, 0, 0], [0, 0, 0]),
            ([200, 120, 0], [120, 200, 0]),
        ];
        for (a, b) in pairs {
            let frames = guarded(10_000, |t| vec![(7, if (t / 50) % 2 == 0 { a } else { b }), (8, [0, 200, 0])]);
            let changes = changes_out(&frames, 7);
            assert!(most_changes_in_a_second(&changes) <= MAX_CHANGES, "{a:?}/{b:?}: {changes:?}");
            assert!(changes.len() >= 5 * 9, "{a:?}/{b:?}: it still changes, at the safe rate: {}", changes.len());
            assert!(frames.iter().all(|(_, f)| f.contains(&(8, [0, 200, 0]))), "a steady key is left alone");
        }
        // A key that comes and goes counts too: it shows what is under it.
        let frames = guarded(4_000, |t| if (t / 50) % 2 == 0 { vec![(3, [255, 255, 255])] } else { vec![] });
        let changes = changes_out(&frames, 3);
        assert!(most_changes_in_a_second(&changes) <= MAX_CHANGES, "{changes:?}");
        assert!(changes.len() >= 3 * 5, "{changes:?}");
        // Small steps add up to one change, not many: a slow ramp passes
        // frame by frame.
        let ramp = guarded(1_000, |t| vec![(1, [(t / 4).min(255) as u8, 0, 0])]);
        assert_eq!(changes_out(&ramp, 1).len(), ramp.len() - 1);
    }

    #[test]
    fn the_guard_lets_every_look_through_untouched() {
        for effect in ALL_EFFECTS {
            for speed in 1..=5 {
                let Some(look) = Look::of_effect([255, 128, 0], effect, speed) else { continue };
                let places = [Place::ALONE, Place { x: 0.0, y: 0.0, rank: 0, count: 10 }];
                let ask = |t: u64| -> Vec<(u8, Rgb)> {
                    places.iter().enumerate().map(|(i, p)| (i as u8, scale(look.color, look.level(t, *p)))).collect()
                };
                let mut guard = FlashGuard::default();
                for t in (0..20_000).step_by(50) {
                    assert_eq!(guard.check(t, ask(t)), ask(t), "{effect:?} speed {speed} at {t} ms");
                }
            }
        }
        // A flash again over the attention's pulse (D) too.
        let mut guard = FlashGuard::default();
        let pulse = Look::of_effect([255, 128, 0], AgentEffect::FlashThenPulse, 2).unwrap();
        for t in (0..20_000).step_by(50) {
            let again = if t >= 7_000 { burst_level(t % 7_000) } else { None };
            let want = vec![(1, scale(pulse.color, again.unwrap_or_else(|| pulse.level(t, Place::ALONE))))];
            assert_eq!(guard.check(t, want.clone()), want, "{t} ms");
        }
    }

    #[test]
    fn colours_scale_with_brightness() {
        assert_eq!(scale([200, 100, 0], 0.5), [100, 50, 0]);
        assert_eq!(scale([200, 100, 0], 2.0), [200, 100, 0]);
        assert_eq!(scale([200, 100, 0], -1.0), [0, 0, 0]);
    }
}
