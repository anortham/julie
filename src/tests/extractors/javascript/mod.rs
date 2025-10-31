// JavaScript Extractor Tests
//
// Direct Implementation of JavaScript extractor tests (TDD RED phase)

// Submodule declarations
pub mod error_handling;
pub mod identifier_extraction;
pub mod jsdoc_comments;
pub mod legacy_patterns;
pub mod modern_features;
pub mod relationships;
pub mod scoping;

use crate::extractors::javascript::JavaScriptExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

/// Initialize JavaScript parser for JavaScript files
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("Error loading JavaScript grammar");
    parser
}
