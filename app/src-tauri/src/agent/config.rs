//! The agents' settings as keys and values, for `settings_get` and
//! `settings_set`: `signals.done.color = "#00ff00"`, `away.after = "5m"`,
//! `night.from = "23:00"`.
//!
//! Agents may change the looks and the behaviour (owner's decision,
//! 2026-11-22). Access, the permissions, "agents may change these
//! settings", "hide while playing" and the screen's hourly limit are the
//! user's alone ([`agent_may_set`]); the coding apps' connections are not
//! settings at all.

use crate::dto::{AgentEffect, AgentLook, AgentNightMode, AgentProgressKeys, AgentSettings};
use kb_agent::values::{format_duration, parse_duration, MAX_SIGNAL_DURATION};
use kb_agent::{AgentError, Color, ErrorCode};
use serde_json::{json, Value};
use std::time::Duration;

/// The looks, by key prefix.
const LOOKS: [&str; 6] =
    ["signals.done", "signals.attention", "signals.error", "signals.busy", "signals.info", "usage_limit"];
const LOOK_FIELDS: [&str; 5] = ["color", "effect", "speed", "duration", "until_answered"];

/// Only the user changes these, in Keymotion.
pub const USER_ONLY: [&str; 7] = [
    "enabled",
    "permissions.screen",
    "permissions.focus",
    "permissions.profiles",
    "agents_may_edit",
    "hide_in_game",
    "screen.per_hour",
];

/// The other keys, in the order `settings_get` lists them.
const OTHERS: [&str; 22] = [
    "usage_limit.on",
    "sessions.on",
    "sessions.color",
    "away.on",
    "away.after",
    "unanswered.on",
    "unanswered.every",
    "unanswered.up_to",
    "progress.keys",
    "progress.color",
    "long_task.on",
    "long_task.after",
    "long_task.show",
    "night.on",
    "night.from",
    "night.to",
    "night.mode",
    "helper.on",
    "helper.color",
    // Kept last: rarely changed.
    "screen.per_hour",
    "agents_may_edit",
    "hide_in_game",
];

/// The effects' names on the pipe.
pub const EFFECTS: [(AgentEffect, &str); 9] = [
    (AgentEffect::Solid, "solid"),
    (AgentEffect::Pulse, "pulse"),
    (AgentEffect::Blink, "blink"),
    (AgentEffect::Wave, "wave"),
    (AgentEffect::Flash, "flash"),
    (AgentEffect::WaveThenSolid, "wave_then_solid"),
    (AgentEffect::FlashThenPulse, "flash_then_pulse"),
    (AgentEffect::BlinkTwiceThenSolid, "blink_twice_then_solid"),
    (AgentEffect::Off, "off"),
];

pub fn effect_name(effect: AgentEffect) -> &'static str {
    EFFECTS.iter().find(|(e, _)| *e == effect).map_or("solid", |(_, n)| n)
}

/// Every key, in order.
pub fn keys() -> Vec<String> {
    let mut out: Vec<String> =
        ["enabled", "permissions.screen", "permissions.focus", "permissions.profiles"].map(String::from).to_vec();
    for prefix in LOOKS {
        out.extend(LOOK_FIELDS.iter().map(|f| format!("{prefix}.{f}")));
    }
    out.extend(OTHERS.iter().map(|k| k.to_string()));
    out
}

/// Whether an agent may change `key` (with `agents_may_edit` on).
pub fn agent_may_set(key: &str) -> bool {
    !USER_ONLY.contains(&key)
}

/// `signals.done.color` → (`signals.done`, `color`).
fn look_field(key: &str) -> Option<(&str, &str)> {
    key.rsplit_once('.').filter(|(prefix, field)| LOOKS.contains(prefix) && LOOK_FIELDS.contains(field))
}

fn look<'a>(s: &'a AgentSettings, prefix: &str) -> Option<&'a AgentLook> {
    Some(match prefix {
        "signals.done" => &s.signals.done,
        "signals.attention" => &s.signals.attention,
        "signals.error" => &s.signals.error,
        "signals.busy" => &s.signals.busy,
        "signals.info" => &s.signals.info,
        "usage_limit" => &s.usage_limit.look,
        _ => return None,
    })
}

