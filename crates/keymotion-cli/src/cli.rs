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
//! Later steps add commands here (hook mode, connect and disconnect, focus,
//! profile, card, scene and spell): each is a [`Cmd`] variant with a
//! [`Plan`].

use std::ffi::OsString;

use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use kb_agent::protocol::{
    ClearArgs, KeysArgs, LightArgs, LogArgs, ProgressArgs, SettingsGetArgs, SettingsSetArgs, SignalArgs, StatusArgs,
};
use kb_agent::values::{duration_ms, parse_duration};
use kb_agent::{AgentError, Command, ErrorCode};
use serde_json::Value;

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
}

impl Env {
    pub fn read() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let test_pipe = if TEST_PIPE_BUILD { var(TEST_PIPE_ENV) } else { None };
        Self { output: var(OUTPUT_ENV), source: var(SOURCE_ENV), test_pipe }
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
    /// keymotion-cli's version and protocol, and Keymotion's when it runs
    Version,
    /// A short guide for agents: what to run, examples, exit codes
    Guide,
    /// Check the setup: the app, its pipe, access, the keyboard, PATH
    Doctor,
    /// Start Keymotion without a window and wait until it answers
    Start,
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
            Self::Version => "version",
            Self::Guide => "guide",
            Self::Doctor => "doctor",
            Self::Start => "start",
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
        };
        Ok(Plan::Call(command))
    }
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
