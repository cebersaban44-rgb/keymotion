//! Reaching the app: the pipe, one deadline for connecting and the call,
//! and starting the app when a command needs it.
//!
//! With `--timeout`, that time bounds the whole run, starting the app
//! included: a hook with a time limit is never killed for waiting on the
//! app. Without it (and for `start`), a start may take up to 15 seconds and
//! the call then has the default timeout.

use std::time::{Duration, Instant};

use kb_agent::transport::{current_pipe_name, Client};
use kb_agent::values::parse_duration;
use kb_agent::{AgentError, ErrorCode, HelloData, SourceName};

use crate::cli::{Env, Global, DEFAULT_TIMEOUT, MAX_TIMEOUT_MS};
use crate::launch::{self, Method};
use crate::output::Logger;

/// Between two looks for the pipe of an app that is starting.
const POLL: Duration = Duration::from_millis(100);

/// What every command runs with.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub source: SourceName,
    /// For connecting and the call together.
    pub timeout: Duration,
    /// `--timeout` was given: it then bounds the whole run, the start of
    /// the app included.
    pub timeout_given: bool,
    pub no_start: bool,
    pub log: Logger,
    /// A test's pipe (`KEYMOTION_CLI_TEST_PIPE`, debug builds and the
    /// `test-pipe` feature only): the app is then never started, and the
    /// instance lock is not looked at.
    pub test_pipe: Option<String>,
}

impl Ctx {
    /// The global options and the environment, checked.
    pub fn new(global: &Global, env: &Env, log: Logger) -> Result<Self, AgentError> {
        let source = SourceName::parse(global.source.as_deref().or(env.source.as_deref()).unwrap_or("cli"))?;
        let timeout = parse_duration(global.timeout.as_deref().unwrap_or(DEFAULT_TIMEOUT))?;
        if timeout > Duration::from_millis(MAX_TIMEOUT_MS) {
            return Err(AgentError::new(ErrorCode::BadDuration, "The timeout is too long; use at most 5m."));
        }
        if env.test_pipe_ignored() {
            log.warn(|| "KEYMOTION_CLI_TEST_PIPE is not a test pipe name; ignored".to_string());
        }
        Ok(Self {
            source,
            timeout,
            timeout_given: global.timeout.is_some(),
            no_start: global.no_start,
            log,
            test_pipe: env.test_pipe().map(str::to_string),
        })
    }

    /// The pipe to talk to.
    pub fn pipe(&self) -> Result<String, AgentError> {
        if let Some(test) = &self.test_pipe {
            return Ok(test.clone());
        }
        current_pipe_name()
            .map_err(|e| AgentError::new(ErrorCode::Internal, format!("No pipe name for this Windows user: {e}")))
    }

    /// Whether a copy of Keymotion runs, open pipe or not.
    pub fn app_running(&self) -> bool {
        self.test_pipe.is_none() && launch::app_running()
    }

    /// How the app would be started.
    pub fn start_method(&self) -> Result<Method, AgentError> {
        if self.test_pipe.is_some() {
            return Ok(Method::Test);
        }
        launch::choose(&launch::facts(), launch::cli_version())
    }

    /// A running Store copy too old to answer agents.
    pub fn old_copy_running(&self) -> Option<[u16; 3]> {
        if self.test_pipe.is_some() {
            return None;
        }
        launch::old_store_copy_running()
    }
}

/// A connection, after `hello`.
#[derive(Debug)]
pub struct Connected {
    pub client: Client,
    /// The call must be answered by then.
    pub deadline: Instant,
    /// Set when this run started the app.
    pub started: Option<Started>,
}

impl Connected {
    pub fn app(&self) -> &HelloData {
        self.client.app()
    }

    /// The time left for the call.
    pub fn left(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}

/// How this run got the app going.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    /// The method's name ([`Method::name`]); `None` when the app was already
    /// running and only its pipe was awaited.
    pub method: Option<&'static str>,
    pub waited_ms: u64,
}

/// What a command does when the app is not running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfNotRunning {
    /// Say so (`status`, `version`, `doctor`).
    Report,
    /// Start it; with `--timeout`, all within that time.
    Start,
    /// Start it and give it the full start time (`start`).
    StartAndWait,
}

/// Connect within the timeout. When nobody serves the pipe, start the app
/// as `mode` says (never with `--no-start`) and wait for it.
pub fn connect(ctx: &Ctx, mode: IfNotRunning) -> Result<Connected, AgentError> {
    let pipe = ctx.pipe()?;
    let began = Instant::now();
    let deadline = began + ctx.timeout;
    let may_start = mode != IfNotRunning::Report;
    match hello(ctx, &pipe, ctx.timeout) {
        Ok(client) => {
            ctx.log.debug(|| format!("connected to {pipe} in {} ms", began.elapsed().as_millis()));
            Ok(Connected { client, deadline, started: None })
        }
        Err(e) if e.code == ErrorCode::AppNotRunning && may_start && !ctx.no_start => {
            start_and_connect(ctx, &pipe, mode, deadline)
        }
        Err(e) if e.code == ErrorCode::AppNotRunning => Err(not_running(ctx, may_start)),
        Err(e) => Err(e),
    }
}

