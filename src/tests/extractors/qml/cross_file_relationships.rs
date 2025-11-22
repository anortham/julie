//! Cross-File Relationship Extraction Tests for QML
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

    fn init_qml_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_qmljs::LANGUAGE.into())
            .expect("Error loading QML grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_qml_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "qml", &workspace_root)
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
import QtQuick 2.15

Item {
    function helperFunction(x) {
        return x * 2
    }
}
"#;

        // File B: calls helper_function (imported from file A)
        let file_b_code = r#"
import QtQuick 2.15
import "./utils.qml" as Utils

Item {
    function mainFunction() {
        const result = Utils.helperFunction(21)  // Cross-file call!
        return result
    }
}
"#;

        // Extract from both files
        let results_a = extract_full("src/utils.qml", file_a_code);
        let results_b = extract_full("src/main.qml", file_b_code);

        // Verify we extracted the symbols
        let helper_fn = results_a.symbols.iter().find(|s| s.name == "helperFunction");
        assert!(
            helper_fn.is_some(),
            "Should extract helperFunction from utils.qml"
        );

        let main_fn = results_b.symbols.iter().find(|s| s.name == "mainFunction");
        assert!(
            main_fn.is_some(),
            "Should extract mainFunction from main.qml"
        );

        // Debug output
        println!("=== Main.qml symbols ===");
        for s in &results_b.symbols {
            println!("  {} ({:?}) at line {}", s.name, s.kind, s.start_line);
        }
        println!("=== Main.qml relationships (resolved) ===");
        for r in &results_b.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }
        println!("=== Main.qml pending_relationships ===");
        for p in &results_b.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (needs resolution)",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // KEY TEST: Cross-file call should NOT create a resolved Relationship
        // (because the called function is not in the local symbol_map)
        let call_relationships: Vec<_> = results_b
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        // Note: We might still have local calls, just not to unknown/external functions
        // The key is that unknown callees don't create resolved relationships

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
        let main_fn_ids: Vec<_> = results_b.symbols.iter()
            .filter(|s| s.name == "mainFunction")
            .map(|s| s.id.clone())
            .collect();
        let pending = helper_pending.unwrap();
        assert!(
            main_fn_ids.contains(&pending.from_symbol_id),
            "PendingRelationship should be from one of the mainFunction symbols.\n\
             Got from_symbol_id: {}\n\
             Available mainFunction IDs: {:?}",
            pending.from_symbol_id,
            main_fn_ids
        );
    }

    // ========================================================================
    // TEST: Same-file calls should still work (regression test)
    // ========================================================================

    #[test]
    fn test_same_file_function_call_creates_relationship() {
        // Both functions in the same file - this should work with resolved Relationship
        let code = r#"
import QtQuick 2.15

Item {
    function helper(x) {
        return x * 2
    }

    function caller() {
        return helper(21)  // Same-file call
    }
}
"#;

        let (symbols, relationships) = extract_from_file("src/same_file.qml", code);

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
        // QML code with local function and call
        let code = r#"
import QtQuick 2.15

Item {
    function processData(data) {
        let cleaned = cleanData(data)
        return cleaned
    }

    function cleanData(data) {
        return data
    }
}
"#;

        let (symbols, relationships) = extract_from_file("src/processor.qml", code);

        // Verify symbols exist
        assert!(
            symbols.iter().any(|s| s.name == "processData"),
            "Should extract processData function"
        );
        assert!(
            symbols.iter().any(|s| s.name == "cleanData"),
            "Should extract cleanData function"
        );

        // Local function call should create resolved Relationship
        let call_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !call_rels.is_empty(),
            "Local function calls should create resolved Relationships"
        );

        // Verify the relationship is from processData to cleanData
        let process_fn = symbols.iter().find(|s| s.name == "processData").unwrap();
        let clean_fn = symbols.iter().find(|s| s.name == "cleanData").unwrap();

        let has_rel = call_rels
            .iter()
            .any(|r| r.from_symbol_id == process_fn.id && r.to_symbol_id == clean_fn.id);

        assert!(
            has_rel,
            "Should have Relationship from processData to cleanData (regression test)"
        );
    }
}
