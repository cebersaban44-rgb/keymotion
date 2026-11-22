//! Values the CLI and the app both check: colors, durations, effects,
//! signal and source names, ids and text.
//!
//! The CLI checks what the user or agent typed before sending, so mistakes
//! get a precise error code without a round trip. The app checks the same
//! values again with the same functions: it never trusts the client.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use crate::error::{AgentError, ErrorCode};

/// At most this much of a rejected input is echoed back in a message.
const ECHO_CHARS: usize = 40;

/// `input` for an error message: trimmed, cut short when long, and with
/// every character a terminal could act on written out ([`printable`]).
/// The CLI uses it for its own messages too.
pub fn shown(input: &str) -> String {
    let input = input.trim();
    if input.chars().count() <= ECHO_CHARS {
        printable(input)
    } else {
        let cut: String = input.chars().take(ECHO_CHARS).collect();
        format!("{}…", printable(&cut))
    }
}

/// Characters that must not reach a terminal or a log as they are: control
/// characters (C0, DEL and C1: an escape sequence can move the cursor,
/// recolour or rewrite what was printed) and the invisible and
/// direction-changing ones ([`check_text`] refuses them too).
pub fn is_unsafe_char(c: char) -> bool {
    c.is_control() || is_invisible(c)
}

/// `text` safe to print: each [unsafe character](is_unsafe_char) written
/// out as `\n`, `\r`, `\t` or `\u{1B}`, the rest as it is. A backslash
/// stays a backslash, so the result is for reading, not for parsing back.
pub fn printable(text: &str) -> String {
    if !text.chars().any(is_unsafe_char) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if is_unsafe_char(c) => out.push_str(&format!("\\u{{{:X}}}", u32::from(c))),
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------- colors

/// A light color, 8 bits per channel as typed. The app calibrates it for
/// the keyboard's LEDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// The color names the CLI accepts, besides `#RRGGBB` and `RRGGBB`.
pub const NAMED_COLORS: [(&str, Color); 10] = [
    ("red", Color::rgb(0xFF, 0x00, 0x00)),
    ("green", Color::rgb(0x00, 0xFF, 0x00)),
    ("blue", Color::rgb(0x00, 0x00, 0xFF)),
    ("amber", Color::rgb(0xFF, 0xBF, 0x00)),
    ("yellow", Color::rgb(0xFF, 0xFF, 0x00)),
    ("orange", Color::rgb(0xFF, 0x80, 0x00)),
    ("purple", Color::rgb(0x80, 0x00, 0xFF)),
    ("pink", Color::rgb(0xFF, 0x40, 0xA0)),
    ("cyan", Color::rgb(0x00, 0xFF, 0xFF)),
    ("white", Color::rgb(0xFF, 0xFF, 0xFF)),
];

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// `#RRGGBB`, `RRGGBB` or a name from [`NAMED_COLORS`], any case.
    pub fn parse(input: &str) -> Result<Self, AgentError> {
        let s = input.trim();
        if let Some((_, color)) = NAMED_COLORS.iter().find(|(name, _)| name.eq_ignore_ascii_case(s)) {
            return Ok(*color);
        }
        let hex = s.strip_prefix('#').unwrap_or(s);
        if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            // All ASCII, so the byte ranges are character boundaries.
            let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16);
            if let (Ok(r), Ok(g), Ok(b)) = (channel(0), channel(2), channel(4)) {
                return Ok(Self { r, g, b });
            }
        }
        let names: Vec<&str> = NAMED_COLORS.iter().map(|(n, _)| *n).collect();
        Err(AgentError::new(ErrorCode::BadColor, format!("'{}' is not a color.", shown(input)))
            .with_hint(format!("Use #RRGGBB, RRGGBB or a name: {}.", names.join(", "))))
    }

    /// `#rrggbb`, the form sent over the pipe.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}

