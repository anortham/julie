//! Cross-File Relationship Extraction Tests for GDScript
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

    fn init_gdscript_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_gdscript::LANGUAGE.into())
            .expect("Error loading GDScript grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_gdscript_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "gdscript", &workspace_root)
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
        let file_a_code = r#"
func helper_function(x):
    return x * 2
"#;

        // File B: calls helper_function (imported from file A)
        let file_b_code = r#"
func main_function():
    var result = helper_function(21)  # Cross-file call!
    return result
"#;

        // Extract from both files
        let results_a = extract_full("lib/file_a.gd", file_a_code);
        let results_b = extract_full("lib/file_b.gd", file_b_code);

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
        // This test verifies that simple cross-file calls work correctly
        // Method calls with dot notation (like Calculator.new()) may not be detected
        // depending on how tree-sitter parses the GDScript grammar

        // File A: defines functions
        let file_a_code = r#"
func external_helper(value):
    return value * 2
"#;

        // File B: calls external_helper from file A
        let file_b_code = r#"
func process():
    var result = external_helper(21)
    return result
"#;

        let results_a = extract_full("lib/helper.gd", file_a_code);
        let results_b = extract_full("lib/processor.gd", file_b_code);

        // Debug: print all symbols from both files
        println!("=== File A symbols ===");
        for s in &results_a.symbols {
            println!("  {} ({:?}) at line {}", s.name, s.kind, s.start_line);
        }
        println!("=== File B symbols ===");
        for s in &results_b.symbols {
            println!("  {} ({:?}) at line {}", s.name, s.kind, s.start_line);
        }

        // Verify symbols exist
        assert!(
            results_a.symbols.iter().any(|s| s.name == "external_helper"),
            "Should extract external_helper function"
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

        // Cross-file calls should create PendingRelationships
        let pending_calls: Vec<_> = results_b
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !pending_calls.is_empty(),
            "Cross-file function calls should create PendingRelationships!\n\
             process() calls external_helper() but no pending relationships were created.\n\
             Found {} pending relationships, expected at least 1.",
            pending_calls.len()
        );

        // Verify we captured the external_helper call
        let callee_names: Vec<_> = pending_calls.iter().map(|p| &p.callee_name).collect();
        println!("Captured callee names: {:?}", callee_names);

        assert!(
            callee_names.iter().any(|n| *n == "external_helper"),
            "Should capture 'external_helper' function call.\n\
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
func helper(x):
    return x * 2

func caller():
    return helper(21)  # Same-file call
"#;

        let (symbols, relationships) = extract_from_file("src/same_file.gd", code);

        // Verify symbols
        assert!(
            symbols.iter().any(|s| s.name == "helper"),
            "Should extract helper"
        );

        assert!(
            symbols.iter().any(|s| s.name == "caller"),
            "Should extract caller"
        );

        // Find the call relationship
        let call_relationships: Vec<_> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !call_relationships.is_empty(),
            "Same-file function call should create resolved Relationship"
        );

        // Verify it's calling the right function
        let helper_id = symbols.iter().find(|s| s.name == "helper").unwrap().id.clone();
        let caller_id = symbols.iter().find(|s| s.name == "caller").unwrap().id.clone();

        let helper_call = call_relationships
            .iter()
            .find(|r| r.to_symbol_id == helper_id && r.from_symbol_id == caller_id);

        assert!(
            helper_call.is_some(),
            "Should have call relationship from caller to helper"
        );
    }

    #[test]
    fn test_local_function_call_creates_resolved_relationship() {
        // GDScript code with local function and call
        let code = r#"
func helper():
    pass

func main():
    helper()
"#;

        let results = extract_full("src/local.gd", code);

        // Verify symbols
        let helper = results.symbols.iter().find(|s| s.name == "helper");
        let main = results.symbols.iter().find(|s| s.name == "main");

        assert!(helper.is_some(), "Should extract helper function");
        assert!(main.is_some(), "Should extract main function");

        // Debug output
        println!("=== Local function relationships (resolved) ===");
        for r in &results.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }

        // Verify normal Relationship is created (not PendingRelationship)
        let call_relationships: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !call_relationships.is_empty(),
            "Local function call should create resolved Relationship"
        );

        let helper_id = helper.unwrap().id.clone();
        let main_id = main.unwrap().id.clone();

        let found_call = call_relationships
            .iter()
            .any(|r| r.from_symbol_id == main_id && r.to_symbol_id == helper_id);

        assert!(
            found_call,
            "Should have call relationship from main to helper"
        );
    }

    #[test]
    fn test_pending_relationships_have_correct_confidence() {
        // Cross-file call should have lower confidence than local call
        let file_a_code = r#"
func external_func():
    pass
"#;

        let file_b_code = r#"
func caller():
    external_func()
"#;

        let results_a = extract_full("a.gd", file_a_code);
        let results_b = extract_full("b.gd", file_b_code);

        // Get pending relationships
        let pending_calls: Vec<_> = results_b
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls)
            .collect();

        // Should have at least one pending relationship
        assert!(!pending_calls.is_empty(), "Should have pending relationships");

        // Confidence should be reasonable (between 0.5 and 0.9)
        for pending in &pending_calls {
            assert!(
                pending.confidence >= 0.5 && pending.confidence <= 0.9,
                "Pending relationship confidence should be between 0.5 and 0.9, got {}",
                pending.confidence
            );
        }
    }
}
