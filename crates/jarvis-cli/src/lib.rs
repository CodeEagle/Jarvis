//! Library half of the Jarvis CLI.
//!
//! The binary's main loop in `main.rs` does only argv dispatch + I/O.
//! Pure logic lives here so it can be unit-tested without spawning
//! processes or hitting stdout.

pub mod cmd;
pub mod render;

pub use cmd::*;
