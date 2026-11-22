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
//!
//! Versions: `hello` and its answer always travel as `v: 1`
//! ([`HELLO_VERSION`]), whatever versions the two sides speak; the client
//! lists the versions it speaks and the server picks the highest both
//! know. Every later message on the connection carries that version. The
//! hello exchange and the refusal envelope (`ok: false` with `error.code`
//! and `error.message`) are frozen: no later version may change them, so
//! any two builds can always agree or say why not.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::error::{AgentError, ErrorCode};
use crate::keys::KeySpec;
use crate::values::{
    check_settings_key, check_text, is_request_id, signal_duration_ms, Color, Effect, SignalId, SignalName, SourceName,
};

/// The newest protocol version this build speaks.
pub const PROTOCOL_VERSION: u32 = SUPPORTED_PROTOCOLS.max;
/// The protocol versions this build can speak.
pub const SUPPORTED_PROTOCOLS: ProtocolRange = ProtocolRange { min: 1, max: 1 };
/// The `v` of `hello` and of its answer, in every version. Frozen.
pub const HELLO_VERSION: u32 = 1;
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
    /// A request for `command` in the connection's `protocol` version (the
    /// one `hello` agreed on). A `hello` always travels as
    /// [`HELLO_VERSION`].
    pub fn new(protocol: u32, id: impl Into<String>, source: &SourceName, command: &Command) -> Self {
        Self {
            v: if matches!(command, Command::Hello(_)) { HELLO_VERSION } else { protocol },
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

    /// Read and check one line (the server side): size, JSON, request id,
    /// source name. The source comes back normalized. Whether `v` fits the
    /// connection is the [`Session`]'s check.
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

    /// The same answer in protocol version `v` (the connection's; answers
    /// are made in [`PROTOCOL_VERSION`]).
    pub fn with_version(mut self, v: u32) -> Self {
        self.v = v;
        self
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
            .with_version(self.v)
            .to_line(),
            Err(e) => Self::error(
                self.id.clone(),
                AgentError::new(ErrorCode::Internal, format!("Could not write the answer: {e}")),
            )
            .with_version(self.v)
            .to_line(),
        }
    }

    /// Read one answer line (the client side). A success must carry
    /// `expected_v`, the version of the request ([`HELLO_VERSION`] for
    /// `hello`); a refusal is read in any version, since its envelope is
    /// frozen.
    pub fn decode(line: &str, expected_v: u32) -> Result<Response, AgentError> {
        if line.len() > MAX_LINE_BYTES {
            return Err(AgentError::new(ErrorCode::BadResponse, "Keymotion's answer is too long."));
        }
        let response: Response = serde_json::from_str(line)
            .map_err(|e| AgentError::new(ErrorCode::BadResponse, format!("Keymotion's answer is malformed: {e}")))?;
        if response.ok && response.v != expected_v {
            return Err(AgentError::new(
                ErrorCode::ProtocolMismatch,
                format!("Keymotion answered in protocol {}, the request used {expected_v}.", response.v),
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

/// Answer `hello`: agree on a protocol version from the server's
/// `protocols` (normally [`SUPPORTED_PROTOCOLS`]).
pub fn answer_hello(
    args: &HelloArgs,
    server: &ServerInfo,
    protocols: ProtocolRange,
) -> Result<(HelloData, Peer), AgentError> {
    let protocol = negotiate(args.protocol, protocols)?;
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

/// The most settings one `settings_set` may change.
pub const MAX_SETTINGS_PER_SET: usize = 32;
/// The longest text value `settings_set` takes, in characters.
pub const MAX_SETTING_TEXT_CHARS: usize = 64;

/// `settings set key=value …`: change look and behaviour settings (colors,
/// effects, durations, thresholds, night hours, progress keys). The app
/// decides which keys an agent may change; access, permissions and limits
/// are the user's alone. As typed; [`SettingsSetArgs::validate`] checks
/// the form, the app checks each value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsSetArgs {
    /// Key → new value: a string, a number or `true`/`false`.
    pub values: Map<String, Value>,
}

impl SettingsSetArgs {
    /// The keys (normalized, see [`check_settings_key`]) with their values,
    /// in the order given. At least one, at most [`MAX_SETTINGS_PER_SET`],
    /// each key once; values are strings (at most
    /// [`MAX_SETTING_TEXT_CHARS`], no control characters), numbers or
    /// booleans.
    pub fn validate(&self) -> Result<Vec<(String, Value)>, AgentError> {
        if self.values.is_empty() {
            return Err(AgentError::new(ErrorCode::BadArgument, "Give at least one setting to change.")
                .with_hint("e.g. signals.done.color=green"));
        }
        if self.values.len() > MAX_SETTINGS_PER_SET {
            return Err(AgentError::new(
                ErrorCode::BadArgument,
                format!("At most {MAX_SETTINGS_PER_SET} settings can change at once."),
            ));
        }
        let mut out: Vec<(String, Value)> = Vec::with_capacity(self.values.len());
        for (key, value) in &self.values {
            let key = check_settings_key(key)?;
            if out.iter().any(|(k, _)| *k == key) {
                return Err(AgentError::new(ErrorCode::BadArgument, format!("'{key}' is given twice.")));
            }
            match value {
                Value::Bool(_) | Value::Number(_) => {}
                Value::String(s) => {
                    if s.chars().count() > MAX_SETTING_TEXT_CHARS || s.chars().any(char::is_control) {
                        return Err(AgentError::new(
                            ErrorCode::BadValue,
                            format!("The value of '{key}' is not a short line of text."),
                        ));
                    }
                }
                _ => {
                    return Err(AgentError::new(
                        ErrorCode::BadValue,
                        format!("The value of '{key}' must be text, a number, true or false."),
                    ))
                }
            }
            out.push((key, value.clone()));
        }
        Ok(out)
    }
}

/// A progress bar's label: at most this many characters, one line.
pub const MAX_PROGRESS_LABEL_CHARS: usize = 40;

/// `progress <percent>`: a progress bar on a row of keys (the F row or the
/// number row by the user's choice, or keys the agent names). As typed;
/// [`ProgressArgs::validate`] checks it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgressArgs {
    /// 0 to 100.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    /// What is progressing, for the app's page (not shown on the keys).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Default: the row the user chose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<String>,
    /// Default: the user's color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// How long the bar stays after this update; default the app's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub for_ms: Option<u64>,
}

// `percent` is checked to be finite, so equality is total in practice.
impl Eq for ProgressArgs {}

/// A checked `progress`.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    /// 0 to 100.
    pub percent: f64,
    pub label: Option<String>,
    pub id: Option<SignalId>,
    pub keys: Option<KeySpec>,
    pub color: Option<Color>,
    pub duration: Option<Duration>,
}

impl ProgressArgs {
    pub fn validate(&self) -> Result<Progress, AgentError> {
        let percent = match self.percent {
            None => {
                return Err(AgentError::new(ErrorCode::BadArgument, "A progress bar needs a percent.")
                    .with_hint("Give a number from 0 to 100."))
            }
            Some(p) if p.is_finite() && (0.0..=100.0).contains(&p) => p,
            Some(p) => {
                return Err(AgentError::new(ErrorCode::BadValue, format!("{p} is not a percent; use 0 to 100.")))
            }
        };
        let label = match self.label.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(label) => {
                check_text(label, MAX_PROGRESS_LABEL_CHARS, 1)?;
                Some(label.to_string())
            }
        };
        let id = self.id.as_deref().map(SignalId::parse).transpose()?;
        let keys = self.keys.as_deref().map(KeySpec::parse).transpose()?;
        let color = self.color.as_deref().map(Color::parse).transpose()?;
        let duration = self.for_ms.map(signal_duration_ms).transpose()?;
        Ok(Progress { percent, label, id, keys, color, duration })
    }
}

/// What a coding agent's hook said, in words of its own that do not depend
/// on the agent: `keymotion-cli hook claude-code|codex` reads the agent's
/// event and sends one of these; the app turns it into lights by the user's
/// settings (the looks, A usage limit, B session numbers, H helper agents).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    /// A session started: it gets its number (B), nothing lights.
    SessionStart,
    /// The user sent a prompt: the session's earlier lights go (`busy`).
    Busy,
    /// The agent waits for the user (an approval, a question).
    Attention,
    /// The user answered, or the agent moved on: the session's question
    /// goes when nothing else is waiting ([`HookArgs::calls`]).
    Answered,
    /// The turn finished.
    Done,
    /// The turn failed.
    Error,
    /// The turn stopped at a usage limit (rate limit, billing, the service
    /// overloaded): behaviour A, told apart from an error.
    UsageLimit,
    /// A helper agent (subagent) or a task of the session finished:
    /// behaviour H.
    Helper,
    /// The session ended: its question and its number go; what it ended
    /// with (done, an error) stays its time.
    SessionEnd,
}

impl HookEvent {
    pub const ALL: [HookEvent; 9] = [
        Self::SessionStart,
        Self::Busy,
        Self::Attention,
        Self::Answered,
        Self::Done,
        Self::Error,
        Self::UsageLimit,
        Self::Helper,
        Self::SessionEnd,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::Busy => "busy",
            Self::Attention => "attention",
            Self::Answered => "answered",
            Self::Done => "done",
            Self::Error => "error",
            Self::UsageLimit => "usage_limit",
            Self::Helper => "helper",
            Self::SessionEnd => "session_end",
        }
    }

    pub fn parse(input: &str) -> Result<Self, AgentError> {
        let s = input.trim();
        Self::ALL.into_iter().find(|e| e.as_str().eq_ignore_ascii_case(s)).ok_or_else(|| {
            AgentError::new(ErrorCode::BadValue, format!("'{}' is not a hook event.", crate::values::shown(input)))
        })
    }

    /// Whether it may light something: when the app is not running, only
    /// these are worth starting it for.
    pub const fn lights(self) -> bool {
        matches!(self, Self::Attention | Self::Done | Self::Error | Self::UsageLimit | Self::Helper)
    }
}

