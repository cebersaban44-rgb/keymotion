//! Send one command to the running Keymotion and print its answer: a
//! stand-in for `keymotion-cli` while it does not exist yet.
//!
//! ```text
//! cargo run -p kb-agent --example agent_call -- status
//! cargo run -p kb-agent --example agent_call -- signal name=done
//! cargo run -p kb-agent --example agent_call -- light color=amber keys=f1-f12 effect=wave for_ms=10000
//! cargo run -p kb-agent --example agent_call -- signal '{"name":"attention","keys":"all"}'
//! cargo run -p kb-agent --example agent_call -- settings_set signals.done.color=green
//! ```
//!
//! Arguments are `key=value` pairs (a value that reads as JSON, such as
//! `5`, `true` or `"x"`, is taken as JSON, anything else as text) or one
//! JSON object. For `settings_set` the pairs are the settings to change.
//! The source is `KEYMOTION_SOURCE` or `cli`. Prints the answer as JSON
//! and exits with the code `keymotion-cli` would use.

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use kb_agent::transport::{current_pipe_name, Client};
    use kb_agent::{AgentError, Command, ErrorCode, SourceName};
    use serde_json::{json, Map, Value};
    use std::time::Duration;

    fn args_of(cmd: &str, rest: &[String]) -> Result<Map<String, Value>, AgentError> {
        if let [one] = rest {
            if one.trim_start().starts_with('{') {
                return serde_json::from_str(one)
                    .map_err(|e| AgentError::new(ErrorCode::BadArgument, format!("Not a JSON object: {e}")));
            }
        }
        let mut pairs = Map::new();
        for item in rest {
            let (key, value) = item
                .split_once('=')
                .ok_or_else(|| AgentError::new(ErrorCode::BadArgument, format!("'{item}' is not key=value.")))?;
            let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
            pairs.insert(key.to_string(), value);
        }
        Ok(if cmd == "settings_set" { Map::from_iter([("values".to_string(), Value::Object(pairs))]) } else { pairs })
    }

    fn run() -> Result<Value, AgentError> {
        let argv: Vec<String> = std::env::args().skip(1).collect();
        let Some((cmd, rest)) = argv.split_first() else {
            return Err(AgentError::new(ErrorCode::BadArgument, "Name a command, e.g. status or signal name=done."));
        };
        let command = Command::from_parts(cmd, &args_of(cmd, rest)?)?;
        let source = SourceName::parse(&std::env::var("KEYMOTION_SOURCE").unwrap_or_else(|_| "cli".into()))?;
        let pipe = current_pipe_name()
            .map_err(|e| AgentError::new(ErrorCode::Internal, format!("No pipe name for this user: {e}")))?;
        let mut client =
            Client::connect(&pipe, source, "agent_call", env!("CARGO_PKG_VERSION"), Duration::from_secs(5))?;
        client.call(&command)
    }

    let result = run();
    let printed = match &result {
        Ok(data) => json!({ "ok": true, "data": data }),
        Err(e) => json!({
            "ok": false,
            "error": { "code": e.code.as_str(), "message": e.message, "hint": e.hint, "retry_after_ms": e.retry_after_ms }
        }),
    };
    println!("{}", serde_json::to_string_pretty(&printed).unwrap_or_default());
    std::process::ExitCode::from(kb_agent::error::exit_code(&result))
}

#[cfg(not(windows))]
fn main() {
    eprintln!("agent_call talks to Keymotion over a Windows named pipe; it runs on Windows only.");
}
