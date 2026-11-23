//! The coding apps on the AI agents page (plan §12.2, §12.4): which of
//! Claude Code and Codex are here and connected, what turning agent access
//! on would add, adding it with one approval, taking it away again.
//!
//! The work is `kb_hooks::setup`'s (keymotion-cli's `connect` and
//! `disconnect` use the same): only Keymotion's own entries and files are
//! written or removed, another program's file is copied first, a file that
//! does not read is left alone.
//!
//! * The page shows the plan first ([`plan_connect`]) and hands its token
//!   back. The token stands for what the page showed: which files change
//!   how, how many hooks, where the copies go, the PATH folder. When the
//!   coding app writes its settings in between, the plan is made again from
//!   the file as it is now; if it still says the same, it is written (the
//!   user's new entries kept); if not, nothing is written and the page shows
//!   the new plan.
//! * The GitHub copy also puts its `bin` folder on the user's PATH, so
//!   terminals find keymotion-cli by its name (owner's decision, plan §12.1
//!   row 6): when the user turns access on, never by itself, and only when
//!   keymotion-cli is there. Turning access off leaves it; the uninstaller
//!   takes it away (only if Keymotion added it); updates leave it alone. A
//!   PATH that does not read is never written (`kb_winsys::user_path`).

use crate::dto::{
    AgentApp, AgentAppPlan, AgentAppState, AgentAppStatus, AgentCli, AgentCliKind, AgentConnectPlan, AgentConnections,
    AgentFileAction, AgentFileChange, AgentFileKind, AgentPathInfo, AgentProblem, AgentProblemKind,
};
use kb_agent::Channel;
use kb_hooks::setup::{self, Cli, CliKind, ErrorKind, Homes, Plan, SetupError};
use kb_hooks::Agent;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// How many times a file the coding app keeps writing is planned again.
const TRIES: usize = 3;

/// The user's PATH, as the page needs it (the registry in the app, a fake in
/// tests).
pub trait UserPath: Send + Sync {
    /// `dir` is on it; an error when it cannot be read.
    fn has(&self, dir: &Path) -> Result<bool, String>;
    /// Put `dir` at its end; whether it changed.
    fn add(&self, dir: &Path) -> Result<bool, String>;
    /// Tell open programs it changed (without waiting for them).
    fn announce(&self);
}

/// The user's own PATH in the registry (`kb_winsys::user_path`).
pub struct WindowsPath;

impl UserPath for WindowsPath {
    fn has(&self, dir: &Path) -> Result<bool, String> {
        kb_winsys::user_path::has(dir)
    }

    fn add(&self, dir: &Path) -> Result<bool, String> {
        kb_winsys::user_path::add(dir)
    }

    fn announce(&self) {
        // A program that hangs costs the broadcast seconds: not ours to wait.
        let spawned = std::thread::Builder::new().name("path-announce".into()).spawn(kb_winsys::user_path::announce);
        if let Err(e) = spawned {
            tracing::warn!("PATH change not announced: {e}");
        }
    }
}

/// Where things are for this copy of Keymotion.
pub struct Place {
    /// The coding apps' folders; `None` without a user folder.
    pub homes: Option<Homes>,
    /// The keymotion-cli this copy comes with.
    pub cli: Option<Cli>,
    /// The GitHub copy's `bin` folder, which goes on PATH.
    pub bin: Option<PathBuf>,
}

