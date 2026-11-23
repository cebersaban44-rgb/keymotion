//! The command line: commands and options (clap), the environment
//! variables, and what each command asks of the app ([`Plan`]).
//!
//! Values (colors, keys, durations, ids) stay text here and are checked by
//! kb-agent before anything is sent, so a wrong value is a data error (65)
//! with kb-agent's own message and hint, as the app would answer. Only
//! what clap itself refuses (an unknown command or option, a missing
//! argument, a number that is not one) is a usage error (64). clap's exit
//! code 2 never happens: [`parse`] uses `try_parse` and maps every refusal
//! to 64.
//!
//! Later steps add commands here (focus, profile, card, scene and spell):
//! each is a [`Cmd`] variant with a [`Plan`]. Hook mode (`hook`) is read by
//! `main` before clap, so nothing about its command line can make it exit
//! with anything but 0 (`crate::hook`); clap knows it too, for `--help`.

use std::ffi::OsString;

use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use kb_agent::protocol::{
    CardArgs, ClearArgs, FocusArgs, KeysArgs, LightArgs, LogArgs, PatternArgs, ProfileArgs, ProgressArgs, SceneArgs,
    SceneStepArgs, SettingsGetArgs, SettingsSetArgs, SignalArgs, SpellArgs, StatusArgs,
};
use kb_agent::values::{duration_ms, parse_duration};
use kb_agent::{AgentError, Command, ErrorCode};
use serde_json::Value;
use std::path::PathBuf;

/// `keymotion-cli --version`. The protocol number is checked against
/// kb-agent's in the tests.
pub const VERSION_LINE: &str = concat!(env!("CARGO_PKG_VERSION"), " (protocol 1)");

/// A pipe name from [`TEST_PIPE_ENV`] must start with this; anything else
/// is ignored.
pub const TEST_PIPE_PREFIX: &str = r"\\.\pipe\keymotion-cli-test-";
/// Tests only: talk to this pipe instead of the app's, and never start the
/// app (the start is only logged). Read only by debug builds and builds with
/// the `test-pipe` feature: in a release, whoever sets a project's or a
/// CI's environment could otherwise point agents' calls at a fake app.
pub const TEST_PIPE_ENV: &str = "KEYMOTION_CLI_TEST_PIPE";
/// Whether this build reads [`TEST_PIPE_ENV`].
pub const TEST_PIPE_BUILD: bool = cfg!(any(debug_assertions, feature = "test-pipe"));
/// Tests only: the home folder `connect` and `disconnect` work in, instead
/// of the user's (`.claude`, `.codex`, `.agents` under it;
/// `CLAUDE_CONFIG_DIR` and `CODEX_HOME` count only inside it). Read by the
/// same builds as [`TEST_PIPE_ENV`].
pub const TEST_HOME_ENV: &str = "KEYMOTION_CLI_TEST_HOME";
pub const OUTPUT_ENV: &str = "KEYMOTION_OUTPUT";
pub const SOURCE_ENV: &str = "KEYMOTION_SOURCE";

/// The default `--timeout`.
pub const DEFAULT_TIMEOUT: &str = "5s";
/// The longest `--timeout`.
pub const MAX_TIMEOUT_MS: u64 = 5 * 60_000;

const HINT: &str = "Run keymotion-cli --help, or keymotion-cli guide.";
/// The heading of the options every command takes, in `--help`.
const GLOBAL: &str = "Options for every command";

/// The environment variables keymotion-cli reads, read once.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// `KEYMOTION_OUTPUT`: `json` for JSON output.
    pub output: Option<String>,
    /// `KEYMOTION_SOURCE`: the default `--source`.
    pub source: Option<String>,
    /// [`TEST_PIPE_ENV`], in builds that read it ([`TEST_PIPE_BUILD`]).
    pub test_pipe: Option<String>,
    /// [`TEST_HOME_ENV`], in the same builds.
    pub test_home: Option<String>,
}

impl Env {
    pub fn read() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let test_pipe = if TEST_PIPE_BUILD { var(TEST_PIPE_ENV) } else { None };
        let test_home = if TEST_PIPE_BUILD { var(TEST_HOME_ENV) } else { None };
        Self { output: var(OUTPUT_ENV), source: var(SOURCE_ENV), test_pipe, test_home }
    }

    /// `KEYMOTION_OUTPUT=json` (any case).
    pub fn json(&self) -> bool {
        self.output.as_deref().is_some_and(|v| v.trim().eq_ignore_ascii_case("json"))
    }

    /// The test pipe, when this build reads one and the variable holds a
    /// test pipe name.
    pub fn test_pipe(&self) -> Option<&str> {
        if !TEST_PIPE_BUILD {
            return None;
        }
        self.test_pipe
            .as_deref()
            .filter(|p| p.len() > TEST_PIPE_PREFIX.len() && p.len() <= 256 && p.starts_with(TEST_PIPE_PREFIX))
    }

    /// The variable is set but holds no test pipe name.
    pub fn test_pipe_ignored(&self) -> bool {
        self.test_pipe.is_some() && self.test_pipe().is_none()
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "keymotion-cli",
    bin_name = "keymotion-cli",
    version = VERSION_LINE,
    about = "Light the keys of the keyboard through Keymotion, for AI coding agents and scripts.",
    long_about = None,
    arg_required_else_help = true,
    after_help = "Agents: run `keymotion-cli guide` first. Examples:\n  \
        keymotion-cli signal done\n  \
        keymotion-cli light --keys f1-f12 --color green --effect wave --times 1\n  \
        keymotion-cli progress 40 --json"
)]
struct Cli {
    #[command(flatten)]
    global: Global,
    #[command(subcommand)]
    command: Cmd,
}