fn look_mut<'a>(s: &'a mut AgentSettings, prefix: &str) -> Option<&'a mut AgentLook> {
    Some(match prefix {
        "signals.done" => &mut s.signals.done,
        "signals.attention" => &mut s.signals.attention,
        "signals.error" => &mut s.signals.error,
        "signals.busy" => &mut s.signals.busy,
        "signals.info" => &mut s.signals.info,
        "usage_limit" => &mut s.usage_limit.look,
        _ => return None,
    })
}

fn hex(c: [u8; 3]) -> Value {
    Value::String(Color::rgb(c[0], c[1], c[2]).hex())
}

fn secs(s: u32) -> Value {
    Value::String(format_duration(Duration::from_secs(u64::from(s))))
}

fn clock(min: u16) -> Value {
    Value::String(format!("{:02}:{:02}", min / 60, min % 60))
}

/// The value of `key`, as `settings_get` shows it; `None` for no such key.
pub fn get(s: &AgentSettings, key: &str) -> Option<Value> {
    if let Some((prefix, field)) = look_field(key) {
        if let Some(l) = look(s, prefix) {
            return Some(match field {
                "color" => hex(l.color),
                "effect" => json!(effect_name(l.effect)),
                "speed" => json!(l.speed),
                "duration" => secs(l.duration_s),
                "until_answered" => json!(l.until_answered),
                _ => return None,
            });
        }
    }
    Some(match key {
        "enabled" => json!(s.enabled),
        "permissions.screen" => json!(s.screen),
        "permissions.focus" => json!(s.focus),
        "permissions.profiles" => json!(s.profiles),
        "agents_may_edit" => json!(s.agents_may_edit),
        "hide_in_game" => json!(s.hide_in_game),
        "screen.per_hour" => json!(s.screen_per_hour),
        "usage_limit.on" => json!(s.usage_limit.on),
        "sessions.on" => json!(s.sessions.on),
        "sessions.color" => hex(s.sessions.color),
        "away.on" => json!(s.away.on),
        "away.after" => secs(s.away.after_s),
        "unanswered.on" => json!(s.unanswered.on),
        "unanswered.every" => secs(s.unanswered.every_s),
        "unanswered.up_to" => secs(s.unanswered.up_to_s),
        "progress.keys" => json!(match s.progress.keys {
            AgentProgressKeys::FRow => "f-row",
            AgentProgressKeys::Numbers => "numbers",
        }),
        "progress.color" => hex(s.progress.color),
        "long_task.on" => json!(s.long_task.on),
        "long_task.after" => secs(s.long_task.after_s),
        "long_task.show" => secs(s.long_task.show_s),
        "night.on" => json!(s.night.on),
        "night.from" => clock(s.night.from_min),
        "night.to" => clock(s.night.to_min),
        "night.mode" => json!(match s.night.mode {
            AgentNightMode::Dim => "dim",
            AgentNightMode::Off => "off",
        }),
        "helper.on" => json!(s.helper.on),
        "helper.color" => hex(s.helper.color),
        _ => return None,
    })
}

fn bad(key: &str, why: impl Into<String>) -> AgentError {
    AgentError::new(ErrorCode::BadValue, format!("{key}: {}", why.into()))
}

fn text<'a>(key: &str, v: &'a Value) -> Result<&'a str, AgentError> {
    v.as_str().ok_or_else(|| bad(key, "give it as text."))
}

fn flag(key: &str, v: &Value) -> Result<bool, AgentError> {
    match v {
        Value::Bool(b) => Ok(*b),
        Value::String(s) if matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "on") => Ok(true),
        Value::String(s) if matches!(s.trim().to_ascii_lowercase().as_str(), "false" | "off") => Ok(false),
        _ => Err(bad(key, "use true or false.")),
    }
}

fn color(key: &str, v: &Value) -> Result<[u8; 3], AgentError> {
    let c = Color::parse(text(key, v)?).map_err(|e| AgentError { message: format!("{key}: {}", e.message), ..e })?;
    Ok([c.r, c.g, c.b])
}