impl Place {
    /// The keymotion-cli this copy of the app comes with: the Store copy's
    /// alias, the GitHub copy's `bin` folder, a development build's own
    /// folder (`cargo build -p keymotion-cli`).
    pub fn for_channel(channel: Channel) -> Place {
        let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
        let (cli, bin) = match channel {
            Channel::Store => {
                let local = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()).map(PathBuf::from);
                let alias = local.map(|l| l.join(r"Microsoft\WindowsApps\keymotion-cli.exe"));
                (alias.map(|path| Cli { path, kind: CliKind::Store }), None)
            }
            Channel::Github => {
                let bin = exe_dir.map(|d| d.join("bin"));
                (bin.as_ref().map(|b| Cli { path: b.join("keymotion-cli.exe"), kind: CliKind::Installed }), bin)
            }
            _ => (exe_dir.map(|d| Cli { path: d.join("keymotion-cli.exe"), kind: CliKind::Dev }), None),
        };
        // Trying the page with a test folder never touches the real PATH.
        let bin = bin.filter(|_| super::test_home().is_none());
        Place { homes: super::homes(), cli, bin }
    }

    fn cli_info(&self) -> Option<AgentCli> {
        self.cli.as_ref().map(|c| AgentCli {
            path: c.path_text(),
            kind: match c.kind {
                CliKind::Store => AgentCliKind::Store,
                CliKind::Installed => AgentCliKind::Installed,
                CliKind::Dev => AgentCliKind::Dev,
            },
            // The Store alias is a link Windows resolves on start: there is
            // something at its path, not necessarily a file.
            present: std::fs::symlink_metadata(&c.path).is_ok(),
        })
    }

    fn path_info(&self, user_path: &dyn UserPath) -> Option<AgentPathInfo> {
        self.bin.as_ref().map(|b| {
            let (on_path, error) = match user_path.has(b) {
                Ok(on) => (on, None),
                Err(e) => (false, Some(e)),
            };
            AgentPathInfo { dir: b.display().to_string(), on_path, error }
        })
    }

    /// The keymotion-cli to connect with, or why there is none.
    fn usable_cli(&self) -> Result<&Cli, AgentProblem> {
        match (&self.cli, self.cli_info()) {
            (Some(cli), Some(info)) if info.present => Ok(cli),
            (Some(cli), _) => Err(AgentProblem {
                kind: AgentProblemKind::NoCli,
                message: format!("keymotion-cli is not at {}.", cli.path_text()),
            }),
            _ => Err(AgentProblem {
                kind: AgentProblemKind::NoCli,
                message: "Keymotion cannot tell where its keymotion-cli is.".into(),
            }),
        }
    }
}

pub fn app_of(agent: Agent) -> AgentApp {
    match agent {
        Agent::ClaudeCode => AgentApp::ClaudeCode,
        Agent::Codex => AgentApp::Codex,
    }
}

pub fn agent_of(app: AgentApp) -> Agent {
    match app {
        AgentApp::ClaudeCode => Agent::ClaudeCode,
        AgentApp::Codex => Agent::Codex,
    }
}

fn dotted(v: [u16; 3]) -> String {
    v.map(|p| p.to_string()).join(".")
}

fn no_home() -> AgentProblem {
    AgentProblem { kind: AgentProblemKind::Io, message: "No user folder (USERPROFILE is not set).".into() }
}

fn problem(e: SetupError) -> AgentProblem {
    let kind = match e.kind {
        ErrorKind::Invalid => AgentProblemKind::Invalid,
        ErrorKind::Io => AgentProblemKind::Io,
        ErrorKind::Unsupported => AgentProblemKind::Unsupported,
        ErrorKind::Changed => AgentProblemKind::Changed,
        ErrorKind::Partial => AgentProblemKind::Partial,
    };
    AgentProblem { kind, message: e.message }
}

// ---------------------------------------------------------------- status

/// Both coding apps, the keymotion-cli and (GitHub copy) PATH.
pub fn connections(place: &Place, user_path: &dyn UserPath) -> AgentConnections {
    let apps = match &place.homes {
        Some(homes) => Agent::ALL.iter().map(|a| app_status(*a, homes, place.cli.as_ref())).collect(),
        None => Vec::new(),
    };
    AgentConnections { apps, cli: place.cli_info(), path: place.path_info(user_path) }
}