/// Options every command takes, before or after its name.
#[derive(Args, Debug, Clone, Default)]
pub struct Global {
    /// Print one JSON object on stdout, on success and on failure (also KEYMOTION_OUTPUT=json)
    #[arg(long, global = true, help_heading = GLOBAL)]
    pub json: bool,
    /// Who is asking, e.g. claude-code; each source has its own lights (also KEYMOTION_SOURCE; default cli)
    #[arg(long, global = true, value_name = "NAME", help_heading = GLOBAL)]
    pub source: Option<String>,
    /// How long to wait for Keymotion to answer, connecting included (default 5s)
    #[arg(long, global = true, value_name = "DURATION", help_heading = GLOBAL)]
    pub timeout: Option<String>,
    /// Do not start Keymotion when it is not running
    #[arg(long, global = true, help_heading = GLOBAL)]
    pub no_start: bool,
    /// Print nothing on success; errors still go to stderr
    #[arg(short, long, global = true, help_heading = GLOBAL)]
    pub quiet: bool,
    /// Diagnostics on stderr
    #[arg(long, global = true, value_enum, value_name = "LEVEL", help_heading = GLOBAL)]
    pub log: Option<LogLevel>,
}

/// `--log`: how much goes to stderr besides errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Cmd {
    /// Is Keymotion running, is agent access on, the keyboard, the lights on now
    Status,
    /// This keyboard's key names, rows and groups
    Keys,
    /// A ready signal: done, attention, error, busy or info (the user chose its look)
    Signal(SignalCmd),
    /// A light of your own: keys, color, effect and time
    Light(LightCmd),
    /// A progress bar on a row of keys (the F row unless the user chose otherwise)
    Progress(ProgressCmd),
    /// Take your lights away: all of this source's, or one by --id
    Clear(ClearCmd),
    /// The last agent requests Keymotion took or refused
    Log(LogCmd),
    /// Read or change the looks and behaviour of agents' lights
    #[command(subcommand)]
    Settings(SettingsCmd),
    /// The focus timer: start (the user's sequence, or one block --for 50m), pause, skip, stop, status
    #[command(subcommand)]
    Focus(FocusCmd),
    /// The user's profiles: list, show one's lighting for a while, apply its lighting (--write-flash)
    #[command(subcommand)]
    Profile(ProfileCmd),
    /// A card on the keyboard's screen: text and/or a picture (--write-flash)
    Card(CardCmd),
    /// Play your own timed animation from a JSON file; --save NAME keeps it
    Scene(SceneCmd),
    /// Your saved scenes: list, play NAME, delete NAME
    #[command(subcommand)]
    Pattern(PatternCmd),
    /// Write a word on its letter keys, one letter after another
    Spell(SpellCmd),
    /// keymotion-cli's version and protocol, and Keymotion's when it runs
    Version,
    /// A short guide for agents: what to run, examples, exit codes
    Guide,
    /// Check the setup: the app, its pipe, access, the keyboard, PATH
    Doctor,
    /// Start Keymotion without a window and wait until it answers
    Start,
    /// Connect Keymotion to a coding agent: its hooks, a skill (and for Codex a rule)
    Connect(ConnectCmd),
    /// Take Keymotion's hooks, skill and rule away from a coding agent again
    Disconnect(ConnectCmd),
    /// Whether Claude Code and Codex are connected
    Connections,
    /// For a coding agent's hooks: reads the event on stdin, prints nothing, always exits 0
    Hook(HookCmd),
}

/// The coding agents `connect` knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AgentArg {
    ClaudeCode,
    Codex,
}

impl AgentArg {
    pub fn agent(self) -> kb_hooks::Agent {
        match self {
            Self::ClaudeCode => kb_hooks::Agent::ClaudeCode,
            Self::Codex => kb_hooks::Agent::Codex,
        }
    }
}

#[derive(Args, Debug, Clone)]
pub struct ConnectCmd {
    /// claude-code or codex
    #[arg(value_enum)]
    pub agent: AgentArg,
    /// Do it without asking (needed when no one is at a terminal)
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Only say what would change
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug, Clone)]
pub struct HookCmd {
    /// claude-code or codex
    pub agent: String,
    /// Internal: deliver an event read earlier, starting Keymotion first
    #[arg(long, hide = true, value_name = "EVENT")]
    pub deliver: Option<String>,
}