impl FromStr for Color {
    type Err = AgentError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

// ------------------------------------------------------------- durations

/// The longest a signal or light may stay on.
pub const MAX_SIGNAL_DURATION: Duration = Duration::from_secs(2 * 60 * 60);

/// A duration as typed: a whole number and a unit, `500ms`, `30s`, `5m`,
/// `1h`. Longer than zero. Over the pipe durations travel as milliseconds.
pub fn parse_duration(input: &str) -> Result<Duration, AgentError> {
    let bad = |why: &str| {
        AgentError::new(ErrorCode::BadDuration, format!("'{}' is not a duration{why}.", shown(input)))
            .with_hint("Use a whole number and a unit: 500ms, 30s, 5m or 1h.")
    };
    let s = input.trim().to_ascii_lowercase();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (digits, unit) = s.split_at(split);
    if digits.is_empty() || digits.len() > 12 {
        return Err(bad(""));
    }
    let factor: u64 = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "" => return Err(bad(" (the unit is missing)")),
        _ => return Err(bad("")),
    };
    let value: u64 = digits.parse().map_err(|_| bad(""))?;
    let ms = value.checked_mul(factor).ok_or_else(|| bad(""))?;
    if ms == 0 {
        return Err(bad(" (it must be longer than zero)"));
    }
    Ok(Duration::from_millis(ms))
}

/// A signal's or light's duration in milliseconds, as sent over the pipe:
/// longer than zero, at most [`MAX_SIGNAL_DURATION`].
pub fn signal_duration_ms(ms: u64) -> Result<Duration, AgentError> {
    check_signal_duration(Duration::from_millis(ms))
}

/// Longer than zero, at most [`MAX_SIGNAL_DURATION`].
pub fn check_signal_duration(d: Duration) -> Result<Duration, AgentError> {
    if d.is_zero() {
        return Err(AgentError::new(ErrorCode::BadDuration, "The duration must be longer than zero."));
    }
    if d > MAX_SIGNAL_DURATION {
        return Err(AgentError::new(
            ErrorCode::BadDuration,
            format!("{} is too long; lights stay on for at most 2h.", format_duration(d)),
        ));
    }
    Ok(d)
}

/// [`parse_duration`] and [`check_signal_duration`] together.
pub fn parse_signal_duration(input: &str) -> Result<Duration, AgentError> {
    check_signal_duration(parse_duration(input)?)
}

/// The shortest exact form: `2h`, `90m`, `45s`, `1500ms`.
pub fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms == 0 {
        "0ms".to_string()
    } else if ms % 3_600_000 == 0 {
        format!("{}h", ms / 3_600_000)
    } else if ms % 60_000 == 0 {
        format!("{}m", ms / 60_000)
    } else if ms % 1_000 == 0 {
        format!("{}s", ms / 1_000)
    } else {
        format!("{ms}ms")
    }
}

/// Milliseconds of a duration, for the pipe.
pub fn duration_ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

// --------------------------------------------------------------- effects

/// How a light moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectKind {
    /// Steady.
    Solid,
    /// Fades in and out.
    Pulse,
    /// On and off.
    Blink,
    /// Sweeps across the keys.
    Wave,
    /// Quick on and off, as fast as is safe (headlight flash).
    Flash,
}

impl EffectKind {
    pub const ALL: [EffectKind; 5] = [Self::Solid, Self::Pulse, Self::Blink, Self::Wave, Self::Flash];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Pulse => "pulse",
            Self::Blink => "blink",
            Self::Wave => "wave",
            Self::Flash => "flash",
        }
    }

    pub fn parse(input: &str) -> Result<Self, AgentError> {
        let s = input.trim();
        Self::ALL.into_iter().find(|k| k.as_str().eq_ignore_ascii_case(s)).ok_or_else(|| {
            AgentError::new(ErrorCode::BadEffect, format!("'{}' is not an effect.", shown(input)))
                .with_hint("Use solid, pulse, blink, wave or flash.")
        })
    }

    /// Switches the light fully on and off, so the photosensitivity limit
    /// ([`MAX_FLASHES_PER_SECOND`]) applies.
    pub const fn flashes(self) -> bool {
        matches!(self, Self::Blink | Self::Flash)
    }
}

