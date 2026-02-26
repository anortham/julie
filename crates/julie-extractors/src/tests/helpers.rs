//! Shared test utilities for julie-extractors tests
//!
//! Common helpers used across language extractor test suites.

use crate::language::get_tree_sitter_language;

/// Initialize a tree-sitter parser for the given language and parse the code.
///
/// # Arguments
/// * `code` - Source code to parse
/// * `language` - Language identifier (e.g., "go", "csharp", "python")
///
/// # Returns
/// Parsed tree-sitter Tree
///
/// # Panics
/// Panics if the language is not supported or parsing fails.
pub fn init_parser(code: &str, language: &str) -> tree_sitter::Tree {
    let mut parser = tree_sitter::Parser::new();
    let ts_language = get_tree_sitter_language(language)
        .unwrap_or_else(|_| panic!("Unsupported language: {}", language));
    parser
        .set_language(&ts_language)
        .unwrap_or_else(|e| panic!("Failed to set language '{}': {}", language, e));
    parser
        .parse(code, None)
        .unwrap_or_else(|| panic!("Failed to parse {} code", language))
}
