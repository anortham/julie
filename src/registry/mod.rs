//! Global project registry — tracks all known projects on this machine.
//!
//! Distinct from `workspace::registry` which manages reference workspaces
//! within a single project. This module provides the machine-wide index
//! stored at `~/.julie/registry.toml`.

pub mod global_registry;

pub use global_registry::{GlobalRegistry, ProjectEntry, ProjectStatus};
