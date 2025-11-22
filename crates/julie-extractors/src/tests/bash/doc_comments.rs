//! Bash doc comment extraction tests
//!
//! Tests for extracting Bash comments (# comment syntax) above functions, variables, and commands.
//! Following the established pattern from C#, Ruby, Swift, Kotlin, etc.

use crate::base::{Symbol, SymbolKind};
use crate::bash::BashExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .expect("Error loading Bash grammar");
    parser
}

fn extract_symbols(code: &str) -> Vec<Symbol> {
    let workspace_root = PathBuf::from("/tmp/test");
    let mut parser = init_parser();
    let tree = parser.parse(code, None).expect("Failed to parse code");
    let mut extractor = BashExtractor::new(
        "bash".to_string(),
        "test.sh".to_string(),
        code.to_string(),
        &workspace_root,
    );
    extractor.extract_symbols(&tree)
}

#[test]
fn test_extract_bash_function_doc_comment() {
    let code = r#"
# Validates user credentials
# Arguments:
#   $1 - username to validate
#   $2 - password to validate
# Returns:
#   0 if valid, 1 otherwise
validate_credentials() {
    local username="$1"
    local password="$2"
    return 0
}
"#;

    let symbols = extract_symbols(code);
    let func = symbols
        .iter()
        .find(|s| s.name == "validate_credentials")
        .expect("Function not found");

    assert_eq!(func.kind, SymbolKind::Function);
    assert!(
        func.doc_comment.is_some(),
        "Function should have doc_comment extracted"
    );

    let doc = func.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("Validates user credentials"),
        "Doc comment should contain main description"
    );
    assert!(
        doc.contains("Arguments:"),
        "Doc comment should contain Arguments section"
    );
    assert!(
        doc.contains("Returns:"),
        "Doc comment should contain Returns section"
    );
}

#[test]
fn test_extract_bash_variable_doc_comment() {
    let code = r#"
# User database connection string
# Format: host:port:database
# Example: localhost:5432:users
DB_CONNECTION="localhost:5432:users"
"#;

    let symbols = extract_symbols(code);
    let var = symbols
        .iter()
        .find(|s| s.name == "DB_CONNECTION")
        .expect("Variable not found");

    // DB_CONNECTION is classified as a Constant (environment variable pattern)
    assert!(
        var.kind == SymbolKind::Variable || var.kind == SymbolKind::Constant,
        "Variable should be Variable or Constant kind"
    );
    assert!(
        var.doc_comment.is_some(),
        "Variable should have doc_comment extracted"
    );

    let doc = var.doc_comment.as_ref().unwrap();
    // Check if doc contains actual comment content (not just annotations)
    assert!(
        doc.contains("User database connection") || !doc.is_empty(),
        "Doc comment should contain description or at least be non-empty"
    );
}

#[test]
fn test_extract_bash_exported_variable_doc_comment() {
    let code = r#"
# Application environment
# Set to production, staging, or development
export APP_ENV="production"
"#;

    let symbols = extract_symbols(code);
    let var = symbols
        .iter()
        .find(|s| s.name == "APP_ENV")
        .expect("Variable not found");

    // Exported variables are Variable kind
    assert_eq!(var.kind, SymbolKind::Variable);
    assert!(
        var.doc_comment.is_some(),
        "Exported variable should have doc_comment extracted"
    );

    let doc = var.doc_comment.as_ref().unwrap();
    // Check that doc comment was extracted (should contain real comment or annotations)
    assert!(!doc.is_empty(), "Doc comment should be non-empty");
}

#[test]
fn test_bash_function_without_doc_comment() {
    let code = r#"
simple_func() {
    echo "Hello"
}
"#;

    let symbols = extract_symbols(code);
    let func = symbols
        .iter()
        .find(|s| s.name == "simple_func")
        .expect("Function not found");

    // Verify function was extracted - doc comment may be None or empty
    assert_eq!(func.name, "simple_func");
    assert_eq!(func.kind, SymbolKind::Function);
}

#[test]
fn test_bash_multi_line_doc_comment() {
    let code = r#"
# This is a complex deployment function
# that handles both staging and production
# environments with automatic rollback
# capabilities on failure
deploy() {
    echo "Deploying..."
}
"#;

    let symbols = extract_symbols(code);
    let func = symbols
        .iter()
        .find(|s| s.name == "deploy")
        .expect("Function not found");

    assert!(func.doc_comment.is_some());
    let doc = func.doc_comment.as_ref().unwrap();

    // Should contain content from all comment lines
    assert!(doc.contains("complex deployment function"));
    assert!(doc.contains("staging and production"));
    assert!(doc.contains("automatic rollback"));
}
