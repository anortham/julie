//! Inline tests extracted from src/tools/ast_symbol_finder.rs
//!
//! These tests were originally embedded in the AST symbol finder module.
//! They have been extracted to maintain separation of concerns while ensuring
//! the core logic remains thoroughly tested.

use crate::tools::ast_symbol_finder::{ASTSymbolFinder, SymbolContext};
use tree_sitter::Parser;

#[test]
fn test_find_symbol_occurrences_typescript() {
    let code = r#"
class UserService {
    getUserData() {
        return "UserService"; // String literal - should be StringLiteral context!
    }
}

// UserService comment - should be Comment context!
const service = new UserService();
"#;

    let mut parser = Parser::new();
    let tsx_lang = crate::language::get_tree_sitter_language("tsx").unwrap();
    parser.set_language(&tsx_lang).unwrap();
    let tree = parser.parse(code, None).unwrap();

    let finder = ASTSymbolFinder::new(code.to_string(), tree, "typescript".to_string());
    let occurrences = finder.find_symbol_occurrences("UserService");

    // Should find: class definition, type usage, but NOT string literal or comment
    assert!(
        occurrences.len() >= 2,
        "Should find at least 2 occurrences (definition + usage)"
    );

    // Check that string literal is marked correctly
    let string_contexts: Vec<_> = occurrences
        .iter()
        .filter(|occ| occ.context == SymbolContext::StringLiteral)
        .collect();

    assert_eq!(
        string_contexts.len(),
        0,
        "String literals should not be found as symbol occurrences (they're string nodes, not identifiers)"
    );

    // Check that we found the class definition
    let definitions: Vec<_> = occurrences
        .iter()
        .filter(|occ| occ.context == SymbolContext::Definition)
        .collect();

    assert!(!definitions.is_empty(), "Should find the class definition");
}
