use crate::base::{Identifier, IdentifierKind, Symbol};
use crate::python::PythonExtractor;
use std::path::PathBuf;

fn extract_all(code: &str) -> (Vec<Symbol>, Vec<Identifier>, PythonExtractor) {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        PythonExtractor::new("test.py".to_string(), code.to_string(), &workspace_root);
    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);
    (symbols, identifiers, extractor)
}

#[test]
fn test_python_type_usage_identifiers_cover_annotations() {
    let code = r#"
from typing import List, Optional

class UserService: pass
class LoginRequest: pass
class AuthResult: pass
class User: pass

service: UserService
users: list[User]

def login(request: LoginRequest, fallback: Optional[User]) -> AuthResult:
    return AuthResult()
"#;

    let (_symbols, identifiers, _extractor) = extract_all(code);
    let type_names: Vec<&str> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .map(|id| id.name.as_str())
        .collect();

    for expected in [
        "UserService",
        "User",
        "LoginRequest",
        "Optional",
        "AuthResult",
    ] {
        assert!(
            type_names.contains(&expected),
            "missing Python type usage {expected}; got {type_names:?}"
        );
    }
}

#[test]
fn test_python_return_type_hint_uses_annotation_node() {
    let code = r#"
class AuthResult: pass
class User: pass

def login() -> list[AuthResult | User]:
    return []
"#;

    let (symbols, _identifiers, extractor) = extract_all(code);
    let types = extractor.infer_types(&symbols);
    let login = symbols
        .iter()
        .find(|symbol| symbol.name == "login")
        .expect("login function should be extracted");

    assert_eq!(
        types.get(&login.id).map(String::as_str),
        Some("list[AuthResult | User]")
    );
}
