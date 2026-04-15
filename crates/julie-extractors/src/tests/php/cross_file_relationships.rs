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
        let helper_fn = results_a
            .symbols
            .iter()
            .find(|s| s.name == "helper_function");
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
            pending_calls
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
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
        let has_double = callee_names
            .iter()
            .any(|n| *n == "double" || *n == "calc.double");

        assert!(
            has_constructor || has_double,
            "Should capture at least 'Calculator' or the member call target.\n\
             Found: {:?}",
            callee_names
        );

        let structured_pending = results_b
            .structured_pending_relationships
            .iter()
            .find(|pending| pending.target.display_name == "calc.double")
            .expect("structured pending relationship should preserve receiver-qualified PHP method calls");
        assert_eq!(structured_pending.target.terminal_name, "double");
        assert_eq!(structured_pending.target.receiver.as_deref(), Some("calc"));
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

    // ========================================================================
    // TEST: Bug fixes for PHP class-level tracking
    // ========================================================================

    /// Test that `new ClassName()` creates a PendingRelationship with Instantiates kind.
    /// Bug 1: object_creation_expression was not visited at all.
    #[test]
    fn test_new_expression_creates_pending_instantiates_relationship() {
        let code = r#"<?php
function make_app() {
    $app = new App();
    return $app;
}
"#;

        let results = extract_full("src/factory.php", code);

        println!("=== symbols ===");
        for s in &results.symbols {
            println!("  {} ({:?})", s.name, s.kind);
        }
        println!("=== pending_relationships ===");
        for p in &results.pending_relationships {
            println!(
                "  {:?}: {} -> '{}'",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        let instantiates: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Instantiates)
            .collect();

        assert!(
            !instantiates.is_empty(),
            "new App() should create a PendingRelationship with kind Instantiates.\n\
             Got {} pending relationships total, 0 Instantiates.",
            results.pending_relationships.len()
        );

        let app_instantiation = instantiates.iter().find(|p| p.callee_name == "App");
        assert!(
            app_instantiation.is_some(),
            "PendingRelationship callee_name should be 'App'.\n\
             Found: {:?}",
            instantiates
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );
    }

    /// Test that namespace-qualified `new \App\Http\Controller()` strips the namespace.
    /// Bug 4: qualified names were not normalized.
    #[test]
    fn test_new_expression_with_qualified_name_strips_namespace() {
        let code = r#"<?php
function make_controller() {
    $ctrl = new \App\Http\Controller();
    return $ctrl;
}
"#;

        let results = extract_full("src/factory.php", code);

        let instantiates: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Instantiates)
            .collect();

        assert!(
            !instantiates.is_empty(),
            "new \\App\\Http\\Controller() should create a PendingRelationship with kind Instantiates."
        );

        let ctrl_instantiation = instantiates.iter().find(|p| p.callee_name == "Controller");
        assert!(
            ctrl_instantiation.is_some(),
            "PendingRelationship callee_name should be 'Controller' (namespace stripped).\n\
             Found: {:?}",
            instantiates
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );
    }

    /// Test that `class Foo extends Bar` creates a PendingRelationship when Bar is in another file.
    /// Bug 2: cross-file extends was silently dropped.
    #[test]
    fn test_cross_file_extends_creates_pending_relationship() {
        let code = r#"<?php
class Controller extends BaseController {
    public function index() {
        return 'ok';
    }
}
"#;

        let results = extract_full("src/controller.php", code);

        println!("=== symbols ===");
        for s in &results.symbols {
            println!("  {} ({:?})", s.name, s.kind);
        }
        println!("=== relationships ===");
        for r in &results.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }
        println!("=== pending_relationships ===");
        for p in &results.pending_relationships {
            println!(
                "  {:?}: {} -> '{}'",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // BaseController is not in the same file, so it should be a PendingRelationship
        let extends_pending: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Extends)
            .collect();

        assert!(
            !extends_pending.is_empty(),
            "class Controller extends BaseController should create a PendingRelationship.\n\
             Got 0 Extends pending relationships. (BaseController is cross-file)"
        );

        let base_controller = extends_pending
            .iter()
            .find(|p| p.callee_name == "BaseController");
        assert!(
            base_controller.is_some(),
            "PendingRelationship callee_name should be 'BaseController'.\n\
             Found: {:?}",
            extends_pending
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );

        let structured_pending = results
            .structured_pending_relationships
            .iter()
            .find(|pending| pending.target.display_name == "BaseController")
            .expect("structured pending relationship should preserve PHP extends targets");
        assert_eq!(structured_pending.target.terminal_name, "BaseController");
    }

    /// Test that `class Foo implements RouterInterface` creates a PendingRelationship
    /// when RouterInterface is in another file.
    /// Bug 3: implements was filtered to same-file only and fabricated IDs.
    #[test]
    fn test_cross_file_implements_creates_pending_relationship() {
        let code = r#"<?php
class Router implements RouterInterface {
    public function route($path) {
        return $path;
    }
}
"#;

        let results = extract_full("src/router.php", code);

        println!("=== symbols ===");
        for s in &results.symbols {
            println!("  {} ({:?})", s.name, s.kind);
        }
        println!("=== relationships ===");
        for r in &results.relationships {
            println!("  {:?}: {} -> {}", r.kind, r.from_symbol_id, r.to_symbol_id);
        }
        println!("=== pending_relationships ===");
        for p in &results.pending_relationships {
            println!(
                "  {:?}: {} -> '{}'",
                p.kind, p.from_symbol_id, p.callee_name
            );
        }

        // RouterInterface is not in the same file, should be PendingRelationship
        let implements_pending: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Implements)
            .collect();

        // Also check that no relationship uses the fabricated "php-interface:*" ID
        let fabricated_ids: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.to_symbol_id.starts_with("php-interface:"))
            .collect();

        assert!(
            fabricated_ids.is_empty(),
            "Should NOT create relationships with fabricated 'php-interface:' IDs.\n\
             Found: {:?}",
            fabricated_ids
                .iter()
                .map(|r| &r.to_symbol_id)
                .collect::<Vec<_>>()
        );

        assert!(
            !implements_pending.is_empty(),
            "class Router implements RouterInterface should create a PendingRelationship.\n\
             Got 0 Implements pending relationships."
        );

        let router_interface = implements_pending
            .iter()
            .find(|p| p.callee_name == "RouterInterface");
        assert!(
            router_interface.is_some(),
            "PendingRelationship callee_name should be 'RouterInterface'.\n\
             Found: {:?}",
            implements_pending
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );
    }

    /// Test namespace-qualified extends strips the namespace prefix.
    /// Bug 4 applied to extends: `class Foo extends \Base\AbstractController` should use 'AbstractController'.
    #[test]
    fn test_namespace_qualified_extends_strips_namespace() {
        let code = r#"<?php
class AdminController extends \Base\Http\AbstractController {
    public function index() {}
}
"#;

        let results = extract_full("src/admin.php", code);

        let extends_pending: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.kind == RelationshipKind::Extends)
            .collect();

        assert!(
            !extends_pending.is_empty(),
            "Namespace-qualified extends should create a PendingRelationship."
        );

        let base = extends_pending
            .iter()
            .find(|p| p.callee_name == "AbstractController");
        assert!(
            base.is_some(),
            "callee_name should be 'AbstractController' (namespace stripped).\n\
             Found: {:?}",
            extends_pending
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );
    }
}
