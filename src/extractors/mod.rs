//! Julie's Language Extractors Module
//!
//! This module contains all the tree-sitter based extractors for various programming languages.
//! Each extractor is responsible for parsing source code and extracting symbols, relationships,
//! and type information using tree-sitter parsers.
//!
//! # Architecture
//!
//! The module is organized into several sub-modules:
//! - `base.rs` - Base extractor trait and common types
//! - `manager.rs` - ExtractorManager public API
//! - `routing_symbols.rs` - Symbol extraction routing (private)
//! - `routing_identifiers.rs` - Identifier extraction routing (private)
//! - `routing_relationships.rs` - Relationship extraction routing (private)
//! - `factory.rs` - Shared factory function and tests
//! - Language modules (rust, typescript, python, etc.)

pub mod base;
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

// Re-export the public API
pub use base::{
    ExtractionResults, Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol,
    SymbolKind,
};
pub use factory::extract_symbols_and_relationships;
pub use manager::ExtractorManager;
