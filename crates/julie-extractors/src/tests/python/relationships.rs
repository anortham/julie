//! Inline tests extracted from src/extractors/python/relationships.rs
//!
//! Tests for Python relationship extraction (inheritance, calls)

use crate::base::RelationshipKind;
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
    let mut extractor = crate::python::PythonExtractor::new(
        "test.py".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    let base = symbols
        .iter()
        .find(|symbol| symbol.name == "Base")
        .expect("Should extract Base class");
    let derived = symbols
        .iter()
        .find(|symbol| symbol.name == "Derived")
        .expect("Should extract Derived class");

    let extends: Vec<_> = relationships
        .iter()
        .filter(|relationship| relationship.kind == RelationshipKind::Extends)
        .collect();
    assert_eq!(extends.len(), 1);
    assert_eq!(extends[0].from_symbol_id, derived.id);
    assert_eq!(extends[0].to_symbol_id, base.id);
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
    let mut extractor = crate::python::PythonExtractor::new(
        "test.py".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    let base1 = symbols
        .iter()
        .find(|symbol| symbol.name == "Base1")
        .expect("Should extract Base1");
    let base2 = symbols
        .iter()
        .find(|symbol| symbol.name == "Base2")
        .expect("Should extract Base2");
    let derived = symbols
        .iter()
        .find(|symbol| symbol.name == "Derived")
        .expect("Should extract Derived");

    let extends_targets: std::collections::HashSet<_> = relationships
        .iter()
        .filter(|relationship| {
            relationship.kind == RelationshipKind::Extends
                && relationship.from_symbol_id == derived.id
        })
        .map(|relationship| relationship.to_symbol_id.as_str())
        .collect();

    assert_eq!(extends_targets.len(), 2);
    assert!(extends_targets.contains(base1.id.as_str()));
    assert!(extends_targets.contains(base2.id.as_str()));
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
    let mut extractor = crate::python::PythonExtractor::new(
        "test.py".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    let caller = symbols
        .iter()
        .find(|symbol| symbol.name == "caller")
        .expect("Should extract caller");
    let callee = symbols
        .iter()
        .find(|symbol| symbol.name == "callee")
        .expect("Should extract callee");

    let calls: Vec<_> = relationships
        .iter()
        .filter(|relationship| relationship.kind == RelationshipKind::Calls)
        .collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].from_symbol_id, caller.id);
    assert_eq!(calls[0].to_symbol_id, callee.id);
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
    let mut extractor = crate::python::PythonExtractor::new(
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
    let mut extractor = crate::python::PythonExtractor::new(
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
    let mut extractor = crate::python::PythonExtractor::new(
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
    let mut extractor = crate::python::PythonExtractor::new(
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
    let mut extractor = crate::python::PythonExtractor::new(
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
