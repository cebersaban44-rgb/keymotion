//! What a coding agent's hook event means for the keyboard.
//!
//! The agent runs `keymotion-cli hook claude-code|codex` with the event as
//! JSON on stdin. [`read`] takes from it only what the lights need: the
//! event's name, the session (for its own lights and its number), the last
//! folder of its working directory (the page shows it beside the number),
//! and the few fields that say what kind of event it is (a notification's
//! type, a failure's kind, a helper agent's type). The prompt, the agent's
//! messages, the transcript's path and every tool's input and output never
//! leave the process: a tool call is named only by [`call key`](call_key), a
//! hash of the call salted with the session, so that a question and its
//! answer can be paired up when several tool calls run at once.
//!
//! Sources, checked 2026-11-22: Claude Code, <https://code.claude.com/docs/en/hooks>;
//! Codex, <https://learn.chatgpt.com/docs/hooks> and `codex-rs/hooks` (the
//! generated input schemas).

use kb_agent::protocol::{HookArgs, HookEvent, MAX_HOOK_LABEL_CHARS};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// The coding agents keymotion-cli connects to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Agent {
    ClaudeCode,
    Codex,
}

impl Agent {
    pub const ALL: [Agent; 2] = [Agent::ClaudeCode, Agent::Codex];

    /// As the command line and the sources name it.
    pub const fn id(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude-code",
            Agent::Codex => "codex",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "Claude Code",
            Agent::Codex => "Codex",
        }
    }

    pub fn parse(name: &str) -> Option<Agent> {
        let n = name.trim();
        Agent::ALL.into_iter().find(|a| a.id().eq_ignore_ascii_case(n))
    }
}

/// The fields of a hook event keymotion-cli reads; serde skips the rest
/// without keeping it.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Payload {
    hook_event_name: String,
    session_id: Option<String>,
    cwd: Option<String>,
    tool_name: Option<String>,
    tool_input: Option<Value>,
    tool_use_id: Option<String>,
    turn_id: Option<String>,
    /// Claude Code's `Notification`.
    notification_type: Option<String>,
    /// Claude Code's `StopFailure` (a string there).
    error: Option<Value>,
    /// `SubagentStop`.
    agent_type: Option<String>,
    /// Codex's `PostModelRequest`: `completed`, `failed` or `cancelled`.
    status: Option<String>,
}

/// One hook event, read: the session's source and what to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    /// `claude-code:3f9a0c1d`, or the agent alone without a session id.
    pub source: String,
    pub args: HookArgs,
}

impl Read {
    pub fn event(&self) -> HookEvent {
        HookEvent::parse(&self.args.event).unwrap_or(HookEvent::Answered)
    }
}

/// Read one event of `agent`; `None` when it is not JSON, is not an event
/// of that agent, or means nothing for the lights (Claude Code's "waiting
/// for your input" notification, for one).
pub fn read(agent: Agent, input: &[u8]) -> Option<Read> {
    // A byte-order mark is not JSON, but a Windows program writing UTF-8
    // to a pipe may put one first.
    let input = input.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(input);
    let p: Payload = serde_json::from_slice(input).ok()?;
    let session = p.session_id.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let (event, calls) = match agent {
        Agent::ClaudeCode => claude_code(&p, session)?,
        Agent::Codex => codex(&p, session)?,
    };
    let source = match session {
        Some(id) => format!("{}:{}", agent.id(), &digest(&["session", id])[..8]),
        None => agent.id().to_string(),
    };
    let args = HookArgs {
        event: event.as_str().to_string(),
        agent: Some(agent.id().to_string()),
        label: p.cwd.as_deref().and_then(folder_label),
        calls,
    };
    Some(Read { source, args })
}

