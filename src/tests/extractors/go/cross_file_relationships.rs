//! Cross-File Relationship Extraction Tests for Go
//!
//! These tests verify that function calls across package boundaries are correctly
//! captured as PendingRelationships. This is critical for trace_call_path to work.
//!
//! Architecture:
//! - Same-file calls → Relationship (directly resolved)
//! - Cross-file/cross-package calls → PendingRelationship (resolved after workspace indexing)

use crate::extractors::base::{PendingRelationship, RelationshipKind};
use crate::extractors::factory::extract_symbols_and_relationships;
use crate::extractors::{ExtractionResults, Relationship, Symbol};
use std::path::PathBuf;
use tree_sitter::Parser;

#[cfg(test)]
mod tests {
    use super::*;

    fn init_go_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("Error loading Go grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_go_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "go", &workspace_root)
            .expect("Failed to extract")
    }

    /// Helper to extract just symbols and relationships (for backward compat)
    fn extract_from_file(filename: &str, code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
        let results = extract_full(filename, code);
        (results.symbols, results.relationships)
    }

    // ========================================================================
    // TEST: Cross-package function calls should create PendingRelationship
    // ========================================================================

    #[test]
    fn test_cross_package_function_call_creates_pending_relationship() {
        // File A: defines a helper function in utils package
        let file_a_code = r#"
package utils

func HelperFunction(x int) int {
    return x * 2
}
"#;

        // File B: calls HelperFunction from utils package
        let file_b_code = r#"
package main

import "myapp/utils"

func MainFunction() int {
    result := utils.HelperFunction(21)  // Cross-package call!
    return result
}
"#;

        // Extract from both files
        let results_a = extract_full("utils/helper.go", file_a_code);
        let results_b = extract_full("main.go", file_b_code);

        // Verify we extracted the symbols
        let helper_fn = results_a.symbols.iter().find(|s| s.name == "HelperFunction");
        assert!(
            helper_fn.is_some(),
            "Should extract HelperFunction from utils package"
        );

        let main_fn = results_b.symbols.iter().find(|s| s.name == "MainFunction");
        assert!(
            main_fn.is_some(),
            "Should extract MainFunction from main package"
        );

        // Debug output
        println!("=== Main file symbols ===");
        for s in &results_b.symbols {
            println!("  {} ({:?}) at line {}", s.name, s.kind, s.start_line);
        }
        println!("=== Main file relationships (resolved) ===");
        for r in &results_b.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }
        println!("=== Main file pending_relationships ===");
        for p in &results_b.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (needs resolution)",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // KEY TEST: Cross-package call should NOT create a resolved Relationship
        // (because the target is unknown at extraction time)
        let call_relationships: Vec<_> = results_b
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            call_relationships.is_empty(),
            "Should NOT create resolved Relationship for cross-package call.\n\
             Found {} relationships, expected 0.\n\
             Cross-package calls should create PendingRelationship instead.",
            call_relationships.len()
        );

        // KEY TEST: Cross-package call SHOULD create a PendingRelationship
        let pending_calls: Vec<_> = results_b
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !pending_calls.is_empty(),
            "Should create PendingRelationship for cross-package call.\n\
             Found {} pending relationships, expected at least 1.\n\
             This is the main bug: cross-file calls are being silently dropped!",
            pending_calls.len()
        );

        // Verify the pending relationship has the correct callee name
        let helper_pending = pending_calls
            .iter()
            .find(|p| p.callee_name == "HelperFunction");

        assert!(
            helper_pending.is_some(),
            "PendingRelationship should have callee_name='HelperFunction'.\n\
             Found: {:?}",
            pending_calls.iter().map(|p| &p.callee_name).collect::<Vec<_>>()
        );

        // Verify the pending relationship has the correct caller
        let main_fn_id = main_fn.unwrap().id.clone();
        let pending = helper_pending.unwrap();
        assert_eq!(
            pending.from_symbol_id, main_fn_id,
            "PendingRelationship should be from MainFunction"
        );
    }

    #[test]
    fn test_builtin_function_call_creates_pending_relationship() {
        // Go code that calls builtin functions from other packages
        let code = r#"
package main

import "fmt"

func main() {
    fmt.Println("Hello")  // Cross-package builtin call!
}
"#;

        let results = extract_full("main.go", code);

        // Debug output
        println!("=== Symbols ===");
        for s in &results.symbols {
            println!("  {} ({:?})", s.name, s.kind);
        }
        println!("=== Pending relationships ===");
        for p in &results.pending_relationships {
            println!("  {:?}: {} -> '{}'", p.kind, p.from_symbol_id, p.callee_name);
        }

        // Should have extracted main function
        let main_fn = results.symbols.iter().find(|s| s.name == "main");
        assert!(main_fn.is_some(), "Should extract main function");

        // Cross-package builtin calls should create PendingRelationship
        let pending_calls: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !pending_calls.is_empty(),
            "fmt.Println() should create PendingRelationship.\n\
             Found {} pending relationships, expected at least 1.",
            pending_calls.len()
        );

        // Verify we captured Println
        let println_pending = pending_calls
            .iter()
            .find(|p| p.callee_name == "Println");

        assert!(
            println_pending.is_some(),
            "PendingRelationship should have callee_name='Println'.\n\
             Found: {:?}",
            pending_calls.iter().map(|p| &p.callee_name).collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // TEST: Same-file calls should still work (regression test)
    // ========================================================================

    #[test]
    fn test_same_file_function_call_creates_relationship() {
        // Both functions in the same file - this should work with resolved Relationship
        let code = r#"
package main

func helper(x int) int {
    return x * 2
}

func caller() int {
    return helper(21)  // Same-file call
}
"#;

        let (symbols, relationships) = extract_from_file("same_file.go", code);

        // Verify symbols
        assert!(
            symbols.iter().any(|s| s.name == "helper"),
            "Should extract helper"
        );
        assert!(
            symbols.iter().any(|s| s.name == "caller"),
            "Should extract caller"
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

        assert!(has_correct_rel, "Should have relationship from caller to helper");
    }
}