/// How a light looks and how long it stays (`signal` and `light`).
#[derive(Args, Debug, Clone, Default)]
pub struct Look {
    /// Keys: ids (esc, f1, w, 1), ranges (f1-f12, 1-3) or groups (all, f-row, numbers, letters, wasd, arrows,
    /// numpad), comma-separated
    #[arg(long, value_name = "KEYS")]
    pub keys: Option<String>,
    /// A name (red, green, blue, amber, yellow, orange, purple, pink, cyan, white) or #RRGGBB
    #[arg(long, value_name = "COLOR")]
    pub color: Option<String>,
    /// solid, pulse, blink, flash or wave
    #[arg(long, value_name = "EFFECT")]
    pub effect: Option<String>,
    /// Which way a wave runs: left, right, up or down
    #[arg(long, value_name = "DIRECTION")]
    pub direction: Option<String>,
    /// 1 (slow) to 5 (fast)
    #[arg(long, value_name = "1-5")]
    pub speed: Option<u32>,
    /// Run the effect this many times, then end
    #[arg(long, value_name = "N", conflicts_with = "for_")]
    pub times: Option<u32>,
    /// How long it stays: 500ms, 30s, 5m, 1h (at most 2h)
    #[arg(long = "for", value_name = "DURATION")]
    pub for_: Option<String>,
    /// The same id replaces your earlier light instead of adding one
    #[arg(long, value_name = "ID")]
    pub id: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct SignalCmd {
    /// done, attention, error, busy or info
    pub name: String,
    #[command(flatten)]
    pub look: Look,
}

#[derive(Args, Debug, Clone)]
pub struct LightCmd {
    #[command(flatten)]
    pub look: Look,
}

#[derive(Args, Debug, Clone)]
pub struct ProgressCmd {
    /// 0 to 100 (42 or 42%)
    #[arg(allow_hyphen_values = true)]
    pub percent: String,
    /// What is progressing, shown in Keymotion (not on the keys)
    #[arg(long, value_name = "TEXT")]
    pub label: Option<String>,
    /// The row for the bar (default: the user's choice, the F row or the number row)
    #[arg(long, value_name = "KEYS")]
    pub keys: Option<String>,
    /// The bar's color (default: the user's)
    #[arg(long, value_name = "COLOR")]
    pub color: Option<String>,
    /// How long the bar stays after this update (default 10m)
    #[arg(long = "for", value_name = "DURATION")]
    pub for_: Option<String>,
    /// The same id updates your earlier bar
    #[arg(long, value_name = "ID")]
    pub id: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct ClearCmd {
    /// Only this light (default: every light of this source)
    #[arg(long, value_name = "ID")]
    pub id: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct LogCmd {
    /// How many entries, newest first (default 20, at most 200)
    #[arg(long, value_name = "N")]
    pub limit: Option<u32>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum FocusCmd {
    /// Start the user's sequence, or with --for one focus block; a paused timer goes on
    Start {
        /// One focus block this long instead (whole minutes: 25m, 50m, 1h; at most 3h)
        #[arg(long = "for", value_name = "DURATION")]
        for_: Option<String>,
    },
    /// Pause the timer
    Pause,
    /// End the current block now
    Skip,
    /// Stop the series
    Stop,
    /// Where the timer stands (needs no permission)
    Status,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ProfileCmd {
    /// The user's profiles and what each holds
    List,
    /// A profile's per-key lighting on the keys for a while (working memory only)
    Show {
        /// Its name (any case) or id
        name: String,
        /// How long it stays: 30s, 5m (default 60s, at most 2h)
        #[arg(long = "for", value_name = "DURATION")]
        for_: Option<String>,
    },
    /// Write a profile's lighting to the keyboard (never its key mapping or macros)
    Apply {
        /// Its name (any case) or id
        name: String,
        /// Needed: this writes the keyboard's memory (the user must allow profile writes)
        #[arg(long)]
        write_flash: bool,
    },
}

#[derive(Args, Debug, Clone)]
pub struct CardCmd {
    /// The text: at most 60 characters on at most 2 lines (\n between them)
    pub text: Option<String>,
    /// A title above it, at most 24 characters
    #[arg(long, value_name = "TEXT")]
    pub title: Option<String>,
    /// A picture: PNG, JPEG, GIF or WebP, at most 40 KiB (shown still; 240x135 fits the screen)
    #[arg(long, value_name = "FILE")]
    pub image: Option<PathBuf>,
    /// An accent color for the card
    #[arg(long, value_name = "COLOR")]
    pub color: Option<String>,
    /// How long it stays before the user's own picture comes back (default 5m; 30s to 2h)
    #[arg(long = "for", value_name = "DURATION")]
    pub for_: Option<String>,
    /// Needed: a card writes the screen's memory (the user must allow screen cards)
    #[arg(long)]
    pub write_flash: bool,
    /// Wait until it is on the screen (at most 60s; it may wait for the screen's budget)
    #[arg(long, value_name = "DURATION")]
    pub wait: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct SceneCmd {
    /// The scene: a JSON file (- reads standard input) with a list of steps, or
    /// {"steps": [...], "repeat": N, "for": "2m"}; a step is
    /// {"keys": "f1-f4", "color": "red", "effect": "pulse", "at": "0ms", "for": "500ms"}
    pub file: String,
    /// Keep it as your pattern of this name (a-z, 0-9, - and _): pattern play NAME
    #[arg(long, value_name = "NAME")]
    pub save: Option<String>,
    /// With --save: keep it without playing it now
    #[arg(long)]
    pub no_play: bool,
    /// How many times it runs (default 1); 0 loops it until --for
    #[arg(long, value_name = "N")]
    pub repeat: Option<u32>,
    /// How long it plays at most (at most 2h)
    #[arg(long = "for", value_name = "DURATION")]
    pub for_: Option<String>,
    /// The same id replaces your earlier light or scene
    #[arg(long)]
    pub id: Option<String>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PatternCmd {
    /// Your saved patterns (every session of your coding app sees them)
    List,
    /// Play one by its name
    Play {
        name: String,
        /// Instead of its saved number of runs; 0 loops it until --for
        #[arg(long, value_name = "N")]
        repeat: Option<u32>,
        /// Instead of its saved length (at most 2h)
        #[arg(long = "for", value_name = "DURATION")]
        for_: Option<String>,
        /// The same id replaces your earlier light or scene
        #[arg(long)]
        id: Option<String>,
    },
    /// Forget one
    Delete { name: String },
}

#[derive(Args, Debug, Clone)]
pub struct SpellCmd {
    /// The word or words: a-z, 0-9, - = , . / ; ' [ ] \ and `; a space is a short pause
    pub text: String,
    /// Default: white
    #[arg(long)]
    pub color: Option<String>,
    /// 1 (slow) to 5 (fast); default 3
    #[arg(long, value_name = "1-5")]
    pub speed: Option<u32>,
    /// How many times it is written (1 to 10)
    #[arg(long, value_name = "N")]
    pub times: Option<u32>,
    /// The same id replaces your earlier light or scene
    #[arg(long)]
    pub id: Option<String>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum SettingsCmd {
    /// Every setting, or those under KEY (e.g. signals.done)
    Get { key: Option<String> },
    /// Change one setting, e.g. signals.done.color green; the user can undo it in Keymotion
    Set {
        key: String,
        #[arg(allow_hyphen_values = true)]
        value: String,
    },
}

/// What a command does.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    /// Ask the app; start it first when it is not running (unless
    /// `--no-start`).
    Call(Command),
    /// Ask the app when it runs; when it does not, say so (exit 0).
    Status,
    Version,
    Guide,
    Doctor,
    Start,
    Connect {
        agent: kb_hooks::Agent,
        yes: bool,
        dry_run: bool,
    },
    Disconnect {
        agent: kb_hooks::Agent,
        yes: bool,
        dry_run: bool,
    },
    Connections,
    /// Hook mode, reached through clap (`main` reads it first otherwise).
    Hook {
        agent: String,
        deliver: Option<String>,
    },
}

impl Cmd {
    /// The command as JSON output names it.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Keys => "keys",
            Self::Signal(_) => "signal",
            Self::Light(_) => "light",
            Self::Progress(_) => "progress",
            Self::Clear(_) => "clear",
            Self::Log(_) => "log",
            Self::Settings(SettingsCmd::Get { .. }) => "settings get",
            Self::Settings(SettingsCmd::Set { .. }) => "settings set",
            Self::Focus(_) => "focus",
            Self::Profile(_) => "profile",
            Self::Card(_) => "card",
            Self::Scene(_) => "scene",
            Self::Pattern(_) => "pattern",
            Self::Spell(_) => "spell",
            Self::Version => "version",
            Self::Guide => "guide",
            Self::Doctor => "doctor",
            Self::Start => "start",
            Self::Connect(_) => "connect",
            Self::Disconnect(_) => "disconnect",
            Self::Connections => "connections",
            Self::Hook(_) => "hook",
        }
    }

    /// What to do, with every value checked as the app will check it.
    pub fn plan(&self) -> Result<Plan, AgentError> {
        let command = match self {
            Self::Status => return Ok(Plan::Status),
            Self::Version => return Ok(Plan::Version),
            Self::Guide => return Ok(Plan::Guide),
            Self::Doctor => return Ok(Plan::Doctor),
            Self::Start => return Ok(Plan::Start),
            Self::Connect(c) => return Ok(Plan::Connect { agent: c.agent.agent(), yes: c.yes, dry_run: c.dry_run }),
            Self::Disconnect(c) => {
                return Ok(Plan::Disconnect { agent: c.agent.agent(), yes: c.yes, dry_run: c.dry_run })
            }
            Self::Connections => return Ok(Plan::Connections),
            Self::Hook(h) => return Ok(Plan::Hook { agent: h.agent.clone(), deliver: h.deliver.clone() }),
            Self::Keys => Command::Keys(KeysArgs {}),
            Self::Signal(s) => {
                let l = &s.look;
                let args = SignalArgs {
                    name: s.name.clone(),
                    id: l.id.clone(),
                    keys: l.keys.clone(),
                    color: l.color.clone(),
                    effect: l.effect.clone(),
                    direction: l.direction.clone(),
                    speed: l.speed,
                    times: l.times,
                    for_ms: for_ms(l.for_.as_deref())?,
                };
                args.validate()?;
                Command::Signal(args)
            }
            Self::Light(c) => {
                let l = &c.look;
                let args = LightArgs {
                    id: l.id.clone(),
                    keys: l.keys.clone(),
                    color: l.color.clone().unwrap_or_default(),
                    effect: l.effect.clone(),
                    direction: l.direction.clone(),
                    speed: l.speed,
                    times: l.times,
                    for_ms: for_ms(l.for_.as_deref())?,
                };
                args.validate()?;
                Command::Light(args)
            }
            Self::Progress(p) => {
                let args = ProgressArgs {
                    percent: Some(percent(&p.percent)?),
                    label: p.label.clone(),
                    id: p.id.clone(),
                    keys: p.keys.clone(),
                    color: p.color.clone(),
                    for_ms: for_ms(p.for_.as_deref())?,
                };
                args.validate()?;
                Command::Progress(args)
            }
            Self::Clear(c) => {
                let args = ClearArgs { id: c.id.clone() };
                args.validate()?;
                Command::Clear(args)
            }
            Self::Log(l) => {
                let args = LogArgs { limit: l.limit };
                args.validate()?;
                Command::Log(args)
            }
            Self::Settings(SettingsCmd::Get { key }) => {
                let args = SettingsGetArgs { key: key.clone() };
                args.validate()?;
                Command::SettingsGet(args)
            }
            Self::Settings(SettingsCmd::Set { key, value }) => {
                let args = SettingsSetArgs { values: [(key.clone(), setting_value(value))].into_iter().collect() };
                args.validate()?;
                Command::SettingsSet(args)
            }
            Self::Focus(f) => {
                let (action, for_) = match f {
                    FocusCmd::Start { for_ } => ("start", for_.as_deref()),
                    FocusCmd::Pause => ("pause", None),
                    FocusCmd::Skip => ("skip", None),
                    FocusCmd::Stop => ("stop", None),
                    FocusCmd::Status => ("status", None),
                };
                let args = FocusArgs { action: action.into(), for_ms: for_ms(for_)? };
                args.validate()?;
                Command::Focus(args)
            }
            Self::Profile(p) => {
                let args = match p {
                    ProfileCmd::List => ProfileArgs { action: "list".into(), ..Default::default() },
                    ProfileCmd::Show { name, for_ } => ProfileArgs {
                        action: "show".into(),
                        name: Some(name.clone()),
                        for_ms: for_ms(for_.as_deref())?,
                        write_flash: false,
                    },
                    ProfileCmd::Apply { name, write_flash } => ProfileArgs {
                        action: "apply".into(),
                        name: Some(name.clone()),
                        for_ms: None,
                        write_flash: *write_flash,
                    },
                };
                args.validate()?;
                Command::Profile(args)
            }
            Self::Card(c) => {
                let image = c.image.as_deref().map(read_image).transpose()?;
                let args = CardArgs {
                    text: c.text.as_deref().map(unescape_lines),
                    title: c.title.clone(),
                    image,
                    color: c.color.clone(),
                    for_ms: for_ms(c.for_.as_deref())?,
                    write_flash: c.write_flash,
                    wait_ms: for_ms(c.wait.as_deref())?,
                };
                args.validate()?;
                Command::Card(args)
            }
            Self::Scene(c) => {
                let mut args = read_scene(&c.file)?;
                if c.repeat.is_some() {
                    args.repeat = c.repeat;
                }
                if let Some(ms) = for_ms(c.for_.as_deref())? {
                    args.for_ms = Some(ms);
                }
                args.id.clone_from(&c.id);
                args.save.clone_from(&c.save);
                args.play = c.no_play.then_some(false);
                args.validate()?;
                Command::Scene(args)
            }
            Self::Pattern(p) => {
                let args = match p {
                    PatternCmd::List => PatternArgs { action: "list".into(), ..Default::default() },
                    PatternCmd::Play { name, repeat, for_, id } => PatternArgs {
                        action: "play".into(),
                        name: Some(name.clone()),
                        repeat: *repeat,
                        for_ms: for_ms(for_.as_deref())?,
                        id: id.clone(),
                    },
                    PatternCmd::Delete { name } => {
                        PatternArgs { action: "delete".into(), name: Some(name.clone()), ..Default::default() }
                    }
                };
                args.validate()?;
                Command::Pattern(args)
            }
            Self::Spell(c) => {
                let args = SpellArgs {
                    text: c.text.clone(),
                    color: c.color.clone(),
                    speed: c.speed,
                    times: c.times,
                    id: c.id.clone(),
                };
                args.validate()?;
                Command::Spell(args)
            }
        };
        Ok(Plan::Call(command))
    }
}

/// The most a scene file may have.
const MAX_SCENE_FILE_BYTES: u64 = 32 * 1024;

/// A scene from its file (`-`: standard input): a list of steps, or an
/// object with `steps` and, if it likes, `repeat` and `for`. In a step,
/// `at` and `for` are durations (`300ms`, `2s`) or numbers of
/// milliseconds; `keys`, `color`, `effect`, `direction` and `speed` are as
/// `light` takes them.
pub fn read_scene(file: &str) -> Result<SceneArgs, AgentError> {
    use std::io::Read;
    let bad = |why: String| {
        AgentError::new(ErrorCode::BadValue, why).with_hint(
            "A scene is JSON: [{\"keys\":\"f1\",\"color\":\"red\",\"for\":\"500ms\"}, ...], or {\"steps\":[...],\"repeat\":2}.",
        )
    };
    let mut bytes = Vec::new();
    let read = if file == "-" {
        std::io::stdin().take(MAX_SCENE_FILE_BYTES + 1).read_to_end(&mut bytes)
    } else {
        std::fs::File::open(file).and_then(|f| f.take(MAX_SCENE_FILE_BYTES + 1).read_to_end(&mut bytes))
    };
    read.map_err(|e| bad(format!("The scene {} cannot be read: {e}.", kb_agent::values::shown(file))))?;
    if bytes.len() as u64 > MAX_SCENE_FILE_BYTES {
        return Err(AgentError::new(ErrorCode::TooLarge, "A scene file has at most 32 KiB."));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| bad(format!("The scene is not JSON: {e}.")))?;
    scene_from_json(&value).map_err(bad)
}

/// [`read_scene`]'s reading, from JSON.
pub fn scene_from_json(value: &Value) -> Result<SceneArgs, String> {
    let (steps, top) = match value {
        Value::Array(steps) => (steps, None),
        Value::Object(top) => match top.get("steps") {
            Some(Value::Array(steps)) => (steps, Some(top)),
            _ => return Err("The scene has no \"steps\" list.".into()),
        },
        _ => return Err("A scene is a list of steps, or an object with \"steps\".".into()),
    };
    let mut args = SceneArgs::default();
    if let Some(top) = top {
        for (key, v) in top {
            match key.as_str() {
                "steps" => {}
                "repeat" => args.repeat = Some(whole(v, "repeat")?),
                "for" | "for_ms" => args.for_ms = Some(millis(v, key)?),
                other => return Err(format!("A scene has no \"{other}\"; it has steps, repeat and for.")),
            }
        }
    }
    for (n, step) in steps.iter().enumerate() {
        let at = |why: String| format!("Step {}: {why}", n + 1);
        let Value::Object(fields) = step else { return Err(at("a step is an object.".into())) };
        let mut s = SceneStepArgs::default();
        for (key, v) in fields {
            let text = || v.as_str().map(str::to_string).ok_or_else(|| at(format!("\"{key}\" is text.")));
            match key.as_str() {
                "keys" => s.keys = Some(text()?),
                "color" => s.color = text()?,
                "effect" => s.effect = Some(text()?),
                "direction" => s.direction = Some(text()?),
                "speed" => s.speed = Some(whole(v, "speed").map_err(at)?),
                "at" | "at_ms" => s.at_ms = Some(millis(v, key).map_err(at)?),
                "for" | "for_ms" => s.for_ms = Some(millis(v, key).map_err(at)?),
                other => {
                    return Err(at(format!(
                        "a step has no \"{other}\"; it has keys, color, effect, direction, speed, at and for."
                    )))
                }
            }
        }
        args.steps.push(s);
    }
    Ok(args)
}

/// A whole number from JSON.
fn whole(v: &Value, key: &str) -> Result<u32, String> {
    v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| format!("\"{key}\" is a whole number."))
}

/// Milliseconds from JSON: a number, or a duration as text.
fn millis(v: &Value, key: &str) -> Result<u64, String> {
    match v {
        Value::Number(n) => n.as_u64().ok_or_else(|| format!("\"{key}\" is a whole number of milliseconds.")),
        Value::String(t) => parse_duration(t).map(duration_ms).map_err(|e| format!("\"{key}\": {}", e.message)),
        _ => Err(format!("\"{key}\" is a duration such as \"500ms\", or milliseconds.")),
    }
}

/// A card's text from the command line: `\n` (backslash, n) starts the
/// second line, as typed in any shell.
fn unescape_lines(text: &str) -> String {
    text.replace("\\n", "\n")
}

/// A picture file for `card --image`, checked (format, size, sides) and
/// as base64. Only its first 40 KiB and one byte are read.
fn read_image(path: &std::path::Path) -> Result<String, AgentError> {
    use std::io::Read;
    let shown = path.display().to_string();
    let file = std::fs::File::open(path).map_err(|e| {
        AgentError::new(
            ErrorCode::BadValue,
            format!("The picture {} cannot be read: {e}.", kb_agent::values::shown(&shown)),
        )
    })?;
    let mut bytes = Vec::new();
    let limit = kb_agent::image::MAX_IMAGE_BYTES as u64 + 1;
    file.take(limit).read_to_end(&mut bytes).map_err(|e| {
        AgentError::new(
            ErrorCode::BadValue,
            format!("The picture {} cannot be read: {e}.", kb_agent::values::shown(&shown)),
        )
    })?;
    kb_agent::image::check(&bytes)?;
    Ok(kb_agent::image::encode(&bytes))
}

/// `status` as a command, for the places that ask the app for it.
pub fn status_command() -> Command {
    Command::Status(StatusArgs {})
}

/// `--for` in milliseconds, as the pipe carries it.
fn for_ms(text: Option<&str>) -> Result<Option<u64>, AgentError> {
    text.map(|t| parse_duration(t).map(duration_ms)).transpose()
}

/// `42`, `42.5` or `42%`.
fn percent(text: &str) -> Result<f64, AgentError> {
    let t = text.trim();
    let number = t.strip_suffix('%').unwrap_or(t).trim();
    number.parse::<f64>().ok().filter(|p| p.is_finite()).ok_or_else(|| {
        AgentError::new(ErrorCode::BadValue, format!("'{}' is not a percent.", kb_agent::values::shown(text)))
            .with_hint("Give a number from 0 to 100, e.g. 40.")
    })
}

/// What `settings set` sends for a typed value: `true` and `false` as
/// such, anything else as text; the app reads numbers, durations, colors
/// and times from text.
fn setting_value(text: &str) -> Value {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::String(text.trim().to_string()),
    }
}

/// What came of the command line.
#[derive(Debug)]
pub enum Parsed {
    /// `--help` or `--version`: print clap's text (in a JSON object with
    /// `--json`), exit 0.
    Print {
        json: bool,
        clap: clap::Error,
    },
    /// clap refused the command line: exit 64.
    Usage {
        json: bool,
        command: Option<String>,
        error: AgentError,
        clap: clap::Error,
    },
    Run(Box<Invocation>),
}

/// A command line clap took.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub global: Global,
    pub command: Cmd,
}

/// Read the command line (`args` starts with the program).
pub fn parse<I, T>(args: I, env: &Env) -> Parsed
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    match Cli::try_parse_from(&args) {
        Ok(cli) => Parsed::Run(Box::new(Invocation { global: cli.global, command: cli.command })),
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            Parsed::Print { json: env.json() || wants_json(&args), clap: e }
        }
        Err(e) => {
            let rendered = e.render().to_string();
            let message = rendered
                .lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| sentence(l.trim().strip_prefix("error:").unwrap_or(l).trim()))
                .filter(|m| m.len() > 1)
                .unwrap_or_else(|| "The command line is not understood.".to_string());
            let message = if e.kind() == ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand {
                "Name a command.".to_string()
            } else {
                message
            };
            let hint = rendered
                .lines()
                .find_map(|l| l.trim().strip_prefix("tip:").map(|t| format!("{} {HINT}", sentence(t.trim()))))
                .unwrap_or_else(|| HINT.to_string());
            Parsed::Usage {
                json: env.json() || wants_json(&args),
                command: command_word(&args),
                error: AgentError::new(ErrorCode::BadArgument, message).with_hint(hint),
                clap: e,
            }
        }
    }
}

