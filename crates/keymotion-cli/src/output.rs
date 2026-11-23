//! What keymotion-cli prints.
//!
//! * stdout: the result. Human output is short English; with `--json`
//!   exactly one JSON object, for success and failure alike:
//!
//! ```text
//! {"ok":true,"schema":1,"command":"signal","data":{…},"warnings":[]}
//! {"ok":false,"schema":1,"command":"signal","error":{"code":"agent_access_off","message":"…","hint":"…","retry_after_ms":null}}
//! ```
//!
//!   `data` is what the app answered (or what the CLI found itself, for
//!   `version`, `guide`, `doctor`, `start`, `help`). Adding fields keeps
//!   `schema`; removing one or changing its meaning raises it.
//! * stderr: errors in human mode, warnings (unless `--quiet`) and the
//!   `--log` diagnostics. Never a question: nothing reads stdin.
//! * Nothing printed can drive a terminal: the app's answers and echoed
//!   input go through [`terminal`] or `printable` in human output, and
//!   [`json_safe`] escapes what serde_json leaves raw.

use std::io::Write;

use kb_agent::values::{is_unsafe_char, printable};
use kb_agent::AgentError;
use serde_json::{json, Value};

use crate::cli::LogLevel;

/// The JSON output's `schema`.
pub const SCHEMA: u32 = 1;

/// Diagnostics on stderr, at `--log`'s level.
#[derive(Debug, Clone, Copy)]
pub struct Logger {
    level: LogLevel,
}

impl Logger {
    /// `--log`, else `error` with `--quiet` and `warn` without.
    pub fn new(level: Option<LogLevel>, quiet: bool) -> Self {
        Self { level: level.unwrap_or(if quiet { LogLevel::Error } else { LogLevel::Warn }) }
    }

    pub fn enabled(&self, level: LogLevel) -> bool {
        level != LogLevel::Off && level <= self.level
    }

    fn log(&self, level: LogLevel, message: impl FnOnce() -> String) {
        if self.enabled(level) {
            let word = match level {
                LogLevel::Off => return,
                LogLevel::Error => "error",
                LogLevel::Warn => "warning",
                LogLevel::Info => "info",
                LogLevel::Debug => "debug",
            };
            write_stderr(&format!("keymotion-cli: {word}: {}\n", printable(&message())));
        }
    }

    pub fn warn(&self, message: impl FnOnce() -> String) {
        self.log(LogLevel::Warn, message);
    }

    pub fn info(&self, message: impl FnOnce() -> String) {
        self.log(LogLevel::Info, message);
    }

    pub fn debug(&self, message: impl FnOnce() -> String) {
        self.log(LogLevel::Debug, message);
    }
}

/// A finished command: its data, its warnings and its human text.
#[derive(Debug, Clone, PartialEq)]
pub struct Done {
    pub data: Value,
    pub warnings: Vec<String>,
    /// For human output; may be empty.
    pub text: String,
}

impl Done {
    pub fn new(data: Value, text: impl Into<String>) -> Self {
        Self { data, warnings: Vec::new(), text: text.into() }
    }

    pub fn warn(mut self, warnings: impl IntoIterator<Item = String>) -> Self {
        self.warnings.extend(warnings);
        self
    }
}

/// Text for a terminal: its lines kept, every other character a terminal
/// could act on (escape sequences and other controls, invisible and
/// direction-changing characters) written out.
pub fn terminal(text: &str) -> String {
    text.split('\n').map(printable).collect::<Vec<_>>().join("\n")
}

