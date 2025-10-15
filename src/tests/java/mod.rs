// Java Extractor Tests - Split Module Structure
//
// Direct port of Miller's Java extractor tests (TDD RED phase)
// Original: /Users/murphy/Source/miller/src/__tests__/parser/java-extractor.test.ts
//
// Split into focused test modules for better maintainability:

pub mod annotation_tests;
pub mod class_tests;
pub mod generic_tests;
pub mod identifier_extraction;
pub mod interface_tests;
pub mod method_tests;
pub mod modern_java_tests;
pub mod package_import_tests;
// TODO: Add more modules as they are extracted from the large java_tests.rs file
// pub mod field_tests;
// pub mod enum_tests;
// pub mod generic_tests;
// pub mod nested_class_tests;
// pub mod modern_java_tests;
// pub mod advanced_features_tests;
// pub mod exception_tests;
// pub mod testing_patterns_tests;
// pub mod java_specific_tests;
// pub mod performance_tests;
// pub mod type_inference_tests;
// pub mod relationship_tests;

use crate::extractors::base::{SymbolKind, Visibility};
use crate::extractors::java::JavaExtractor;
use tree_sitter::Parser;

/// Initialize Java parser (shared across all test modules)
pub fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .expect("Error loading Java grammar");
    parser
}
