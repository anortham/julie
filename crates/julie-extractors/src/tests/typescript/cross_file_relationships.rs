//! Cross-File Relationship Extraction Tests for TypeScript
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

    fn init_typescript_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_typescript_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "typescript", &workspace_root)
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
export function helperFunction(x: number): number {
    return x * 2;
}
"#;

        // File B: calls helper_function (imported from file A)
        let file_b_code = r#"
import { helperFunction } from './utils';

export function mainFunction(): number {
    const result = helperFunction(21);  // Cross-file call!
    return result;
}
"#;

        // Extract from both files
        let results_a = extract_full("src/utils.ts", file_a_code);
        let results_b = extract_full("src/main.ts", file_b_code);

        // Verify we extracted the symbols
        let helper_fn = results_a
            .symbols
            .iter()
            .find(|s| s.name == "helperFunction");
        assert!(
            helper_fn.is_some(),
            "Should extract helperFunction from utils.ts"
        );

        let main_fn = results_b.symbols.iter().find(|s| s.name == "mainFunction");
        assert!(
            main_fn.is_some(),
            "Should extract mainFunction from main.ts"
        );

        // Debug output
        println!("=== Main.ts symbols ===");
        for s in &results_b.symbols {
            println!("  {} ({:?}) at line {}", s.name, s.kind, s.start_line);
        }
        println!("=== Main.ts relationships (resolved) ===");
        for r in &results_b.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }
        println!("=== Main.ts pending_relationships ===");
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
            .find(|p| p.callee_name == "helperFunction");

        assert!(
            helper_pending.is_some(),
            "PendingRelationship should have callee_name='helperFunction'.\n\
             Found: {:?}",
            pending_calls
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );

        // Verify the pending relationship has the correct caller
        // Note: TypeScript can have duplicate symbols (Export + Function), so check if caller matches ANY mainFunction
        let main_fn_ids: Vec<_> = results_b
            .symbols
            .iter()
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

        let structured_pending = results_b
            .structured_pending_relationships
            .iter()
            .find(|pending| pending.target.display_name == "helperFunction")
            .expect("structured pending relationship should preserve imported call target");
        assert_eq!(structured_pending.target.terminal_name, "helperFunction");
        assert_eq!(
            structured_pending.target.import_context.as_deref(),
            Some("helperFunction")
        );
    }

    #[test]
    fn test_cross_file_method_call_creates_pending_relationship() {
        // File A: defines a class with methods
        let file_a_code = r#"
export class Calculator {
    constructor(value: number) {
        this.value = value;
    }

    double(): number {
        return this.value * 2;
    }
}
"#;

        // File B: uses Calculator from file A
        let file_b_code = r#"
import { Calculator } from './calculator';

export function process(): number {
    const calc = new Calculator(21);  // Cross-file constructor call
    return calc.double();              // Cross-file method call
}
"#;

        let results_a = extract_full("src/calculator.ts", file_a_code);
        let results_b = extract_full("src/processor.ts", file_b_code);

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
        let has_double = callee_names
            .iter()
            .any(|n| *n == "double" || *n == "calc.double");

        assert!(
            has_constructor || has_double,
            "Should capture at least 'Calculator' or the member call target.\n\
             Found: {:?}",
            callee_names
        );

        let structured_double = results_b
            .structured_pending_relationships
            .iter()
            .find(|pending| pending.target.display_name == "calc.double");
        assert!(
            structured_double.is_some(),
            "Cross-file method calls should retain structured unresolved target context. Got: {:?}",
            results_b
                .structured_pending_relationships
                .iter()
                .map(|pending| pending.target.display_name.as_str())
                .collect::<Vec<_>>()
        );

        let structured_double = structured_double.unwrap();
        assert_eq!(structured_double.target.terminal_name, "double");
        assert_eq!(structured_double.target.receiver.as_deref(), Some("calc"));
        assert_eq!(
            structured_double.target.import_context.as_deref(),
            Some("Calculator")
        );
    }

    // ========================================================================
    // TEST: Same-file calls should still work (regression test)
    // ========================================================================

    #[test]
    fn test_same_file_function_call_creates_relationship() {
        // Both functions in the same file - this should work with resolved Relationship
        let code = r#"
function helper(x: number): number {
    return x * 2;
}

function caller(): number {
    return helper(21);  // Same-file call
}
"#;

        let (symbols, relationships) = extract_from_file("src/same_file.ts", code);

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

        assert!(
            has_correct_rel,
            "Should have relationship from caller to helper"
        );
    }

    // ========================================================================
    // TEST: Cross-file extends should create PendingRelationship
    // ========================================================================

    #[test]
    fn test_cross_file_extends_creates_pending_relationship() {
        // BaseComponent is NOT defined in this file
        let code = r#"
class MyComponent extends Namespace.BaseComponent {
    render() {
        return "hello";
    }
}
"#;

        let results = extract_full("src/my-component.ts", code);

        let pending_extends: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Extends)
            .collect();

        let base_pending = pending_extends
            .iter()
            .find(|p| p.callee_name == "BaseComponent");
        assert!(
            base_pending.is_some(),
            "Should create PendingRelationship(Extends) for cross-file Namespace.BaseComponent using terminal name BaseComponent.\n\
             Found pending: {:?}",
            pending_extends
                .iter()
                .map(|p| (&p.callee_name, &p.kind))
                .collect::<Vec<_>>()
        );

        let my_component = results
            .symbols
            .iter()
            .find(|s| s.name == "MyComponent")
            .expect("Should extract MyComponent class");
        assert_eq!(base_pending.unwrap().from_symbol_id, my_component.id);
    }

    #[test]
    fn test_cross_file_implements_creates_pending_relationship() {
        // IService is NOT defined in this file.
        // Use qualified name to ensure we keep only the terminal identifier.
        let code = r#"
class ServiceImpl implements Api.IService<Response> {
    serve() { }
}
"#;

        let results = extract_full("src/service-impl.ts", code);

        let pending_implements: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Implements)
            .collect();

        let iface_pending = pending_implements
            .iter()
            .find(|p| p.callee_name == "IService");
        assert!(
            iface_pending.is_some(),
            "Should create PendingRelationship(Implements) with terminal name IService for Api.IService. Found pending: {:?}",
            results
                .pending_relationships
                .iter()
                .map(|p| (&p.callee_name, &p.kind))
                .collect::<Vec<_>>()
        );

        let response_pending = pending_implements
            .iter()
            .find(|p| p.callee_name == "Response");
        assert!(
            response_pending.is_none(),
            "Should not extract type argument Response from Api.IService<Response>"
        );
    }

    #[test]
    fn test_cross_file_extends_with_generic_targets_base_only() {
        let code = r#"
class FancyWidget extends Base<T> {
    render() {
        return "ok";
    }
}
"#;

        let results = extract_full("src/fancy-widget.ts", code);

        let pending_extends: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Extends)
            .collect();

        assert!(
            pending_extends.iter().any(|p| p.callee_name == "Base"),
            "Should create pending extends relationship targeting Base"
        );
        assert!(
            pending_extends.iter().all(|p| p.callee_name != "T"),
            "Should not extract generic type argument T from Base<T>"
        );
    }

    #[test]
    fn test_same_file_extends_still_creates_direct_relationship() {
        // Both class and superclass in the same file
        let code = r#"
class Animal {
    eat() { }
}

class Dog extends Animal {
    bark() { }
}
"#;

        let results = extract_full("src/animals.ts", code);

        let extends_rels: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Extends)
            .collect();

        assert!(
            !extends_rels.is_empty(),
            "Same-file extends should create direct Relationship.\n\
             Found {} extends relationships",
            extends_rels.len()
        );

        let dog = results
            .symbols
            .iter()
            .find(|s| s.name == "Dog")
            .expect("Should extract Dog");
        let animal = results
            .symbols
            .iter()
            .find(|s| s.name == "Animal")
            .expect("Should extract Animal");

        let has_correct_rel = extends_rels
            .iter()
            .any(|r| r.from_symbol_id == dog.id && r.to_symbol_id == animal.id);
        assert!(
            has_correct_rel,
            "Should have Extends relationship from Dog to Animal"
        );
    }
}