fn hello(ctx: &Ctx, pipe: &str, timeout: Duration) -> Result<Client, AgentError> {
    Client::connect(pipe, ctx.source.clone(), crate::NAME, crate::VERSION, timeout)
}

/// `app_not_running`, with what to do.
pub fn not_running(ctx: &Ctx, may_start: bool) -> AgentError {
    let e = AgentError::new(ErrorCode::AppNotRunning, "Keymotion is not running.");
    if may_start && ctx.no_start {
        e.with_hint("Start Keymotion, or leave out --no-start so keymotion-cli starts it.")
    } else {
        e.with_hint("Start Keymotion, or run keymotion-cli start.")
    }
}

/// A running Store copy that will never open the pipe.
pub fn old_copy(version: [u16; 3]) -> AgentError {
    let v = version.map(|p| p.to_string()).join(".");
    AgentError::new(
        ErrorCode::AppNotRunning,
        format!(
            "Keymotion {v} from the Microsoft Store is running; it is too old for keymotion-cli {} and does not answer agents.",
            crate::VERSION
        ),
    )
    .with_hint("Update Keymotion from the Microsoft Store.")
}

/// How long the start may take: the start time, and when the timeout is
/// shared, no later than the run's deadline.
fn start_until(began: Instant, limit: Duration, deadline: Instant, shared: bool) -> Instant {
    if shared {
        (began + limit).min(deadline)
    } else {
        began + limit
    }
}

fn start_and_connect(ctx: &Ctx, pipe: &str, mode: IfNotRunning, deadline: Instant) -> Result<Connected, AgentError> {
    let began = Instant::now();
    let shared = ctx.timeout_given && mode == IfNotRunning::Start;
    let running = ctx.app_running();
    let (method, limit) = if running {
        if let Some(old) = ctx.old_copy_running() {
            return Err(old_copy(old));
        }
        ctx.log.info(|| "Keymotion runs but its pipe is not open yet; waiting".to_string());
        (None, launch::RUNNING_WAIT)
    } else {
        let method = ctx.start_method()?;
        ctx.log.info(|| format!("starting Keymotion: {}", method.describe()));
        launch::start(&method)?;
        (Some(method.name()), launch::START_WAIT)
    };
    let until = start_until(began, limit, deadline, shared);
    let mut last: Option<AgentError> = None;
    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match hello(ctx, pipe, ctx.timeout.min(left)) {
            Ok(client) => {
                let waited_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
                ctx.log.debug(|| format!("Keymotion answered after {waited_ms} ms"));
                return Ok(Connected {
                    client,
                    // With --timeout the call has what is left of it;
                    // otherwise its own full timeout from here.
                    deadline: if shared { deadline } else { Instant::now() + ctx.timeout },
                    started: Some(Started { method, waited_ms }),
                });
            }
            // Not up yet, or up but too busy starting to answer: again,
            // until the time is up.
            Err(e) if matches!(e.code, ErrorCode::AppNotRunning | ErrorCode::Timeout | ErrorCode::ConnectionLost) => {
                if e.code == ErrorCode::AppNotRunning {
                    std::thread::sleep(POLL.min(left));
                }
                last = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    if shared && until >= deadline {
        let starting = if running { "" } else { " while it was starting" };
        return Err(AgentError::new(
            ErrorCode::Timeout,
            format!(
                "Keymotion did not answer within the {} timeout{starting}.",
                kb_agent::values::format_duration(ctx.timeout)
            ),
        )
        .with_hint("Try again in a moment, or give a longer --timeout.")
        .with_retry_after(Duration::from_secs(1)));
    }
    Err(match last {
        Some(e) if e.code != ErrorCode::AppNotRunning => e,
        _ if running => AgentError::new(ErrorCode::AppNotRunning, "Keymotion is running but does not answer agents.")
            .with_hint("This Keymotion may be older than keymotion-cli: update it, or restart it."),
        _ => AgentError::new(ErrorCode::AppNotRunning, format!("Keymotion did not start within {}s.", limit.as_secs()))
            .with_hint("Start Keymotion, then try again; keymotion-cli doctor shows more."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_a_timeout_the_start_ends_by_the_deadline() {
        let began = Instant::now();
        let deadline = began + Duration::from_secs(2);
        assert_eq!(start_until(began, launch::START_WAIT, deadline, true), deadline);
        assert_eq!(start_until(began, launch::START_WAIT, deadline, false), began + launch::START_WAIT);
        // A deadline past the start time does not stretch it.
        let late = began + Duration::from_secs(60);
        assert_eq!(start_until(began, launch::RUNNING_WAIT, late, true), began + launch::RUNNING_WAIT);
    }

    #[test]
    fn an_old_running_copy_says_how_to_fix_it() {
        let e = old_copy([1, 1, 0]);
        assert_eq!((e.code, e.exit_code()), (ErrorCode::AppNotRunning, 69));
        assert!(e.message.contains("1.1.0"));
        assert_eq!(e.hint.as_deref(), Some("Update Keymotion from the Microsoft Store."));
    }
}
