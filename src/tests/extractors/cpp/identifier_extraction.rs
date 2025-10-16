// C++ Identifier Extraction Tests (TDD RED phase)
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Function calls (call_expression)
// - Member access (field_expression)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

#![allow(unused_imports)]

use crate::extractors::base::{IdentifierKind, SymbolKind};
use crate::extractors::cpp::CppExtractor;
use crate::tests::extractors::cpp::init_parser;

#[cfg(test)]
mod identifier_extraction_tests {
    use super::*;

    #[test]
    fn test_extract_function_calls() {
        let cpp_code = r#"
class Calculator {
public:
    int add(int a, int b) {
        return a + b;
    }

    int calculate() {
        int result = add(5, 3);      // Function call to add
        printf("Result: %d\n", result);    // Function call to printf
        return result;
    }
};
"#;

        let mut parser = init_parser();
        let tree = parser.parse(cpp_code, None).unwrap();

        let mut extractor = CppExtractor::new("test.cpp".to_string(), cpp_code.to_string());

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
        let cpp_code = r#"
class User {
public:
    std::string name;
    std::string email;

    void printInfo() {
        std::cout << this->name << std::endl;   // Member access: this->name
        auto userEmail = this->email;           // Member access: this->email
    }
};
"#;

        let mut parser = init_parser();
        let tree = parser.parse(cpp_code, None).unwrap();

        let mut extractor = CppExtractor::new("test.cpp".to_string(), cpp_code.to_string());

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
        let cpp_code = r#"
class Service {
public:
    void process() {
        helper();              // Call to helper in same file
    }

private:
    void helper() {
        // Helper method
    }
};
"#;

        let mut parser = init_parser();
        let tree = parser.parse(cpp_code, None).unwrap();

        let mut extractor = CppExtractor::new("test.cpp".to_string(), cpp_code.to_string());

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
        let cpp_code = r#"
class DataService {
public:
    void execute() {
        auto balance = user.account.balance;   // Chained member access
        auto name = customer.profile.name;     // Chained member access
    }
};
"#;

        let mut parser = init_parser();
        let tree = parser.parse(cpp_code, None).unwrap();

        let mut extractor = CppExtractor::new("test.cpp".to_string(), cpp_code.to_string());

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
        let cpp_code = r#"
class Test {
public:
    void run() {
        process();
        process();  // Same call twice
    }

private:
    void process() {
    }
};
"#;

        let mut parser = init_parser();
        let tree = parser.parse(cpp_code, None).unwrap();

        let mut extractor = CppExtractor::new("test.cpp".to_string(), cpp_code.to_string());

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
