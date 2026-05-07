use crate::base::{Identifier, IdentifierKind, Symbol};
use crate::swift::SwiftExtractor;
use std::path::PathBuf;

fn extract_all(code: &str) -> (Vec<Symbol>, Vec<Identifier>) {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = SwiftExtractor::new(
        "swift".to_string(),
        "test.swift".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);
    (symbols, identifiers)
}

#[test]
fn test_swift_type_usage_identifiers_cover_properties_params_returns_and_generics() {
    let code = r#"
class User {}
class LoginRequest {}
class AuthResult {}
class Repository<T> {}

class AuthController {
    private let repository: Repository<User>
    var cachedUsers: [User]
    var lookup: [String: AuthResult]

    func login(request: LoginRequest, fallback: User?) -> AuthResult {
        return AuthResult()
    }
}
"#;

    let (_symbols, identifiers) = extract_all(code);
    let type_names: Vec<&str> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .map(|id| id.name.as_str())
        .collect();

    for expected in ["Repository", "User", "AuthResult", "LoginRequest"] {
        assert!(
            type_names.contains(&expected),
            "missing Swift type usage {expected}; got {type_names:?}"
        );
    }

    assert!(
        !type_names.contains(&"String"),
        "Swift primitive/library noise should stay filtered: {type_names:?}"
    );
}
