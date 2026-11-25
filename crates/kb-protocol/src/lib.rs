//! Keyboard protocol families, as pure functions over bytes.
//!
//! Nothing here performs I/O, so every packet can be unit-tested and the
//! crate can be compiled to WebAssembly for a future WebHID front end.
//!
//! Safety model: every packet that reaches a device must pass its family's
//! guard first ([`sonix::guard`], [`aa::guard::guard`]). The guards reject
//! bootloader/ISP sequences and any command that is not on the allowlist,
//! regardless of what the caller intended.

pub mod aa;
pub mod sonix;
