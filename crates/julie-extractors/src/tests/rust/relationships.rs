// Tests for Rust relationship extraction with scoped/qualified paths
//
// Scoped calls should preserve namespace metadata instead of resolving by bare name.

use crate::base::{Relationship, RelationshipKind, StructuredPendingRelationship, Symbol};
use crate::rust::RustExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Error loading Rust grammar");
    parser
}

fn extract_with_relationships(
    code: &str,
) -> (
    Vec<Symbol>,
    Vec<Relationship>,
    Vec<StructuredPendingRelationship>,
) {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    let structured_pending_relationships = extractor.get_structured_pending_relationships();
    (symbols, relationships, structured_pending_relationships)
}

#[test]
fn test_scoped_call_creates_pending_target_with_namespace() {
    let code = r#"
fn target_function() {}

fn caller() {
    crate::module::target_function();
}
"#;
    let (_symbols, relationships, structured_pending) = extract_with_relationships(code);

    assert!(
        relationships.is_empty(),
        "Scoped calls without scoped resolution evidence should not resolve by bare name"
    );

    let pending = structured_pending
        .iter()
        .find(|pending| pending.target.display_name == "crate::module::target_function")
        .expect("scoped call should create structured pending target metadata");
    assert_eq!(pending.target.terminal_name, "target_function");
    assert_eq!(pending.target.namespace_path, vec!["crate", "module"]);
}

#[test]
fn test_simple_call_relationship_still_works() {
    let code = r#"
fn do_something() {}

fn caller() {
    do_something();
}
"#;
    let (_symbols, relationships, _pending) = extract_with_relationships(code);

    assert!(
        !relationships.is_empty(),
        "Simple direct calls should still produce relationships"
    );
}

#[test]
fn test_std_hashmap_new_scoped_call_preserves_namespace_without_local_resolution() {
    let code = r#"
fn new() {}

fn example() {
    std::collections::HashMap::new();
}
"#;
    let (symbols, relationships, structured_pending) = extract_with_relationships(code);

    let local_new = symbols
        .iter()
        .find(|symbol| symbol.name == "new")
        .expect("local new function should be extracted");
    assert!(
        !relationships
            .iter()
            .any(|relationship| relationship.kind == RelationshipKind::Calls
                && relationship.to_symbol_id == local_new.id),
        "std::collections::HashMap::new() must not resolve to the local new function"
    );

    let pending = structured_pending
        .iter()
        .find(|pending| pending.target.display_name == "std::collections::HashMap::new")
        .expect("scoped HashMap::new call should create structured pending target metadata");
    assert_eq!(pending.target.terminal_name, "new");
    assert_eq!(
        pending.target.namespace_path,
        vec!["std", "collections", "HashMap"]
    );
}

#[test]
fn test_crate_scoped_call_preserves_namespace_in_pending_target() {
    let code = r#"
fn caller() {
    crate::search::hybrid::should_use_semantic_fallback();
}
"#;
    let (_symbols, _relationships, structured_pending) = extract_with_relationships(code);

    let pending = structured_pending
        .iter()
        .find(|pending| {
            pending.target.display_name == "crate::search::hybrid::should_use_semantic_fallback"
        })
        .expect("crate-scoped call should create structured pending target metadata");
    assert_eq!(pending.target.terminal_name, "should_use_semantic_fallback");
    assert_eq!(
        pending.target.namespace_path,
        vec!["crate", "search", "hybrid"]
    );
}
