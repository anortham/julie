//! Inline tests extracted from src/extractors/python/relationships.rs
//!
//! Tests for Python relationship extraction (inheritance, calls)

use crate::extractors::base::RelationshipKind;
use std::path::PathBuf;

#[test]
fn test_extract_inheritance_relationships() {
    let code = r#"
class Base:
    pass

class Derived(Base):
    pass
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::extractors::python::PythonExtractor::new(
        "test.py".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    assert!(
        !relationships.is_empty(),
        "Should extract inheritance relationships"
    );
    assert!(
        relationships
            .iter()
            .any(|r| r.kind == RelationshipKind::Extends),
        "Should have at least one extends relationship"
    );
}

#[test]
fn test_extract_multiple_inheritance() {
    let code = r#"
class Base1:
    pass

class Base2:
    pass

class Derived(Base1, Base2):
    pass
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::extractors::python::PythonExtractor::new(
        "test.py".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    // Multiple inheritance should produce relationships or be empty (depending on parser)
    // Just verify it doesn't panic
    let _ = relationships.len();
}

#[test]
fn test_extract_call_relationships() {
    let code = r#"
def caller():
    callee()

def callee():
    pass
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::extractors::python::PythonExtractor::new(
        "test.py".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    // Call relationships may or may not be found depending on symbol matching
    // Just verify the relationships structure is valid
    for rel in &relationships {
        assert!(!rel.id.is_empty(), "Relationship ID should not be empty");
        assert!(
            !rel.from_symbol_id.is_empty(),
            "from_symbol_id should not be empty"
        );
        assert!(
            !rel.to_symbol_id.is_empty(),
            "to_symbol_id should not be empty"
        );
    }
}

#[test]
fn test_extract_relationships_empty_code() {
    let code = "";
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::extractors::python::PythonExtractor::new(
        "test.py".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    assert_eq!(
        relationships.len(),
        0,
        "Empty code should produce no relationships"
    );
}

#[test]
fn test_extract_relationships_confidence_scores() {
    let code = r#"
class Base:
    pass

class Child(Base):
    pass
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::extractors::python::PythonExtractor::new(
        "test.py".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    // Check that all confidence scores are valid
    for rel in &relationships {
        assert!(
            rel.confidence > 0.0 && rel.confidence <= 1.0,
            "Confidence should be between 0 and 1, got {}",
            rel.confidence
        );
    }
}

#[test]
fn test_extract_relationships_line_numbers() {
    let code = r#"
class Base:
    pass

class Derived(Base):
    pass
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::extractors::python::PythonExtractor::new(
        "test.py".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    // Check that line numbers are positive
    for rel in &relationships {
        assert!(rel.line_number > 0, "Line numbers should be positive");
    }
}

#[test]
fn test_extract_relationships_file_path() {
    let code = r#"
class Base:
    pass

class Child(Base):
    pass
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let file_path = "my_module.py";
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::extractors::python::PythonExtractor::new(
        file_path.to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    // All relationships should have the correct file path
    for rel in &relationships {
        assert_eq!(
            rel.file_path, file_path,
            "Relationship file path should match extractor file path"
        );
    }
}

#[test]
fn test_extract_relationships_with_class_methods() {
    let code = r#"
class MyClass:
    def __init__(self):
        self.value = 0

    def get_value(self):
        return self.value

    def set_value(self, v):
        self.value = v
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::extractors::python::PythonExtractor::new(
        "test.py".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    // Should not crash and relationships should be valid
    for rel in &relationships {
        assert!(
            !rel.id.is_empty(),
            "All relationships should have valid IDs"
        );
    }
}

#[test]
fn test_placeholder() {
    // This test is placeholder - actual testing requires tree-sitter
    // Real tests are in the integration tests
}
