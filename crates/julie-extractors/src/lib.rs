//! # julie-extractors
//!
//! Tree-sitter-backed code extraction for 34 languages plus TSX/JSX variants.
//! Produces a stable [`ExtractionResults`] shape: symbols, relationships,
//! structured pending relationships, identifiers, type info, and parse
//! diagnostics. Used by Julie's MCP server but consumable from any Rust crate.
//!
//! ## Quickstart
//!
//! ```
//! use julie_extractors::{extract_canonical, capability_snapshot};
//! use std::path::Path;
//!
//! let source = "fn main() { println!(\"hi\"); }";
//! let result = extract_canonical(
//!     "hello.rs",
//!     source,
//!     Path::new("."),
//! ).unwrap();
//! assert!(!result.symbols.is_empty());
//!
//! // Inspect what the crate guarantees for this language:
//! let cap = capability_snapshot().get("rust").unwrap();
//! assert!(cap.target_capabilities.symbols);
//! ```
//!
//! See [`EXTRACTION_CONTRACT_VERSION`] for drift detection. The capability
//! contract lives in `fixtures/extraction/capabilities.json` and is exposed
//! to downstream consumers via [`capability_snapshot()`].
//!
//! ## Supported Languages
//!
//! **Systems**: Rust, C, C++, Go, Zig
//! **Web**: TypeScript, JavaScript, HTML, CSS, Vue, QML
//! **Backend**: Python, Java, C#, VB.NET, PHP, Ruby, Swift, Kotlin, Dart
//! **Functional**: Elixir, Scala
//! **Scripting**: Lua, R, Bash, PowerShell
//! **Specialized**: GDScript, Razor, SQL, Regex
//! **Documentation**: Markdown, JSON, TOML, YAML

// Core infrastructure
pub mod base;
pub mod capability_snapshot;
mod factory;
pub mod language;
mod language_spec;
pub mod manager;
pub mod pipeline;
pub mod registry;
// Compatibility surface for the main crate re-export layer.
// These modules are thin projections over canonical extraction results,
// not separate production dispatch paths.
pub mod routing_identifiers;
pub mod routing_relationships;
pub mod routing_symbols;
pub mod test_calls;
pub mod test_detection;
pub mod utils;

// Language extractors (33 concrete extractors, plus JSX/TSX aliases in the registry)
pub mod bash;
pub mod c;
pub mod cpp;
pub mod csharp;
pub mod css;
pub mod dart;
pub mod elixir;
pub mod gdscript;
pub mod go;
pub mod html;
pub mod java;
pub mod javascript;
pub mod json;
pub mod kotlin;
pub mod lua;
pub mod markdown;
pub mod php;
pub mod powershell;
pub mod python;
pub mod qml;
pub mod r;
pub mod razor;
pub mod regex;
pub mod ruby;
pub mod rust;
pub mod scala;
pub mod sql;
pub mod swift;
pub mod toml;
pub mod typescript;
pub mod vbnet;
pub mod vue;
pub mod yaml;
pub mod zig;

// Re-export the public API - Core types
pub use base::{
    AnnotationMarker, ContextConfig, ExtractionResults, Identifier, IdentifierKind,
    ParseDiagnostic, ParseDiagnosticKind, PendingRelationship, Relationship, RelationshipKind,
    Symbol, SymbolKind, SymbolOptions, TestRole, TypeInfo, Visibility, normalize_annotations,
};

// Re-export the public API - canonical extraction functions
pub use manager::ExtractorManager;
pub use pipeline::extract_canonical;
pub use registry::{LanguageCapabilities, LanguageRegistryEntry};

// Re-export Pillar 3 stable capability snapshot API
pub use capability_snapshot::{
    CapabilityFlags, CapabilityGap, CapabilityRow, CapabilitySnapshot, FixtureRef,
    capability_snapshot,
};

/// Stable extraction-contract version string. Downstream consumers and
/// upstream index engines compose this into their own engine version so that
/// schema/shape drift in extractor outputs triggers a visible mismatch.
///
/// **Stable.** Bump the suffix after `v` when the canonical extraction shape
/// changes in a way downstream consumers must observe.
pub const EXTRACTION_CONTRACT_VERSION: &str = "2026-05-10.tree-sitter-best-in-class-v1";

// Re-export BaseExtractor for language implementors
pub use base::BaseExtractor;

// Re-export test detection
pub use test_detection::is_test_symbol;

// Re-export language detection utilities
pub use language::{detect_language_from_extension, get_tree_sitter_language};

// Tests module (only compiled during testing)
#[cfg(test)]
pub mod tests;