/// Claude Code (<https://code.claude.com/docs/en/hooks>).
fn claude_code(p: &Payload, session: Option<&str>) -> Option<(HookEvent, Vec<String>)> {
    let tool = || p.tool_name.as_deref().unwrap_or_default();
    Some(match p.hook_event_name.as_str() {
        "SessionStart" => (HookEvent::SessionStart, vec![]),
        "UserPromptSubmit" => (HookEvent::Busy, vec![]),
        // Asked before the dialog shows; it carries no tool_use_id.
        "PermissionRequest" => (HookEvent::Attention, calls(&[content_key(session, p)])),
        // The tools that wait for the user without a permission dialog.
        "PreToolUse" if ASKING_TOOLS.contains(&tool()) => {
            (HookEvent::Attention, calls(&[use_key(session, p), content_key(session, p)]))
        }
        "PreToolUse" => return None,
        "PostToolUse" | "PostToolUseFailure" | "PermissionDenied" => {
            (HookEvent::Answered, calls(&[use_key(session, p), content_key(session, p)]))
        }
        // Every call of the batch resolved.
        "PostToolBatch" | "ElicitationResult" => (HookEvent::Answered, vec![]),
        "Elicitation" => (HookEvent::Attention, vec![]),
        "Notification" => match p.notification_type.as_deref()? {
            // A backup for PermissionRequest (and the only word of a
            // sandboxed command's network request): about 6 s after it.
            "permission_prompt" | "elicitation_dialog" | "elicitation_url_dialog" | "agent_needs_input" => {
                (HookEvent::Attention, vec![])
            }
            "elicitation_complete" | "elicitation_response" => (HookEvent::Answered, vec![]),
            // `idle_prompt` comes about 60 s after every finished turn: the
            // turn's `done` already said so, and a flashing "waiting for
            // you" after each of them would wear the signal out.
            _ => return None,
        },
        "Stop" => (HookEvent::Done, vec![]),
        "StopFailure" => {
            let kind = p.error.as_ref().and_then(Value::as_str).unwrap_or_default();
            if USAGE_LIMITS.contains(&kind) {
                (HookEvent::UsageLimit, vec![])
            } else {
                (HookEvent::Error, vec![])
            }
        }
        // Internal helpers (prompt suggestions, /btw) have no type.
        "SubagentStop" if p.agent_type.as_deref().is_some_and(|t| !t.trim().is_empty()) => (HookEvent::Helper, vec![]),
        "TaskCompleted" => (HookEvent::Helper, vec![]),
        "SessionEnd" => (HookEvent::SessionEnd, vec![]),
        _ => return None,
    })
}

/// Claude Code's tools that wait for the user's answer without asking for
/// permission (`PreToolUse` with this matcher).
pub const ASKING_TOOLS: [&str; 2] = ["AskUserQuestion", "ExitPlanMode"];

/// Claude Code's `StopFailure` kinds that mean "come back later", not
/// "something is wrong": behaviour A. An overloaded service (HTTP 529)
/// comes as `server_error`.
pub const USAGE_LIMITS: [&str; 3] = ["rate_limit", "billing_error", "server_error"];

/// Codex (<https://learn.chatgpt.com/docs/hooks>; `codex-rs/hooks`).
fn codex(p: &Payload, session: Option<&str>) -> Option<(HookEvent, Vec<String>)> {
    Some(match p.hook_event_name.as_str() {
        "SessionStart" => (HookEvent::SessionStart, vec![]),
        "UserPromptSubmit" => (HookEvent::Busy, vec![]),
        // No tool_use_id here: the call and its turn name the question.
        "PermissionRequest" => (HookEvent::Attention, calls(&[content_key(session, p), turn_key(session, p)])),
        "PostToolUse" => (HookEvent::Answered, calls(&[use_key(session, p), content_key(session, p)])),
        // The next model request of a turn comes after every call of the
        // turn was answered.
        "PreModelRequest" => (HookEvent::Answered, calls(&[turn_key(session, p)])),
        // A turn that fails ends without Stop: the failed request is the only
        // word of it. Codex names the kind in the error's text only. Codex
        // tries some requests again (seen: a 401, twelve times in a minute);
        // the same turn failing again changes nothing (the engine), and a
        // request of that turn that goes through takes the failure away.
        "PostModelRequest" if p.status.as_deref() == Some("failed") => {
            let text = p.error.as_ref().and_then(Value::as_str).unwrap_or_default();
            let event = if codex_usage_limit(text) { HookEvent::UsageLimit } else { HookEvent::Error };
            (event, calls(&[failure_key(session, p)]))
        }
        "PostModelRequest" if p.status.as_deref() == Some("completed") => {
            (HookEvent::Answered, calls(&[failure_key(session, p)]))
        }
        "PostModelRequest" => return None,
        // The user stopped the turn: whatever it waited for is moot.
        "Interrupt" => (HookEvent::Interrupted, vec![]),
        "Stop" => (HookEvent::Done, vec![]),
        "SubagentStop" => (HookEvent::Helper, vec![]),
        "SessionEnd" => (HookEvent::SessionEnd, vec![]),
        _ => return None,
    })
}

