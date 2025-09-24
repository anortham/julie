// Julie's Language Extractors Module
//
// This module contains all the tree-sitter based extractors for various programming languages.
// Each extractor is responsible for parsing source code and extracting symbols, relationships,
// and type information using tree-sitter parsers.

pub mod base;

// TODO: Implement language extractors (Phase 1 & 2)
// Phase 1 - Core Languages:
// pub mod typescript;
// pub mod javascript;
// pub mod python;
// pub mod rust;
// pub mod go;

// Phase 2 - Extended Languages:
// pub mod c;
// pub mod cpp;
// pub mod java;
// pub mod csharp;
// pub mod ruby;
// pub mod php;
// pub mod swift;
// pub mod kotlin;

// Phase 2 - Specialized Languages:
// pub mod gdscript;
// pub mod lua;
// pub mod vue;
// pub mod razor;
// pub mod sql;
// pub mod html;
// pub mod css;
// pub mod regex;
// pub mod bash;

// Re-export the base extractor types
pub use base::{BaseExtractor, Symbol, SymbolKind, Relationship, RelationshipKind, TypeInfo};

/// Manager for all language extractors
pub struct ExtractorManager {
    // TODO: Store language parsers and extractors
}

impl ExtractorManager {
    pub fn new() -> Self {
        Self {
            // TODO: Initialize
        }
    }

    /// Get supported languages
    pub fn supported_languages(&self) -> Vec<&'static str> {
        vec![
            // TODO: Return actual supported languages as they are implemented
            "placeholder"
        ]
    }
}