fn app_status(agent: Agent, homes: &Homes, cli: Option<&Cli>) -> AgentAppStatus {
    let s = setup::status(agent, homes, cli);
    let state = match s.state {
        "not_connected" => AgentAppState::NotConnected,
        "connected" => AgentAppState::Connected,
        "needs_approval" => AgentAppState::NeedsApproval,
        "outdated" => AgentAppState::Outdated,
        "unreadable" => AgentAppState::Unreadable,
        _ => AgentAppState::NotFound,
    };
    let claude = agent == Agent::ClaudeCode;
    let oldest = if claude { setup::claude_code_versions(homes).into_iter().map(|(_, v)| v).min() } else { None };
    AgentAppStatus {
        app: app_of(agent),
        name: agent.title().to_string(),
        found: s.found,
        state,
        hooks: s.hooks as u32,
        hooks_wanted: s.hooks_wanted as u32,
        trusted: s.trusted.map(|t| t as u32),
        version: oldest.map(dotted),
        too_old: oldest.is_some_and(|v| v < setup::CLAUDE_CODE_MIN),
        min_version: claude.then(|| dotted(setup::CLAUDE_CODE_MIN)),
        home: homes.of(agent).display().to_string(),
        hooks_file: s.hooks_file.display().to_string(),
        skill_file: s.skill_file.display().to_string(),
        runs: s.runs,
        error: s.error,
    }
}

// ------------------------------------------------------------------ plans

fn kind_of(agent: Agent, homes: &Homes, path: &Path) -> AgentFileKind {
    if path == homes.hooks_file(agent) {
        AgentFileKind::Hooks
    } else if path == homes.skill_file(agent) {
        AgentFileKind::Skill
    } else if agent == Agent::Codex && path == homes.rules_file() {
        AgentFileKind::Rule
    } else {
        AgentFileKind::Note
    }
}

/// A kb-hooks plan as the page shows it. `hooks`: how many of ours the
/// settings file holds afterwards (connecting) or loses (disconnecting).
fn app_plan(agent: Agent, homes: &Homes, plan: Result<&Plan, AgentProblem>, hooks: u32) -> AgentAppPlan {
    let mut out = AgentAppPlan {
        app: app_of(agent),
        name: agent.title().to_string(),
        home: homes.of(agent).display().to_string(),
        changes: Vec::new(),
        command_line: None,
        needs_approval: false,
        warnings: Vec::new(),
        problem: None,
    };
    let plan = match plan {
        Ok(plan) => plan,
        Err(p) => {
            out.problem = Some(p);
            return out;
        }
    };
    out.changes = plan
        .changes()
        .map(|e| {
            let kind = kind_of(agent, homes, &e.path);
            let action = match (&e.before, &e.after) {
                (None, Some(_)) => AgentFileAction::Create,
                (Some(_), None) => AgentFileAction::Remove,
                _ => AgentFileAction::Update,
            };
            let backup = (e.backup && e.before.is_some() && e.after.is_some()).then(|| {
                let mut name = e.path.as_os_str().to_os_string();
                name.push(setup::BACKUP_SUFFIX);
                PathBuf::from(name).display().to_string()
            });
            AgentFileChange {
                kind,
                action,
                path: e.path.display().to_string(),
                summary: e.summary.clone(),
                hooks: (kind == AgentFileKind::Hooks).then_some(hooks),
                backup,
            }
        })
        .collect();
    out.command_line = plan.command_line.clone();
    out.needs_approval = agent == Agent::Codex && !out.changes.is_empty() && plan.command_line.is_some();
    out.warnings = plan.warnings.clone();
    out
}

/// What the page's token stands for: what it showed, not the files' bytes.
fn shape_hash(state: &mut impl Hasher, p: &AgentAppPlan) {
    p.app.hash(state);
    p.problem.as_ref().map(|x| x.kind as u8).hash(state);
    for c in &p.changes {
        (c.kind as u8, c.action as u8, &c.path, c.hooks, &c.backup).hash(state);
    }
    p.command_line.hash(state);
}

fn token(apps: &[AgentAppPlan], path: Option<&AgentPathInfo>, cli: Option<&AgentCli>) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in apps {
        shape_hash(&mut h, p);
    }
    path.map(|p| &p.dir).hash(&mut h);
    cli.map(|c| &c.path).hash(&mut h);
    format!("{:016x}", h.finish())
}

