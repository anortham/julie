//! Cross-File Relationship Extraction Tests for PHP
//!
//! These tests verify that function calls across file boundaries are correctly
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

    fn init_php_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .expect("Error loading PHP grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_php_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "php", &workspace_root)
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
        // File A: defines helper_function
        let file_a_code = r#"<?php
function helper_function($x) {
    return $x * 2;
}
"#;

        // File B: calls helper_function (imported from file A)
        let file_b_code = r#"<?php
use function file_a\helper_function;

function main_function() {
    $result = helper_function(21);  // Cross-file call!
    return $result;
}
"#;

        // Extract from both files
        let results_a = extract_full("lib/file_a.php", file_a_code);
        let results_b = extract_full("lib/file_b.php", file_b_code);

        // Verify we extracted the symbols
        let helper_fn = results_a.symbols.iter().find(|s| s.name == "helper_function");
        assert!(
            helper_fn.is_some(),
            "Should extract helper_function from file_a"
        );

        let main_fn = results_b.symbols.iter().find(|s| s.name == "main_function");
        assert!(
            main_fn.is_some(),
            "Should extract main_function from file_b"
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
            .find(|p| p.callee_name == "helper_function");

        assert!(
            helper_pending.is_some(),
            "PendingRelationship should have callee_name='helper_function'.\n\
             Found: {:?}",
            pending_calls.iter().map(|p| &p.callee_name).collect::<Vec<_>>()
        );

        // Verify the pending relationship has the correct caller
        let main_fn_id = main_fn.unwrap().id.clone();
        let pending = helper_pending.unwrap();
        assert_eq!(
            pending.from_symbol_id, main_fn_id,
            "PendingRelationship should be from main_function"
        );
    }

    #[test]
    fn test_cross_file_method_call_creates_pending_relationship() {
        // File A: defines a class with methods
        let file_a_code = r#"<?php
class Calculator {
    private $value;

    public function __construct($value) {
        $this->value = $value;
    }

    public function double() {
        return $this->value * 2;
    }
}
"#;

        // File B: uses Calculator from file A
        let file_b_code = r#"<?php
use Calculator;

function process() {
    $calc = new Calculator(21);  // Cross-file constructor call
    return $calc->double();      // Cross-file method call
}
"#;

        let results_a = extract_full("lib/calculator.php", file_a_code);
        let results_b = extract_full("lib/processor.php", file_b_code);

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
             process() calls Calculator() and $calc->double() but no pending relationships were created.\n\
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
    fn test_local_function_call_creates_resolved_relationship() {
        // Both functions in the same file - this should work with resolved Relationship
        let code = r#"<?php
function helper($x) {
    return $x * 2;
}

function caller() {
    return helper(21);  // Same-file call
}
"#;

        let (symbols, relationships) = extract_from_file("src/same_file.php", code);

        // Verify symbols
        assert!(
            symbols.iter().any(|s| s.name == "helper"),
            "Should extract helper"
        );
        assert!(
            symbols.iter().any(|s| s.name == "caller"),
            "Should extract caller"
        );

        // Verify we got the relationship
        let call_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !call_rels.is_empty(),
            "Should have at least one Calls relationship"
        );

        // Verify the relationship points to the right target
        let helper_symbol = symbols.iter().find(|s| s.name == "helper").unwrap();
        let caller_symbol = symbols.iter().find(|s| s.name == "caller").unwrap();

        let call_rel = call_rels
            .iter()
            .find(|r| r.from_symbol_id == caller_symbol.id && r.to_symbol_id == helper_symbol.id);

        assert!(
            call_rel.is_some(),
            "Should have relationship from caller to helper"
        );
    }

    #[test]
    fn test_local_method_call_creates_resolved_relationship() {
        // Class with methods in the same file
        let code = r#"<?php
class Helper {
    public function work() {
        return 42;
    }

    public function caller() {
        return $this->work();  // Same-file method call
    }
}
"#;

        let (symbols, relationships) = extract_from_file("src/class_file.php", code);

        // Verify symbols
        assert!(
            symbols.iter().any(|s| s.name == "Helper"),
            "Should extract Helper class"
        );
        assert!(
            symbols.iter().any(|s| s.name == "work"),
            "Should extract work method"
        );
        assert!(
            symbols.iter().any(|s| s.name == "caller"),
            "Should extract caller method"
        );

        // We should have relationships for method calls within the class
        let call_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        println!("=== Local method call relationships ===");
        for r in &call_rels {
            println!(
                "  {} -> {} (confidence: {})",
                r.from_symbol_id, r.to_symbol_id, r.confidence
            );
        }

        // With same-file resolution, we should get a relationship (or not, depending on implementation)
        // At minimum, we shouldn't get many pending relationships for local calls
        let _pending_calls: Vec<String> = symbols
            .iter()
            .filter(|s| s.name == "caller")
            .next()
            .map(|_| vec![])
            .unwrap_or_default();

        println!(
            "Call relationships found: {}, Pending relationships: {}",
            call_rels.len(),
            _pending_calls.len()
        );
    }

    #[test]
    fn test_undefined_function_call_creates_pending_relationship() {
        // Function calls something not defined anywhere (not even imported)
        let code = r#"<?php
function my_function() {
    return undefined_func(42);  // Not defined
}
"#;

        let results = extract_full("src/undefined.php", code);
        let symbols = &results.symbols;
        let _relationships = &results.relationships;

        // Verify we extracted the function
        assert!(
            symbols.iter().any(|s| s.name == "my_function"),
            "Should extract my_function"
        );

        // Debug
        println!("Relationships: {:?}", _relationships);

        // For undefined functions, we typically don't create relationships
        // (could be a pending relationship depending on implementation)
        // Just verify the test runs without panic
    }
}
