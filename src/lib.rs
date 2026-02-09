// Julie - Cross-Platform Code Intelligence Server Library
//!
//! Julie provides code intelligence across 26+ programming languages using
//! Tantivy search with code-aware tokenization (CamelCase/snake_case splitting).

pub mod database;
pub mod extractors;
pub mod search;
pub mod handler;
pub mod mcp_compat; // MCP compatibility layer for rmcp migration
pub mod health;
pub mod language; // Shared language support (tree-sitter configuration)
pub mod startup; // Startup utilities (indexing checks, staleness detection)
pub mod tools;
pub mod tracing;
pub mod utils;
pub mod watcher;
pub mod user_registry;
pub mod workspace;

#[cfg(test)]
pub mod tests;

// #[cfg(test)]
// mod test_parser_pool;

// #[cfg(test)]
// mod test_async_extraction;

// #[cfg(test)]
// mod debug_test;

// Re-export common types
pub use extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
pub use tracing::{
    ArchitecturalLayer, ConnectionType, CrossLanguageTracer, DataFlowTrace, TraceOptions, TraceStep,
};
pub use workspace::{JulieWorkspace, WorkspaceConfig, WorkspaceHealth};
