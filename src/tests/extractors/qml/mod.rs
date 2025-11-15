// QML Extractor Test Suite
// Comprehensive test coverage following patterns from other extractors
// Structure mirrors GDScript, C++, and CSS test organization

use crate::extractors::base::{Relationship, Symbol};
use crate::extractors::qml::QmlExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

/// Helper function to extract symbols from QML code
/// Mirrors the pattern from GDScript and other extractors
pub fn extract_symbols(code: &str) -> Vec<Symbol> {
    let tree = init_parser(code, "qml");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = QmlExtractor::new(
        "qml".to_string(),
        "test.qml".to_string(),
        code.to_string(),
        &workspace_root,
    );
    extractor.extract_symbols(&tree)
}

/// Helper function to extract both symbols and relationships
/// Used for tests that need to verify symbol connections
pub fn extract_symbols_and_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let tree = init_parser(code, "qml");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = QmlExtractor::new(
        "qml".to_string(),
        "test.qml".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

// Test module organization
// Each module focuses on a specific aspect of QML functionality
pub mod animations; // States, transitions, animations
pub mod basics; // Core QML: imports, objects, basic properties
pub mod bindings; // Property bindings and expressions
pub mod components; // Custom components, loaders, repeaters
pub mod functions; // Functions and JavaScript code
pub mod layouts; // Anchors, layouts, positioning
pub mod modern; // Qt 5.x/6.x modern features
pub mod real_world;
pub mod signals; // Signals and signal handlers // Real-world validation (cool-retro-term, KDE)
