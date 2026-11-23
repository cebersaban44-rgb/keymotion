//! The coding agents' side of Keymotion's agent tools (plan:
//! `ajan-cli-plan.md` §12.2).
//!
//! * [`event`]: what a Claude Code or Codex hook event means for the
//!   keyboard (`keymotion-cli hook claude-code|codex` reads it from stdin).
//! * [`setup`]: connecting keymotion-cli to Claude Code and Codex, and
//!   taking it away again: their hooks (one fixed line per agent that never
//!   changes between versions), a Codex rule that lets the agent's own
//!   keymotion-cli calls out of Codex's sandbox, and one thin skill. Only
//!   entries and files of our own are ever written or removed; the user's
//!   are kept as they are, in their order.
//! * [`json`]: JSON that keeps the order of an object's keys.
//!
//! keymotion-cli uses all of it; the app's AI agents page calls [`setup`]
//! directly (`app/src-tauri/src/agent/apps.rs`), and the app refreshes the
//! skill file at startup ([`setup::refresh`]).

pub mod event;
pub mod json;
pub mod setup;

pub use event::Agent;
