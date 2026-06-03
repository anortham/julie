// Julie - Cross-Platform Code Intelligence Server Library
//!
//! Julie provides code intelligence across 34 programming languages using
//! Tantivy search with code-aware tokenization (CamelCase/snake_case splitting).

pub use julie_index::analysis;
pub mod cli;
pub mod cli_tools;
pub use julie_core::database;
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
pub mod tools;
pub mod utils;
pub mod watcher;
pub mod workspace;

pub mod adapter;
pub mod daemon;
pub mod dashboard;
pub mod migration;
pub mod paths;

#[cfg(test)]
pub mod tests;

// Re-export common types
pub use extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
pub use workspace::{JulieWorkspace, WorkspaceConfig, WorkspaceHealth};
