// C# Extractor Tests - modularized to keep individual files manageable

// Submodule declarations
pub mod extractor;

use crate::extractors::base::{Symbol, SymbolKind, Visibility};
use crate::extractors::csharp::CSharpExtractor;
use tree_sitter::Parser;

/// Initialize C# parser for testing and share across modules
pub fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .expect("Error loading C# grammar");
    parser
}

pub(crate) trait VisibilityExt {
    fn to_string(&self) -> String;
}

impl VisibilityExt for Visibility {
    fn to_string(&self) -> String {
        match self {
            Visibility::Public => "public".to_string(),
            Visibility::Private => "private".to_string(),
            Visibility::Protected => "protected".to_string(),
        }
    }
}

pub(crate) fn get_csharp_visibility(symbol: &Symbol) -> String {
    if let Some(csharp_visibility) = symbol
        .metadata
        .as_ref()
        .and_then(|m| m.get("csharp_visibility"))
        .and_then(|v| v.as_str())
    {
        return csharp_visibility.to_string();
    }

    symbol
        .visibility
        .as_ref()
        .map_or("private".to_string(), |v| VisibilityExt::to_string(v))
}

pub mod core;
pub mod cross_file_relationships;
pub mod identifier_extraction;
pub mod language_features;
pub mod metadata;
pub mod runtime;
mod types; // Phase 4: Type extraction verification tests
