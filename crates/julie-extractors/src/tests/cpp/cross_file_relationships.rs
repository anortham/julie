//! Cross-File Relationship Extraction Tests for C++
//!
//! These tests verify that function calls across file boundaries are correctly
//! captured as PendingRelationships. This is critical for trace_call_path to work.
//!
//! Architecture:
//! - Same-file calls → Relationship (directly resolved)
//! - Cross-file calls → PendingRelationship (resolved after workspace indexing)

use crate::base::RelationshipKind;
use crate::cpp::CppExtractor;
use crate::{ExtractionResults, Relationship, Symbol};
use std::collections::HashMap;
use std::path::PathBuf;
use tree_sitter::Parser;

#[cfg(test)]
mod tests {
    use super::*;

    fn init_cpp_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .expect("Error loading C++ grammar");
        parser
    }

    /// Helper to extract full results from code with a specific filename
    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_cpp_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");
        let mut extractor =
            CppExtractor::new(filename.to_string(), code.to_string(), &workspace_root);
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);
        let pending_relationships = extractor.get_pending_relationships();
        let structured_pending_relationships = extractor.get_structured_pending_relationships();

        ExtractionResults {
            symbols,
            relationships,
            pending_relationships,
            structured_pending_relationships,
            types: HashMap::new(),
            identifiers,
            parse_diagnostics: Vec::new(),
        }
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
int helper_function(int x) {
    return x * 2;
}
"#;

        // File B: calls helper_function (imported from file A via header)
        let file_b_code = r#"
#include "utils.h"

