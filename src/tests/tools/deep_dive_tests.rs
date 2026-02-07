//! Tests for the deep_dive tool — progressive-depth, kind-aware symbol context
//!
//! Two test layers:
//! 1. Formatting tests: construct SymbolContext in memory, verify output strings
//! 2. Data layer tests: create temp SQLite, store symbols + relationships, test queries

#[cfg(test)]
mod formatting_tests {
    use crate::extractors::base::{RelationshipKind, Symbol, SymbolKind, Visibility};
    use crate::tools::deep_dive::data::{RefEntry, SymbolContext};
    use crate::tools::deep_dive::formatting::format_symbol_context;

    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        file_path: &str,
        line: u32,
        signature: Option<&str>,
        visibility: Option<Visibility>,
        code_context: Option<&str>,
    ) -> Symbol {
        Symbol {
            id: format!("test_{}_{}", name, line),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: line,
            end_line: line + 10,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            parent_id: None,
            signature: signature.map(|s| s.to_string()),
            doc_comment: None,
            visibility,
            metadata: None,
            semantic_group: None,
            confidence: Some(0.9),
            code_context: code_context.map(|s| s.to_string()),
            content_type: None,
        }
    }

    fn make_ref(kind: RelationshipKind, file: &str, line: u32, sym: Option<Symbol>) -> RefEntry {
        RefEntry {
            kind,
            file_path: file.to_string(),
            line_number: line,
            symbol: sym,
        }
    }

    fn empty_context(symbol: Symbol) -> SymbolContext {
        SymbolContext {
            symbol,
            incoming: vec![],
            incoming_total: 0,
            outgoing: vec![],
            outgoing_total: 0,
            children: vec![],
            implementations: vec![],
            test_refs: vec![],
        }
    }

    // === Header formatting ===

    #[test]
    fn test_header_shows_file_line_kind() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            Some("pub fn process(data: &[u8]) -> Result<()>"),
            Some(Visibility::Public),
            None,
        );
        let ctx = empty_context(sym);
        let output = format_symbol_context(&ctx, "overview");

        assert!(output.contains("src/engine.rs:42"), "should show file:line");
        assert!(output.contains("function"), "should show kind");
        assert!(output.contains("public"), "should show visibility");
        assert!(
            output.contains("pub fn process(data: &[u8]) -> Result<()>"),
            "should show signature"
        );
    }

    #[test]
    fn test_header_no_visibility_when_none() {
        let sym = make_symbol(
            "helper",
            SymbolKind::Function,
            "src/utils.rs",
            10,
            None,
            None,
            None,
        );
        let ctx = empty_context(sym);
        let output = format_symbol_context(&ctx, "overview");

        assert!(output.contains("src/utils.rs:10"));
        assert!(output.contains("function"));
        // Should not contain "public" or "private"
        assert!(!output.contains("public"));
        assert!(!output.contains("private"));
    }

    // === Callable (Function/Method) formatting ===

    #[test]
    fn test_callable_shows_callers_section() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            None,
            None,
            None,
        );
        let caller_sym = make_symbol(
            "main",
            SymbolKind::Function,
            "src/main.rs",
            10,
            None,
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.incoming = vec![make_ref(
            RelationshipKind::Calls,
            "src/main.rs",
            15,
            Some(caller_sym),
        )];
        ctx.incoming_total = 1;

        let output = format_symbol_context(&ctx, "overview");
        assert!(output.contains("Callers"), "should have Callers section");
        assert!(output.contains("src/main.rs:15"), "should show caller location");
        assert!(output.contains("main"), "should show caller name at overview depth");
    }

    #[test]
    fn test_callable_shows_callees_section() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            None,
            None,
            None,
        );
        let callee_sym = make_symbol(
            "validate",
            SymbolKind::Function,
            "src/validate.rs",
            5,
            Some("fn validate(input: &str) -> bool"),
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.outgoing = vec![make_ref(
            RelationshipKind::Calls,
            "src/validate.rs",
            5,
            Some(callee_sym),
        )];
        ctx.outgoing_total = 1;

        let output = format_symbol_context(&ctx, "overview");
        assert!(output.contains("Callees"), "should have Callees section");
        assert!(output.contains("src/validate.rs:5"));
    }

    #[test]
    fn test_callable_context_depth_shows_signatures() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            None,
            None,
            Some("fn process() {\n    validate();\n    transform();\n}"),
        );
        let caller_sym = make_symbol(
            "main",
            SymbolKind::Function,
            "src/main.rs",
            10,
            Some("fn main() -> Result<()>"),
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.incoming = vec![make_ref(
            RelationshipKind::Calls,
            "src/main.rs",
            15,
            Some(caller_sym),
        )];
        ctx.incoming_total = 1;

        let output = format_symbol_context(&ctx, "context");
        // At context depth, refs should show signature (not just name)
        assert!(
            output.contains("fn main() -> Result<()>"),
            "context depth should show caller signature"
        );
        // Should also include body
        assert!(output.contains("Body:"), "context depth should show body");
        assert!(output.contains("validate()"), "body should have content");
    }

    #[test]
    fn test_callable_overview_no_body() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            None,
            None,
            Some("fn process() { do_stuff(); }"),
        );
        let ctx = empty_context(sym);
        let output = format_symbol_context(&ctx, "overview");

        assert!(!output.contains("Body:"), "overview should NOT show body");
    }

    #[test]
    fn test_body_truncation_shows_remaining_count() {
        // Create a body with 40 lines — context depth caps at 30
        let lines: Vec<String> = (1..=40).map(|i| format!("    line_{};", i)).collect();
        let body = format!("fn big_func() {{\n{}\n}}", lines.join("\n"));

        let sym = make_symbol(
            "big_func",
            SymbolKind::Function,
            "src/engine.rs",
            1,
            None,
            None,
            Some(&body),
        );
        let ctx = empty_context(sym);
        let output = format_symbol_context(&ctx, "context");

        assert!(output.contains("Body:"), "context depth should show body");
        assert!(output.contains("line_1"), "should show first lines");
        assert!(!output.contains("line_40"), "should NOT show last lines (truncated)");
        assert!(
            output.contains("more lines"),
            "should indicate remaining lines, got:\n{}",
            output
        );
    }

    #[test]
    fn test_body_no_truncation_indicator_when_fits() {
        let sym = make_symbol(
            "small_func",
            SymbolKind::Function,
            "src/engine.rs",
            1,
            None,
            None,
            Some("fn small_func() {\n    do_stuff();\n}"),
        );
        let ctx = empty_context(sym);
        let output = format_symbol_context(&ctx, "context");

        assert!(output.contains("Body:"), "should show body");
        assert!(
            !output.contains("more lines"),
            "should NOT show truncation for short body"
        );
    }

    // === Ref truncation indicator ===

    #[test]
    fn test_ref_section_shows_truncation_count() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            None,
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        // 2 refs shown but 10 total
        ctx.incoming = vec![
            make_ref(RelationshipKind::Calls, "src/a.rs", 1, None),
            make_ref(RelationshipKind::Calls, "src/b.rs", 2, None),
        ];
        ctx.incoming_total = 10;

        let output = format_symbol_context(&ctx, "overview");
        assert!(
            output.contains("2 of 10"),
            "should show '2 of 10' when truncated, got: {}",
            output
        );
    }

    // === Trait/Interface formatting ===

    #[test]
    fn test_trait_shows_required_methods() {
        let sym = make_symbol(
            "Handler",
            SymbolKind::Trait,
            "src/handler.rs",
            5,
            Some("pub trait Handler"),
            Some(Visibility::Public),
            None,
        );
        let method1 = make_symbol(
            "handle",
            SymbolKind::Method,
            "src/handler.rs",
            8,
            Some("fn handle(&self, req: Request) -> Response"),
            None,
            None,
        );
        let method2 = make_symbol(
            "can_handle",
            SymbolKind::Method,
            "src/handler.rs",
            12,
            Some("fn can_handle(&self, req: &Request) -> bool"),
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.children = vec![method1, method2];

        let output = format_symbol_context(&ctx, "overview");
        assert!(
            output.contains("Required methods (2)"),
            "should show required methods count"
        );
        assert!(output.contains("fn handle(&self, req: Request) -> Response"));
        assert!(output.contains("fn can_handle(&self, req: &Request) -> bool"));
    }

    #[test]
    fn test_trait_shows_implementations() {
        let sym = make_symbol(
            "Handler",
            SymbolKind::Trait,
            "src/handler.rs",
            5,
            None,
            None,
            None,
        );
        let impl1 = make_symbol(
            "ApiHandler",
            SymbolKind::Class,
            "src/api.rs",
            20,
            None,
            None,
            None,
        );
        let impl2 = make_symbol(
            "WebHandler",
            SymbolKind::Class,
            "src/web.rs",
            30,
            None,
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.implementations = vec![impl1, impl2];

        let output = format_symbol_context(&ctx, "overview");
        assert!(
            output.contains("Implementations (2)"),
            "should show implementations count"
        );
        assert!(output.contains("src/api.rs:20"));
        assert!(output.contains("ApiHandler"));
        assert!(output.contains("src/web.rs:30"));
        assert!(output.contains("WebHandler"));
    }

    // === Class/Struct formatting ===

    #[test]
    fn test_struct_shows_fields_and_methods() {
        let sym = make_symbol(
            "UserService",
            SymbolKind::Class,
            "src/service.rs",
            10,
            Some("pub struct UserService"),
            Some(Visibility::Public),
            None,
        );
        let field = make_symbol(
            "users",
            SymbolKind::Property,
            "src/service.rs",
            12,
            Some("users: Vec<User>"),
            None,
            None,
        );
        let method = make_symbol(
            "get_user",
            SymbolKind::Method,
            "src/service.rs",
            15,
            Some("pub fn get_user(&self, id: u64) -> Option<&User>"),
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.children = vec![field, method];

        let output = format_symbol_context(&ctx, "overview");
        assert!(output.contains("Fields (1)"), "should have Fields section");
        assert!(output.contains("users: Vec<User>"), "should show field signature");
        assert!(output.contains("Methods (1)"), "should have Methods section");
        assert!(output.contains("pub fn get_user"));
    }

    // === Enum formatting ===

    #[test]
    fn test_enum_shows_members() {
        let sym = make_symbol(
            "Color",
            SymbolKind::Enum,
            "src/types.rs",
            1,
            Some("pub enum Color"),
            Some(Visibility::Public),
            None,
        );
        let red = make_symbol("Red", SymbolKind::EnumMember, "src/types.rs", 2, None, None, None);
        let green = make_symbol("Green", SymbolKind::EnumMember, "src/types.rs", 3, None, None, None);
        let blue = make_symbol("Blue", SymbolKind::EnumMember, "src/types.rs", 4, None, None, None);
        let mut ctx = empty_context(sym);
        ctx.children = vec![red, green, blue];

        let output = format_symbol_context(&ctx, "overview");
        assert!(output.contains("Members (3)"), "should show enum members");
        assert!(output.contains("Red"));
        assert!(output.contains("Green"));
        assert!(output.contains("Blue"));
    }

    // === Module formatting ===

    #[test]
    fn test_module_shows_exports() {
        let sym = make_symbol(
            "auth",
            SymbolKind::Module,
            "src/auth/mod.rs",
            1,
            None,
            Some(Visibility::Public),
            None,
        );
        let child1 = make_symbol(
            "authenticate",
            SymbolKind::Function,
            "src/auth/mod.rs",
            5,
            Some("pub fn authenticate(token: &str) -> bool"),
            Some(Visibility::Public),
            None,
        );
        let child2 = make_symbol(
            "Token",
            SymbolKind::Class,
            "src/auth/mod.rs",
            20,
            Some("pub struct Token"),
            Some(Visibility::Public),
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.children = vec![child1, child2];

        let output = format_symbol_context(&ctx, "overview");
        assert!(
            output.contains("Public exports (2)"),
            "should show public exports, got: {}",
            output
        );
        assert!(output.contains("authenticate"));
        assert!(output.contains("Token"));
    }

    // === Generic (fallback) formatting ===

    #[test]
    fn test_generic_symbol_shows_references() {
        let sym = make_symbol(
            "MAX_RETRIES",
            SymbolKind::Constant,
            "src/config.rs",
            1,
            Some("pub const MAX_RETRIES: u32 = 3"),
            Some(Visibility::Public),
            None,
        );
        let ref_sym = make_symbol(
            "retry_loop",
            SymbolKind::Function,
            "src/retry.rs",
            10,
            None,
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.incoming = vec![make_ref(
            RelationshipKind::Uses,
            "src/retry.rs",
            15,
            Some(ref_sym),
        )];
        ctx.incoming_total = 1;

        let output = format_symbol_context(&ctx, "overview");
        assert!(output.contains("Referenced by"), "should show 'Referenced by'");
        assert!(output.contains("src/retry.rs:15"));
    }

    // === Callable Types section ===

    #[test]
    fn test_callable_shows_types_section() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            Some("pub fn process(order: &Order) -> Result<Receipt>"),
            None,
            None,
        );
        let order_sym = make_symbol(
            "Order",
            SymbolKind::Class,
            "src/models/order.rs",
            12,
            None,
            Some(Visibility::Public),
            None,
        );
        let receipt_sym = make_symbol(
            "Receipt",
            SymbolKind::Class,
            "src/models/payment.rs",
            45,
            None,
            Some(Visibility::Public),
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.outgoing = vec![
            make_ref(
                RelationshipKind::Parameter,
                "src/models/order.rs",
                12,
                Some(order_sym),
            ),
            make_ref(
                RelationshipKind::Returns,
                "src/models/payment.rs",
                45,
                Some(receipt_sym),
            ),
        ];
        ctx.outgoing_total = 2;

        let output = format_symbol_context(&ctx, "overview");
        assert!(output.contains("Types"), "should have Types section");
        assert!(output.contains("Order"), "should show parameter type");
        assert!(output.contains("Receipt"), "should show return type");
        assert!(
            output.contains("src/models/order.rs"),
            "should show type location"
        );
    }

    #[test]
    fn test_callable_types_deduped_by_name() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            None,
            None,
            None,
        );
        // Same type used as both parameter and return
        let order1 = make_symbol("Order", SymbolKind::Class, "src/models/order.rs", 12, None, None, None);
        let order2 = make_symbol("Order", SymbolKind::Class, "src/models/order.rs", 12, None, None, None);
        let mut ctx = empty_context(sym);
        ctx.outgoing = vec![
            make_ref(RelationshipKind::Parameter, "src/models/order.rs", 12, Some(order1)),
            make_ref(RelationshipKind::Returns, "src/models/order.rs", 12, Some(order2)),
        ];
        ctx.outgoing_total = 2;

        let output = format_symbol_context(&ctx, "overview");
        // Should show "Types (1)" not "Types (2)" — deduped
        assert!(
            output.contains("Types (1)"),
            "should dedup types by name, got: {}",
            output
        );
    }

    // === Struct Implements section ===

    #[test]
    fn test_struct_shows_implements_section() {
        let sym = make_symbol(
            "UserService",
            SymbolKind::Class,
            "src/service.rs",
            10,
            Some("pub struct UserService"),
            Some(Visibility::Public),
            None,
        );
        let display_trait = make_symbol(
            "Display",
            SymbolKind::Trait,
            "src/core/fmt.rs",
            15,
            None,
            None,
            None,
        );
        let serialize_trait = make_symbol(
            "Serialize",
            SymbolKind::Trait,
            "src/serde/ser.rs",
            8,
            None,
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.outgoing = vec![
            make_ref(RelationshipKind::Implements, "src/service.rs", 20, Some(display_trait)),
            make_ref(RelationshipKind::Implements, "src/service.rs", 30, Some(serialize_trait)),
        ];
        ctx.outgoing_total = 2;

        let output = format_symbol_context(&ctx, "overview");
        assert!(
            output.contains("Implements (2)"),
            "should have Implements section, got: {}",
            output
        );
        assert!(output.contains("Display"), "should show Display trait");
        assert!(output.contains("Serialize"), "should show Serialize trait");
    }

    // === Module Dependencies section ===

    #[test]
    fn test_module_shows_dependencies() {
        let sym = make_symbol(
            "payment",
            SymbolKind::Module,
            "src/payment/mod.rs",
            1,
            None,
            Some(Visibility::Public),
            None,
        );
        let order_sym = make_symbol("Order", SymbolKind::Class, "src/models/order.rs", 12, None, None, None);
        let line_item_sym = make_symbol("LineItem", SymbolKind::Class, "src/models/order.rs", 50, None, None, None);
        let money_sym = make_symbol("Money", SymbolKind::Class, "src/models/money.rs", 5, None, None, None);

        let mut ctx = empty_context(sym);
        ctx.outgoing = vec![
            make_ref(RelationshipKind::Imports, "src/models/order.rs", 12, Some(order_sym)),
            make_ref(RelationshipKind::Imports, "src/models/order.rs", 50, Some(line_item_sym)),
            make_ref(RelationshipKind::Imports, "src/models/money.rs", 5, Some(money_sym)),
        ];
        ctx.outgoing_total = 3;

        let output = format_symbol_context(&ctx, "overview");
        assert!(
            output.contains("Dependencies"),
            "should have Dependencies section, got: {}",
            output
        );
        // Grouped by file: order.rs should show both Order and LineItem
        assert!(
            output.contains("src/models/order.rs") && output.contains("Order") && output.contains("LineItem"),
            "should group imports by file"
        );
        assert!(output.contains("src/models/money.rs"));
        assert!(output.contains("Money"));
    }

    // === Test locations at full depth ===

    #[test]
    fn test_full_depth_shows_test_locations() {
        let sym = make_symbol(
            "search_index",
            SymbolKind::Function,
            "src/search/index.rs",
            42,
            Some("pub fn search_index()"),
            None,
            None,
        );
        let test_sym = make_symbol(
            "test_search_basic",
            SymbolKind::Function,
            "src/tests/search_tests.rs",
            78,
            Some("fn test_search_basic()"),
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.test_refs = vec![make_ref(
            RelationshipKind::Calls,
            "src/tests/search_tests.rs",
            78,
            Some(test_sym),
        )];

        let output = format_symbol_context(&ctx, "full");
        assert!(
            output.contains("Test locations"),
            "full depth should show test locations, got: {}",
            output
        );
        assert!(output.contains("test_search_basic"));
        assert!(output.contains("src/tests/search_tests.rs"));
    }

    #[test]
    fn test_overview_depth_hides_test_locations() {
        let sym = make_symbol(
            "search_index",
            SymbolKind::Function,
            "src/search/index.rs",
            42,
            None,
            None,
            None,
        );
        let test_sym = make_symbol(
            "test_search_basic",
            SymbolKind::Function,
            "src/tests/search_tests.rs",
            78,
            None,
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.test_refs = vec![make_ref(
            RelationshipKind::Calls,
            "src/tests/search_tests.rs",
            78,
            Some(test_sym),
        )];

        let output = format_symbol_context(&ctx, "overview");
        assert!(
            !output.contains("Test locations"),
            "overview should NOT show test locations"
        );
    }

    // === Full depth shows bodies in refs ===

    #[test]
    fn test_full_depth_shows_ref_bodies() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            None,
            None,
            None,
        );
        let caller_sym = make_symbol(
            "main",
            SymbolKind::Function,
            "src/main.rs",
            10,
            Some("fn main()"),
            None,
            Some("fn main() {\n    let result = process();\n    println!(\"{}\", result);\n}"),
        );
        let mut ctx = empty_context(sym);
        ctx.incoming = vec![make_ref(
            RelationshipKind::Calls,
            "src/main.rs",
            15,
            Some(caller_sym),
        )];
        ctx.incoming_total = 1;

        let output = format_symbol_context(&ctx, "full");
        // Full depth should show the body of referenced symbols
        assert!(
            output.contains("let result = process()"),
            "full depth should show ref body, got: {}",
            output
        );
    }
}

#[cfg(test)]
mod data_tests {
    use crate::database::{FileInfo, SymbolDatabase};
    use crate::extractors::base::{
        Relationship, RelationshipKind, Symbol, SymbolKind, Visibility,
    };
    use crate::tools::deep_dive::data::{build_symbol_context, find_symbol};
    use tempfile::TempDir;

    fn setup_db() -> (TempDir, SymbolDatabase) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Store file info (FK constraint requires this)
        for file in &["src/engine.rs", "src/main.rs", "src/handler.rs", "src/tests/search_tests.rs"] {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 500,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 2,
                content: None,
            })
            .unwrap();
        }

        (temp_dir, db)
    }

    fn make_symbol(
        id: &str,
        name: &str,
        kind: SymbolKind,
        file: &str,
        line: u32,
        parent_id: Option<&str>,
        signature: Option<&str>,
        visibility: Option<Visibility>,
        code_context: Option<&str>,
    ) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: file.to_string(),
            start_line: line,
            end_line: line + 10,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            parent_id: parent_id.map(|s| s.to_string()),
            signature: signature.map(|s| s.to_string()),
            doc_comment: None,
            visibility,
            metadata: None,
            semantic_group: None,
            confidence: Some(0.9),
            code_context: code_context.map(|s| s.to_string()),
            content_type: None,
        }
    }

    fn make_rel(
        id: &str,
        from: &str,
        to: &str,
        kind: RelationshipKind,
        file: &str,
        line: u32,
    ) -> Relationship {
        Relationship {
            id: id.to_string(),
            from_symbol_id: from.to_string(),
            to_symbol_id: to.to_string(),
            kind,
            file_path: file.to_string(),
            line_number: line,
            confidence: 0.9,
            metadata: None,
        }
    }

    // === find_symbol tests ===

    #[test]
    fn test_find_symbol_by_name() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![make_symbol(
            "sym-1",
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            Some("pub fn process()"),
            Some(Visibility::Public),
            None,
        )];
        db.store_symbols(&symbols).unwrap();

        let found = find_symbol(&db, "process", None).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "process");
        assert_eq!(found[0].file_path, "src/engine.rs");
    }

    #[test]
    fn test_find_symbol_filters_imports() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym-def",
                "process",
                SymbolKind::Function,
                "src/engine.rs",
                10,
                None,
                None,
                None,
                None,
            ),
            make_symbol(
                "sym-import",
                "process",
                SymbolKind::Import,
                "src/main.rs",
                1,
                None,
                None,
                None,
                None,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        let found = find_symbol(&db, "process", None).unwrap();
        assert_eq!(found.len(), 1, "imports should be filtered out");
        assert_eq!(found[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_find_symbol_disambiguates_by_file() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym-1",
                "handle",
                SymbolKind::Function,
                "src/engine.rs",
                10,
                None,
                None,
                None,
                None,
            ),
            make_symbol(
                "sym-2",
                "handle",
                SymbolKind::Function,
                "src/handler.rs",
                20,
                None,
                None,
                None,
                None,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        let found = find_symbol(&db, "handle", Some("handler")).unwrap();
        assert_eq!(found.len(), 1, "should disambiguate by file");
        assert_eq!(found[0].file_path, "src/handler.rs");
    }

    #[test]
    fn test_find_symbol_not_found() {
        let (_tmp, db) = setup_db();

        let found = find_symbol(&db, "nonexistent", None).unwrap();
        assert!(found.is_empty());
    }

    // === build_symbol_context tests ===

    #[test]
    fn test_build_context_with_incoming_relationships() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym-target",
                "process",
                SymbolKind::Function,
                "src/engine.rs",
                10,
                None,
                None,
                None,
                None,
            ),
            make_symbol(
                "sym-caller",
                "main",
                SymbolKind::Function,
                "src/main.rs",
                5,
                None,
                Some("fn main()"),
                None,
                None,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        let rels = vec![make_rel(
            "rel-1",
            "sym-caller",
            "sym-target",
            RelationshipKind::Calls,
            "src/main.rs",
            8,
        )];
        db.store_relationships(&rels).unwrap();

        let ctx =
            build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

        assert_eq!(ctx.incoming.len(), 1);
        assert_eq!(ctx.incoming_total, 1);
        assert_eq!(ctx.incoming[0].file_path, "src/main.rs");
        assert_eq!(ctx.incoming[0].line_number, 8);
        // Overview depth: still enriched (name is always useful)
        assert!(
            ctx.incoming[0].symbol.is_some(),
            "overview should still enrich refs for symbol names"
        );
        assert_eq!(ctx.incoming[0].symbol.as_ref().unwrap().name, "main");
    }

    #[test]
    fn test_build_context_enriches_at_context_depth() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym-target",
                "process",
                SymbolKind::Function,
                "src/engine.rs",
                10,
                None,
                None,
                None,
                None,
            ),
            make_symbol(
                "sym-caller",
                "main",
                SymbolKind::Function,
                "src/main.rs",
                5,
                None,
                Some("fn main()"),
                None,
                Some("fn main() { process(); }"),
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        let rels = vec![make_rel(
            "rel-1",
            "sym-caller",
            "sym-target",
            RelationshipKind::Calls,
            "src/main.rs",
            8,
        )];
        db.store_relationships(&rels).unwrap();

        let ctx =
            build_symbol_context(&db, &symbols[0], "context", 15, 15).unwrap();

        assert_eq!(ctx.incoming.len(), 1);
        // Context depth: should enrich with symbol data
        assert!(
            ctx.incoming[0].symbol.is_some(),
            "context depth should enrich refs"
        );
        let enriched = ctx.incoming[0].symbol.as_ref().unwrap();
        assert_eq!(enriched.name, "main");
        assert_eq!(
            enriched.signature.as_deref(),
            Some("fn main()")
        );
    }

    #[test]
    fn test_build_context_with_outgoing_relationships() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym-source",
                "process",
                SymbolKind::Function,
                "src/engine.rs",
                10,
                None,
                None,
                None,
                None,
            ),
            make_symbol(
                "sym-callee",
                "validate",
                SymbolKind::Function,
                "src/engine.rs",
                50,
                None,
                Some("fn validate()"),
                None,
                None,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        let rels = vec![make_rel(
            "rel-1",
            "sym-source",
            "sym-callee",
            RelationshipKind::Calls,
            "src/engine.rs",
            15,
        )];
        db.store_relationships(&rels).unwrap();

        let ctx =
            build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

        assert_eq!(ctx.outgoing.len(), 1);
        assert_eq!(ctx.outgoing_total, 1);
        assert_eq!(ctx.outgoing[0].file_path, "src/engine.rs");
    }

    #[test]
    fn test_build_context_with_children() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym-parent",
                "UserService",
                SymbolKind::Class,
                "src/engine.rs",
                1,
                None,
                Some("pub struct UserService"),
                Some(Visibility::Public),
                None,
            ),
            make_symbol(
                "sym-field",
                "users",
                SymbolKind::Property,
                "src/engine.rs",
                3,
                Some("sym-parent"),
                Some("users: Vec<User>"),
                None,
                None,
            ),
            make_symbol(
                "sym-method",
                "get_user",
                SymbolKind::Method,
                "src/engine.rs",
                10,
                Some("sym-parent"),
                Some("pub fn get_user(&self) -> Option<&User>"),
                Some(Visibility::Public),
                None,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        let ctx =
            build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

        assert_eq!(ctx.children.len(), 2, "should have 2 children");
        // Children ordered by start_line
        assert_eq!(ctx.children[0].name, "users");
        assert_eq!(ctx.children[1].name, "get_user");
    }

    #[test]
    fn test_build_context_non_container_has_no_children() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![make_symbol(
            "sym-func",
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            None,
            None,
            None,
        )];
        db.store_symbols(&symbols).unwrap();

        let ctx =
            build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

        assert!(
            ctx.children.is_empty(),
            "functions should not query for children"
        );
    }

    // === Identifier fallback ===

    /// Helper to insert a raw identifier into the test database
    fn insert_identifier(
        db: &SymbolDatabase,
        name: &str,
        kind: &str,
        file: &str,
        line: u32,
        containing_symbol_id: Option<&str>,
    ) {
        db.conn.execute(
            "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, containing_symbol_id, confidence)
             VALUES (?1, ?2, ?3, 'rust', ?4, ?5, 0, ?5, 10, 0, 100, ?6, 0.9)",
            rusqlite::params![
                format!("ident_{}_{}", name, line),
                name,
                kind,
                file,
                line,
                containing_symbol_id,
            ],
        ).unwrap();
    }

    #[test]
    fn test_identifier_fallback_adds_refs() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol("sym-target", "process", SymbolKind::Function, "src/engine.rs", 10, None, None, None, None),
        ];
        db.store_symbols(&symbols).unwrap();

        // No relationships — only an identifier ref (no containing symbol)
        insert_identifier(&db, "process", "call", "src/main.rs", 25, None);

        let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

        assert_eq!(ctx.incoming.len(), 1, "identifier fallback should add ref");
        assert_eq!(ctx.incoming[0].file_path, "src/main.rs");
        assert_eq!(ctx.incoming[0].line_number, 25);
        assert_eq!(ctx.incoming_total, 1);
    }

    #[test]
    fn test_identifier_fallback_deduplicates() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol("sym-target", "process", SymbolKind::Function, "src/engine.rs", 10, None, None, None, None),
            make_symbol("sym-caller", "main", SymbolKind::Function, "src/main.rs", 5, None, None, None, None),
        ];
        db.store_symbols(&symbols).unwrap();

        // Relationship at src/main.rs:8
        let rels = vec![make_rel("rel-1", "sym-caller", "sym-target", RelationshipKind::Calls, "src/main.rs", 8)];
        db.store_relationships(&rels).unwrap();

        // Identifier at the SAME location — should be deduped
        insert_identifier(&db, "process", "call", "src/main.rs", 8, Some("sym-caller"));

        // Plus one at a DIFFERENT location — should be added
        insert_identifier(&db, "process", "call", "src/handler.rs", 42, None);

        let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

        // 1 from relationship + 1 from identifier (the duplicate is deduped)
        assert_eq!(ctx.incoming.len(), 2, "should have 1 relationship + 1 identifier ref, got {}", ctx.incoming.len());
        assert_eq!(ctx.incoming_total, 2);
    }

    #[test]
    fn test_identifier_fallback_filters_own_file_definition_line() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol("sym-target", "process", SymbolKind::Function, "src/engine.rs", 10, None, None, None, None),
        ];
        db.store_symbols(&symbols).unwrap();

        // Identifier at the definition site itself — should be skipped
        insert_identifier(&db, "process", "call", "src/engine.rs", 10, None);
        // Identifier at a different location — should be kept
        insert_identifier(&db, "process", "call", "src/main.rs", 30, None);

        let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

        assert_eq!(ctx.incoming.len(), 1, "should skip definition-site identifiers");
        assert_eq!(ctx.incoming[0].file_path, "src/main.rs");
    }

    // === Test refs (test file identifiers at full depth) ===

    #[test]
    fn test_build_context_populates_test_refs_at_full_depth() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol("sym-target", "process", SymbolKind::Function, "src/engine.rs", 10, None, None, None, None),
        ];
        db.store_symbols(&symbols).unwrap();

        // Identifier in a test file
        insert_identifier(&db, "process", "call", "src/tests/search_tests.rs", 42, None);
        // Identifier in a non-test file (should NOT be in test_refs)
        insert_identifier(&db, "process", "call", "src/main.rs", 25, None);

        let ctx = build_symbol_context(&db, &symbols[0], "full", 10, 10).unwrap();

        assert_eq!(ctx.test_refs.len(), 1, "should have 1 test ref");
        assert_eq!(ctx.test_refs[0].file_path, "src/tests/search_tests.rs");
    }

    #[test]
    fn test_build_context_no_test_refs_at_overview() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol("sym-target", "process", SymbolKind::Function, "src/engine.rs", 10, None, None, None, None),
        ];
        db.store_symbols(&symbols).unwrap();

        // Identifier in a test file
        insert_identifier(&db, "process", "call", "src/tests/search_tests.rs", 42, None);

        let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

        assert!(ctx.test_refs.is_empty(), "overview should not populate test_refs");
    }

    #[test]
    fn test_build_context_caps_incoming() {
        let (_tmp, mut db) = setup_db();

        let mut symbols = vec![make_symbol(
            "sym-target",
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            None,
            None,
            None,
        )];

        // Create 5 callers
        let mut rels = vec![];
        for i in 0..5 {
            let caller_id = format!("sym-caller-{}", i);
            symbols.push(make_symbol(
                &caller_id,
                &format!("caller_{}", i),
                SymbolKind::Function,
                "src/main.rs",
                (i * 10 + 1) as u32,
                None,
                None,
                None,
                None,
            ));
            rels.push(make_rel(
                &format!("rel-{}", i),
                &caller_id,
                "sym-target",
                RelationshipKind::Calls,
                "src/main.rs",
                (i * 10 + 5) as u32,
            ));
        }
        db.store_symbols(&symbols).unwrap();
        db.store_relationships(&rels).unwrap();

        // Cap at 2 incoming
        let ctx =
            build_symbol_context(&db, &symbols[0], "overview", 2, 10).unwrap();

        assert_eq!(ctx.incoming.len(), 2, "should cap at 2");
        assert_eq!(ctx.incoming_total, 5, "total should reflect all 5");
    }
}
