//! Error codes and the process exit codes they map to.
//!
//! Every error has a stable string code (`bad_color`, `agent_access_off`,
//! …) that scripts and agents can match on, and an exit code close to BSD
//! `sysexits`. Exit code 2 is never used: a Claude Code or Codex `Stop`
//! hook that exits with 2 makes the agent keep working, and in Codex it
//! also refuses a permission or blocks a prompt.

use std::fmt;
use std::time::Duration;

/// Exit codes of `keymotion-cli`. There is no 2 (see the module comment).
pub mod exit {
    /// Success; hook mode always exits with this.
    pub const OK: u8 = 0;
    /// Something unexpected.
    pub const INTERNAL: u8 = 1;
    /// Unknown command or option (`EX_USAGE`).
    pub const USAGE: u8 = 64;
    /// A value is wrong: color, duration, key, text (`EX_DATAERR`).
    pub const DATA: u8 = 65;
    /// Not available: the app is not running, no keyboard, needs the USB
    /// cable, not supported (`EX_UNAVAILABLE`).
    pub const UNAVAILABLE: u8 = 69;
    /// Try again later: rate limit, screen budget, a flash write in
    /// progress, timeout (`EX_TEMPFAIL`).
    pub const TEMPORARY: u8 = 75;
    /// The CLI and the app do not speak the same protocol (`EX_PROTOCOL`).
    pub const PROTOCOL: u8 = 76;
    /// Not allowed: agent access or a permission is off, `--write-flash`
    /// is missing, or the write guard refused (`EX_NOPERM`).
    pub const PERMISSION: u8 = 77;

    /// Every exit code the CLI may use.
    pub const ALL: [u8; 8] = [OK, INTERNAL, USAGE, DATA, UNAVAILABLE, TEMPORARY, PROTOCOL, PERMISSION];
}