/// `a tip` → `A tip.`
fn sentence(text: &str) -> String {
    let mut out: String = text.chars().take(1).flat_map(char::to_uppercase).chain(text.chars().skip(1)).collect();
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

/// `--json` given, before a `--`.
fn wants_json(args: &[OsString]) -> bool {
    args.iter().skip(1).take_while(|a| *a != "--").any(|a| a == "--json")
}

/// The command named on a line clap refused, for the JSON answer: the
/// first word that is a command (`settings get` with its word).
fn command_word(args: &[OsString]) -> Option<String> {
    let cli = Cli::command();
    let words: Vec<String> = args.iter().skip(1).filter_map(|a| a.to_str().map(str::to_string)).collect();
    let at = words.iter().position(|w| cli.find_subcommand(w).is_some_and(|c| c.get_name() == w))?;
    let word = &words[at];
    if word == "settings" {
        if let Some(next) = words.get(at + 1).filter(|n| *n == "get" || *n == "set") {
            return Some(format!("settings {next}"));
        }
    }
    Some(word.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kb_agent::exit;
    use serde_json::json;

    fn run(line: &str) -> Parsed {
        let args = std::iter::once("keymotion-cli").chain(line.split_whitespace());
        parse(args, &Env::default())
    }

    fn invocation(line: &str) -> Invocation {
        match run(line) {
            Parsed::Run(inv) => *inv,
            other => panic!("{line}: {other:?}"),
        }
    }

    fn plan(line: &str) -> Result<Plan, AgentError> {
        invocation(line).command.plan()
    }

    fn call(line: &str) -> Command {
        match plan(line) {
            Ok(Plan::Call(command)) => command,
            other => panic!("{line}: {other:?}"),
        }
    }

    fn usage(line: &str) -> (AgentError, Option<String>, bool) {
        match run(line) {
            Parsed::Usage { error, command, json, .. } => (error, command, json),
            other => panic!("{line}: {other:?}"),
        }
    }

    #[test]
    fn the_version_line_names_kb_agents_protocol() {
        let p = kb_agent::SUPPORTED_PROTOCOLS;
        assert_eq!(p.min, p.max, "say the range in VERSION_LINE once the CLI speaks several versions");
        assert_eq!(VERSION_LINE, format!("{} (protocol {})", env!("CARGO_PKG_VERSION"), p.max));
    }

    #[test]
    fn commands_become_protocol_requests() {
        assert_eq!(call("keys"), Command::Keys(KeysArgs {}));
        assert_eq!(
            call("signal done --color amber --keys f1-f4 --for 5m --id build"),
            Command::Signal(SignalArgs {
                name: "done".into(),
                id: Some("build".into()),
                keys: Some("f1-f4".into()),
                color: Some("amber".into()),
                for_ms: Some(300_000),
                ..Default::default()
            })
        );
        assert_eq!(
            call("light --keys f1-f12 --color green --effect wave --direction left --speed 4 --times 1"),
            Command::Light(LightArgs {
                keys: Some("f1-f12".into()),
                color: "green".into(),
                effect: Some("wave".into()),
                direction: Some("left".into()),
                speed: Some(4),
                times: Some(1),
                ..Default::default()
            })
        );
        assert_eq!(
            call("progress 42% --label Tests --keys numbers --color blue --id t"),
            Command::Progress(ProgressArgs {
                percent: Some(42.0),
                label: Some("Tests".into()),
                id: Some("t".into()),
                keys: Some("numbers".into()),
                color: Some("blue".into()),
                for_ms: None,
            })
        );
        assert_eq!(call("clear"), Command::Clear(ClearArgs { id: None }));
        assert_eq!(call("clear --id build"), Command::Clear(ClearArgs { id: Some("build".into()) }));
        assert_eq!(call("log --limit 5"), Command::Log(LogArgs { limit: Some(5) }));
        assert_eq!(call("settings get"), Command::SettingsGet(SettingsGetArgs { key: None }));
        assert_eq!(
            call("settings get signals.done"),
            Command::SettingsGet(SettingsGetArgs { key: Some("signals.done".into()) })
        );
        assert_eq!(
            call("settings set night.on TRUE"),
            Command::SettingsSet(SettingsSetArgs { values: [("night.on".into(), json!(true))].into_iter().collect() })
        );
        assert_eq!(
            call("settings set signals.done.speed 4"),
            Command::SettingsSet(SettingsSetArgs {
                values: [("signals.done.speed".into(), json!("4"))].into_iter().collect()
            })
        );
        for (line, expected) in [
            ("status", Plan::Status),
            ("version", Plan::Version),
            ("guide", Plan::Guide),
            ("doctor", Plan::Doctor),
            ("start", Plan::Start),
        ] {
            assert_eq!(plan(line).unwrap(), expected, "{line}");
        }
    }

    #[test]
    fn global_options_go_before_or_after_the_command() {
        for line in [
            "--json --source claude-code --timeout 2s --no-start -q status",
            "status --json --source claude-code --timeout 2s --no-start --quiet",
        ] {
            let inv = invocation(line);
            assert!(inv.global.json && inv.global.no_start && inv.global.quiet, "{line}");
            assert_eq!(inv.global.source.as_deref(), Some("claude-code"));
            assert_eq!(inv.global.timeout.as_deref(), Some("2s"));
        }
        assert_eq!(invocation("--log debug keys").global.log, Some(LogLevel::Debug));
    }

    #[test]
    fn bad_values_are_data_errors_with_kb_agents_words() {
        for (line, code) in [
            ("signal sparkle", ErrorCode::BadSignal),
            ("light --color teal", ErrorCode::BadColor),
            ("light --color red --keys f1,,f2", ErrorCode::BadKeys),
            ("light --color red --effect glitter", ErrorCode::BadEffect),
            ("light --color red --for 3h", ErrorCode::BadDuration),
            ("light --color red --for soon", ErrorCode::BadDuration),
            ("light --color red --speed 9", ErrorCode::BadValue),
            ("progress 150", ErrorCode::BadValue),
            ("progress lots", ErrorCode::BadValue),
            ("progress -5", ErrorCode::BadValue),
            ("clear --id Bad/Id", ErrorCode::BadId),
            ("log --limit 500", ErrorCode::BadValue),
            ("settings get Bad:Key", ErrorCode::BadValue),
        ] {
            let e = plan(line).unwrap_err();
            assert_eq!(e.code, code, "{line}: {e:?}");
            assert_eq!(e.exit_code(), exit::DATA, "{line}");
        }
        // What does not go together is a usage error, as in kb-agent.
        let e = plan("light").unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::BadArgument, exit::USAGE));
        assert!(e.hint.is_some());
        let e = plan("signal done --speed 2").unwrap_err();
        assert_eq!(e.exit_code(), exit::USAGE);
    }

    #[test]
    fn what_clap_refuses_is_64_never_2() {
        for line in [
            "",
            "sparkle",
            "light --colour red",
            "signal",
            "progress",
            "settings",
            "settings set night.on",
            "light --color red --times 2 --for 5s",
            "light --color red --speed fast",
            "log --limit many",
            "--log loud status",
            "status extra",
        ] {
            let (error, _, _) = usage(line);
            assert_eq!(error.code, ErrorCode::BadArgument, "{line}");
            assert_eq!(error.exit_code(), exit::USAGE, "{line}");
            assert!(!error.message.is_empty() && !error.message.starts_with("error"), "{line}: {}", error.message);
            assert!(error.hint.as_deref().is_some_and(|h| h.contains("keymotion-cli")), "{line}");
        }
        let (error, command, json) = usage("light --colour red --json");
        assert!(error.message.contains("--colour"), "{}", error.message);
        assert_eq!((command.as_deref(), json), (Some("light"), true));
        assert_eq!(usage("--json settings set x").1.as_deref(), Some("settings set"));
        assert_eq!(usage("sparkle").1, None);
        // After `--`, --json is a value, not the option.
        let env = Env { output: Some("JSON".into()), ..Env::default() };
        assert!(matches!(parse(["keymotion-cli", "nope"], &env), Parsed::Usage { json: true, .. }));
    }

    #[test]
    fn help_and_version_print_and_succeed() {
        for line in ["--help", "-h", "signal --help", "help light", "--version", "-V"] {
            assert!(matches!(run(line), Parsed::Print { json: false, .. }), "{line}");
        }
        assert!(matches!(run("--help --json"), Parsed::Print { json: true, .. }));
        assert!(matches!(run("--json help light"), Parsed::Print { json: true, .. }));
        let Parsed::Print { clap: e, .. } = run("--version") else { unreachable!() };
        assert!(e.render().to_string().contains(VERSION_LINE));
    }

    #[cfg(any(debug_assertions, feature = "test-pipe"))]
    #[test]
    fn the_test_pipe_must_look_like_one() {
        let env = |p: &str| Env { test_pipe: Some(p.into()), ..Env::default() };
        assert_eq!(env(r"\\.\pipe\keymotion-cli-test-1").test_pipe(), Some(r"\\.\pipe\keymotion-cli-test-1"));
        for bad in [r"\\.\pipe\keymotion-agent-S-1-5-21-1-1", r"\\.\pipe\keymotion-cli-test-", r"C:\x", ""] {
            assert_eq!(env(bad).test_pipe(), None, "{bad}");
        }
        assert!(Env { output: Some(" Json ".into()), ..Env::default() }.json());
        assert!(!Env { output: Some("text".into()), ..Env::default() }.json());
    }
}