/// Words of Codex's error messages (`codex-rs/protocol/src/error.rs`,
/// lowercase) that mean "come back later": rate and usage limits, quota,
/// credits and spend caps, an overloaded service.
pub const CODEX_USAGE_LIMITS: [&str; 7] =
    ["usage limit", "rate limit", "out of credits", "spend cap", "quota exceeded", "at capacity", "high demand"];

/// Whether a Codex error message means "come back later" (behaviour A):
/// one of [`CODEX_USAGE_LIMITS`], or Codex giving up its retries on a rate
/// limit (429) or a server's error (5xx), as Claude Code's `rate_limit` and
/// `server_error` are A too.
pub fn codex_usage_limit(message: &str) -> bool {
    let text = message.to_lowercase();
    if CODEX_USAGE_LIMITS.iter().any(|m| text.contains(m)) {
        return true;
    }
    // "exceeded retry limit, last status: 429 Too Many Requests[, request id: …]"
    let Some(at) = text.find("exceeded retry limit") else { return false };
    let status = text[at..]
        .split("last status:")
        .nth(1)
        .and_then(|rest| rest.trim_start().get(..3))
        .and_then(|code| code.parse::<u16>().ok());
    matches!(status, Some(429) | Some(500..=599))
}

fn calls(keys: &[Option<String>]) -> Vec<String> {
    keys.iter().flatten().cloned().collect()
}

/// A tool call by its id (`u…`).
fn use_key(session: Option<&str>, p: &Payload) -> Option<String> {
    let id = p.tool_use_id.as_deref().filter(|s| !s.is_empty())?;
    Some(format!("u{}", &digest(&["use", session.unwrap_or_default(), id])[..16]))
}

/// A tool call by what it does (`c…`): its tool and input, without the
/// description an agent may word differently. The only key a permission
/// request has.
fn content_key(session: Option<&str>, p: &Payload) -> Option<String> {
    let tool = p.tool_name.as_deref().filter(|s| !s.is_empty())?;
    let mut input = p.tool_input.clone().unwrap_or(Value::Null);
    if let Value::Object(o) = &mut input {
        o.remove("description");
    }
    // serde_json's maps are sorted here (no preserve_order): the same input
    // always reads the same.
    let canonical = serde_json::to_string(&input).ok()?;
    Some(format!("c{}", &digest(&["call", session.unwrap_or_default(), tool, &canonical])[..16]))
}

/// A turn (`t…`), for Codex.
fn turn_key(session: Option<&str>, p: &Payload) -> Option<String> {
    let turn = p.turn_id.as_deref().filter(|s| !s.is_empty())?;
    Some(format!("t{}", &digest(&["turn", session.unwrap_or_default(), turn])[..16]))
}

/// A turn's failure (`f…`), for Codex: apart from [`turn_key`], so the
/// turn's next request (which answers its questions) leaves the failure.
fn failure_key(session: Option<&str>, p: &Payload) -> Option<String> {
    let turn = p.turn_id.as_deref().filter(|s| !s.is_empty())?;
    Some(format!("f{}", &digest(&["failed", session.unwrap_or_default(), turn])[..16]))
}

