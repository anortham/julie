//! Cross-File Relationship Extraction Tests for Java
//!
//! These tests verify that method calls across file boundaries are correctly
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

    fn init_java_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .expect("Error loading Java grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_java_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "java", &workspace_root)
            .expect("Failed to extract")
    }

    /// Helper to extract just symbols and relationships (for backward compat)
    fn extract_from_file(filename: &str, code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
        let results = extract_full(filename, code);
        (results.symbols, results.relationships)
    }

    // ========================================================================
    // TEST: Cross-file method calls should create PendingRelationship
    // ========================================================================

    #[test]
    fn test_cross_file_method_call_creates_pending_relationship() {
        // File A: defines a class with a method
        let file_a_code = r#"
package com.utils;

public class Helper {
    public static int process(int x) {
        return x * 2;
    }
}
"#;

        // File B: calls Helper.process() (imported from file A)
        let file_b_code = r#"
package com.app;

import com.utils.Helper;

public class Main {
    public static void main(String[] args) {
        int result = Helper.process(21);  // Cross-file call!
    }
}
"#;

        // Extract from both files
        let results_a = extract_full("src/com/utils/Helper.java", file_a_code);
        let results_b = extract_full("src/com/app/Main.java", file_b_code);

        // Verify we extracted the symbols
        let process_method = results_a.symbols.iter().find(|s| s.name == "process");
        assert!(
            process_method.is_some(),
            "Should extract process method from Helper class"
        );

        let main_method = results_b.symbols.iter().find(|s| s.name == "main");
        assert!(
            main_method.is_some(),
            "Should extract main method from Main class"
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
        // (because pointing to Import symbol is useless for trace_call_path)
        let call_relationships: Vec<_> = results_b
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            call_relationships.is_empty(),
            "Should NOT create resolved Relationship for cross-file method call.\n\
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
            "Should create PendingRelationship for cross-file method call.\n\
             Found {} pending relationships, expected at least 1.",
            pending_calls.len()
        );

        // Verify the pending relationship has the correct callee name
        let process_pending = pending_calls.iter().find(|p| p.callee_name == "process");

        assert!(
            process_pending.is_some(),
            "PendingRelationship should have callee_name='process'.\n\
             Found: {:?}",
            pending_calls
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );

        // Verify the pending relationship has the correct caller
        let main_fn_id = main_method.unwrap().id.clone();
        let pending = process_pending.unwrap();
        assert_eq!(
            pending.from_symbol_id, main_fn_id,
            "PendingRelationship should be from main method"
        );
    }

    #[test]
    fn test_cross_file_constructor_call_creates_pending_relationship() {
        // File A: defines a class with a constructor
        let file_a_code = r#"
package com.utils;

public class Calculator {
    private int value;

    public Calculator(int val) {
        this.value = val;
    }

    public int getValue() {
        return value;
    }
}
"#;

        // File B: uses Calculator from file A
        let file_b_code = r#"
package com.app;

import com.utils.Calculator;

public class Processor {
    public int process(int x) {
        Calculator calc = new Calculator(x);  // Cross-file constructor call
        return calc.getValue();                 // Cross-file method call
    }
}
"#;

        let results_a = extract_full("src/com/utils/Calculator.java", file_a_code);
        let results_b = extract_full("src/com/app/Processor.java", file_b_code);

        // Verify symbols exist
        assert!(
            results_a.symbols.iter().any(|s| s.name == "Calculator"),
            "Should extract Calculator class"
        );
        assert!(
            results_a
                .symbols
                .iter()
                .any(|s| s.name == "Calculator" && s.kind.to_string() == "constructor"),
            "Should extract Calculator constructor"
        );
        assert!(
            results_a.symbols.iter().any(|s| s.name == "getValue"),
            "Should extract getValue method"
        );
        assert!(
            results_b.symbols.iter().any(|s| s.name == "process"),
            "Should extract process method"
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
             process() calls Calculator constructor and getValue() but no pending relationships were created.\n\
             Found {} pending relationships, expected at least 1.",
            pending_calls.len()
        );

        // Verify we captured at least one of the calls we expect
        let callee_names: Vec<_> = pending_calls.iter().map(|p| &p.callee_name).collect();
        println!("Captured callee names: {:?}", callee_names);

        // We should have captured either 'Calculator', 'getValue', or both
        let has_constructor = callee_names.iter().any(|n| *n == "Calculator");
        let has_method = callee_names.iter().any(|n| *n == "getValue");

        assert!(
            has_constructor || has_method,
            "Should capture at least constructor or method calls.\n\
             Found: {:?}",
            callee_names
        );
    }

    // ========================================================================
    // TEST: Same-file method calls should still work (regression test)
    // ========================================================================

    #[test]
    fn test_same_file_method_call_creates_relationship() {
        // Both methods in the same file - this should work with resolved Relationship
        let code = r#"
public class Calculator {
    private int helper(int x) {
        return x * 2;
    }

    public int caller(int x) {
        return helper(x);  // Same-file call
    }
}
"#;

        let (symbols, relationships) = extract_from_file("src/Calculator.java", code);

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
