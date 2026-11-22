//! Protocol v1: newline-delimited JSON over the pipe, one object per line,
//! at most [`MAX_LINE_BYTES`] per line.
//!
//! ```text
//! → {"v":1,"id":"1","source":"cli","cmd":"hello","args":{"client":"keymotion-cli","version":"1.2.0","protocol":{"min":1,"max":1}}}
//! ← {"v":1,"id":"1","ok":true,"data":{"app":"keymotion","version":"1.2.0","channel":"store","protocol":1}}
//! → {"v":1,"id":"2","source":"claude-code:3f9a","cmd":"signal","args":{"name":"done"}}
//! ← {"v":1,"id":"2","ok":false,"error":{"code":"agent_access_off","message":"…","hint":"…"}}
//! ```
//!
//! The first request on a connection is `hello`; the server refuses others
//! before it. Readers ignore fields they do not know, so later versions
//! can add fields without breaking older ones; `cmd` + `args` leave room
//! for new commands (focus, profile, card, scene).

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::error::{AgentError, ErrorCode};
use crate::keys::KeySpec;
use crate::values::{
    check_settings_key, is_request_id, signal_duration_ms, Color, Effect, SignalId, SignalName, SourceName,
};

/// The protocol version this build speaks, and the `v` of every message.
pub const PROTOCOL_VERSION: u32 = 1;
/// The protocol versions this build can speak.
pub const SUPPORTED_PROTOCOLS: ProtocolRange = ProtocolRange { min: 1, max: 1 };
/// The longest line either side accepts, in bytes, without the newline.
pub const MAX_LINE_BYTES: usize = 64 * 1024;

/// The most log entries `log` returns, and the default.
pub const MAX_LOG_LIMIT: u32 = 200;
pub const DEFAULT_LOG_LIMIT: u32 = 20;

// ------------------------------------------------------------- envelopes

/// A request as it travels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub v: u32,
    pub id: String,
    #[serde(default = "default_source")]
    pub source: String,
    pub cmd: String,
    #[serde(default, deserialize_with = "object_or_null")]
    pub args: Map<String, Value>,
}

fn default_source() -> String {
    crate::values::DEFAULT_SOURCE.to_string()
}

/// `args` may be missing or `null`; both mean no arguments.
fn object_or_null<'de, D: Deserializer<'de>>(d: D) -> Result<Map<String, Value>, D::Error> {
    Ok(Option::<Map<String, Value>>::deserialize(d)?.unwrap_or_default())
}

/// A line the server could not take, with the request id when it could be
/// read (empty otherwise), so the answer can still name the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub id: String,
    pub error: AgentError,
}

impl Request {
    /// A request for `command`.
    pub fn new(id: impl Into<String>, source: &SourceName, command: &Command) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            id: id.into(),
            source: source.as_str().to_string(),
            cmd: command.name().to_string(),
            args: command.args(),
        }
    }

    /// The line to send, without the newline. `too_large` when longer
    /// than [`MAX_LINE_BYTES`].
    pub fn to_line(&self) -> Result<String, AgentError> {
        let line = serde_json::to_string(self)
            .map_err(|e| AgentError::new(ErrorCode::Internal, format!("Could not write the request: {e}")))?;
        if line.len() > MAX_LINE_BYTES {
            return Err(AgentError::new(
                ErrorCode::TooLarge,
                format!("The request is longer than {MAX_LINE_BYTES} bytes."),
            ));
        }
        Ok(line)
    }

    /// Read and check one line (the server side): size, JSON, protocol
    /// version, request id, source name. The source comes back normalized.
    pub fn decode(line: &str) -> Result<Request, Refusal> {
        let refuse = |id: &str, code: ErrorCode, message: String| Refusal {
            id: id.to_string(),
            error: AgentError::new(code, message),
        };
        if line.len() > MAX_LINE_BYTES {
            return Err(refuse("", ErrorCode::TooLarge, format!("The request is longer than {MAX_LINE_BYTES} bytes.")));
        }
        let value: Value = serde_json::from_str(line)
            .map_err(|e| refuse("", ErrorCode::BadRequest, format!("The request is not JSON: {e}")))?;
        let Value::Object(object) = &value else {
            return Err(refuse("", ErrorCode::BadRequest, "The request is not a JSON object.".to_string()));
        };
        let id = match object.get("id") {
            Some(Value::String(id)) if is_request_id(id) => id.as_str(),
            _ => return Err(refuse("", ErrorCode::BadRequest, "The request has no valid id.".to_string())),
        };
        let v = object.get("v").and_then(Value::as_u64);
        if v != Some(u64::from(PROTOCOL_VERSION)) {
            let got = v.map_or_else(|| "none".to_string(), |v| v.to_string());
            return Err(Refusal {
                id: id.to_string(),
                error: AgentError::new(
                    ErrorCode::ProtocolMismatch,
                    format!("This Keymotion speaks protocol {PROTOCOL_VERSION}, the request uses {got}."),
                )
                .with_hint("Use the keymotion-cli installed with Keymotion, or update both."),
            });
        }
        let id = id.to_string();
        let mut request: Request = serde_json::from_value(value)
            .map_err(|e| refuse(&id, ErrorCode::BadRequest, format!("The request is malformed: {e}")))?;
        let source = SourceName::parse(&request.source).map_err(|error| Refusal { id: id.clone(), error })?;
        request.source = source.as_str().to_string();
        Ok(request)
    }

    /// The source, checked.
    pub fn source_name(&self) -> Result<SourceName, AgentError> {
        SourceName::parse(&self.source)
    }

    /// The typed command. `unknown_command` for a command this build does
    /// not know; `bad_request` when the arguments have the wrong types.
    pub fn command(&self) -> Result<Command, AgentError> {
        Command::from_parts(&self.cmd, &self.args)
    }
}

