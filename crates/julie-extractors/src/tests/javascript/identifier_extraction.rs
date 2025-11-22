//! Identifier Extraction Tests for JavaScript
//!
//! Tests for extracting identifiers (function calls, member access, chained access)
//! from JavaScript code. Validates that identifier extraction correctly:
//! - Finds function calls
//! - Finds member access patterns
//! - Handles chained member access
//! - Tracks containing symbols
//! - Avoids duplicate identifiers at same location

use crate::base::IdentifierKind;
use crate::javascript::JavaScriptExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("Error loading JavaScript grammar");
    parser
}

#[test]
fn test_extract_function_calls() {
    let js_code = r#"
function add(a, b) {
    return a + b;
}

function calculate() {
    const result = add(5, 3);      // Function call to add
    console.log(result);            // Function call to log
    return result;
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(js_code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        js_code.to_string(),
        &workspace_root,
    );

    // Extract symbols first
    let symbols = extractor.extract_symbols(&tree);

    // NOW extract identifiers (this will FAIL until we implement it)
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    // Verify we found the function calls
    let add_call = identifiers.iter().find(|id| id.name == "add");
    assert!(
        add_call.is_some(),
        "Should extract 'add' function call identifier"
    );
    let add_call = add_call.unwrap();
    assert_eq!(add_call.kind, IdentifierKind::Call);

    let log_call = identifiers.iter().find(|id| id.name == "log");
    assert!(
        log_call.is_some(),
        "Should extract 'log' function call identifier"
    );
    let log_call = log_call.unwrap();
    assert_eq!(log_call.kind, IdentifierKind::Call);

    // Verify containing symbol is set correctly (should be inside calculate function)
    assert!(
        add_call.containing_symbol_id.is_some(),
        "Function call should have containing symbol"
    );

    // Find the calculate function symbol
    let calculate_fn = symbols.iter().find(|s| s.name == "calculate").unwrap();

    // Verify the add call is contained within calculate function
    assert_eq!(
        add_call.containing_symbol_id.as_ref(),
        Some(&calculate_fn.id),
        "add call should be contained within calculate function"
    );
}

#[test]
fn test_extract_member_access() {
    let js_code = r#"
class User {
    constructor(name, email) {
        this.name = name;
        this.email = email;
    }

    printInfo() {
        console.log(this.name);   // Member access: this.name
        const email = this.email;  // Member access: this.email
    }
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(js_code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        js_code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    // Verify we found member access identifiers
    let name_access = identifiers
        .iter()
        .filter(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess)
        .count();
    assert!(
        name_access > 0,
        "Should extract 'name' member access identifier"
    );

    let email_access = identifiers
        .iter()
        .filter(|id| id.name == "email" && id.kind == IdentifierKind::MemberAccess)
        .count();
    assert!(
        email_access > 0,
        "Should extract 'email' member access identifier"
    );
}

#[test]
fn test_file_scoped_containing_symbol() {
    // This test ensures we ONLY match symbols from the SAME FILE
    // Critical bug fix from Rust implementation
    let js_code = r#"
function process() {
    helper();              // Call to helper in same file
}

function helper() {
    // Helper function
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(js_code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        js_code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    // Find the helper call
    let helper_call = identifiers.iter().find(|id| id.name == "helper");
    assert!(helper_call.is_some());
    let helper_call = helper_call.unwrap();

    // Verify it has a containing symbol (the process function)
    assert!(
        helper_call.containing_symbol_id.is_some(),
        "helper call should have containing symbol from same file"
    );

    // Verify the containing symbol is the process function
    let process_fn = symbols.iter().find(|s| s.name == "process").unwrap();
    assert_eq!(
        helper_call.containing_symbol_id.as_ref(),
        Some(&process_fn.id),
        "helper call should be contained within process function"
    );
}

#[test]
fn test_chained_member_access() {
    let js_code = r#"
class DataService {
    execute() {
        const result = user.account.balance;   // Chained member access
        const name = customer.profile.name;     // Chained member access
    }
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(js_code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        js_code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    // Should extract the rightmost identifiers in chains
    let balance_access = identifiers
        .iter()
        .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
    assert!(
        balance_access.is_some(),
        "Should extract 'balance' from chained member access"
    );

    let name_access = identifiers
        .iter()
        .find(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess);
    assert!(
        name_access.is_some(),
        "Should extract 'name' from chained member access"
    );
}

#[test]
fn test_no_duplicate_identifiers() {
    let js_code = r#"
function run() {
    process();
    process();  // Same call twice
}

function process() {
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(js_code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        js_code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    // Should extract BOTH calls (they're at different locations)
    let process_calls: Vec<_> = identifiers
        .iter()
        .filter(|id| id.name == "process" && id.kind == IdentifierKind::Call)
        .collect();

    assert_eq!(
        process_calls.len(),
        2,
        "Should extract both process calls at different locations"
    );

    // Verify they have different line numbers
    assert_ne!(
        process_calls[0].start_line, process_calls[1].start_line,
        "Duplicate calls should have different line numbers"
    );
}
