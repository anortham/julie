// C# Extractor Inline Tests
//
// This module contains tests extracted from src/extractors/csharp/mod.rs.
// Tests verify core functionality of the CSharpExtractor including:
// - Basic class extraction
// - Interface extraction
// - Property extraction
// - Enum extraction with members

use crate::extractors::csharp::CSharpExtractor;

#[test]
fn test_basic_class_extraction() {
    let code = r#"
        public class MyClass {
            public void MyMethod() {
            }
        }
    "#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let mut extractor = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
    );
    let symbols = extractor.extract_symbols(&tree);

    // Should extract namespace, class, and method
    assert!(!symbols.is_empty());
    assert!(symbols.iter().any(|s| s.name == "MyClass"));
    assert!(symbols.iter().any(|s| s.name == "MyMethod"));
}

#[test]
fn test_interface_extraction() {
    let code = r#"
        public interface IMyInterface {
            void DoSomething();
        }
    "#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let mut extractor = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
    );
    let symbols = extractor.extract_symbols(&tree);

    assert!(symbols.iter().any(|s| s.name == "IMyInterface"));
}

#[test]
fn test_property_extraction() {
    let code = r#"
        public class MyClass {
            public string Name { get; set; }
        }
    "#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let mut extractor = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
    );
    let symbols = extractor.extract_symbols(&tree);

    assert!(symbols.iter().any(|s| s.name == "Name"));
}

#[test]
fn test_enum_extraction() {
    let code = r#"
        public enum Status {
            Active = 1,
            Inactive = 2
        }
    "#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let mut extractor = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
    );
    let symbols = extractor.extract_symbols(&tree);

    assert!(symbols.iter().any(|s| s.name == "Status"));
    assert!(symbols.iter().any(|s| s.name == "Active"));
    assert!(symbols.iter().any(|s| s.name == "Inactive"));
}
