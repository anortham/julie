//! TypeScript symbols module inline tests extracted from src/extractors/typescript/symbols.rs
//!
//! This module contains all test functions that were previously defined inline in the
//! TypeScript symbols extraction module. They test core functionality for symbol routing
//! and extraction including:
//! - Multi-symbol kind extraction (classes, functions, interfaces, etc.)
//! - Symbol type routing via visit_node
//! - Comprehensive symbol collection from mixed syntax

use crate::base::{SymbolKind, Visibility};
use crate::typescript::TypeScriptExtractor;
use std::path::PathBuf;

#[test]
fn test_visit_all_symbol_kinds() {
    let code = r#"
    class MyClass {
        prop: string;
        method() {}
    }

    function myFunc() {}
    const myVar = 42;
    interface MyInterface {}
    type MyType = string;
    enum MyEnum { A, B }
    import { foo } from './bar';
    export { myVar };
    namespace MyNamespace {}
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

    assert!(!symbols.is_empty(), "Should extract some symbols");
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "MyClass" && s.kind == SymbolKind::Class),
        "Should extract class"
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "myFunc" && s.kind == SymbolKind::Function),
        "Should extract function"
    );
    assert!(
        symbols.iter().any(|s| s.name == "myVar"),
        "Should extract variable"
    );
}

#[test]
fn test_enum_members_extracted() {
    let code = r#"
    enum Direction {
        Up,
        Down,
        Left,
        Right
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

    // Should have the enum itself
    let direction_enum = symbols.iter().find(|s| s.name == "Direction" && s.kind == SymbolKind::Enum);
    assert!(direction_enum.is_some(), "Should extract Direction enum");
    let enum_id = &direction_enum.unwrap().id;

    // Should have enum members
    let members: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::EnumMember && s.parent_id.as_ref() == Some(enum_id))
        .collect();
    assert!(
        members.len() >= 2,
        "Should extract at least 2 enum members, got {}",
        members.len()
    );
}

