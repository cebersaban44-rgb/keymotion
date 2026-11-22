//! Human output: a few short English lines per command, from the same data
//! `--json` prints. Agents use `--json`; this is for a person at a
//! terminal, so it says what happened and leaves out the rest.

use serde_json::Value;

/// `850ms`, `45s`, `1.5s`, `2m`, `1m 58s`, `1h 5m`.
pub fn duration(ms: u64) -> String {
    if ms < 1000 {
        return format!("{ms}ms");
    }
    if ms < 10_000 {
        let tenths = (ms + 50) / 100;
        return if tenths % 10 == 0 {
            format!("{}s", tenths / 10)
        } else {
            format!("{}.{}s", tenths / 10, tenths % 10)
        };
    }
    let s = (ms + 500) / 1000;
    if s < 60 {
        return format!("{s}s");
    }
    if s < 3600 {
        return if s % 60 == 0 { format!("{}m", s / 60) } else { format!("{}m {}s", s / 60, s % 60) };
    }
    let m = (s + 30) / 60;
    if m % 60 == 0 {
        format!("{}h", m / 60)
    } else {
        format!("{}h {}m", m / 60, m % 60)
    }
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn yes_no(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The app's `"lights"` word in a few words.
fn lights_word(word: &str) -> &str {
    match word {
        "shown" => "agents' lights show",
        "no_keyboard" => "no keyboard is connected",
        "needs_usb" => "they need the USB cable",
        "game" => "hidden while a game wants the keyboard left alone",
        "night_dim" => "dimmed for the night",
        "night_off" => "off for the night",
        other => other,
    }
}

/// The app's version line: `Keymotion 1.2.0 (store), protocol 1`.
fn app_line(app: &Value) -> String {
    let mut line = format!("Keymotion {}", text(app, "version"));
    if let Some(channel) = app.get("channel").and_then(Value::as_str) {
        line.push_str(&format!(" ({channel})"));
    }
    if let Some(p) = app.get("protocol").and_then(Value::as_u64) {
        line.push_str(&format!(", protocol {p}"));
    }
    line
}

pub fn status(data: &Value) -> String {
    let app = &data["app"];
    if !flag(app, "running") {
        return "Keymotion is not running.".to_string();
    }
    if app.get("answering") == Some(&Value::Bool(false)) {
        return "Keymotion is running but does not answer agents yet.".to_string();
    }
    let mut out = vec![app_line(app)];
    let agents = &data["agents"];
    out.push(format!("Agent access: {}", yes_no(flag(agents, "enabled"))));
    let keyboards = data["keyboards"].as_array().map(Vec::as_slice).unwrap_or_default();
    if keyboards.is_empty() {
        out.push("Keyboard: none connected".to_string());
    }
    for k in keyboards {
        let mut line = format!("Keyboard: {}, {}", text(k, "name"), text(k, "connection"));
        if let Some(p) = k["battery"].get("percent").and_then(Value::as_u64) {
            line.push_str(&format!(", battery {p}%"));
        }
        if !flag(k, "lights") {
            line.push_str(" (no lights over this connection)");
        }
        out.push(line);
    }
    out.push(format!("Lights: {}", lights_word(text(data, "lights"))));
    let signals = data["signals"].as_array().map(Vec::as_slice).unwrap_or_default();
    if signals.is_empty() {
        out.push("No agent lights on.".to_string());
    } else {
        out.push(format!("{} on:", plural(signals.len() as u64, "agent light", "agent lights")));
        for s in signals {
            let left = s["expires_in_ms"].as_u64().map(duration).unwrap_or_default();
            let mut line = format!("  {}  {}  {}  {left} left", text(s, "id"), text(s, "kind"), text(s, "keys"));
            if let Some(p) = s["percent"].as_f64() {
                line.push_str(&format!("  {p}%"));
            }
            if !flag(s, "shown") {
                line.push_str("  (covered)");
            }
            out.push(line);
        }
    }
    if let Some(conflict) = data["conflict"].as_str() {
        out.push(format!("Also talking to the keyboard: {conflict}"));
    }
    out.join("\n")
}

pub fn keys(data: &Value) -> String {
    let keys = data["keys"].as_array().map(Vec::as_slice).unwrap_or_default();
    let mut out =
        vec![format!("{} ({}): {}", text(data, "name"), text(data, "model"), plural(keys.len() as u64, "key", "keys"))];
    // Rows by position, top to bottom, left to right.
    let mut sorted: Vec<(f64, f64, &str)> =
        keys.iter().map(|k| (k["y"].as_f64().unwrap_or(0.0), k["x"].as_f64().unwrap_or(0.0), text(k, "id"))).collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut row: Vec<&str> = Vec::new();
    let mut row_y = None;
    for (y, _, id) in sorted {
        if row_y.is_some_and(|ry: f64| (y - ry).abs() >= 0.5) {
            out.push(format!("  {}", row.join(" ")));
            row.clear();
        }
        if row.is_empty() {
            row_y = Some(y);
        }
        row.push(id);
    }
    if !row.is_empty() {
        out.push(format!("  {}", row.join(" ")));
    }
    let groups: Vec<&str> =
        data["groups"].as_array().map(Vec::as_slice).unwrap_or_default().iter().map(|g| text(g, "name")).collect();
    out.push(format!("Groups: {}", groups.join(", ")));
    out.push("Use ids, ranges (f1-f12) and groups, comma-separated, with --keys.".to_string());
    out.join("\n")
}

/// What `signal`, `light` and `progress` answered: one line.
pub fn lit(command: &str, data: &Value) -> String {
    let id = text(data, "id");
    if data.get("lit") == Some(&Value::Bool(false)) {
        let cleared = data["cleared"].as_u64().unwrap_or(0);
        return format!(
            "Signal {id}: no light (its look is off in Keymotion); {} cleared.",
            plural(cleared, "earlier light", "earlier lights")
        );
    }
    let what = match command {
        "progress" => match data["percent"].as_f64() {
            Some(p) => format!("Progress {id} at {p}%"),
            None => format!("Progress {id}"),
        },
        "signal" => format!("Signal {id}"),
        _ => format!("Light {id}"),
    };
    let mut line = match data["expires_in_ms"].as_u64() {
        Some(ms) => format!("{what} on for {}.", duration(ms)),
        None => format!("{what} on."),
    };
    // A bar is meant to be sent again; a light that replaced one is worth
    // saying.
    if flag(data, "replaced") && command != "progress" {
        line.push_str(" It replaced the earlier one.");
    }
    if let Some(n) = data["cleared"].as_u64().filter(|n| *n > 0) {
        line.push_str(&format!(" {} cleared.", plural(n, "earlier light", "earlier lights")));
    }
    line
}

/// Warnings about a light just set: hidden, covered, or pushing out an
/// older one.
pub fn lit_warnings(data: &Value) -> Vec<String> {
    let mut out = Vec::new();
    match data["hidden"].as_str() {
        Some("game") => out.push(
            "Not visible now: a game wants the keyboard left alone. It shows when the game ends, if still on."
                .to_string(),
        ),
        Some("night") => out.push("Not visible now: the user turned agents' lights off for the night.".to_string()),
        Some(other) => out.push(format!("Not visible now ({other}).")),
        None if data.get("lit") != Some(&Value::Bool(false)) && data.get("shown") == Some(&Value::Bool(false)) => {
            out.push("Not visible now: another light covers these keys; it shows when that one ends.".to_string())
        }
        None => {}
    }
    let evicted: Vec<&str> =
        data["evicted"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(Value::as_str).collect();
    if !evicted.is_empty() {
        out.push(format!("A source keeps at most 8 lights: {} went to make room.", evicted.join(", ")));
    }
    out
}

pub fn clear(data: &Value) -> String {
    match data["cleared"].as_u64().unwrap_or(0) {
        0 => "Nothing to clear.".to_string(),
        n => format!("Cleared {}.", plural(n, "light", "lights")),
    }
}

/// The log, newest first; `now_ms` is the wall clock in Unix milliseconds.
pub fn log(data: &Value, now_ms: f64) -> String {
    let entries = data["entries"].as_array().map(Vec::as_slice).unwrap_or_default();
    if entries.is_empty() {
        return "No agent requests yet.".to_string();
    }
    entries
        .iter()
        .map(|e| {
            let ago = e["at_ms"].as_f64().map(|at| (now_ms - at).max(0.0) as u64).map(duration).unwrap_or_default();
            let mut line = format!("{ago} ago  {}  {}  {}", text(e, "source"), text(e, "command"), text(e, "result"));
            if let Some(id) = e["id"].as_str() {
                line.push_str(&format!("  {id}"));
            }
            for c in e["changes"].as_array().map(Vec::as_slice).unwrap_or_default() {
                line.push_str(&format!("  {}: {} -> {}", text(c, "key"), value(&c["before"]), value(&c["after"])));
            }
            if flag(e, "undone") {
                line.push_str("  (undone)");
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A setting's value without JSON quotes.
fn value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn settings_get(data: &Value) -> String {
    let user_only: Vec<&str> =
        data["user_only"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(Value::as_str).collect();
    let mut out: Vec<String> = data["settings"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    let only = if user_only.contains(&k.as_str()) { "  (only the user changes it)" } else { "" };
                    format!("{k} = {}{only}", value(v))
                })
                .collect()
        })
        .unwrap_or_default();
    if !flag(data, "agents_may_edit") {
        out.push("Agents may not change these settings now (the user's choice).".to_string());
    }
    out.join("\n")
}

pub fn settings_set(data: &Value) -> String {
    let changes = data["changed"].as_array().map(Vec::as_slice).unwrap_or_default();
    if changes.is_empty() {
        return "Nothing changed: the setting already had that value.".to_string();
    }
    let mut out: Vec<String> = changes
        .iter()
        .map(|c| format!("{}: {} -> {}", text(c, "key"), value(&c["before"]), value(&c["after"])))
        .collect();
    out.push("The user sees this in Keymotion's AI agents page and can undo it there.".to_string());
    out.join("\n")
}

pub fn version(data: &Value) -> String {
    let cli = &data["cli"];
    let mut out = vec![format!(
        "keymotion-cli {} (protocol {})",
        text(cli, "version"),
        cli["protocol"].as_u64().unwrap_or_default()
    )];
    let app = &data["app"];
    out.push(if !flag(app, "running") {
        "Keymotion is not running.".to_string()
    } else if app.get("compatible") == Some(&Value::Bool(false)) {
        format!("Keymotion runs but speaks another protocol: {}", text(&app["error"], "message"))
    } else if app.get("answering") == Some(&Value::Bool(false)) {
        "Keymotion is running but does not answer agents yet.".to_string()
    } else {
        app_line(app)
    });
    out.join("\n")
}

pub fn start(data: &Value) -> String {
    let app = app_line(&data["app"]);
    if flag(data, "already_running") {
        return format!("{app} is running.");
    }
    match data["waited_ms"].as_u64() {
        Some(ms) => format!("Started {app} in the background ({}).", duration(ms)),
        None => format!("Started {app} in the background."),
    }
}

pub fn doctor(data: &Value) -> String {
    let checks = data["checks"].as_array().map(Vec::as_slice).unwrap_or_default();
    let mut out: Vec<String> =
        checks.iter().map(|c| format!("{:<5} {:<9} {}", text(c, "status"), text(c, "id"), text(c, "detail"))).collect();
    out.push(match data["problems"].as_u64().unwrap_or(0) {
        0 => "No problems found.".to_string(),
        n => format!("{} to look at.", plural(n, "problem", "problems")),
    });
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn durations_read_short() {
        for (ms, text) in [
            (0, "0ms"),
            (850, "850ms"),
            (1_000, "1s"),
            (1_500, "1.5s"),
            (45_000, "45s"),
            (59_600, "1m"),
            (120_000, "2m"),
            (118_000, "1m 58s"),
            (3_600_000, "1h"),
            (3_900_000, "1h 5m"),
            (7_200_000, "2h"),
        ] {
            assert_eq!(duration(ms), text, "{ms}");
        }
    }

    fn running_status() -> Value {
        json!({
            "app": {"running": true, "version": "1.2.0", "channel": "store", "protocol": 1},
            "agents": {"enabled": true, "screen": false, "focus": false, "profiles": false, "agents_may_edit": true, "hide_in_game": true},
            "keyboards": [{"model": "ajazz-ak980-pro", "name": "AJAZZ AK980 PRO", "connection": "usb", "battery": null, "lights": true, "screen": true}],
            "lights": "shown",
            "quiet": "off",
            "focus": null,
            "signals": [
                {"id": "cli:done", "source": "cli", "kind": "done", "keys": "all", "shown": true, "expires_in_ms": 118000, "percent": null},
                {"id": "cli:progress", "source": "cli", "kind": "progress", "keys": "f-row", "shown": false, "expires_in_ms": 600000, "percent": 40.0}
            ],
            "screen_budget": null,
            "flash_write": null,
            "conflict": null
        })
    }

    #[test]
    fn status_says_what_matters() {
        assert_eq!(
            status(&running_status()),
            "Keymotion 1.2.0 (store), protocol 1\nAgent access: on\nKeyboard: AJAZZ AK980 PRO, usb\nLights: agents' lights show\n2 agent lights on:\n  cli:done  done  all  1m 58s left\n  cli:progress  progress  f-row  10m left  40%  (covered)"
        );
        assert_eq!(status(&json!({"app": {"running": false}})), "Keymotion is not running.");
        assert!(status(&json!({"app": {"running": true, "answering": false}})).contains("does not answer"));
        let mut off = running_status();
        off["agents"]["enabled"] = json!(false);
        off["signals"] = json!([]);
        off["keyboards"] = json!([]);
        let text = status(&off);
        assert!(
            text.contains("Agent access: off") && text.contains("none connected") && text.contains("No agent lights")
        );
    }

    #[test]
    fn keys_come_in_rows() {
        let data = json!({
            "model": "m", "name": "Board",
            "keys": [
                {"id": "f1", "x": 2.0, "y": 0.0}, {"id": "esc", "x": 0.0, "y": 0.0},
                {"id": "1", "x": 1.0, "y": 1.25}, {"id": "2", "x": 2.0, "y": 1.3}
            ],
            "groups": [{"name": "all", "keys": []}, {"name": "f-row", "keys": ["f1"]}]
        });
        assert_eq!(keys(&data), "Board (m): 4 keys\n  esc f1\n  1 2\nGroups: all, f-row\nUse ids, ranges (f1-f12) and groups, comma-separated, with --keys.");
    }

    #[test]
    fn lights_say_how_long_and_why_not_visible() {
        let data = json!({"id": "cli:light", "shown": true, "lit": true, "expires_in_ms": 60000, "replaced": false, "evicted": [], "coalesced": false, "hidden": null});
        assert_eq!(lit("light", &data), "Light cli:light on for 1m.");
        assert!(lit_warnings(&data).is_empty());
        let data = json!({"id": "cli:done", "shown": false, "lit": true, "expires_in_ms": 120000, "replaced": true, "evicted": ["a"], "hidden": "game"});
        assert_eq!(lit("signal", &data), "Signal cli:done on for 2m. It replaced the earlier one.");
        let w = lit_warnings(&data);
        assert_eq!(w.len(), 2);
        assert!(w[0].contains("game") && w[1].contains("a went"), "{w:?}");
        let covered = json!({"id": "x", "shown": false, "lit": true, "expires_in_ms": 1000, "hidden": null});
        assert!(lit_warnings(&covered)[0].contains("covers"));
        let busy = json!({"id": "cli:busy", "shown": false, "lit": false, "cleared": 2});
        assert_eq!(
            lit("signal", &busy),
            "Signal cli:busy: no light (its look is off in Keymotion); 2 earlier lights cleared."
        );
        assert!(lit_warnings(&busy).is_empty());
        let bar = json!({"id": "cli:progress", "shown": true, "lit": true, "expires_in_ms": 600000, "percent": 42.5, "replaced": true});
        assert_eq!(lit("progress", &bar), "Progress cli:progress at 42.5% on for 10m.");
    }

    #[test]
    fn the_rest_read_in_one_line_or_a_few() {
        assert_eq!(clear(&json!({"cleared": 0})), "Nothing to clear.");
        assert_eq!(clear(&json!({"cleared": 1})), "Cleared 1 light.");
        let entries = json!({"entries": [
            {"seq": 2, "at_ms": 9000.0, "source": "cli", "command": "settings_set", "result": "ok", "changes": [{"key": "night.on", "before": false, "after": true}], "undone": true},
            {"seq": 1, "at_ms": 5000.0, "source": "cli", "command": "signal", "result": "agent_access_off", "id": "cli:done"}
        ]});
        assert_eq!(
            log(&entries, 10_000.0),
            "1s ago  cli  settings_set  ok  night.on: false -> true  (undone)\n5s ago  cli  signal  agent_access_off  cli:done"
        );
        assert_eq!(log(&json!({"entries": []}), 0.0), "No agent requests yet.");
        let settings = json!({"settings": {"enabled": true, "signals.done.color": "#00ff00"}, "agents_may_edit": false, "user_only": ["enabled"]});
        assert_eq!(
            settings_get(&settings),
            "enabled = true  (only the user changes it)\nsignals.done.color = #00ff00\nAgents may not change these settings now (the user's choice)."
        );
        let set = json!({"changed": [{"key": "signals.done.color", "before": "#00ff00", "after": "#0000ff"}], "log_entry": 3});
        assert!(settings_set(&set).starts_with("signals.done.color: #00ff00 -> #0000ff\n"));
        assert!(settings_set(&json!({"changed": []})).starts_with("Nothing changed"));
        let v = json!({"cli": {"version": "1.2.0", "protocol": 1}, "app": {"running": false}});
        assert_eq!(version(&v), "keymotion-cli 1.2.0 (protocol 1)\nKeymotion is not running.");
        let v = json!({"cli": {"version": "1.2.0", "protocol": 1}, "app": {"running": true, "version": "1.2.0", "channel": "dev", "protocol": 1}});
        assert_eq!(version(&v), "keymotion-cli 1.2.0 (protocol 1)\nKeymotion 1.2.0 (dev), protocol 1");
        let s = json!({"already_running": false, "waited_ms": 2100, "app": {"version": "1.2.0", "channel": "github", "protocol": 1}});
        assert_eq!(start(&s), "Started Keymotion 1.2.0 (github), protocol 1 in the background (2.1s).");
        let d = json!({"checks": [{"id": "app", "status": "ok", "detail": "answers"}], "problems": 0});
        assert_eq!(doctor(&d), "ok    app       answers\nNo problems found.");
    }
}
