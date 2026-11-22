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
//!   `version`, `guide`, `doctor`, `start`). Adding fields keeps `schema`;
//!   removing one or changing its meaning raises it.
//! * stderr: errors in human mode, warnings (unless `--quiet`) and the
//!   `--log` diagnostics. Never a question: nothing reads stdin.

use std::io::Write;

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
            write_stderr(&format!("keymotion-cli: {word}: {}\n", message()));
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

/// The JSON line for a success.
pub fn success_json(command: &str, data: &Value, warnings: &[String]) -> String {
    // Written by hand so the keys keep this order in every build.
    format!(
        r#"{{"ok":true,"schema":{SCHEMA},"command":{},"data":{},"warnings":{}}}"#,
        json!(command),
        data,
        json!(warnings)
    )
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
    format!(r#"{{"ok":false,"schema":{SCHEMA},"command":{},"error":{error}}}"#, json!(command))
}

/// An error for a person, on stderr.
pub fn human_error(e: &AgentError) -> String {
    let mut out = format!("error: {}\n", e.message);
    if let Some(hint) = &e.hint {
        out.push_str(&format!("hint: {hint}\n"));
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
                    write_stdout(&format!("{text}\n"));
                }
                for w in &done.warnings {
                    write_stderr(&format!("warning: {w}\n"));
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
        // clap's own text: the error, the usage line and a tip.
        let _ = clap.print();
    }
    error.exit_code()
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
