// Dart identifier extraction tests — type_usage identifiers for type annotations
//
// Dart uses `type_identifier` tree-sitter nodes for type annotations in
// variable declarations, parameter types, return types, generic type arguments,
// implements, extends, with clauses. These must produce TypeUsage identifiers
// for centrality scoring.

use crate::base::IdentifierKind;
use crate::dart::DartExtractor;
use std::path::PathBuf;

fn init_parser() -> tree_sitter::Parser {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&harper_tree_sitter_dart::LANGUAGE.into())
        .expect("Error loading Dart grammar");
    parser
}

#[test]
fn test_dart_type_usage_identifiers() {
    // Dart type annotations should produce TypeUsage identifiers.
    // These drive centrality scoring for classes, mixins, and typedefs.
    let code = r#"
class UserService {
  User getUser(String name) {
    return User(name);
  }
}

class AuthController extends BaseController {
  late final AuthService service;
  final ProviderContainer container;

  AuthController(this.service, this.container);

  Future<AuthResult> login(LoginRequest request) {
    return service.authenticate(request);
  }
}

mixin LoggerMixin on BaseLogger {
  void log(LogEntry entry);
}

typedef Callback = void Function(Event event);
"#;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = DartExtractor::new(
        "dart".to_string(),
        "test.dart".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    let type_usages: Vec<_> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .collect();

    assert!(
        !type_usages.is_empty(),
        "Dart type annotations must produce TypeUsage identifiers for centrality scoring"
    );

    let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

    // Return type
    assert!(
        type_names.contains(&"User"),
        "Return type 'User' must be extracted. Got: {:?}",
        type_names
    );

    // Field types
    assert!(
        type_names.contains(&"AuthService"),
        "Field type 'AuthService' must be extracted. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"ProviderContainer"),
        "Field type 'ProviderContainer' must be extracted. Got: {:?}",
        type_names
    );

    // Superclass
    assert!(
        type_names.contains(&"BaseController"),
        "Superclass 'BaseController' must be extracted. Got: {:?}",
        type_names
    );

    // Parameter types
    assert!(
        type_names.contains(&"LoginRequest"),
        "Parameter type 'LoginRequest' must be extracted. Got: {:?}",
        type_names
    );

    // Generic type arguments
    assert!(
        type_names.contains(&"AuthResult"),
        "Generic arg 'AuthResult' must be extracted. Got: {:?}",
        type_names
    );

    // Mixin constraint type
    assert!(
        type_names.contains(&"BaseLogger"),
        "Mixin 'on' constraint 'BaseLogger' must be extracted. Got: {:?}",
        type_names
    );

    // Typedef parameter type
    assert!(
        type_names.contains(&"Event"),
        "Typedef param type 'Event' must be extracted. Got: {:?}",
        type_names
    );

    // LogEntry parameter type
    assert!(
        type_names.contains(&"LogEntry"),
        "Parameter type 'LogEntry' must be extracted. Got: {:?}",
        type_names
    );

    // Should NOT contain declaration names
    assert!(
        !type_names.contains(&"UserService"),
        "Class declaration name 'UserService' must NOT be type_usage. Got: {:?}",
        type_names
    );
    assert!(
        !type_names.contains(&"AuthController"),
        "Class declaration name 'AuthController' must NOT be type_usage. Got: {:?}",
        type_names
    );
    assert!(
        !type_names.contains(&"LoggerMixin"),
        "Mixin declaration name 'LoggerMixin' must NOT be type_usage. Got: {:?}",
        type_names
    );
    assert!(
        !type_names.contains(&"Callback"),
        "Typedef declaration name 'Callback' must NOT be type_usage. Got: {:?}",
        type_names
    );

    // Should NOT contain single-letter generics
    // (not in this test, but verified by the skip logic)
}

#[test]
fn test_dart_type_usage_skips_single_letter_generics() {
    let code = r#"
class Container<T> {
  T value;

  Container(this.value);

  R transform<R>(R Function(T) mapper) {
    return mapper(value);
  }
}
"#;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = DartExtractor::new(
        "dart".to_string(),
        "test.dart".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    let type_usages: Vec<_> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .collect();
    let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

    // Single-letter generics T, R should be filtered
    assert!(
        !type_names.contains(&"T"),
        "Single-letter generic 'T' must be filtered. Got: {:?}",
        type_names
    );
    assert!(
        !type_names.contains(&"R"),
        "Single-letter generic 'R' must be filtered. Got: {:?}",
        type_names
    );

    // Container should NOT appear (it's the declaration name)
    assert!(
        !type_names.contains(&"Container"),
        "Class declaration name 'Container' must NOT be type_usage. Got: {:?}",
        type_names
    );
}