/// A session's label, for the app's page: the project folder's name.
pub const MAX_HOOK_LABEL_CHARS: usize = 40;
/// The most tool calls one hook request names.
pub const MAX_HOOK_CALLS: usize = 4;
/// A tool call's key: a short hash keymotion-cli makes of the call (never
/// the call itself).
pub const MAX_HOOK_CALL_LEN: usize = 32;

/// `hook`: a coding agent's hook event ([`HookEvent`]). As typed;
/// [`HookArgs::validate`] checks it. The request's source is the session
/// (`claude-code:3f9a0c1d`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HookArgs {
    pub event: String,
    /// The coding agent: `claude-code`, `codex`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The session's project folder name, for the page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// For `attention`: the tool calls that wait; for `answered`: the calls
    /// that were answered. Empty: for `answered`, every question of the
    /// session; for `attention`, a question with no call of its own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<String>,
}

/// A checked `hook`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hook {
    pub event: HookEvent,
    pub agent: Option<SourceName>,
    pub label: Option<String>,
    pub calls: Vec<String>,
}

impl HookArgs {
    pub fn validate(&self) -> Result<Hook, AgentError> {
        let event = HookEvent::parse(&self.event)?;
        let agent = self.agent.as_deref().map(SourceName::parse).transpose()?;
        let label = match self.label.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(label) => {
                check_text(label, MAX_HOOK_LABEL_CHARS, 1)?;
                Some(label.to_string())
            }
        };
        if self.calls.len() > MAX_HOOK_CALLS {
            return Err(AgentError::new(
                ErrorCode::BadArgument,
                format!("A hook names at most {MAX_HOOK_CALLS} tool calls."),
            ));
        }
        for call in &self.calls {
            let ok = !call.is_empty()
                && call.len() <= MAX_HOOK_CALL_LEN
                && call.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !ok {
                return Err(AgentError::new(
                    ErrorCode::BadValue,
                    format!("'{}' is not a tool call key.", crate::values::shown(call)),
                ));
            }
        }
        Ok(Hook { event, agent, label, calls: self.calls.clone() })
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
    SettingsSet(SettingsSetArgs),
    Progress(ProgressArgs),
    Hook(HookArgs),
}