/// SHA-256 of the parts, in lowercase hex.
pub(crate) fn digest(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for part in parts {
        h.update(part.as_bytes());
        h.update([0]);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The last folder of a working directory, as the page may show it: one
/// line of visible characters, cut to fit.
pub fn folder_label(cwd: &str) -> Option<String> {
    let last = cwd.trim().trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next()?;
    let clean: String = last.chars().filter(|c| !kb_agent::values::is_unsafe_char(*c)).collect();
    let clean = clean.trim();
    if clean.is_empty() || clean.ends_with(':') {
        return None;
    }
    let cut: String = clean.chars().take(MAX_HOOK_LABEL_CHARS).collect();
    Some(cut.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(agent: Agent, payload: Value) -> Option<(HookEvent, Vec<String>)> {
        read(agent, payload.to_string().as_bytes()).map(|r| (r.event(), r.args.calls))
    }

    fn cc(payload: Value) -> Option<(HookEvent, Vec<String>)> {
        event(Agent::ClaudeCode, payload)
    }

    fn base(name: &str) -> Value {
        json!({
            "session_id": "abc123",
            "transcript_path": "C:\\Users\\a\\.claude\\projects\\x\\00893aaf.jsonl",
            "cwd": "C:\\Users\\a\\dev\\klavye",
            "permission_mode": "default",
            "hook_event_name": name,
        })
    }

    fn with(mut v: Value, extra: Value) -> Value {
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    }

    // Sample payloads: the ones in Claude Code's hooks reference, fields as
    // there (2026-11-22).

    #[test]
    fn claude_codes_events_map_as_planned() {
        let bash = json!({"command": "rm -rf node_modules", "description": "Remove node_modules directory"});
        let cases = [
            (
                with(base("SessionStart"), json!({"source": "startup", "model": "claude-x"})),
                Some(HookEvent::SessionStart),
            ),
            (with(base("UserPromptSubmit"), json!({"prompt": "Write a function"})), Some(HookEvent::Busy)),
            (
                with(
                    base("PermissionRequest"),
                    json!({"tool_name": "Bash", "tool_input": bash, "permission_suggestions": [{"type": "addRules"}]}),
                ),
                Some(HookEvent::Attention),
            ),
            (
                with(
                    base("PreToolUse"),
                    json!({"tool_name": "AskUserQuestion", "tool_input": {"questions": []}, "tool_use_id": "toolu_1"}),
                ),
                Some(HookEvent::Attention),
            ),
            (with(base("PreToolUse"), json!({"tool_name": "Bash", "tool_use_id": "toolu_2"})), None),
            (
                with(
                    base("PostToolUse"),
                    json!({"tool_name": "Write", "tool_input": {"file_path": "/x"}, "tool_response": {"type": "create"}, "tool_use_id": "toolu_3", "duration_ms": 12}),
                ),
                Some(HookEvent::Answered),
            ),
            (
                with(
                    base("PostToolUseFailure"),
                    json!({"tool_name": "Bash", "tool_input": {"command": "npm test"}, "tool_use_id": "toolu_4", "error": "Exit code 1", "is_interrupt": false}),
                ),
                Some(HookEvent::Answered),
            ),
            (
                with(
                    base("PermissionDenied"),
                    json!({"tool_name": "Bash", "tool_input": {"command": "rm -rf /tmp/build"}, "tool_use_id": "toolu_5", "reason": "[Irreversible]"}),
                ),
                Some(HookEvent::Answered),
            ),
            (base("PostToolBatch"), Some(HookEvent::Answered)),
            (
                with(
                    base("Elicitation"),
                    json!({"mcp_server_name": "my-mcp-server", "message": "Please provide", "mode": "form"}),
                ),
                Some(HookEvent::Attention),
            ),
            (
                with(base("ElicitationResult"), json!({"mcp_server_name": "my-mcp-server", "action": "accept"})),
                Some(HookEvent::Answered),
            ),
            (
                with(
                    base("Notification"),
                    json!({"message": "Claude needs your permission", "notification_type": "permission_prompt"}),
                ),
                Some(HookEvent::Attention),
            ),
            (
                with(base("Notification"), json!({"notification_type": "elicitation_dialog"})),
                Some(HookEvent::Attention),
            ),
            (with(base("Notification"), json!({"notification_type": "agent_needs_input"})), Some(HookEvent::Attention)),
            (
                with(base("Notification"), json!({"notification_type": "elicitation_response"})),
                Some(HookEvent::Answered),
            ),
            (with(base("Notification"), json!({"notification_type": "idle_prompt"})), None),
            (with(base("Notification"), json!({"notification_type": "auth_success"})), None),
            (with(base("Notification"), json!({"message": "no type"})), None),
            (
                with(base("Stop"), json!({"stop_hook_active": true, "last_assistant_message": "I've completed it."})),
                Some(HookEvent::Done),
            ),
            (
                with(base("StopFailure"), json!({"error": "rate_limit", "error_details": "429 Too Many Requests"})),
                Some(HookEvent::UsageLimit),
            ),
            (with(base("StopFailure"), json!({"error": "billing_error"})), Some(HookEvent::UsageLimit)),
            (with(base("StopFailure"), json!({"error": "server_error"})), Some(HookEvent::UsageLimit)),
            (with(base("StopFailure"), json!({"error": "authentication_failed"})), Some(HookEvent::Error)),
            (with(base("StopFailure"), json!({"error": "max_output_tokens"})), Some(HookEvent::Error)),
            (with(base("StopFailure"), json!({})), Some(HookEvent::Error)),
            (
                with(
                    base("SubagentStop"),
                    json!({"agent_id": "def456", "agent_type": "Explore", "stop_hook_active": false}),
                ),
                Some(HookEvent::Helper),
            ),
            (with(base("SubagentStop"), json!({"agent_id": "x", "agent_type": ""})), None),
            (
                with(base("TaskCompleted"), json!({"task_id": "task-001", "task_subject": "Implement auth"})),
                Some(HookEvent::Helper),
            ),
            (with(base("SessionEnd"), json!({"reason": "prompt_input_exit"})), Some(HookEvent::SessionEnd)),
            (base("PreCompact"), None),
            (base("SomethingNew"), None),
        ];
        for (payload, expected) in cases {
            let name = payload["hook_event_name"].as_str().unwrap().to_string();
            assert_eq!(cc(payload).map(|e| e.0), expected, "{name}");
        }
    }

    #[test]
    fn a_question_and_its_answer_share_a_key_and_nothing_private_leaves() {
        let input = json!({"command": "cargo test", "description": "Run the tests"});
        let ask = cc(with(base("PermissionRequest"), json!({"tool_name": "Bash", "tool_input": input}))).unwrap();
        // PostToolUse words the description differently, or leaves it out.
        let done = cc(with(
            base("PostToolUse"),
            json!({"tool_name": "Bash", "tool_input": {"command": "cargo test"}, "tool_use_id": "toolu_9", "tool_response": {"stdout": "ok"}}),
        ))
        .unwrap();
        assert_eq!(ask.1.len(), 1);
        assert!(done.1.contains(&ask.1[0]), "{ask:?} {done:?}");
        // Another command, another key; the same command in another session too.
        let other =
            cc(with(base("PostToolUse"), json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}))).unwrap();
        assert!(!other.1.contains(&ask.1[0]));
        let mut elsewhere = with(base("PermissionRequest"), json!({"tool_name": "Bash", "tool_input": input}));
        elsewhere["session_id"] = json!("zzz");
        assert_ne!(cc(elsewhere).unwrap().1, ask.1);

        let full = with(
            base("UserPromptSubmit"),
            json!({"prompt": "my secret plan", "last_assistant_message": "secret answer"}),
        );
        let r = read(Agent::ClaudeCode, full.to_string().as_bytes()).unwrap();
        let sent = serde_json::to_string(&r.args).unwrap() + &r.source;
        for private in ["secret", "transcript", "00893aaf", "abc123", "Users"] {
            assert!(!sent.contains(private), "{private} in {sent}");
        }
        assert_eq!(r.args.label.as_deref(), Some("klavye"));
        assert!(r.source.starts_with("claude-code:") && r.source.len() == "claude-code:".len() + 8);
        for key in done.1.iter().chain(&ask.1) {
            assert!(kb_agent::protocol::HookArgs {
                event: "answered".into(),
                calls: vec![key.clone()],
                ..Default::default()
            }
            .validate()
            .is_ok());
        }
    }

    #[test]
    fn codexs_events_map_as_planned() {
        let cx = |name: &str, extra: Value| {
            let v = with(
                json!({"session_id": "thr_123", "transcript_path": null, "cwd": "/workspace/proj", "hook_event_name": name, "model": "gpt-x", "turn_id": "turn_1"}),
                extra,
            );
            event(Agent::Codex, v)
        };
        assert_eq!(cx("SessionStart", json!({"source": "startup"})).unwrap().0, HookEvent::SessionStart);
        assert_eq!(cx("UserPromptSubmit", json!({"prompt": "x"})).unwrap().0, HookEvent::Busy);
        let ask =
            cx("PermissionRequest", json!({"tool_name": "Bash", "tool_input": {"command": "ls", "description": null}}))
                .unwrap();
        assert_eq!((ask.0, ask.1.len()), (HookEvent::Attention, 2));
        let next =
            cx("PreModelRequest", json!({"attempt_number": 1, "kind": "x", "request_id": "r", "thread_id": "t"}))
                .unwrap();
        assert_eq!(next.0, HookEvent::Answered);
        assert!(ask.1.contains(&next.1[0]), "the turn names the question");
        let ran = cx("PostToolUse", json!({"tool_name": "Bash", "tool_use_id": "call_1", "tool_input": {"command": "ls"}, "tool_response": "x"}))
            .unwrap();
        assert!(ask.1.iter().any(|k| ran.1.contains(k)), "the call names it too");
        assert_eq!(cx("Interrupt", json!({})).unwrap().0, HookEvent::Interrupted);
        assert_eq!(
            cx("Stop", json!({"stop_hook_active": false, "last_assistant_message": null})).unwrap().0,
            HookEvent::Done
        );
        assert_eq!(cx("SubagentStop", json!({"agent_id": "a", "agent_type": "worker"})).unwrap().0, HookEvent::Helper);
        let end = json!({"session_id": "thr_123", "transcript_path": "/workspace/.codex/rollout.jsonl", "cwd": "/workspace", "hook_event_name": "SessionEnd", "reason": "other"});
        assert_eq!(event(Agent::Codex, end).unwrap().0, HookEvent::SessionEnd);
        assert_eq!(cx("PreToolUse", json!({"tool_name": "Bash"})), None);
        // Requests: a failed one says what failed, by its text; one that went
        // through takes its turn's failure away; a cancelled one says nothing.
        let through = cx("PostModelRequest", json!({"status": "completed", "error": null})).unwrap();
        assert_eq!(through.0, HookEvent::Answered);
        assert!(!ask.1.contains(&through.1[0]), "it answers no question");
        assert_eq!(cx("PostModelRequest", json!({"status": "cancelled", "error": null})), None);
        assert_eq!(cx("PostModelRequest", json!({})), None);
        for text in [
            "You\u{2019}ve hit your usage limit. Upgrade to Pro",
            "rate limit exceeded: try again in 20s",
            "Your workspace is out of credits.",
            "Quota exceeded. Check your plan and billing details.",
            "Selected model is at capacity. Please try a different model.",
            "We\u{2019}re currently experiencing high demand, which may cause temporary errors.",
        ] {
            let failed = cx("PostModelRequest", json!({"status": "failed", "error": text})).unwrap();
            assert_eq!(failed.0, HookEvent::UsageLimit, "{text}");
            assert_eq!(failed.1, through.1, "the turn's next request takes it away: {text}");
            assert_ne!(failed.1, next.1, "its next attempt does not: {text}");
        }
        // Retries given up on a rate limit or a server's error: A, as Claude
        // Code's rate_limit and server_error.
        for text in [
            "exceeded retry limit, last status: 429 Too Many Requests",
            "exceeded retry limit, last status: 503 Service Unavailable, request id: req_1",
            "exceeded retry limit, last status: 529",
        ] {
            let failed = cx("PostModelRequest", json!({"status": "failed", "error": text})).unwrap();
            assert_eq!(failed.0, HookEvent::UsageLimit, "{text}");
        }
        for text in [
            "exceeded retry limit, last status: 401 Unauthorized",
            "exceeded retry limit",
            "unexpected status 401 Unauthorized: Missing bearer",
            "context window exceeded",
            "",
        ] {
            let failed = cx("PostModelRequest", json!({"status": "failed", "error": text})).unwrap();
            assert_eq!(failed.0, HookEvent::Error, "{text}");
        }
        // Claude Code's names mean nothing to the Codex reader, and back.
        assert_eq!(cx("StopFailure", json!({"error": "rate_limit"})), None);
    }

    #[test]
    fn anything_else_reads_as_nothing() {
        for junk in [&b""[..], b"null", b"[]", b"{", b"\xff\xfe", b"{\"hook_event_name\": 5}", b"42"] {
            assert_eq!(read(Agent::ClaudeCode, junk), None, "{junk:?}");
            assert_eq!(read(Agent::Codex, junk), None, "{junk:?}");
        }
        // No session id: the agent's own source.
        // A byte-order mark first (.NET writes one), a newline last.
        assert!(read(Agent::Codex, b"\xEF\xBB\xBF{\"hook_event_name\":\"Stop\"}\r\n").is_some());
        let r = read(Agent::Codex, br#"{"hook_event_name":"Stop"}"#).unwrap();
        assert_eq!((r.source.as_str(), r.args.label.clone()), ("codex", None));
    }

    #[test]
    fn folder_labels_are_short_visible_names() {
        assert_eq!(folder_label(r"C:\Users\a\dev\klavye").as_deref(), Some("klavye"));
        assert_eq!(folder_label("/home/a/my project/").as_deref(), Some("my project"));
        assert_eq!(folder_label(r"C:\").as_deref(), None);
        assert_eq!(folder_label("").as_deref(), None);
        assert_eq!(folder_label("x/\u{1b}[31mred\u{202e}").as_deref(), Some("[31mred"));
        assert_eq!(folder_label(&format!("/{}", "é".repeat(100))).unwrap().chars().count(), MAX_HOOK_LABEL_CHARS);
        assert_eq!(Agent::parse(" Claude-Code "), Some(Agent::ClaudeCode));
        assert_eq!(Agent::parse("cursor"), None);
    }
}
