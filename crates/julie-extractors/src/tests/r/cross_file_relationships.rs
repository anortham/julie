//! Cross-File Relationship Extraction Tests for R
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

    fn init_r_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_r::LANGUAGE.into())
            .expect("Error loading R grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_r_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "r", &workspace_root)
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
helper_function <- function(x) {
    return(x * 2)
}
"#;

        // File B: calls helper_function (sourced from file A)
        let file_b_code = r#"
source("file_a.R")

main_function <- function() {
    result <- helper_function(21)  # Cross-file call!
    return(result)
}
"#;

        // Extract from both files
        let results_a = extract_full("lib/file_a.R", file_a_code);
        let results_b = extract_full("lib/file_b.R", file_b_code);

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
        // (because the helper function is not in this file's symbol map)
        let call_relationships: Vec<_> = results_b
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls && r.to_symbol_id.contains("helper_function"))
            .collect();

        assert!(
            call_relationships.is_empty(),
            "Should NOT create resolved Relationship for cross-file call to helper_function.\n\
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
    fn test_local_function_call_creates_resolved_relationship() {
        // Both functions in same file
        let code = r#"
helper <- function() {
    return(42)
}

main <- function() {
    result <- helper()  # Local call - should be resolved
    return(result)
}
"#;

        let results = extract_full("lib/main.R", code);

        // Debug output
        println!("=== Symbols ===");
        for s in &results.symbols {
            println!("  {} ({:?}) at line {}", s.name, s.kind, s.start_line);
        }
        println!("=== Relationships ===");
        for r in &results.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }
        println!("=== Pending relationships ===");
        for p in &results.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (needs resolution)",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // Find the functions
        let helper_fn = results.symbols.iter().find(|s| s.name == "helper");
        let main_fn = results.symbols.iter().find(|s| s.name == "main");

        assert!(helper_fn.is_some(), "Should extract helper function");
        assert!(main_fn.is_some(), "Should extract main function");

        // KEY TEST: Local call should create resolved Relationship (not pending)
        let resolved_calls: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !resolved_calls.is_empty(),
            "Should create resolved Relationship for local call.\n\
             Found {} resolved relationships, expected at least 1.",
            resolved_calls.len()
        );

        // Verify it points to the correct function
        let helper_id = helper_fn.unwrap().id.clone();
        let call_to_helper = resolved_calls
            .iter()
            .find(|r| r.to_symbol_id == helper_id);

        assert!(
            call_to_helper.is_some(),
            "Should have resolved Relationship pointing to helper function"
        );

        // LOCAL calls should NOT create pending relationships
        let pending_calls: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            pending_calls.is_empty(),
            "Should NOT create PendingRelationship for local call.\n\
             Found {} pending relationships, expected 0.",
            pending_calls.len()
        );
    }

    #[test]
    fn test_builtin_function_not_pending() {
        // R code calling built-in functions
        let code = r#"
process_data <- function(x) {
    result <- mean(x)         # Built-in: mean
    doubled <- x * 2          # Built-in: *
    printed <- print(result)  # Built-in: print
    return(result)
}
"#;

        let results = extract_full("lib/processor.R", code);

        // Debug output
        println!("=== Symbols ===");
        for s in &results.symbols {
            println!("  {} ({:?}) at line {}", s.name, s.kind, s.start_line);
        }
        println!("=== Relationships ===");
        for r in &results.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }
        println!("=== Pending relationships ===");
        for p in &results.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (needs resolution)",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // KEY TEST: Built-in function calls should NOT create PendingRelationship
        // (they are known to be in the R standard library)
        let pending_for_builtins: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| {
                p.kind == RelationshipKind::Calls
                    && (p.callee_name == "mean" || p.callee_name == "print")
            })
            .collect();

        assert!(
            pending_for_builtins.is_empty(),
            "Should NOT create PendingRelationship for built-in functions like mean, print.\n\
             Found {} pending relationships, expected 0.",
            pending_for_builtins.len()
        );
    }

    #[test]
    fn test_builtin_functions_can_still_create_relationships() {
        // Some built-in functions can still appear in relationships (just not pending)
        let code = r#"
process_data <- function(x) {
    result <- mean(x)  # Built-in: mean
    return(result)
}
"#;

        let results = extract_full("lib/processor.R", code);

        // Built-in function calls might appear in relationships (with builtin_ prefix)
        // or might not appear at all, but they should NEVER appear as pending
        let pending_builtin_calls: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls && p.callee_name == "mean")
            .collect();

        assert!(
            pending_builtin_calls.is_empty(),
            "Built-in functions should NOT create PendingRelationship"
        );
    }

    #[test]
    fn test_multiple_cross_file_calls() {
        // File A: defines multiple functions
        let file_a_code = r#"
add <- function(a, b) {
    return(a + b)
}

multiply <- function(a, b) {
    return(a * b)
}
"#;

        // File B: calls multiple functions from file A
        let file_b_code = r#"
source("file_a.R")

calculator <- function(x, y) {
    sum_result <- add(x, y)           # Cross-file call 1
    product_result <- multiply(x, y)  # Cross-file call 2
    return(list(sum = sum_result, product = product_result))
}
"#;

        let results_a = extract_full("lib/file_a.R", file_a_code);
        let results_b = extract_full("lib/file_b.R", file_b_code);

        // Find functions
        let add_fn = results_a.symbols.iter().find(|s| s.name == "add");
        let multiply_fn = results_a.symbols.iter().find(|s| s.name == "multiply");
        let calc_fn = results_b.symbols.iter().find(|s| s.name == "calculator");

        assert!(add_fn.is_some(), "Should extract add function");
        assert!(multiply_fn.is_some(), "Should extract multiply function");
        assert!(calc_fn.is_some(), "Should extract calculator function");

        // Debug output
        println!("=== File B pending_relationships ===");
        for p in &results_b.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (needs resolution)",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // Should have 2 pending relationships for the 2 cross-file calls
        let pending_calls: Vec<_> = results_b
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            pending_calls.len() >= 2,
            "Should create PendingRelationship for both cross-file calls.\n\
             Found {} pending relationships, expected at least 2.",
            pending_calls.len()
        );

        // Verify both function names are in pending relationships
        let calc_id = calc_fn.unwrap().id.clone();
        let pending_add = pending_calls.iter().find(|p| p.callee_name == "add");
        let pending_multiply = pending_calls.iter().find(|p| p.callee_name == "multiply");

        assert!(
            pending_add.is_some() && pending_add.unwrap().from_symbol_id == calc_id,
            "Should have PendingRelationship from calculator to add"
        );
        assert!(
            pending_multiply.is_some() && pending_multiply.unwrap().from_symbol_id == calc_id,
            "Should have PendingRelationship from calculator to multiply"
        );
    }

    #[test]
    fn test_mixed_local_and_cross_file_calls() {
        // File A: defines helper
        let file_a_code = r#"
helper <- function(x) {
    return(x * 2)
}
"#;

        // File B: defines local function and calls both local and cross-file functions
        let file_b_code = r#"
source("file_a.R")

local_helper <- function(x) {
    return(x + 1)
}

main <- function(x) {
    local_result <- local_helper(x)    # Local call
    cross_result <- helper(local_result) # Cross-file call
    return(cross_result)
}
"#;

        let results = extract_full("lib/file_b.R", file_b_code);

        // Debug output
        println!("=== Symbols ===");
        for s in &results.symbols {
            println!("  {} ({:?}) at line {}", s.name, s.kind, s.start_line);
        }
        println!("=== Relationships ===");
        for r in &results.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }
        println!("=== Pending relationships ===");
        for p in &results.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (needs resolution)",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // Find symbols
        let local_helper = results.symbols.iter().find(|s| s.name == "local_helper");
        let main = results.symbols.iter().find(|s| s.name == "main");

        assert!(local_helper.is_some(), "Should extract local_helper");
        assert!(main.is_some(), "Should extract main");

        // Should have 1 resolved relationship (main -> local_helper)
        let resolved_calls: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !resolved_calls.is_empty(),
            "Should have resolved Relationship for local call"
        );

        // Should have 1 pending relationship (main -> helper)
        let pending_calls: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls && p.callee_name == "helper")
            .collect();

        assert!(
            !pending_calls.is_empty(),
            "Should have PendingRelationship for cross-file call"
        );
    }

    #[test]
    fn test_piped_call_to_unknown_function() {
        // R code with pipe operator calling unknown function
        let code = r#"
process_data <- function(data) {
    result <- data %>% external_transform()  # Cross-file piped call
    return(result)
}
"#;

        let results = extract_full("lib/processor.R", code);

        // Debug output
        println!("=== Relationships ===");
        for r in &results.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }
        println!("=== Pending relationships ===");
        for p in &results.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (needs resolution)",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // The current implementation creates relationships with piped_ prefix
        // This test just ensures no regressions occur
        let process_fn = results.symbols.iter().find(|s| s.name == "process_data");
        assert!(process_fn.is_some(), "Should extract process_data function");
    }
}