#[test]
fn test_class_signature_with_extends() {
    let code = r#"
    class Animal {
        name: string;
    }

    class Dog extends Animal {
        breed: string;
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

    let dog_class = symbols.iter().find(|s| s.name == "Dog" && s.kind == SymbolKind::Class);
    assert!(dog_class.is_some(), "Should extract Dog class");
    let dog = dog_class.unwrap();
    assert!(
        dog.signature.as_ref().map_or(false, |sig| sig.contains("extends Animal")),
        "Dog class signature should contain 'extends Animal', got: {:?}",
        dog.signature
    );
}

// ========================================================================
// Decorator extraction tests
// ========================================================================

#[test]
fn test_class_decorator_in_signature() {
    let code = r#"
@Component({
    selector: 'app-root'
})
class AppComponent {
    title: string = 'app';
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

    let class_sym = symbols
        .iter()
        .find(|s| s.name == "AppComponent")
        .expect("Should extract AppComponent class");
    let sig = class_sym.signature.as_deref().unwrap_or("");
    assert!(
        sig.contains("@Component"),
        "Class decorator should be in signature, got: {:?}",
        sig
    );
}

#[test]
fn test_multiple_class_decorators() {
    let code = r#"
@Injectable()
@Singleton
class UserService {
    getUser(): void {}
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

    let class_sym = symbols
        .iter()
        .find(|s| s.name == "UserService")
        .expect("Should extract UserService class");
    let sig = class_sym.signature.as_deref().unwrap_or("");
    assert!(
        sig.contains("@Injectable"),
        "First decorator should be in signature, got: {:?}",
        sig
    );
    assert!(
        sig.contains("@Singleton"),
        "Second decorator should be in signature, got: {:?}",
        sig
    );
}

#[test]
fn test_member_decorator_in_property_signature() {
    let code = r#"
class MyComponent {
    @Input() user: User;
    @Output() userChange = new EventEmitter<User>();
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

    let user_prop = symbols
        .iter()
        .find(|s| s.name == "user" && s.kind == SymbolKind::Property)
        .expect("Should extract user property");
    let sig = user_prop.signature.as_deref().unwrap_or("");
    assert!(
        sig.contains("@Input"),
        "Property decorator should be in signature, got: {:?}",
        sig
    );
}

#[test]
fn test_method_decorator_in_signature() {
    // NOTE: In tree-sitter TypeScript, method decorators are siblings of the method_definition
    // inside class_body, not children. The extractor must look at the preceding sibling.
    let code = r#"
class AppComponent {
    @HostListener('click')
    onClick(): void {}
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

    let method_sym = symbols
        .iter()
        .find(|s| s.name == "onClick" && s.kind == SymbolKind::Method)
        .expect("Should extract onClick method");
    let sig = method_sym.signature.as_deref().unwrap_or("");
    assert!(
        sig.contains("@HostListener"),
        "Method decorator should be in signature, got: {:?}",
        sig
    );
}

// ========================================================================
// Access modifier (visibility) extraction tests
// ========================================================================

#[test]
fn test_property_access_modifiers() {
    let code = r#"
class User {
    private name: string;
    protected age: number;
    public email: string;
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

    let name_prop = symbols
        .iter()
        .find(|s| s.name == "name" && s.kind == SymbolKind::Property)
        .expect("Should extract name property");
    assert_eq!(
        name_prop.visibility,
        Some(Visibility::Private),
        "name should be private"
    );

    let age_prop = symbols
        .iter()
        .find(|s| s.name == "age" && s.kind == SymbolKind::Property)
        .expect("Should extract age property");
    assert_eq!(
        age_prop.visibility,
        Some(Visibility::Protected),
        "age should be protected"
    );

    let email_prop = symbols
        .iter()
        .find(|s| s.name == "email" && s.kind == SymbolKind::Property)
        .expect("Should extract email property");
    assert_eq!(
        email_prop.visibility,
        Some(Visibility::Public),
        "email should be public"
    );
}

#[test]
fn test_method_access_modifiers() {
    let code = r#"
class UserService {
    private getName(): string { return this.name; }
    protected validate(): boolean { return true; }
    public getEmail(): string { return this.email; }
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

    let get_name = symbols
        .iter()
        .find(|s| s.name == "getName" && s.kind == SymbolKind::Method)
        .expect("Should extract getName method");
    assert_eq!(
        get_name.visibility,
        Some(Visibility::Private),
        "getName should be private"
    );

    let validate = symbols
        .iter()
        .find(|s| s.name == "validate" && s.kind == SymbolKind::Method)
        .expect("Should extract validate method");
    assert_eq!(
        validate.visibility,
        Some(Visibility::Protected),
        "validate should be protected"
    );

    let get_email = symbols
        .iter()
        .find(|s| s.name == "getEmail" && s.kind == SymbolKind::Method)
        .expect("Should extract getEmail method");
    assert_eq!(
        get_email.visibility,
        Some(Visibility::Public),
        "getEmail should be public"
    );
}

#[test]
fn test_readonly_modifier_in_signature() {
    let code = r#"
class Config {
    public readonly apiUrl: string;
    readonly timeout: number;
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

    let api_url = symbols
        .iter()
        .find(|s| s.name == "apiUrl" && s.kind == SymbolKind::Property)
        .expect("Should extract apiUrl property");
    let sig = api_url.signature.as_deref().unwrap_or("");
    assert!(
        sig.contains("readonly"),
        "apiUrl signature should contain 'readonly', got: {:?}",
        sig
    );
    assert_eq!(
        api_url.visibility,
        Some(Visibility::Public),
        "apiUrl should be public"
    );

    let timeout = symbols
        .iter()
        .find(|s| s.name == "timeout" && s.kind == SymbolKind::Property)
        .expect("Should extract timeout property");
    let sig = timeout.signature.as_deref().unwrap_or("");
    assert!(
        sig.contains("readonly"),
        "timeout signature should contain 'readonly', got: {:?}",
        sig
    );
}

#[test]
fn test_combined_decorators_and_access_modifiers() {
    // Full integration test combining decorators + access modifiers
    let code = r#"
@Injectable()
class UserService {
    @Inject(HttpClient) private http: HttpClient;

    @Log()
    public async fetchUser(id: string): Promise<User> {
        return this.http.get(`/users/${id}`);
    }
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

    // Class should have @Injectable in signature
    let class_sym = symbols
        .iter()
        .find(|s| s.name == "UserService")
        .expect("Should extract UserService class");
    let sig = class_sym.signature.as_deref().unwrap_or("");
    assert!(
        sig.contains("@Injectable"),
        "Class signature should contain @Injectable, got: {:?}",
        sig
    );

    // Property should have @Inject decorator and private visibility
    let http_prop = symbols
        .iter()
        .find(|s| s.name == "http" && s.kind == SymbolKind::Property)
        .expect("Should extract http property");
    assert_eq!(
        http_prop.visibility,
        Some(Visibility::Private),
        "http should be private"
    );
    let sig = http_prop.signature.as_deref().unwrap_or("");
    assert!(
        sig.contains("@Inject"),
        "http signature should contain @Inject, got: {:?}",
        sig
    );
}
