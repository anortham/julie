//! Cross-File Relationship Extraction Tests for Swift
//!
//! These tests verify that function/method calls across file boundaries are correctly
//! captured as PendingRelationships. This is critical for trace_call_path to work.
//!
//! Architecture:
//! - Same-file calls → Relationship (directly resolved)
//! - Cross-file calls → PendingRelationship (resolved after workspace indexing)

use crate::base::{PendingRelationship, RelationshipKind};
use crate::factory::extract_symbols_and_relationships;
use crate::{ExtractionResults, Relationship, Symbol};
use std::path::PathBuf;
use tree_sitter::Parser;

#[cfg(test)]
mod tests {
    use super::*;

    fn init_swift_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_swift::LANGUAGE.into())
            .expect("Error loading Swift grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_swift_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "swift", &workspace_root)
            .expect("Failed to extract")
    }

    /// Helper to extract just symbols and relationships (for backward compat)
    fn extract_from_file(filename: &str, code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
        let results = extract_full(filename, code);
        (results.symbols, results.relationships)
    }

    // ========================================================================
    // TEST: Cross-file function calls should create PendingRelationship
    // ========================================================================

    #[test]
    fn test_cross_file_function_call_creates_pending_relationship() {
        // File A: defines a function
        let file_a_code = r#"
func processData(x: Int) -> Int {
    return x * 2
}
"#;

        // File B: calls the function from file A
        let file_b_code = r#"
func main() {
    let result = processData(x: 21)  // Cross-file call!
}
"#;

        // Extract from both files
        let results_a = extract_full("Utils.swift", file_a_code);
        let results_b = extract_full("Main.swift", file_b_code);

        // Verify we extracted the symbols
        let process_func = results_a.symbols.iter().find(|s| s.name == "processData");
        assert!(
            process_func.is_some(),
            "Should extract processData function from Utils.swift"
        );

        let main_func = results_b.symbols.iter().find(|s| s.name == "main");
        assert!(
            main_func.is_some(),
            "Should extract main function from Main.swift"
        );

        // Debug output
        println!("=== File B symbols ===");
        for s in &results_b.symbols {
            println!("  {} ({:?}) at line {}", s.name, s.kind, s.start_line);
        }
        println!("=== File B relationships (resolved) ===");
        for r in &results_b.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }
        println!("=== File B pending_relationships ===");
        for p in &results_b.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (needs resolution)",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // KEY TEST: Cross-file call should NOT create a resolved Relationship
        let call_relationships: Vec<_> = results_b
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            call_relationships.is_empty(),
            "Should NOT create resolved Relationship for cross-file function call.\n\
             Found {} relationships, expected 0.\n\
             Cross-file calls should create PendingRelationship instead.",
            call_relationships.len()
        );

        // KEY TEST: Cross-file call SHOULD create a PendingRelationship
        let pending_calls: Vec<_> = results_b
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !pending_calls.is_empty(),
            "Should create PendingRelationship for cross-file function call.\n\
             Found {} pending relationships, expected at least 1.",
            pending_calls.len()
        );

        // Verify the pending relationship has the correct callee name
        let process_pending = pending_calls.iter().find(|p| p.callee_name == "processData");

        assert!(
            process_pending.is_some(),
            "PendingRelationship should have callee_name='processData'.\n\
             Found: {:?}",
            pending_calls
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );

        // Verify the pending relationship has the correct caller
        let main_fn_id = main_func.unwrap().id.clone();
        let pending = process_pending.unwrap();
        assert_eq!(
            pending.from_symbol_id, main_fn_id,
            "PendingRelationship should be from main function"
        );
    }

    #[test]
    fn test_cross_file_method_call_creates_pending_relationship() {
        // File A: defines a class with methods
        let file_a_code = r#"
class Helper {
    func process(x: Int) -> Int {
        return x * 2
    }
}
"#;

        // File B: calls methods from Helper class
        let file_b_code = r#"
class Main {
    func run(x: Int) {
        let helper = Helper()
        let result = helper.process(x: x)  // Cross-file method call!
        let num = process(x: 5)  // Direct call to cross-file function
    }
}

func process(x: Int) -> Int {
    return x * 2
}
"#;

        let results_a = extract_full("Utils.swift", file_a_code);
        let results_b = extract_full("Main.swift", file_b_code);

        // Verify symbols exist
        assert!(
            results_a.symbols.iter().any(|s| s.name == "Helper"),
            "Should extract Helper class"
        );
        assert!(
            results_a.symbols.iter().any(|s| s.name == "process"),
            "Should extract process method"
        );
        assert!(
            results_b.symbols.iter().any(|s| s.name == "run"),
            "Should extract run method"
        );

        // Debug output
        println!("=== Main pending_relationships ===");
        for p in &results_b.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (needs resolution)",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // Cross-file method and function calls should create PendingRelationships
        let pending_calls: Vec<_> = results_b
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !pending_calls.is_empty(),
            "Cross-file method calls should create PendingRelationships!\n\
             run() calls Helper() and process() but no pending relationships were created.\n\
             Found {} pending relationships, expected at least 2.",
            pending_calls.len()
        );

        // Verify we captured at least one call (Helper constructor OR process method)
        let callee_names: Vec<_> = pending_calls.iter().map(|p| &p.callee_name).collect();
        println!("Captured callee names: {:?}", callee_names);

        // We should have Helper and process in our captures
        let has_constructor = callee_names.iter().any(|n| *n == "Helper");
        let has_method_call = callee_names.iter().any(|n| *n == "process");

        assert!(
            has_constructor || has_method_call,
            "Should capture Helper constructor or process method call.\n\
             Found: {:?}",
            callee_names
        );
    }

    // ========================================================================
    // TEST: Same-file function calls should still work (regression test)
    // ========================================================================

    #[test]
    fn test_same_file_function_call_creates_relationship() {
        // Both functions in the same file - this should work with resolved Relationship
        let code = r#"
func helper(x: Int) -> Int {
    return x * 2
}

func caller(x: Int) -> Int {
    return helper(x: x)  // Same-file call
}
"#;

        let (symbols, relationships) = extract_from_file("Utils.swift", code);

        // Verify symbols
        assert!(
            symbols.iter().any(|s| s.name == "helper"),
            "Should extract helper function"
        );
        assert!(
            symbols.iter().any(|s| s.name == "caller"),
            "Should extract caller function"
        );

        // Same-file calls SHOULD create resolved Relationships
        let call_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !call_rels.is_empty(),
            "Same-file function calls should create resolved relationships.\n\
             Found {} call relationships, expected at least 1.",
            call_rels.len()
        );

        // Verify it's the right relationship
        let helper = symbols.iter().find(|s| s.name == "helper").unwrap();
        let caller = symbols.iter().find(|s| s.name == "caller").unwrap();

        let has_correct_rel = call_rels
            .iter()
            .any(|r| r.from_symbol_id == caller.id && r.to_symbol_id == helper.id);

        assert!(
            has_correct_rel,
            "Should have relationship from caller to helper"
        );
    }

    #[test]
    fn test_same_file_method_call_creates_relationship() {
        // Both methods in the same class - this should work with resolved Relationship
        let code = r#"
class Calculator {
    func helper(x: Int) -> Int {
        return x * 2
    }

    func caller(x: Int) -> Int {
        return helper(x: x)  // Same-file method call
    }
}
"#;

        let (symbols, relationships) = extract_from_file("Calculator.swift", code);

        // Verify symbols
        assert!(
            symbols.iter().any(|s| s.name == "helper"),
            "Should extract helper method"
        );
        assert!(
            symbols.iter().any(|s| s.name == "caller"),
            "Should extract caller method"
        );

        // Same-file calls SHOULD create resolved Relationships
        let call_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !call_rels.is_empty(),
            "Same-file method calls should create resolved relationships.\n\
             Found {} call relationships, expected at least 1.",
            call_rels.len()
        );

        // Verify it's the right relationship
        let helper = symbols.iter().find(|s| s.name == "helper").unwrap();
        let caller = symbols.iter().find(|s| s.name == "caller").unwrap();

        let has_correct_rel = call_rels
            .iter()
            .any(|r| r.from_symbol_id == caller.id && r.to_symbol_id == helper.id);

        assert!(
            has_correct_rel,
            "Should have relationship from caller to helper"
        );
    }
}