/// The kb-hooks plan for one coding app, and how it shows.
fn connect_one(agent: Agent, homes: &Homes, cli: Result<&Cli, AgentProblem>) -> (Option<Plan>, AgentAppPlan) {
    let hooks = setup::our_hooks(agent).len() as u32;
    let planned = cli.and_then(|cli| setup::plan_connect(agent, homes, cli).map_err(problem));
    match planned {
        Ok(plan) => {
            let shown = app_plan(agent, homes, Ok(&plan), hooks);
            (Some(plan), shown)
        }
        Err(p) => (None, app_plan(agent, homes, Err(p), hooks)),
    }
}

/// The coding apps to connect: those asked for (default both) that are on
/// this computer.
fn chosen(homes: &Homes, apps: Option<&[AgentApp]>) -> (Vec<Agent>, Vec<AgentApp>) {
    let mut found = Vec::new();
    let mut not_found = Vec::new();
    for agent in Agent::ALL {
        if apps.is_some_and(|a| !a.contains(&app_of(agent))) {
            continue;
        }
        if homes.of(agent).is_dir() {
            found.push(agent);
        } else {
            not_found.push(app_of(agent));
        }
    }
    (found, not_found)
}

/// What turning access on would write, and the kb-hooks plans behind it.
fn build(place: &Place, user_path: &dyn UserPath, apps: Option<&[AgentApp]>) -> (Vec<(Agent, Plan)>, AgentConnectPlan) {
    let cli_info = place.cli_info();
    // PATH only for a keymotion-cli that is there: no dead entries.
    let path = place.path_info(user_path).filter(|p| !p.on_path && place.usable_cli().is_ok());
    let mut plans = Vec::new();
    let mut shown = Vec::new();
    let mut not_found = Vec::new();
    match &place.homes {
        Some(homes) => {
            let (found, missing) = chosen(homes, apps);
            not_found = missing;
            for agent in found {
                let (plan, view) = connect_one(agent, homes, place.usable_cli());
                if let Some(plan) = plan {
                    plans.push((agent, plan));
                }
                shown.push(view);
            }
        }
        None => {
            for agent in Agent::ALL {
                if apps.is_none_or(|a| a.contains(&app_of(agent))) {
                    let mut view = app_plan(agent, &Homes::with(Path::new(""), None, None), Err(no_home()), 0);
                    view.home = String::new();
                    shown.push(view);
                }
            }
        }
    }
    let empty = path.as_ref().is_none_or(|p| p.error.is_some())
        && shown.iter().all(|p| p.problem.is_some() || p.changes.is_empty());
    let token = token(&shown, path.as_ref(), cli_info.as_ref());
    (plans, AgentConnectPlan { token, apps: shown, not_found, path, cli: cli_info, empty })
}

/// What turning agent access on would add.
pub fn plan_connect(place: &Place, user_path: &dyn UserPath, apps: Option<&[AgentApp]>) -> AgentConnectPlan {
    build(place, user_path, apps).1
}

/// What [`connect`] did.
#[derive(Debug, Default)]
pub struct Applied {
    pub connected: Vec<AgentApp>,
    pub failed: Vec<AgentAppPlan>,
    pub path_added: bool,
    pub path_error: Option<String>,
}

/// Nothing left to do and nothing in the way: what a plan approved twice
/// (a double click) finds the second time.
fn finished(plan: &AgentConnectPlan) -> bool {
    plan.empty && plan.apps.iter().all(|p| p.problem.is_none())
}

/// Write what the page showed (`token`). `Err` with the plan as it is now
/// when that is not what was shown: nothing was written. When it is all
/// done already (approved twice), that is success.
pub fn connect(
    place: &Place,
    user_path: &dyn UserPath,
    token: &str,
    apps: Option<&[AgentApp]>,
) -> Result<Applied, Box<AgentConnectPlan>> {
    let (plans, shown) = build(place, user_path, apps);
    if shown.token != token && !finished(&shown) {
        return Err(Box::new(shown));
    }
    let mut done =
        Applied { failed: shown.apps.iter().filter(|p| p.problem.is_some()).cloned().collect(), ..Applied::default() };
    let homes = place.homes.as_ref();
    for (agent, plan) in plans {
        let first = shown.apps.iter().find(|p| p.app == app_of(agent)).cloned();
        let Some(first) = first else { continue };
        match (homes, place.usable_cli()) {
            (Some(homes), Ok(cli)) => match apply_as_shown(agent, homes, cli, plan, &first) {
                Ok(()) => done.connected.push(app_of(agent)),
                Err(p) => done.failed.push(AgentAppPlan { problem: Some(p), ..first }),
            },
            (_, Err(p)) => done.failed.push(AgentAppPlan { problem: Some(p), ..first }),
            (None, _) => done.failed.push(AgentAppPlan { problem: Some(no_home()), ..first }),
        }
    }
    if let Some(path) = &shown.path {
        match user_path.add(Path::new(&path.dir)) {
            Ok(changed) => done.path_added = changed,
            Err(e) => done.path_error = Some(e),
        }
    }
    Ok(done)
}

