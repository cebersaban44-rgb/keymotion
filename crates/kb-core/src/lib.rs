//! High-level keyboard operations shared by the desktop app and the CLI.

pub mod clock;
pub mod color;
pub mod conflicts;
pub mod detect;
pub mod keymap;
pub mod readback;
pub mod screen_import;
pub mod shape;
pub mod sonix;

pub use detect::{app_capabilities, detect, wired_count, DetectedKeyboard};
