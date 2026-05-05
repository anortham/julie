//! Inline tests extracted from src/extractors/typescript/relationships.rs
//!
//! These tests verify the relationship extraction functionality for TypeScript code,
//! including function calls and inheritance relationships.

use crate::base::RelationshipKind;
use crate::typescript::TypeScriptExtractor;
use crate::typescript::relationships::extract_relationships;
use std::path::PathBuf;

#[test]
fn test_extract_call_relationships() {
    let code = r#"
    function caller() {
        callee();
    }
    function callee() {}
    "#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");

    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extract_relationships(&mut extractor, &tree, &symbols);

    assert!(!relationships.is_empty());
    assert!(
        relationships
            .iter()
            .any(|r| r.kind == RelationshipKind::Calls)
    );
}

#[test]
fn test_method_call_relationships() {
    // A method inside a class calls a standalone function.
    // Before the fix, find_containing_function only matched SymbolKind::Function,
    // so calls originating from methods/constructors were silently dropped.
    let code = r#"
    function helper() {
        return 42;
    }

    class Service {
        process() {
            helper();
        }
    }
    "#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");

    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extract_relationships(&mut extractor, &tree, &symbols);

    // There must be a Calls relationship from process -> helper
    let calls: Vec<_> = relationships
        .iter()
        .filter(|r| r.kind == RelationshipKind::Calls)
        .collect();
    assert!(
        !calls.is_empty(),
        "Expected a Calls relationship from method 'process' to function 'helper', but got none. \
         Symbols: {:?}",
        symbols
            .iter()
            .map(|s| (&s.name, &s.kind))
            .collect::<Vec<_>>()
    );

    // Verify the relationship points from the method to the function
    let helper_symbol = symbols
        .iter()
        .find(|s| s.name == "helper")
        .expect("helper symbol");
    let process_symbol = symbols
        .iter()
        .find(|s| s.name == "process")
        .expect("process symbol");
    assert!(
        calls
            .iter()
            .any(|r| r.from_symbol_id == process_symbol.id && r.to_symbol_id == helper_symbol.id),
        "Expected Calls relationship from 'process' to 'helper'"
    );
}

#[test]
fn test_this_receiver_call_resolves_to_same_class_method() {
    let code = r#"
    class A {
        render() {}
        caller() {
            this.render();
        }
    }

    class B {
        render() {}
    }
    "#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");

    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extract_relationships(&mut extractor, &tree, &symbols);

    let class_a = symbols
        .iter()
        .find(|symbol| symbol.name == "A")
        .expect("A class symbol");
    let class_b = symbols
        .iter()
        .find(|symbol| symbol.name == "B")
        .expect("B class symbol");
    let caller = symbols
        .iter()
        .find(|symbol| symbol.name == "caller")
        .expect("caller method symbol");
    let a_render = symbols
        .iter()
        .find(|symbol| symbol.name == "render" && symbol.parent_id.as_deref() == Some(&class_a.id))
        .expect("A.render method symbol");
    let b_render = symbols
        .iter()
        .find(|symbol| symbol.name == "render" && symbol.parent_id.as_deref() == Some(&class_b.id))
        .expect("B.render method symbol");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::Calls
                && relationship.from_symbol_id == caller.id
                && relationship.to_symbol_id == a_render.id
        }),
        "this.render() should resolve to A.render"
    );
    assert!(
        !relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::Calls
                && relationship.from_symbol_id == caller.id
                && relationship.to_symbol_id == b_render.id
        }),
        "this.render() must not resolve to B.render"
    );
}

#[test]
fn test_extract_inheritance_relationships() {
    let code = r#"
    class Animal {}
    class Dog extends Animal {}
    "#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");

    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extract_relationships(&mut extractor, &tree, &symbols);

    assert!(!relationships.is_empty());
    assert!(
        relationships
            .iter()
            .any(|r| r.kind == RelationshipKind::Extends)
    );
}