/// Whether turning access on is what the user approved: at least one
/// coding app connected, or none failed (nothing to connect, or connected
/// already). Not when every one of them failed.
pub fn access_follows(done: &Applied) -> bool {
    !done.connected.is_empty() || done.failed.is_empty()
}

/// Apply `plan`; when the coding app wrote its file meanwhile, plan again
/// and go on only while the plan still shows as `shown` did.
fn apply_as_shown(
    agent: Agent,
    homes: &Homes,
    cli: &Cli,
    plan: Plan,
    shown: &AgentAppPlan,
) -> Result<(), AgentProblem> {
    let mut plan = plan;
    for _ in 0..TRIES {
        if plan.is_empty() {
            return Ok(());
        }
        match setup::apply(&plan) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind == ErrorKind::Changed => {
                let (again, view) = connect_one(agent, homes, Ok(cli));
                let same = |p: &AgentAppPlan| {
                    let mut a = std::collections::hash_map::DefaultHasher::new();
                    let mut b = std::collections::hash_map::DefaultHasher::new();
                    shape_hash(&mut a, p);
                    shape_hash(&mut b, shown);
                    a.finish() == b.finish()
                };
                match again {
                    Some(next) if same(&view) => plan = next,
                    _ => {
                        return Err(view.problem.unwrap_or(AgentProblem {
                            kind: AgentProblemKind::Changed,
                            message: format!(
                                "{} changed its settings while Keymotion was writing; nothing was written.",
                                agent.title()
                            ),
                        }))
                    }
                }
            }
            Err(e) => return Err(problem(e)),
        }
    }
    Err(AgentProblem {
        kind: AgentProblemKind::Changed,
        message: format!("{} kept changing its settings; try again when it is idle.", agent.title()),
    })
}

/// What disconnecting `app` would take away.
pub fn plan_disconnect(place: &Place, app: AgentApp) -> AgentAppPlan {
    disconnect_view(place, app).1
}

fn disconnect_view(place: &Place, app: AgentApp) -> (Option<Plan>, AgentAppPlan) {
    let agent = agent_of(app);
    let Some(homes) = &place.homes else {
        return (None, app_plan(agent, &Homes::with(Path::new(""), None, None), Err(no_home()), 0));
    };
    let ours = setup::status(agent, homes, None).hooks as u32;
    match setup::plan_disconnect(agent, homes) {
        Ok(plan) => {
            let view = app_plan(agent, homes, Ok(&plan), ours);
            (Some(plan), view)
        }
        Err(e) => (None, app_plan(agent, homes, Err(problem(e)), ours)),
    }
}

