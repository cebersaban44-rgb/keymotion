//! `keymotion-cli hook claude-code|codex`: what the coding agents run on
//! their hook events (`kb_hooks::setup`).
//!
//! * Always exit 0, never a byte on stdout, whatever happens: bad JSON, an
//!   event or agent it does not know, no app, access off, a panic. An exit
//!   code of 2 means "deny", "keep going" or "block" to these agents, and
//!   Codex reads text on stdout as a decision.
//! * Fast: the agent may wait for it (Codex's approval screen does). The
//!   event is read from stdin with a size limit and a deadline (and not at
//!   all from a terminal); the app gets one short request.
//! * When the app is not running, the hook never waits for it: an event that
//!   lights something goes to a copy of keymotion-cli that runs on its own
//!   (no handles, no window), which starts Keymotion as `launch` does and
//!   then delivers it. A question answered, a new prompt or a session's end
//!   is not worth starting the app for.
//! * Only the event's meaning leaves the process (`kb_hooks::event`): never
//!   the prompt, the agent's messages, the transcript or a tool's input.

use std::ffi::OsString;
use std::io::{IsTerminal, Read};
use std::time::Duration;

use kb_agent::protocol::HookArgs;
use kb_agent::{Command, ErrorCode, HookEvent};
use kb_hooks::event::{self, Agent};

use crate::cli::{Env, Global, LogLevel};
use crate::output::Logger;
use crate::session::{self, Ctx, IfNotRunning};

/// The most of stdin read: a tool's whole output may come with the event.
pub const MAX_INPUT: usize = 16 * 1024 * 1024;
/// How long stdin may take.
const READ_WAIT: Duration = Duration::from_secs(2);
/// Connecting and the request, together, when the app runs.
const CALL_WAIT: &str = "1500ms";

/// Hook mode from `main`: `args` are what follows `hook`. Returns when done;
/// the caller exits with 0.
pub fn main(args: &[OsString]) {
    let words: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    let agent = words.first().cloned().unwrap_or_default();
    let deliver = match words.get(1).map(String::as_str) {
        Some("--deliver") => words.get(2).cloned(),
        _ => None,
    };
    run(&agent, deliver.as_deref(), &Env::read());
}

/// Hook mode: nothing it does reaches stdout or the exit code.
pub fn run(agent: &str, deliver: Option<&str>, env: &Env) {
    // A panic's message would go to stderr; the agent shows that to nobody,
    // but there is no reason to write it either.
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(|| {
        let Some(agent) = Agent::parse(agent) else { return };
        match deliver {
            Some(token) => deliver_later(env, token),
            None => {
                let input = read_input(MAX_INPUT, READ_WAIT);
                if let Some(read) = event::read(agent, &input) {
                    send(env, &read);
                }
            }
        }
    });
}

/// The event on stdin, or nothing: from a terminal (nobody would type an
/// event), past `max` bytes, or not there within `wait`.
fn read_input(max: usize, wait: Duration) -> Vec<u8> {
    if std::io::stdin().is_terminal() {
        return Vec::new();
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::Builder::new().name("stdin".into()).spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::stdin().lock().take(max as u64 + 1).read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    if reader.is_err() {
        return Vec::new();
    }
    // A reader still waiting is left behind: the process ends anyway.
    match rx.recv_timeout(wait) {
        Ok(buf) if buf.len() <= max => buf,
        _ => Vec::new(),
    }
}

/// What every request of hook mode runs with: the session's source, a short
/// time, no start, no messages.
fn ctx(env: &Env, source: &str, timeout: Option<&str>, no_start: bool) -> Option<Ctx> {
    let global = Global {
        source: Some(source.to_string()),
        timeout: timeout.map(str::to_string),
        no_start,
        quiet: true,
        log: Some(LogLevel::Off),
        ..Global::default()
    };
    Ctx::new(&global, env, Logger::new(Some(LogLevel::Off), true)).ok()
}

/// Send the event; when the app is not running and the event lights
/// something, hand it to a copy of ourselves that starts the app.
fn send(env: &Env, read: &event::Read) {
    let Some(ctx) = ctx(env, &read.source, Some(CALL_WAIT), true) else { return };
    let command = Command::Hook(read.args.clone());
    match session::connect(&ctx, IfNotRunning::Report) {
        Ok(mut conn) => {
            let left = conn.left();
            let _ = conn.client.call_within(&command, left);
        }
        Err(e) if e.code == ErrorCode::AppNotRunning && read.event().lights() => {
            if let Ok(exe) = std::env::current_exe() {
                let token = encode(read);
                let agent = read.args.agent.as_deref().unwrap_or("claude-code");
                let _ = kb_winsys::shell::spawn_detached(&exe, &["hook", agent, "--deliver", &token]);
            }
        }
        Err(_) => {}
    }
}

/// The copy that runs on its own: start the app (it may take a few seconds)
/// and deliver the event.
fn deliver_later(env: &Env, token: &str) {
    let Some(read) = decode(token) else { return };
    if !read.event().lights() {
        return;
    }
    let Some(ctx) = ctx(env, &read.source, None, false) else { return };
    if let Ok(mut conn) = session::connect(&ctx, IfNotRunning::Start) {
        let left = conn.left();
        let _ = conn.client.call_within(&Command::Hook(read.args), left);
    }
}

/// An event as one plain command line argument: its JSON in hex.
fn encode(read: &event::Read) -> String {
    let v = serde_json::json!({ "source": read.source, "args": read.args });
    v.to_string().bytes().map(|b| format!("{b:02x}")).collect()
}

fn decode(token: &str) -> Option<event::Read> {
    if token.len() % 2 != 0 || token.len() > 16 * 1024 {
        return None;
    }
    let bytes: Option<Vec<u8>> =
        (0..token.len()).step_by(2).map(|i| u8::from_str_radix(token.get(i..i + 2)?, 16).ok()).collect();
    let v: serde_json::Value = serde_json::from_slice(&bytes?).ok()?;
    let source = v["source"].as_str()?.to_string();
    let args: HookArgs = serde_json::from_value(v["args"].clone()).ok()?;
    args.validate().ok()?;
    HookEvent::parse(&args.event).ok()?;
    Some(event::Read { source, args })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_goes_through_the_command_line_and_back() {
        let read = event::read(
            Agent::ClaudeCode,
            br#"{"hook_event_name":"Stop","session_id":"s1","cwd":"C:\\Users\\a b\\proje \u00e7"}"#,
        )
        .unwrap();
        let token = encode(&read);
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()), "a plain argument");
        assert_eq!(decode(&token), Some(read));
        for bad in ["", "0", "zz", "7b7d", &"ab".repeat(9000)] {
            assert_eq!(decode(bad), None, "{bad}");
        }
    }
}
