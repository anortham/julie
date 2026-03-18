//! Kotlin identifier extraction tests — type_usage
//!
//! Validates that type annotations in Kotlin produce TypeUsage identifiers
//! for centrality scoring. Same bug pattern as TypeScript, Scala, GDScript, Zig.

use crate::base::IdentifierKind;
use crate::kotlin::KotlinExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
        .expect("Error loading Kotlin grammar");
    parser
}

fn extract_identifiers(code: &str) -> Vec<crate::base::Identifier> {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = KotlinExtractor::new(
        "kotlin".to_string(),
        "test.kt".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    extractor.extract_identifiers(&tree, &symbols)
}

#[test]
fn test_kotlin_type_usage_identifiers() {
    // Type annotations in Kotlin should produce TypeUsage identifiers.
    // These drive centrality scoring — without them, heavily-referenced types
    // like JsonAdapter get centrality 0.00 despite 99 references.
    let code = r#"
interface UserService {
    fun getUser(id: Long): User
}

class AuthController(service: UserService) {
    fun login(request: LoginRequest): AuthResult {
        val config: AppConfig = loadConfig()
    }
}

typealias Handler = Request
"#;

    let identifiers = extract_identifiers(code);
    let type_usages: Vec<_> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .collect();

    assert!(
        !type_usages.is_empty(),
        "Kotlin type annotations must produce TypeUsage identifiers for centrality scoring"
    );

    let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

    // Core type references that MUST be extracted
    assert!(
        type_names.contains(&"User"),
        "Return type 'User' must be extracted. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"UserService"),
        "Constructor param type 'UserService' must be extracted. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"LoginRequest"),
        "Method param type 'LoginRequest' must be extracted. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"AuthResult"),
        "Return type 'AuthResult' must be extracted. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"AppConfig"),
        "Val type annotation 'AppConfig' must be extracted. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"Request"),
        "Type alias target 'Request' must be extracted. Got: {:?}",
        type_names
    );

    // Declaration names must NOT appear as TypeUsage
    assert!(
        !type_names.contains(&"AuthController"),
        "Class declaration name 'AuthController' must NOT be TypeUsage. Got: {:?}",
        type_names
    );

    // Kotlin builtins should be filtered
    assert!(
        !type_names.contains(&"Long"),
        "Builtin 'Long' must NOT be a TypeUsage identifier. Got: {:?}",
        type_names
    );
}

#[test]
fn test_kotlin_type_usage_skips_noise_types() {
    // Kotlin primitive/wrapper types and single-letter generics should NOT
    // produce TypeUsage identifiers — they pollute centrality with noise.
    let code = r#"
fun greet(name: String, age: Int): Boolean {
    return true
}
val x: Any = null
val items: List<T> = emptyList()
"#;

    let identifiers = extract_identifiers(code);
    let type_usages: Vec<_> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .collect();
    let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

    assert!(
        !type_names.contains(&"String"),
        "Builtin 'String' must NOT be a TypeUsage identifier"
    );
    assert!(
        !type_names.contains(&"Int"),
        "Builtin 'Int' must NOT be a TypeUsage identifier"
    );
    assert!(
        !type_names.contains(&"Boolean"),
        "Builtin 'Boolean' must NOT be a TypeUsage identifier"
    );
    assert!(
        !type_names.contains(&"Any"),
        "Builtin 'Any' must NOT be a TypeUsage identifier"
    );
    // Single-letter generic
    assert!(
        !type_names.contains(&"T"),
        "Single-letter generic 'T' must NOT be a TypeUsage identifier"
    );

    // But List should be extracted (it's a real type, not a primitive)
    assert!(
        type_names.contains(&"List"),
        "Non-primitive type 'List' should be extracted as TypeUsage. Got: {:?}",
        type_names
    );
}