/// Take Keymotion's hooks, skill and rule away from `app`: what was taken
/// (its `problem` set when it could not be). Removing only ours, so a file
/// the app wrote meanwhile is simply planned again.
pub fn disconnect(place: &Place, app: AgentApp) -> AgentAppPlan {
    let mut last = None;
    for _ in 0..TRIES {
        let (plan, view) = disconnect_view(place, app);
        let Some(plan) = plan else { return view };
        match setup::apply(&plan) {
            Ok(()) => return view,
            Err(e) if e.kind == ErrorKind::Changed => last = Some((view, e)),
            Err(e) => return AgentAppPlan { problem: Some(problem(e)), ..view },
        }
    }
    match last {
        Some((view, e)) => AgentAppPlan { problem: Some(problem(e)), ..view },
        None => plan_disconnect(place, app),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A PATH in memory: its folders, whether adding fails, whether it
    /// does not even read.
    #[derive(Default)]
    struct FakePath(Mutex<Vec<PathBuf>>, Mutex<bool>, Mutex<bool>);

    impl UserPath for FakePath {
        fn has(&self, dir: &Path) -> Result<bool, String> {
            if *self.2.lock().unwrap() {
                return Err("Path is not text in the registry".into());
            }
            Ok(self.0.lock().unwrap().iter().any(|d| d == dir))
        }
        fn add(&self, dir: &Path) -> Result<bool, String> {
            if *self.1.lock().unwrap() {
                return Err("PATH is full".into());
            }
            if self.has(dir)? {
                return Ok(false);
            }
            self.0.lock().unwrap().push(dir.to_path_buf());
            Ok(true)
        }
        fn announce(&self) {}
    }

    fn temp(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("km-apps-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A GitHub copy installed under `home`, with its keymotion-cli.
    fn github(home: &Path) -> Place {
        let bin = home.join(r"AppData\Local\Keymotion\bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("keymotion-cli.exe"), b"MZ").unwrap();
        Place {
            homes: Some(Homes::with(home, None, None)),
            cli: Some(Cli { path: bin.join("keymotion-cli.exe"), kind: CliKind::Installed }),
            bin: Some(bin),
        }
    }

    fn state_of(c: &AgentConnections, app: AgentApp) -> AgentAppState {
        c.apps.iter().find(|a| a.app == app).unwrap().state
    }

    #[test]
    fn turning_access_on_adds_what_the_page_showed_and_the_path() {
        let home = temp("on");
        let place = github(&home);
        let path = FakePath::default();

        // Nothing found: approving only puts the folder on PATH.
        let c = connections(&place, &path);
        assert_eq!(state_of(&c, AgentApp::ClaudeCode), AgentAppState::NotFound);
        assert_eq!(c.cli.as_ref().map(|c| (c.kind, c.present)), Some((AgentCliKind::Installed, true)));
        assert_eq!(c.path.as_ref().map(|p| p.on_path), Some(false));
        let plan = plan_connect(&place, &path, None);
        assert!(plan.apps.is_empty());
        assert_eq!(plan.not_found, [AgentApp::ClaudeCode, AgentApp::Codex]);
        assert!(!plan.empty, "the PATH folder is still to add");

        // Claude Code and Codex appear, the user has settings of their own.
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::write(home.join(r".claude\settings.json"), "{\n  \"model\": \"opus\"\n}\n").unwrap();
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        let old = plan.token.clone();
        let plan = plan_connect(&place, &path, None);
        assert_ne!(plan.token, old, "a token stands for what was shown");
        let claude = &plan.apps[0];
        assert_eq!(claude.app, AgentApp::ClaudeCode);
        let hooks = claude.changes.iter().find(|c| c.kind == AgentFileKind::Hooks).unwrap();
        assert_eq!((hooks.action, hooks.hooks), (AgentFileAction::Update, Some(16)));
        assert!(hooks.backup.as_deref().is_some_and(|b| b.ends_with("settings.json.before-keymotion")));
        assert!(claude.changes.iter().any(|c| c.kind == AgentFileKind::Skill && c.action == AgentFileAction::Create));
        assert!(!claude.needs_approval);
        let codex = &plan.apps[1];
        assert!(codex.needs_approval);
        assert!(codex.changes.iter().any(|c| c.kind == AgentFileKind::Rule));
        assert_eq!(
            codex.changes.iter().find(|c| c.kind == AgentFileKind::Hooks).map(|c| (c.action, c.hooks)),
            Some((AgentFileAction::Create, Some(10)))
        );

        // Claude Code writes its settings while the user reads: the same
        // shape, so it goes ahead and the new entry stays.
        std::fs::write(home.join(r".claude\settings.json"), "{\n  \"model\": \"sonnet\"\n}\n").unwrap();
        let done = connect(&place, &path, &plan.token, None).unwrap();
        assert_eq!(done.connected, [AgentApp::ClaudeCode, AgentApp::Codex]);
        assert!(done.failed.is_empty() && done.path_added && done.path_error.is_none());
        let text = std::fs::read_to_string(home.join(r".claude\settings.json")).unwrap();
        assert!(text.contains("\"sonnet\"") && text.contains("hook"), "{text}");

        let c = connections(&place, &path);
        assert_eq!(state_of(&c, AgentApp::ClaudeCode), AgentAppState::Connected);
        assert_eq!(state_of(&c, AgentApp::Codex), AgentAppState::NeedsApproval, "trusted in Codex next");
        assert_eq!(c.path.as_ref().map(|p| p.on_path), Some(true));
        let again = plan_connect(&place, &path, None);
        assert!(again.empty, "twice changes nothing: {again:?}");
        assert!(connect(&place, &path, &again.token, None).unwrap().connected.len() == 2);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_plan_that_no_longer_holds_writes_nothing() {
        let home = temp("stale");
        let place = github(&home);
        let path = FakePath::default();
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        let plan = plan_connect(&place, &path, Some(&[AgentApp::ClaudeCode]));
        assert_eq!(plan.apps.len(), 1);
        // Codex shows up meanwhile but was not asked for: still the same.
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        assert_eq!(plan_connect(&place, &path, Some(&[AgentApp::ClaudeCode])).token, plan.token);
        // The settings now have hooks of ours from another copy: the shape
        // differs (an update, a copy kept); shown again, nothing written.
        let other = Cli { path: home.join(r"elsewhere\keymotion-cli.exe"), kind: CliKind::Dev };
        setup::connect(Agent::ClaudeCode, place.homes.as_ref().unwrap(), &other).unwrap();
        let before = std::fs::read_to_string(home.join(r".claude\settings.json")).unwrap();
        let fresh = connect(&place, &path, &plan.token, Some(&[AgentApp::ClaudeCode])).unwrap_err();
        assert_ne!(fresh.token, plan.token);
        assert_eq!(std::fs::read_to_string(home.join(r".claude\settings.json")).unwrap(), before);
        assert!(path.0.lock().unwrap().is_empty(), "nor PATH");
        assert_eq!(connect(&place, &path, &fresh.token, Some(&[AgentApp::ClaudeCode])).unwrap().connected.len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn what_cannot_be_connected_says_why_and_the_rest_goes_ahead() {
        let home = temp("problems");
        let place = github(&home);
        let path = FakePath::default();
        // Claude Code 2.1.100 from npm: too old for the hooks.
        let npm = home.join(r"AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code");
        std::fs::create_dir_all(&npm).unwrap();
        std::fs::write(npm.join("package.json"), r#"{"name":"@anthropic-ai/claude-code","version":"2.1.100"}"#)
            .unwrap();
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        // Codex's hooks file does not read.
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(home.join(r".codex\hooks.json"), "{ not json").unwrap();

        let c = connections(&place, &path);
        let claude = c.apps.iter().find(|a| a.app == AgentApp::ClaudeCode).unwrap();
        assert_eq!(
            (claude.too_old, claude.version.as_deref(), claude.min_version.as_deref()),
            (true, Some("2.1.100"), Some("2.1.139"))
        );
        assert_eq!(state_of(&c, AgentApp::Codex), AgentAppState::Unreadable);

        let plan = plan_connect(&place, &path, None);
        assert_eq!(plan.apps[0].problem.as_ref().map(|p| p.kind), Some(AgentProblemKind::Unsupported));
        assert_eq!(plan.apps[1].problem.as_ref().map(|p| p.kind), Some(AgentProblemKind::Invalid));
        assert!(plan.apps.iter().all(|p| p.changes.is_empty()));
        *path.1.lock().unwrap() = true;
        let done = connect(&place, &path, &plan.token, None).unwrap();
        assert!(done.connected.is_empty());
        assert_eq!(done.failed.len(), 2);
        assert_eq!((done.path_added, done.path_error.as_deref()), (false, Some("PATH is full")));
        assert_eq!(std::fs::read_to_string(home.join(r".codex\hooks.json")).unwrap(), "{ not json", "left alone");

        // A development build that did not build keymotion-cli connects
        // nothing.
        let dev = Place {
            homes: place.homes.clone(),
            cli: Some(Cli { path: home.join(r"target\debug\keymotion-cli.exe"), kind: CliKind::Dev }),
            bin: None,
        };
        std::fs::remove_dir_all(&npm).unwrap();
        let plan = plan_connect(&dev, &path, Some(&[AgentApp::ClaudeCode]));
        assert_eq!(plan.apps[0].problem.as_ref().map(|p| p.kind), Some(AgentProblemKind::NoCli));
        assert!(plan.empty && plan.path.is_none());
        assert_eq!(connections(&dev, &path).cli.map(|c| (c.kind, c.present)), Some((AgentCliKind::Dev, false)));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn disconnecting_takes_only_ours_away() {
        let home = temp("off");
        let place = github(&home);
        let path = FakePath::default();
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        let plan = plan_connect(&place, &path, Some(&[AgentApp::Codex]));
        connect(&place, &path, &plan.token, Some(&[AgentApp::Codex])).unwrap();
        // The user adds a rule of their own beside ours.
        std::fs::write(home.join(r".codex\rules\mine.rules"), "# mine\n").unwrap();

        let shown = plan_disconnect(&place, AgentApp::Codex);
        let hooks = shown.changes.iter().find(|c| c.kind == AgentFileKind::Hooks).unwrap();
        assert_eq!((hooks.action, hooks.hooks), (AgentFileAction::Remove, Some(10)), "the file was ours");
        let done = disconnect(&place, AgentApp::Codex);
        assert!(done.problem.is_none(), "{done:?}");
        assert_eq!(state_of(&connections(&place, &path), AgentApp::Codex), AgentAppState::NotConnected);
        assert!(home.join(r".codex\rules\mine.rules").is_file());
        assert!(!home.join(r".codex\rules\keymotion.rules").exists());
        assert!(plan_disconnect(&place, AgentApp::Codex).changes.is_empty());
        assert!(path.has(place.bin.as_ref().unwrap()).unwrap(), "PATH stays");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn no_path_entry_for_a_keymotion_cli_that_is_not_there() {
        let home = temp("no-cli");
        let place = github(&home);
        std::fs::remove_file(place.cli.as_ref().unwrap().path.clone()).unwrap();
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        let path = FakePath::default();
        let plan = plan_connect(&place, &path, None);
        assert!(plan.path.is_none(), "a dead entry is never planned");
        assert_eq!(plan.apps[0].problem.as_ref().map(|p| p.kind), Some(AgentProblemKind::NoCli));
        let done = connect(&place, &path, &plan.token, None).unwrap();
        assert!(!done.path_added && path.0.lock().unwrap().is_empty());
        assert!(!access_follows(&done), "every app failed: access stays as it was");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_path_that_does_not_read_is_shown_and_never_written() {
        let home = temp("bad-path");
        let place = github(&home);
        let path = FakePath::default();
        *path.2.lock().unwrap() = true;
        let plan = plan_connect(&place, &path, None);
        let info = plan.path.as_ref().expect("still shown, with why");
        assert!(info.error.as_deref().is_some_and(|e| e.contains("not text")));
        assert!(plan.empty, "nothing will be written");
        let done = connect(&place, &path, &plan.token, None).unwrap();
        assert!(!done.path_added && done.path_error.is_some());
        assert!(path.0.lock().unwrap().is_empty());
        assert!(access_follows(&done), "nothing failed: access goes on for the user's own scripts");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn approving_twice_is_no_error() {
        let home = temp("twice");
        let place = github(&home);
        let path = FakePath::default();
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        let plan = plan_connect(&place, &path, None);
        assert_eq!(connect(&place, &path, &plan.token, None).unwrap().connected, [AgentApp::ClaudeCode]);
        let again = connect(&place, &path, &plan.token, None).expect("the second click finds it done");
        assert_eq!((again.connected, again.failed.len(), again.path_added), (vec![AgentApp::ClaudeCode], 0, false));
        let _ = std::fs::remove_dir_all(&home);
    }
}
