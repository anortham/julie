//! Julie's Language Extractors Library
//!
//! Cross-platform code intelligence extractors for 31 programming languages.
//! Each extractor is responsible for parsing source code and extracting symbols, relationships,
//! and type information using tree-sitter parsers.
//!
//! # Usage
//!
//! ```rust,ignore
//! use julie_extractors::{ExtractorManager, Symbol, SymbolKind};
//!
//! let manager = ExtractorManager::new();
//! let symbols = manager.extract_symbols("src/main.rs", content, workspace_root)?;
//! ```
//!
//! # Supported Languages (31 total)
//!
//! **Systems**: Rust, C, C++, Go, Zig
//! **Web**: TypeScript, JavaScript, HTML, CSS, Vue, QML
//! **Backend**: Python, Java, C#, PHP, Ruby, Swift, Kotlin, Dart
//! **Scripting**: Lua, R, Bash, PowerShell
//! **Specialized**: GDScript, Razor, SQL, Regex
//! **Documentation**: Markdown, JSON, TOML, YAML

// Core infrastructure
pub mod base;
pub mod language;
pub mod utils;
pub mod factory;
pub mod manager;
pub mod routing_identifiers;
pub mod routing_relationships;
pub mod routing_symbols;

// Language extractors (31 total - including documentation/config languages)
pub mod bash;
pub mod c;
pub mod cpp;
pub mod csharp;
pub mod css;
pub mod dart;
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
pub mod sql;
pub mod swift;
pub mod toml;
pub mod typescript;
pub mod vue;
pub mod yaml;
pub mod zig;

// Re-export the public API - Core types
pub use base::{
    ContextConfig, ExtractionResults, Identifier, IdentifierKind, PendingRelationship,
    Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions, TypeInfo, Visibility,
};

// Re-export the public API - Extraction functions
pub use factory::extract_symbols_and_relationships;
pub use manager::ExtractorManager;

// Re-export BaseExtractor for language implementors
pub use base::BaseExtractor;

// Re-export language detection utilities
pub use language::{detect_language_from_extension, get_tree_sitter_language};

// Tests module (only compiled during testing)
#[cfg(test)]
pub mod tests;
