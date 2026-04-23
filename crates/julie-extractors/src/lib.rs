//! Julie's Language Extractors Library
//!
//! Cross-platform code intelligence extractors for 34 languages, plus JSX and TSX aliases.
//! Each extractor is responsible for parsing source code and extracting symbols, relationships,
//! and type information using tree-sitter parsers.
//!
//! # Usage
//!
//! ```rust,ignore
//! use julie_extractors::{ExtractorManager, Symbol, SymbolKind};
//!
//! let manager = ExtractorManager::new();
//! let results = manager.extract_all("src/main.rs", content, workspace_root)?;
//! let symbols = results.symbols;
//! ```
//!
//! # Supported Languages (33 concrete extractors, plus JSX and TSX aliases)
//!
//! **Systems**: Rust, C, C++, Go, Zig
//! **Web**: TypeScript, JavaScript, HTML, CSS, Vue, QML
//! **Backend**: Python, Java, C#, PHP, Ruby, Swift, Kotlin, Dart
//! **Functional**: Elixir, Scala
//! **Scripting**: Lua, R, Bash, PowerShell
//! **Specialized**: GDScript, Razor, SQL, Regex
//! **Documentation**: Markdown, JSON, TOML, YAML

// Core infrastructure
pub mod base;
mod factory;
pub mod language;
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
    PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions,
    TypeInfo, Visibility, normalize_annotations,
};

// Re-export the public API - canonical extraction functions
pub use manager::ExtractorManager;
pub use pipeline::extract_canonical;
pub use registry::{LanguageCapabilities, LanguageRegistryEntry};

// Re-export BaseExtractor for language implementors
pub use base::BaseExtractor;

// Re-export test detection
pub use test_detection::is_test_symbol;

// Re-export language detection utilities
pub use language::{detect_language_from_extension, get_tree_sitter_language};

// Tests module (only compiled during testing)
#[cfg(test)]
pub mod tests;
