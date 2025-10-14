// Java Identifier Extraction Tests (TDD RED phase)
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Function/method calls (method_invocation)
// - Member/field access (field_access)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

#![allow(unused_imports)]

use crate::extractors::base::{IdentifierKind, SymbolKind};
use crate::extractors::java::JavaExtractor;
use crate::tests::java::init_parser;

#[cfg(test)]
mod identifier_extraction_tests {
    use super::*;

    #[test]
    fn test_extract_function_calls() {
        let java_code = r#"
public class Calculator {
    public int add(int a, int b) {
        return a + b;
    }

    public int calculate() {
        int result = add(5, 3);           // Method call to add
        System.out.println(result);       // Method call to println
        return result;
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(java_code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
        );

        // Extract symbols first
        let symbols = extractor.extract_symbols(&tree);

        // NOW extract identifiers (this will FAIL until we implement it)
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found the method calls
        let add_call = identifiers.iter().find(|id| id.name == "add");
        assert!(
            add_call.is_some(),
            "Should extract 'add' method call identifier"
        );
        let add_call = add_call.unwrap();
        assert_eq!(add_call.kind, IdentifierKind::Call);

        let println_call = identifiers.iter().find(|id| id.name == "println");
        assert!(
            println_call.is_some(),
            "Should extract 'println' method call identifier"
        );
        let println_call = println_call.unwrap();
        assert_eq!(println_call.kind, IdentifierKind::Call);

        // Verify containing symbol is set correctly (should be inside calculate method)
        assert!(
            add_call.containing_symbol_id.is_some(),
            "Method call should have containing symbol"
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
        let java_code = r#"
public class User {
    public String name;
    public String email;

    public void printInfo() {
        System.out.println(this.name);    // Field access: this.name
        String userEmail = this.email;    // Field access: this.email
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(java_code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found field access identifiers
        let name_access = identifiers
            .iter()
            .filter(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            name_access > 0,
            "Should extract 'name' field access identifier"
        );

        let email_access = identifiers
            .iter()
            .filter(|id| id.name == "email" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            email_access > 0,
            "Should extract 'email' field access identifier"
        );
    }

    #[test]
    fn test_file_scoped_containing_symbol() {
        // This test ensures we ONLY match symbols from the SAME FILE
        // Critical bug fix from Rust implementation (line 1311-1318 in rust.rs)
        let java_code = r#"
public class Service {
    public void process() {
        helper();              // Call to helper in same file
    }

    private void helper() {
        // Helper method
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(java_code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
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
        let java_code = r#"
public class DataService {
    public void execute() {
        double result = user.account.balance;      // Chained field access
        String name = customer.profile.fullName;   // Chained field access
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(java_code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract the rightmost identifiers in chains
        let balance_access = identifiers
            .iter()
            .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            balance_access.is_some(),
            "Should extract 'balance' from chained field access"
        );

        let fullname_access = identifiers
            .iter()
            .find(|id| id.name == "fullName" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            fullname_access.is_some(),
            "Should extract 'fullName' from chained field access"
        );
    }

    #[test]
    fn test_no_duplicate_identifiers() {
        let java_code = r#"
public class Test {
    public void run() {
        process();
        process();  // Same call twice
    }

    private void process() {
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(java_code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
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
