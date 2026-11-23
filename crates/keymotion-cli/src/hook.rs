//! `keymotion-cli hook claude-code|codex`: what the coding agents run on
//! their hook events (`kb_hooks::setup`).
//!
//! * Always exit 0, never a byte on stdout, whatever happens: bad JSON, an
//!   event or agent it does not know, no app, access off, a panic. An exit
//!   code of 2 means "deny", "keep going" or "block" to these agents, and
//!   Codex reads text on stdout as a decision.
//! * Fast: the agent may wait for it (Codex's approval screen does). The
//!   event is read from stdin with a size limit and a deadline (and not at
//!   all from a terminal); the app gets one short request.
//! * When the app is not running, the hook never waits for it: an event that
//!   lights something goes to a copy of keymotion-cli that runs on its own
//!   (no handles, no window, a working folder of its own), which starts
//!   Keymotion as `launch` does and then delivers it. One such copy at a
//!   time: the hooks that come while Keymotion starts queue behind it (a
//!   question answered meanwhile is not lit late). A question answered, a
//!   new prompt, a session's end or a helper agent's finish is not worth
//!   starting the app for; nor is anything when the user quit Keymotion from
//!   its menu or turned agent access off (it stays closed).
//! * Only the event's meaning leaves the process (`kb_hooks::event`): never
//!   the prompt, the agent's messages, the transcript or a tool's input.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use kb_agent::protocol::HookArgs;
use kb_agent::transport::Client;
use kb_agent::{AgentError, Channel, Command, ErrorCode, HookEvent, ServerInfo, SourceName};
use kb_hooks::event::{self, Agent};

use crate::cli::{Env, Global, LogLevel};
use crate::output::Logger;
use crate::session::{self, Ctx, IfNotRunning};

/// The most of stdin read: a tool's whole output may come with the event.
pub const MAX_INPUT: usize = 16 * 1024 * 1024;
/// How long stdin may take.
const READ_WAIT: Duration = Duration::from_secs(2);
/// Connecting and the request, together, when the app runs.
const CALL_WAIT: &str = "1500ms";
/// The same, for the delivering copy and the hooks that talk to it.
const CALL: Duration = Duration::from_millis(1500);
/// How long a hook waits for the delivering copy's pipe to open.
const DELIVERY_WAIT: Duration = Duration::from_secs(1);
/// Once Keymotion answers, the delivering copy relays what comes until
/// nothing came for this long...
const LINGER: Duration = Duration::from_secs(2);
/// ...or this long at most; the hooks then go to Keymotion themselves.
const RELAY_MAX: Duration = Duration::from_secs(30);
/// The most events queued while Keymotion starts (the oldest go first).
const MAX_QUEUE: usize = 256;
/// An event this old by the time Keymotion answers is no news any more.
const MAX_AGE: Duration = Duration::from_secs(60);

/// Hook mode from `main`: `args` are what follows `hook`. Returns when done;
/// the caller exits with 0.
pub fn main(args: &[OsString]) {
    let words: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    let agent = words.first().cloned().unwrap_or_default();
    let deliver = match words.get(1).map(String::as_str) {
        Some("--deliver") => words.get(2).cloned(),
        _ => None,
    };
    run(&agent, deliver.as_deref(), &Env::read());
}

/// Hook mode: nothing it does reaches stdout or the exit code.
pub fn run(agent: &str, deliver: Option<&str>, env: &Env) {
    // A panic's message would go to stderr; the agent shows that to nobody,
    // but there is no reason to write it either.
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(|| {
        let Some(agent) = Agent::parse(agent) else {
            trace(|| format!("not an agent keymotion-cli knows: {}", kb_agent::values::shown(agent)));
            return;
        };
        match deliver {
            Some(token) => deliver_later(env, agent, token),
            None => {
                let input = read_input(MAX_INPUT, READ_WAIT);
                match event::read(agent, &input) {
                    Some(read) => {
                        trace(|| format!("{} bytes: {} from {}", input.len(), read.args.event, read.source));
                        send(env, agent, &read);
                    }
                    None => trace(|| format!("{} bytes: nothing for the lights", input.len())),
                }
            }
        }
    });
}

