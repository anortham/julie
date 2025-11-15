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

pub mod creation_methods;
pub mod extractor;
pub mod tree_methods;
pub mod types;

// Re-export key types for external use
pub use extractor::BaseExtractor;
pub use types::{
    ContextConfig, ExtractionResults, Identifier, IdentifierKind, Relationship, RelationshipKind,
    Symbol, SymbolKind, SymbolOptions, TypeInfo, Visibility,
};
