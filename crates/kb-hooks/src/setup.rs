//! Connecting keymotion-cli to Claude Code and Codex, and taking it away.
//!
//! What "connected" adds (plan §12.2, approved 2026-11-22):
//!
//! * Hooks: the agent runs `keymotion-cli hook <agent>` on the events in
//!   [`CLAUDE_HOOKS`] / [`CODEX_HOOKS`]. The line is the same for every event
//!   and never changes between versions: what an event means lives in
//!   keymotion-cli and in the user's settings, so updating Keymotion never
//!   touches these files (Codex asks the user to trust a hook again when it
//!   changes). It names keymotion-cli by an absolute path that stays put:
//!   the Store copy's app execution alias, the GitHub copy's `bin` folder,
//!   or (with a warning) a development build's own path ([`Cli`]).
//!   * Claude Code (`settings.json`): the program and its arguments apart
//!     (`command` + `args`), so no shell reads them: Claude Code runs hooks
//!     with Git Bash when it is there and PowerShell otherwise, and the two
//!     cannot agree on one quoted line.
//!   * Codex (`hooks.json`): one line for its shell, PowerShell on Windows
//!     ([`codex_command`]).
//! * Codex only, a rule (`rules/keymotion.rules`) that lets the agent's own
//!   keymotion-cli calls run outside Codex's sandbox, which cannot reach
//!   Keymotion's per-user pipe: lights and status, and the focus timer,
//!   profiles and screen cards (each still behind the user's own permission
//!   in Keymotion). Not `connect`, `disconnect` or `hook`.
//! * One skill, the same text for both ([`skill_text`]): "keymotion-cli is
//!   here; run `keymotion-cli guide` first". Keymotion rewrites its own copy
//!   when it is outdated ([`refresh`]).
//!
//! Rules for other programs' files: strict JSON only (a file that does not
//! read is refused and left alone); the user's keys and their order stay;
//! a copy of the file is kept before the first change (`<file>.before-
//! keymotion`); files are written whole, through a temporary file; doing it
//! twice changes nothing. Taking it away removes only our own entries and
//! files, and only the empty containers we made (`connection.json` beside
//! the skill remembers which).

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::event::Agent;
use crate::json::Json;

/// The skill's version: [`refresh`] rewrites an older copy.
pub const SKILL_VERSION: u32 = 3;
/// The marker in our skill file, with its version after it.
pub const SKILL_MARKER: &str = "keymotion-skill:";
/// The Codex rule's version, and its marker.
pub const RULES_VERSION: u32 = 3;
pub const RULES_MARKER: &str = "keymotion-rules:";
/// The folder of our skill, under each agent's skills folder.
pub const SKILL_DIR: &str = "keymotion";
/// Beside the skill: what connecting made.
const STATE_FILE: &str = "connection.json";
/// The copy of a file kept before our first change to it.
pub const BACKUP_SUFFIX: &str = ".before-keymotion";

/// One hook of ours: the event, a matcher, whether the agent waits for it,
/// and its time limit (keymotion-cli itself ends within about 3 s).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OurHook {
    pub event: &'static str,
    pub matcher: Option<&'static str>,
    /// The agent does not wait for it: for frequent events and those whose
    /// order does not matter.
    pub run_async: bool,
    pub timeout_s: u32,
}

const fn hook(event: &'static str, run_async: bool) -> OurHook {
    OurHook { event, matcher: None, run_async, timeout_s: 10 }
}

/// Claude Code's events (<https://code.claude.com/docs/en/hooks>). The agent
/// waits for the few that start or end something, so they reach Keymotion
/// in order; not for tool calls.
pub const CLAUDE_HOOKS: [OurHook; 16] = [
    hook("SessionStart", false),
    hook("UserPromptSubmit", false),
    OurHook { event: "PreToolUse", matcher: Some(CLAUDE_ASKING_MATCHER), run_async: true, timeout_s: 10 },
    hook("PermissionRequest", false),
    hook("PermissionDenied", true),
    hook("PostToolUse", true),
    hook("PostToolUseFailure", true),
    hook("PostToolBatch", true),
    hook("Notification", true),
    hook("Elicitation", true),
    hook("ElicitationResult", true),
    hook("Stop", false),
    hook("StopFailure", true),
    hook("SubagentStop", true),
    hook("TaskCompleted", true),
    // Its time also bounds every SessionEnd hook of the user's (Claude Code
    // takes the largest): keep it short.
    OurHook { event: "SessionEnd", matcher: None, run_async: false, timeout_s: 2 },
];

/// `PreToolUse` only for the tools that wait for the user without a
/// permission dialog.
pub const CLAUDE_ASKING_MATCHER: &str = "AskUserQuestion|ExitPlanMode";

/// Codex's events (<https://learn.chatgpt.com/docs/hooks>, `codex-rs` at
/// 0.166.0). Codex starts PowerShell for every hook, a few hundred
/// milliseconds: it waits for none but `Interrupt` and `SessionEnd` (always
/// waited for; at most 3 s).
pub const CODEX_HOOKS: [OurHook; 10] = [
    hook("SessionStart", true),
    hook("UserPromptSubmit", true),
    hook("PermissionRequest", true),
    hook("PostToolUse", true),
    hook("PreModelRequest", true),
    hook("PostModelRequest", true),
    hook("Stop", true),
    OurHook { event: "Interrupt", matcher: None, run_async: false, timeout_s: 3 },
    hook("SubagentStop", true),
    OurHook { event: "SessionEnd", matcher: None, run_async: false, timeout_s: 3 },
];

pub fn our_hooks(agent: Agent) -> &'static [OurHook] {
    match agent {
        Agent::ClaudeCode => &CLAUDE_HOOKS,
        Agent::Codex => &CODEX_HOOKS,
    }
}

// ------------------------------------------------------------------ where

/// The folders the agents keep their settings in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Homes {
    /// Claude Code: `CLAUDE_CONFIG_DIR`, else `%USERPROFILE%\.claude`.
    pub claude: PathBuf,
    /// Codex: `CODEX_HOME`, else `%USERPROFILE%\.codex`.
    pub codex: PathBuf,
    /// The user's skills for Codex (the open Agent Skills folder):
    /// `%USERPROFILE%\.agents\skills`.
    pub agent_skills: PathBuf,
    /// The user's folder, `%USERPROFILE%`: where Claude Code's own
    /// installation is found ([`claude_code_versions`]).
    pub home: PathBuf,
}

impl Homes {
    /// The real ones, from the environment.
    pub fn from_env() -> Option<Homes> {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = var("USERPROFILE").or_else(|| var("HOME"))?;
        Some(Homes::with(&home, var("CLAUDE_CONFIG_DIR"), var("CODEX_HOME")))
    }

    /// Under `home`, with the agents' own folder variables when given.
    pub fn with(home: &Path, claude: Option<PathBuf>, codex: Option<PathBuf>) -> Homes {
        Homes {
            claude: claude.unwrap_or_else(|| home.join(".claude")),
            codex: codex.unwrap_or_else(|| home.join(".codex")),
            agent_skills: home.join(".agents").join("skills"),
            home: home.to_path_buf(),
        }
    }

    /// Where an agent's settings live.
    pub fn of(&self, agent: Agent) -> &Path {
        match agent {
            Agent::ClaudeCode => &self.claude,
            Agent::Codex => &self.codex,
        }
    }

    /// The file our hooks go in.
    pub fn hooks_file(&self, agent: Agent) -> PathBuf {
        match agent {
            Agent::ClaudeCode => self.claude.join("settings.json"),
            Agent::Codex => self.codex.join("hooks.json"),
        }
    }

    /// Our skill's folder.
    pub fn skill_dir(&self, agent: Agent) -> PathBuf {
        match agent {
            Agent::ClaudeCode => self.claude.join("skills").join(SKILL_DIR),
            Agent::Codex => self.agent_skills.join(SKILL_DIR),
        }
    }

    pub fn skill_file(&self, agent: Agent) -> PathBuf {
        self.skill_dir(agent).join("SKILL.md")
    }

    /// Codex's rules folder.
    pub fn rules_dir(&self) -> PathBuf {
        self.codex.join("rules")
    }

    pub fn rules_file(&self) -> PathBuf {
        self.rules_dir().join("keymotion.rules")
    }
}

/// Which keymotion-cli the hooks run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliKind {
    /// The Store copy, by its app execution alias.
    Store,
    /// The GitHub copy, in its installation's `bin` folder.
    Installed,
    /// A development build: the hooks stop working when it moves.
    Dev,
}

/// The keymotion-cli the hooks name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub path: PathBuf,
    pub kind: CliKind,
}

impl Cli {
    /// The stable path for a keymotion-cli running from `exe`: the Store
    /// copy (it runs from its versioned package folder) by its alias in
    /// `%LOCALAPPDATA%\Microsoft\WindowsApps`, never by that folder; a copy
    /// in a `bin` folder beside `keymotion.exe` by its own path; anything
    /// else by its own path, as a development build.
    pub fn for_exe(exe: &Path, packaged: bool, local_app_data: Option<&Path>) -> Cli {
        if packaged {
            if let Some(local) = local_app_data {
                let alias = local.join("Microsoft").join("WindowsApps").join("keymotion-cli.exe");
                return Cli { path: alias, kind: CliKind::Store };
            }
        }
        let installed = exe
            .parent()
            .filter(|bin| bin.file_name().is_some_and(|n| n.eq_ignore_ascii_case("bin")))
            .and_then(Path::parent)
            .is_some_and(|dir| dir.join("keymotion.exe").is_file());
        Cli { path: exe.to_path_buf(), kind: if installed { CliKind::Installed } else { CliKind::Dev } }
    }

    pub fn path_text(&self) -> String {
        self.path.display().to_string()
    }
}

/// Codex's line for its shell, PowerShell (Codex 0.166.0 runs every hook
/// with `powershell -NoProfile -Command`). Its final form, fixed now: Codex
/// asks the user to trust a hook again when its line changes.
///
/// It runs keymotion-cli only while it is there: after Keymotion was
/// uninstalled the line does nothing and ends with 0, instead of failing on
/// every event. By its full path: `if (Test-Path -LiteralPath '<path>') {
/// & '<path>' hook codex }`; by a bare name (on PATH): the same with
/// `Get-Command`. The path goes in single quotes, each of PowerShell's five
/// single-quote characters doubled ([`ps_quoted`]).
pub fn codex_command(cli: &Cli) -> String {
    let path = cli.path_text();
    let quoted = ps_quoted(&path);
    if cli.path.is_absolute() {
        format!("if (Test-Path -LiteralPath {quoted}) {{ & {quoted} hook codex }}")
    } else {
        format!("if (Get-Command {quoted} -ErrorAction SilentlyContinue) {{ & {quoted} hook codex }}")
    }
}

/// PowerShell's single-quote characters: it reads each of them as a single
/// quote (`'`, `‘`, `’`, `‚`, `‛`), so a user folder named O’Brien ends a
/// string quoted with `'`.
pub const PS_SINGLE_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

