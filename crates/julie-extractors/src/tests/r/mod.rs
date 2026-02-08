// R Extractor Test Suite
// Comprehensive test coverage following patterns from QML and other extractors
// Structure mirrors established test organization

use crate::base::{Identifier, Relationship, Symbol};
use crate::r::RExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

/// Helper function to extract symbols from R code
/// Mirrors the pattern from QML and other extractors
pub fn extract_symbols(code: &str) -> Vec<Symbol> {
    let tree = init_parser(code, "r");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = RExtractor::new(
        "r".to_string(),
        "test.R".to_string(),
        code.to_string(),
        &workspace_root,
    );
    extractor.extract_symbols(&tree)
}

/// Helper function to extract both symbols and relationships
/// Used for tests that need to verify symbol connections
pub fn extract_symbols_and_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let tree = init_parser(code, "r");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = RExtractor::new(
        "r".to_string(),
        "test.R".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

/// Helper function to extract identifiers from R code
/// Used for tests that verify identifier extraction (calls, member access, variable refs)
pub fn extract_identifiers(code: &str) -> Vec<Identifier> {
    let tree = init_parser(code, "r");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = RExtractor::new(
        "r".to_string(),
        "test.R".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    extractor.extract_identifiers(&tree, &symbols)
}

/// Helper function to extract symbols, relationships, and identifiers together
/// Used for tests that need to verify scope_id on identifiers and relationship targets
pub fn extract_all(code: &str) -> (Vec<Symbol>, Vec<Relationship>, Vec<Identifier>) {
    let tree = init_parser(code, "r");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = RExtractor::new(
        "r".to_string(),
        "test.R".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);
    (symbols, relationships, identifiers)
}

// Test module organization
// Each module focuses on a specific aspect of R functionality
pub mod basics; // Core R: functions, assignments, variables
pub mod classes; // S3, S4, R6 class systems
pub mod control_flow; // if/else, loops, vectorized operations
pub mod cross_file_relationships; // Cross-file relationship resolution (pending relationships)
pub mod data_structures; // data.frame, tibble, vector, list, matrix
pub mod file_integration_bug; // BUG HUNT: Reproduction test for file extraction failure
pub mod functions; // Function definitions, parameters, closures
pub mod identifiers; // Identifier extraction (calls, member access, variable refs)
pub mod modern; // Modern R patterns (tidyverse, data.table)
pub mod packages; // library(), require(), package::function syntax
pub mod real_world; // Real-world validation (ggplot2, dplyr)
pub mod relationships; // Relationship extraction (calls, pipes, library usage)
pub mod tidyverse; // %>% pipes, dplyr verbs, ggplot2 patterns
