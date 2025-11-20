// TDD RED: Test for JavaScript type extraction via JSDoc
// This test will fail until we implement infer_types() for JavaScriptExtractor

use crate::extractors::javascript::JavaScriptExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

#[test]
fn test_javascript_jsdoc_return_types() {
    let code = r#"
/**
 * Get user name
 * @returns {string}
 */
function getName() {
    return "Alice";
}

/**
 * Calculate age
 * @returns {number}
 */
function calculateAge() {
    return 42;
}

/**
 * Process data
 * @returns {Promise<Array<number>>}
 */
async function processData() {
    return [1, 2, 3];
}
"#;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("Failed to load JavaScript grammar");

    let tree = parser.parse(code, None).expect("Failed to parse code");
    let workspace_root = PathBuf::from("/test");

    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let types = extractor.infer_types(&symbols);

    // Should extract return types from JSDoc for all 3 functions
    assert!(
        !types.is_empty(),
        "Should extract at least one type from JSDoc"
    );

    // Find function symbols
    let get_name_symbol = symbols
        .iter()
        .find(|s| s.name == "getName")
        .expect("Should find getName");
    let calculate_age_symbol = symbols
        .iter()
        .find(|s| s.name == "calculateAge")
        .expect("Should find calculateAge");
    let process_data_symbol = symbols
        .iter()
        .find(|s| s.name == "processData")
        .expect("Should find processData");

    // Verify types were extracted from JSDoc
    assert_eq!(types.get(&get_name_symbol.id), Some(&"string".to_string()));
    assert_eq!(
        types.get(&calculate_age_symbol.id),
        Some(&"number".to_string())
    );
    assert_eq!(
        types.get(&process_data_symbol.id),
        Some(&"Promise<Array<number>>".to_string())
    );
}

#[test]
fn test_javascript_jsdoc_variable_types() {
    let code = r#"
/**
 * @type {string}
 */
const userName = "Alice";

/**
 * @type {number}
 */
let userAge = 42;

/**
 * @type {Array<User>}
 */
var users = [];
"#;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("Failed to load JavaScript grammar");

    let tree = parser.parse(code, None).expect("Failed to parse code");
    let workspace_root = PathBuf::from("/test");

    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let types = extractor.infer_types(&symbols);

    // Should extract types from JSDoc for variables (if they're captured as symbols)
    if let Some(user_name_symbol) = symbols.iter().find(|s| s.name == "userName") {
        assert_eq!(types.get(&user_name_symbol.id), Some(&"string".to_string()));
    }

    if let Some(user_age_symbol) = symbols.iter().find(|s| s.name == "userAge") {
        assert_eq!(types.get(&user_age_symbol.id), Some(&"number".to_string()));
    }

    if let Some(users_symbol) = symbols.iter().find(|s| s.name == "users") {
        assert_eq!(
            types.get(&users_symbol.id),
            Some(&"Array<User>".to_string())
        );
    }
}