/// Which way a wave runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub const ALL: [Direction; 4] = [Self::Left, Self::Right, Self::Up, Self::Down];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    pub fn parse(input: &str) -> Result<Self, AgentError> {
        let s = input.trim();
        Self::ALL.into_iter().find(|d| d.as_str().eq_ignore_ascii_case(s)).ok_or_else(|| {
            AgentError::new(ErrorCode::BadEffect, format!("'{}' is not a direction.", shown(input)))
                .with_hint("Use left, right, up or down.")
        })
    }
}

pub const MIN_SPEED: u32 = 1;
pub const MAX_SPEED: u32 = 5;
pub const DEFAULT_SPEED: u32 = 3;
/// The most cycles an effect may be asked to run.
pub const MAX_TIMES: u32 = 100;

/// Photosensitivity (WCAG 2.3.1, "Three Flashes or Below Threshold"): a
/// light never switches on and off more than three times a second.
pub const MAX_FLASHES_PER_SECOND: u32 = 3;
/// The shortest on-and-off cycle: a third of a second, rounded up.
pub const MIN_FLASH_PERIOD: Duration = Duration::from_millis(1000_u64.div_ceil(MAX_FLASHES_PER_SECOND as u64));

/// `period` made safe for a light that switches on and off: never shorter
/// than [`MIN_FLASH_PERIOD`]. Whatever draws a blink or flash goes through
/// this.
pub fn clamp_flash_period(period: Duration) -> Duration {
    period.max(MIN_FLASH_PERIOD)
}

/// One cycle of each effect per speed (1 slowest … 5 fastest), in ms.
const PULSE_PERIODS: [u64; 5] = [3000, 2400, 1800, 1400, 1000];
const BLINK_PERIODS: [u64; 5] = [1500, 1200, 900, 600, 400];
const WAVE_PERIODS: [u64; 5] = [3000, 2400, 1800, 1400, 1000];
const FLASH_PERIODS: [u64; 5] = [600, 500, 450, 400, 334];

/// A checked effect: the speed is in range, the direction belongs to a
/// wave, a cycle count is not asked of a steady light, and a blink or
/// flash stays within [`MAX_FLASHES_PER_SECOND`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Effect {
    kind: EffectKind,
    direction: Option<Direction>,
    speed: u32,
    times: Option<u32>,
}

impl Effect {
    /// A steady light.
    pub const fn solid() -> Self {
        Self { kind: EffectKind::Solid, direction: None, speed: DEFAULT_SPEED, times: None }
    }

    /// Check an effect. `speed` defaults to [`DEFAULT_SPEED`]; a wave's
    /// direction to `right`.
    pub fn new(
        kind: EffectKind,
        direction: Option<Direction>,
        speed: Option<u32>,
        times: Option<u32>,
    ) -> Result<Self, AgentError> {
        let speed = speed.unwrap_or(DEFAULT_SPEED);
        if !(MIN_SPEED..=MAX_SPEED).contains(&speed) {
            return Err(AgentError::new(
                ErrorCode::BadValue,
                format!("Speed {speed} is out of range; use {MIN_SPEED} (slow) to {MAX_SPEED} (fast)."),
            ));
        }
        let direction = match (kind, direction) {
            (EffectKind::Wave, d) => Some(d.unwrap_or(Direction::Right)),
            (_, None) => None,
            (_, Some(_)) => {
                return Err(AgentError::new(ErrorCode::BadEffect, "Only the wave effect takes a direction."));
            }
        };
        if let Some(n) = times {
            if kind == EffectKind::Solid {
                return Err(AgentError::new(ErrorCode::BadEffect, "A solid light has no cycles to count.")
                    .with_hint("Give a duration instead, e.g. 30s."));
            }
            if !(1..=MAX_TIMES).contains(&n) {
                return Err(AgentError::new(
                    ErrorCode::BadValue,
                    format!("{n} times is out of range; use 1 to {MAX_TIMES}."),
                ));
            }
        }
        Ok(Self { kind, direction, speed, times })
    }

