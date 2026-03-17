//! Tests extracted from src/extractors/typescript/identifiers.rs
//!
//! This module contains inline tests that were previously embedded in the identifiers.rs module.
//! They test the identifier extraction functionality for TypeScript/JavaScript code, including
//! function calls, member access, and chained member access patterns.

use crate::base::IdentifierKind;
use crate::typescript::TypeScriptExtractor;
use std::path::PathBuf;

#[test]
fn test_extract_function_calls() {
    let code = r#"
    function foo() {}
    function bar() {
        foo();
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
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    assert!(!identifiers.is_empty());
    assert!(
        identifiers
            .iter()
            .any(|id| id.name == "foo" && id.kind == IdentifierKind::Call)
    );
}

#[test]
fn test_extract_member_access() {
    let code = r#"
    const obj = { prop: 42 };
    console.log(obj.prop);
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
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    assert!(!identifiers.is_empty());
    assert!(
        identifiers
            .iter()
            .any(|id| id.kind == IdentifierKind::MemberAccess)
    );
}

#[test]
fn test_extract_chained_member_access() {
    let code = "const value = obj.foo.bar.baz;";
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
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    assert!(!identifiers.is_empty());
}

#[test]
fn test_extract_type_usage_identifiers() {
    // TypeScript type annotations should produce TypeUsage identifiers.
    // These drive centrality scoring for interfaces, classes, and types.
    let code = r#"
interface UserService {
    getUser(id: string): User;
}

class AuthController {
    private service: UserService;

    constructor(svc: UserService) {
        this.service = svc;
    }

    async login(request: LoginRequest): Promise<AuthResult> {
        return this.service.getUser(request.userId);
    }
}

const config: AppConfig = loadConfig();
type Handler = (req: Request, res: Response) => void;
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
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    let type_usages: Vec<_> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .collect();

    // Should find type_usage for: User (return type), UserService (field type + param type),
    // LoginRequest (param type), AuthResult (generic arg), AppConfig (variable type),
    // Request, Response (type alias refs), Promise (return type)
    assert!(
        !type_usages.is_empty(),
        "TypeScript type annotations must produce TypeUsage identifiers for centrality scoring"
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
        "Field/param type 'UserService' must be extracted. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"LoginRequest"),
        "Parameter type 'LoginRequest' must be extracted. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"AppConfig"),
        "Variable type annotation 'AppConfig' must be extracted. Got: {:?}",
        type_names
    );
}

#[test]
fn test_type_usage_skips_builtin_types() {
    // Builtin types (string, number, boolean, void, any, etc.) should NOT produce
    // TypeUsage identifiers — they pollute centrality with noise.
    let code = r#"
function greet(name: string, age: number): boolean {
    return true;
}
const x: any = null;
const y: void = undefined;
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
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    let type_usages: Vec<_> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .collect();

    // Builtin types should be filtered out
    let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();
    assert!(
        !type_names.contains(&"string"),
        "Builtin 'string' should NOT be a TypeUsage identifier"
    );
    assert!(
        !type_names.contains(&"number"),
        "Builtin 'number' should NOT be a TypeUsage identifier"
    );
    assert!(
        !type_names.contains(&"boolean"),
        "Builtin 'boolean' should NOT be a TypeUsage identifier"
    );
    assert!(
        !type_names.contains(&"void"),
        "Builtin 'void' should NOT be a TypeUsage identifier"
    );
    assert!(
        !type_names.contains(&"any"),
        "Builtin 'any' should NOT be a TypeUsage identifier"
    );
}

#[test]
fn test_type_usage_excludes_declaration_names() {
    // Declaration names (interface Foo, type Bar, class Baz) define types —
    // they should NOT produce TypeUsage identifiers. Only references to types
    // should count. Without this filter, every declaration inflates its own
    // centrality by 1 (self-reference).
    let code = r#"
interface UserService {
    getUser(): User;
}

type ApiResult = UserService | Error;

class AuthController implements UserService {
    getUser(): User { return {} as User; }
}

abstract class BaseService {
    abstract init(): void;
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
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    let type_usages: Vec<_> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .collect();
    let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

    // Declaration names must NOT appear as TypeUsage
    // "UserService" appears as: interface declaration (NO), type alias reference (YES),
    // implements clause (YES) → should appear exactly for the reference contexts
    assert!(
        !type_names.contains(&"ApiResult"),
        "Type alias declaration name 'ApiResult' must NOT be a TypeUsage. Got: {:?}",
        type_names
    );
    assert!(
        !type_names.contains(&"AuthController"),
        "Class declaration name 'AuthController' must NOT be a TypeUsage. Got: {:?}",
        type_names
    );
    assert!(
        !type_names.contains(&"BaseService"),
        "Abstract class declaration name 'BaseService' must NOT be a TypeUsage. Got: {:?}",
        type_names
    );

    // But references TO those types should still be TypeUsage
    assert!(
        type_names.contains(&"User"),
        "Reference to 'User' (return type) must be TypeUsage. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"UserService"),
        "Reference to 'UserService' (in type alias + implements) must be TypeUsage. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"Error"),
        "Reference to 'Error' (in union type) must be TypeUsage. Got: {:?}",
        type_names
    );
}

#[test]
fn test_type_usage_excludes_generic_type_parameters() {
    // Generic type parameters (T, K, V) are declarations, not references.
    // They also appear in reference positions within the generic scope, but
    // single-letter type params are noise for centrality purposes.
    let code = r#"
function identity<T>(value: T): T {
    return value;
}

interface Container<T, K extends string> {
    get(key: K): T;
    items: Map<K, T>;
}

type Mapper<Input, Output> = (val: Input) => Output;
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
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    let type_usages: Vec<_> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .collect();
    let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

    // Single-letter generic params must be filtered (noise)
    assert!(
        !type_names.contains(&"T"),
        "Single-letter generic param 'T' must NOT be a TypeUsage. Got: {:?}",
        type_names
    );
    assert!(
        !type_names.contains(&"K"),
        "Single-letter generic param 'K' must NOT be a TypeUsage. Got: {:?}",
        type_names
    );

    // Declaration names must NOT appear (filtered by parent-context check)
    assert!(
        !type_names.contains(&"Container"),
        "Interface declaration name 'Container' must NOT be a TypeUsage. Got: {:?}",
        type_names
    );
    assert!(
        !type_names.contains(&"Mapper"),
        "Type alias declaration name 'Mapper' must NOT be a TypeUsage. Got: {:?}",
        type_names
    );

    // Multi-letter generic param REFERENCES within scope (e.g. `val: Input`)
    // are acceptable — they're legitimate type_identifier references in the AST.
    // Filtering them would require scope analysis. They cause minimal centrality
    // noise since they rarely collide with real type names across files.
}
