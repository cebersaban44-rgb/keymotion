//! The protocol over the named pipe (Windows): the app's server and the
//! CLI's client, on top of `kb_winsys::pipe`.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use kb_winsys::pipe::{self, ClientError, Incoming, PipeClient, PipeServer, ServerOptions};
use serde_json::Value;

use crate::error::{AgentError, ErrorCode};
use crate::protocol::{
    not_text_line, too_long_line, Call, Command, HelloArgs, HelloData, Request, Response, ServerInfo, Session,
    MAX_LINE_BYTES, SUPPORTED_PROTOCOLS,
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
    let info = Arc::new(info);
    let handler = Arc::new(handler);
    let options = ServerOptions { max_line: MAX_LINE_BYTES, ..ServerOptions::default() };
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
    source: SourceName,
    timeout: Duration,
    next_id: u64,
    app: HelloData,
}

impl Client {
    /// Connect to `name` and say hello as `client` `version`. `timeout`
    /// bounds the connection and each answer. `app_not_running` when
    /// nobody serves the pipe, `timeout` when no answer came in time,
    /// `protocol_mismatch` when the two sides share no protocol version.
    pub fn connect(
        name: &str,
        source: SourceName,
        client: &str,
        version: &str,
        timeout: Duration,
    ) -> Result<Client, AgentError> {
        let mut pipe = PipeClient::connect(name, timeout).map_err(client_error)?;
        let hello =
            HelloArgs { client: client.to_string(), version: version.to_string(), protocol: SUPPORTED_PROTOCOLS };
        let data = exchange(&mut pipe, &source, timeout, "1", &Command::Hello(hello))?;
        let app: HelloData = serde_json::from_value(data).map_err(|e| {
            AgentError::new(ErrorCode::BadResponse, format!("Keymotion's answer to hello is malformed: {e}"))
        })?;
        if !(SUPPORTED_PROTOCOLS.min..=SUPPORTED_PROTOCOLS.max).contains(&app.protocol) {
            return Err(AgentError::new(
                ErrorCode::ProtocolMismatch,
                format!("Keymotion chose protocol {}, which this keymotion-cli does not speak.", app.protocol),
            ));
        }
        Ok(Client { pipe, source, timeout, next_id: 1, app })
    }

    /// The app's answer to `hello`: its version, channel and the protocol.
    pub fn app(&self) -> &HelloData {
        &self.app
    }

    /// Send a command and wait for its answer.
    pub fn call(&mut self, command: &Command) -> Result<Value, AgentError> {
        self.next_id += 1;
        exchange(&mut self.pipe, &self.source, self.timeout, &self.next_id.to_string(), command)
    }

    /// The process serving the pipe, for `doctor`.
    pub fn server_process_id(&self) -> Option<u32> {
        self.pipe.server_process_id().ok()
    }
}

/// One request and its answer.
fn exchange(
    pipe: &mut PipeClient,
    source: &SourceName,
    timeout: Duration,
    id: &str,
    command: &Command,
) -> Result<Value, AgentError> {
    let line = Request::new(id, source, command).to_line()?;
    let answer = pipe.request(&line, timeout).map_err(client_error)?;
    let response = Response::decode(&answer)?;
    // A refusal of an unreadable request carries no id.
    if response.id != id && !(response.id.is_empty() && !response.ok) {
        return Err(AgentError::new(ErrorCode::BadResponse, "Keymotion answered another request."));
    }
    response.into_result()
}

fn client_error(e: ClientError) -> AgentError {
    match e {
        ClientError::NotRunning => AgentError::new(ErrorCode::AppNotRunning, "Keymotion is not running."),
        ClientError::Timeout => AgentError::new(ErrorCode::Timeout, "Keymotion did not answer in time."),
        ClientError::Closed => AgentError::new(ErrorCode::Internal, "Keymotion closed the connection."),
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
        let e = Client::connect(&name, SourceName::default(), "keymotion-cli", "1.2.0", WAIT).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::AppNotRunning, 69));

        let _first = serve(&name, app(), |_, _| Ok(json!({}))).unwrap();
        let second = serve(&name, app(), |_, _| Ok(json!({}))).unwrap_err();
        assert_eq!(second.kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn a_slow_app_times_out() {
        let name = unique_name();
        let _server = serve(&name, app(), |_, _| {
            std::thread::sleep(Duration::from_millis(1500));
            Ok(json!({}))
        })
        .unwrap();
        let mut client =
            Client::connect(&name, SourceName::default(), "keymotion-cli", "1.2.0", Duration::from_millis(300))
                .unwrap();
        let e = client.call(&Command::Status(StatusArgs {})).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::Timeout, 75));
    }
}