/// `KEYMOTION_HOOK_LOG=1`: what hook mode did, on stderr (the event's
/// meaning and the app's answer, nothing of the event itself). Agents do
/// not show a hook's stderr when it exits with 0.
pub const LOG_ENV: &str = "KEYMOTION_HOOK_LOG";

fn trace(message: impl FnOnce() -> String) {
    if std::env::var_os(LOG_ENV).is_some_and(|v| !v.is_empty() && v != "0") {
        crate::output::write_stderr(&format!("keymotion-cli hook: {}\n", kb_agent::values::printable(&message())));
    }
}

/// The event on stdin, or nothing: from a terminal (nobody would type an
/// event), past `max` bytes, or not there within `wait`.
fn read_input(max: usize, wait: Duration) -> Vec<u8> {
    if std::io::stdin().is_terminal() {
        return Vec::new();
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::Builder::new().name("stdin".into()).spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::stdin().lock().take(max as u64 + 1).read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    if reader.is_err() {
        return Vec::new();
    }
    // A reader still waiting is left behind: the process ends anyway.
    match rx.recv_timeout(wait) {
        Ok(buf) if buf.len() <= max => buf,
        _ => Vec::new(),
    }
}

/// What every request of hook mode runs with: the session's source, a short
/// time, no start, no messages.
fn ctx(env: &Env, source: &str, timeout: Option<&str>, no_start: bool) -> Option<Ctx> {
    let global = Global {
        source: Some(source.to_string()),
        timeout: timeout.map(str::to_string),
        no_start,
        quiet: true,
        log: Some(LogLevel::Off),
        ..Global::default()
    };
    Ctx::new(&global, env, Logger::new(Some(LogLevel::Off), true)).ok()
}

/// The names the delivering copy goes by, next to the app's pipe: its pipe,
/// and the named mutex that says one runs (or is being started).
struct Names {
    app: String,
    delivery: String,
    lock: String,
}

impl Names {
    fn of(ctx: &Ctx) -> Option<Names> {
        let app = ctx.pipe().ok()?;
        let tail = app.rsplit('\\').next().filter(|t| !t.is_empty())?;
        Some(Names { delivery: format!("{app}-delivery"), lock: format!(r"Local\keymotion-cli-delivery-{tail}"), app })
    }
}

/// Send the event: to the copy that delivers while Keymotion starts, if one
/// runs (it keeps the order), or to Keymotion. When Keymotion is not running
/// and the event is worth starting it for, hand the event to a copy of
/// ourselves that starts it, unless the user closed Keymotion (or turned
/// agent access off): then it stays closed.
fn send(env: &Env, agent: Agent, read: &event::Read) {
    let Some(ctx) = ctx(env, &read.source, Some(CALL_WAIT), true) else {
        trace(|| "not a source name".to_string());
        return;
    };
    let Some(names) = Names::of(&ctx) else { return };
    if kb_winsys::copies::running(&names.lock) && to_delivery(&names, read) {
        trace(|| "Keymotion is starting; the delivering copy has it".to_string());
        return;
    }
    let command = Command::Hook(read.args.clone());
    match session::connect(&ctx, IfNotRunning::Report) {
        Ok(mut conn) => {
            let left = conn.left();
            let answer = conn.client.call_within(&command, left);
            trace(|| match &answer {
                Ok(v) => format!("Keymotion: lit {}, cleared {}", v["lit"], v["cleared"]),
                Err(e) => format!("Keymotion: {}", e.code.as_str()),
            });
            return;
        }
        Err(e) if e.code == ErrorCode::AppNotRunning => {}
        Err(e) => {
            trace(|| format!("not sent: {}", e.code.as_str()));
            return;
        }
    }
    if !read.event().starts_app() {
        trace(|| format!("Keymotion is not running; {} is not worth starting it for", read.args.event));
        return;
    }
    if stopped(env, agent) {
        trace(|| "Keymotion is not running: the user closed it, or turned agent access off".to_string());
        return;
    }
    let Some((lock, existed)) = kb_winsys::copies::hold(&names.lock) else { return };
    if existed {
        // Another hook is starting a copy this very moment.
        let given = to_delivery(&names, read);
        trace(|| format!("a copy is being started; given to it: {given}"));
        return;
    }
    let Some(exe) = delivering_program() else { return };
    let token = encode(read);
    let args = ["hook", agent.id(), "--deliver", token.as_str()];
    let started = kb_winsys::shell::spawn_detached(&exe, &args, &kb_winsys::shell::neutral_dir());
    trace(|| format!("Keymotion is not running; a copy delivers it after starting it: {started:?}"));
    if started.is_ok() {
        // Until the copy serves its pipe, the lock held here says it is
        // coming: an event right behind this one (its answer) waits for it
        // instead of going nowhere.
        wait_for_pipe(&names.delivery, DELIVERY_WAIT);
    }
    drop(lock);
}

/// The program for the delivering copy: by the path that stays put (the
/// Store copy's app execution alias, which gives it the package), else
/// this one.
fn delivering_program() -> Option<PathBuf> {
    crate::connect::this_cli()
        .ok()
        .map(|cli| cli.path)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::current_exe().ok())
}

