//! Running a command: its plan, the app's answer, the output.

use kb_agent::protocol::SUPPORTED_PROTOCOLS;
use kb_agent::{AgentError, Command, ErrorCode};
use serde_json::{json, Value};

use crate::cli::{self, Env, Invocation, Plan};
use crate::output::{self, Done, Logger};
use crate::session::{self, Ctx, Started};
use crate::{doctor, guide, human};

/// Run a command line clap took; the exit code.
pub fn run(inv: Invocation, env: &Env) -> u8 {
    let json = inv.global.json || env.json();
    let quiet = inv.global.quiet;
    let name = inv.command.name();
    let log = Logger::new(inv.global.log, quiet);
    let result = inv.command.plan().and_then(|plan| match plan {
        // Needs nothing, so a wrong KEYMOTION_SOURCE does not hide it.
        Plan::Guide => Ok(Done::new(guide::data(), guide::GUIDE)),
        plan => execute(&Ctx::new(&inv.global, env, log)?, plan),
    });
    output::finish(json, quiet, name, result)
}

fn execute(ctx: &Ctx, plan: Plan) -> Result<Done, AgentError> {
    match plan {
        Plan::Call(command) => call(ctx, command),
        Plan::Status => status(ctx),
        Plan::Version => version(ctx),
        Plan::Guide => Ok(Done::new(guide::data(), guide::GUIDE)),
        Plan::Doctor => Ok(doctor::run(ctx)),
        Plan::Start => start(ctx),
    }
}

/// A command for the app, which is started first when it is not running.
fn call(ctx: &Ctx, command: Command) -> Result<Done, AgentError> {
    let mut conn = session::connect(ctx, true)?;
    let left = conn.left();
    let data = conn.client.call_within(&command, left)?;
    let mut warnings = Vec::new();
    if let Some(Started { method: Some(_), waited_ms }) = &conn.started {
        warnings.push(started_note(*waited_ms));
    }
    let name = command.name();
    let text = match name {
        "keys" => human::keys(&data),
        "signal" | "light" | "progress" => {
            warnings.extend(human::lit_warnings(&data));
            human::lit(name, &data)
        }
        "clear" => human::clear(&data),
        "log" => human::log(&data, now_ms()),
        "settings_get" => human::settings_get(&data),
        "settings_set" => human::settings_set(&data),
        _ => String::new(),
    };
    Ok(Done { data, warnings, text })
}

fn started_note(waited_ms: u64) -> String {
    format!("Keymotion was not running; started it in the background ({}).", human::duration(waited_ms))
}

fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_millis() as f64)
}

/// `status`: works when the app is not running (exit 0, `running: false`).
fn status(ctx: &Ctx) -> Result<Done, AgentError> {
    match session::connect(ctx, false) {
        Ok(mut conn) => {
            let left = conn.left();
            let data = conn.client.call_within(&cli::status_command(), left)?;
            let mut warnings = Vec::new();
            if data["agents"]["enabled"] == json!(false) {
                warnings.push(
                    "Agent access is off: lights are refused until the user turns it on in Keymotion > Tools > AI agents."
                        .to_string(),
                );
            }
            let text = human::status(&data);
            Ok(Done { data, warnings, text })
        }
        Err(e) if e.code == ErrorCode::AppNotRunning => {
            if ctx.app_running() {
                let data = json!({ "app": { "running": true, "answering": false } });
                let text = human::status(&data);
                Ok(Done::new(data, text).warn([
                    "Keymotion runs but does not answer agents yet: it may be starting, or older than keymotion-cli."
                        .to_string(),
                ]))
            } else {
                let data = json!({ "app": { "running": false } });
                let text = human::status(&data);
                Ok(Done::new(data, text))
            }
        }
        Err(e) => Err(e),
    }
}

/// The CLI's own version line, for `version` and `doctor`.
pub fn cli_data() -> Value {
    json!({
        "version": crate::VERSION,
        "protocol": SUPPORTED_PROTOCOLS.max,
        "protocols": { "min": SUPPORTED_PROTOCOLS.min, "max": SUPPORTED_PROTOCOLS.max },
    })
}

/// `version`: the CLI's, and the app's when it runs (never starts it).
fn version(ctx: &Ctx) -> Result<Done, AgentError> {
    let mut warnings = Vec::new();
    let app = match session::connect(ctx, false) {
        Ok(conn) => {
            let a = conn.app();
            if a.version != crate::VERSION {
                warnings.push(format!(
                    "Keymotion is {} and keymotion-cli is {}; they still work together (protocol {}).",
                    a.version,
                    crate::VERSION,
                    a.protocol
                ));
            }
            json!({ "running": true, "compatible": true, "version": a.version, "channel": a.channel, "protocol": a.protocol })
        }
        Err(e) if e.code == ErrorCode::AppNotRunning && ctx.app_running() => {
            json!({ "running": true, "answering": false })
        }
        Err(e) if e.code == ErrorCode::AppNotRunning => json!({ "running": false }),
        Err(e) if e.code == ErrorCode::ProtocolMismatch => {
            warnings.push(format!("{} {}", e.message, e.hint.as_deref().unwrap_or_default()).trim().to_string());
            json!({
                "running": true,
                "compatible": false,
                "error": { "code": e.code.as_str(), "message": e.message, "hint": e.hint },
            })
        }
        Err(e) => return Err(e),
    };
    let data = json!({ "cli": cli_data(), "app": app });
    let text = human::version(&data);
    Ok(Done::new(data, text).warn(warnings))
}

/// `start`: start the app without a window and wait for it; nothing to do
/// when it runs.
fn start(ctx: &Ctx) -> Result<Done, AgentError> {
    if ctx.no_start {
        return Err(AgentError::new(ErrorCode::BadArgument, "start and --no-start ask for opposite things.")
            .with_hint("Leave out --no-start."));
    }
    let conn = session::connect(ctx, true)?;
    let a = conn.app();
    let app = json!({ "version": a.version, "channel": a.channel, "protocol": a.protocol });
    let data = match &conn.started {
        Some(Started { method: Some(method), waited_ms }) => {
            json!({ "already_running": false, "method": method, "waited_ms": waited_ms, "app": app })
        }
        // Running already, perhaps with its pipe still opening.
        other => json!({
            "already_running": true,
            "method": null,
            "waited_ms": other.as_ref().map_or(0, |s| s.waited_ms),
            "app": app,
        }),
    };
    let text = human::start(&data);
    Ok(Done::new(data, text))
}
