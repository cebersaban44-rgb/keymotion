//! `connect`, `disconnect` and `connections`: keymotion-cli's hooks, skill
//! and (for Codex) rule in a coding agent's settings, through
//! `kb_hooks::setup`, which the app's Agents page calls too.
//!
//! `connect` and `disconnect` say what they will change first. At a
//! terminal they ask; otherwise they need `--yes` (`--dry-run` only says).
//! They never run inside hook mode or from an agent's sandbox: the Codex
//! rule leaves them out.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use kb_agent::{AgentError, ErrorCode};
use kb_hooks::setup::{self, Cli, ErrorKind, Homes, Plan, SetupError, Status};
use kb_hooks::Agent;
use serde_json::{json, Value};

use crate::cli::Env;
use crate::output::{write_stderr, Done};

/// The agents' folders: the user's, or a test's ([`crate::cli::TEST_HOME_ENV`]).
pub fn homes(env: &Env) -> Result<Homes, AgentError> {
    if let Some(home) = env.test_home.as_deref().filter(|_| crate::cli::TEST_PIPE_BUILD) {
        let home = PathBuf::from(home);
        // The agents' own variables count only inside the test's home.
        let inside = |name: &str| {
            std::env::var_os(name).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty() && p.starts_with(&home))
        };
        return Ok(Homes::with(&home, inside("CLAUDE_CONFIG_DIR"), inside("CODEX_HOME")));
    }
    Homes::from_env().ok_or_else(|| AgentError::new(ErrorCode::Internal, "No home folder (USERPROFILE is not set)."))
}

/// This keymotion-cli, by the path that stays put.
pub fn this_cli() -> Result<Cli, AgentError> {
    let exe = std::env::current_exe()
        .map_err(|e| AgentError::new(ErrorCode::Internal, format!("keymotion-cli cannot find itself: {e}")))?;
    let packaged = kb_winsys::package::current().is_some();
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    Ok(Cli::for_exe(&exe, packaged, local.as_deref()))
}

fn setup_error(e: SetupError) -> AgentError {
    match e.kind {
        ErrorKind::Invalid => AgentError::new(ErrorCode::BadValue, e.message),
        ErrorKind::Io => AgentError::new(ErrorCode::Internal, e.message),
        ErrorKind::Unsupported => AgentError::new(ErrorCode::Unsupported, e.message),
        ErrorKind::Changed => AgentError::new(ErrorCode::Internal, e.message)
            .with_hint("The agent kept changing its settings; run it again when the agent is idle."),
    }
}

/// How many times a file the agent changed meanwhile is planned again.
const TRIES: usize = 3;

/// `connect` or `disconnect`.
pub fn run(
    env: &Env,
    agent: Agent,
    connecting: bool,
    yes: bool,
    dry_run: bool,
    json: bool,
) -> Result<Done, AgentError> {
    let homes = homes(env)?;
    let cli = if connecting { Some(this_cli()?) } else { None };
    let verb = if connecting { "connect" } else { "disconnect" };
    let interactive = !json && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    let mut confirm = |plan: &Plan, preview: &str, again: bool| -> Result<bool, AgentError> {
        if yes {
            return Ok(true);
        }
        if !interactive {
            return Err(AgentError::new(
                ErrorCode::BadArgument,
                format!("{verb} changes {}'s settings and asks first.", agent.title()),
            )
            .with_hint(format!("Add --yes to {verb} without asking, or --dry-run to see what would change.")));
        }
        if again {
            write_stderr(&format!(
                "{} changed its settings meanwhile; this is what would change now.\n",
                agent.title()
            ));
        }
        write_stderr(&format!("{preview}\n"));
        for w in &plan.warnings {
            write_stderr(&format!("note: {w}\n"));
        }
        Ok(ask(&format!("{} {}? [y/N] ", capitalized(verb), agent.title())))
    };
    change(agent, &homes, cli.as_ref(), dry_run, &mut confirm)
}

/// Plan, ask (`confirm`, told whether it asks again), write. The files are
/// read again right before they are written: one the agent changed since
/// the plan was made is planned again from what it is now and asked about
/// again, never written from the old copy.
fn change(
    agent: Agent,
    homes: &Homes,
    cli: Option<&Cli>,
    dry_run: bool,
    confirm: &mut dyn FnMut(&Plan, &str, bool) -> Result<bool, AgentError>,
) -> Result<Done, AgentError> {
    let connecting = cli.is_some();
    let mut tries = 0;
    loop {
        let plan = match cli {
            Some(cli) => setup::plan_connect(agent, homes, cli),
            None => setup::plan_disconnect(agent, homes),
        }
        .map_err(setup_error)?;
        if plan.is_empty() {
            let text = if connecting {
                format!("{} is connected already; nothing to change.", agent.title())
            } else {
                format!("{} has nothing of Keymotion's; nothing to change.", agent.title())
            };
            return Ok(Done::new(result(&plan, false), text));
        }
        let preview = describe(&plan, homes.of(agent), connecting);
        if dry_run {
            return Ok(Done::new(result(&plan, false), preview).warn(plan.warnings.clone()));
        }
        if !confirm(&plan, &preview, tries > 0)? {
            return Ok(Done::new(
                json!({ "agent": agent.id(), "changed": false, "cancelled": true }),
                "Nothing was changed.",
            ));
        }
        match setup::apply(&plan) {
            Ok(()) => {
                let text = if connecting {
                    format!("Connected {}.\n{preview}", agent.title())
                } else {
                    format!("Disconnected {}.\n{preview}", agent.title())
                };
                return Ok(Done::new(result(&plan, true), text).warn(plan.warnings.clone()));
            }
            Err(e) if e.kind == ErrorKind::Changed && tries + 1 < TRIES => tries += 1,
            Err(e) => return Err(setup_error(e)),
        }
    }
}

