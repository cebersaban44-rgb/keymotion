//! `keymotion-cli`: the keyboard's lights for AI coding agents and scripts
//! (plan: `ajan-cli-plan.md`; docs site: developers/agent-cli).
//!
//! * It never opens the keyboard. Every command goes over kb-agent's named
//!   pipe to the running Keymotion app, whose device actor stays the
//!   keyboard's only owner (`tests/dependencies.rs` keeps the HID stack,
//!   Tauri and tokio out of this crate).
//! * Results go to stdout, diagnostics to stderr. It reads stdin only in
//!   hook mode (the agent's event) and asks only in `connect` and
//!   `disconnect` at a terminal. Human output is short English; `--json`
//!   (or `KEYMOTION_OUTPUT=json`) prints exactly one JSON object, on
//!   success and on failure ([`output`]).
//! * Hook mode (`keymotion-cli hook claude-code|codex`) is read before
//!   clap: whatever comes, it prints nothing and exits with 0 ([`hook`]).
//! * Exit codes are kb-agent's (`kb_agent::exit`): 0, 1, 64, 65, 69, 75,
//!   76, 77, never 2. A Claude Code or Codex hook that exits with 2 makes
//!   the agent keep going, or refuses a permission. clap's own usage error
//!   becomes 64 ([`cli::parse`]).
//! * When the app is not running, a command that needs it starts it without
//!   a window and waits for its pipe (`--no-start` to not), in a way that
//!   keeps the app outside the package box a calling app may run in
//!   (`launch`).
//! * Hooks will call it on every tool use, so it starts fast: no runtime, no
//!   logger crate, nothing read before it is needed.

// The program is Windows-only; elsewhere only its parts that need no pipe
// build.
#![cfg_attr(not(windows), allow(dead_code))]

mod cli;
#[cfg(windows)]
mod commands;
#[cfg(windows)]
mod connect;
#[cfg(windows)]
mod doctor;
mod guide;
#[cfg(windows)]
mod hook;
mod human;
#[cfg(windows)]
mod launch;
mod output;
#[cfg(windows)]
mod session;

use std::process::ExitCode;

/// What the CLI calls itself in `hello` and in messages.
pub const NAME: &str = "keymotion-cli";
/// This build's version: always the app's (`version.workspace = true`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(windows)]
fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if args.get(1).is_some_and(|a| a == "hook") {
        hook::main(&args[2..]);
        return ExitCode::from(kb_agent::exit::OK);
    }
    let env = cli::Env::read();
    let code = match cli::parse(args, &env) {
        // --help and --version.
        cli::Parsed::Print { json, clap } => output::print_help(json, &clap),
        cli::Parsed::Usage { json, command, error, clap } => {
            output::usage_error(json, command.as_deref(), &error, &clap)
        }
        cli::Parsed::Run(invocation) => commands::run(*invocation, &env),
    };
    ExitCode::from(code)
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("keymotion-cli talks to Keymotion over a Windows named pipe; it runs on Windows only.");
    ExitCode::from(kb_agent::exit::UNAVAILABLE)
}
