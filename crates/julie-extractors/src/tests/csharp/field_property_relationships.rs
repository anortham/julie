//! Field and Property Type Relationship Tests for C#
//!
//! Tests that field type declarations (`private ILogger _logger;`) and property
//! type declarations (`public ILogger Logger { get; set; }`) create `Uses`
//! relationships. In C# codebases, fields and properties are the primary way
//! classes declare their dependencies beyond constructor parameters.
//!
//! Without these relationships, important classes like `LuceneIndexService` get
//! `ref_score: 0` while generic methods like `GetFileName` dominate centrality.

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
    // TEST: Field type creates Uses relationship
    // ========================================================================

    #[test]
    fn test_field_type_creates_uses_relationship() {
        let code = r#"
public interface ILogger { }

public class MyService {
    private ILogger _logger;
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

        let has_uses = results.relationships.iter().any(|r| {
            r.from_symbol_id == my_service.id
                && r.to_symbol_id == ilogger.id
                && r.kind == RelationshipKind::Uses
        });

        assert!(
            has_uses,
            "MyService should have Uses relationship to ILogger from field declaration.\n\
             All relationships: {:?}",
            results
                .relationships
                .iter()
                .map(|r| format!("{} --{:?}--> {}", r.from_symbol_id, r.kind, r.to_symbol_id))
                .collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // TEST: Property type creates Uses relationship
    // ========================================================================

    #[test]
    fn test_property_type_creates_uses_relationship() {
        let code = r#"
public interface ILogger { }

public class MyService {
    public ILogger Logger { get; set; }
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

        let has_uses = results.relationships.iter().any(|r| {
            r.from_symbol_id == my_service.id
                && r.to_symbol_id == ilogger.id
                && r.kind == RelationshipKind::Uses
        });

        assert!(
            has_uses,
            "MyService should have Uses relationship to ILogger from property declaration.\n\
             All relationships: {:?}",
            results
                .relationships
                .iter()
                .map(|r| format!("{} --{:?}--> {}", r.from_symbol_id, r.kind, r.to_symbol_id))
                .collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // TEST: Predefined type fields are skipped (string, int, bool)
    // ========================================================================

    #[test]
    fn test_predefined_type_fields_skipped() {
        let code = r#"
public interface IRepo { }

public class MyService {
    private string _name;
    private int _count;
    private bool _active;
    private IRepo _repo;
}
"#;

        let results = extract_full("src/MyService.cs", code);

        let my_service = results
            .symbols
            .iter()
            .find(|s| s.name == "MyService" && s.kind == SymbolKind::Class)
            .expect("Should find MyService class");

        // Only IRepo should create a Uses relationship — string, int, bool should be skipped
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
            "Should have exactly 1 Uses relationship (to IRepo), not to string/int/bool.\n\
             Found: {:?}",
            uses_rels
                .iter()
                .map(|r| &r.to_symbol_id)
                .collect::<Vec<_>>()
        );
        assert_eq!(uses_rels[0].to_symbol_id, irepo.id);
    }

    // ========================================================================
    // TEST: Generic field type extracts base type (IRepository<User> -> IRepository)
    // ========================================================================

    #[test]
    fn test_generic_field_type_extracts_base_type() {
        let code = r#"
public interface IRepository { }

public class UserService {
    private readonly IRepository<User> _repo;
}
"#;

        let results = extract_full("src/UserService.cs", code);

        let user_service = results
            .symbols
            .iter()
            .find(|s| s.name == "UserService" && s.kind == SymbolKind::Class)
            .expect("Should find UserService class");

        let irepo = results
            .symbols
            .iter()
            .find(|s| s.name == "IRepository")
            .expect("Should find IRepository");

        let has_uses = results.relationships.iter().any(|r| {
            r.from_symbol_id == user_service.id
                && r.to_symbol_id == irepo.id
                && r.kind == RelationshipKind::Uses
        });

        assert!(
            has_uses,
            "UserService should have Uses relationship to IRepository (from IRepository<User> field)"
        );
    }

    // ========================================================================
    // TEST: No duplicate when field and constructor share same type
    // ========================================================================

    #[test]
    fn test_no_duplicate_when_field_and_constructor_share_type() {
        let code = r#"
public interface ILogger { }

public class MyService {
    private ILogger _logger;

    public MyService(ILogger logger) {
        _logger = logger;
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

        // Should have exactly 1 Uses relationship to ILogger (deduplicated)
        let uses_to_ilogger: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| {
                r.from_symbol_id == my_service.id
                    && r.to_symbol_id == ilogger.id
                    && r.kind == RelationshipKind::Uses
            })
            .collect();

        assert_eq!(
            uses_to_ilogger.len(),
            1,
            "Should have exactly 1 Uses relationship to ILogger (not duplicated from field + constructor).\n\
             Found {} Uses to ILogger",
            uses_to_ilogger.len()
        );
    }

    // ========================================================================
    // TEST: Nullable field type creates relationship
    // ========================================================================

    #[test]
    fn test_nullable_field_type_creates_relationship() {
        let code = r#"
public interface ICache { }

public class MyService {
    private ICache? _cache;
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

        let has_uses = results.relationships.iter().any(|r| {
            r.from_symbol_id == my_service.id
                && r.to_symbol_id == icache.id
                && r.kind == RelationshipKind::Uses
        });

        assert!(
            has_uses,
            "MyService should have Uses relationship to ICache (from nullable ICache? field)"
        );
    }

    // ========================================================================
    // TEST: Cross-file field type creates PendingRelationship
    // ========================================================================

    #[test]
    fn test_cross_file_field_type_creates_pending_relationship() {
        let code = r#"
public class OrderController {
    private IOrderService _orderService;
    private IPaymentGateway _gateway;
}
"#;

        let results = extract_full("src/OrderController.cs", code);

        let controller = results
            .symbols
            .iter()
            .find(|s| s.name == "OrderController" && s.kind == SymbolKind::Class)
            .expect("Should find OrderController class");

        let pending_uses: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| p.from_symbol_id == controller.id && p.kind == RelationshipKind::Uses)
            .collect();

        assert!(
            pending_uses.len() >= 2,
            "Should create PendingRelationships for cross-file field types.\n\
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
    }
}