/// A JSON line that is safe to print too. serde_json escapes C0 controls
/// but leaves DEL, the C1 controls and invisible or direction-changing
/// characters raw; here they become `\u` escapes as well. They can only
/// stand inside strings, so the JSON means the same.
pub fn json_safe(line: String) -> String {
    if !line.chars().any(is_unsafe_char) {
        return line;
    }
    let mut out = String::with_capacity(line.len() + 12);
    for c in line.chars() {
        if is_unsafe_char(c) {
            // Every one of them is in the Basic Multilingual Plane.
            out.push_str(&format!("\\u{:04x}", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

/// The JSON line for a success.
pub fn success_json(command: &str, data: &Value, warnings: &[String]) -> String {
    // Written by hand so the keys keep this order in every build.
    json_safe(format!(
        r#"{{"ok":true,"schema":{SCHEMA},"command":{},"data":{},"warnings":{}}}"#,
        json!(command),
        data,
        json!(warnings)
    ))
}

/// The JSON line for a failure. `command` is `null` when the command line
/// named none.
pub fn failure_json(command: Option<&str>, e: &AgentError) -> String {
    let error = json!({
        "code": e.code.as_str(),
        "message": e.message,
        "hint": e.hint,
        "retry_after_ms": e.retry_after_ms,
    });
    // The error's keys in this order too.
    let error = format!(
        r#"{{"code":{},"message":{},"hint":{},"retry_after_ms":{}}}"#,
        error["code"], error["message"], error["hint"], error["retry_after_ms"]
    );
    json_safe(format!(r#"{{"ok":false,"schema":{SCHEMA},"command":{},"error":{error}}}"#, json!(command)))
}

/// An error for a person, on stderr. The message and the hint may come
/// from the app: each stays one written-out line.
pub fn human_error(e: &AgentError) -> String {
    let mut out = format!("error: {}\n", printable(&e.message));
    if let Some(hint) = &e.hint {
        out.push_str(&format!("hint: {}\n", printable(hint)));
    }
    if let Some(ms) = e.retry_after_ms {
        out.push_str(&format!("try again in {}\n", crate::human::duration(ms)));
    }
    out
}

/// Print a command's result; its exit code.
pub fn finish(json: bool, quiet: bool, command: &str, result: Result<Done, AgentError>) -> u8 {
    match result {
        Ok(done) => {
            if json {
                write_stdout(&format!("{}\n", success_json(command, &done.data, &done.warnings)));
            } else if !quiet {
                if !done.text.is_empty() {
                    let text = done.text.trim_end_matches('\n');
                    write_stdout(&format!("{}\n", terminal(text)));
                }
                for w in &done.warnings {
                    write_stderr(&format!("warning: {}\n", printable(w)));
                }
            }
            kb_agent::exit::OK
        }
        Err(e) => {
            if json {
                write_stdout(&format!("{}\n", failure_json(Some(command), &e)));
            } else {
                write_stderr(&human_error(&e));
            }
            e.exit_code()
        }
    }
}

/// clap refused the command line: 64, never clap's 2.
pub fn usage_error(json: bool, command: Option<&str>, error: &AgentError, clap: &clap::Error) -> u8 {
    if json {
        write_stdout(&format!("{}\n", failure_json(command, error)));
    } else {
        // clap's own text (the error, the usage line and a tip) echoes
        // what was typed: plain, and written out.
        let text = clap.render().to_string();
        write_stderr(&format!("{}\n", terminal(text.trim_end())));
    }
    error.exit_code()
}

/// `--help` and `--version`: clap's text, or with `--json` that text in
/// the usual object (`command` is `help` or `version`). Exit 0.
pub fn print_help(json: bool, clap: &clap::Error) -> u8 {
    if !json {
        let _ = clap.print();
        return kb_agent::exit::OK;
    }
    write_stdout(&format!("{}\n", help_json(clap)));
    kb_agent::exit::OK
}

fn help_json(clap: &clap::Error) -> String {
    let text = clap.render().to_string();
    if clap.kind() == clap::error::ErrorKind::DisplayVersion {
        let data =
            json!({ "text": text.trim_end(), "version": crate::VERSION, "protocol": kb_agent::PROTOCOL_VERSION });
        success_json("version", &data, &[])
    } else {
        success_json("help", &json!({ "text": text }), &[])
    }
}

/// A closed stdout (`| head`) is not an error worth a panic.
pub fn write_stdout(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes()).and_then(|()| out.flush());
}

pub fn write_stderr(text: &str) {
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(text.as_bytes()).and_then(|()| err.flush());
}

#[cfg(test)]
mod tests {
    use super::*;
    use kb_agent::ErrorCode;
    use std::time::Duration;

    #[test]
    fn success_is_one_json_object_in_the_documented_shape() {
        let line = success_json("signal", &json!({"id": "cli:done", "shown": true}), &["Hidden for now.".into()]);
        assert!(line.starts_with(r#"{"ok":true,"schema":1,"command":"signal","data":"#), "{line}");
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            v,
            json!({"ok": true, "schema": 1, "command": "signal", "data": {"id": "cli:done", "shown": true}, "warnings": ["Hidden for now."]})
        );
        assert!(!line.contains('\n'));
        let empty: Value = serde_json::from_str(&success_json("clear", &json!({}), &[])).unwrap();
        assert_eq!(empty["warnings"], json!([]));
    }

    #[test]
    fn failure_is_one_json_object_in_the_documented_shape() {
        let e = AgentError::new(ErrorCode::RateLimited, "Too many requests.")
            .with_hint("Wait a moment.")
            .with_retry_after(Duration::from_millis(300));
        let line = failure_json(Some("light"), &e);
        assert_eq!(
            line,
            r#"{"ok":false,"schema":1,"command":"light","error":{"code":"rate_limited","message":"Too many requests.","hint":"Wait a moment.","retry_after_ms":300}}"#
        );
        let bare: Value =
            serde_json::from_str(&failure_json(None, &AgentError::new(ErrorCode::BadArgument, "Say \"what\".")))
                .unwrap();
        assert_eq!(
            bare,
            json!({"ok": false, "schema": 1, "command": null, "error": {"code": "bad_argument", "message": "Say \"what\".", "hint": null, "retry_after_ms": null}})
        );
    }

    #[test]
    fn nothing_printed_can_drive_a_terminal() {
        assert_eq!(terminal("line one\n\u{1b}[2Jtwo\u{9b}"), "line one\n\\u{1B}[2Jtwo\\u{9B}");
        let e = AgentError::new(ErrorCode::BadValue, "bad\u{1b}]0;title\u{7}\nnext").with_hint("\u{202e}gnp.exe");
        assert_eq!(human_error(&e), "error: bad\\u{1B}]0;title\\u{7}\\nnext\nhint: \\u{202E}gnp.exe\n");
        // JSON: C0 by serde_json, DEL, C1 and direction overrides here; the
        // value reads back the same.
        let raw = "a\u{1b}b\u{7f}c\u{9b}d\u{202e}e";
        let line = success_json("x", &json!({ "v": raw }), &[]);
        assert!(!line.chars().any(is_unsafe_char), "{line}");
        for escape in [r"\u001b", r"\u007f", r"\u009b", r"\u202e"] {
            assert!(line.contains(escape), "{escape}: {line}");
        }
        let back: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(back["data"]["v"], json!(raw));
        let line = failure_json(Some("light"), &AgentError::new(ErrorCode::BadColor, "'\u{9b}31m' is no color"));
        assert!(!line.chars().any(is_unsafe_char), "{line}");
    }

    #[test]
    fn people_read_the_error_hint_and_retry_time() {
        let e = AgentError::new(ErrorCode::AgentAccessOff, "Agent access is off in Keymotion.")
            .with_hint("The user can turn it on.")
            .with_retry_after(Duration::from_millis(1500));
        assert_eq!(
            human_error(&e),
            "error: Agent access is off in Keymotion.\nhint: The user can turn it on.\ntry again in 1.5s\n"
        );
    }

    #[test]
    fn the_log_level_follows_quiet_unless_given() {
        assert!(Logger::new(None, false).enabled(LogLevel::Warn));
        assert!(!Logger::new(None, false).enabled(LogLevel::Info));
        assert!(!Logger::new(None, true).enabled(LogLevel::Warn));
        assert!(Logger::new(None, true).enabled(LogLevel::Error));
        assert!(Logger::new(Some(LogLevel::Debug), true).enabled(LogLevel::Debug));
        assert!(!Logger::new(Some(LogLevel::Off), false).enabled(LogLevel::Error));
        assert!(!Logger::new(Some(LogLevel::Debug), false).enabled(LogLevel::Off));
    }
}
