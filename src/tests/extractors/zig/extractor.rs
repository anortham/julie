use crate::extractors::base::SymbolKind;
use crate::extractors::zig::ZigExtractor;
use std::path::PathBuf;

#[test]
fn test_zig_basic_extraction() {
    let code = r#"
pub fn main() void {
    var x: i32 = 5;
}
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_zig::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ZigExtractor::new(
        "Zig".to_string(),
        "test.zig".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    assert!(!symbols.is_empty());
}

#[test]
fn test_zig_struct_extraction() {
    let code = r#"
pub const Point = struct {
    x: i32,
    y: i32,
};
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_zig::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ZigExtractor::new(
        "Zig".to_string(),
        "test.zig".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    // Should extract struct (as Class)
    assert!(symbols.iter().any(|s| s.kind == SymbolKind::Class));
}

#[test]
fn test_zig_doc_comment_on_function() {
    let code = r#"
    /// Validates user credentials against database
    /// Returns true if credentials match
    pub fn validateCredentials(username: []const u8) bool {
        return true;
    }
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_zig::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ZigExtractor::new(
        "Zig".to_string(),
        "test.zig".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    // Find the function
    let func_symbol = symbols
        .iter()
        .find(|s| s.name == "validateCredentials")
        .expect("Should find validateCredentials function");

    // Should have doc comment extracted
    assert!(
        func_symbol.doc_comment.is_some(),
        "Function should have doc_comment populated"
    );

    let doc = func_symbol.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("Validates user credentials"),
        "Doc comment should contain description text, got: {}",
        doc
    );
    assert!(
        doc.contains("Returns true if credentials match"),
        "Doc comment should contain return information, got: {}",
        doc
    );
}

#[test]
fn test_zig_doc_comment_on_struct() {
    let code = r#"
    /// User account information
    /// Stores authentication and profile data
    pub const User = struct {
        id: u32,
        name: []const u8,
    };
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_zig::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ZigExtractor::new(
        "Zig".to_string(),
        "test.zig".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    // Find the struct symbol
    let struct_symbol = symbols
        .iter()
        .find(|s| s.name == "User")
        .expect("Should find User struct");

    // Should have doc comment extracted
    assert!(
        struct_symbol.doc_comment.is_some(),
        "Struct should have doc_comment populated"
    );

    let doc = struct_symbol.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("User account information"),
        "Doc comment should contain description, got: {}",
        doc
    );
    assert!(
        doc.contains("authentication and profile data"),
        "Doc comment should contain details, got: {}",
        doc
    );
}

#[test]
fn test_zig_doc_comment_on_enum() {
    let code = r#"
    /// User role enumeration
    /// Defines different permission levels
    pub const UserRole = enum {
        admin,
        user,
        guest,
    };
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_zig::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ZigExtractor::new(
        "Zig".to_string(),
        "test.zig".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    // Find the enum symbol
    let enum_symbol = symbols
        .iter()
        .find(|s| s.name == "UserRole")
        .expect("Should find UserRole enum");

    // Should have doc comment extracted
    assert!(
        enum_symbol.doc_comment.is_some(),
        "Enum should have doc_comment populated"
    );

    let doc = enum_symbol.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("User role enumeration"),
        "Doc comment should contain summary, got: {}",
        doc
    );
    assert!(
        doc.contains("permission levels"),
        "Doc comment should contain details, got: {}",
        doc
    );
}

#[test]
fn test_zig_doc_comment_on_const() {
    let code = r#"
    /// Default timeout duration in milliseconds
    /// Used for all network operations
    pub const DEFAULT_TIMEOUT: u64 = 5000;
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_zig::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ZigExtractor::new(
        "Zig".to_string(),
        "test.zig".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    // Find the constant symbol
    let const_symbol = symbols
        .iter()
        .find(|s| s.name == "DEFAULT_TIMEOUT")
        .expect("Should find DEFAULT_TIMEOUT constant");

    // Should have doc comment extracted
    assert!(
        const_symbol.doc_comment.is_some(),
        "Constant should have doc_comment populated"
    );

    let doc = const_symbol.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("Default timeout duration"),
        "Doc comment should contain description, got: {}",
        doc
    );
    assert!(
        doc.contains("network operations"),
        "Doc comment should contain usage context, got: {}",
        doc
    );
}

#[test]
fn test_zig_no_doc_comment_when_missing() {
    let code = r#"
pub fn noComment() void {
    return;
}
"#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_zig::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = ZigExtractor::new(
        "Zig".to_string(),
        "test.zig".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    // Find the function
    let func_symbol = symbols
        .iter()
        .find(|s| s.name == "noComment")
        .expect("Should find noComment function");

    // Should NOT have doc comment
    assert!(
        func_symbol.doc_comment.is_none(),
        "Function without doc comment should have None"
    );
}
