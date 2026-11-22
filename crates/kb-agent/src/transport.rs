//! The protocol over the named pipe (Windows): the app's server and the
//! CLI's client, on top of `kb_winsys::pipe`.

use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kb_winsys::pipe::{self, ClientError, Incoming, PipeClient, PipeServer, ServerOptions};
use serde_json::Value;

use crate::error::{AgentError, ErrorCode};
use crate::protocol::{
    not_text_line, too_long_line, Call, Command, HelloArgs, HelloData, Request, Response, ServerInfo, Session,
    HELLO_VERSION, MAX_LINE_BYTES, SUPPORTED_PROTOCOLS,
};
use crate::values::SourceName;

/// This user's pipe in this logon session ([`crate::pipe_name`]).
pub fn current_pipe_name() -> io::Result<String> {
    let sid = pipe::current_user_sid()?;
    crate::pipe_name(&sid, pipe::current_session_id()?)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("unexpected user SID {sid}")))
}

/// Serve the protocol on `name` (the app's side). `hello` is answered here
/// from `info`; every other command goes to `handler` once the connection
/// said hello. The server stops when the returned value is dropped.
///
/// Fails with [`io::ErrorKind::AddrInUse`] when another process holds the
/// name.
pub fn serve<F>(name: &str, info: ServerInfo, handler: F) -> io::Result<PipeServer>
where
    F: Fn(Call<'_>, Command) -> Result<Value, AgentError> + Send + Sync + 'static,
{
    serve_with(name, info, ServerOptions::default(), handler)
}

fn serve_with<F>(name: &str, info: ServerInfo, options: ServerOptions, handler: F) -> io::Result<PipeServer>
where
    F: Fn(Call<'_>, Command) -> Result<Value, AgentError> + Send + Sync + 'static,
{
    let info = Arc::new(info);
    let handler = Arc::new(handler);
    let options = ServerOptions { max_line: MAX_LINE_BYTES, ..options };
    PipeServer::start(name, options, move || {
        let mut session = Session::new(info.clone());
        let handler = handler.clone();
        move |incoming: Incoming<'_>| match incoming {
            Incoming::Line(line) => session.handle_line(line, |call, command| handler(call, command)),
            Incoming::TooLong => too_long_line(),
            Incoming::NotText => not_text_line(),
        }
    })
}

/// A connection to the running app (the CLI's side), after `hello`.
#[derive(Debug)]
pub struct Client {
    pipe: PipeClient,
    name: String,
    source: SourceName,
    client: String,
    version: String,
    timeout: Duration,
    next_id: u64,
    app: HelloData,
}

/// Why one exchange failed.
enum Failure {
    /// The line never reached the app (the connection had closed): safe to
    /// send again on a new connection.
    NotDelivered,
    Error(AgentError),
}

impl From<AgentError> for Failure {
    fn from(e: AgentError) -> Self {
        Failure::Error(e)
    }
}

impl Client {
    /// Connect to `name` and say hello as `client` `version`, both within
    /// `timeout`; each later call has `timeout` too.
    ///
    /// `app_not_running` when nobody serves the pipe, `pipe_not_trusted`
    /// when the pipe is not this user's, `timeout` when no answer came in
    /// time, `protocol_mismatch` when the two sides share no protocol
    /// version.
    pub fn connect(
        name: &str,
        source: SourceName,
        client: &str,
        version: &str,
        timeout: Duration,
    ) -> Result<Client, AgentError> {
        let deadline = Instant::now() + timeout;
        let (pipe, app) = open(name, &source, client, version, deadline)?;
        Ok(Client {
            pipe,
            name: name.to_string(),
            source,
            client: client.to_string(),
            version: version.to_string(),
            timeout,
            next_id: 1,
            app,
        })
    }

    /// The app's answer to `hello`: its version, channel and the protocol.
    pub fn app(&self) -> &HelloData {
        &self.app
    }

    /// Send a command and wait for its answer. When the app had closed the
    /// idle connection, the command never reached it: the client connects
    /// again, says hello and sends it once more, all within the timeout.
    pub fn call(&mut self, command: &Command) -> Result<Value, AgentError> {
        self.call_within(command, self.timeout)
    }

    /// [`Client::call`] within `timeout` instead of the connection's, so a
    /// program can give connecting and calling one deadline together.
    pub fn call_within(&mut self, command: &Command, timeout: Duration) -> Result<Value, AgentError> {
        let deadline = Instant::now() + timeout;
        self.next_id += 1;
        let id = self.next_id.to_string();
        match exchange(&mut self.pipe, self.app.protocol, &self.source, deadline, &id, command) {
            Ok(data) => return Ok(data),
            Err(Failure::Error(e)) => return Err(e),
            Err(Failure::NotDelivered) => {}
        }
        let (pipe, app) = open(&self.name, &self.source, &self.client, &self.version, deadline)?;
        self.pipe = pipe;
        self.app = app;
        match exchange(&mut self.pipe, self.app.protocol, &self.source, deadline, &id, command) {
            Ok(data) => Ok(data),
            Err(Failure::Error(e)) => Err(e),
            Err(Failure::NotDelivered) => Err(connection_lost()),
        }
    }

    /// The process serving the pipe, for `doctor`.
    pub fn server_process_id(&self) -> Option<u32> {
        self.pipe.server_process_id().ok()
    }

    /// The pipe's owner as a SID string, for `doctor`. Connecting already
    /// refused a pipe this user does not own.
    pub fn pipe_owner_sid(&self) -> Option<String> {
        self.pipe.owner_sid().ok()
    }
}

/// Connect and say hello, by `deadline`.
fn open(
    name: &str,
    source: &SourceName,
    client: &str,
    version: &str,
    deadline: Instant,
) -> Result<(PipeClient, HelloData), AgentError> {
    let mut pipe = PipeClient::connect(name, left(deadline)).map_err(client_error)?;
    let hello = HelloArgs { client: client.to_string(), version: version.to_string(), protocol: SUPPORTED_PROTOCOLS };
    let data = match exchange(&mut pipe, HELLO_VERSION, source, deadline, "1", &Command::Hello(hello)) {
        Ok(data) => data,
        // A fresh connection closed before hello arrived: the app is going.
        Err(Failure::NotDelivered) => return Err(connection_lost()),
        Err(Failure::Error(e)) => return Err(e),
    };
    let app: HelloData = serde_json::from_value(data).map_err(|e| {
        AgentError::new(ErrorCode::BadResponse, format!("Keymotion's answer to hello is malformed: {e}"))
    })?;
    if !(SUPPORTED_PROTOCOLS.min..=SUPPORTED_PROTOCOLS.max).contains(&app.protocol) {
        return Err(AgentError::new(
            ErrorCode::ProtocolMismatch,
            format!("Keymotion chose protocol {}, which this keymotion-cli does not speak.", app.protocol),
        ));
    }
    Ok((pipe, app))
}

fn left(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

/// One request in the connection's `protocol` version and its answer, by
/// `deadline`.
fn exchange(
    pipe: &mut PipeClient,
    protocol: u32,
    source: &SourceName,
    deadline: Instant,
    id: &str,
    command: &Command,
) -> Result<Value, Failure> {
    let request = Request::new(protocol, id, source, command);
    let line = request.to_line()?;
    let answer = match pipe.request(&line, left(deadline)) {
        Ok(answer) => answer,
        Err(ClientError::NotDelivered) => return Err(Failure::NotDelivered),
        Err(e) => return Err(client_error(e).into()),
    };
    let response = Response::decode(&answer, request.v)?;
    // A refusal of an unreadable request carries no id.
    if response.id != id && !(response.id.is_empty() && !response.ok) {
        return Err(AgentError::new(ErrorCode::BadResponse, "Keymotion answered another request.").into());
    }
    Ok(response.into_result()?)
}

fn connection_lost() -> AgentError {
    AgentError::new(ErrorCode::ConnectionLost, "Keymotion closed the connection before answering.")
        .with_hint("Try again in a moment; if it keeps happening, run keymotion-cli doctor.")
}

fn client_error(e: ClientError) -> AgentError {
    match e {
        ClientError::NotRunning => AgentError::new(ErrorCode::AppNotRunning, "Keymotion is not running."),
        ClientError::NotTrusted(why) => {
            AgentError::new(ErrorCode::PipeNotTrusted, format!("The pipe to Keymotion is not trusted: {why}."))
                .with_hint("Nothing was sent. Run keymotion-cli doctor.")
        }
        ClientError::Timeout => AgentError::new(ErrorCode::Timeout, "Keymotion did not answer in time."),
        ClientError::NotDelivered | ClientError::Closed => connection_lost(),
        ClientError::TooLong => AgentError::new(ErrorCode::BadResponse, "Keymotion's answer is too long."),
        ClientError::Io(e) => AgentError::new(ErrorCode::Internal, format!("The connection to Keymotion failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Channel, LightArgs, SignalArgs, StatusArgs};
    use serde_json::json;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn unique_name() -> String {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        format!(r"\\.\pipe\kb-agent-test-{}-{}-{nanos}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed))
    }

    const WAIT: Duration = Duration::from_secs(5);

    fn app() -> ServerInfo {
        ServerInfo { version: "1.2.0".into(), channel: Channel::Dev }
    }

    fn connect(name: &str, timeout: Duration) -> Result<Client, AgentError> {
        Client::connect(name, SourceName::default(), "keymotion-cli", "1.2.0", timeout)
    }

    #[test]
    fn the_pipe_name_is_this_users() {
        let name = current_pipe_name().unwrap();
        assert!(name.starts_with(crate::PIPE_PREFIX), "{name}");
        assert!(name.contains(&pipe::current_user_sid().unwrap()), "{name}");
    }

    #[test]
    fn a_client_talks_to_a_server() {
        let name = unique_name();
        let _server = serve(&name, app(), |call, command| match command {
            Command::Status(_) => Ok(json!({ "source": call.source.as_str(), "client": call.peer.client })),
            Command::Light(args) => {
                let light = args.validate()?;
                Ok(json!({ "keys": light.keys.to_string(), "color": light.color.hex() }))
            }
            _ => Err(AgentError::new(ErrorCode::AgentAccessOff, "Agent access is off.")
                .with_hint("Turn it on in Keymotion's settings.")),
        })
        .unwrap();

        let source = SourceName::parse("tests").unwrap();
        let mut client = Client::connect(&name, source, "keymotion-cli", "1.2.0", WAIT).unwrap();
        assert_eq!(client.app().channel, Channel::Dev);
        assert_eq!(client.app().protocol, 1);
        assert_eq!(client.server_process_id(), Some(std::process::id()));

        let status = client.call(&Command::Status(StatusArgs {})).unwrap();
        assert_eq!(status, json!({ "source": "tests", "client": "keymotion-cli" }));

        let light =
            Command::Light(LightArgs { keys: Some("F12-F1".into()), color: "Amber".into(), ..Default::default() });
        assert_eq!(client.call(&light).unwrap(), json!({ "keys": "f1-f12", "color": "#ffbf00" }));

        // The app re-checks what the client sends.
        let bad = Command::Light(LightArgs { color: "teal".into(), ..Default::default() });
        let e = client.call(&bad).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::BadColor, 65));

        let refused =
            client.call(&Command::Signal(SignalArgs { name: "done".into(), ..Default::default() })).unwrap_err();
        assert_eq!((refused.code, refused.exit_code()), (ErrorCode::AgentAccessOff, 77));
        assert!(refused.hint.is_some());
    }

    #[test]
    fn no_app_and_a_taken_name_are_reported() {
        let name = unique_name();
        let e = connect(&name, WAIT).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::AppNotRunning, 69));

        let _first = serve(&name, app(), |_, _| Ok(json!({}))).unwrap();
        let second = serve(&name, app(), |_, _| Ok(json!({}))).unwrap_err();
        assert_eq!(second.kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn a_slow_app_times_out_within_one_timeout() {
        let name = unique_name();
        let _server = serve(&name, app(), |_, _| {
            std::thread::sleep(Duration::from_millis(1500));
            Ok(json!({}))
        })
        .unwrap();
        let mut client = connect(&name, Duration::from_millis(300)).unwrap();
        let started = Instant::now();
        let e = client.call(&Command::Status(StatusArgs {})).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::Timeout, 75));
        assert!(started.elapsed() < Duration::from_millis(1000), "{:?}", started.elapsed());

        // A call can have less time than the connection, what is left of a
        // deadline the program set for connecting and calling together.
        let mut client = connect(&name, WAIT).unwrap();
        let started = Instant::now();
        let e = client.call_within(&Command::Status(StatusArgs {}), Duration::from_millis(200)).unwrap_err();
        assert_eq!(e.code, ErrorCode::Timeout);
        assert!(started.elapsed() < Duration::from_millis(900), "{:?}", started.elapsed());
        assert_eq!(client.pipe_owner_sid(), pipe::current_user_sid().ok());
    }

    #[test]
    fn connecting_and_hello_share_one_timeout() {
        // An app that accepts but never answers hello.
        let name = unique_name();
        let _server = kb_winsys::pipe::serve(&name, || {
            |_: Incoming<'_>| {
                std::thread::sleep(Duration::from_millis(1500));
                String::new()
            }
        })
        .unwrap();
        let started = Instant::now();
        let e = connect(&name, Duration::from_millis(400)).unwrap_err();
        assert_eq!(e.code, ErrorCode::Timeout);
        assert!(started.elapsed() < Duration::from_millis(700), "{:?}", started.elapsed());
    }

    #[test]
    fn an_idle_connection_is_reopened_once() {
        let name = unique_name();
        let quick = ServerOptions { read_timeout: Duration::from_millis(200), ..ServerOptions::default() };
        let _server = serve_with(&name, app(), quick, |_, _| Ok(json!({ "ok": true }))).unwrap();
        let mut client = connect(&name, WAIT).unwrap();
        // The app drops the idle connection after 200 ms.
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(client.call(&Command::Status(StatusArgs {})).unwrap(), json!({ "ok": true }));
        assert_eq!(client.call(&Command::Status(StatusArgs {})).unwrap(), json!({ "ok": true }));
    }

    #[test]
    fn an_app_that_went_away_is_unavailable_not_internal() {
        let name = unique_name();
        let quick = ServerOptions { read_timeout: Duration::from_millis(200), ..ServerOptions::default() };
        let server = serve_with(&name, app(), quick, |_, _| Ok(json!({}))).unwrap();
        let mut client = connect(&name, WAIT).unwrap();
        drop(server);
        let e = client.call(&Command::Status(StatusArgs {})).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::AppNotRunning, 69));

        // An app that closes after the command arrived (here a handler that
        // fails hard) is a lost connection: temporary.
        let name = unique_name();
        let _server = serve(&name, app(), |_, _| panic!("handler failed (expected in this test)")).unwrap();
        let mut client = connect(&name, WAIT).unwrap();
        let e = client.call(&Command::Status(StatusArgs {})).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::ConnectionLost, 75));
    }
}
