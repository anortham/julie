//! Inline tests extracted from src/extractors/javascript/relationships.rs
//!
//! Tests for JavaScript relationship extraction (calls, inheritance)

use crate::base::RelationshipKind;
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
    let mut extractor = crate::javascript::JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    assert!(
        !relationships.is_empty(),
        "Should extract call relationships"
    );
    assert!(
        relationships
            .iter()
            .any(|r| r.kind == RelationshipKind::Calls)
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
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::javascript::JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

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
fn test_exported_class_method_call_resolves_unique_local_function() {
    let code = r#"
    export class Worker {
        run() {
            return helper(this.id);
        }
    }

    function helper(value) {
        return value;
    }
    "#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::javascript::JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    let run = symbols
        .iter()
        .find(|symbol| symbol.name == "run")
        .expect("run method symbol");
    let helper = symbols
        .iter()
        .find(|symbol| symbol.name == "helper")
        .expect("helper function symbol");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::Calls
                && relationship.from_symbol_id == run.id
                && relationship.to_symbol_id == helper.id
        }),
        "expected run -> helper relationship, symbols: {:?}, relationships: {:?}",
        symbols
            .iter()
            .map(|symbol| (
                symbol.name.as_str(),
                &symbol.kind,
                symbol.start_byte,
                symbol.end_byte
            ))
            .collect::<Vec<_>>(),
        relationships
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
    let mut extractor = crate::javascript::JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
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
            .any(|r| r.kind == RelationshipKind::Extends)
    );
}
