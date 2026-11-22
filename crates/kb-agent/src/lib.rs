//! The agent protocol: how `keymotion-cli` (and through it scripts and AI
//! coding agents) talks to the running Keymotion app.
//!
//! * The CLI never opens the keyboard. It sends short JSON messages over a
//!   named pipe that only the current Windows user can open, in the current
//!   logon session ([`pipe_name`]). The app's device actor stays the
//!   keyboard's only owner, with its light layers, budgets and guards.
//! * [`protocol`]: protocol v1, newline-delimited JSON, at most
//!   [`MAX_LINE_BYTES`] per line, `hello` first; typed arguments for each
//!   command; the server's per-connection [`Session`].
//! * [`values`] and [`keys`]: colors, durations, effects, key lists, signal
//!   and source names, text limits. The CLI checks them before sending; the
//!   app checks them again and never trusts the client. Blinking stays at
//!   or below three flashes a second (WCAG 2.3.1).
//! * [`error`]: stable string codes and exit codes. Exit code 2 is never
//!   used: Claude Code and Codex read 2 from a hook as "keep going" or
//!   "block".
//! * `transport` (Windows): the app's server and the CLI's client on top of
//!   `kb_winsys::pipe`, which holds the `unsafe` Win32 calls.
//!
//! No `unsafe` here, and no dependency on `kb-transport`, hidapi, Tauri or
//! tokio (`tests/dependencies.rs` checks it).

pub mod error;
pub mod keys;
pub mod protocol;
#[cfg(windows)]
pub mod transport;
pub mod values;

pub use error::{exit, AgentError, ErrorCode};
pub use keys::{KeyGroup, KeyItem, KeySpec};
pub use protocol::{
    Call, Channel, Command, HelloArgs, HelloData, Peer, ProtocolRange, Request, Response, ServerInfo, Session,
    MAX_LINE_BYTES, PROTOCOL_VERSION, SUPPORTED_PROTOCOLS,
};
pub use values::{Color, Direction, Effect, EffectKind, SignalId, SignalName, SourceName};

/// Every pipe name starts with this.
pub const PIPE_PREFIX: &str = r"\\.\pipe\keymotion-agent-";

/// The pipe for a Windows user (`S-1-5-21-…`) in a logon session:
/// `\\.\pipe\keymotion-agent-<SID>-<session>`. Pipe names are shared by the
/// whole computer; the SID and session keep two users, or the same user
/// signed in twice, apart. `None` when `user_sid` does not look like a SID.
pub fn pipe_name(user_sid: &str, session: u32) -> Option<String> {
    let looks_like_sid = user_sid.len() <= 184
        && user_sid.strip_prefix("S-").is_some_and(|rest| {
            !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit() || b == b'-') && !rest.contains("--")
        });
    looks_like_sid.then(|| format!("{PIPE_PREFIX}{user_sid}-{session}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_names_carry_the_user_and_the_session() {
        assert_eq!(
            pipe_name("S-1-5-21-1004336348-1177238915-682003330-1001", 1).as_deref(),
            Some(r"\\.\pipe\keymotion-agent-S-1-5-21-1004336348-1177238915-682003330-1001-1")
        );
        assert_ne!(pipe_name("S-1-5-21-1-2-3-1001", 1), pipe_name("S-1-5-21-1-2-3-1001", 2));
        assert_ne!(pipe_name("S-1-5-21-1-2-3-1001", 1), pipe_name("S-1-5-21-1-2-3-1002", 1));
        for bad in ["", "S-", "1-5-21", "S-1-5-21\\..\\x", "S-1-5 21", "S-1--5", r"S-1\pipe", "s-1-5-21"] {
            assert_eq!(pipe_name(bad, 1), None, "{bad:?}");
        }
    }
}
