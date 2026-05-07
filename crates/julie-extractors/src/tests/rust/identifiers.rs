// Tests for Rust identifier extraction with scoped/qualified paths
//
// Bug: `crate::module::function()` was indexed as "crate::module::function"
// instead of "function", causing fast_refs to miss the reference.

use crate::base::{Identifier, IdentifierKind, Symbol};
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

fn extract_all(code: &str) -> (Vec<Symbol>, Vec<Identifier>) {
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
    let identifiers = extractor.extract_identifiers(&tree, &symbols);
    (symbols, identifiers)
}

#[test]
fn test_scoped_call_extracts_last_segment() {
    let code = r#"
fn caller() {
    crate::search::hybrid::should_use_semantic_fallback("query", 5);
}
"#;
    let (_symbols, identifiers) = extract_all(code);
    let calls: Vec<&Identifier> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::Call)
        .collect();

    assert!(
        !calls.is_empty(),
        "Should find at least one call identifier"
    );
    let call = calls
        .iter()
        .find(|id| id.name == "should_use_semantic_fallback");
    assert!(
        call.is_some(),
        "Should find should_use_semantic_fallback call, got: {:?}",
        calls.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    assert_eq!(
        call.unwrap().name,
        "should_use_semantic_fallback",
        "Should extract bare name, not qualified path"
    );
}

#[test]
fn test_simple_call_still_works() {
    let code = r#"
fn caller() {
    do_something();
}
fn do_something() {}
"#;
    let (_symbols, identifiers) = extract_all(code);
    let calls: Vec<&Identifier> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::Call)
        .collect();

    assert!(
        calls.iter().any(|c| c.name == "do_something"),
        "Simple calls should still work, got: {:?}",
        calls.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_nested_scoped_call() {
    let code = r#"
fn example() {
    std::collections::HashMap::new();
}
"#;
    let (_symbols, identifiers) = extract_all(code);
    let calls: Vec<&Identifier> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::Call)
        .collect();

    // Should extract "new" as the call name, not the full qualified path
    assert!(
        calls.iter().any(|c| c.name == "new"),
        "Should extract 'new' from HashMap::new(), got: {:?}",
        calls.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_enum_variant_scoped_identifiers_are_type_usages() {
    let code = r#"
enum IndexingRepairReason {
    SemanticVersionChanged,
    StaleFiles,
}

impl IndexingRepairReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::SemanticVersionChanged => "semantic_version_changed",
            Self::StaleFiles => "stale_files",
        }
    }
}

fn mark(reasons: &mut Vec<IndexingRepairReason>) {
    reasons.push(IndexingRepairReason::SemanticVersionChanged);
}
"#;
    let (_symbols, identifiers) = extract_all(code);
    let variant_usages: Vec<&Identifier> = identifiers
        .iter()
        .filter(|id| id.name == "SemanticVersionChanged" && id.kind == IdentifierKind::TypeUsage)
        .collect();

    assert_eq!(
        variant_usages.len(),
        2,
        "Should extract enum variant usages from Self::Variant and Enum::Variant, got: {:?}",
        identifiers
            .iter()
            .map(|id| (&id.name, &id.kind, id.start_line))
            .collect::<Vec<_>>()
    );
    assert!(
        variant_usages
            .iter()
            .all(|id| id.containing_symbol_id.is_some()),
        "Enum variant usages should retain containing symbols"
    );
}

#[test]
fn test_rust_type_usage_identifiers_cover_type_identifier_nodes() {
    let code = r#"
struct UserService {
    repository: Repository<User>,
}

struct LoginRequest;
struct AuthResult;
struct Repository<T> { item: T }
struct User;
struct Error;

impl UserService {
    fn login(&self, request: &LoginRequest, users: Vec<User>) -> Result<AuthResult, Error> {
        let fallback: Option<User> = None;
        todo!()
    }
}
"#;
    let (_symbols, identifiers) = extract_all(code);
    let type_names: Vec<&str> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .map(|id| id.name.as_str())
        .collect();

    for expected in [
        "Repository",
        "User",
        "LoginRequest",
        "Vec",
        "Result",
        "AuthResult",
        "Error",
        "Option",
    ] {
        assert!(
            type_names.contains(&expected),
            "missing Rust type usage {expected}; got {type_names:?}"
        );
    }
}
