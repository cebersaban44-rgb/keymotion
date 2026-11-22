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
    Call, Channel, Command, HelloArgs, HelloData, Hook, HookArgs, HookEvent, Peer, ProtocolRange, Request, Response,
    ServerInfo, Session, HELLO_VERSION, MAX_LINE_BYTES, PROTOCOL_VERSION, SUPPORTED_PROTOCOLS,
};
pub use values::{Color, Direction, Effect, EffectKind, SignalId, SignalName, SourceName};

/// The first Keymotion version that takes `--background` and
/// `keymotion://background`: start without a window, for an agent. An older
/// app opens its window instead, and has no pipe for agents at all.
pub const BACKGROUND_START_SINCE: [u16; 3] = [1, 2, 0];

/// Whether a Keymotion of `version` (`[major, minor, patch]`) can be
/// started without a window ([`BACKGROUND_START_SINCE`]).
pub fn starts_in_background(version: [u16; 3]) -> bool {
    version >= BACKGROUND_START_SINCE
}

/// `1.2.0` or `1.2.0-rc.1` → `[1, 2, 0]`.
pub fn version_triple(version: &str) -> Option<[u16; 3]> {
    let core = version.trim().split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u16>().ok());
    let out = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(out)
}

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
    fn only_new_enough_apps_start_without_a_window() {
        assert_eq!(version_triple("1.2.0"), Some([1, 2, 0]));
        assert_eq!(version_triple("1.10.3-rc.1"), Some([1, 10, 3]));
        assert_eq!(version_triple(env!("CARGO_PKG_VERSION")).map(|v| v.len()), Some(3));
        for bad in ["", "1.2", "1.2.3.4", "a.b.c", "1.2.x"] {
            assert_eq!(version_triple(bad), None, "{bad}");
        }
        assert!(!starts_in_background([1, 1, 0]));
        assert!(!starts_in_background([0, 9, 9]));
        assert!(starts_in_background([1, 2, 0]));
        assert!(starts_in_background([1, 10, 0]));
        assert!(starts_in_background([2, 0, 0]));
    }

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