/// An answer as it travels: `data` when `ok`, `error` otherwise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub v: u32,
    pub id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

/// An error as it travels. `code` stays a string so a newer app's codes
/// still reach the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

impl From<AgentError> for ErrorBody {
    fn from(e: AgentError) -> Self {
        Self { code: e.code.as_str().to_string(), message: e.message, hint: e.hint, retry_after_ms: e.retry_after_ms }
    }
}

impl ErrorBody {
    /// As an [`AgentError`]. A code this build does not know becomes
    /// `internal`, with the code kept in the message.
    pub fn into_agent_error(self) -> AgentError {
        let (code, message) = match ErrorCode::from_code(&self.code) {
            Some(code) => (code, self.message),
            None => (ErrorCode::Internal, format!("{} ({})", self.message, self.code)),
        };
        AgentError { code, message, hint: self.hint, retry_after_ms: self.retry_after_ms }
    }
}

impl Response {
    /// A success with `data` (an object; `null` becomes `{}`).
    pub fn ok(id: impl Into<String>, data: impl Serialize) -> Self {
        let id = id.into();
        match serde_json::to_value(data) {
            Ok(Value::Null) => Self::ok_value(id, Value::Object(Map::new())),
            Ok(value) => Self::ok_value(id, value),
            Err(e) => Self::error(id, AgentError::new(ErrorCode::Internal, format!("Could not write the answer: {e}"))),
        }
    }

    fn ok_value(id: String, data: Value) -> Self {
        Self { v: PROTOCOL_VERSION, id, ok: true, data: Some(data), error: None }
    }

    pub fn error(id: impl Into<String>, error: AgentError) -> Self {
        Self { v: PROTOCOL_VERSION, id: id.into(), ok: false, data: None, error: Some(error.into()) }
    }

    /// From a handler's result.
    pub fn from_result(id: impl Into<String>, result: Result<Value, AgentError>) -> Self {
        match result {
            Ok(data) => Self::ok(id, data),
            Err(e) => Self::error(id, e),
        }
    }

    /// The line to send, without the newline. Never fails: an answer too
    /// long for the protocol becomes a `too_large` error.
    pub fn to_line(&self) -> String {
        match serde_json::to_string(self) {
            Ok(line) if line.len() <= MAX_LINE_BYTES => line,
            Ok(_) => Self::error(
                self.id.clone(),
                AgentError::new(ErrorCode::TooLarge, format!("The answer is longer than {MAX_LINE_BYTES} bytes.")),
            )
            .to_line(),
            Err(e) => Self::error(
                self.id.clone(),
                AgentError::new(ErrorCode::Internal, format!("Could not write the answer: {e}")),
            )
            .to_line(),
        }
    }

    /// Read one answer line (the client side).
    pub fn decode(line: &str) -> Result<Response, AgentError> {
        if line.len() > MAX_LINE_BYTES {
            return Err(AgentError::new(ErrorCode::BadResponse, "Keymotion's answer is too long."));
        }
        let response: Response = serde_json::from_str(line)
            .map_err(|e| AgentError::new(ErrorCode::BadResponse, format!("Keymotion's answer is malformed: {e}")))?;
        if response.v != PROTOCOL_VERSION {
            return Err(AgentError::new(
                ErrorCode::ProtocolMismatch,
                format!("Keymotion answered in protocol {}, this keymotion-cli speaks {PROTOCOL_VERSION}.", response.v),
            ));
        }
        Ok(response)
    }

    /// The data, or the error.
    pub fn into_result(self) -> Result<Value, AgentError> {
        match (self.ok, self.data, self.error) {
            (true, data, _) => Ok(data.unwrap_or_else(|| Value::Object(Map::new()))),
            (false, _, Some(error)) => Err(error.into_agent_error()),
            (false, _, None) => Err(AgentError::new(ErrorCode::BadResponse, "Keymotion refused without saying why.")),
        }
    }
}

