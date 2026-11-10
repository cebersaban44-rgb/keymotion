//! High-level keyboard operations shared by the desktop app and the CLI.

pub mod clock;
pub mod color;
pub mod detect;
pub mod keymap;
pub mod readback;
pub mod sonix;

pub use detect::{detect, DetectedKeyboard};
