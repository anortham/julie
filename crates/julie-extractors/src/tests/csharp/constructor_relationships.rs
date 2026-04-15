//! Constructor Parameter Relationship Tests for C#
//!
//! Tests that DI (Dependency Injection) constructor parameter types are extracted
//! as `Uses` relationships. In C#/.NET, constructor injection is THE primary
//! wiring mechanism — nearly every service class takes its dependencies as
//! constructor parameters. Without these relationships, Julie misses the most
//! important connections in a C# codebase.

use crate::ExtractionResults;
use crate::base::{RelationshipKind, SymbolKind};
use crate::factory::extract_symbols_and_relationships;
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

    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_csharp_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "csharp", &workspace_root)
            .expect("Failed to extract")
    }

    // ========================================================================
    // TEST: Basic DI constructor injection creates Uses relationships
    // ========================================================================

    #[test]
    fn test_csharp_constructor_di_creates_uses_relationships() {
        let code = r#"
public interface ILogger { }
public interface IUserRepository { }

public class UserService {
    public UserService(ILogger logger, IUserRepository repo) {
    }
}
"#;

        let results = extract_full("src/UserService.cs", code);

        // Find the class symbol
        let user_service = results
            .symbols
            .iter()
            .find(|s| s.name == "UserService" && s.kind == SymbolKind::Class)
            .expect("Should find UserService class");

        // Find Uses relationships from UserService
        let uses_rels: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.from_symbol_id == user_service.id && r.kind == RelationshipKind::Uses)
            .collect();

        // Should have Uses relationships to ILogger and IUserRepository
        assert!(
            uses_rels.len() >= 2,
            "UserService should have Uses relationships to ILogger and IUserRepository.\n\
             Found {} Uses relationships: {:?}",
            uses_rels.len(),
            uses_rels
                .iter()
                .map(|r| &r.to_symbol_id)
                .collect::<Vec<_>>()
        );

        // Verify targets are the correct interfaces
        let ilogger = results
            .symbols
            .iter()
            .find(|s| s.name == "ILogger")
            .expect("Should find ILogger");
        let irepo = results
            .symbols
            .iter()
            .find(|s| s.name == "IUserRepository")
            .expect("Should find IUserRepository");

        assert!(
            uses_rels.iter().any(|r| r.to_symbol_id == ilogger.id),
            "Should have Uses relationship to ILogger"
        );
        assert!(
            uses_rels.iter().any(|r| r.to_symbol_id == irepo.id),
            "Should have Uses relationship to IUserRepository"
        );
    }

    // ========================================================================
    // TEST: Generic constructor parameter extracts base type name
    // ========================================================================

    #[test]
    fn test_csharp_generic_constructor_parameter_extracts_base_type() {
        let code = r#"
public interface ILogger { }

public class MyService {
    public MyService(ILogger<MyService> logger) {
    }
}
"#;

        let results = extract_full("src/MyService.cs", code);

        let my_service = results
            .symbols
            .iter()
            .find(|s| s.name == "MyService" && s.kind == SymbolKind::Class)
            .expect("Should find MyService class");

        let ilogger = results
            .symbols
            .iter()
            .find(|s| s.name == "ILogger")
            .expect("Should find ILogger");

        // Should have Uses relationship from MyService to ILogger (base name, not generic)
        let has_ilogger_rel = results.relationships.iter().any(|r| {
            r.from_symbol_id == my_service.id
                && r.to_symbol_id == ilogger.id
                && r.kind == RelationshipKind::Uses
        });

        assert!(
            has_ilogger_rel,
            "MyService should have Uses relationship to ILogger (extracted from ILogger<MyService>).\n\
             All relationships: {:?}",
            results
                .relationships
                .iter()
                .map(|r| format!("{} --{:?}--> {}", r.from_symbol_id, r.kind, r.to_symbol_id))
                .collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // TEST: Constructor skips predefined types (string, int, bool, etc.)
    // ========================================================================

    #[test]
    fn test_csharp_constructor_skips_predefined_types() {
        let code = r#"
public interface IRepo { }

public class MyService {
    public MyService(string name, int count, IRepo repo) {
    }
}
"#;

        let results = extract_full("src/MyService.cs", code);

        let my_service = results
            .symbols
            .iter()
            .find(|s| s.name == "MyService" && s.kind == SymbolKind::Class)
            .expect("Should find MyService class");

        // Should only have relationship to IRepo, not to string or int
        let uses_rels: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.from_symbol_id == my_service.id && r.kind == RelationshipKind::Uses)
            .collect();

        let irepo = results
            .symbols
            .iter()
            .find(|s| s.name == "IRepo")
            .expect("Should find IRepo");

        assert_eq!(
            uses_rels.len(),
            1,
            "Should have exactly 1 Uses relationship (to IRepo), not to string or int.\n\
             Found: {:?}",
            uses_rels
        );

        assert_eq!(
            uses_rels[0].to_symbol_id, irepo.id,
            "The single Uses relationship should point to IRepo"
        );
    }

    // ========================================================================
    // TEST: Constructor with nullable types
    // ========================================================================

    #[test]
    fn test_csharp_constructor_nullable_type_parameter() {
        let code = r#"
public interface ICache { }

public class MyService {
    public MyService(ICache? cache) {
    }
}
"#;

        let results = extract_full("src/MyService.cs", code);

        let my_service = results
            .symbols
            .iter()
            .find(|s| s.name == "MyService" && s.kind == SymbolKind::Class)
            .expect("Should find MyService class");

        let icache = results
            .symbols
            .iter()
            .find(|s| s.name == "ICache")
            .expect("Should find ICache");

        let has_cache_rel = results.relationships.iter().any(|r| {
            r.from_symbol_id == my_service.id
                && r.to_symbol_id == icache.id
                && r.kind == RelationshipKind::Uses
        });

        assert!(
            has_cache_rel,
            "MyService should have Uses relationship to ICache (from nullable ICache? parameter)"
        );
    }

    // ========================================================================
    // TEST: Constructor with cross-file types creates PendingRelationship
    // ========================================================================

    #[test]
    fn test_csharp_constructor_cross_file_type_creates_pending() {
        // Types not defined in this file should create PendingRelationship
        let code = r#"
using MyApp.Services;

public class OrderController {
    public OrderController(IOrderService orderService, IPaymentGateway gateway) {
    }
}
"#;

        let results = extract_full("src/OrderController.cs", code);

        let controller = results
            .symbols
            .iter()
            .find(|s| s.name == "OrderController" && s.kind == SymbolKind::Class)
            .expect("Should find OrderController class");

        // Since IOrderService and IPaymentGateway are not defined in this file,
        // they should create PendingRelationships
        let pending_uses: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.from_symbol_id == controller.id && p.kind == RelationshipKind::Uses)
            .collect();

        assert!(
            pending_uses.len() >= 2,
            "Should create PendingRelationships for cross-file constructor parameter types.\n\
             Found {} pending Uses: {:?}",
            pending_uses.len(),
            pending_uses
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );

        assert!(
            pending_uses
                .iter()
                .any(|p| p.callee_name == "IOrderService"),
            "Should have pending Uses for IOrderService"
        );
        assert!(
            pending_uses
                .iter()
                .any(|p| p.callee_name == "IPaymentGateway"),
            "Should have pending Uses for IPaymentGateway"
        );

        let structured_pending_uses: Vec<_> = results
            .structured_pending_relationships
            .iter()
            .filter(|p| {
                p.pending.from_symbol_id == controller.id
                    && p.pending.kind == RelationshipKind::Uses
            })
            .collect();

        assert!(
            structured_pending_uses
                .iter()
                .any(|p| p.target.display_name == "IOrderService"),
            "Should preserve structured pending Uses for IOrderService"
        );
        assert!(
            structured_pending_uses
                .iter()
                .any(|p| p.target.display_name == "IPaymentGateway"),
            "Should preserve structured pending Uses for IPaymentGateway"
        );
    }

    #[test]
    fn test_csharp_primary_constructor_creates_uses_relationships() {
        let code = r#"
public interface ILogger { }
public interface IClock { }

public class Worker(ILogger logger, IClock clock) {
}
"#;

        let results = extract_full("src/Worker.cs", code);

        let worker = results
            .symbols
            .iter()
            .find(|s| s.name == "Worker" && s.kind == SymbolKind::Class)
            .expect("Should find Worker class");
        let ilogger = results
            .symbols
            .iter()
            .find(|s| s.name == "ILogger")
            .expect("Should find ILogger");
        let iclock = results
            .symbols
            .iter()
            .find(|s| s.name == "IClock")
            .expect("Should find IClock");

        let uses_targets: std::collections::HashSet<_> = results
            .relationships
            .iter()
            .filter(|relationship| {
                relationship.from_symbol_id == worker.id
                    && relationship.kind == RelationshipKind::Uses
            })
            .map(|relationship| relationship.to_symbol_id.as_str())
            .collect();

        assert_eq!(uses_targets.len(), 2);
        assert!(uses_targets.contains(ilogger.id.as_str()));
        assert!(uses_targets.contains(iclock.id.as_str()));
    }

    #[test]
    fn test_csharp_record_primary_constructor_creates_uses_relationships() {
        let code = r#"
public interface ILogger { }
public interface IClock { }

public record WorkerRecord(ILogger logger, IClock clock);
"#;

        let results = extract_full("src/WorkerRecord.cs", code);

        let worker = results
            .symbols
            .iter()
            .find(|s| s.name == "WorkerRecord" && s.kind == SymbolKind::Class)
            .expect("Should find WorkerRecord class");
        let ilogger = results
            .symbols
            .iter()
            .find(|s| s.name == "ILogger")
            .expect("Should find ILogger");
        let iclock = results
            .symbols
            .iter()
            .find(|s| s.name == "IClock")
            .expect("Should find IClock");

        let uses_targets: std::collections::HashSet<_> = results
            .relationships
            .iter()
            .filter(|relationship| {
                relationship.from_symbol_id == worker.id
                    && relationship.kind == RelationshipKind::Uses
            })
            .map(|relationship| relationship.to_symbol_id.as_str())
            .collect();

        assert_eq!(uses_targets.len(), 2);
        assert!(uses_targets.contains(ilogger.id.as_str()));
        assert!(uses_targets.contains(iclock.id.as_str()));
    }

    // ========================================================================
    // TEST: Multiple constructors in same class
    // ========================================================================

    #[test]
    fn test_csharp_multiple_constructors() {
        let code = r#"
public interface ILogger { }
public interface IRepo { }

public class MyService {
    public MyService(ILogger logger) {
    }

    public MyService(ILogger logger, IRepo repo) {
    }
}
"#;

        let results = extract_full("src/MyService.cs", code);

        let my_service = results
            .symbols
            .iter()
            .find(|s| s.name == "MyService" && s.kind == SymbolKind::Class)
            .expect("Should find MyService class");

        let uses_rels: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.from_symbol_id == my_service.id && r.kind == RelationshipKind::Uses)
            .collect();

        let ilogger = results
            .symbols
            .iter()
            .find(|s| s.name == "ILogger")
            .expect("Should find ILogger");
        let irepo = results
            .symbols
            .iter()
            .find(|s| s.name == "IRepo")
            .expect("Should find IRepo");

        // Should have exactly 2 Uses relationships (deduplicated across constructors):
        // MyService->ILogger and MyService->IRepo. NOT 3 (ILogger duplicated from both ctors).
        assert_eq!(
            uses_rels.len(),
            2,
            "Should have exactly 2 deduplicated Uses relationships (ILogger + IRepo), not 3.\n\
             ILogger appears in both constructors but should only create one relationship.\n\
             Found: {:?}",
            uses_rels
                .iter()
                .map(|r| &r.to_symbol_id)
                .collect::<Vec<_>>()
        );

        assert!(
            uses_rels.iter().any(|r| r.to_symbol_id == ilogger.id),
            "Should have Uses relationship to ILogger"
        );
        assert!(
            uses_rels.iter().any(|r| r.to_symbol_id == irepo.id),
            "Should have Uses relationship to IRepo"
        );
    }

    // ========================================================================
    // TEST: Constructor skips tuple types
    // ========================================================================

    #[test]
    fn test_csharp_constructor_skips_tuple_types() {
        let code = r#"
public interface IRepo { }

public class MyService {
    public MyService((string, int) coords, IRepo repo) {
    }
}
"#;

        let results = extract_full("src/MyService.cs", code);

        let my_service = results
            .symbols
            .iter()
            .find(|s| s.name == "MyService" && s.kind == SymbolKind::Class)
            .expect("Should find MyService class");

        // Should only have relationship to IRepo, not to tuple type "(string, int)"
        let uses_rels: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.from_symbol_id == my_service.id && r.kind == RelationshipKind::Uses)
            .collect();

        let irepo = results
            .symbols
            .iter()
            .find(|s| s.name == "IRepo")
            .expect("Should find IRepo");

        assert_eq!(
            uses_rels.len(),
            1,
            "Should have exactly 1 Uses relationship (to IRepo), tuple types should be skipped.\n\
             Found: {:?}",
            uses_rels
                .iter()
                .map(|r| &r.to_symbol_id)
                .collect::<Vec<_>>()
        );

        assert_eq!(
            uses_rels[0].to_symbol_id, irepo.id,
            "The single Uses relationship should point to IRepo"
        );

        // Also verify no pending relationship was created for the tuple type
        let pending_uses: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.from_symbol_id == my_service.id && p.kind == RelationshipKind::Uses)
            .collect();

        assert!(
            pending_uses.is_empty(),
            "Should NOT create PendingRelationship for tuple type parameter.\n\
             Found pending Uses: {:?}",
            pending_uses
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // TEST: Confidence level should be 0.9
    // ========================================================================

    #[test]
    fn test_csharp_constructor_relationship_confidence() {
        let code = r#"
public interface ILogger { }

public class MyService {
    public MyService(ILogger logger) {
    }
}
"#;

        let results = extract_full("src/MyService.cs", code);

        let my_service = results
            .symbols
            .iter()
            .find(|s| s.name == "MyService" && s.kind == SymbolKind::Class)
            .expect("Should find MyService class");

        let uses_rel = results
            .relationships
            .iter()
            .find(|r| r.from_symbol_id == my_service.id && r.kind == RelationshipKind::Uses)
            .expect("Should have Uses relationship");

        assert!(
            (uses_rel.confidence - 0.9).abs() < f32::EPSILON,
            "Constructor parameter Uses relationship should have confidence 0.9, got {}",
            uses_rel.confidence
        );
    }
}