impl Command {
    /// Every command name of this build.
    pub const NAMES: [&'static str; 11] = [
        "hello",
        "status",
        "keys",
        "signal",
        "light",
        "clear",
        "log",
        "settings_get",
        "settings_set",
        "progress",
        "hook",
    ];

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
            Self::SettingsSet(_) => "settings_set",
            Self::Progress(_) => "progress",
            Self::Hook(_) => "hook",
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
            Self::SettingsSet(a) => serde_json::to_value(a),
            Self::Progress(a) => serde_json::to_value(a),
            Self::Hook(a) => serde_json::to_value(a),
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
            "settings_set" => Self::SettingsSet(typed(cmd, args)?),
            "progress" => Self::Progress(typed(cmd, args)?),
            "hook" => Self::Hook(typed(cmd, args)?),
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
    /// The versions this server speaks.
    protocols: ProtocolRange,
    peer: Option<Peer>,
}

impl Session {
    pub fn new(info: Arc<ServerInfo>) -> Self {
        Self { info, protocols: SUPPORTED_PROTOCOLS, peer: None }
    }

    /// A session of a build that speaks `protocols`, to test how versions
    /// meet.
    #[cfg(test)]
    fn speaking(mut self, protocols: ProtocolRange) -> Self {
        self.protocols = protocols;
        self
    }

