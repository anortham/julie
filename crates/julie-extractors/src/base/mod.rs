// Base Extractor Types and Traits for Julie
//
// This module is a precise Implementation of base-extractor.ts (404 lines).
// Every method, utility, and algorithm has been carefully ported to maintain exact functionality.
//
// CRITICAL: This represents months of development work. Any changes must maintain
// 100% functional parity with extractors and pass all tests.
//
// Refactored from monolithic 1090-line file into modular structure:
// - types.rs: All data structures (Symbol, Identifier, Relationship, TypeInfo, etc.)
// - extractor.rs: BaseExtractor implementation (core methods)
// - tree_methods.rs: Tree navigation and traversal methods

pub mod annotations;
pub mod creation_methods;
pub mod extractor;
pub mod relationship_resolution;
mod results_normalization;
pub mod span;
pub mod tree_methods;
pub mod types;

// Re-export key types for external use
pub use annotations::normalize_annotations;
pub use extractor::BaseExtractor;
pub use relationship_resolution::{StructuredPendingRelationship, UnresolvedTarget};
pub use span::{NormalizedSpan, RecordOffset, normalize_file_path};
pub use tree_methods::{find_child_by_type, find_child_by_types};
pub use types::{
    AnnotationMarker, ContextConfig, ExtractionResults, Identifier, IdentifierKind,
    PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions,
    TestRole, TypeInfo, Visibility,
};
