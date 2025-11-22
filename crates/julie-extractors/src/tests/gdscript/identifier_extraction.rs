// GDScript Identifier Extraction Tests (TDD RED phase)
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Function calls (call, get_node)
// - Member access (attribute, subscript)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

#![allow(unused_imports)]

use crate::base::{IdentifierKind, SymbolKind};
use crate::gdscript::GDScriptExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[cfg(test)]
mod identifier_extraction_tests {
    use super::*;

    #[test]
    fn test_gdscript_function_calls() {
        let gdscript_code = r#"
extends Node

func process_data():
    var result = calculate(5, 3)  # Function call to calculate
    print(result)                  # Function call to print
    return result

func calculate(a, b):
    return a + b
"#;

        let tree = init_parser(gdscript_code, "gdscript");
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = GDScriptExtractor::new(
            "gdscript".to_string(),
            "test.gd".to_string(),
            gdscript_code.to_string(),
            &workspace_root,
        );

        // Extract symbols first
        let symbols = extractor.extract_symbols(&tree);

        // NOW extract identifiers (this will FAIL until we implement it)
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found the function calls
        let calculate_call = identifiers.iter().find(|id| id.name == "calculate");
        assert!(
            calculate_call.is_some(),
            "Should extract 'calculate' function call identifier"
        );
        let calculate_call = calculate_call.unwrap();
        assert_eq!(calculate_call.kind, IdentifierKind::Call);

        let print_call = identifiers.iter().find(|id| id.name == "print");
        assert!(
            print_call.is_some(),
            "Should extract 'print' function call identifier"
        );
        let print_call = print_call.unwrap();
        assert_eq!(print_call.kind, IdentifierKind::Call);

        // Verify containing symbol is set correctly (should be inside process_data function)
        assert!(
            calculate_call.containing_symbol_id.is_some(),
            "Function call should have containing symbol"
        );

        // Find the process_data function symbol
        let process_data_func = symbols.iter().find(|s| s.name == "process_data").unwrap();

        // Verify the calculate call is contained within process_data function
        assert_eq!(
            calculate_call.containing_symbol_id.as_ref(),
            Some(&process_data_func.id),
            "calculate call should be contained within process_data function"
        );
    }

    #[test]
    fn test_gdscript_member_access() {
        let gdscript_code = r#"
extends Node

var player

func _ready():
    var pos = player.position    # Member access: player.position
    var health = player.health    # Member access: player.health
"#;

        let tree = init_parser(gdscript_code, "gdscript");
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = GDScriptExtractor::new(
            "gdscript".to_string(),
            "test.gd".to_string(),
            gdscript_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found member access identifiers
        let position_access = identifiers
            .iter()
            .filter(|id| id.name == "position" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            position_access > 0,
            "Should extract 'position' member access identifier"
        );

        let health_access = identifiers
            .iter()
            .filter(|id| id.name == "health" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            health_access > 0,
            "Should extract 'health' member access identifier"
        );
    }

    #[test]
    fn test_gdscript_identifiers_have_containing_symbol() {
        // This test ensures we ONLY match symbols from the SAME FILE
        // Critical bug fix from Rust implementation (line 1311-1318 in rust.rs)
        let gdscript_code = r#"
extends Node

func start():
    helper()              # Call to helper in same file

func helper():
    pass
"#;

        let tree = init_parser(gdscript_code, "gdscript");
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = GDScriptExtractor::new(
            "gdscript".to_string(),
            "test.gd".to_string(),
            gdscript_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Find the helper call
        let helper_call = identifiers.iter().find(|id| id.name == "helper");
        assert!(helper_call.is_some());
        let helper_call = helper_call.unwrap();

        // Verify it has a containing symbol (the start function)
        assert!(
            helper_call.containing_symbol_id.is_some(),
            "helper call should have containing symbol from same file"
        );

        // Verify the containing symbol is the start function
        let start_func = symbols.iter().find(|s| s.name == "start").unwrap();
        assert_eq!(
            helper_call.containing_symbol_id.as_ref(),
            Some(&start_func.id),
            "helper call should be contained within start function"
        );
    }

    #[test]
    fn test_gdscript_chained_member_access() {
        let gdscript_code = r#"
extends Node

func get_data():
    var balance = user.account.balance   # Chained member access
    var name = customer.profile.name      # Chained member access
"#;

        let tree = init_parser(gdscript_code, "gdscript");
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = GDScriptExtractor::new(
            "gdscript".to_string(),
            "test.gd".to_string(),
            gdscript_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract the rightmost identifiers in chains
        let balance_access = identifiers
            .iter()
            .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            balance_access.is_some(),
            "Should extract 'balance' from chained member access"
        );

        let name_access = identifiers
            .iter()
            .find(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            name_access.is_some(),
            "Should extract 'name' from chained member access"
        );
    }

    #[test]
    fn test_gdscript_duplicate_calls_at_different_locations() {
        let gdscript_code = r#"
extends Node

func run():
    process()
    process()  # Same call twice

func process():
    pass
"#;

        let tree = init_parser(gdscript_code, "gdscript");
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = GDScriptExtractor::new(
            "gdscript".to_string(),
            "test.gd".to_string(),
            gdscript_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract BOTH calls (they're at different locations)
        let process_calls: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "process" && id.kind == IdentifierKind::Call)
            .collect();

        assert_eq!(
            process_calls.len(),
            2,
            "Should extract both process calls at different locations"
        );

        // Verify they have different line numbers
        assert_ne!(
            process_calls[0].start_line, process_calls[1].start_line,
            "Duplicate calls should have different line numbers"
        );
    }
}
