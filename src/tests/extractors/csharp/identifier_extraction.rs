// C# Identifier Extraction Tests (TDD RED phase)
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Function calls (invocation_expression)
// - Member access (member_access_expression)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

#![allow(unused_imports)]

use crate::extractors::base::{IdentifierKind, SymbolKind};
use crate::extractors::csharp::CSharpExtractor;
use crate::tests::extractors::csharp::init_parser;
use std::path::PathBuf;

#[cfg(test)]
mod identifier_extraction_tests {
    use super::*;

    #[test]
    fn test_extract_function_calls() {
        let csharp_code = r#"
using System;

public class Calculator {
    public int Add(int a, int b) {
        return a + b;
    }

    public int Calculate() {
        int result = Add(5, 3);      // Function call to Add
        Console.WriteLine(result);    // Function call to WriteLine
        return result;
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(csharp_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "csharp".to_string(),
            "test.cs".to_string(),
            csharp_code.to_string(),
            &workspace_root,
        );

        // Extract symbols first
        let symbols = extractor.extract_symbols(&tree);

        // NOW extract identifiers (this will FAIL until we implement it)
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found the function calls
        let add_call = identifiers.iter().find(|id| id.name == "Add");
        assert!(
            add_call.is_some(),
            "Should extract 'Add' function call identifier"
        );
        let add_call = add_call.unwrap();
        assert_eq!(add_call.kind, IdentifierKind::Call);

        let writeline_call = identifiers.iter().find(|id| id.name == "WriteLine");
        assert!(
            writeline_call.is_some(),
            "Should extract 'WriteLine' function call identifier"
        );
        let writeline_call = writeline_call.unwrap();
        assert_eq!(writeline_call.kind, IdentifierKind::Call);

        // Verify containing symbol is set correctly (should be inside Calculate method)
        assert!(
            add_call.containing_symbol_id.is_some(),
            "Function call should have containing symbol"
        );

        // Find the Calculate method symbol
        let calculate_method = symbols.iter().find(|s| s.name == "Calculate").unwrap();

        // Verify the Add call is contained within Calculate method
        assert_eq!(
            add_call.containing_symbol_id.as_ref(),
            Some(&calculate_method.id),
            "Add call should be contained within Calculate method"
        );
    }

    #[test]
    fn test_extract_member_access() {
        let csharp_code = r#"
public class User {
    public string Name { get; set; }
    public string Email { get; set; }

    public void PrintInfo() {
        Console.WriteLine(this.Name);   // Member access: this.Name
        var email = this.Email;          // Member access: this.Email
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(csharp_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "csharp".to_string(),
            "test.cs".to_string(),
            csharp_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found member access identifiers
        let name_access = identifiers
            .iter()
            .filter(|id| id.name == "Name" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            name_access > 0,
            "Should extract 'Name' member access identifier"
        );

        let email_access = identifiers
            .iter()
            .filter(|id| id.name == "Email" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            email_access > 0,
            "Should extract 'Email' member access identifier"
        );
    }

    #[test]
    fn test_file_scoped_containing_symbol() {
        // This test ensures we ONLY match symbols from the SAME FILE
        // Critical bug fix from Rust implementation (line 1311-1318 in rust.rs)
        let csharp_code = r#"
public class Service {
    public void Process() {
        Helper();              // Call to Helper in same file
    }

    private void Helper() {
        // Helper method
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(csharp_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "csharp".to_string(),
            "test.cs".to_string(),
            csharp_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Find the Helper call
        let helper_call = identifiers.iter().find(|id| id.name == "Helper");
        assert!(helper_call.is_some());
        let helper_call = helper_call.unwrap();

        // Verify it has a containing symbol (the Process method)
        assert!(
            helper_call.containing_symbol_id.is_some(),
            "Helper call should have containing symbol from same file"
        );

        // Verify the containing symbol is the Process method
        let process_method = symbols.iter().find(|s| s.name == "Process").unwrap();
        assert_eq!(
            helper_call.containing_symbol_id.as_ref(),
            Some(&process_method.id),
            "Helper call should be contained within Process method"
        );
    }

    #[test]
    fn test_chained_member_access() {
        let csharp_code = r#"
public class DataService {
    public void Execute() {
        var result = user.Account.Balance;   // Chained member access
        var name = customer.Profile.Name;     // Chained member access
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(csharp_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "csharp".to_string(),
            "test.cs".to_string(),
            csharp_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract the rightmost identifiers in chains
        let balance_access = identifiers
            .iter()
            .find(|id| id.name == "Balance" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            balance_access.is_some(),
            "Should extract 'Balance' from chained member access"
        );

        let name_access = identifiers
            .iter()
            .find(|id| id.name == "Name" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            name_access.is_some(),
            "Should extract 'Name' from chained member access"
        );
    }

    #[test]
    fn test_no_duplicate_identifiers() {
        let csharp_code = r#"
public class Test {
    public void Run() {
        Process();
        Process();  // Same call twice
    }

    private void Process() {
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(csharp_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "csharp".to_string(),
            "test.cs".to_string(),
            csharp_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract BOTH calls (they're at different locations)
        let process_calls: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "Process" && id.kind == IdentifierKind::Call)
            .collect();

        assert_eq!(
            process_calls.len(),
            2,
            "Should extract both Process calls at different locations"
        );

        // Verify they have different line numbers
        assert_ne!(
            process_calls[0].start_line, process_calls[1].start_line,
            "Duplicate calls should have different line numbers"
        );
    }
}