    /// From the strings and numbers the CLI and the pipe carry. Without an
    /// effect name the light is solid.
    pub fn from_parts(
        effect: Option<&str>,
        direction: Option<&str>,
        speed: Option<u32>,
        times: Option<u32>,
    ) -> Result<Self, AgentError> {
        let kind = effect.map(EffectKind::parse).transpose()?.unwrap_or(EffectKind::Solid);
        let direction = direction.map(Direction::parse).transpose()?;
        Self::new(kind, direction, speed, times)
    }

    pub const fn kind(&self) -> EffectKind {
        self.kind
    }

    /// Set for a wave only.
    pub const fn direction(&self) -> Option<Direction> {
        self.direction
    }

    pub const fn speed(&self) -> u32 {
        self.speed
    }

    /// How many cycles to run; `None` runs until the light's time is up.
    pub const fn times(&self) -> Option<u32> {
        self.times
    }

    /// One cycle of the effect; `None` for a steady light. A blink or
    /// flash is never shorter than [`MIN_FLASH_PERIOD`].
    pub fn period(&self) -> Option<Duration> {
        let i = (self.speed.clamp(MIN_SPEED, MAX_SPEED) - MIN_SPEED) as usize;
        let ms = match self.kind {
            EffectKind::Solid => return None,
            EffectKind::Pulse => PULSE_PERIODS[i],
            EffectKind::Blink => BLINK_PERIODS[i],
            EffectKind::Wave => WAVE_PERIODS[i],
            EffectKind::Flash => FLASH_PERIODS[i],
        };
        let period = Duration::from_millis(ms);
        Some(if self.kind.flashes() { clamp_flash_period(period) } else { period })
    }

    /// On-and-off cycles per second; 0 for effects that do not switch the
    /// light off.
    pub fn flashes_per_second(&self) -> f64 {
        match self.period() {
            Some(p) if self.kind.flashes() => 1.0 / p.as_secs_f64(),
            _ => 0.0,
        }
    }

    /// How long `times` cycles take; `None` without a cycle count.
    pub fn run_time(&self) -> Option<Duration> {
        Some(self.period()? * self.times?)
    }
}

// --------------------------------------------------------------- signals

/// The ready-made signals. How each one looks is the user's choice in the
/// app; an agent may override it per call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalName {
    /// A task finished.
    Done,
    /// The agent waits for the user.
    Attention,
    /// Something failed.
    Error,
    /// The agent is working (lights nothing by default; clears the
    /// source's earlier signals).
    Busy,
    /// A short note.
    Info,
}

impl SignalName {
    pub const ALL: [SignalName; 5] = [Self::Done, Self::Attention, Self::Error, Self::Busy, Self::Info];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Attention => "attention",
            Self::Error => "error",
            Self::Busy => "busy",
            Self::Info => "info",
        }
    }

    pub fn parse(input: &str) -> Result<Self, AgentError> {
        let s = input.trim();
        Self::ALL.into_iter().find(|n| n.as_str().eq_ignore_ascii_case(s)).ok_or_else(|| {
            AgentError::new(ErrorCode::BadSignal, format!("'{}' is not a signal.", shown(input)))
                .with_hint("Use done, attention, error, busy or info.")
        })
    }
}

impl fmt::Display for SignalName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ----------------------------------------------------------- names, ids

/// Who sends a request: `cli`, `claude-code:3f9a`, `tests`. Each source
/// has its own signals and cannot clear another's.
pub const MAX_SOURCE_LEN: usize = 64;
/// The source when none is given.
pub const DEFAULT_SOURCE: &str = "cli";
/// A signal's id; the default is `<source>:<signal>`.
pub const MAX_SIGNAL_ID_LEN: usize = 128;
/// A request's id, chosen by the client and echoed in the answer.
pub const MAX_REQUEST_ID_LEN: usize = 64;
/// A settings key for `settings get`.
pub const MAX_SETTINGS_KEY_LEN: usize = 64;

