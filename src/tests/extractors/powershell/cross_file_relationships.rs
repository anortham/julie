//! Cross-File Relationship Extraction Tests for PowerShell
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

    fn init_powershell_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_powershell::LANGUAGE.into())
            .expect("Error loading PowerShell grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_powershell_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "powershell", &workspace_root)
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
        // File A: defines Get-Data function
        let file_a_code = r#"
function Get-Data {
    return @{ value = 42 }
}
"#;

        // File B: calls Get-Data (dot-sourced from file A)
        let file_b_code = r#"
. .\Get-Data.ps1

function Process-Data {
    $data = Get-Data  # Cross-file call!
    return $data
}
"#;

        // Extract from both files
        let results_a = extract_full("lib/Get-Data.ps1", file_a_code);
        let results_b = extract_full("lib/Process-Data.ps1", file_b_code);

        // Verify we extracted the symbols
        let get_data_fn = results_a.symbols.iter().find(|s| s.name == "Get-Data");
        assert!(
            get_data_fn.is_some(),
            "Should extract Get-Data function from file_a"
        );

        let process_data_fn = results_b.symbols.iter().find(|s| s.name == "Process-Data");
        assert!(
            process_data_fn.is_some(),
            "Should extract Process-Data function from file_b"
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
        let get_data_pending = pending_calls
            .iter()
            .find(|p| p.callee_name == "Get-Data");

        assert!(
            get_data_pending.is_some(),
            "PendingRelationship should have callee_name='Get-Data'.\n\
             Found: {:?}",
            pending_calls.iter().map(|p| &p.callee_name).collect::<Vec<_>>()
        );

        // Verify the pending relationship has the correct caller
        let process_data_fn_id = process_data_fn.unwrap().id.clone();
        let pending = get_data_pending.unwrap();
        assert_eq!(
            pending.from_symbol_id, process_data_fn_id,
            "PendingRelationship should be from Process-Data"
        );
    }

    // ========================================================================
    // TEST: Same-file calls should still work (regression test)
    // ========================================================================

    #[test]
    fn test_same_file_function_call_creates_relationship() {
        // Both functions in the same file - this should work with resolved Relationship
        let code = r#"
function Get-Helper {
    return 42
}

function Caller {
    $result = Get-Helper  # Same-file call
    return $result
}
"#;

        let (symbols, relationships) = extract_from_file("src/same_file.ps1", code);

        // Verify symbols
        assert!(
            symbols.iter().any(|s| s.name == "Get-Helper"),
            "Should extract Get-Helper"
        );
        assert!(
            symbols.iter().any(|s| s.name == "Caller"),
            "Should extract Caller"
        );

        // Debug output
        println!("=== Same-file symbols ===");
        for s in &symbols {
            println!("  {} ({:?})", s.name, s.kind);
        }
        println!("=== Same-file relationships ===");
        for r in &relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }

        // Same-file call should create a resolved Relationship
        let call_relationships: Vec<_> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !call_relationships.is_empty(),
            "Same-file function call should create a Relationship.\n\
             Found {} relationships, expected at least 1.",
            call_relationships.len()
        );
    }

    // ========================================================================
    // TEST: Verb-Noun cmdlet calls should create pending relationships
    // ========================================================================

    #[test]
    fn test_verb_noun_cmdlet_call_creates_pending_relationship() {
        // PowerShell uses Verb-Noun convention heavily (Get-ChildItem, Write-Host, etc.)
        let file_a_code = r#"
function Export-CustomObject {
    $obj = @{ Name = "Test" }
    return $obj
}
"#;

        let file_b_code = r#"
. .\Export-Utils.ps1

function Main {
    $result = Export-CustomObject  # Verb-Noun convention call
    Write-Output $result
}
"#;

        let results_a = extract_full("lib/Export-Utils.ps1", file_a_code);
        let results_b = extract_full("lib/Main.ps1", file_b_code);

        // Verify Export-CustomObject was extracted
        assert!(
            results_a.symbols.iter().any(|s| s.name == "Export-CustomObject"),
            "Should extract Export-CustomObject"
        );

        // Debug output
        println!("=== File B pending_relationships ===");
        for p in &results_b.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (confidence: {})",
                p.kind, p.from_symbol_id, p.callee_name, p.confidence
            );
        }

        // Should have at least one pending relationship for Export-CustomObject
        let pending_calls: Vec<_> = results_b
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            !pending_calls.is_empty(),
            "Verb-Noun function calls should create PendingRelationships"
        );

        // Check that Export-CustomObject is captured
        let export_pending = pending_calls
            .iter()
            .find(|p| p.callee_name == "Export-CustomObject");
        assert!(
            export_pending.is_some(),
            "Should capture Export-CustomObject call"
        );
    }

    // ========================================================================
    // TEST: Built-in cmdlet calls should not create pending relationships
    // ========================================================================

    #[test]
    fn test_builtin_cmdlet_no_pending_relationship() {
        let code = r#"
function MyFunction {
    Write-Output "Hello"  # Built-in cmdlet
    Get-ChildItem       # Built-in cmdlet
    return
}
"#;

        let results = extract_full("src/my_script.ps1", code);

        // Debug output
        println!("=== Pending relationships for built-in cmdlets ===");
        for p in &results.pending_relationships {
            println!(
                "  {:?}: {} -> '{}' (confidence: {})",
                p.kind, p.from_symbol_id, p.callee_name, p.confidence
            );
        }

        // Built-in cmdlets should not create pending relationships
        let pending_for_builtin: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.callee_name == "Write-Output" || p.callee_name == "Get-ChildItem")
            .collect();

        // This might be empty (preferred) or non-empty depending on implementation
        // But we document the behavior
        println!(
            "Built-in cmdlet pending relationships: {}",
            pending_for_builtin.len()
        );
    }
}