/// `text` as a PowerShell single-quoted string: inside one, a single-quote
/// character is written twice.
pub fn ps_quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        out.push(c);
        if PS_SINGLE_QUOTES.contains(&c) {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// The hook entry for `agent`, as it goes in the file.
pub fn hook_entry(agent: Agent, cli: &Cli, h: &OurHook) -> Json {
    let mut o = vec![("type".to_string(), Json::str("command"))];
    match agent {
        Agent::ClaudeCode => {
            o.push(("command".into(), Json::String(cli.path_text())));
            o.push(("args".into(), Json::Array(vec![Json::str("hook"), Json::str(agent.id())])));
        }
        Agent::Codex => o.push(("command".into(), Json::String(codex_command(cli)))),
    }
    if h.run_async {
        o.push(("async".into(), Json::Bool(true)));
    }
    o.push(("timeout".into(), Json::Number(h.timeout_s.into())));
    Json::Object(o)
}

/// How the hook line reads, for people.
pub fn command_line(agent: Agent, cli: &Cli) -> String {
    match agent {
        Agent::ClaudeCode => format!("\"{}\" hook claude-code", cli.path_text()),
        Agent::Codex => codex_command(cli),
    }
}

/// Our entries for `agent` that name another keymotion-cli than `cli`:
/// each of our hooks (its `command`), in every event; the skill, whose
/// full-path line names its keymotion-cli; the Codex rule, whose
/// `host_executable` does. Empty when all of them name `cli`, or there are
/// none. A file that cannot be read counts as naming another (it is left
/// alone); a skill or rule that names no keymotion-cli does not.
pub fn other_copies(agent: Agent, homes: &Homes, cli: &Cli) -> Vec<String> {
    let mut out = Vec::new();
    let path = cli.path_text().to_lowercase();
    let names_other = |named: Option<PathBuf>| named.is_some_and(|p| p.to_string_lossy().to_lowercase() != path);
    let hooks_file = homes.hooks_file(agent);
    match read_text(&hooks_file).and_then(|t| t.map(|t| parse_object(&hooks_file, Some(&t))).transpose()) {
        Ok(Some(root)) => {
            let groups = root.get("hooks").and_then(Json::as_object).into_iter().flatten();
            for (_, list) in groups {
                for group in list.as_array().into_iter().flatten() {
                    for handler in group.get("hooks").and_then(Json::as_array).into_iter().flatten() {
                        if !is_ours(agent, handler) {
                            continue;
                        }
                        let command = handler.get("command").and_then(Json::as_str).unwrap_or_default();
                        if !runs_cli(agent, command, cli) && !out.iter().any(|o| o == command) {
                            out.push(command.to_string());
                        }
                    }
                }
            }
        }
        Ok(None) => {}
        Err(_) => out.push(hooks_file.display().to_string()),
    }
    let skill_file = homes.skill_file(agent);
    match std::fs::read_to_string(&skill_file) {
        Ok(text) if skill_version(&text).is_some() && names_other(skill_cli_path(&text)) => {
            out.push(skill_file.display().to_string());
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => out.push(skill_file.display().to_string()),
    }
    if agent == Agent::Codex {
        let rules_file = homes.rules_file();
        match std::fs::read_to_string(&rules_file) {
            Ok(text) if marker_version(&text, RULES_MARKER).is_some() && names_other(rules_cli_path(&text)) => {
                out.push(rules_file.display().to_string());
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => out.push(rules_file.display().to_string()),
        }
    }
    out
}

/// Whether our hooks' program ([`Status::runs`]: a handler's `command`) is
/// `cli`: its path (Claude Code), or the line we write for it (Codex).
pub fn runs_cli(agent: Agent, runs: &str, cli: &Cli) -> bool {
    let runs = runs.trim();
    let path = cli.path_text();
    match agent {
        Agent::ClaudeCode => {
            // `<path>` with `args`, or an older `"<path>" hook claude-code`.
            let line = runs.to_lowercase();
            let line = line.strip_suffix(" hook claude-code").unwrap_or(&line);
            line.trim().trim_matches('"') == path.to_lowercase()
        }
        Agent::Codex => {
            runs.to_lowercase() == codex_command(cli).to_lowercase()
                || runs.to_lowercase().contains(&format!("'{}'", path.to_lowercase()))
        }
    }
}

/// Whether a hook handler is ours: keymotion-cli's hook mode for `agent`,
/// whichever copy it names.
pub fn is_ours(agent: Agent, handler: &Json) -> bool {
    let command = handler.get("command").and_then(Json::as_str).unwrap_or_default().to_ascii_lowercase();
    if !command.contains("keymotion-cli") {
        return false;
    }
    let args: Vec<&str> = handler
        .get("args")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(Json::as_str).collect())
        .unwrap_or_default();
    let tail = format!("hook {}", agent.id());
    // Any form we ever wrote: `<path> hook codex`, `& '<path>' hook codex`,
    // `if (…) { & '<path>' hook codex }`.
    let line = command.trim_end().trim_end_matches(['"', '\'', '}', ' ']);
    args == ["hook", agent.id()] || line.ends_with(&tail)
}

// ---------------------------------------------------------------- errors

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// A file is not what it should be (not JSON, not an object): left
    /// alone.
    Invalid,
    /// Reading or writing failed.
    Io,
    /// The agent installed here is too old for our hooks: nothing written.
    Unsupported,
    /// A file changed after the plan was made (the agent wrote its settings
    /// meanwhile): nothing was written; plan again from the file as it is.
    Changed,
    /// Writing failed part way and what was written could not all be put
    /// back: some of our entries stayed (disconnecting removes them).
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupError {
    pub kind: ErrorKind,
    pub message: String,
}

impl SetupError {
    fn invalid(path: &Path, why: impl std::fmt::Display) -> Self {
        SetupError {
            kind: ErrorKind::Invalid,
            message: format!("{} {why}; nothing was changed. Fix the file, then try again.", path.display()),
        }
    }

    fn io(path: &Path, e: std::io::Error) -> Self {
        SetupError { kind: ErrorKind::Io, message: format!("{}: {e}", path.display()) }
    }

    fn changed(path: &Path) -> Self {
        SetupError {
            kind: ErrorKind::Changed,
            message: format!("{} changed while Keymotion was getting ready; nothing was written.", path.display()),
        }
    }
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

// ----------------------------------------------------------------- plans

/// What happens to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub path: PathBuf,
    /// Its text now; `None` when it is not there.
    pub before: Option<String>,
    /// Its text after; `None` to remove it.
    pub after: Option<String>,
    /// For people: "add 16 hooks", "new skill".
    pub summary: String,
    /// Keep a copy before the first change (another program's file).
    pub backup: bool,
}

impl Edit {
    pub fn changes(&self) -> bool {
        self.before != self.after
    }
}

/// What connecting or disconnecting would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub agent: Agent,
    pub edits: Vec<Edit>,
    /// Folders to remove once empty (ours, made when connecting).
    pub remove_dirs: Vec<PathBuf>,
    /// The hook line, as people read it.
    pub command_line: Option<String>,
    pub warnings: Vec<String>,
}

impl Plan {
    /// The edits that change something.
    pub fn changes(&self) -> impl Iterator<Item = &Edit> {
        self.edits.iter().filter(|e| e.changes())
    }

    pub fn is_empty(&self) -> bool {
        self.changes().next().is_none()
    }

    /// As JSON, for `--json` and the app's page.
    pub fn to_value(&self) -> serde_json::Value {
        let changes: Vec<serde_json::Value> = self
            .changes()
            .map(|e| {
                let action = match (&e.before, &e.after) {
                    (None, Some(_)) => "create",
                    (Some(_), None) => "remove",
                    _ => "update",
                };
                json!({ "path": e.path.display().to_string(), "action": action, "summary": e.summary })
            })
            .collect();
        json!({
            "agent": self.agent.id(),
            "changes": changes,
            "command_line": self.command_line,
            "warnings": self.warnings,
        })
    }
}

/// What connecting made, kept beside the skill so disconnecting removes
/// only that.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Made {
    /// The hooks file itself (Codex) or its `hooks` object (Claude Code).
    file: bool,
    hooks: bool,
    events: Vec<String>,
    rules_dir: bool,
}