/// Lowercase letters, digits and `:._-`, starting with a letter or digit.
fn is_name(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b':' | b'.' | b'_' | b'-'))
}

/// A source name: up to [`MAX_SOURCE_LEN`] of `[a-z0-9:._-]`. Upper case
/// is folded to lower case.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceName(String);

impl SourceName {
    pub fn parse(input: &str) -> Result<Self, AgentError> {
        let s = input.trim().to_ascii_lowercase();
        if is_name(&s, MAX_SOURCE_LEN) {
            Ok(Self(s))
        } else {
            Err(AgentError::new(ErrorCode::BadSource, format!("'{}' is not a valid source name.", shown(input)))
                .with_hint(format!("Use up to {MAX_SOURCE_LEN} letters, digits and : . _ -, e.g. claude-code.")))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SourceName {
    fn default() -> Self {
        Self(DEFAULT_SOURCE.to_string())
    }
}

impl fmt::Display for SourceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A signal's id: up to [`MAX_SIGNAL_ID_LEN`] of `[a-z0-9:._-]`. A new
/// signal with the same id replaces the old one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignalId(String);

impl SignalId {
    pub fn parse(input: &str) -> Result<Self, AgentError> {
        let s = input.trim().to_ascii_lowercase();
        if is_name(&s, MAX_SIGNAL_ID_LEN) {
            Ok(Self(s))
        } else {
            Err(AgentError::new(ErrorCode::BadId, format!("'{}' is not a valid id.", shown(input)))
                .with_hint(format!("Use up to {MAX_SIGNAL_ID_LEN} letters, digits and : . _ -.")))
        }
    }

    /// `<source>:<signal>`, the id when none is given.
    pub fn default_for(source: &SourceName, signal: SignalName) -> Self {
        Self(format!("{}:{}", source.as_str(), signal.as_str()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SignalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A request id: 1 to [`MAX_REQUEST_ID_LEN`] letters (either case),
/// digits and `:._-`.
pub fn is_request_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_REQUEST_ID_LEN
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'.' | b'_' | b'-'))
}

/// A settings key: up to [`MAX_SETTINGS_KEY_LEN`] of `[a-z0-9._-]`.
pub fn check_settings_key(input: &str) -> Result<String, AgentError> {
    let s = input.trim().to_ascii_lowercase();
    if is_name(&s, MAX_SETTINGS_KEY_LEN) && !s.contains(':') {
        Ok(s)
    } else {
        Err(AgentError::new(ErrorCode::BadValue, format!("'{}' is not a settings key.", shown(input))))
    }
}

// ------------------------------------------------------------------ text

/// A screen card: at most this many characters…
pub const MAX_CARD_TEXT_CHARS: usize = 60;
/// …on at most this many lines.
pub const MAX_CARD_LINES: usize = 2;
/// A card's title.
pub const MAX_CARD_TITLE_CHARS: usize = 24;

/// Characters that change how text around them looks without being seen
/// themselves: zero-width characters, direction overrides, the byte-order
/// mark. Refused so a card shows what it says.
fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
}

/// Text for the keyboard's screen: not empty, at most `max_chars`
/// characters on at most `max_lines` lines (`\n` between lines), and only
/// visible characters.
pub fn check_text(text: &str, max_chars: usize, max_lines: usize) -> Result<(), AgentError> {
    if text.trim().is_empty() {
        return Err(AgentError::new(ErrorCode::BadText, "The text is empty."));
    }
    if let Some(c) = text.chars().find(|&c| (c.is_control() && c != '\n') || is_invisible(c)) {
        return Err(AgentError::new(
            ErrorCode::BadText,
            format!("The text has a character the screen cannot show (U+{:04X}).", u32::from(c)),
        ));
    }
    let lines = text.split('\n').count();
    if lines > max_lines {
        return Err(AgentError::new(
            ErrorCode::TextTooLong,
            format!("The text has {lines} lines; at most {max_lines} fit."),
        ));
    }
    let chars = text.chars().filter(|&c| c != '\n').count();
    if chars > max_chars {
        return Err(AgentError::new(
            ErrorCode::TextTooLong,
            format!("The text has {chars} characters; at most {max_chars} fit."),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echoed_input_cannot_drive_a_terminal() {
        assert_eq!(printable("plain red, ünicode ok"), "plain red, ünicode ok");
        assert_eq!(printable("\u{1b}[31mred\u{7}"), "\\u{1B}[31mred\\u{7}");
        assert_eq!(printable("a\nb\r\tc"), "a\\nb\\r\\tc");
        // DEL, C1 (CSI) and a direction override.
        assert_eq!(printable("\u{7f}\u{9b}2J\u{202e}x"), "\\u{7F}\\u{9B}2J\\u{202E}x");
        let e = Color::parse("\u{1b}[2J red").unwrap_err();
        assert!(!e.message.chars().any(is_unsafe_char), "{:?}", e.message);
        assert!(e.message.contains("\\u{1B}[2J red"), "{}", e.message);
        let long = format!("{}\u{1b}", "x".repeat(39));
        assert_eq!(shown(&long), format!("{}\\u{{1B}}", "x".repeat(39)));
        assert!(!shown(&"\u{9b}".repeat(60)).chars().any(is_unsafe_char));
    }

    #[test]
    fn colors_from_hex_and_names() {
        assert_eq!(Color::parse("#FF8000").unwrap(), Color::rgb(0xFF, 0x80, 0x00));
        assert_eq!(Color::parse("ff8000").unwrap(), Color::rgb(0xFF, 0x80, 0x00));
        assert_eq!(Color::parse(" #00aaFF ").unwrap(), Color::rgb(0x00, 0xAA, 0xFF));
        assert_eq!(Color::parse("Amber").unwrap(), Color::rgb(0xFF, 0xBF, 0x00));
        for (name, color) in NAMED_COLORS {
            assert_eq!(Color::parse(name).unwrap(), color);
            assert_eq!(Color::parse(&color.hex()).unwrap(), color, "round trip of {name}");
        }
        assert_eq!(Color::rgb(1, 2, 255).to_string(), "#0102ff");
        assert_eq!("cyan".parse::<Color>().unwrap(), Color::rgb(0, 0xFF, 0xFF));
    }

    #[test]
    fn bad_colors_are_refused() {
        for bad in ["", "#", "#fff", "#ff800", "#ff80000", "gg0000", "#ff 800", "magenta", "ééé", "#ﬀ8000", "0x123456"]
        {
            let e = Color::parse(bad).unwrap_err();
            assert_eq!(e.code, ErrorCode::BadColor, "{bad:?}");
        }
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration(" 2H ").unwrap(), Duration::from_secs(7200));
        for bad in ["", "5", "s", "-5s", "1.5s", "5 s", "5sec", "1h30m", "0s", "0ms", "99999999999999h", "５s"] {
            assert_eq!(parse_duration(bad).unwrap_err().code, ErrorCode::BadDuration, "{bad:?}");
        }
    }

    #[test]
    fn signal_durations_stop_at_two_hours() {
        assert_eq!(parse_signal_duration("2h").unwrap(), MAX_SIGNAL_DURATION);
        assert_eq!(parse_signal_duration("120m").unwrap(), MAX_SIGNAL_DURATION);
        assert_eq!(parse_signal_duration("121m").unwrap_err().code, ErrorCode::BadDuration);
        assert_eq!(parse_signal_duration("3h").unwrap_err().code, ErrorCode::BadDuration);
        assert_eq!(signal_duration_ms(7_200_000).unwrap(), MAX_SIGNAL_DURATION);
        assert_eq!(signal_duration_ms(7_200_001).unwrap_err().code, ErrorCode::BadDuration);
        assert_eq!(signal_duration_ms(0).unwrap_err().code, ErrorCode::BadDuration);
        assert_eq!(signal_duration_ms(u64::MAX).unwrap_err().code, ErrorCode::BadDuration);
    }

    #[test]
    fn durations_print_in_their_shortest_form() {
        for (ms, text) in [(0, "0ms"), (1500, "1500ms"), (45_000, "45s"), (5_400_000, "90m"), (7_200_000, "2h")] {
            let d = Duration::from_millis(ms);
            assert_eq!(format_duration(d), text);
            if ms > 0 {
                assert_eq!(parse_duration(text).unwrap(), d);
            }
        }
    }

    #[test]
    fn effects_check_their_options() {
        let wave = Effect::from_parts(Some("wave"), None, None, Some(2)).unwrap();
        assert_eq!(wave.direction(), Some(Direction::Right));
        assert_eq!(wave.speed(), DEFAULT_SPEED);
        assert_eq!(wave.run_time(), Some(Duration::from_millis(1800 * 2)));
        assert_eq!(Effect::from_parts(None, None, None, None).unwrap(), Effect::solid());
        assert_eq!(Effect::solid().period(), None);
        assert_eq!(
            Effect::from_parts(Some("Pulse"), None, Some(5), None).unwrap().period(),
            Some(Duration::from_secs(1))
        );

        let code = |e: Result<Effect, AgentError>| e.unwrap_err().code;
        assert_eq!(code(Effect::from_parts(Some("strobe"), None, None, None)), ErrorCode::BadEffect);
        assert_eq!(code(Effect::from_parts(Some("blink"), Some("left"), None, None)), ErrorCode::BadEffect);
        assert_eq!(code(Effect::from_parts(Some("wave"), Some("sideways"), None, None)), ErrorCode::BadEffect);
        assert_eq!(code(Effect::from_parts(Some("solid"), None, None, Some(3))), ErrorCode::BadEffect);
        assert_eq!(code(Effect::from_parts(Some("blink"), None, Some(0), None)), ErrorCode::BadValue);
        assert_eq!(code(Effect::from_parts(Some("blink"), None, Some(6), None)), ErrorCode::BadValue);
        assert_eq!(code(Effect::from_parts(Some("blink"), None, None, Some(0))), ErrorCode::BadValue);
        assert_eq!(code(Effect::from_parts(Some("blink"), None, None, Some(MAX_TIMES + 1))), ErrorCode::BadValue);
    }

    #[test]
    fn nothing_flashes_more_than_three_times_a_second() {
        assert!(MIN_FLASH_PERIOD.as_secs_f64() >= 1.0 / f64::from(MAX_FLASHES_PER_SECOND));
        for kind in EffectKind::ALL {
            for speed in MIN_SPEED..=MAX_SPEED {
                let effect = Effect::new(kind, None, Some(speed), None).unwrap();
                let rate = effect.flashes_per_second();
                assert!(rate <= f64::from(MAX_FLASHES_PER_SECOND), "{kind:?} at speed {speed}: {rate}/s");
                if kind.flashes() {
                    assert!(effect.period().unwrap() >= MIN_FLASH_PERIOD, "{kind:?} at speed {speed}");
                    assert!(rate > 0.0);
                } else {
                    assert_eq!(rate, 0.0);
                }
            }
        }
        // Whatever period anything asks for, a flashing light gets a safe one.
        assert_eq!(clamp_flash_period(Duration::from_millis(50)), MIN_FLASH_PERIOD);
        assert_eq!(clamp_flash_period(Duration::ZERO), MIN_FLASH_PERIOD);
        assert_eq!(clamp_flash_period(Duration::from_secs(1)), Duration::from_secs(1));
        assert!(1.0 / clamp_flash_period(Duration::from_millis(1)).as_secs_f64() <= 3.0);
    }

    #[test]
    fn signal_names() {
        for name in SignalName::ALL {
            assert_eq!(SignalName::parse(name.as_str()).unwrap(), name);
        }
        assert_eq!(SignalName::parse("DONE").unwrap(), SignalName::Done);
        assert_eq!(SignalName::parse("finished").unwrap_err().code, ErrorCode::BadSignal);
    }

    #[test]
    fn source_names() {
        assert_eq!(SourceName::parse("claude-code:3f9a").unwrap().as_str(), "claude-code:3f9a");
        assert_eq!(SourceName::parse("Codex").unwrap().as_str(), "codex");
        assert_eq!(SourceName::parse("my_build.v2").unwrap().as_str(), "my_build.v2");
        assert_eq!(SourceName::default().as_str(), DEFAULT_SOURCE);
        assert!(SourceName::parse(&"a".repeat(MAX_SOURCE_LEN)).is_ok());
        for bad in ["", " ", "-x", ":x", "a b", "a/b", "a\\b", "çay", "a\nb", "a;b", &"a".repeat(MAX_SOURCE_LEN + 1)] {
            assert_eq!(SourceName::parse(bad).unwrap_err().code, ErrorCode::BadSource, "{bad:?}");
        }
    }

    #[test]
    fn signal_ids() {
        let source = SourceName::parse("claude-code:3f9a").unwrap();
        assert_eq!(SignalId::default_for(&source, SignalName::Attention).as_str(), "claude-code:3f9a:attention");
        let longest = SourceName::parse(&"a".repeat(MAX_SOURCE_LEN)).unwrap();
        assert!(SignalId::parse(SignalId::default_for(&longest, SignalName::Attention).as_str()).is_ok());
        assert_eq!(SignalId::parse("build:1").unwrap().as_str(), "build:1");
        assert_eq!(SignalId::parse("a b").unwrap_err().code, ErrorCode::BadId);
        assert_eq!(SignalId::parse(&"a".repeat(MAX_SIGNAL_ID_LEN + 1)).unwrap_err().code, ErrorCode::BadId);
    }

    #[test]
    fn request_ids_and_settings_keys() {
        assert!(is_request_id("1"));
        assert!(is_request_id("cli-42.A:b_c"));
        assert!(!is_request_id(""));
        assert!(!is_request_id("a b"));
        assert!(!is_request_id(&"a".repeat(MAX_REQUEST_ID_LEN + 1)));
        assert_eq!(check_settings_key("Agents.Screen").unwrap(), "agents.screen");
        assert_eq!(check_settings_key("a:b").unwrap_err().code, ErrorCode::BadValue);
        assert_eq!(check_settings_key("../x").unwrap_err().code, ErrorCode::BadValue);
    }

    #[test]
    fn card_text_limits() {
        let ok = |t: &str| check_text(t, MAX_CARD_TEXT_CHARS, MAX_CARD_LINES);
        assert!(ok("Build finished · 4 min").is_ok());
        assert!(ok("Tests passed\nmerge ready").is_ok());
        assert!(ok(&"é".repeat(MAX_CARD_TEXT_CHARS)).is_ok(), "characters, not bytes");
        assert!(ok("黑爵 AK980").is_ok());
        assert_eq!(ok(&"x".repeat(MAX_CARD_TEXT_CHARS + 1)).unwrap_err().code, ErrorCode::TextTooLong);
        assert_eq!(ok("one\ntwo\nthree").unwrap_err().code, ErrorCode::TextTooLong);
        assert_eq!(ok("").unwrap_err().code, ErrorCode::BadText);
        assert_eq!(ok("  \n ").unwrap_err().code, ErrorCode::BadText);
        for bad in ["tab\there", "bell\u{7}", "cr\r\nlf", "rtl\u{202E}txet", "zero\u{200B}width", "\u{FEFF}bom"] {
            assert_eq!(ok(bad).unwrap_err().code, ErrorCode::BadText, "{bad:?}");
        }
        assert!(check_text(&"t".repeat(MAX_CARD_TITLE_CHARS), MAX_CARD_TITLE_CHARS, 1).is_ok());
    }

    #[test]
    fn long_input_is_cut_short_in_messages() {
        let e = Color::parse(&"z".repeat(10_000)).unwrap_err();
        assert!(e.message.len() < 100, "{}", e.message);
    }
}