/// The user quit Keymotion from its menu, or turned agent access off: its
/// note in the agent's skill folder says so (`kb_hooks::setup::mark_stopped`).
fn stopped(env: &Env, agent: Agent) -> bool {
    let Ok(homes) = crate::connect::homes(env) else { return false };
    kb_hooks::setup::hooks_stopped(&homes, agent, kb_winsys::pipe::current_logon_id().ok())
}

/// Hand the event to the delivering copy; `false` when there is none (any
/// more) to take it.
fn to_delivery(names: &Names, read: &event::Read) -> bool {
    if !wait_for_pipe(&names.delivery, DELIVERY_WAIT) {
        return false;
    }
    let Ok(source) = SourceName::parse(&read.source) else { return false };
    match Client::connect(&names.delivery, source, crate::NAME, crate::VERSION, CALL) {
        Ok(mut client) => client.call(&Command::Hook(read.args.clone())).is_ok(),
        Err(_) => false,
    }
}

/// Whether `pipe` is served within `wait`.
fn wait_for_pipe(pipe: &str, wait: Duration) -> bool {
    let until = Instant::now() + wait;
    loop {
        if kb_winsys::pipe::exists(pipe) {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// An event waiting in the delivering copy.
#[derive(Debug, Clone)]
struct Queued {
    source: String,
    args: HookArgs,
    at: Instant,
}

/// The delivering copy's queue: hooks put in, the copy takes out, in order.
#[derive(Default)]
struct Queue {
    state: Mutex<QueueState>,
    came: Condvar,
}

#[derive(Default)]
struct QueueState {
    events: VecDeque<Queued>,
    /// The copy is ending: nothing more is taken (the hook then goes to
    /// Keymotion itself).
    closed: bool,
}

impl Queue {
    fn lock(&self) -> std::sync::MutexGuard<'_, QueueState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `false` once the queue is closed.
    fn push(&self, source: String, args: HookArgs) -> bool {
        let mut q = self.lock();
        if q.closed {
            return false;
        }
        if q.events.len() >= MAX_QUEUE {
            q.events.pop_front();
        }
        q.events.push_back(Queued { source, args, at: Instant::now() });
        self.came.notify_all();
        true
    }

    /// Everything queued, waiting up to `wait` for something to come.
    fn take(&self, wait: Duration) -> Vec<Queued> {
        let q = self.lock();
        let (mut q, _) =
            self.came.wait_timeout_while(q, wait, |q| q.events.is_empty()).unwrap_or_else(|e| e.into_inner());
        q.events.drain(..).collect()
    }

    /// Take nothing more; what was queued.
    fn close(&self) -> Vec<Queued> {
        let mut q = self.lock();
        q.closed = true;
        q.events.drain(..).collect()
    }
}

/// The copy that runs on its own (one at a time): serve the delivering pipe
/// so the hooks that come meanwhile queue behind the first event, start
/// Keymotion (it may take a few seconds), deliver the queue in order
/// without what came after it made it old news, relay a little longer, end.
fn deliver_later(env: &Env, agent: Agent, token: &str) {
    let Some(first) = decode(token) else { return };
    let Some(ctx) = ctx(env, &first.source, None, false) else { return };
    let Some(names) = Names::of(&ctx) else { return };
    let lock = kb_winsys::copies::hold(&names.lock);
    let queue = Arc::new(Queue::default());
    queue.push(first.source.clone(), first.args.clone());
    let into = queue.clone();
    let info = ServerInfo { version: crate::VERSION.to_string(), channel: Channel::Dev };
    let served = kb_agent::transport::serve(&names.delivery, info, move |call, command| match command {
        Command::Hook(args) => {
            args.validate()?;
            if !into.push(call.source.as_str().to_string(), args) {
                return Err(AgentError::new(ErrorCode::ConnectionLost, "Done delivering; send it to Keymotion."));
            }
            Ok(serde_json::json!({ "queued": true }))
        }
        _ => Err(AgentError::new(ErrorCode::UnknownCommand, "This keymotion-cli only passes hook events on.")),
    });
    let server = match served {
        Ok(server) => server,
        // Another copy delivers: it takes this event too.
        Err(_) => {
            drop(lock);
            let _ = to_delivery(&names, &first);
            return;
        }
    };
    // The user may have closed Keymotion since the hook looked.
    if stopped(env, agent) {
        return;
    }
    let Ok(conn) = session::connect(&ctx, IfNotRunning::Start) else { return };
    // One connection at a time: Keymotion takes only a few at once, and
    // the hooks need theirs.
    let mut client = Some((first.source.clone(), conn.client));
    let began = Instant::now();
    loop {
        let batch = queue.take(LINGER);
        if batch.is_empty() {
            break;
        }
        for event in news(batch, Instant::now()) {
            deliver(&names.app, &mut client, event);
        }
        if began.elapsed() > RELAY_MAX {
            break;
        }
    }
    // The lock first: hooks go to Keymotion from now on; then what came in
    // between (a hook that comes later still is sent back to Keymotion).
    drop(lock);
    for event in news(queue.close(), Instant::now()) {
        deliver(&names.app, &mut client, event);
    }
    drop(server);
}

/// One event to Keymotion, on a connection of its session's: the last
/// one's when it was the same session, a new one otherwise.
fn deliver(app: &str, client: &mut Option<(String, Client)>, event: Queued) {
    if client.as_ref().is_none_or(|(source, _)| *source != event.source) {
        *client = None;
        let Ok(source) = SourceName::parse(&event.source) else { return };
        match Client::connect(app, source, crate::NAME, crate::VERSION, CALL) {
            Ok(c) => *client = Some((event.source.clone(), c)),
            Err(_) => return,
        }
    }
    if let Some((_, c)) = client.as_mut() {
        if c.call_within(&Command::Hook(event.args), CALL).is_err() {
            *client = None;
        }
    }
}

/// What of `batch` is still news, in order: nothing older than
/// [`MAX_AGE`]; no question its session answered or went past later in the
/// batch; no "done" or "error" of a session that began a new turn since.
fn news(batch: Vec<Queued>, now: Instant) -> Vec<Queued> {
    let keep: Vec<bool> = batch
        .iter()
        .enumerate()
        .map(|(i, e)| {
            now.saturating_duration_since(e.at) <= MAX_AGE
                && !batch[i + 1..].iter().any(|later| later.source == e.source && supersedes(&later.args, &e.args))
        })
        .collect();
    batch.into_iter().zip(keep).filter_map(|(e, keep)| keep.then_some(e)).collect()
}

/// `later` makes `earlier` old news, from the same session.
fn supersedes(later: &HookArgs, earlier: &HookArgs) -> bool {
    use HookEvent as E;
    let (Ok(next), Ok(event)) = (HookEvent::parse(&later.event), HookEvent::parse(&earlier.event)) else {
        return false;
    };
    match event {
        // An answer naming one of its calls, or none (which answers all; a
        // question with no call of its own only so).
        E::Attention => match next {
            E::Answered => later.calls.is_empty() || later.calls.iter().any(|c| earlier.calls.contains(c)),
            E::Busy | E::Done | E::Error | E::UsageLimit | E::SessionEnd | E::Interrupted => true,
            _ => false,
        },
        E::Done | E::Error | E::UsageLimit | E::Helper => next == E::Busy,
        _ => false,
    }
}

/// An event as one plain command line argument: its JSON in hex.
fn encode(read: &event::Read) -> String {
    let v = serde_json::json!({ "source": read.source, "args": read.args });
    v.to_string().bytes().map(|b| format!("{b:02x}")).collect()
}

fn decode(token: &str) -> Option<event::Read> {
    if token.len() % 2 != 0 || token.len() > 16 * 1024 {
        return None;
    }
    let bytes: Option<Vec<u8>> =
        (0..token.len()).step_by(2).map(|i| u8::from_str_radix(token.get(i..i + 2)?, 16).ok()).collect();
    let v: serde_json::Value = serde_json::from_slice(&bytes?).ok()?;
    let source = v["source"].as_str()?.to_string();
    let args: HookArgs = serde_json::from_value(v["args"].clone()).ok()?;
    args.validate().ok()?;
    HookEvent::parse(&args.event).ok()?;
    Some(event::Read { source, args })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_goes_through_the_command_line_and_back() {
        let read = event::read(
            Agent::ClaudeCode,
            br#"{"hook_event_name":"Stop","session_id":"s1","cwd":"C:\\Users\\a b\\proje \u00e7"}"#,
        )
        .unwrap();
        let token = encode(&read);
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()), "a plain argument");
        assert_eq!(decode(&token), Some(read));
        for bad in ["", "0", "zz", "7b7d", &"ab".repeat(9000)] {
            assert_eq!(decode(bad), None, "{bad}");
        }
    }

    fn queued(source: &str, event: &str, calls: &[&str], at: Instant) -> Queued {
        let args = HookArgs {
            event: event.into(),
            calls: calls.iter().map(|c| c.to_string()).collect(),
            ..HookArgs::default()
        };
        Queued { source: source.into(), args, at }
    }

    fn shown(events: &[Queued]) -> Vec<String> {
        events.iter().map(|e| format!("{} {} {}", e.source, e.args.event, e.args.calls.join(","))).collect()
    }

    #[test]
    fn what_piled_up_while_keymotion_started_goes_in_order_without_old_news() {
        let t = Instant::now();
        let batch = vec![
            queued("a", "attention", &["c1"], t),
            queued("b", "attention", &[], t),
            queued("a", "answered", &["u1", "c1"], t),
            queued("b", "answered", &["u9"], t),
            queued("c", "attention", &["c3"], t),
            queued("c", "done", &[], t),
            queued("d", "error", &[], t),
            queued("d", "busy", &[], t),
            queued("e", "attention", &["c5"], t),
            queued("f", "attention", &["c6"], t),
            queued("f", "answered", &[], t),
        ];
        assert_eq!(
            shown(&news(batch, t)),
            [
                // b's question has no call of its own: another call finishing
                // (a helper agent's) is no answer to it.
                "b attention ",
                "a answered u1,c1",
                "b answered u9",
                "c done ",
                "d busy ",
                "e attention c5",
                "f answered ",
            ]
        );
        // Another session's answer answers nothing here.
        let other = vec![queued("a", "attention", &["c1"], t), queued("z", "answered", &["c1"], t)];
        assert_eq!(news(other, t).len(), 2);
        // Old news: too long ago.
        let late = t + MAX_AGE + Duration::from_secs(1);
        assert_eq!(
            shown(&news(vec![queued("a", "done", &[], t), queued("a", "helper", &[], late)], late)),
            ["a helper "]
        );
    }

    #[test]
    fn the_queue_keeps_the_order_and_the_newest() {
        let q = Queue::default();
        let done = || HookArgs { event: "done".into(), ..HookArgs::default() };
        for i in 0..MAX_QUEUE + 3 {
            assert!(q.push(format!("s{i}"), done()));
        }
        let all = q.take(Duration::ZERO);
        assert_eq!(all.len(), MAX_QUEUE);
        assert_eq!(
            (all[0].source.as_str(), all[MAX_QUEUE - 1].source.as_str()),
            ("s3", format!("s{}", MAX_QUEUE + 2).as_str())
        );
        assert!(q.take(Duration::from_millis(20)).is_empty());
        // Closed: what was there comes out once; nothing more goes in.
        assert!(q.push("last".into(), done()));
        assert_eq!(q.close().len(), 1);
        assert!(!q.push("late".into(), done()));
        assert!(q.take(Duration::ZERO).is_empty());
    }
}
