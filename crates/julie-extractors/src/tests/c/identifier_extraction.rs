// C Identifier Extraction Tests (TDD RED phase)
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Function calls (call_expression)
// - Member access (field_expression)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

use crate::base::IdentifierKind;
use crate::c::CExtractor;
use crate::tests::c::init_parser;
use std::path::PathBuf;

#[cfg(test)]
mod identifier_extraction_tests {
    use super::*;

    #[test]
    fn test_extract_function_calls() {
        let c_code = r#"
#include <stdio.h>

int add(int a, int b) {
    return a + b;
}

int calculate() {
    int result = add(5, 3);      // Function call to add
    printf("Result: %d\n", result);    // Function call to printf
    return result;
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
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

        let printf_call = identifiers.iter().find(|id| id.name == "printf");
        assert!(
            printf_call.is_some(),
            "Should extract 'printf' function call identifier"
        );
        let printf_call = printf_call.unwrap();
        assert_eq!(printf_call.kind, IdentifierKind::Call);

        // Verify containing symbol is set correctly (should be inside calculate function)
        assert!(
            add_call.containing_symbol_id.is_some(),
            "Function call should have containing symbol"
        );

        // Find the calculate function symbol
        let calculate_function = symbols.iter().find(|s| s.name == "calculate").unwrap();

        // Verify the add call is contained within calculate function
        assert_eq!(
            add_call.containing_symbol_id.as_ref(),
            Some(&calculate_function.id),
            "add call should be contained within calculate function"
        );
    }

    #[test]
    fn test_extract_member_access() {
        let c_code = r#"
typedef struct {
    int x;
    int y;
} Point;

void print_point(Point* p) {
    printf("x: %d\n", p->x);   // Member access: p->x
    printf("y: %d\n", p->y);   // Member access: p->y
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found member access identifiers
        let x_access = identifiers
            .iter()
            .filter(|id| id.name == "x" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(x_access > 0, "Should extract 'x' member access identifier");

        let y_access = identifiers
            .iter()
            .filter(|id| id.name == "y" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(y_access > 0, "Should extract 'y' member access identifier");
    }

    #[test]
    fn test_file_scoped_containing_symbol() {
        // This test ensures we ONLY match symbols from the SAME FILE
        // Critical bug fix from Rust implementation (line 1311-1318 in rust.rs)
        let c_code = r#"
void helper() {
    // Helper function
}

void process() {
    helper();              // Call to helper in same file
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
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
        let process_function = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(
            helper_call.containing_symbol_id.as_ref(),
            Some(&process_function.id),
            "helper call should be contained within process function"
        );
    }

    #[test]
    fn test_chained_member_access() {
        let c_code = r#"
typedef struct {
    int balance;
} Account;

typedef struct {
    Account* account;
} User;

void execute(User* user) {
    int balance = user->account->balance;   // Chained member access
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract the rightmost identifier in the chain
        let balance_access = identifiers
            .iter()
            .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            balance_access.is_some(),
            "Should extract 'balance' from chained member access"
        );
    }

    #[test]
    fn test_no_duplicate_identifiers() {
        let c_code = r#"
void process() {
    // Process implementation
}

void run() {
    process();
    process();  // Same call twice
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
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
}