/// A duration as text, in whole seconds, from `min` to `max`.
fn duration(key: &str, v: &Value, min: Duration, max: Duration) -> Result<u32, AgentError> {
    let d = parse_duration(text(key, v)?).map_err(|e| AgentError { message: format!("{key}: {}", e.message), ..e })?;
    let out_of_range = || {
        AgentError::new(
            ErrorCode::BadDuration,
            format!("{key}: use {} to {}.", format_duration(min), format_duration(max)),
        )
    };
    if d.subsec_millis() != 0 {
        return Err(AgentError::new(ErrorCode::BadDuration, format!("{key}: use whole seconds, e.g. 30s or 5m.")));
    }
    if d < min || d > max {
        return Err(out_of_range());
    }
    u32::try_from(d.as_secs()).map_err(|_| out_of_range())
}

fn minutes(m: u64) -> Duration {
    Duration::from_secs(m * 60)
}

/// `HH:MM`, 24-hour.
fn time_of_day(key: &str, v: &Value) -> Result<u16, AgentError> {
    let s = text(key, v)?.trim();
    let parsed = s.split_once(':').and_then(|(h, m)| {
        let ok = |p: &str| (1..=2).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit());
        (ok(h) && m.len() == 2 && ok(m)).then(|| (h.parse::<u16>().ok(), m.parse::<u16>().ok()))
    });
    match parsed {
        Some((Some(h), Some(m))) if h < 24 && m < 60 => Ok(h * 60 + m),
        _ => Err(bad(key, "use a time like 23:00.")),
    }
}

