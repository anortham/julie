//! CLI tool surface for Julie.
//!
//! Provides shell-first access to Julie's code intelligence tools,
//! with named wrappers for high-frequency commands and a generic
//! fallback for any tool by name.

pub mod subcommands;

pub use subcommands::*;