impl Made {
    fn read(text: Option<&str>) -> Option<Made> {
        let v: serde_json::Value = serde_json::from_str(text?).ok()?;
        let made = &v["made"];
        Some(Made {
            file: made["file"].as_bool().unwrap_or(false),
            hooks: made["hooks"].as_bool().unwrap_or(false),
            events: made["events"]
                .as_array()
                .map(|a| a.iter().filter_map(|e| e.as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
            rules_dir: made["rules_dir"].as_bool().unwrap_or(false),
        })
    }

    fn text(&self, agent: Agent) -> String {
        let v = json!({
            "about": "Written by Keymotion when it connected this agent: what it made, so that disconnecting removes only that.",
            "agent": agent.id(),
            "made": { "file": self.file, "hooks": self.hooks, "events": self.events, "rules_dir": self.rules_dir },
        });
        serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"
    }

    fn merge(&mut self, other: &Made) {
        self.file |= other.file;
        self.hooks |= other.hooks;
        self.rules_dir |= other.rules_dir;
        for e in &other.events {
            if !self.events.contains(e) {
                self.events.push(e.clone());
            }
        }
    }
}

fn read_text(path: &Path) -> Result<Option<String>, SetupError> {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| SetupError::invalid(path, "is not UTF-8 text")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(SetupError::io(path, e)),
    }
}

/// The settings file as an object; a missing or empty file is an empty one.
fn parse_object(path: &Path, text: Option<&str>) -> Result<Json, SetupError> {
    let Some(text) = text.filter(|t| !t.trim().is_empty()) else { return Ok(Json::object()) };
    let v = Json::parse(text).map_err(|e| SetupError::invalid(path, format!("is not valid JSON ({e})")))?;
    if v.as_object().is_none() {
        return Err(SetupError::invalid(path, "does not hold a JSON object"));
    }
    Ok(v)
}

/// The text to write back: indented like the file was, with its line
/// endings.
fn render(v: &Json, before: Option<&str>) -> String {
    let text = v.to_pretty(before);
    if before.is_some_and(|b| b.contains("\r\n")) {
        text.replace('\n', "\r\n")
    } else {
        text
    }
}

// ------------------------------------------------------------ the hooks

/// Our hooks merged into the `hooks` object of `root`: each of `ours` once,
/// in a group of its own at the end of its event (or where it already was),
/// in its current form; ours on other events (an older set) removed. What
/// was made, and how many of ours are there now.
fn merge_hooks(agent: Agent, path: &Path, root: &mut Json, cli: &Cli) -> Result<Made, SetupError> {
    let mut made = Made::default();
    if root.get("hooks").is_none() {
        root.set("hooks", Json::object());
        made.hooks = true;
    }
    let hooks = root
        .get_mut("hooks")
        .filter(|h| h.as_object().is_some())
        .ok_or_else(|| SetupError::invalid(path, "has a \"hooks\" entry that is not an object"))?;
    check_events(path, hooks)?;
    let wanted = our_hooks(agent);
    // Ours on events we no longer use.
    let names: Vec<String> = hooks.as_object().map(|o| o.iter().map(|(k, _)| k.clone()).collect()).unwrap_or_default();
    for name in names {
        if !wanted.iter().any(|h| h.event == name) {
            if let Some(groups) = hooks.get_mut(&name) {
                remove_ours(agent, groups);
            }
            if hooks.get(&name).and_then(Json::as_array).is_some_and(Vec::is_empty) {
                hooks.remove(&name);
            }
        }
    }
    for h in wanted {
        let entry = hook_entry(agent, cli, h);
        if hooks.get(h.event).is_none() {
            hooks.set(h.event, Json::Array(Vec::new()));
            made.events.push(h.event.to_string());
        }
        let groups = hooks.get_mut(h.event).and_then(Json::as_array_mut).expect("checked: an array");
        // Where ours is, in a group with our matcher.
        let mut kept = false;
        for group in groups.iter_mut() {
            let matcher = group.get("matcher").and_then(Json::as_str).filter(|m| !m.is_empty());
            let right_group = matcher == h.matcher;
            let Some(handlers) = group.get_mut("hooks").and_then(Json::as_array_mut) else { continue };
            handlers.retain_mut(|handler| {
                if !is_ours(agent, handler) {
                    return true;
                }
                if right_group && !kept {
                    kept = true;
                    if *handler != entry {
                        *handler = entry.clone();
                    }
                    return true;
                }
                false
            });
        }
        groups.retain(|g| g.get("hooks").and_then(Json::as_array).is_none_or(|a| !a.is_empty()));
        if !kept {
            let mut group = Vec::new();
            if let Some(m) = h.matcher {
                group.push(("matcher".to_string(), Json::str(m)));
            }
            group.push(("hooks".to_string(), Json::Array(vec![entry])));
            groups.push(Json::Object(group));
        }
    }
    Ok(made)
}

/// Every event's value is a list of groups, each with a list of handlers.
fn check_events(path: &Path, hooks: &Json) -> Result<(), SetupError> {
    for (name, groups) in hooks.as_object().into_iter().flatten() {
        let Some(groups) = groups.as_array() else {
            return Err(SetupError::invalid(path, format!("has hooks for {name} that are not a list")));
        };
        for g in groups {
            if g.as_object().is_none() || g.get("hooks").is_some_and(|h| h.as_array().is_none()) {
                return Err(SetupError::invalid(path, format!("has a hook group for {name} that is not as expected")));
            }
        }
    }
    Ok(())
}

/// Take our handlers out of an event's groups; groups left empty by it go.
/// How many went.
fn remove_ours(agent: Agent, groups: &mut Json) -> usize {
    let Some(list) = groups.as_array_mut() else { return 0 };
    let mut gone = 0;
    list.retain_mut(|group| {
        let Some(handlers) = group.get_mut("hooks").and_then(Json::as_array_mut) else { return true };
        let before = handlers.len();
        handlers.retain(|h| !is_ours(agent, h));
        let removed = before - handlers.len();
        gone += removed;
        !(removed > 0 && handlers.is_empty())
    });
    gone
}

/// Ours, out of the whole `hooks` object; event lists and the object itself
/// go when they are left empty and we made them (or, not knowing, when our
/// removal emptied them). How many went.
fn unmerge_hooks(agent: Agent, root: &mut Json, made: Option<&Made>) -> usize {
    let Some(hooks) = root.get_mut("hooks").filter(|h| h.as_object().is_some()) else { return 0 };
    let names: Vec<String> = hooks.as_object().map(|o| o.iter().map(|(k, _)| k.clone()).collect()).unwrap_or_default();
    let mut gone = 0;
    let mut emptied = 0;
    for name in names {
        let Some(groups) = hooks.get_mut(&name) else { continue };
        let removed = remove_ours(agent, groups);
        gone += removed;
        let empty = groups.as_array().is_some_and(Vec::is_empty);
        let ours = made.map_or(removed > 0, |m| m.events.contains(&name));
        if empty && ours {
            hooks.remove(&name);
            emptied += 1;
        }
    }
    let empty = hooks.as_object().is_some_and(Vec::is_empty);
    if empty && made.map_or(gone > 0 && emptied > 0, |m| m.hooks) {
        root.remove("hooks");
    }
    gone
}

/// Ours in `root` and whether they are as `cli` would write them.
fn find_ours(agent: Agent, root: &Json, cli: Option<&Cli>) -> (usize, bool, Option<String>) {
    let mut count = 0;
    let mut current = true;
    let mut path = None;
    let Some(hooks) = root.get("hooks").and_then(Json::as_object) else { return (0, false, None) };
    for h in our_hooks(agent) {
        let found =
            hooks.iter().filter(|(k, _)| k == h.event).flat_map(|(_, g)| g.as_array().into_iter().flatten()).find_map(
                |group| {
                    let handlers = group.get("hooks")?.as_array()?;
                    let handler = handlers.iter().find(|x| is_ours(agent, x))?;
                    Some((group.get("matcher").and_then(Json::as_str).filter(|m| !m.is_empty()) == h.matcher, handler))
                },
            );
        match found {
            Some((right_group, handler)) => {
                count += 1;
                if path.is_none() {
                    path = handler.get("command").and_then(Json::as_str).map(str::to_string);
                }
                if !right_group || cli.is_some_and(|c| *handler != hook_entry(agent, c, h)) {
                    current = false;
                }
            }
            None => current = false,
        }
    }
    (count, current, path)
}

// ------------------------------------------------------------ the skill

/// The skill, the same for both agents (the open Agent Skills format: a
/// name and a one- or two-line description up front; the rest is read only
/// when the agent uses it). Thin on purpose: what keymotion-cli can do is
/// read from `keymotion-cli guide`, which comes with every update.
pub fn skill_text(cli: &Cli) -> String {
    format!(
        "---
name: keymotion
description: Light the user's keyboard through Keymotion to tell them something (a finished task, a question, an error, progress, a word). Run `keymotion-cli guide` before the first use.
---

<!-- {SKILL_MARKER} {SKILL_VERSION}. Keymotion keeps this file up to date; changes made here do not last. -->

# The user's keyboard lights

This computer has `keymotion-cli`. It lights the keys of the user's keyboard
through the Keymotion app; the lights stay in the keyboard's working memory
and end by themselves.

- Before you use it the first time, run `keymotion-cli guide`: it lists this
  version's commands, values and exit codes. If `keymotion-cli` is not found,
  {SKILL_PATH_LEAD}{path}`
- You decide how to tell the user things. Keymotion already lights \"done\",
  \"waiting for you\" and errors by itself, from the coding app's own events;
  the ready signals (`keymotion-cli signal done`) are optional shortcuts. You
  may as well choose keys, colors, effects and timing yourself: a word on its
  letter keys, a count on the number keys, a progress bar on the F row, a
  timed scene of your own.
- You can write on the keyboard: `keymotion-cli spell \"DONE\"` lights the
  letters one after another.
- Put the command right after `keymotion-cli` (`keymotion-cli status --json`,
  not `keymotion-cli --json status`): Codex lets only that form reach the app.
- Exit code 77 means the user has not allowed it in Keymotion (agent access, or
  the permission a command needs): tell the user (Keymotion > Tools > AI
  agents) and do not retry.
",
        path = cli.path_text()
    )
}

/// Before the keymotion-cli path in the skill ([`skill_cli_path`]).
const SKILL_PATH_LEAD: &str = "run it by its full path: `";

/// The version in one of our skill files; `None` when the file is not ours.
pub fn skill_version(text: &str) -> Option<u32> {
    marker_version(text, SKILL_MARKER)
}

fn marker_version(text: &str, marker: &str) -> Option<u32> {
    let at = text.find(marker)? + marker.len();
    let digits: String = text[at..].trim_start().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The Codex rule (checked with `codex execpolicy check
/// --resolve-host-executables`, Codex 0.166.0): `keymotion-cli <command>`
/// by its name, or by the full path of this keymotion-cli, and no other
/// program of that name (a copy a repository brings along stays in the
/// sandbox: `host_executable`). Kept to what it must say: a mistake in any
/// rules file drops all of the user's rules too, so it carries no examples
/// that a later Codex could read differently.
pub fn rules_text(cli: &Cli) -> String {
    let allowed = ALLOWED_COMMANDS.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let path = starlark_string(&cli.path_text());
    format!(
        "# {RULES_MARKER} {RULES_VERSION}. Written by Keymotion; changes made here do not last.
# Lets the agent's own keymotion-cli calls (lights, status; the focus timer,
# profiles and screen cards, each still behind the user's permission in
# Keymotion) run outside Codex's sandbox, which cannot reach Keymotion's
# per-user pipe. Connecting or disconnecting a coding app is not among them:
# Codex still asks.
prefix_rule(
    pattern = [[\"keymotion-cli\", \"keymotion-cli.exe\"], [{allowed}]],
    decision = \"allow\",
    justification = \"Keymotion on the user's keyboard: keymotion-cli talks to the Keymotion app over a pipe the sandbox cannot reach.\",
)
host_executable(name = \"keymotion-cli\", paths = [{path}])
"
    )
}

/// `text` as a Starlark string literal.
fn starlark_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// keymotion-cli's commands the Codex rule lets out of the sandbox: lights,
/// the focus timer, profiles, screen cards, status and help (each still
/// needs the user's permission in Keymotion). Later steps add theirs here
/// (and [`RULES_VERSION`]; version 2 added focus, profile and card).
pub const ALLOWED_COMMANDS: [&str; 18] = [
    "status", "keys", "signal", "light", "progress", "clear", "log", "settings", "focus", "profile", "card", "scene",
    "pattern", "spell", "version", "guide", "doctor", "start",
];

// ------------------------------------------------------------- planning

/// What `connect` would change for `agent`, running `cli`.
pub fn plan_connect(agent: Agent, homes: &Homes, cli: &Cli) -> Result<Plan, SetupError> {
    let mut warnings = Vec::new();
    if agent == Agent::ClaudeCode {
        if let Some(old) = claude_code_too_old(homes) {
            return Err(SetupError { kind: ErrorKind::Unsupported, message: old });
        }
    }
    if cli.kind == CliKind::Dev {
        warnings.push(format!(
            "This is a development build of keymotion-cli: the hooks will run {}, and stop working when it moves.",
            cli.path_text()
        ));
    }
    let hooks_path = homes.hooks_file(agent);
    let state_path = homes.skill_dir(agent).join(STATE_FILE);
    let before = read_text(&hooks_path)?;
    let mut root = parse_object(&hooks_path, before.as_deref())?;
    if agent == Agent::Codex {
        check_codex_file(&hooks_path, &root)?;
    }
    let (had, _, _) = find_ours(agent, &root, None);
    let mut made = merge_hooks(agent, &hooks_path, &mut root, cli)?;
    made.file = before.is_none();
    let after = render(&root, before.as_deref());
    let count = our_hooks(agent).len();
    let summary = if had == 0 {
        format!("add {count} hooks that run keymotion-cli")
    } else {
        format!("bring Keymotion's {count} hooks up to date")
    };
    let backup = before.is_some();
    let mut edits =
        vec![Edit { path: hooks_path.clone(), before: before.clone(), after: Some(after.clone()), summary, backup }];
    if before.as_deref() == Some(after.as_str()) {
        edits[0].after = before.clone();
    }

    let skill_path = homes.skill_file(agent);
    let skill_before = read_text(&skill_path)?;
    if skill_before.as_deref().is_some_and(|t| skill_version(t).is_none()) {
        return Err(SetupError::invalid(&skill_path, "is a skill that is not Keymotion's, in Keymotion's folder"));
    }
    edits.push(Edit {
        path: skill_path,
        before: skill_before,
        after: Some(skill_text(cli)),
        summary: "the keymotion skill: \"run keymotion-cli guide first\"".into(),
        backup: false,
    });

    if agent == Agent::Codex {
        let rules = homes.rules_file();
        made.rules_dir = !homes.rules_dir().is_dir();
        let rules_before = read_text(&rules)?;
        if rules_before.as_deref().is_some_and(|t| marker_version(t, RULES_MARKER).is_none()) {
            return Err(SetupError::invalid(&rules, "is a rules file that is not Keymotion's"));
        }
        edits.push(Edit {
            path: rules.clone(),
            before: rules_before,
            after: Some(rules_text(cli)),
            summary: format!("a rule that lets keymotion-cli {} out of the sandbox", ALLOWED_COMMANDS.join(", ")),
            backup: false,
        });
        warnings.push(
            "Codex asks you to trust new hooks once: run /hooks in Codex, or open Settings > Hooks in the app.".into(),
        );
    }

    // What was made before stays recorded.
    let state_before = read_text(&state_path)?;
    if let Some(old) = Made::read(state_before.as_deref()) {
        let mut all = old;
        all.merge(&Made { file: false, ..made.clone() });
        made = all;
    }
    edits.push(Edit {
        path: state_path,
        before: state_before,
        after: Some(made.text(agent)),
        summary: "what Keymotion made, for disconnecting".into(),
        backup: false,
    });
    Ok(Plan { agent, edits, remove_dirs: Vec::new(), command_line: Some(command_line(agent, cli)), warnings })
}

/// Codex reads a `hooks.json` with only `description` and `hooks` at its
/// top: one with anything else does not load at all.
fn check_codex_file(path: &Path, root: &Json) -> Result<(), SetupError> {
    match root.as_object().into_iter().flatten().find(|(k, _)| k != "description" && k != "hooks") {
        Some((k, _)) => Err(SetupError::invalid(path, format!("has \"{k}\", which Codex does not read in this file"))),
        None => Ok(()),
    }
}

/// What `disconnect` would change for `agent`.
pub fn plan_disconnect(agent: Agent, homes: &Homes) -> Result<Plan, SetupError> {
    let hooks_path = homes.hooks_file(agent);
    let skill_dir = homes.skill_dir(agent);
    let state_path = skill_dir.join(STATE_FILE);
    let state_before = read_text(&state_path)?;
    let made = Made::read(state_before.as_deref());
    let mut edits = Vec::new();
    let before = read_text(&hooks_path)?;
    if before.is_some() {
        let mut root = parse_object(&hooks_path, before.as_deref())?;
        let gone = unmerge_hooks(agent, &mut root, made.as_ref());
        let empty = root.as_object().is_some_and(Vec::is_empty)
            || (agent == Agent::Codex && root.get("hooks").and_then(Json::as_object).is_none_or(Vec::is_empty));
        let after = if gone == 0 {
            before.clone()
        } else if empty && made.as_ref().is_some_and(|m| m.file) {
            None
        } else {
            if agent == Agent::Codex && root.get("hooks").is_none() {
                root.set("hooks", Json::object());
            }
            Some(render(&root, before.as_deref()))
        };
        edits.push(Edit {
            path: hooks_path,
            before: before.clone(),
            after,
            summary: format!("remove Keymotion's {gone} hooks"),
            backup: true,
        });
    }
    let skill_path = skill_dir.join("SKILL.md");
    if let Some(text) = read_text(&skill_path)? {
        if skill_version(&text).is_some() {
            edits.push(Edit {
                path: skill_path,
                before: Some(text),
                after: None,
                summary: "remove the keymotion skill".into(),
                backup: false,
            });
        }
    }
    if state_before.is_some() {
        edits.push(Edit {
            path: state_path,
            before: state_before,
            after: None,
            summary: "remove what Keymotion noted when it connected".into(),
            backup: false,
        });
    }
    let stopped = skill_dir.join(STOPPED_FILE);
    if let Some(text) = read_text(&stopped)? {
        edits.push(Edit {
            path: stopped,
            before: Some(text),
            after: None,
            summary: "remove the note that Keymotion was closed".into(),
            backup: false,
        });
    }
    let mut remove_dirs = vec![skill_dir];
    if agent == Agent::Codex {
        let rules = homes.rules_file();
        if let Some(text) = read_text(&rules)? {
            if marker_version(&text, RULES_MARKER).is_some() {
                edits.push(Edit {
                    path: rules,
                    before: Some(text),
                    after: None,
                    summary: "remove the keymotion-cli rule".into(),
                    backup: false,
                });
            }
        }
        if made.as_ref().is_some_and(|m| m.rules_dir) {
            remove_dirs.push(homes.rules_dir());
        }
    }
    Ok(Plan { agent, edits, remove_dirs, command_line: None, warnings: Vec::new() })
}

// ------------------------------------------------------------- applying

/// Carry `plan` out. Every file is read again first: if one is not as the
/// plan found it (the agent wrote its settings meanwhile), nothing is
/// written and the error is [`ErrorKind::Changed`]: plan again. Then each
/// changed file is written whole through a temporary file beside it (a
/// fresh copy of another program's file kept first, `<file>.before-
/// keymotion`), removed files are removed, then our emptied folders.
///
/// All or nothing: when a file cannot be written, the files written before
/// it are put back as they were (ours removed again), so an agent is never
/// left half connected. Should putting back fail too, the error is
/// [`ErrorKind::Partial`]: some of ours stayed, and disconnecting removes
/// them (it finds our entries without the note of what connecting made).
pub fn apply(plan: &Plan) -> Result<(), SetupError> {
    for edit in plan.changes() {
        if read_text(&edit.path)? != edit.before {
            return Err(SetupError::changed(&edit.path));
        }
    }
    let mut done: Vec<&Edit> = Vec::new();
    for edit in plan.changes() {
        if let Err(e) = apply_one(edit) {
            let stuck = roll_back(&done);
            if stuck.is_empty() {
                return Err(e);
            }
            let names: Vec<String> = stuck.iter().map(|p| p.display().to_string()).collect();
            return Err(SetupError {
                kind: ErrorKind::Partial,
                message: format!(
                    "{} Some of Keymotion's entries stayed in {}; disconnecting removes them.",
                    e.message,
                    names.join(", ")
                ),
            });
        }
        done.push(edit);
    }
    for dir in &plan.remove_dirs {
        // Only when empty: a file the user put there stays, and its folder.
        let _ = std::fs::remove_dir(dir);
    }
    Ok(())
}

fn apply_one(edit: &Edit) -> Result<(), SetupError> {
    match &edit.after {
        Some(text) => {
            if let Some(dir) = edit.path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| SetupError::io(dir, e))?;
            }
            if let (true, Some(before)) = (edit.backup, &edit.before) {
                let backup = with_suffix(&edit.path, BACKUP_SUFFIX);
                write_atomic(&backup, before)?;
            }
            write_atomic(&edit.path, text)
        }
        None => match std::fs::remove_file(&edit.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(SetupError::io(&edit.path, e)),
        },
    }
}

/// Put `done` back as it was, newest first: a file that was there gets its
/// old text, one we made goes (and its folder, when that leaves it empty).
/// The files that could not be put back.
fn roll_back(done: &[&Edit]) -> Vec<PathBuf> {
    let mut stuck = Vec::new();
    for edit in done.iter().rev() {
        let back = match &edit.before {
            Some(text) => write_atomic(&edit.path, text).is_ok(),
            None => match std::fs::remove_file(&edit.path) {
                Ok(()) => {
                    if let Some(dir) = edit.path.parent() {
                        let _ = std::fs::remove_dir(dir);
                    }
                    true
                }
                Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            },
        };
        if !back {
            stuck.push(edit.path.clone());
        }
    }
    stuck
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Write `text` to `path` whole: a temporary file beside it, flushed to
/// the disk, then a rename over it, tried again for a moment while another
/// program (the agent itself, watching its settings) holds the file open.
/// A link (a dotfiles setup) is written through: the file it points to is
/// replaced, the link stays.
fn write_atomic(path: &Path, text: &str) -> Result<(), SetupError> {
    let is_link = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    let target =
        if is_link { std::fs::canonicalize(path).map_err(|e| SetupError::io(path, e))? } else { path.to_path_buf() };
    let tmp = with_suffix(&target, &format!(".keymotion-{}.tmp", std::process::id()));
    let written = std::fs::File::create(&tmp).and_then(|mut f| {
        use std::io::Write;
        f.write_all(text.as_bytes())?;
        f.sync_all()
    });
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(SetupError::io(&tmp, e));
    }
    let mut last = None;
    for _ in 0..20 {
        match std::fs::rename(&tmp, &target) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(SetupError::io(path, last.unwrap_or_else(|| std::io::Error::other("not written"))))
}

/// Connect `agent`: plan, then apply. The plan, done.
pub fn connect(agent: Agent, homes: &Homes, cli: &Cli) -> Result<Plan, SetupError> {
    let plan = plan_connect(agent, homes, cli)?;
    apply(&plan)?;
    Ok(plan)
}

pub fn disconnect(agent: Agent, homes: &Homes) -> Result<Plan, SetupError> {
    let plan = plan_disconnect(agent, homes)?;
    apply(&plan)?;
    Ok(plan)
}

// --------------------------------------------------------------- status

/// How one agent stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub agent: Agent,
    /// Its settings folder is there (the agent is installed for this user).
    pub found: bool,
    /// `not_found`, `not_connected`, `connected`, `needs_approval` (Codex:
    /// the user has not trusted our hooks yet), `outdated` (some of ours
    /// are missing or name another keymotion-cli), `unreadable` (the hooks
    /// file does not read).
    pub state: &'static str,
    /// Ours in the hooks file, of how many.
    pub hooks: usize,
    pub hooks_wanted: usize,
    /// The keymotion-cli our hooks name.
    pub runs: Option<String>,
    /// Our skill's version, when it is there.
    pub skill: Option<u32>,
    /// Codex: our rule's version, when it is there.
    pub rules: Option<u32>,
    /// Codex: of ours, how many the user trusted (as they are now).
    pub trusted: Option<usize>,
    pub hooks_file: PathBuf,
    pub skill_file: PathBuf,
    pub error: Option<String>,
    /// Something to fix that is not an error of ours: a Claude Code too old
    /// for the hooks.
    pub warning: Option<String>,
}

impl Status {
    pub fn to_value(&self) -> serde_json::Value {
        json!({
            "agent": self.agent.id(),
            "name": self.agent.title(),
            "found": self.found,
            "state": self.state,
            "hooks": self.hooks,
            "hooks_wanted": self.hooks_wanted,
            "runs": self.runs,
            "skill": self.skill,
            "rules": self.rules,
            "trusted": self.trusted,
            "hooks_file": self.hooks_file.display().to_string(),
            "skill_file": self.skill_file.display().to_string(),
            "error": self.error,
            "warning": self.warning,
        })
    }
}

/// How `agent` stands; `cli` (this keymotion-cli) to tell whether our
/// hooks name it.
pub fn status(agent: Agent, homes: &Homes, cli: Option<&Cli>) -> Status {
    let hooks_file = homes.hooks_file(agent);
    let skill_file = homes.skill_file(agent);
    let found = homes.of(agent).is_dir();
    let skill = std::fs::read_to_string(&skill_file).ok().as_deref().and_then(skill_version);
    let rules = (agent == Agent::Codex)
        .then(|| {
            std::fs::read_to_string(homes.rules_file()).ok().as_deref().and_then(|t| marker_version(t, RULES_MARKER))
        })
        .flatten();
    let wanted = our_hooks(agent).len();
    let mut s = Status {
        agent,
        found,
        state: "not_found",
        hooks: 0,
        hooks_wanted: wanted,
        runs: None,
        skill,
        rules,
        trusted: None,
        hooks_file: hooks_file.clone(),
        skill_file,
        error: None,
        warning: if agent == Agent::ClaudeCode { claude_code_too_old(homes) } else { None },
    };
    let text = match read_text(&hooks_file) {
        Ok(t) => t,
        Err(e) => {
            s.state = "unreadable";
            s.error = Some(e.message);
            return s;
        }
    };
    let root = match parse_object(&hooks_file, text.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            s.state = "unreadable";
            s.error = Some(e.message);
            return s;
        }
    };
    let shape = match root.get("hooks") {
        None => Ok(()),
        Some(h) if h.as_object().is_some() => check_events(&hooks_file, h),
        Some(_) => Err(SetupError::invalid(&hooks_file, "has a \"hooks\" entry that is not an object")),
    };
    if let Err(e) = shape {
        s.state = "unreadable";
        s.error = Some(e.message);
        return s;
    }
    let (count, current, runs) = find_ours(agent, &root, cli);
    s.hooks = count;
    s.runs = runs;
    s.state = if count == 0 {
        if found {
            "not_connected"
        } else {
            "not_found"
        }
    } else if !current || skill.is_none() || (agent == Agent::Codex && rules.is_none()) {
        "outdated"
    } else {
        "connected"
    };
    if agent == Agent::Codex && count > 0 {
        let config = std::fs::read_to_string(homes.codex.join("config.toml")).unwrap_or_default();
        let trusted = codex_trusted(&hooks_file, &root, &config);
        s.trusted = Some(trusted);
        if s.state == "connected" && trusted < count {
            s.state = "needs_approval";
        }
    }
    s
}

// ------------------------------------------------- Claude Code's version

/// The first Claude Code that runs a hook's program with its `args`, no
/// shell (its changelog, 2.1.139: "Added hook `args: string[]` field (exec
/// form)"). An older one would run the bare path through a shell; before
/// 2.1.101 an event name it did not know made it ignore the whole settings
/// file.
pub const CLAUDE_CODE_MIN: [u16; 3] = [2, 1, 139];

/// The Claude Code copies found here, without starting any: the native
/// installer's `claude.exe` (its file version) and npm's package. The
/// desktop app's own copy is not looked at.
pub fn claude_code_versions(homes: &Homes) -> Vec<(PathBuf, [u16; 3])> {
    let mut found = Vec::new();
    let native = homes.home.join(r".local\bin\claude.exe");
    #[cfg(windows)]
    if let Some(v) = kb_winsys::os::file_version(&native) {
        found.push((native, [v[0], v[1], v[2]]));
    }
    #[cfg(not(windows))]
    let _ = native;
    let npm = homes.home.join(r"AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\package.json");
    if let Ok(text) = std::fs::read_to_string(&npm) {
        let version = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v["version"].as_str().and_then(kb_agent::version_triple));
        if let Some(v) = version {
            found.push((npm, v));
        }
    }
    found
}

/// What to say when a Claude Code older than [`CLAUDE_CODE_MIN`] is here.
pub fn claude_code_too_old(homes: &Homes) -> Option<String> {
    old_claude_code(&claude_code_versions(homes))
}

fn old_claude_code(found: &[(PathBuf, [u16; 3])]) -> Option<String> {
    let dotted = |v: &[u16; 3]| v.map(|p| p.to_string()).join(".");
    let (path, v) = found.iter().find(|(_, v)| *v < CLAUDE_CODE_MIN)?;
    Some(format!(
        "Claude Code {} is installed ({}); Keymotion's hooks need {} or newer. Update Claude Code (claude update), then connect again.",
        dotted(v),
        path.display(),
        dotted(&CLAUDE_CODE_MIN)
    ))
}

// ---------------------------------------------------------- Codex trust

/// Codex's name for an event in its trust keys
/// (`codex-rs/hooks/src/lib.rs`, `hook_event_key_label`).
fn codex_event_label(event: &str) -> String {
    let mut out = String::new();
    for (i, c) in event.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The hash Codex keeps for a trusted hook (`codex-rs/hooks/src/engine/
/// discovery.rs`, `hook_hash`, at 0.166.0): SHA-256 of the handler's
/// identity as compact JSON with sorted keys. Our handlers set no matcher
/// (their events take none), no status message and no context limit; the
/// time limit as Codex reads it.
pub fn codex_hook_hash(event: &str, command: &str, run_async: bool, timeout_s: u32) -> String {
    let timeout = match event {
        "SessionEnd" | "Interrupt" => timeout_s.clamp(1, 3),
        _ => timeout_s.max(1),
    };
    let identity = json!({
        "event_name": codex_event_label(event),
        "hooks": [{ "async": run_async, "command": command, "timeout": timeout, "type": "command" }],
    });
    // serde_json's maps are sorted (no preserve_order).
    let text = serde_json::to_string(&identity).unwrap_or_default();
    let raw = sha2_hex(text.as_bytes());
    format!("sha256:{raw}")
}

fn sha2_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A path as Codex's keys and ours compare: no `\\?\`, one kind of slash,
/// any case.
fn key_path(p: &str) -> String {
    p.trim_start_matches(r"\\?\").replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// How many of our handlers in `root` (the hooks file at `path`) Codex's
/// `config.toml` says the user trusted, as they are now.
fn codex_trusted(path: &Path, root: &Json, config: &str) -> usize {
    let Ok(table) = config.parse::<toml::Table>() else { return 0 };
    let Some(state) = table.get("hooks").and_then(|h| h.get("state")).and_then(|s| s.as_table()) else { return 0 };
    let file = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let ours_path = key_path(&file.display().to_string());
    let plain_path = key_path(&path.display().to_string());
    let mut trusted = 0;
    for (event, groups) in root.get("hooks").and_then(Json::as_object).into_iter().flatten() {
        for (g, group) in groups.as_array().into_iter().flatten().enumerate() {
            for (h, handler) in group.get("hooks").and_then(Json::as_array).into_iter().flatten().enumerate() {
                if !is_ours(Agent::Codex, handler) {
                    continue;
                }
                let command = handler.get("command").and_then(Json::as_str).unwrap_or_default();
                let run_async = matches!(handler.get("async"), Some(Json::Bool(true)));
                let timeout = match handler.get("timeout") {
                    Some(Json::Number(n)) => n.as_u64().map_or(600, |t| u32::try_from(t).unwrap_or(u32::MAX)),
                    _ => default_codex_timeout(event),
                };
                let hash = codex_hook_hash(event, command, run_async, timeout);
                let suffix = format!(":{}:{g}:{h}", codex_event_label(event));
                let ok = state.iter().any(|(key, v)| {
                    let Some(key_file) = key.strip_suffix(&suffix) else { return false };
                    let kp = key_path(key_file);
                    (kp == ours_path || kp == plain_path)
                        && v.get("enabled").and_then(|e| e.as_bool()) != Some(false)
                        && v.get("trusted_hash").and_then(|t| t.as_str()) == Some(hash.as_str())
                });
                if ok {
                    trusted += 1;
                }
            }
        }
    }
    trusted
}

fn default_codex_timeout(event: &str) -> u32 {
    match event {
        "SessionEnd" | "Interrupt" => 1,
        _ => 600,
    }
}

// -------------------------------------------------------------- refresh

/// At the app's start: our skill files and our Codex rule, where they are
/// (only connected agents have them) and older than this version's, are
/// written anew for `cli`. Only an older version counts: a copy that merely
/// names another keymotion-cli was written by another connection (the
/// Store copy's, say, while a development build runs), and connecting again
/// is what moves it. Our files only, never the agents' settings. The files
/// rewritten.
pub fn refresh(homes: &Homes) -> Vec<PathBuf> {
    let mut done = Vec::new();
    for agent in Agent::ALL {
        let path = homes.skill_file(agent);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        if !skill_version(&text).is_some_and(|v| v < SKILL_VERSION) {
            continue;
        }
        // The keymotion-cli the file names stays: the copy that connected.
        let Some(cli) = skill_cli_path(&text) else { continue };
        if write_atomic(&path, &skill_text(&Cli { path: cli, kind: CliKind::Installed })).is_ok() {
            done.push(path);
        }
    }
    let rules = homes.rules_file();
    if let Ok(text) = std::fs::read_to_string(&rules) {
        let outdated = marker_version(&text, RULES_MARKER).is_some_and(|v| v < RULES_VERSION);
        if let (true, Some(cli)) = (outdated, rules_cli_path(&text)) {
            if write_atomic(&rules, &rules_text(&Cli { path: cli, kind: CliKind::Installed })).is_ok() {
                done.push(rules);
            }
        }
    }
    done
}

/// The keymotion-cli one of our skill files names.
pub fn skill_cli_path(text: &str) -> Option<PathBuf> {
    let at = text.find(SKILL_PATH_LEAD)? + SKILL_PATH_LEAD.len();
    let path = &text[at..text[at..].find('`')? + at];
    (!path.trim().is_empty()).then(|| PathBuf::from(path))
}

/// The keymotion-cli our Codex rule names (its `host_executable` path).
pub fn rules_cli_path(text: &str) -> Option<PathBuf> {
    let lead = "paths = [\"";
    let at = text.find(lead)? + lead.len();
    let mut out = String::new();
    let mut chars = text[at..].chars();
    loop {
        match chars.next()? {
            '"' => break,
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                c => out.push(c),
            },
            c => out.push(c),
        }
    }
    (!out.is_empty()).then(|| PathBuf::from(out))
}

// ------------------------------------------------- "Keymotion was closed"

/// Beside our skill: Keymotion was closed by the user, or agent access is
/// off. Hook events then do not start Keymotion (the owner's decision,
/// 2026-11-23: "if I closed it, it stays closed"); the agent's own commands
/// still do.
///
/// Why there: the hook may run inside another app's package box (the
/// Claude and ChatGPT desktop apps are MSIX packages), and so may the Store
/// copy of Keymotion. Inside a box, AppData and HKCU writes go to that
/// package's own folder: a note the Store copy left in AppData would never
/// reach a hook, and one a boxed hook wrote would never leave its box. The
/// rest of the user's folder is shared by all (measured 2026-11-23 from the
/// Claude desktop app's box: its writes to `%USERPROFILE%\.x` were seen
/// outside, its writes to `%LOCALAPPDATA%` were not). Our skill folders are
/// there, and exist exactly while an agent is connected.
pub const STOPPED_FILE: &str = "stopped.json";

/// Why hooks should not start Keymotion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// "Quit" in the tray menu: until Keymotion runs again, or the user
    /// signs in again (Windows restarted, signed out).
    Quit,
    /// Agent access is off: until it is on.
    AccessOff,
}

impl Stopped {
    fn as_str(self) -> &'static str {
        match self {
            Stopped::Quit => "quit",
            Stopped::AccessOff => "access_off",
        }
    }
}

/// The skill folders of the connected agents (ours, with our skill in it).
fn connected_dirs(homes: &Homes) -> Vec<PathBuf> {
    Agent::ALL
        .iter()
        .map(|a| homes.skill_dir(*a))
        .filter(|d| std::fs::read_to_string(d.join("SKILL.md")).ok().as_deref().and_then(skill_version).is_some())
        .collect()
}

/// Note that hooks should not start Keymotion, in every connected agent's
/// skill folder. `logon` is the sign-in's id (`kb_winsys::pipe::
/// current_logon_id`): a quit counts for that sign-in only. The files
/// written.
pub fn mark_stopped(homes: &Homes, why: Stopped, logon: Option<u64>) -> Vec<PathBuf> {
    let text = serde_json::to_string_pretty(&json!({
        "about": "Written by Keymotion: hook events do not start Keymotion while this file is here.",
        "reason": why.as_str(),
        "logon": logon.map(|l| format!("{l:x}")),
    }))
    .unwrap_or_default()
        + "\n";
    let mut done = Vec::new();
    for dir in connected_dirs(homes) {
        let path = dir.join(STOPPED_FILE);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) && write_atomic(&path, &text).is_ok() {
            done.push(path);
        }
    }
    done
}

