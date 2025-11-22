//! Cross-File Relationship Extraction Tests for Dart
//!
//! These tests verify that function calls across file boundaries are correctly
//! captured as PendingRelationships. This is critical for trace_call_path to work.
//!
//! Architecture:
//! - Same-file calls → Relationship (directly resolved)
//! - Cross-file calls → PendingRelationship (resolved after workspace indexing)

use crate::extractors::base::{PendingRelationship, RelationshipKind};
use crate::extractors::factory::extract_symbols_and_relationships;
use crate::extractors::{ExtractionResults, Relationship, Symbol};
use std::path::PathBuf;
use tree_sitter::Parser;

#[cfg(test)]
mod tests {
    use super::*;

    fn init_dart_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&harper_tree_sitter_dart::LANGUAGE.into())
            .expect("Error loading Dart grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_dart_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "dart", &workspace_root)
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
        // File A: defines helper function
        let file_a_code = r#"
int helperFunction(int x) {
    return x * 2;
}
"#;

        // File B: calls helper_function (imported from file A)
        let file_b_code = r#"
import 'utils.dart';

int mainFunction() {
    final result = helperFunction(21);  // Cross-file call!
    return result;
}
"#;

        // Extract from both files
        let results_a = extract_full("lib/utils.dart", file_a_code);
        let results_b = extract_full("lib/main.dart", file_b_code);

        // Verify we extracted the symbols
        let helper_fn = results_a.symbols.iter().find(|s| s.name == "helperFunction");
        assert!(
            helper_fn.is_some(),
            "Should extract helperFunction from utils.dart"
        );

        let main_fn = results_b.symbols.iter().find(|s| s.name == "mainFunction");
        assert!(
            main_fn.is_some(),
            "Should extract mainFunction from main.dart"
        );

        // Debug output
        println!("=== Main.dart symbols ===");
        for s in &results_b.symbols {
            println!("  {} ({:?}) at line {}", s.name, s.kind, s.start_line);
        }
        println!("=== Main.dart relationships (resolved) ===");
        for r in &results_b.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }
        println!("=== Main.dart pending_relationships ===");
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
            "Should NOT create resolved Relationship for cross-file call.\n\
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
            "Should create PendingRelationship for cross-file call.\n\
             Found {} pending relationships, expected at least 1.",
            pending_calls.len()
        );

        // Verify the pending relationship has the correct callee name
        let helper_pending = pending_calls
            .iter()
            .find(|p| p.callee_name == "helperFunction");

        assert!(
            helper_pending.is_some(),
            "PendingRelationship should have callee_name='helperFunction'.\n\
             Found: {:?}",
            pending_calls.iter().map(|p| &p.callee_name).collect::<Vec<_>>()
        );

        // Verify the pending relationship has the correct caller
        let main_fn_ids: Vec<_> = results_b
            .symbols
            .iter()
            .filter(|s| s.name == "mainFunction")
            .map(|s| s.id.clone())
            .collect();
        let pending = helper_pending.unwrap();
        assert!(
            main_fn_ids.contains(&pending.from_symbol_id),
            "PendingRelationship should be from mainFunction.\n\
             Got from_symbol_id: {}\n\
             Available mainFunction IDs: {:?}",
            pending.from_symbol_id,
            main_fn_ids
        );
    }

    #[test]
    fn test_cross_file_method_call_creates_pending_relationship() {
        // File A: defines a class with methods
        let file_a_code = r#"
class Calculator {
    Calculator(int value) {
        this.value = value;
    }

    int double() {
        return this.value * 2;
    }

    int value;
}
"#;

        // File B: uses Calculator from file A
        let file_b_code = r#"
import 'calculator.dart';

int process() {
    final calc = Calculator(21);  // Cross-file constructor call
    return calc.double();           // Cross-file method call
}
"#;

        let results_a = extract_full("lib/calculator.dart", file_a_code);
        let results_b = extract_full("lib/processor.dart", file_b_code);

        // Verify symbols exist
        assert!(
            results_a.symbols.iter().any(|s| s.name == "Calculator"),
            "Should extract Calculator class"
        );
        assert!(
            results_a.symbols.iter().any(|s| s.name == "double"),
            "Should extract double method"
        );
        assert!(
            results_b.symbols.iter().any(|s| s.name == "process"),
            "Should extract process function"
        );

        // Debug output
        println!("=== Processor pending_relationships ===");
        for p in &results_b.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (needs resolution)",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // Cross-file method calls should create PendingRelationships
        let pending_calls: Vec<_> = results_b
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !pending_calls.is_empty(),
            "Cross-file method calls should create PendingRelationships!\n\
             process() calls Calculator constructor and calc.double() but no pending relationships were created.\n\
             Found {} pending relationships, expected at least 1.",
            pending_calls.len()
        );

        // Verify we captured at least the method calls we expect
        let callee_names: Vec<_> = pending_calls.iter().map(|p| &p.callee_name).collect();
        println!("Captured callee names: {:?}", callee_names);

        // We should have captured either 'Calculator', 'double', or both
        let has_constructor = callee_names.iter().any(|n| *n == "Calculator");
        let has_double = callee_names.iter().any(|n| *n == "double");

        assert!(
            has_constructor || has_double,
            "Should capture at least 'Calculator' or 'double' method calls.\n\
             Found: {:?}",
            callee_names
        );
    }

    // ========================================================================
    // TEST: Same-file calls should still work (regression test)
    // ========================================================================

    #[test]
    fn test_same_file_function_call_creates_relationship() {
        // Both functions in the same file - this should work with resolved Relationship
        let code = r#"
int helper(int x) {
    return x * 2;
}

int caller() {
    return helper(21);  // Same-file call
}
"#;

        let (symbols, relationships) = extract_from_file("lib/same_file.dart", code);

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

    #[test]
    fn test_local_function_call_creates_resolved_relationship() {
        // Dart code with local function and call
        let code = r#"
void helper() {
    print('helper');
}

void main() {
    helper();
}
"#;

        let results = extract_full("bin/main.dart", code);

        // Verify symbols
        assert!(
            results.symbols.iter().any(|s| s.name == "helper"),
            "Should extract helper function"
        );
        assert!(
            results.symbols.iter().any(|s| s.name == "main"),
            "Should extract main function"
        );

        // Helper function call should create normal Relationship (not pending)
        let call_rels: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !call_rels.is_empty(),
            "Local function call should create a resolved Relationship"
        );

        // Should NOT create pending relationships for same-file calls to 'helper'
        // Note: There may be pending relationships for built-in functions like 'print'
        let pending_helper_calls: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls && p.callee_name == "helper")
            .collect();

        assert!(
            pending_helper_calls.is_empty(),
            "Same-file calls should NOT create pending relationships for local functions.\n\
             Found {} pending relationships to 'helper', expected 0.",
            pending_helper_calls.len()
        );
    }
}
