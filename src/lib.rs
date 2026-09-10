// Julie - Cross-Platform Code Intelligence Server Library
//!
//! Julie provides code intelligence across 36 programming languages using
//! Tantivy search with code-aware tokenization (CamelCase/snake_case splitting).

pub use julie_index::analysis;
pub mod cli;
pub mod cli_tools;
pub mod embeddings;
pub mod external_extract;
pub mod extractors;
pub mod handler;
pub mod health;
pub mod indexing_core;
pub mod language;
pub mod logging;
pub mod mcp_compat;
pub use julie_index::search;
pub mod startup;
pub(crate) mod startup_repair_plan;
pub mod tools;
pub mod utils;
pub use julie_runtime::watcher;
pub use julie_runtime::workspace;

pub mod dashboard;
pub mod paths;
pub mod registry;
pub mod request_engine;
pub mod service;
pub mod workspace_runtime;

#[cfg(test)]
pub mod tests;

// Re-export common types
pub use extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
pub use workspace::{JulieWorkspace, WorkspaceConfig, WorkspaceHealth};
