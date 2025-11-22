//! Cross-File Relationship Extraction Tests for C#
//!
//! These tests verify that method calls across file boundaries are correctly
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

    fn init_csharp_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .expect("Error loading C# grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_csharp_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "csharp", &workspace_root)
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
        // File A: defines Helper class with Process method
        let file_a_code = r#"
namespace Utils
{
    public class Helper
    {
        public static int Process(int x)
        {
            return x * 2;
        }
    }
}
"#;

        // File B: calls Helper.Process (imported from file A)
        let file_b_code = r#"
using Utils;

namespace Main
{
    public class Service
    {
        public int Main()
        {
            int result = Helper.Process(21);  // Cross-file call!
            return result;
        }
    }
}
"#;

        // Extract from both files
        let results_a = extract_full("src/Utils.cs", file_a_code);
        let results_b = extract_full("src/Main.cs", file_b_code);

        // Verify we extracted the symbols
        let process_method = results_a.symbols.iter().find(|s| s.name == "Process");
        assert!(
            process_method.is_some(),
            "Should extract Process method from Utils.cs"
        );

        let main_method = results_b.symbols.iter().find(|s| s.name == "Main");
        assert!(
            main_method.is_some(),
            "Should extract Main method from Main.cs"
        );


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
        let process_pending = pending_calls.iter().find(|p| p.callee_name == "Process");

        assert!(
            process_pending.is_some(),
            "PendingRelationship should have callee_name='Process'.\n\
             Found: {:?}",
            pending_calls.iter().map(|p| &p.callee_name).collect::<Vec<_>>()
        );

        // Verify the pending relationship exists
        let pending = process_pending.unwrap();
        assert!(
            !pending.from_symbol_id.is_empty(),
            "PendingRelationship should have a valid from_symbol_id"
        );
    }
}