// ----------------------------------------------------------------- hello

/// A range of protocol versions, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolRange {
    pub min: u32,
    pub max: u32,
}

/// The highest version both sides speak.
pub fn negotiate(client: ProtocolRange, server: ProtocolRange) -> Result<u32, AgentError> {
    let low = client.min.max(server.min);
    let high = client.max.min(server.max);
    if client.min <= client.max && low <= high {
        return Ok(high);
    }
    let error = AgentError::new(
        ErrorCode::ProtocolMismatch,
        format!(
            "keymotion-cli speaks protocol {}–{}, Keymotion speaks {}–{}.",
            client.min, client.max, server.min, server.max
        ),
    );
    Err(if client.max < server.min {
        error.with_hint("This keymotion-cli is older than Keymotion. Use the one installed with Keymotion.")
    } else {
        error.with_hint("Keymotion is older than this keymotion-cli. Update Keymotion.")
    })
}

/// `hello`: who the client is and which protocol versions it speaks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloArgs {
    /// The client program, e.g. `keymotion-cli`.
    #[serde(default)]
    pub client: String,
    /// Its version, e.g. `1.2.0`.
    pub version: String,
    pub protocol: ProtocolRange,
}

/// Which copy of Keymotion answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Installed from GitHub (NSIS).
    Github,
    /// Installed from the Microsoft Store.
    Store,
    /// A development build.
    Dev,
    /// A channel a newer app knows and this build does not.
    #[serde(other)]
    Other,
}

/// The answer to `hello`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloData {
    /// Always `keymotion`.
    pub app: String,
    pub version: String,
    pub channel: Channel,
    /// The protocol version this connection uses from now on.
    pub protocol: u32,
}

/// The app's side of `hello`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    pub version: String,
    pub channel: Channel,
}

/// The client on a connection, after its `hello`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub client: String,
    pub version: String,
    pub protocol: u32,
}

/// Answer `hello`: agree on a protocol version.
pub fn answer_hello(args: &HelloArgs, server: &ServerInfo) -> Result<(HelloData, Peer), AgentError> {
    let protocol = negotiate(args.protocol, SUPPORTED_PROTOCOLS)?;
    let data =
        HelloData { app: "keymotion".to_string(), version: server.version.clone(), channel: server.channel, protocol };
    let peer = Peer { client: args.client.clone(), version: args.version.clone(), protocol };
    Ok((data, peer))
}

// -------------------------------------------------------------- commands

/// `status`: no arguments yet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusArgs {}

/// `keys`: no arguments yet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeysArgs {}

/// `signal <name>`: a ready-made signal, optionally restyled. As typed;
/// [`SignalArgs::validate`] checks it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SignalArgs {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub times: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub for_ms: Option<u64>,
}

/// A checked `signal`. `None` fields keep the look the user chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signal {
    pub name: SignalName,
    pub id: Option<SignalId>,
    pub keys: Option<KeySpec>,
    pub color: Option<Color>,
    pub effect: Option<Effect>,
    pub duration: Option<Duration>,
}

impl SignalArgs {
    pub fn validate(&self) -> Result<Signal, AgentError> {
        let name = SignalName::parse(&self.name)?;
        let id = self.id.as_deref().map(SignalId::parse).transpose()?;
        let keys = self.keys.as_deref().map(KeySpec::parse).transpose()?;
        let color = self.color.as_deref().map(Color::parse).transpose()?;
        let restyled = self.direction.is_some() || self.speed.is_some() || self.times.is_some();
        if self.effect.is_none() && restyled {
            return Err(AgentError::new(ErrorCode::BadArgument, "Direction, speed and times need an effect."));
        }
        let effect = match &self.effect {
            Some(effect) => Some(Effect::from_parts(Some(effect), self.direction.as_deref(), self.speed, self.times)?),
            None => None,
        };
        let duration = lifetime(self.times, self.for_ms)?;
        Ok(Signal { name, id, keys, color, effect, duration })
    }
}

/// `light`: a light of the agent's own design. As typed;
/// [`LightArgs::validate`] checks it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Default: the whole keyboard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<String>,
    pub color: String,
    /// Default: solid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub times: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub for_ms: Option<u64>,
}

/// A checked `light`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Light {
    pub id: Option<SignalId>,
    pub keys: KeySpec,
    pub color: Color,
    pub effect: Effect,
    /// `None`: until `effect.times()` cycles ran, or the app's default.
    pub duration: Option<Duration>,
}

