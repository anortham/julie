// Julie - Cross-Platform Code Intelligence Server Library
//!
//! Julie provides code intelligence across 31 programming languages using
//! Tantivy search with code-aware tokenization (CamelCase/snake_case splitting).

pub mod analysis;
pub mod cli;
pub mod database;
pub mod embeddings;
pub mod extractors;
pub mod handler;
pub mod health;
pub mod language;
pub mod mcp_compat;
pub mod search;
pub mod startup;
pub mod tools;
pub mod tracing;
pub mod utils;
pub mod watcher;
pub mod workspace;

pub mod adapter;
pub mod daemon;
pub mod migration;
pub mod paths;

#[cfg(test)]
pub mod tests;

// Re-export common types
pub use extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
pub use tracing::{
    ArchitecturalLayer, ConnectionType, CrossLanguageTracer, DataFlowTrace, TraceOptions, TraceStep,
};
pub use workspace::{JulieWorkspace, WorkspaceConfig, WorkspaceHealth};
