use crate::extractors::base::SymbolKind;
use crate::extractors::zig::ZigExtractor;

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

    let mut extractor = ZigExtractor::new("Zig".to_string(), "test.zig".to_string(), code.to_string());
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

    let mut extractor = ZigExtractor::new("Zig".to_string(), "test.zig".to_string(), code.to_string());
    let symbols = extractor.extract_symbols(&tree);

    // Should extract struct (as Class)
    assert!(symbols.iter().any(|s| s.kind == SymbolKind::Class));
}