impl LightArgs {
    pub fn validate(&self) -> Result<Light, AgentError> {
        let id = self.id.as_deref().map(SignalId::parse).transpose()?;
        let keys = self.keys.as_deref().map(KeySpec::parse).transpose()?.unwrap_or_default();
        if self.color.trim().is_empty() {
            return Err(AgentError::new(ErrorCode::BadArgument, "A light needs a color.")
                .with_hint("Add a color, e.g. --color amber or --color #FF8000."));
        }
        let color = Color::parse(&self.color)?;
        let effect = Effect::from_parts(self.effect.as_deref(), self.direction.as_deref(), self.speed, self.times)?;
        let duration = lifetime(self.times, self.for_ms)?;
        Ok(Light { id, keys, color, effect, duration })
    }
}

/// A cycle count or a duration, not both.
fn lifetime(times: Option<u32>, for_ms: Option<u64>) -> Result<Option<Duration>, AgentError> {
    match (times, for_ms) {
        (Some(_), Some(_)) => {
            Err(AgentError::new(ErrorCode::BadArgument, "Give either times or a duration, not both."))
        }
        (_, Some(ms)) => Ok(Some(signal_duration_ms(ms)?)),
        _ => Ok(None),
    }
}

/// `clear`: remove the request source's signals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClearArgs {
    /// One signal; without it, every signal of the request's source.
    /// Another source's signals are never touched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl ClearArgs {
    /// The id to clear; `None` clears all of the source's signals.
    pub fn validate(&self) -> Result<Option<SignalId>, AgentError> {
        self.id.as_deref().map(SignalId::parse).transpose()
    }
}

/// `log`: the last agent requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl LogArgs {
    /// How many entries, 1 to [`MAX_LOG_LIMIT`]; [`DEFAULT_LOG_LIMIT`]
    /// when not given.
    pub fn validate(&self) -> Result<u32, AgentError> {
        match self.limit {
            None => Ok(DEFAULT_LOG_LIMIT),
            Some(n) if (1..=MAX_LOG_LIMIT).contains(&n) => Ok(n),
            Some(n) => Err(AgentError::new(
                ErrorCode::BadValue,
                format!("Limit {n} is out of range; use 1 to {MAX_LOG_LIMIT}."),
            )),
        }
    }
}

/// `settings get [key]`: the settings that concern agents, read-only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsGetArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl SettingsGetArgs {
    /// The key, normalized; `None` for all of them.
    pub fn validate(&self) -> Result<Option<String>, AgentError> {
        self.key.as_deref().map(check_settings_key).transpose()
    }
}

/// Every command of protocol v1, with its arguments as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Hello(HelloArgs),
    Status(StatusArgs),
    Keys(KeysArgs),
    Signal(SignalArgs),
    Light(LightArgs),
    Clear(ClearArgs),
    Log(LogArgs),
    SettingsGet(SettingsGetArgs),
}

impl Command {
    /// Every command name of this build.
    pub const NAMES: [&'static str; 8] = ["hello", "status", "keys", "signal", "light", "clear", "log", "settings_get"];

    /// The `cmd` string.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Hello(_) => "hello",
            Self::Status(_) => "status",
            Self::Keys(_) => "keys",
            Self::Signal(_) => "signal",
            Self::Light(_) => "light",
            Self::Clear(_) => "clear",
            Self::Log(_) => "log",
            Self::SettingsGet(_) => "settings_get",
        }
    }

    /// The `args` object.
    pub fn args(&self) -> Map<String, Value> {
        let value = match self {
            Self::Hello(a) => serde_json::to_value(a),
            Self::Status(a) => serde_json::to_value(a),
            Self::Keys(a) => serde_json::to_value(a),
            Self::Signal(a) => serde_json::to_value(a),
            Self::Light(a) => serde_json::to_value(a),
            Self::Clear(a) => serde_json::to_value(a),
            Self::Log(a) => serde_json::to_value(a),
            Self::SettingsGet(a) => serde_json::to_value(a),
        };
        // Plain structs of strings and numbers always serialize to objects.
        match value {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        }
    }

    /// From `cmd` and `args`.
    pub fn from_parts(cmd: &str, args: &Map<String, Value>) -> Result<Self, AgentError> {
        fn typed<T: serde::de::DeserializeOwned>(cmd: &str, args: &Map<String, Value>) -> Result<T, AgentError> {
            serde_json::from_value(Value::Object(args.clone())).map_err(|e| {
                AgentError::new(ErrorCode::BadRequest, format!("The arguments of '{cmd}' are malformed: {e}"))
            })
        }
        Ok(match cmd {
            "hello" => Self::Hello(typed(cmd, args)?),
            "status" => Self::Status(typed(cmd, args)?),
            "keys" => Self::Keys(typed(cmd, args)?),
            "signal" => Self::Signal(typed(cmd, args)?),
            "light" => Self::Light(typed(cmd, args)?),
            "clear" => Self::Clear(typed(cmd, args)?),
            "log" => Self::Log(typed(cmd, args)?),
            "settings_get" => Self::SettingsGet(typed(cmd, args)?),
            _ => {
                return Err(AgentError::new(
                    ErrorCode::UnknownCommand,
                    format!("This Keymotion does not know the command '{}'.", crate::values::shown(cmd)),
                )
                .with_hint("Update Keymotion."))
            }
        })
    }
}