int main_function() {
    int result = helper_function(21);  // Cross-file call!
    return result;
}
"#;

        // Extract from both files
        let results_a = extract_full("src/utils.cpp", file_a_code);
        let results_b = extract_full("src/main.cpp", file_b_code);

        // Verify we extracted the symbols
        let helper_fn = results_a
            .symbols
            .iter()
            .find(|s| s.name == "helper_function");
        assert!(
            helper_fn.is_some(),
            "Should extract helper_function from utils.cpp"
        );

        let main_fn = results_b.symbols.iter().find(|s| s.name == "main_function");
        assert!(
            main_fn.is_some(),
            "Should extract main_function from main.cpp"
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
        // (because pointing to non-local symbol is not useful)
        let call_relationships: Vec<_> = results_b
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        // Cross-file calls might not resolve, but we should have pending ones
        println!(
            "Found {} resolved call relationships",
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

        let structured_pending = results_b
            .structured_pending_relationships
            .iter()
            .find(|pending| pending.target.display_name == "helper_function")
            .expect(
                "structured pending relationship should preserve the unresolved C++ call target",
            );
        assert_eq!(structured_pending.target.terminal_name, "helper_function");
        assert_eq!(structured_pending.target.receiver, None);
    }

    #[test]
    fn test_same_file_function_call_creates_relationship() {
        // Both functions in the same file - this should work with resolved Relationship
        let code = r#"
int helper(int x) {
    return x * 2;
}

int caller() {
    return helper(21);  // Same-file call
}
"#;

        let (symbols, relationships) = extract_from_file("src/same_file.cpp", code);

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

    #[test]
    fn test_receiver_qualified_call_does_not_resolve_to_unrelated_local_function() {
        let code = r#"
struct Widget {};

int helper() {
    return 7;
}

int caller(Widget widget) {
    return widget.helper();
}
"#;

        let results = extract_full("src/receiver_qualified.cpp", code);
        let helper = results
            .symbols
            .iter()
            .find(|symbol| symbol.name == "helper")
            .expect("helper symbol should be extracted");
        let caller = results
            .symbols
            .iter()
            .find(|symbol| symbol.name == "caller")
            .expect("caller symbol should be extracted");

        let wrong_local_resolution = results.relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::Calls
                && relationship.from_symbol_id == caller.id
                && relationship.to_symbol_id == helper.id
        });
        assert!(
            !wrong_local_resolution,
            "Receiver-qualified calls should not resolve to unrelated local functions by terminal name"
        );

        let structured_pending = results
            .structured_pending_relationships
            .iter()
            .find(|pending| {
                pending.pending.kind == RelationshipKind::Calls
                    && pending.target.terminal_name == "helper"
                    && pending.target.receiver.as_deref() == Some("widget")
            })
            .expect(
                "receiver-qualified unresolved call should produce structured pending relationship",
            );
        assert_eq!(structured_pending.pending.from_symbol_id, caller.id);
    }

    #[test]
    fn test_cpp_duplicate_method_names_keep_this_call_relationships() {
        let code = r#"
class Alpha {
public:
    void helper() {}
    void run() { this->helper(); }
};

class Beta {
public:
    void helper() {}
    void run() { this->helper(); }
};
"#;

        let results = extract_full("src/duplicate_methods.cpp", code);

        let run_methods: Vec<_> = results
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "run")
            .collect();
        let helper_methods: Vec<_> = results
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "helper")
            .collect();
        assert_eq!(run_methods.len(), 2);
        assert_eq!(helper_methods.len(), 2);

        for run in run_methods {
            let expected_helper = helper_methods
                .iter()
                .find(|helper| helper.parent_id == run.parent_id)
                .expect("each run method should have a helper in the same class");

            assert!(
                results.relationships.iter().any(|relationship| {
                    relationship.kind == RelationshipKind::Calls
                        && relationship.from_symbol_id == run.id
                        && relationship.to_symbol_id == expected_helper.id
                }),
                "expected a call relationship from run {:?} to same-class helper {:?}; relationships: {:?}",
                run,
                expected_helper,
                results.relationships
            );
        }
    }

    #[test]
    fn test_cpp_overloaded_functions_keep_call_relationships() {
        let code = r#"
int helper() {
    return 1;
}

int caller(int value) {
    return helper() + value;
}

int caller(double value) {
    return helper() + static_cast<int>(value);
}
"#;

        let results = extract_full("src/overloaded_functions.cpp", code);
        let helper = results
            .symbols
            .iter()
            .find(|symbol| symbol.name == "helper")
            .expect("helper should be extracted");
        let callers: Vec<_> = results
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "caller")
            .collect();
        assert_eq!(callers.len(), 2);

        for caller in callers {
            assert!(
                results.relationships.iter().any(|relationship| {
                    relationship.kind == RelationshipKind::Calls
                        && relationship.from_symbol_id == caller.id
                        && relationship.to_symbol_id == helper.id
                }),
                "expected overloaded caller {:?} to call helper {:?}; relationships: {:?}",
                caller,
                helper,
                results.relationships
            );
        }
    }

    #[test]
    fn test_cpp_constructor_name_collision_does_not_drop_inheritance() {
        let code = r#"
class Base {
public:
    Base() {}
};

class Derived : public Base {
public:
    Derived() {}
};
"#;

        let results = extract_full("src/constructor_collision.cpp", code);

        let base_class = results
            .symbols
            .iter()
            .find(|symbol| symbol.name == "Base" && symbol.kind == crate::base::SymbolKind::Class)
            .expect("Base class should be extracted");
        let derived_class = results
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == "Derived" && symbol.kind == crate::base::SymbolKind::Class
            })
            .expect("Derived class should be extracted");

        assert!(
            results.symbols.iter().any(|symbol| {
                symbol.name == "Base" && symbol.kind == crate::base::SymbolKind::Constructor
            }),
            "constructor should make Base non-unique by name"
        );
        assert!(
            results.relationships.iter().any(|relationship| {
                relationship.kind == RelationshipKind::Extends
                    && relationship.from_symbol_id == derived_class.id
                    && relationship.to_symbol_id == base_class.id
            }),
            "expected Derived to extend Base despite constructor name collision; relationships: {:?}",
            results.relationships
        );
    }

    #[test]
    fn test_cpp_inheritance_prefers_base_type_in_same_namespace_when_names_are_duplicated() {
        let code = r#"
namespace B {
class Base {};
}

namespace A {
class Base {};
class Derived : public Base {};
}
"#;

        let results = extract_full("src/namespaced_inheritance.cpp", code);
        let namespace_a = results
            .symbols
            .iter()
            .find(|symbol| symbol.name == "A" && symbol.kind == crate::base::SymbolKind::Namespace)
            .expect("namespace A should be extracted");
        let namespace_b = results
            .symbols
            .iter()
            .find(|symbol| symbol.name == "B" && symbol.kind == crate::base::SymbolKind::Namespace)
            .expect("namespace B should be extracted");
        let a_base = results
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == "Base"
                    && symbol.kind == crate::base::SymbolKind::Class
                    && symbol.parent_id.as_deref() == Some(namespace_a.id.as_str())
            })
            .expect("A::Base should be extracted");
        let b_base = results
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == "Base"
                    && symbol.kind == crate::base::SymbolKind::Class
                    && symbol.parent_id.as_deref() == Some(namespace_b.id.as_str())
            })
            .expect("B::Base should be extracted");
        let derived = results
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == "Derived"
                    && symbol.kind == crate::base::SymbolKind::Class
                    && symbol.parent_id.as_deref() == Some(namespace_a.id.as_str())
            })
            .expect("A::Derived should be extracted");

        assert!(
            results.relationships.iter().any(|relationship| {
                relationship.kind == RelationshipKind::Extends
                    && relationship.from_symbol_id == derived.id
                    && relationship.to_symbol_id == a_base.id
            }),
            "expected A::Derived to extend A::Base; relationships: {:?}",
            results.relationships
        );
        assert!(
            !results.relationships.iter().any(|relationship| {
                relationship.kind == RelationshipKind::Extends
                    && relationship.from_symbol_id == derived.id
                    && relationship.to_symbol_id == b_base.id
            }),
            "A::Derived should not extend B::Base"
        );
    }
}