/// A whole number, given as a number or as text: `keymotion-cli settings
/// set` sends what was typed as text.
fn whole(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn speed(key: &str, v: &Value) -> Result<u8, AgentError> {
    match whole(v) {
        Some(n @ 1..=5) => Ok(n as u8),
        _ => Err(bad(key, "use 1 (slow) to 5 (fast).")),
    }
}

/// Set `key` to `value` in `s`. Checks the value, not who may change it.
pub fn set(s: &mut AgentSettings, key: &str, value: &Value) -> Result<(), AgentError> {
    if let Some((prefix, field)) = look_field(key) {
        if let Some(l) = look_mut(s, prefix) {
            match field {
                "color" => l.color = color(key, value)?,
                "effect" => {
                    let name = text(key, value)?.trim().to_ascii_lowercase();
                    l.effect = EFFECTS.iter().find(|(_, n)| *n == name).map(|(e, _)| *e).ok_or_else(|| {
                        let names: Vec<&str> = EFFECTS.iter().map(|(_, n)| *n).collect();
                        AgentError::new(ErrorCode::BadEffect, format!("{key}: use {}.", names.join(", ")))
                    })?;
                }
                "speed" => l.speed = speed(key, value)?,
                "duration" => l.duration_s = duration(key, value, Duration::from_secs(1), MAX_SIGNAL_DURATION)?,
                "until_answered" => l.until_answered = flag(key, value)?,
                _ => return Err(unknown(key)),
            }
            return Ok(());
        }
    }
    match key {
        "enabled" => s.enabled = flag(key, value)?,
        "permissions.screen" => s.screen = flag(key, value)?,
        "permissions.focus" => s.focus = flag(key, value)?,
        "permissions.profiles" => s.profiles = flag(key, value)?,
        "agents_may_edit" => s.agents_may_edit = flag(key, value)?,
        "hide_in_game" => s.hide_in_game = flag(key, value)?,
        "screen.per_hour" => {
            s.screen_per_hour = match whole(value) {
                Some(n @ 1..=20) => n as u8,
                _ => return Err(bad(key, "use 1 to 20.")),
            }
        }
        "usage_limit.on" => s.usage_limit.on = flag(key, value)?,
        "sessions.on" => s.sessions.on = flag(key, value)?,
        "sessions.color" => s.sessions.color = color(key, value)?,
        "away.on" => s.away.on = flag(key, value)?,
        "away.after" => s.away.after_s = duration(key, value, minutes(1), minutes(60))?,
        "unanswered.on" => s.unanswered.on = flag(key, value)?,
        "unanswered.every" => s.unanswered.every_s = duration(key, value, minutes(1), minutes(30))?,
        "unanswered.up_to" => s.unanswered.up_to_s = duration(key, value, minutes(1), minutes(120))?,
        "progress.keys" => {
            s.progress.keys = match text(key, value)?.trim().to_ascii_lowercase().as_str() {
                "f-row" => AgentProgressKeys::FRow,
                "numbers" => AgentProgressKeys::Numbers,
                _ => return Err(bad(key, "use f-row or numbers.")),
            }
        }
        "progress.color" => s.progress.color = color(key, value)?,
        "long_task.on" => s.long_task.on = flag(key, value)?,
        "long_task.after" => s.long_task.after_s = duration(key, value, minutes(1), minutes(240))?,
        "long_task.show" => s.long_task.show_s = duration(key, value, Duration::from_secs(10), minutes(30))?,
        "night.on" => s.night.on = flag(key, value)?,
        "night.from" => s.night.from_min = time_of_day(key, value)?,
        "night.to" => s.night.to_min = time_of_day(key, value)?,
        "night.mode" => {
            s.night.mode = match text(key, value)?.trim().to_ascii_lowercase().as_str() {
                "dim" => AgentNightMode::Dim,
                "off" => AgentNightMode::Off,
                _ => return Err(bad(key, "use dim or off.")),
            }
        }
        "helper.on" => s.helper.on = flag(key, value)?,
        "helper.color" => s.helper.color = color(key, value)?,
        _ => return Err(unknown(key)),
    }
    Ok(())
}

pub fn unknown(key: &str) -> AgentError {
    AgentError::new(ErrorCode::BadValue, format!("Keymotion has no setting '{key}'."))
        .with_hint("keymotion-cli settings get lists them.")
}

/// A value as the page shows it: text without quotes.
pub fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_may_come_as_text_from_the_command_line() {
        let mut s = AgentSettings::default();
        set(&mut s, "signals.done.speed", &json!("5")).unwrap();
        set(&mut s, "signals.info.speed", &json!(1)).unwrap();
        assert_eq!((get(&s, "signals.done.speed"), get(&s, "signals.info.speed")), (Some(json!(5)), Some(json!(1))));
        assert_eq!(set(&mut s, "signals.done.speed", &json!("fast")).unwrap_err().code, ErrorCode::BadValue);
        assert_eq!(set(&mut s, "signals.done.speed", &json!("9")).unwrap_err().code, ErrorCode::BadValue);
        set(&mut s, "night.on", &json!("true")).unwrap();
        assert_eq!(get(&s, "night.on"), Some(json!(true)));
    }

    #[test]
    fn every_key_reads_back_what_it_sets() {
        let defaults = AgentSettings::default();
        let all = keys();
        assert_eq!(all.len(), 4 + LOOKS.len() * LOOK_FIELDS.len() + OTHERS.len());
        let mut seen = std::collections::HashSet::new();
        for key in &all {
            assert!(seen.insert(key.clone()), "{key} twice");
            assert!(kb_agent::values::check_settings_key(key).is_ok(), "{key} is not a settings key");
            let value = get(&defaults, key).unwrap_or_else(|| panic!("{key} cannot be read"));
            let mut s = defaults;
            set(&mut s, key, &value).unwrap_or_else(|e| panic!("{key} = {value}: {e}"));
            assert_eq!(s, defaults, "{key} = {value} changed something");
        }
        assert!(get(&defaults, "signals.done.glow").is_none());
        assert!(get(&defaults, "nothing").is_none());
    }

    #[test]
    fn the_defaults_read_as_approved() {
        let s = AgentSettings::default();
        let g = |k| get(&s, k).unwrap();
        assert_eq!(g("enabled"), json!(false));
        for k in ["permissions.screen", "permissions.focus", "permissions.profiles", "night.on", "helper.on"] {
            assert_eq!(g(k), json!(false), "{k}");
        }
        for k in ["agents_may_edit", "hide_in_game", "usage_limit.on", "sessions.on", "away.on", "unanswered.on"] {
            assert_eq!(g(k), json!(true), "{k}");
        }
        assert_eq!(g("long_task.on"), json!(true));
        assert_eq!(
            (g("signals.done.color"), g("signals.done.effect"), g("signals.done.duration")),
            (json!("#00ff00"), json!("wave_then_solid"), json!("2m"))
        );
        assert_eq!(
            (g("signals.attention.effect"), g("signals.attention.until_answered"), g("signals.attention.duration")),
            (json!("flash_then_pulse"), json!(true), json!("30m"))
        );
        assert_eq!(
            (g("signals.error.effect"), g("signals.error.duration")),
            (json!("blink_twice_then_solid"), json!("10m"))
        );
        assert_eq!(g("signals.busy.effect"), json!("off"));
        assert_eq!((g("signals.info.effect"), g("signals.info.duration")), (json!("wave"), json!("10s")));
        assert_eq!(
            (g("usage_limit.color"), g("usage_limit.effect"), g("usage_limit.speed")),
            (json!("#8000ff"), json!("pulse"), json!(1))
        );
        assert_eq!(g("away.after"), json!("5m"));
        assert_eq!((g("unanswered.every"), g("unanswered.up_to")), (json!("5m"), json!("30m")));
        assert_eq!((g("long_task.after"), g("long_task.show")), (json!("10m"), json!("5m")));
        assert_eq!((g("night.from"), g("night.to"), g("night.mode")), (json!("23:00"), json!("08:00"), json!("dim")));
        assert_eq!(g("progress.keys"), json!("f-row"));
        assert_eq!(g("screen.per_hour"), json!(4));
    }

    #[test]
    fn values_are_checked() {
        let mut s = AgentSettings::default();
        set(&mut s, "signals.done.color", &json!("Amber")).unwrap();
        assert_eq!(s.signals.done.color, [0xFF, 0xBF, 0x00]);
        set(&mut s, "signals.error.effect", &json!("FLASH")).unwrap();
        assert_eq!(s.signals.error.effect, AgentEffect::Flash);
        set(&mut s, "signals.info.duration", &json!("90s")).unwrap();
        assert_eq!(s.signals.info.duration_s, 90);
        set(&mut s, "night.from", &json!("7:30")).unwrap();
        assert_eq!(s.night.from_min, 7 * 60 + 30);
        set(&mut s, "night.on", &json!("on")).unwrap();
        assert!(s.night.on);
        set(&mut s, "progress.keys", &json!("numbers")).unwrap();
        assert_eq!(s.progress.keys, AgentProgressKeys::Numbers);

        let code = |key: &str, v: Value| set(&mut AgentSettings::default(), key, &v).unwrap_err().code;
        assert_eq!(code("signals.done.color", json!("teal")), ErrorCode::BadColor);
        assert_eq!(code("signals.done.color", json!(5)), ErrorCode::BadValue);
        assert_eq!(code("signals.done.effect", json!("strobe")), ErrorCode::BadEffect);
        assert_eq!(code("signals.done.speed", json!(6)), ErrorCode::BadValue);
        assert_eq!(code("signals.done.speed", json!(2.5)), ErrorCode::BadValue);
        assert_eq!(code("signals.done.duration", json!("3h")), ErrorCode::BadDuration);
        assert_eq!(code("signals.done.duration", json!("1500ms")), ErrorCode::BadDuration);
        assert_eq!(code("signals.done.duration", json!(120)), ErrorCode::BadValue);
        assert_eq!(code("away.after", json!("30s")), ErrorCode::BadDuration);
        assert_eq!(code("unanswered.every", json!("31m")), ErrorCode::BadDuration);
        assert_eq!(code("night.to", json!("24:00")), ErrorCode::BadValue);
        assert_eq!(code("night.to", json!("8")), ErrorCode::BadValue);
        assert_eq!(code("night.mode", json!("dark")), ErrorCode::BadValue);
        assert_eq!(code("night.on", json!("yes")), ErrorCode::BadValue);
        assert_eq!(code("signals.glow.color", json!("red")), ErrorCode::BadValue);
        assert_eq!(code("signals.done.glow", json!("red")), ErrorCode::BadValue);
    }

    #[test]
    fn access_permissions_and_limits_are_the_users() {
        for key in USER_ONLY {
            assert!(!agent_may_set(key), "{key}");
            assert!(keys().contains(&key.to_string()), "{key}");
        }
        for key in ["signals.done.color", "night.from", "progress.keys", "away.after", "usage_limit.on"] {
            assert!(agent_may_set(key), "{key}");
        }
    }
}