// --------------------------------------------------------------- serving

/// A request the app is asked to carry out, after `hello`.
#[derive(Debug, Clone, Copy)]
pub struct Call<'a> {
    pub peer: &'a Peer,
    pub source: &'a SourceName,
    pub id: &'a str,
}

/// One connection's state on the server side: the line-by-line protocol
/// without the pipe, so it can be tested anywhere.
#[derive(Debug, Clone)]
pub struct Session {
    info: Arc<ServerInfo>,
    peer: Option<Peer>,
}

impl Session {
    pub fn new(info: Arc<ServerInfo>) -> Self {
        Self { info, peer: None }
    }

    /// The client, once it said `hello`.
    pub fn peer(&self) -> Option<&Peer> {
        self.peer.as_ref()
    }

    /// Answer one line. `hello` is answered here; any other command goes to
    /// `handler`, but only after `hello`. The answer echoes the request id.
    pub fn handle_line(
        &mut self,
        line: &str,
        handler: impl FnOnce(Call<'_>, Command) -> Result<Value, AgentError>,
    ) -> String {
        let request = match Request::decode(line) {
            Ok(request) => request,
            Err(refusal) => return Response::error(refusal.id, refusal.error).to_line(),
        };
        let id = request.id.as_str();
        let command = match request.command() {
            Ok(command) => command,
            Err(e) => return Response::error(id, e).to_line(),
        };
        let response = match command {
            Command::Hello(args) => match answer_hello(&args, &self.info) {
                Ok((data, peer)) => {
                    self.peer = Some(peer);
                    Response::ok(id, data)
                }
                Err(e) => Response::error(id, e),
            },
            command => match (&self.peer, request.source_name()) {
                (None, _) => Response::error(
                    id,
                    AgentError::new(ErrorCode::BadRequest, "The first request on a connection must be hello."),
                ),
                (Some(_), Err(e)) => Response::error(id, e),
                (Some(peer), Ok(source)) => {
                    Response::from_result(id, handler(Call { peer, source: &source, id }, command))
                }
            },
        };
        response.to_line()
    }
}

/// The answer to a line that was too long to read; the server closes the
/// connection after it.
pub fn too_long_line() -> String {
    Response::error(
        "",
        AgentError::new(ErrorCode::TooLarge, format!("The request is longer than {MAX_LINE_BYTES} bytes.")),
    )
    .to_line()
}

/// The answer to a line that is not UTF-8 text; the server closes the
/// connection after it.
pub fn not_text_line() -> String {
    Response::error("", AgentError::new(ErrorCode::BadRequest, "The request is not UTF-8 text.")).to_line()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn source() -> SourceName {
        SourceName::parse("claude-code:3f9a").unwrap()
    }

    fn hello_args() -> HelloArgs {
        HelloArgs { client: "keymotion-cli".into(), version: "1.2.0".into(), protocol: SUPPORTED_PROTOCOLS }
    }

    fn info() -> Arc<ServerInfo> {
        Arc::new(ServerInfo { version: "1.2.0".into(), channel: Channel::Store })
    }

    #[test]
    fn requests_round_trip() {
        let commands = [
            Command::Hello(hello_args()),
            Command::Status(StatusArgs {}),
            Command::Keys(KeysArgs {}),
            Command::Signal(SignalArgs { name: "done".into(), for_ms: Some(120_000), ..Default::default() }),
            Command::Light(LightArgs {
                keys: Some("f1-f12".into()),
                color: "amber".into(),
                effect: Some("wave".into()),
                direction: Some("left".into()),
                speed: Some(4),
                times: Some(2),
                id: Some("build".into()),
                ..Default::default()
            }),
            Command::Clear(ClearArgs { id: Some("build".into()) }),
            Command::Clear(ClearArgs::default()),
            Command::Log(LogArgs { limit: Some(5) }),
            Command::SettingsGet(SettingsGetArgs { key: None }),
        ];
        for (i, command) in commands.iter().enumerate() {
            let request = Request::new(i.to_string(), &source(), command);
            let line = request.to_line().unwrap();
            assert!(!line.contains('\n'));
            let back = Request::decode(&line).unwrap();
            assert_eq!(back, request);
            assert_eq!(&back.command().unwrap(), command);
            assert!(Command::NAMES.contains(&command.name()));
        }
    }

    #[test]
    fn the_wire_form_is_as_documented() {
        let request =
            Request::new("7", &source(), &Command::Signal(SignalArgs { name: "done".into(), ..Default::default() }));
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({"v":1,"id":"7","source":"claude-code:3f9a","cmd":"signal","args":{"name":"done"}})
        );
        let ok = Response::ok("7", json!({"shown": true}));
        assert_eq!(ok.to_line(), r#"{"v":1,"id":"7","ok":true,"data":{"shown":true}}"#);
        let err = Response::error(
            "8",
            AgentError::new(ErrorCode::RateLimited, "Too many requests.")
                .with_hint("Wait.")
                .with_retry_after(Duration::from_millis(250)),
        );
        assert_eq!(
            err.to_line(),
            r#"{"v":1,"id":"8","ok":false,"error":{"code":"rate_limited","message":"Too many requests.","hint":"Wait.","retry_after_ms":250}}"#
        );
        let bare = Response::error("9", AgentError::new(ErrorCode::NoKeyboard, "No keyboard."));
        assert_eq!(
            bare.to_line(),
            r#"{"v":1,"id":"9","ok":false,"error":{"code":"no_keyboard","message":"No keyboard."}}"#
        );
    }

    #[test]
    fn responses_round_trip() {
        let ok = Response::ok("1", json!({"a": [1, 2]}));
        assert_eq!(Response::decode(&ok.to_line()).unwrap().into_result().unwrap(), json!({"a": [1, 2]}));
        assert_eq!(Response::decode(&Response::ok("1", ()).to_line()).unwrap().into_result().unwrap(), json!({}));
        let err = Response::error("2", AgentError::new(ErrorCode::NeedsUsb, "Connect the cable.").with_hint("USB."));
        let e = Response::decode(&err.to_line()).unwrap().into_result().unwrap_err();
        assert_eq!(e.code, ErrorCode::NeedsUsb);
        assert_eq!(e.exit_code(), 69);
        assert_eq!(e.hint.as_deref(), Some("USB."));
    }

    #[test]
    fn readers_ignore_unknown_fields() {
        let line = r#"{"v":1,"id":"3","source":"cli","cmd":"light","trace":"x","args":{"color":"red","glow":true,"keys":"esc"}}"#;
        let request = Request::decode(line).unwrap();
        let Command::Light(args) = request.command().unwrap() else { panic!() };
        assert_eq!(args.validate().unwrap().color, Color::rgb(0xFF, 0, 0));

        let answer = r#"{"v":1,"id":"3","ok":true,"data":{"app":"keymotion","version":"9.0.0","channel":"beta","protocol":1,"new":1},"extra":[]}"#;
        let data = Response::decode(answer).unwrap().into_result().unwrap();
        let hello: HelloData = serde_json::from_value(data).unwrap();
        assert_eq!(hello.channel, Channel::Other, "a channel this build does not know");

        let refusal =
            r#"{"v":1,"id":"4","ok":false,"error":{"code":"new_code","message":"Something new.","detail":1}}"#;
        let e = Response::decode(refusal).unwrap().into_result().unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
        assert!(e.message.contains("new_code"));
    }

    #[test]
    fn missing_and_null_args_mean_none() {
        for line in [r#"{"v":1,"id":"1","cmd":"status"}"#, r#"{"v":1,"id":"1","cmd":"status","args":null}"#] {
            let request = Request::decode(line).unwrap();
            assert_eq!(request.source, "cli");
            assert_eq!(request.command().unwrap(), Command::Status(StatusArgs {}));
        }
    }

    #[test]
    fn bad_envelopes_are_refused_with_the_right_code() {
        let code = |line: &str| Request::decode(line).unwrap_err();
        assert_eq!(code("not json").error.code, ErrorCode::BadRequest);
        assert_eq!(code("[1]").error.code, ErrorCode::BadRequest);
        assert_eq!(code(r#"{"v":1,"cmd":"status"}"#).error.code, ErrorCode::BadRequest);
        assert_eq!(code(r#"{"v":1,"id":"a b","cmd":"status"}"#).error.code, ErrorCode::BadRequest);
        assert_eq!(code(r#"{"v":1,"id":7,"cmd":"status"}"#).error.code, ErrorCode::BadRequest);
        let v2 = code(r#"{"v":2,"id":"5","cmd":"status"}"#);
        assert_eq!((v2.id.as_str(), v2.error.code), ("5", ErrorCode::ProtocolMismatch));
        assert_eq!(code(r#"{"id":"5","cmd":"status"}"#).error.code, ErrorCode::ProtocolMismatch);
        let no_cmd = code(r#"{"v":1,"id":"6"}"#);
        assert_eq!((no_cmd.id.as_str(), no_cmd.error.code), ("6", ErrorCode::BadRequest));
        assert_eq!(code(r#"{"v":1,"id":"6","cmd":"status","args":[1]}"#).error.code, ErrorCode::BadRequest);
        assert_eq!(code(r#"{"v":1,"id":"6","source":"a b","cmd":"status"}"#).error.code, ErrorCode::BadSource);
        let wrong_type = Request::decode(r#"{"v":1,"id":"6","cmd":"log","args":{"limit":"ten"}}"#).unwrap();
        assert_eq!(wrong_type.command().unwrap_err().code, ErrorCode::BadRequest);
        let unknown = Request::decode(r#"{"v":1,"id":"6","cmd":"teleport"}"#).unwrap();
        assert_eq!(unknown.command().unwrap_err().code, ErrorCode::UnknownCommand);
        assert_eq!(unknown.command().unwrap_err().exit_code(), 76);
    }

    #[test]
    fn lines_stop_at_64_kib() {
        let pad = |len: usize| {
            let head = r#"{"v":1,"id":"1","cmd":"status","args":{"pad":""#;
            let tail = r#""}}"#;
            format!("{head}{}{tail}", "x".repeat(len - head.len() - tail.len()))
        };
        assert!(Request::decode(&pad(MAX_LINE_BYTES)).is_ok());
        let refusal = Request::decode(&pad(MAX_LINE_BYTES + 1)).unwrap_err();
        assert_eq!(refusal.error.code, ErrorCode::TooLarge);

        let huge = Command::Light(LightArgs { color: "x".repeat(MAX_LINE_BYTES), ..Default::default() });
        assert_eq!(Request::new("1", &source(), &huge).to_line().unwrap_err().code, ErrorCode::TooLarge);

        let big_answer = Response::ok("2", json!({ "pad": "x".repeat(MAX_LINE_BYTES) }));
        let line = big_answer.to_line();
        assert!(line.len() <= MAX_LINE_BYTES);
        let e = Response::decode(&line).unwrap().into_result().unwrap_err();
        assert_eq!(e.code, ErrorCode::TooLarge);
        assert_eq!(Response::decode(&"x".repeat(MAX_LINE_BYTES + 1)).unwrap_err().code, ErrorCode::BadResponse);

        let refusal = Response::decode(&too_long_line()).unwrap().into_result().unwrap_err();
        assert_eq!(refusal.code, ErrorCode::TooLarge);
        let refusal = Response::decode(&not_text_line()).unwrap().into_result().unwrap_err();
        assert_eq!(refusal.code, ErrorCode::BadRequest);
    }

    #[test]
    fn protocol_negotiation() {
        let r = |min, max| ProtocolRange { min, max };
        assert_eq!(negotiate(r(1, 1), r(1, 1)).unwrap(), 1);
        assert_eq!(negotiate(r(1, 3), r(2, 5)).unwrap(), 3);
        let old_cli = negotiate(r(1, 1), r(2, 3)).unwrap_err();
        assert_eq!(old_cli.code, ErrorCode::ProtocolMismatch);
        assert_eq!(old_cli.exit_code(), 76);
        assert!(old_cli.hint.unwrap().contains("older than Keymotion"));
        let old_app = negotiate(r(2, 3), r(1, 1)).unwrap_err();
        assert!(old_app.hint.unwrap().contains("Update Keymotion"));
        assert!(negotiate(r(3, 1), r(1, 5)).is_err(), "an empty range");
    }

    #[test]
    fn signal_args_are_checked() {
        let ok = SignalArgs {
            name: "attention".into(),
            keys: Some("all".into()),
            color: Some("#ff8000".into()),
            effect: Some("flash".into()),
            times: Some(3),
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(ok.name, SignalName::Attention);
        assert_eq!(ok.effect.unwrap().times(), Some(3));
        assert_eq!(ok.duration, None);

        let plain = SignalArgs { name: "done".into(), ..Default::default() }.validate().unwrap();
        assert_eq!((plain.color, plain.effect, plain.keys), (None, None, None));

        let code = |a: SignalArgs| a.validate().unwrap_err().code;
        assert_eq!(code(SignalArgs::default()), ErrorCode::BadSignal);
        assert_eq!(
            code(SignalArgs { name: "done".into(), speed: Some(2), ..Default::default() }),
            ErrorCode::BadArgument
        );
        assert_eq!(
            code(SignalArgs {
                name: "done".into(),
                effect: Some("blink".into()),
                times: Some(2),
                for_ms: Some(1000),
                ..Default::default()
            }),
            ErrorCode::BadArgument
        );
        assert_eq!(
            code(SignalArgs { name: "done".into(), for_ms: Some(3 * 3_600_000), ..Default::default() }),
            ErrorCode::BadDuration
        );
        assert_eq!(
            code(SignalArgs { name: "done".into(), color: Some("teal".into()), ..Default::default() }),
            ErrorCode::BadColor
        );
        assert_eq!(
            code(SignalArgs { name: "done".into(), keys: Some("f1-".into()), ..Default::default() }),
            ErrorCode::BadKeys
        );
        assert_eq!(
            code(SignalArgs { name: "done".into(), id: Some("a b".into()), ..Default::default() }),
            ErrorCode::BadId
        );
    }

    #[test]
    fn light_args_are_checked() {
        let light = LightArgs { color: "red".into(), for_ms: Some(30_000), ..Default::default() }.validate().unwrap();
        assert!(light.keys.is_all());
        assert_eq!(light.effect, Effect::solid());
        assert_eq!(light.duration, Some(Duration::from_secs(30)));
        let code = |a: LightArgs| a.validate().unwrap_err().code;
        assert_eq!(code(LightArgs::default()), ErrorCode::BadArgument);
        assert_eq!(code(LightArgs { color: "red".into(), times: Some(2), ..Default::default() }), ErrorCode::BadEffect);
        assert_eq!(
            code(LightArgs { color: "red".into(), effect: Some("blink".into()), speed: Some(9), ..Default::default() }),
            ErrorCode::BadValue
        );
    }

    #[test]
    fn small_commands_are_checked() {
        assert_eq!(ClearArgs::default().validate().unwrap(), None);
        assert_eq!(ClearArgs { id: Some("Build".into()) }.validate().unwrap().unwrap().as_str(), "build");
        assert_eq!(LogArgs::default().validate().unwrap(), DEFAULT_LOG_LIMIT);
        assert_eq!(LogArgs { limit: Some(MAX_LOG_LIMIT) }.validate().unwrap(), MAX_LOG_LIMIT);
        assert_eq!(LogArgs { limit: Some(0) }.validate().unwrap_err().code, ErrorCode::BadValue);
        assert_eq!(LogArgs { limit: Some(MAX_LOG_LIMIT + 1) }.validate().unwrap_err().code, ErrorCode::BadValue);
        assert_eq!(
            SettingsGetArgs { key: Some("agents.screen".into()) }.validate().unwrap().as_deref(),
            Some("agents.screen")
        );
        assert_eq!(SettingsGetArgs { key: Some("../x".into()) }.validate().unwrap_err().code, ErrorCode::BadValue);
    }

    fn call(session: &mut Session, line: &str) -> Response {
        let answer = session.handle_line(line, |call, command| {
            Ok(json!({"cmd": command.name(), "source": call.source.as_str(), "client": call.peer.client}))
        });
        Response::decode(&answer).unwrap()
    }

    #[test]
    fn a_session_starts_with_hello() {
        let mut session = Session::new(info());
        let status = r#"{"v":1,"id":"s1","source":"Tests","cmd":"status"}"#;
        let early = call(&mut session, status);
        assert_eq!(early.id, "s1");
        assert_eq!(early.into_result().unwrap_err().code, ErrorCode::BadRequest);
        assert!(session.peer().is_none());

        let hello = Request::new("h", &source(), &Command::Hello(hello_args())).to_line().unwrap();
        let answer = call(&mut session, &hello);
        assert_eq!(answer.id, "h");
        let data: HelloData = serde_json::from_value(answer.into_result().unwrap()).unwrap();
        assert_eq!(
            data,
            HelloData { app: "keymotion".into(), version: "1.2.0".into(), channel: Channel::Store, protocol: 1 }
        );
        assert_eq!(session.peer().unwrap().version, "1.2.0");

        let answer = call(&mut session, status);
        assert_eq!(answer.id, "s1");
        assert_eq!(
            answer.into_result().unwrap(),
            json!({"cmd": "status", "source": "tests", "client": "keymotion-cli"})
        );
    }

    #[test]
    fn a_session_refuses_an_unknown_protocol_and_keeps_going_after_errors() {
        let mut session = Session::new(info());
        let future = HelloArgs { protocol: ProtocolRange { min: 2, max: 4 }, ..hello_args() };
        let line = Request::new("h", &source(), &Command::Hello(future)).to_line().unwrap();
        let e = call(&mut session, &line).into_result().unwrap_err();
        assert_eq!(e.code, ErrorCode::ProtocolMismatch);
        assert!(session.peer().is_none());

        let line = Request::new("h2", &source(), &Command::Hello(hello_args())).to_line().unwrap();
        assert!(call(&mut session, &line).ok);
        assert_eq!(call(&mut session, "{").into_result().unwrap_err().code, ErrorCode::BadRequest);
        let answer = call(&mut session, r#"{"v":1,"id":"x","cmd":"teleport"}"#);
        assert_eq!(answer.id, "x");
        assert_eq!(answer.into_result().unwrap_err().code, ErrorCode::UnknownCommand);
        let failing = session.handle_line(r#"{"v":1,"id":"f","cmd":"keys"}"#, |_, _| {
            Err(AgentError::new(ErrorCode::NoKeyboard, "No keyboard is connected."))
        });
        assert_eq!(Response::decode(&failing).unwrap().into_result().unwrap_err().code, ErrorCode::NoKeyboard);
    }
}