/// Hooks may start Keymotion again. The files removed.
pub fn clear_stopped(homes: &Homes) -> Vec<PathBuf> {
    Agent::ALL
        .iter()
        .map(|a| homes.skill_dir(*a).join(STOPPED_FILE))
        .filter(|p| std::fs::remove_file(p).is_ok())
        .collect()
}

/// Whether `agent`'s hooks should leave Keymotion closed: access is off, or
/// the user quit it in this sign-in (`logon`). A missing or unreadable note
/// means no: Keymotion is just not running (a restart, never started).
pub fn hooks_stopped(homes: &Homes, agent: Agent, logon: Option<u64>) -> bool {
    let Ok(text) = std::fs::read_to_string(homes.skill_dir(agent).join(STOPPED_FILE)) else { return false };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return false };
    match v["reason"].as_str() {
        Some("access_off") => true,
        Some("quit") => {
            let noted = v["logon"].as_str().and_then(|l| u64::from_str_radix(l, 16).ok());
            noted.is_some() && noted == logon
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fresh folder of our own under the system's temp folder, its name
    /// with a space in it.
    fn temp_home() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kb-hooks test {}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cli(home: &Path) -> Cli {
        Cli {
            path: home.join("Program Files").join("Key Motion").join("bin").join("keymotion-cli.exe"),
            kind: CliKind::Installed,
        }
    }

    fn read(p: &Path) -> String {
        std::fs::read_to_string(p).unwrap()
    }

    const USERS_OWN: &str = r#"{
    "model": "opus",
    "hooks": {
        "Stop": [
            {
                "hooks": [
                    {
                        "type": "command",
                        "command": "say done"
                    }
                ]
            }
        ],
        "PreToolUse": [
            {
                "matcher": "Bash",
                "hooks": [
                    {
                        "type": "command",
                        "command": "check.sh"
                    }
                ]
            }
        ]
    },
    "permissions": {
        "allow": []
    }
}
"#;

    #[test]
    fn the_program_our_hooks_run_is_told_apart_from_another_copy() {
        let mine = Cli {
            path: PathBuf::from(r"C:\Users\A B\AppData\Local\Keymotion\bin\keymotion-cli.exe"),
            kind: CliKind::Installed,
        };
        let store = Cli {
            path: PathBuf::from(r"C:\Users\A B\AppData\Local\Microsoft\WindowsApps\keymotion-cli.exe"),
            kind: CliKind::Store,
        };
        for agent in Agent::ALL {
            let line = |cli: &Cli| match agent {
                Agent::ClaudeCode => cli.path_text(),
                Agent::Codex => codex_command(cli),
            };
            assert!(runs_cli(agent, &line(&mine), &mine), "{agent:?}");
            assert!(runs_cli(agent, &line(&mine).to_uppercase(), &mine), "{agent:?}: case");
            assert!(!runs_cli(agent, &line(&store), &mine), "{agent:?}");
        }
        // An older Claude Code line, the program and its arguments in one.
        assert!(runs_cli(Agent::ClaudeCode, &format!("\"{}\" hook claude-code", mine.path_text()), &mine));
        // A folder whose name only starts the same.
        let longer = Cli {
            path: PathBuf::from(r"C:\Users\A B\AppData\Local\Keymotion\bin2\keymotion-cli.exe"),
            kind: CliKind::Dev,
        };
        assert!(!runs_cli(Agent::Codex, &codex_command(&longer), &mine));
    }

    #[test]
    fn a_write_that_fails_part_way_puts_back_what_was_written() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        let settings = homes.hooks_file(Agent::ClaudeCode);
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(&settings, USERS_OWN).unwrap();
        // The skill's folder cannot be made: a file stands where it goes.
        std::fs::create_dir_all(homes.claude.join("skills")).unwrap();
        std::fs::write(homes.skill_dir(Agent::ClaudeCode), "in the way").unwrap();
        let plan = plan_connect(Agent::ClaudeCode, &homes, &cli(&home)).unwrap();
        let e = apply(&plan).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Io, "{e:?}");
        assert_eq!(read(&settings), USERS_OWN, "the hooks went again with the rest");
        assert_eq!(status(Agent::ClaudeCode, &homes, None).hooks, 0);

        // Codex: the hooks file it made goes again too.
        let hooks = homes.hooks_file(Agent::Codex);
        std::fs::create_dir_all(homes.codex.join("rules")).unwrap();
        std::fs::create_dir_all(&homes.agent_skills).unwrap();
        std::fs::write(homes.skill_dir(Agent::Codex), "in the way").unwrap();
        let plan = plan_connect(Agent::Codex, &homes, &cli(&home)).unwrap();
        assert!(apply(&plan).is_err());
        assert!(!hooks.exists(), "made by the failed connect, gone again");
        assert!(!homes.rules_file().exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn claude_code_connects_beside_the_users_own_hooks_and_disconnects_cleanly() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        std::fs::create_dir_all(&homes.claude).unwrap();
        let settings = homes.hooks_file(Agent::ClaudeCode);
        std::fs::write(&settings, USERS_OWN).unwrap();
        let cli = cli(&home);

        let plan = connect(Agent::ClaudeCode, &homes, &cli).unwrap();
        assert!(!plan.is_empty());
        let text = read(&settings);
        let v = Json::parse(&text).unwrap();
        // The user's keys and hooks stay, in their order, four spaces wide.
        let keys: Vec<&str> = v.as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["model", "hooks", "permissions"]);
        assert!(text.contains("\n    \"model\": \"opus\""), "{text}");
        let hooks = v.get("hooks").unwrap();
        let stop = hooks.get("Stop").unwrap().as_array().unwrap();
        assert_eq!(stop.len(), 2, "the user's group first, ours after");
        assert_eq!(
            stop[0].get("hooks").unwrap().as_array().unwrap()[0].get("command").unwrap().as_str(),
            Some("say done")
        );
        let ours = &stop[1].get("hooks").unwrap().as_array().unwrap()[0];
        assert_eq!(ours.get("command").and_then(Json::as_str), Some(cli.path_text().as_str()));
        assert_eq!(serde_json::to_string(ours.get("args").unwrap()).unwrap(), r#"["hook","claude-code"]"#);
        let pre = hooks.get("PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(pre[1].get("matcher").and_then(Json::as_str), Some(CLAUDE_ASKING_MATCHER));
        for h in CLAUDE_HOOKS {
            assert!(hooks.get(h.event).is_some(), "{}", h.event);
        }
        // A copy of the file as it was, once; the skill and the record.
        assert_eq!(read(&with_suffix(&settings, BACKUP_SUFFIX)), USERS_OWN);
        let skill = read(&homes.skill_file(Agent::ClaudeCode));
        assert_eq!(skill_version(&skill), Some(SKILL_VERSION));
        assert!(skill.starts_with("---\nname: keymotion\ndescription: "));
        assert!(skill.contains(&cli.path_text()));
        let st = status(Agent::ClaudeCode, &homes, Some(&cli));
        assert_eq!((st.state, st.hooks, st.skill), ("connected", CLAUDE_HOOKS.len(), Some(SKILL_VERSION)));

        // Again: nothing changes, not even the file's time.
        let again = plan_connect(Agent::ClaudeCode, &homes, &cli).unwrap();
        assert!(again.is_empty(), "{:?}", again.changes().map(|e| &e.path).collect::<Vec<_>>());
        connect(Agent::ClaudeCode, &homes, &cli).unwrap();
        assert_eq!(read(&settings), text);

        // Another keymotion-cli: ours updated in place, not added twice.
        let moved = Cli { path: home.join("elsewhere").join("keymotion-cli.exe"), kind: CliKind::Dev };
        assert_eq!(status(Agent::ClaudeCode, &homes, Some(&moved)).state, "outdated");
        let plan = connect(Agent::ClaudeCode, &homes, &moved).unwrap();
        assert!(plan.warnings.iter().any(|w| w.contains("development build")));
        let v = Json::parse(&read(&settings)).unwrap();
        assert_eq!(v.get("hooks").unwrap().get("Stop").unwrap().as_array().unwrap().len(), 2);
        assert!(read(&settings).contains("elsewhere"));
        assert_eq!(read(&with_suffix(&settings, BACKUP_SUFFIX)), text, "a fresh copy before each write");

        // Disconnect: the file as the user had it, the skill folder gone.
        disconnect(Agent::ClaudeCode, &homes).unwrap();
        assert_eq!(read(&settings), USERS_OWN);
        assert!(!homes.skill_dir(Agent::ClaudeCode).exists());
        assert_eq!(status(Agent::ClaudeCode, &homes, Some(&cli)).state, "not_connected");
        // Twice is fine.
        assert!(plan_disconnect(Agent::ClaudeCode, &homes).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_new_settings_file_is_made_and_taken_away_again() {
        let home = temp_home();
        let homes = Homes::with(&home, Some(home.join("custom claude")), None);
        let cli = cli(&home);
        connect(Agent::ClaudeCode, &homes, &cli).unwrap();
        let settings = home.join("custom claude").join("settings.json");
        assert!(settings.is_file(), "CLAUDE_CONFIG_DIR is honoured");
        assert!(!with_suffix(&settings, BACKUP_SUFFIX).exists(), "nothing to keep a copy of");
        disconnect(Agent::ClaudeCode, &homes).unwrap();
        assert!(!settings.exists(), "we made the file, and nothing else is in it");

        // An empty list the user had stays, an emptied one of ours goes.
        std::fs::write(&settings, "{\n  \"hooks\": {\n    \"Stop\": []\n  }\n}\n").unwrap();
        connect(Agent::ClaudeCode, &homes, &cli).unwrap();
        disconnect(Agent::ClaudeCode, &homes).unwrap();
        assert_eq!(read(&settings), "{\n  \"hooks\": {\n    \"Stop\": []\n  }\n}\n");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_file_that_does_not_read_is_left_alone() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        std::fs::create_dir_all(&homes.claude).unwrap();
        let settings = homes.hooks_file(Agent::ClaudeCode);
        for bad in [
            "{ \"a\": 1, }",
            "// comment\n{}",
            "[1, 2]",
            "{\"hooks\": []}",
            "{\"hooks\": {\"Stop\": {}}}",
            "{\"hooks\": {\"Stop\": [{\"hooks\": 5}]}}",
        ] {
            std::fs::write(&settings, bad).unwrap();
            let e = connect(Agent::ClaudeCode, &homes, &cli(&home)).unwrap_err();
            assert_eq!(e.kind, ErrorKind::Invalid, "{bad}");
            assert!(e.message.contains("nothing was changed"), "{}", e.message);
            assert_eq!(read(&settings), bad);
            assert_eq!(status(Agent::ClaudeCode, &homes, None).state, "unreadable");
            // Taking ours away from a file that does not read is refused too;
            // from one of another shape, there is nothing of ours to find.
            match Json::parse(bad) {
                Ok(v) if v.as_object().is_some() => {
                    assert!(plan_disconnect(Agent::ClaudeCode, &homes).unwrap().is_empty(), "{bad}")
                }
                _ => assert!(plan_disconnect(Agent::ClaudeCode, &homes).is_err(), "{bad}"),
            }
        }
        assert!(!homes.skill_dir(Agent::ClaudeCode).exists());
        // Someone else's skill in our folder is not overwritten.
        std::fs::write(&settings, "{}").unwrap();
        std::fs::create_dir_all(homes.skill_dir(Agent::ClaudeCode)).unwrap();
        std::fs::write(homes.skill_file(Agent::ClaudeCode), "---\nname: keymotion\n---\nmine").unwrap();
        assert_eq!(connect(Agent::ClaudeCode, &homes, &cli(&home)).unwrap_err().kind, ErrorKind::Invalid);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn crlf_files_keep_their_line_endings() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        std::fs::create_dir_all(&homes.claude).unwrap();
        let settings = homes.hooks_file(Agent::ClaudeCode);
        let crlf = USERS_OWN.replace('\n', "\r\n");
        std::fs::write(&settings, &crlf).unwrap();
        connect(Agent::ClaudeCode, &homes, &cli(&home)).unwrap();
        let text = read(&settings);
        assert!(text.contains("\r\n") && !text.replace("\r\n", "").contains('\n'));
        disconnect(Agent::ClaudeCode, &homes).unwrap();
        assert_eq!(read(&settings), crlf);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn codex_gets_hooks_a_rule_and_the_skill_and_waits_for_trust() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        std::fs::create_dir_all(&homes.codex).unwrap();
        let cli = cli(&home);
        let plan = connect(Agent::Codex, &homes, &cli).unwrap();
        assert!(plan.warnings.iter().any(|w| w.contains("/hooks")));
        let file = homes.hooks_file(Agent::Codex);
        let v = Json::parse(&read(&file)).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["hooks"], "Codex reads nothing else at the top");
        let stop = &v.get("hooks").unwrap().get("Stop").unwrap().as_array().unwrap()[0];
        let handler = &stop.get("hooks").unwrap().as_array().unwrap()[0];
        let line = handler.get("command").and_then(Json::as_str).unwrap();
        assert_eq!(line, format!("if (Test-Path -LiteralPath '{0}') {{ & '{0}' hook codex }}", cli.path_text()));
        assert!(stop.get("matcher").is_none());
        assert!(read(&homes.rules_file()).contains("prefix_rule("));
        assert_eq!(skill_version(&read(&homes.skill_file(Agent::Codex))), Some(SKILL_VERSION));
        assert!(homes.skill_file(Agent::Codex).starts_with(home.join(".agents").join("skills")));
        let st = status(Agent::Codex, &homes, Some(&cli));
        assert_eq!((st.state, st.trusted, st.rules), ("needs_approval", Some(0), Some(RULES_VERSION)));

        // The user trusts them in Codex: Codex writes their hashes.
        let mut config = String::from("model = \"x\"\n\n[hooks.state]\n");
        for h in CODEX_HOOKS {
            let key = format!("{}:{}:0:0", file.display(), codex_event_label(h.event));
            let hash = codex_hook_hash(h.event, line, h.run_async, h.timeout_s);
            config.push_str(&format!("[hooks.state.{}]\ntrusted_hash = \"{hash}\"\n", toml_key(&key)));
        }
        let config = config.replace("[hooks.state]\n", "");
        std::fs::write(homes.codex.join("config.toml"), &config).unwrap();
        let st = status(Agent::Codex, &homes, Some(&cli));
        assert_eq!((st.state, st.trusted), ("connected", Some(CODEX_HOOKS.len())), "{config}");
        // A changed line is not trusted any more.
        let moved = Cli { path: home.join("x").join("keymotion-cli.exe"), kind: CliKind::Dev };
        connect(Agent::Codex, &homes, &moved).unwrap();
        assert_eq!(status(Agent::Codex, &homes, Some(&moved)).state, "needs_approval");

        disconnect(Agent::Codex, &homes).unwrap();
        assert!(!file.exists(), "we made the file, we took it away");
        assert!(!homes.rules_file().exists() && !homes.rules_dir().exists());
        assert!(!homes.skill_dir(Agent::Codex).exists());
        assert!(homes.codex.join("config.toml").exists(), "Codex's own file is never touched");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The hashes Codex 0.166.0 itself gave for these two hooks
    /// (`codex app-server`, `hooks/list`, `currentHash`, 2026-11-22).
    #[test]
    fn the_trust_hash_is_the_one_codex_computes() {
        assert_eq!(
            codex_hook_hash("Stop", r"& 'C:\Users\A B\bin\keymotion-cli.exe' hook codex", true, 10),
            "sha256:0e8fc55d859112b65786dd58471581168cbff85c091e71692fb749ba119af30c"
        );
        assert_eq!(
            codex_hook_hash("SessionEnd", r"C:\x\keymotion-cli.exe hook codex", false, 3),
            "sha256:5405e135140ffbf019672932b7710cbc386fa93b93949b72a049268067fe3589"
        );
    }

    fn toml_key(k: &str) -> String {
        format!("'{k}'")
    }

    #[test]
    fn codexs_own_hooks_file_keeps_the_users_hooks() {
        let home = temp_home();
        let homes = Homes::with(&home, None, Some(home.join("codex home")));
        std::fs::create_dir_all(&homes.codex).unwrap();
        let file = homes.hooks_file(Agent::Codex);
        let users = "{\n  \"description\": \"mine\",\n  \"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"notify.exe\"\n          }\n        ]\n      }\n    ]\n  }\n}\n";
        std::fs::write(&file, users).unwrap();
        std::fs::create_dir_all(homes.rules_dir()).unwrap();
        std::fs::write(homes.rules_dir().join("default.rules"), "prefix_rule(pattern=[\"git\"], decision=\"allow\")\n")
            .unwrap();
        connect(Agent::Codex, &homes, &cli(&home)).unwrap();
        let v = Json::parse(&read(&file)).unwrap();
        assert_eq!(v.get("description").and_then(Json::as_str), Some("mine"));
        assert_eq!(v.get("hooks").unwrap().get("Stop").unwrap().as_array().unwrap().len(), 2);
        disconnect(Agent::Codex, &homes).unwrap();
        assert_eq!(read(&file), users);
        assert!(homes.rules_dir().join("default.rules").exists(), "the user's rules and their folder stay");
        // A key Codex cannot read: refused, as Codex would not load it.
        std::fs::write(&file, "{\"hooks\": {}, \"extra\": 1}").unwrap();
        assert_eq!(connect(Agent::Codex, &homes, &cli(&home)).unwrap_err().kind, ErrorKind::Invalid);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_codex_line_quotes_every_single_quote_powershell_knows() {
        let alias = Cli {
            path: PathBuf::from(r"C:\Users\saban\AppData\Local\Microsoft\WindowsApps\keymotion-cli.exe"),
            kind: CliKind::Store,
        };
        assert_eq!(
            codex_command(&alias),
            r"if (Test-Path -LiteralPath 'C:\Users\saban\AppData\Local\Microsoft\WindowsApps\keymotion-cli.exe') { & 'C:\Users\saban\AppData\Local\Microsoft\WindowsApps\keymotion-cli.exe' hook codex }"
        );
        let spaced =
            Cli { path: PathBuf::from(r"C:\Users\Ana María\bin\keymotion-cli.exe"), kind: CliKind::Installed };
        assert!(codex_command(&spaced).contains(r"& 'C:\Users\Ana María\bin\keymotion-cli.exe' hook codex }"));
        // ' ‘ ’ ‚ ‛: PowerShell ends a single-quoted string at each of them.
        for q in PS_SINGLE_QUOTES {
            let cli = Cli {
                path: PathBuf::from(format!(r"C:\Users\O{q}Neil\bin\keymotion-cli.exe")),
                kind: CliKind::Installed,
            };
            let line = codex_command(&cli);
            assert!(line.contains(&format!(r"'C:\Users\O{q}{q}Neil\bin\keymotion-cli.exe'")), "{line}");
        }
        assert_eq!(ps_quoted("a’b'c"), "'a’’b''c'");
        // On PATH, by its bare name.
        let bare = Cli { path: PathBuf::from("keymotion-cli"), kind: CliKind::Installed };
        assert_eq!(
            codex_command(&bare),
            "if (Get-Command 'keymotion-cli' -ErrorAction SilentlyContinue) { & 'keymotion-cli' hook codex }"
        );
        assert_eq!(
            command_line(Agent::ClaudeCode, &spaced),
            r#""C:\Users\Ana María\bin\keymotion-cli.exe" hook claude-code"#
        );
        // Ours, whichever form we ever wrote.
        let older = [
            r"C:\x\keymotion-cli.exe hook codex",
            r"& 'C:\x y\keymotion-cli.exe' hook codex",
            "\"C:\\x\\keymotion-cli.exe\" hook codex",
        ];
        for line in [codex_command(&alias), codex_command(&spaced), codex_command(&bare)]
            .iter()
            .map(String::as_str)
            .chain(older)
        {
            assert!(is_ours(Agent::Codex, &Json::Object(vec![("command".into(), Json::str(line))])), "{line}");
            assert!(!is_ours(Agent::ClaudeCode, &Json::Object(vec![("command".into(), Json::str(line))])), "{line}");
        }
        assert!(!is_ours(Agent::Codex, &Json::Object(vec![("command".into(), Json::str("notify.exe hook codex"))])));
    }

    #[test]
    fn the_stable_path_is_the_alias_or_the_install() {
        let local = PathBuf::from(r"C:\Users\a\AppData\Local");
        let store = Cli::for_exe(
            Path::new(r"C:\Program Files\WindowsApps\cebersaban.Keymotion_1.2.0.0_x64__pa98\keymotion-cli.exe"),
            true,
            Some(&local),
        );
        assert_eq!(store, Cli { path: local.join(r"Microsoft\WindowsApps\keymotion-cli.exe"), kind: CliKind::Store });
        let home = temp_home();
        let dir = home.join("Keymotion");
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("keymotion.exe"), b"").unwrap();
        let exe = dir.join("bin").join("keymotion-cli.exe");
        assert_eq!(Cli::for_exe(&exe, false, Some(&local)).kind, CliKind::Installed);
        assert_eq!(Cli::for_exe(Path::new(r"C:\dev\target\debug\keymotion-cli.exe"), false, None).kind, CliKind::Dev);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn other_copies_looks_at_every_hook_the_skill_and_the_rule() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        let mine = cli(&home);
        let other = Cli { path: home.join("Other").join("bin").join("keymotion-cli.exe"), kind: CliKind::Installed };
        for agent in [Agent::ClaudeCode, Agent::Codex] {
            assert!(other_copies(agent, &homes, &mine).is_empty(), "nothing connected");
            connect(agent, &homes, &mine).unwrap();
            assert!(other_copies(agent, &homes, &mine).is_empty(), "all mine");
            // The same path in other letter case: still mine.
            let upper = Cli { path: PathBuf::from(mine.path_text().to_uppercase()), kind: CliKind::Installed };
            assert!(other_copies(agent, &homes, &upper).is_empty());
            assert!(!other_copies(agent, &homes, &other).is_empty());
        }
        // No hooks left: the skill (both agents) and the rule (Codex) still
        // name `mine`.
        std::fs::write(homes.hooks_file(Agent::ClaudeCode), "{}\n").unwrap();
        std::fs::write(homes.hooks_file(Agent::Codex), "{}\n").unwrap();
        let skill = |agent| homes.skill_file(agent).display().to_string();
        assert_eq!(other_copies(Agent::ClaudeCode, &homes, &other), [skill(Agent::ClaudeCode)]);
        let rules = homes.rules_file().display().to_string();
        assert_eq!(other_copies(Agent::Codex, &homes, &other), [skill(Agent::Codex), rules]);
        // One hook among ours names the other copy: it counts.
        connect(Agent::ClaudeCode, &homes, &mine).unwrap();
        let settings = homes.hooks_file(Agent::ClaudeCode);
        let json_text = |s: String| serde_json::to_string(&s).unwrap().trim_matches('"').to_string();
        let swapped = read(&settings).replacen(&json_text(mine.path_text()), &json_text(other.path_text()), 1);
        std::fs::write(&settings, swapped).unwrap();
        assert_eq!(other_copies(Agent::ClaudeCode, &homes, &mine), [other.path_text()]);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn refresh_rewrites_only_our_outdated_files_and_keeps_their_path() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        let cli = cli(&home);
        assert!(refresh(&homes).is_empty(), "not connected: nothing");
        connect(Agent::ClaudeCode, &homes, &cli).unwrap();
        connect(Agent::Codex, &homes, &cli).unwrap();
        assert!(refresh(&homes).is_empty(), "up to date");
        let skill = homes.skill_file(Agent::Codex);
        assert_eq!(skill_cli_path(&read(&skill)), Some(cli.path.clone()));
        assert_eq!(rules_cli_path(&read(&homes.rules_file())), Some(cli.path.clone()));
        let old = |t: String, m: &str, v: u32| t.replace(&format!("{m} {v}"), &format!("{m} 0"));
        std::fs::write(&skill, old(read(&skill), SKILL_MARKER, SKILL_VERSION)).unwrap();
        let rules = homes.rules_file();
        std::fs::write(&rules, old(read(&rules), RULES_MARKER, RULES_VERSION)).unwrap();
        let done = refresh(&homes);
        assert_eq!(done, [skill.clone(), rules.clone()]);
        // Brought up to date for the keymotion-cli it named, whoever runs.
        assert_eq!(read(&skill), skill_text(&cli));
        assert_eq!(read(&rules), rules_text(&cli));
        // A skill the user wrote in our folder is not ours to rewrite; nor
        // an old one whose path cannot be read.
        std::fs::write(&skill, "mine").unwrap();
        std::fs::write(&rules, format!("# {RULES_MARKER} 0\n")).unwrap();
        assert!(refresh(&homes).is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_file_that_changed_since_the_plan_is_not_overwritten() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        std::fs::create_dir_all(&homes.claude).unwrap();
        let settings = homes.hooks_file(Agent::ClaudeCode);
        std::fs::write(&settings, USERS_OWN).unwrap();
        let plan = plan_connect(Agent::ClaudeCode, &homes, &cli(&home)).unwrap();
        // Claude Code writes its settings while the user reads the plan
        // (/model, /permissions).
        let theirs = USERS_OWN.replace("\"opus\"", "\"sonnet\"");
        std::fs::write(&settings, &theirs).unwrap();
        let e = apply(&plan).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Changed, "{}", e.message);
        assert_eq!(read(&settings), theirs, "their change stays");
        assert!(!homes.skill_dir(Agent::ClaudeCode).exists(), "nothing of the plan was written");
        // Planned again from the file as it is: both.
        apply(&plan_connect(Agent::ClaudeCode, &homes, &cli(&home)).unwrap()).unwrap();
        let text = read(&settings);
        assert!(text.contains("\"sonnet\"") && text.contains("keymotion-cli.exe"), "{text}");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_read_only_file_is_left_as_it_was() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        std::fs::create_dir_all(&homes.claude).unwrap();
        let settings = homes.hooks_file(Agent::ClaudeCode);
        std::fs::write(&settings, USERS_OWN).unwrap();
        let mut perms = std::fs::metadata(&settings).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&settings, perms.clone()).unwrap();
        let e = connect(Agent::ClaudeCode, &homes, &cli(&home)).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Io, "{}", e.message);
        assert_eq!(read(&settings), USERS_OWN);
        let left: Vec<String> = std::fs::read_dir(&homes.claude)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect();
        assert!(left.iter().all(|n| !n.ends_with(".tmp")), "no temporary file left: {left:?}");
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&settings, perms).unwrap();
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_linked_settings_file_is_written_through_the_link() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        std::fs::create_dir_all(&homes.claude).unwrap();
        let dotfiles = home.join("dotfiles");
        std::fs::create_dir_all(&dotfiles).unwrap();
        let real = dotfiles.join("claude-settings.json");
        std::fs::write(&real, USERS_OWN).unwrap();
        let settings = homes.hooks_file(Agent::ClaudeCode);
        if let Err(e) = std::os::windows::fs::symlink_file(&real, &settings) {
            // Creating links needs Developer Mode or an administrator.
            eprintln!("skipped: no symbolic link here ({e})");
            let _ = std::fs::remove_dir_all(&home);
            return;
        }
        connect(Agent::ClaudeCode, &homes, &cli(&home)).unwrap();
        assert!(std::fs::symlink_metadata(&settings).unwrap().file_type().is_symlink(), "the link stays");
        assert!(read(&real).contains("keymotion-cli.exe"), "the file it points to has our hooks");
        disconnect(Agent::ClaudeCode, &homes).unwrap();
        assert!(std::fs::symlink_metadata(&settings).unwrap().file_type().is_symlink());
        assert_eq!(read(&real), USERS_OWN);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_old_claude_code_is_named_and_connecting_waits_for_an_update() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(old_claude_code(&[]), None);
        assert_eq!(old_claude_code(&[(p("a"), [2, 1, 139]), (p("b"), [2, 1, 312])]), None);
        let old = old_claude_code(&[(p("new"), [2, 1, 312]), (p(r"C:\npm\package.json"), [2, 1, 100])]).unwrap();
        assert!(old.contains("2.1.100") && old.contains("2.1.139") && old.contains(r"C:\npm\package.json"), "{old}");
        // An npm copy in the user's folder is found by its package.
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        let npm = home.join(r"AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code");
        std::fs::create_dir_all(&npm).unwrap();
        std::fs::write(npm.join("package.json"), r#"{"name":"@anthropic-ai/claude-code","version":"2.1.120"}"#)
            .unwrap();
        assert_eq!(claude_code_versions(&homes).len(), 1);
        let e = connect(Agent::ClaudeCode, &homes, &cli(&home)).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Unsupported, "{}", e.message);
        assert!(!home.join(".claude").exists(), "nothing written");
        assert!(status(Agent::ClaudeCode, &homes, None).warning.is_some_and(|w| w.contains("2.1.120")));
        std::fs::write(npm.join("package.json"), r#"{"version":"2.1.200"}"#).unwrap();
        assert!(connect(Agent::ClaudeCode, &homes, &cli(&home)).is_ok());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_rules_file_that_is_not_ours_is_refused() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        std::fs::create_dir_all(homes.rules_dir()).unwrap();
        let mine = "prefix_rule(pattern=[\"git\"], decision=\"allow\")\n";
        std::fs::write(homes.rules_file(), mine).unwrap();
        let e = connect(Agent::Codex, &homes, &cli(&home)).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Invalid);
        assert_eq!(read(&homes.rules_file()), mine);
        assert!(!homes.hooks_file(Agent::Codex).exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_closed_keymotion_is_noted_beside_the_skill() {
        let home = temp_home();
        let homes = Homes::with(&home, None, None);
        let logon = Some(0x1234_5678);
        assert!(mark_stopped(&homes, Stopped::Quit, logon).is_empty(), "nothing connected, nothing written");
        assert!(!home.join(".claude").exists() && !home.join(".agents").exists());
        connect(Agent::ClaudeCode, &homes, &cli(&home)).unwrap();
        assert!(!hooks_stopped(&homes, Agent::ClaudeCode, logon));
        assert_eq!(mark_stopped(&homes, Stopped::Quit, logon), [homes.skill_dir(Agent::ClaudeCode).join(STOPPED_FILE)]);
        assert!(hooks_stopped(&homes, Agent::ClaudeCode, logon));
        // A quit counts for that sign-in only: after a restart, Keymotion is
        // just not running.
        assert!(!hooks_stopped(&homes, Agent::ClaudeCode, Some(0x9999)));
        assert!(!hooks_stopped(&homes, Agent::ClaudeCode, None));
        assert!(!hooks_stopped(&homes, Agent::Codex, logon), "not connected");
        // Access off counts whatever the sign-in.
        mark_stopped(&homes, Stopped::AccessOff, logon);
        assert!(hooks_stopped(&homes, Agent::ClaudeCode, Some(0x9999)));
        assert_eq!(clear_stopped(&homes).len(), 1);
        assert!(!hooks_stopped(&homes, Agent::ClaudeCode, logon));
        // Unreadable: as if it were not there.
        std::fs::write(homes.skill_dir(Agent::ClaudeCode).join(STOPPED_FILE), "{ not json").unwrap();
        assert!(!hooks_stopped(&homes, Agent::ClaudeCode, logon));
        // Disconnecting takes it away with the skill.
        mark_stopped(&homes, Stopped::AccessOff, logon);
        disconnect(Agent::ClaudeCode, &homes).unwrap();
        assert!(!homes.skill_dir(Agent::ClaudeCode).exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_skill_is_thin_and_says_what_was_decided() {
        let text = skill_text(&cli(Path::new(r"C:\h")));
        let front: Vec<&str> = text.lines().skip(1).take_while(|l| *l != "---").collect();
        assert_eq!(front.len(), 2, "name and a short description: {front:?}");
        assert!(front[1].len() < 300);
        assert!(text.contains("keymotion-cli guide"));
        assert!(text.contains("You decide how to tell the user things"));
        assert!(text.contains("optional"));
        assert!(text.lines().count() < 40);
        assert!(text.is_ascii());
        assert_eq!(skill_version(&text), Some(SKILL_VERSION));
        let rules = rules_text(&cli(Path::new(r"C:\Users\Ana B")));
        assert!(rules.contains("\"keymotion-cli\""));
        // Starlark reads a backslash as an escape: each one doubled.
        assert!(
            rules.contains(r#"paths = ["C:\\Users\\Ana B\\Program Files\\Key Motion\\bin\\keymotion-cli.exe"]"#),
            "{rules}"
        );
        for never in ["connect", "disconnect", "hook"] {
            assert!(!ALLOWED_COMMANDS.contains(&never));
        }
        assert_eq!(codex_event_label("PostToolUse"), "post_tool_use");
        assert_eq!(codex_event_label("Stop"), "stop");
    }
}