    /// The client, once it said `hello`.
    pub fn peer(&self) -> Option<&Peer> {
        self.peer.as_ref()
    }

    /// The version of this connection's messages: [`HELLO_VERSION`] until
    /// `hello`, then the one it agreed on.
    pub fn version(&self) -> u32 {
        self.peer.as_ref().map_or(HELLO_VERSION, |peer| peer.protocol)
    }

    /// Answer one line. `hello` (always `v: 1`) is answered here; any other
    /// command goes to `handler`, but only after `hello` and only in the
    /// version it agreed on. The answer echoes the request id and carries
    /// the request's version.
    pub fn handle_line(
        &mut self,
        line: &str,
        handler: impl FnOnce(Call<'_>, Command) -> Result<Value, AgentError>,
    ) -> String {
        let request = match Request::decode(line) {
            Ok(request) => request,
            Err(refusal) => return Response::error(refusal.id, refusal.error).with_version(self.version()).to_line(),
        };
        let id = request.id.as_str();
        if request.cmd == "hello" {
            return self.hello(&request).with_version(HELLO_VERSION).to_line();
        }
        let Some(peer) = &self.peer else {
            let e = AgentError::new(ErrorCode::BadRequest, "The first request on a connection must be hello.");
            return Response::error(id, e).with_version(HELLO_VERSION).to_line();
        };
        let response = if request.v != peer.protocol {
            Response::error(
                id,
                AgentError::new(
                    ErrorCode::ProtocolMismatch,
                    format!("This connection agreed on protocol {}, the request uses {}.", peer.protocol, request.v),
                ),
            )
        } else {
            match (request.command(), request.source_name()) {
                (Err(e), _) | (_, Err(e)) => Response::error(id, e),
                (Ok(command), Ok(source)) => {
                    Response::from_result(id, handler(Call { peer, source: &source, id }, command))
                }
            }
        };
        response.with_version(peer.protocol).to_line()
    }

    fn hello(&mut self, request: &Request) -> Response {
        let id = request.id.as_str();
        if request.v != HELLO_VERSION {
            let e = AgentError::new(
                ErrorCode::ProtocolMismatch,
                format!("hello travels as v {HELLO_VERSION}, this one as v {}.", request.v),
            );
            return Response::error(id, e);
        }
        let args = match request.command() {
            Ok(Command::Hello(args)) => args,
            Ok(_) => {
                return Response::error(id, AgentError::new(ErrorCode::Internal, "hello was read as another command."))
            }
            Err(e) => return Response::error(id, e),
        };
        match answer_hello(&args, &self.info, self.protocols) {
            Ok((data, peer)) => {
                self.peer = Some(peer);
                Response::ok(id, data)
            }
            Err(e) => Response::error(id, e),
        }
    }
}

/// The answer to a line that was too long to read; the server closes the
/// connection after it.
pub fn too_long_line() -> String {
    Response::error(
        "",
        AgentError::new(ErrorCode::TooLarge, format!("The request is longer than {MAX_LINE_BYTES} bytes.")),
    )
    .with_version(HELLO_VERSION)
    .to_line()
}

/// The answer to a line that is not UTF-8 text; the server closes the
/// connection after it.
pub fn not_text_line() -> String {
    Response::error("", AgentError::new(ErrorCode::BadRequest, "The request is not UTF-8 text."))
        .with_version(HELLO_VERSION)
        .to_line()
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
            Command::SettingsSet(SettingsSetArgs {
                values: [("signals.done.color".to_string(), json!("green")), ("night.on".to_string(), json!(true))]
                    .into_iter()
                    .collect(),
            }),
            Command::Progress(ProgressArgs {
                percent: Some(42.5),
                label: Some("Tests".into()),
                keys: Some("numbers".into()),
                ..Default::default()
            }),
            Command::Hook(HookArgs {
                event: "attention".into(),
                agent: Some("claude-code".into()),
                label: Some("klavye".into()),
                calls: vec!["3f9a0c1d2b4e5f60".into()],
            }),
            Command::Hook(HookArgs { event: "answered".into(), ..Default::default() }),
        ];
        for (i, command) in commands.iter().enumerate() {
            let request = Request::new(1, i.to_string(), &source(), command);
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
            Request::new(1, "7", &source(), &Command::Signal(SignalArgs { name: "done".into(), ..Default::default() }));
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
        assert_eq!(Response::decode(&ok.to_line(), 1).unwrap().into_result().unwrap(), json!({"a": [1, 2]}));
        assert_eq!(Response::decode(&Response::ok("1", ()).to_line(), 1).unwrap().into_result().unwrap(), json!({}));
        let err = Response::error("2", AgentError::new(ErrorCode::NeedsUsb, "Connect the cable.").with_hint("USB."));
        let e = Response::decode(&err.to_line(), 1).unwrap().into_result().unwrap_err();
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
        let data = Response::decode(answer, 1).unwrap().into_result().unwrap();
        let hello: HelloData = serde_json::from_value(data).unwrap();
        assert_eq!(hello.channel, Channel::Other, "a channel this build does not know");

        let refusal =
            r#"{"v":1,"id":"4","ok":false,"error":{"code":"new_code","message":"Something new.","detail":1}}"#;
        let e = Response::decode(refusal, 1).unwrap().into_result().unwrap_err();
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
        // Whether the version fits is the session's question; a missing one
        // is malformed.
        assert_eq!(Request::decode(r#"{"v":2,"id":"5","cmd":"status"}"#).unwrap().v, 2);
        assert_eq!(code(r#"{"id":"5","cmd":"status"}"#).error.code, ErrorCode::BadRequest);
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
        assert_eq!(Request::new(1, "1", &source(), &huge).to_line().unwrap_err().code, ErrorCode::TooLarge);

        let big_answer = Response::ok("2", json!({ "pad": "x".repeat(MAX_LINE_BYTES) }));
        let line = big_answer.to_line();
        assert!(line.len() <= MAX_LINE_BYTES);
        let e = Response::decode(&line, 1).unwrap().into_result().unwrap_err();
        assert_eq!(e.code, ErrorCode::TooLarge);
        assert_eq!(Response::decode(&"x".repeat(MAX_LINE_BYTES + 1), 1).unwrap_err().code, ErrorCode::BadResponse);

        let refusal = Response::decode(&too_long_line(), 1).unwrap().into_result().unwrap_err();
        assert_eq!(refusal.code, ErrorCode::TooLarge);
        let refusal = Response::decode(&not_text_line(), 1).unwrap().into_result().unwrap_err();
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

    #[test]
    fn settings_set_takes_short_scalar_values_once_each() {
        let args = |pairs: &[(&str, Value)]| SettingsSetArgs {
            values: pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        };
        let ok =
            args(&[("Signals.Done.Color", json!("#00ff00")), ("away.after", json!("5m")), ("night.on", json!(false))])
                .validate()
                .unwrap();
        assert_eq!(
            ok,
            vec![
                ("signals.done.color".to_string(), json!("#00ff00")),
                ("away.after".to_string(), json!("5m")),
                ("night.on".to_string(), json!(false)),
            ]
        );
        assert!(args(&[("progress.speed", json!(3))]).validate().is_ok());

        let code = |a: SettingsSetArgs| a.validate().unwrap_err().code;
        assert_eq!(code(SettingsSetArgs::default()), ErrorCode::BadArgument);
        assert_eq!(code(args(&[("a.b", json!([1]))])), ErrorCode::BadValue);
        assert_eq!(code(args(&[("a.b", json!({"x": 1}))])), ErrorCode::BadValue);
        assert_eq!(code(args(&[("a.b", Value::Null)])), ErrorCode::BadValue);
        assert_eq!(code(args(&[("a.b", json!("x".repeat(MAX_SETTING_TEXT_CHARS + 1)))])), ErrorCode::BadValue);
        assert_eq!(code(args(&[("a.b", json!("line\nbreak"))])), ErrorCode::BadValue);
        assert_eq!(code(args(&[("../x", json!(1))])), ErrorCode::BadValue);
        // Two spellings of one key.
        assert_eq!(code(args(&[("night.on", json!(true)), ("Night.On", json!(false))])), ErrorCode::BadArgument);
        let many: Vec<(String, Value)> = (0..=MAX_SETTINGS_PER_SET).map(|i| (format!("k{i}"), json!(i))).collect();
        assert_eq!(code(SettingsSetArgs { values: many.into_iter().collect() }), ErrorCode::BadArgument);
    }

    #[test]
    fn progress_args_are_checked() {
        let p = ProgressArgs {
            percent: Some(100.0),
            label: Some("  Build  ".into()),
            keys: Some("f1-f12".into()),
            color: Some("cyan".into()),
            id: Some("Build".into()),
            for_ms: Some(60_000),
        }
        .validate()
        .unwrap();
        assert_eq!(p.percent, 100.0);
        assert_eq!(p.label.as_deref(), Some("Build"));
        assert_eq!(p.id.unwrap().as_str(), "build");
        assert_eq!(p.keys.unwrap().to_string(), "f1-f12");
        assert_eq!(p.duration, Some(Duration::from_secs(60)));
        let bare = ProgressArgs { percent: Some(0.0), label: Some(" ".into()), ..Default::default() };
        let bare = bare.validate().unwrap();
        assert_eq!((bare.label, bare.keys, bare.color, bare.duration), (None, None, None, None));

        let code = |a: ProgressArgs| a.validate().unwrap_err().code;
        assert_eq!(code(ProgressArgs::default()), ErrorCode::BadArgument);
        for bad in [-0.5, 100.01, f64::NAN, f64::INFINITY] {
            assert_eq!(code(ProgressArgs { percent: Some(bad), ..Default::default() }), ErrorCode::BadValue, "{bad}");
        }
        let long = "x".repeat(MAX_PROGRESS_LABEL_CHARS + 1);
        assert_eq!(
            code(ProgressArgs { percent: Some(5.0), label: Some(long), ..Default::default() }),
            ErrorCode::TextTooLong
        );
        assert_eq!(
            code(ProgressArgs { percent: Some(5.0), label: Some("two\nlines".into()), ..Default::default() }),
            ErrorCode::TextTooLong
        );
        assert_eq!(
            code(ProgressArgs { percent: Some(5.0), color: Some("teal".into()), ..Default::default() }),
            ErrorCode::BadColor
        );
        assert_eq!(
            code(ProgressArgs { percent: Some(5.0), for_ms: Some(0), ..Default::default() }),
            ErrorCode::BadDuration
        );
        // A newer client's extra fields are ignored; a percent of the wrong
        // type is malformed.
        let args: Map<String, Value> = serde_json::from_str(r#"{"percent":50,"steps":3}"#).unwrap();
        assert!(matches!(Command::from_parts("progress", &args), Ok(Command::Progress(_))));
        let args: Map<String, Value> = serde_json::from_str(r#"{"percent":"half"}"#).unwrap();
        assert_eq!(Command::from_parts("progress", &args).unwrap_err().code, ErrorCode::BadRequest);
    }

    #[test]
    fn hook_args_are_checked() {
        for event in HookEvent::ALL {
            assert_eq!(HookEvent::parse(event.as_str()).unwrap(), event);
        }
        let lit: Vec<&str> = HookEvent::ALL.iter().filter(|e| e.lights()).map(|e| e.as_str()).collect();
        assert_eq!(lit, ["attention", "done", "error", "usage_limit", "helper"]);
        let hook = HookArgs {
            event: "Attention".into(),
            agent: Some("Codex".into()),
            label: Some("  my project  ".into()),
            calls: vec!["ab12".into(), "c-3".into()],
        }
        .validate()
        .unwrap();
        assert_eq!(hook.event, HookEvent::Attention);
        assert_eq!(hook.agent.unwrap().as_str(), "codex");
        assert_eq!(hook.label.as_deref(), Some("my project"));
        let bare = HookArgs { event: "done".into(), label: Some(" ".into()), ..Default::default() }.validate().unwrap();
        assert_eq!((bare.agent, bare.label, bare.calls.len()), (None, None, 0));

        let code = |a: HookArgs| a.validate().unwrap_err().code;
        assert_eq!(code(HookArgs::default()), ErrorCode::BadValue);
        assert_eq!(code(HookArgs { event: "stop".into(), ..Default::default() }), ErrorCode::BadValue);
        let long = "x".repeat(MAX_HOOK_LABEL_CHARS + 1);
        assert_eq!(code(HookArgs { event: "done".into(), label: Some(long), ..Default::default() }), ErrorCode::TextTooLong);
        assert_eq!(
            code(HookArgs { event: "done".into(), label: Some("a\u{1b}[2J".into()), ..Default::default() }),
            ErrorCode::BadText
        );
        assert_eq!(
            code(HookArgs { event: "done".into(), agent: Some("a b".into()), ..Default::default() }),
            ErrorCode::BadSource
        );
        for bad in ["", "AB12", "a b", "a/b", &"a".repeat(MAX_HOOK_CALL_LEN + 1)] {
            let args = HookArgs { event: "answered".into(), calls: vec![bad.to_string()], ..Default::default() };
            assert_eq!(code(args), ErrorCode::BadValue, "{bad:?}");
        }
        let many =
            HookArgs { event: "answered".into(), calls: vec!["a".into(); MAX_HOOK_CALLS + 1], ..Default::default() };
        assert_eq!(code(many), ErrorCode::BadArgument);
        // An app of this version reads what a newer CLI adds.
        let args: Map<String, Value> = serde_json::from_str(r#"{"event":"done","tool":"Bash"}"#).unwrap();
        assert!(matches!(Command::from_parts("hook", &args), Ok(Command::Hook(_))));
    }

    fn call(session: &mut Session, line: &str) -> Response {
        let answer = session.handle_line(line, |call, command| {
            Ok(json!({"cmd": command.name(), "source": call.source.as_str(), "client": call.peer.client}))
        });
        Response::decode(&answer, 1).unwrap()
    }

    #[test]
    fn a_session_starts_with_hello() {
        let mut session = Session::new(info());
        let status = r#"{"v":1,"id":"s1","source":"Tests","cmd":"status"}"#;
        let early = call(&mut session, status);
        assert_eq!(early.id, "s1");
        assert_eq!(early.into_result().unwrap_err().code, ErrorCode::BadRequest);
        assert!(session.peer().is_none());

        let hello = Request::new(1, "h", &source(), &Command::Hello(hello_args())).to_line().unwrap();
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
        let line = Request::new(1, "h", &source(), &Command::Hello(future)).to_line().unwrap();
        let e = call(&mut session, &line).into_result().unwrap_err();
        assert_eq!(e.code, ErrorCode::ProtocolMismatch);
        assert!(session.peer().is_none());

        let line = Request::new(1, "h2", &source(), &Command::Hello(hello_args())).to_line().unwrap();
        assert!(call(&mut session, &line).ok);
        assert_eq!(call(&mut session, "{").into_result().unwrap_err().code, ErrorCode::BadRequest);
        let answer = call(&mut session, r#"{"v":1,"id":"x","cmd":"teleport"}"#);
        assert_eq!(answer.id, "x");
        assert_eq!(answer.into_result().unwrap_err().code, ErrorCode::UnknownCommand);
        let failing = session.handle_line(r#"{"v":1,"id":"f","cmd":"keys"}"#, |_, _| {
            Err(AgentError::new(ErrorCode::NoKeyboard, "No keyboard is connected."))
        });
        assert_eq!(Response::decode(&failing, 1).unwrap().into_result().unwrap_err().code, ErrorCode::NoKeyboard);
    }

    #[test]
    fn newer_builds_meet_older_ones_at_the_highest_common_version() {
        let future_hello = r#"{"v":1,"id":"h","cmd":"hello","args":{"client":"keymotion-cli","version":"9.0.0","protocol":{"min":1,"max":2}}}"#;

        // A future keymotion-cli speaking 1 and 2 says hello as v1, like every
        // build, and today's app agrees on 1.
        let mut app = Session::new(info());
        let answer = call(&mut app, future_hello);
        assert_eq!(answer.v, HELLO_VERSION);
        let data: HelloData = serde_json::from_value(answer.into_result().unwrap()).unwrap();
        assert_eq!(data.protocol, 1);
        // From then on the connection speaks 1: a v2 message is refused, a v1
        // one answered as v1.
        let refused = call(&mut app, r#"{"v":2,"id":"a","cmd":"status"}"#);
        assert_eq!(refused.into_result().unwrap_err().code, ErrorCode::ProtocolMismatch);
        let answered = call(&mut app, r#"{"v":1,"id":"b","cmd":"status"}"#);
        assert_eq!((answered.v, answered.ok), (1, true));

        // A future app speaking 1 and 2 with today's client: 1 again.
        let mut future_app = Session::new(info()).speaking(ProtocolRange { min: 1, max: 2 });
        let hello = Request::new(1, "h", &source(), &Command::Hello(hello_args())).to_line().unwrap();
        let data: HelloData = serde_json::from_value(call(&mut future_app, &hello).into_result().unwrap()).unwrap();
        assert_eq!(data.protocol, 1);
        let status = Request::new(data.protocol, "s", &source(), &Command::Status(StatusArgs {})).to_line().unwrap();
        let answered = call(&mut future_app, &status);
        assert_eq!((answered.v, answered.ok), (1, true));

        // Two future builds agree on 2; later messages carry it, hello stays v1.
        let mut both = Session::new(info()).speaking(ProtocolRange { min: 1, max: 2 });
        let answer = call(&mut both, future_hello);
        assert_eq!(answer.v, HELLO_VERSION);
        let data: HelloData = serde_json::from_value(answer.into_result().unwrap()).unwrap();
        assert_eq!(data.protocol, 2);
        let status = Request::new(2, "s", &source(), &Command::Status(StatusArgs {}));
        assert_eq!(status.v, 2);
        let line = both.handle_line(&status.to_line().unwrap(), |_, _| Ok(json!({})));
        let answered = Response::decode(&line, 2).unwrap();
        assert_eq!((answered.v, answered.ok), (2, true));
        assert_eq!(Response::decode(&line, 1).unwrap_err().code, ErrorCode::ProtocolMismatch);
        assert_eq!(Request::new(2, "h", &source(), &Command::Hello(hello_args())).v, HELLO_VERSION);

        // hello itself never leaves v1.
        let v2_hello = future_hello.replacen(r#""v":1"#, r#""v":2"#, 1);
        let refused = call(&mut Session::new(info()), &v2_hello);
        assert_eq!(refused.into_result().unwrap_err().code, ErrorCode::ProtocolMismatch);
        // A refusal reads in any version: its envelope is frozen.
        let refusal = Response::error("r", AgentError::new(ErrorCode::NoKeyboard, "x")).with_version(7).to_line();
        assert_eq!(Response::decode(&refusal, 1).unwrap().into_result().unwrap_err().code, ErrorCode::NoKeyboard);
    }
}
