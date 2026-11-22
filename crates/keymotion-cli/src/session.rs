//! Reaching the app: the pipe, one deadline for connecting and the call,
//! and starting the app when a command needs it.

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
    pub no_start: bool,
    pub log: Logger,
    /// A test's pipe (`KEYMOTION_CLI_TEST_PIPE`): the app is then never
    /// started, and the instance lock is not looked at.
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
        if env.test_pipe.is_some() && env.test_pipe().is_none() {
            log.warn(|| "KEYMOTION_CLI_TEST_PIPE is not a test pipe name; ignored".to_string());
        }
        Ok(Self { source, timeout, no_start: global.no_start, log, test_pipe: env.test_pipe().map(str::to_string) })
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
        launch::choose(&launch::facts())
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

/// Connect within the timeout. When nobody serves the pipe and `may_start`
/// (and not `--no-start`), start the app and wait for it.
pub fn connect(ctx: &Ctx, may_start: bool) -> Result<Connected, AgentError> {
    let pipe = ctx.pipe()?;
    let began = Instant::now();
    match hello(ctx, &pipe, ctx.timeout) {
        Ok(client) => {
            ctx.log.debug(|| format!("connected to {pipe} in {} ms", began.elapsed().as_millis()));
            Ok(Connected { client, deadline: began + ctx.timeout, started: None })
        }
        Err(e) if e.code == ErrorCode::AppNotRunning && may_start && !ctx.no_start => start_and_connect(ctx, &pipe),
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

fn start_and_connect(ctx: &Ctx, pipe: &str) -> Result<Connected, AgentError> {
    let began = Instant::now();
    let running = ctx.app_running();
    let (method, limit) = if running {
        ctx.log.info(|| "Keymotion runs but its pipe is not open yet; waiting".to_string());
        (None, launch::RUNNING_WAIT)
    } else {
        let method = ctx.start_method()?;
        ctx.log.info(|| format!("starting Keymotion: {}", method.describe()));
        launch::start(&method)?;
        (Some(method.name()), launch::START_WAIT)
    };
    let until = began + limit;
    loop {
        let left = until.saturating_duration_since(Instant::now());
        match hello(ctx, pipe, ctx.timeout.min(left.max(POLL))) {
            Ok(client) => {
                let waited_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
                ctx.log.debug(|| format!("Keymotion answered after {waited_ms} ms"));
                // The command gets its own full timeout from here.
                return Ok(Connected {
                    client,
                    deadline: Instant::now() + ctx.timeout,
                    started: Some(Started { method, waited_ms }),
                });
            }
            Err(e) if e.code == ErrorCode::AppNotRunning && !left.is_zero() => std::thread::sleep(POLL.min(left)),
            Err(e) if e.code == ErrorCode::AppNotRunning => break,
            Err(e) => return Err(e),
        }
    }
    Err(if running {
        AgentError::new(ErrorCode::AppNotRunning, "Keymotion is running but does not answer agents.")
            .with_hint("This Keymotion may be older than keymotion-cli: update it, or restart it.")
    } else {
        AgentError::new(ErrorCode::AppNotRunning, format!("Keymotion did not start within {}s.", limit.as_secs()))
            .with_hint("Start Keymotion, then try again; keymotion-cli doctor shows more.")
    })
}
