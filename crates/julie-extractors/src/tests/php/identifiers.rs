//! Identifier Extraction Tests for PHP
//!
//! Tests for extracting identifiers (function calls, member access, chained access)
//! from PHP code. Validates that identifier extraction correctly:
//! - Finds function calls
//! - Finds member access patterns
//! - Handles chained member access
//! - Tracks containing symbols
//! - Avoids duplicate identifiers at same location

use crate::base::IdentifierKind;
use crate::php::PhpExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .expect("Error loading PHP grammar");
    parser
}

#[test]
fn test_extract_function_calls() {
    let php_code = r#"<?php

class Calculator {
    public function add(int $a, int $b): int {
        return $a + $b;
    }

    public function calculate(): int {
        $result = $this->add(5, 3);      // Function call to add
        echo print_r($result, true);     // Function call to print_r
        return $result;
    }
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(php_code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = PhpExtractor::new(
        "php".to_string(),
        "test.php".to_string(),
        php_code.to_string(),
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

    let print_r_call = identifiers.iter().find(|id| id.name == "print_r");
    assert!(
        print_r_call.is_some(),
        "Should extract 'print_r' function call identifier"
    );
    let print_r_call = print_r_call.unwrap();
    assert_eq!(print_r_call.kind, IdentifierKind::Call);

    // Verify containing symbol is set correctly (should be inside calculate method)
    assert!(
        add_call.containing_symbol_id.is_some(),
        "Function call should have containing symbol"
    );

    // Find the calculate method symbol
    let calculate_method = symbols.iter().find(|s| s.name == "calculate").unwrap();

    // Verify the add call is contained within calculate method
    assert_eq!(
        add_call.containing_symbol_id.as_ref(),
        Some(&calculate_method.id),
        "add call should be contained within calculate method"
    );
}

#[test]
fn test_extract_member_access() {
    let php_code = r#"<?php

class User {
    public string $name;
    public string $email;

    public function printInfo(): void {
        echo $this->name;         // Member access: $this->name
        $email = $this->email;    // Member access: $this->email
    }
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(php_code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = PhpExtractor::new(
        "php".to_string(),
        "test.php".to_string(),
        php_code.to_string(),
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
    // Critical bug fix from Rust implementation (line 1311-1318 in rust.rs)
    let php_code = r#"<?php

class Service {
    public function process(): void {
        $this->helper();              // Call to helper in same file
    }

    private function helper(): void {
        // Helper method
    }
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(php_code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = PhpExtractor::new(
        "php".to_string(),
        "test.php".to_string(),
        php_code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    // Find the helper call
    let helper_call = identifiers.iter().find(|id| id.name == "helper");
    assert!(helper_call.is_some());
    let helper_call = helper_call.unwrap();

    // Verify it has a containing symbol (the process method)
    assert!(
        helper_call.containing_symbol_id.is_some(),
        "helper call should have containing symbol from same file"
    );

    // Verify the containing symbol is the process method
    let process_method = symbols.iter().find(|s| s.name == "process").unwrap();
    assert_eq!(
        helper_call.containing_symbol_id.as_ref(),
        Some(&process_method.id),
        "helper call should be contained within process method"
    );
}

#[test]
fn test_chained_member_access() {
    let php_code = r#"<?php

class DataService {
    public function execute(): void {
        $result = $user->account->balance;      // Chained member access
        $name = $customer->profile->name;       // Chained member access
    }
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(php_code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = PhpExtractor::new(
        "php".to_string(),
        "test.php".to_string(),
        php_code.to_string(),
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
    let php_code = r#"<?php

class Test {
    public function run(): void {
        $this->process();
        $this->process();  // Same call twice
    }

    private function process(): void {
    }
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(php_code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = PhpExtractor::new(
        "php".to_string(),
        "test.php".to_string(),
        php_code.to_string(),
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
