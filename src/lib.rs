// Julie - Cross-Platform Code Intelligence Server Library
//!
//! Julie provides code intelligence across 26+ programming languages using
//! a three-pillar architecture: SQLite (truth), Tantivy (search), FastEmbed (semantic).

pub mod extractors;
pub mod database;
pub mod search;
pub mod embeddings;
pub mod utils;
pub mod workspace;

// Re-export common types
pub use workspace::{JulieWorkspace, WorkspaceConfig, WorkspaceHealth};
pub use extractors::{Symbol, SymbolKind, Relationship, RelationshipKind};