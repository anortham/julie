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
fn test_receiver_method_call_stays_pending_when_local_method_shares_name() {
    let code = r#"
struct Handler;
struct CallPathTool;

impl Handler {
    fn call_tool(&self) {}

    async fn call_path(&self, params: CallPathTool) {
        params.call_tool(self).await;
    }
}
"#;
    let (symbols, relationships, structured_pending) = extract_with_relationships(code);

    let local_call_tool = symbols
        .iter()
        .find(|symbol| symbol.name == "call_tool")
        .expect("local call_tool method should be extracted");
    assert!(
        !relationships
            .iter()
            .any(|relationship| relationship.kind == RelationshipKind::Calls
                && relationship.to_symbol_id == local_call_tool.id),
        "receiver-qualified params.call_tool() must not resolve to the local call_tool method"
    );

    let pending = structured_pending
        .iter()
        .find(|pending| pending.target.display_name == "params.call_tool")
        .expect("receiver-qualified call should create structured pending target metadata");
    assert_eq!(pending.target.terminal_name, "call_tool");
    assert_eq!(pending.target.receiver.as_deref(), Some("params"));
    assert!(pending.target.namespace_path.is_empty());
}

#[test]
fn test_self_receiver_method_call_resolves_local_method() {
    let code = r#"
struct Handler;

impl Handler {
    fn call_tool(&self) {}

    fn call_path(&self) {
        self.call_tool();
    }
}
"#;
    let (symbols, relationships, structured_pending) = extract_with_relationships(code);

    let local_call_tool = symbols
        .iter()
        .find(|symbol| symbol.name == "call_tool")
        .expect("local call_tool method should be extracted");
    assert!(
        relationships
            .iter()
            .any(|relationship| relationship.kind == RelationshipKind::Calls
                && relationship.to_symbol_id == local_call_tool.id),
        "self.call_tool() should resolve to the local method"
    );
    assert!(
        !structured_pending
            .iter()
            .any(|pending| pending.target.display_name == "self.call_tool"),
        "self.call_tool() should not go through cross-file pending resolution"
    );
}

#[test]
fn test_self_receiver_method_call_uses_same_impl_parent_when_names_duplicate() {
    let code = r#"
struct A;
struct B;

impl A {
    fn render(&self) {}

    fn caller(&self) {
        self.render();
    }
}

impl B {
    fn render(&self) {}
}
"#;
    let (symbols, relationships, structured_pending) = extract_with_relationships(code);

    let struct_a = symbols
        .iter()
        .find(|symbol| symbol.name == "A")
        .expect("A struct should be extracted");
    let struct_b = symbols
        .iter()
        .find(|symbol| symbol.name == "B")
        .expect("B struct should be extracted");
    let caller = symbols
        .iter()
        .find(|symbol| symbol.name == "caller")
        .expect("caller method should be extracted");
    let a_render = symbols
        .iter()
        .find(|symbol| symbol.name == "render" && symbol.parent_id.as_deref() == Some(&struct_a.id))
        .expect("A.render should be extracted");
    let b_render = symbols
        .iter()
        .find(|symbol| symbol.name == "render" && symbol.parent_id.as_deref() == Some(&struct_b.id))
        .expect("B.render should be extracted");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::Calls
                && relationship.from_symbol_id == caller.id
                && relationship.to_symbol_id == a_render.id
        }),
        "self.render() should resolve to A.render"
    );
    assert!(
        !relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::Calls
                && relationship.from_symbol_id == caller.id
                && relationship.to_symbol_id == b_render.id
        }),
        "self.render() must not resolve to B.render"
    );
    assert!(
        !structured_pending
            .iter()
            .any(|pending| pending.target.display_name == "self.render"),
        "self.render() has enough local scope evidence and should not be pending"
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

#[test]
fn test_rust_use_declarations_emit_import_relationships() {
    let code = r#"
use std::collections::HashMap;
use crate::models::User as AppUser;

fn main() {}
"#;
    let (symbols, relationships, structured_pending) = extract_with_relationships(code);

    assert!(
        relationships
            .iter()
            .all(|relationship| relationship.kind != RelationshipKind::Imports),
        "Rust use imports should be modeled as structured pending import relationships"
    );

    let import_pending: Vec<_> = structured_pending
        .iter()
        .filter(|pending| pending.pending.kind == RelationshipKind::Imports)
        .collect();
    assert_eq!(
        import_pending.len(),
        2,
        "Expected one import relationship per use declaration"
    );

    let hash_map_import = symbols
        .iter()
        .find(|symbol| symbol.name == "HashMap")
        .expect("HashMap import symbol should be extracted");
    let hash_map_pending = import_pending
        .iter()
        .find(|pending| pending.target.display_name == "std::collections::HashMap")
        .expect("HashMap use declaration should emit an Imports pending relationship");
    assert_eq!(
        hash_map_pending.pending.from_symbol_id, hash_map_import.id,
        "Import relationship source should be the corresponding use symbol"
    );
    assert_eq!(hash_map_pending.target.terminal_name, "HashMap");
    assert_eq!(
        hash_map_pending.target.namespace_path,
        vec!["std", "collections"]
    );

    let app_user_import = symbols
        .iter()
        .find(|symbol| symbol.name == "AppUser")
        .expect("Aliased import symbol should be extracted");
    let app_user_pending = import_pending
        .iter()
        .find(|pending| pending.target.display_name == "crate::models::User")
        .expect("Aliased use declaration should emit Imports pending relationship to source path");
    assert_eq!(
        app_user_pending.pending.from_symbol_id, app_user_import.id,
        "Import relationship source should match aliased use symbol"
    );
    assert_eq!(app_user_pending.target.terminal_name, "User");
    assert_eq!(
        app_user_pending.target.namespace_path,
        vec!["crate", "models"]
    );
}
