// Julie - Cross-Platform Code Intelligence Server Library
//!
//! Julie provides code intelligence across 26+ programming languages using
//! a three-pillar architecture: SQLite (truth), Tantivy (search), FastEmbed (semantic).

pub mod database;
pub mod embeddings;
pub mod extractors;
pub mod handler;
pub mod search;
pub mod tools;
pub mod tracing;
pub mod utils;
pub mod watcher;
pub mod workspace;

#[cfg(test)]
pub mod tests;

// Re-export common types
pub use extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
pub use tracing::{
    ArchitecturalLayer, ConnectionType, CrossLanguageTracer, DataFlowTrace, TraceOptions, TraceStep,
};
pub use workspace::{JulieWorkspace, WorkspaceConfig, WorkspaceHealth};