fn capitalized(word: &str) -> String {
    let mut c = word.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// A y/N question on the terminal.
fn ask(question: &str) -> bool {
    write_stderr(question);
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn result(plan: &Plan, changed: bool) -> Value {
    let mut v = plan.to_value();
    v["changed"] = json!(changed);
    v
}

/// The plan for people: each file, what happens to it.
fn describe(plan: &Plan, home: &Path, connecting: bool) -> String {
    let mut out = String::new();
    for e in plan.changes() {
        let action = match (&e.before, &e.after) {
            (None, Some(_)) => "new",
            (Some(_), None) => "remove",
            _ => "change",
        };
        let shown =
            e.path.strip_prefix(home).map_or_else(|_| e.path.display().to_string(), |p| p.display().to_string());
        out.push_str(&format!("  {action:<7} {shown}: {}\n", e.summary));
        if e.backup && e.before.is_some() && e.after.is_some() {
            out.push_str(&format!("          (a copy of it as it was: {}{})\n", shown, setup::BACKUP_SUFFIX));
        }
    }
    let head = if connecting { "Keymotion adds, in" } else { "Keymotion takes away, in" };
    let mut text = format!("{head} {}:\n{out}", home.display());
    if let Some(line) = &plan.command_line {
        text.push_str(&format!("Each hook runs: {line}\n"));
    }
    text.trim_end().to_string()
}

/// `connections`: both agents.
pub fn connections(env: &Env) -> Result<Done, AgentError> {
    let homes = homes(env)?;
    let cli = this_cli().ok();
    let all: Vec<Status> = Agent::ALL.iter().map(|a| setup::status(*a, &homes, cli.as_ref())).collect();
    let text = all.iter().map(status_line).collect::<Vec<_>>().join("\n");
    let mut warnings = Vec::new();
    for s in &all {
        warnings.extend(s.error.iter().chain(&s.warning).cloned());
    }
    let data = json!({
        "cli": cli.as_ref().map(|c| json!({ "path": c.path_text(), "kind": format!("{:?}", c.kind).to_lowercase() })),
        "agents": all.iter().map(Status::to_value).collect::<Vec<_>>(),
    });
    Ok(Done::new(data, text).warn(warnings))
}

fn status_line(s: &Status) -> String {
    let what = match s.state {
        "connected" => format!("connected ({} hooks, skill {})", s.hooks, s.skill.unwrap_or_default()),
        "needs_approval" => format!(
            "connected, waiting for your approval in Codex ({} of {} hooks trusted): run /hooks in Codex, or Settings > Hooks in the app",
            s.trusted.unwrap_or_default(),
            s.hooks
        ),
        "outdated" => format!(
            "connected, but not as this keymotion-cli would ({} of {} hooks); run keymotion-cli connect {}",
            s.hooks,
            s.hooks_wanted,
            s.agent.id()
        ),
        "not_connected" => format!("not connected; keymotion-cli connect {} connects it", s.agent.id()),
        "unreadable" => "its settings file does not read; see the warning".to_string(),
        _ => "not found on this computer".to_string(),
    };
    format!("{:<12} {what}", s.agent.title())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("km-connect-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cli(home: &Path) -> Cli {
        Cli { path: home.join(r"bin\keymotion-cli.exe"), kind: setup::CliKind::Installed }
    }

    #[test]
    fn a_file_the_agent_changed_while_the_user_decided_is_planned_and_asked_again() {
        let home = temp("again");
        let homes = Homes::with(&home, None, None);
        let settings = home.join(r".claude\settings.json");
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(&settings, "{\n  \"model\": \"opus\"\n}\n").unwrap();
        let mut asked = Vec::new();
        let mut confirm = |_: &Plan, _: &str, again: bool| {
            asked.push(again);
            if !again {
                // Claude Code writes its settings while the question waits.
                std::fs::write(&settings, "{\n  \"model\": \"sonnet\",\n  \"theme\": \"dark\"\n}\n").unwrap();
            }
            Ok(true)
        };
        let done = change(Agent::ClaudeCode, &homes, Some(&cli(&home)), false, &mut confirm).unwrap();
        assert_eq!(asked, [false, true]);
        assert_eq!(done.data["changed"], true);
        let text = std::fs::read_to_string(&settings).unwrap();
        assert!(text.contains("\"sonnet\"") && text.contains("\"theme\"") && text.contains("hook"), "{text}");
        // The copy kept is the file as it was just before Keymotion wrote it.
        let kept =
            std::fs::read_to_string(home.join(format!(r".claude\settings.json{}", setup::BACKUP_SUFFIX))).unwrap();
        assert!(kept.contains("\"sonnet\""), "{kept}");

        // An agent that never stops writing: given up after a few tries,
        // nothing written from an old copy.
        let mut n = 0;
        let mut busy = |_: &Plan, _: &str, _: bool| {
            n += 1;
            std::fs::write(home.join(r".codex\hooks.json"), format!("{{\"hooks\": {{}}{}}}", " ".repeat(n))).unwrap();
            Ok(true)
        };
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        let e = change(Agent::Codex, &homes, Some(&cli(&home)), false, &mut busy).unwrap_err();
        assert!(e.hint.as_deref().is_some_and(|h| h.contains("idle")), "{e:?}");
        assert_eq!(n, TRIES);
        let _ = std::fs::remove_dir_all(&home);
    }
}
