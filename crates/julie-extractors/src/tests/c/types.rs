// TDD RED: Test for C type extraction
// This test will fail until we implement infer_types() for CExtractor

use crate::c::CExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

#[test]
fn test_c_function_return_types() {
    let code = r#"
int get_count() {
    return 42;
}

char* get_name() {
    return "Alice";
}

struct User* get_user() {
    return NULL;
}

void process_data() {
    // no return
}
"#;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .expect("Failed to load C grammar");

    let tree = parser.parse(code, None).expect("Failed to parse code");
    let workspace_root = PathBuf::from("/test");

    let mut extractor = CExtractor::new(
        "c".to_string(),
        "test.c".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let types = extractor.infer_types(&symbols);

    // Should extract return types for functions
    assert!(!types.is_empty(), "Should extract at least one type");

    // Find function symbols
    let get_count_symbol = symbols.iter().find(|s| s.name == "get_count").expect("Should find get_count");
    let get_name_symbol = symbols.iter().find(|s| s.name == "get_name").expect("Should find get_name");
    let get_user_symbol = symbols.iter().find(|s| s.name == "get_user").expect("Should find get_user");
    let process_data_symbol = symbols.iter().find(|s| s.name == "process_data").expect("Should find process_data");

    // Verify types were extracted
    assert_eq!(types.get(&get_count_symbol.id), Some(&"int".to_string()));
    assert_eq!(types.get(&get_name_symbol.id), Some(&"char*".to_string()));
    assert_eq!(types.get(&get_user_symbol.id), Some(&"struct User*".to_string()));
    assert_eq!(types.get(&process_data_symbol.id), Some(&"void".to_string()));
}

#[test]
fn test_c_variable_types() {
    let code = r#"
int count = 42;
char* name = "Alice";
struct User user;
const char* const MESSAGE = "Hello";
"#;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .expect("Failed to load C grammar");

    let tree = parser.parse(code, None).expect("Failed to parse code");
    let workspace_root = PathBuf::from("/test");

    let mut extractor = CExtractor::new(
        "c".to_string(),
        "test.c".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let types = extractor.infer_types(&symbols);

    // Should extract types for variables (if they're captured as symbols)
    if let Some(count_symbol) = symbols.iter().find(|s| s.name == "count") {
        assert_eq!(types.get(&count_symbol.id), Some(&"int".to_string()));
    }

    if let Some(name_symbol) = symbols.iter().find(|s| s.name == "name") {
        assert_eq!(types.get(&name_symbol.id), Some(&"char*".to_string()));
    }
}
