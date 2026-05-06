use crate::base::{Identifier, IdentifierKind, Symbol};
use crate::go::GoExtractor;
use crate::tests::helpers::init_parser;
use std::path::PathBuf;

fn extract_all(code: &str) -> (Vec<Symbol>, Vec<Identifier>) {
    let tree = init_parser(code, "go");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = GoExtractor::new(
        "go".to_string(),
        "test.go".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);
    (symbols, identifiers)
}

#[test]
fn test_go_type_usage_identifiers_cover_fields_params_returns_and_generics() {
    let code = r#"
package main

type User struct {}
type Request struct {}
type Response struct {}
type Store[T any] struct {}

type Controller struct {
    store *Store[User]
    users []User
    byID map[string]pkg.External
}

func (c *Controller) Load(req Request, users []User) (*Response, error) {
    var current User
    _ = current
    return &Response{}, nil
}
"#;

    let (_symbols, identifiers) = extract_all(code);
    let type_names: Vec<&str> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .map(|id| id.name.as_str())
        .collect();

    for expected in [
        "Controller",
        "Store",
        "User",
        "Request",
        "Response",
        "External",
    ] {
        assert!(
            type_names.contains(&expected),
            "missing Go type usage {expected}; got {type_names:?}"
        );
    }

    assert!(
        !type_names.contains(&"string"),
        "builtin Go type string should not be a TypeUsage: {type_names:?}"
    );
}

#[test]
fn test_go_malformed_struct_recovery_does_not_emit_function_names_as_type_usage() {
    let code = r#"
package main

type Empty struct{}

type EmbeddedStruct struct {
    Empty
}

type MissingBrace struct {
    field int

func VariadicFunction(format string, args ...interface{}) {
    fmt.Printf(format, args...)
}
"#;

    let (_symbols, identifiers) = extract_all(code);
    let type_names: Vec<&str> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .map(|id| id.name.as_str())
        .collect();

    assert!(
        type_names.contains(&"Empty"),
        "valid embedded fields should still be TypeUsage identifiers: {type_names:?}"
    );
    for unexpected in ["VariadicFunction", "format", "Printf"] {
        assert!(
            !type_names.contains(&unexpected),
            "malformed function text should not become a TypeUsage {unexpected}; got {type_names:?}"
        );
    }
}
