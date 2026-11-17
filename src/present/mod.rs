//! Presentation that doesn't depend on a terminal UI: formatting helpers and
//! the task view's content as text segments tagged with a [`Role`]. The TUI
//! colors them with a theme; the CLI prints them as plain text.

pub mod detail;
pub mod format;
mod role;

pub use role::Role;