/// Declares [`ErrorCode`] from one list, so the string codes, the exit
/// codes and [`ErrorCode::ALL`] can never disagree.
macro_rules! error_codes {
    ($( $(#[doc = $doc:literal])* $variant:ident = $code:literal => $exit:ident; )*) => {
        /// Why a request failed. The string form ([`ErrorCode::as_str`]) is
        /// part of the protocol and of the CLI's JSON output: never rename one.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum ErrorCode {
            $( $(#[doc = $doc])* $variant, )*
        }

        impl ErrorCode {
            /// Every code, in declaration order.
            pub const ALL: &'static [ErrorCode] = &[ $( ErrorCode::$variant, )* ];

            /// The stable string code.
            pub const fn as_str(self) -> &'static str {
                match self { $( ErrorCode::$variant => $code, )* }
            }

            /// The process exit code (never 2).
            pub const fn exit_code(self) -> u8 {
                match self { $( ErrorCode::$variant => exit::$exit, )* }
            }

            /// The code for a string code; `None` for one this version does
            /// not know (a newer app may send new codes).
            pub fn from_code(code: &str) -> Option<ErrorCode> {
                match code { $( $code => Some(ErrorCode::$variant), )* _ => None }
            }
        }
    };
}

error_codes! {
    /// Something unexpected; the message says what.
    Internal = "internal" => INTERNAL;
    /// A command or option the CLI does not know, or options that do not
    /// go together.
    BadArgument = "bad_argument" => USAGE;
    /// Not a color.
    BadColor = "bad_color" => DATA;
    /// Not a duration, or out of range.
    BadDuration = "bad_duration" => DATA;
    /// Not an effect, or an option the effect does not take.
    BadEffect = "bad_effect" => DATA;
    /// The key list cannot be read.
    BadKeys = "bad_keys" => DATA;
    /// A key the keyboard does not have.
    UnknownKey = "unknown_key" => DATA;
    /// Not a signal name.
    BadSignal = "bad_signal" => DATA;
    /// Not a valid source name.
    BadSource = "bad_source" => DATA;
    /// Not a valid signal id.
    BadId = "bad_id" => DATA;
    /// A number or name out of range.
    BadValue = "bad_value" => DATA;
    /// Text with characters the screen cannot show.
    BadText = "bad_text" => DATA;
    /// Text longer than allowed.
    TextTooLong = "text_too_long" => DATA;
    /// A message longer than the protocol allows.
    TooLarge = "too_large" => DATA;
    /// Keymotion is not running and was not started.
    AppNotRunning = "app_not_running" => UNAVAILABLE;
    /// No keyboard is connected.
    NoKeyboard = "no_keyboard" => UNAVAILABLE;
    /// Lights and the screen work only over the USB cable.
    NeedsUsb = "needs_usb" => UNAVAILABLE;
    /// The keyboard or this copy of the app cannot do it.
    Unsupported = "unsupported" => UNAVAILABLE;
    /// Too many requests; `retry_after_ms` says when to try again.
    RateLimited = "rate_limited" => TEMPORARY;
    /// The screen's write budget is used up for now.
    BudgetExhausted = "budget_exhausted" => TEMPORARY;
    /// The keyboard's memory is being written; try again later.
    BusyFlashWrite = "busy_flash_write" => TEMPORARY;
    /// No answer in time.
    Timeout = "timeout" => TEMPORARY;
    /// The CLI and the app have no protocol version in common.
    ProtocolMismatch = "protocol_mismatch" => PROTOCOL;
    /// The app does not know this command (it is older than the CLI).
    UnknownCommand = "unknown_command" => PROTOCOL;
    /// The message breaks the protocol (not JSON, a missing field, no
    /// `hello` first).
    BadRequest = "bad_request" => PROTOCOL;
    /// The app's answer breaks the protocol.
    BadResponse = "bad_response" => PROTOCOL;
    /// The user has not turned on agent access in Keymotion.
    AgentAccessOff = "agent_access_off" => PERMISSION;
    /// The user has not turned on the permission this command needs.
    PermissionOff = "permission_off" => PERMISSION;
    /// The command writes the keyboard's memory and `--write-flash` was not
    /// given.
    WriteFlashFlagMissing = "write_flash_flag_missing" => PERMISSION;
    /// The write guard refused (several keyboards, a conflicting program,
    /// untested firmware).
    WriteBlocked = "write_blocked" => PERMISSION;
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A failed request: what the app answers and what the CLI prints.
/// Messages and hints are short English sentences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentError {
    pub code: ErrorCode,
    pub message: String,
    /// What to do about it, when there is something to do.
    pub hint: Option<String>,
    /// For temporary errors: when trying again makes sense.
    pub retry_after_ms: Option<u64>,
}

impl AgentError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), hint: None, retry_after_ms: None }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_retry_after(mut self, after: Duration) -> Self {
        self.retry_after_ms = Some(u64::try_from(after.as_millis()).unwrap_or(u64::MAX));
        self
    }

    /// The process exit code for this error (never 2).
    pub fn exit_code(&self) -> u8 {
        self.code.exit_code()
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AgentError {}

/// The exit code for a finished command.
pub fn exit_code<T>(result: &Result<T, AgentError>) -> u8 {
    match result {
        Ok(_) => exit::OK,
        Err(e) => e.exit_code(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_error_ever_exits_with_two() {
        assert!(!exit::ALL.contains(&2));
        for code in ErrorCode::ALL {
            let exit = code.exit_code();
            assert_ne!(exit, 2, "{code}");
            assert_ne!(exit, exit::OK, "{code}: an error must not look like success");
            assert!(exit::ALL.contains(&exit), "{code} exits with {exit}, not a documented code");
        }
        assert_eq!(exit_code::<()>(&Ok(())), 0);
        assert_eq!(exit_code::<()>(&Err(AgentError::new(ErrorCode::NeedsUsb, "x"))), 69);
    }

    #[test]
    fn codes_are_stable_snake_case_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for code in ErrorCode::ALL {
            let s = code.as_str();
            assert!(seen.insert(s), "duplicate code {s}");
            assert!(s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'), "{s}");
            assert_eq!(ErrorCode::from_code(s), Some(*code));
        }
        assert_eq!(ErrorCode::from_code("made_up"), None);
    }

    #[test]
    fn the_documented_codes_have_the_documented_exits() {
        use ErrorCode::*;
        let table = [
            (Internal, 1),
            (BadArgument, 64),
            (BadColor, 65),
            (BadDuration, 65),
            (UnknownKey, 65),
            (TextTooLong, 65),
            (AppNotRunning, 69),
            (NoKeyboard, 69),
            (NeedsUsb, 69),
            (Unsupported, 69),
            (RateLimited, 75),
            (BudgetExhausted, 75),
            (BusyFlashWrite, 75),
            (Timeout, 75),
            (ProtocolMismatch, 76),
            (AgentAccessOff, 77),
            (PermissionOff, 77),
            (WriteFlashFlagMissing, 77),
            (WriteBlocked, 77),
        ];
        for (code, exit) in table {
            assert_eq!(code.exit_code(), exit, "{code}");
        }
    }

    #[test]
    fn errors_carry_hints_and_retry_times() {
        let e = AgentError::new(ErrorCode::RateLimited, "Too many requests.")
            .with_hint("Wait a moment.")
            .with_retry_after(Duration::from_millis(1500));
        assert_eq!(e.retry_after_ms, Some(1500));
        assert_eq!(e.hint.as_deref(), Some("Wait a moment."));
        assert_eq!(e.to_string(), "Too many requests.");
        assert_eq!(e.exit_code(), 75);
    }
}
