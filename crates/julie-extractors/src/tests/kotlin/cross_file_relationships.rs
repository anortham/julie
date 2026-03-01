//! Cross-file relationship resolution tests for Kotlin
//!
//! Tests that pending relationships are created when methods/functions are called
//! but not defined in the local file (indicating cross-file resolution needed).

use crate::base::{PendingRelationship, RelationshipKind, SymbolKind};
use crate::kotlin::KotlinExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

/// Initialize Kotlin parser
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
        .expect("Error loading Kotlin grammar");
    parser
}

#[cfg(test)]
mod cross_file_relationships {
    use super::*;

    #[test]
    fn test_cross_file_function_call_creates_pending_relationship() {
        let code = r#"
class Calculator {
    fun calculate(): Int {
        return externalHelper()  // Function not defined locally
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "Calculator.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);

        // Get pending relationships
        let pending = extractor.get_pending_relationships();

        // Should create a pending relationship for externalHelper call
        let pending_call = pending
            .iter()
            .find(|p| p.callee_name == "externalHelper" && p.kind == RelationshipKind::Calls);

        assert!(
            pending_call.is_some(),
            "Should create pending relationship for external function call"
        );

        let pending_call = pending_call.unwrap();
        assert_eq!(pending_call.callee_name, "externalHelper");
        assert_eq!(pending_call.kind, RelationshipKind::Calls);
        assert_eq!(pending_call.line_number, 4);
        assert!(
            pending_call.confidence < 0.9,
            "Pending calls should have lower confidence"
        );
    }

    #[test]
    fn test_local_function_call_creates_resolved_relationship() {
        let code = r#"
class Calculator {
    fun helper(): Int {
        return 42
    }

    fun calculate(): Int {
        return helper()  // Local function call
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "Calculator.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        // Should create a resolved relationship for local helper call
        let resolved_call = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Calls
                && symbols
                    .iter()
                    .find(|s| &s.id == &r.to_symbol_id)
                    .map(|s| s.name.as_str())
                    == Some("helper")
        });

        assert!(
            resolved_call.is_some(),
            "Should create resolved relationship for local function call"
        );

        // Should NOT create a pending relationship for local calls
        let pending_local = pending.iter().find(|p| p.callee_name == "helper");
        assert!(
            pending_local.is_none(),
            "Should not create pending relationship for local function calls"
        );
    }

    #[test]
    fn test_method_call_on_external_type_creates_pending_relationship() {
        let code = r#"
class Service {
    fun process(): String {
        val helper = ExternalHelper()
        return helper.compute()  // Method not defined locally
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "Service.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        // Should create pending relationship for compute method call
        let pending_method = pending
            .iter()
            .find(|p| p.callee_name == "compute" && p.kind == RelationshipKind::Calls);

        assert!(
            pending_method.is_some(),
            "Should create pending relationship for method call on external type"
        );
    }

    #[test]
    fn test_multiple_external_calls_create_multiple_pending_relationships() {
        let code = r#"
class Handler {
    fun handle() {
        init()      // External call 1
        process()   // External call 2
        finish()    // External call 3
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "Handler.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        // Should create pending relationships for all three external calls
        assert!(
            pending.iter().any(|p| p.callee_name == "init"),
            "Should have pending relationship for init() call"
        );
        assert!(
            pending.iter().any(|p| p.callee_name == "process"),
            "Should have pending relationship for process() call"
        );
        assert!(
            pending.iter().any(|p| p.callee_name == "finish"),
            "Should have pending relationship for finish() call"
        );
    }

    #[test]
    fn test_pending_relationships_have_correct_metadata() {
        let code = r#"
class App {
    fun run(): Unit {
        externalFunction()
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "App.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        let pending_rel = pending.iter().find(|p| p.callee_name == "externalFunction");
        assert!(pending_rel.is_some());

        let pending_rel = pending_rel.unwrap();
        assert_eq!(pending_rel.file_path, "App.kt");
        assert_eq!(pending_rel.callee_name, "externalFunction");
        assert_eq!(pending_rel.kind, RelationshipKind::Calls);
        assert!(
            pending_rel.from_symbol_id.len() > 0,
            "Should have from_symbol_id"
        );
    }

    #[test]
    fn test_mixed_local_and_external_calls() {
        let code = r#"
class Processor {
    fun helper(): Int {
        return 10
    }

    fun execute(): Int {
        val local = helper()        // Local call
        val external = compute()    // External call
        return local + external
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "Processor.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        // Should have resolved relationship for local helper call
        assert!(
            relationships.iter().any(|r| {
                r.kind == RelationshipKind::Calls
                    && symbols
                        .iter()
                        .find(|s| &s.id == &r.to_symbol_id)
                        .map(|s| s.name.as_str())
                        == Some("helper")
            }),
            "Should have resolved relationship for local call"
        );

        // Should have pending relationship for external compute call
        assert!(
            pending.iter().any(|p| p.callee_name == "compute"),
            "Should have pending relationship for external call"
        );

        // Should NOT have pending relationship for local helper
        assert!(
            !pending.iter().any(|p| p.callee_name == "helper"),
            "Should not have pending relationship for local call"
        );
    }

    #[test]
    fn test_pending_relationship_with_from_symbol() {
        let code = r#"
class Service {
    fun process() {
        externalFunction()
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "Service.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        let pending_rel = pending.iter().find(|p| p.callee_name == "externalFunction");
        assert!(pending_rel.is_some());

        let pending_rel = pending_rel.unwrap();

        // from_symbol_id should point to the process method
        let from_symbol = symbols
            .iter()
            .find(|s| &s.id == &pending_rel.from_symbol_id);
        assert!(from_symbol.is_some());
        assert_eq!(from_symbol.unwrap().name, "process");
    }

    // ========================================================================
    // Cross-file inheritance tests
    // ========================================================================

    #[test]
    fn test_cross_file_inheritance_creates_pending_relationship() {
        // BaseService is NOT defined in this file
        let code = r#"
class MyService : BaseService() {
    fun doWork() {
        println("working")
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "MyService.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        // Should create a pending relationship for cross-file BaseService
        let pending_inheritance = pending.iter().find(|p| {
            p.callee_name == "BaseService"
                && (p.kind == RelationshipKind::Extends || p.kind == RelationshipKind::Implements)
        });

        assert!(
            pending_inheritance.is_some(),
            "Should create pending relationship for cross-file BaseService.\n\
             Found pending: {:?}",
            pending
                .iter()
                .map(|p| (&p.callee_name, &p.kind))
                .collect::<Vec<_>>()
        );

        // Verify from_symbol_id references MyService
        let my_service = symbols
            .iter()
            .find(|s| s.name == "MyService")
            .expect("Should extract MyService class");
        assert_eq!(pending_inheritance.unwrap().from_symbol_id, my_service.id);
    }

    #[test]
    fn test_same_file_inheritance_still_creates_direct_relationship() {
        // Both class and superclass in the same file
        let code = r#"
open class Animal {
    fun eat() { }
}

class Dog : Animal() {
    fun bark() { }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "Animals.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Should create a direct Extends relationship
        let extends_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Extends)
            .collect();

        assert!(
            !extends_rels.is_empty(),
            "Same-file inheritance should create direct Relationship"
        );

        let dog = symbols
            .iter()
            .find(|s| s.name == "Dog")
            .expect("Should extract Dog");
        let animal = symbols
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

    #[test]
    fn test_cross_file_constructor_supertype_defaults_to_extends() {
        let code = r#"
class Foo : BaseModel() {
    fun work() { }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "Foo.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        let pending_rel = pending
            .iter()
            .find(|p| p.callee_name == "BaseModel")
            .expect("Should create pending relationship for BaseModel");

        assert_eq!(pending_rel.kind, RelationshipKind::Extends);
    }

    #[test]
    fn test_cross_file_interface_supertype_defaults_to_implements_for_classes() {
        let code = r#"
class Foo : IService {
    fun work() { }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "Foo.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        let pending_rel = pending
            .iter()
            .find(|p| p.callee_name == "IService")
            .expect("Should create pending relationship for IService");

        assert_eq!(pending_rel.kind, RelationshipKind::Implements);
    }

    #[test]
    fn test_cross_file_interface_source_unresolved_inheritance_defaults_to_extends() {
        let code = r#"
interface ChildService : ParentService {
    fun work()
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "Service.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        let pending_rel = pending
            .iter()
            .find(|p| p.callee_name == "ParentService")
            .expect("Should create pending relationship for ParentService");

        assert_eq!(pending_rel.kind, RelationshipKind::Extends);
    }

    #[test]
    fn test_cross_file_enum_conformance_creates_pending_relationship() {
        let code = r#"
enum class SyncState : ExternalProtocol {
    IDLE,
    RUNNING
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "SyncState.kt".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        let pending_rel = pending
            .iter()
            .find(|p| p.callee_name == "ExternalProtocol")
            .expect("Should create pending relationship for ExternalProtocol");

        assert_eq!(pending_rel.kind, RelationshipKind::Implements);
    }
}
