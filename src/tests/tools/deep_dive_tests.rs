//! Tests for the deep_dive tool — progressive-depth, kind-aware symbol context
//!
//! Two test layers:
//! 1. Formatting tests: construct SymbolContext in memory, verify output strings
//! 2. Data layer tests: create temp SQLite, store symbols + relationships, test queries

#[cfg(test)]
mod deserialization_tests {
    use crate::tools::deep_dive::DeepDiveTool;

    #[test]
    fn test_deep_dive_accepts_symbol_name_alias() {
        // Some MCP clients send "symbol_name" instead of "symbol".
        // Verify serde accepts both field names.
        let json = r#"{"symbol_name": "MyFunction"}"#;
        let tool: DeepDiveTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.symbol, "MyFunction");
    }

    #[test]
    fn test_deep_dive_accepts_canonical_symbol_field() {
        let json = r#"{"symbol": "MyFunction"}"#;
        let tool: DeepDiveTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.symbol, "MyFunction");
    }
}

#[cfg(test)]
mod formatting_tests {
    use crate::extractors::base::{RelationshipKind, Symbol, SymbolKind, Visibility};
    use crate::tools::deep_dive::data::{RefEntry, SimilarEntry, SymbolContext};
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
            similar: vec![],
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
        assert!(
            output.contains("src/main.rs:15"),
            "should show caller location"
        );
        assert!(
            output.contains("main"),
            "should show caller name at overview depth"
        );
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
        assert!(
            !output.contains("line_40"),
            "should NOT show last lines (truncated)"
        );
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
        assert!(
            output.contains("users: Vec<User>"),
            "should show field signature"
        );
        assert!(
            output.contains("Methods (1)"),
            "should have Methods section"
        );
        assert!(output.contains("pub fn get_user"));
    }

    // Verify SymbolKind::Struct uses the class_or_struct formatter, not the generic fallback.
    // Regression test for: Struct falling into `_ => format_generic()` branch.
    #[test]
    fn test_struct_kind_uses_class_or_struct_formatter() {
        let sym = make_symbol(
            "Connection",
            SymbolKind::Struct,
            "src/db.rs",
            5,
            Some("pub struct Connection"),
            Some(Visibility::Public),
            None,
        );
        let field = make_symbol(
            "host",
            SymbolKind::Property,
            "src/db.rs",
            6,
            Some("host: String"),
            None,
            None,
        );
        let method = make_symbol(
            "connect",
            SymbolKind::Method,
            "src/db.rs",
            10,
            Some("pub fn connect(&self) -> Result<()>"),
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.children = vec![field, method];

        let output = format_symbol_context(&ctx, "overview");
        assert!(
            output.contains("Fields (1)"),
            "SymbolKind::Struct should use class_or_struct formatter, not generic. Got:\n{}",
            output
        );
        assert!(
            output.contains("Methods (1)"),
            "SymbolKind::Struct should show Methods section. Got:\n{}",
            output
        );
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
        let red = make_symbol(
            "Red",
            SymbolKind::EnumMember,
            "src/types.rs",
            2,
            None,
            None,
            None,
        );
        let green = make_symbol(
            "Green",
            SymbolKind::EnumMember,
            "src/types.rs",
            3,
            None,
            None,
            None,
        );
        let blue = make_symbol(
            "Blue",
            SymbolKind::EnumMember,
            "src/types.rs",
            4,
            None,
            None,
            None,
        );
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
        assert!(
            output.contains("Referenced by"),
            "should show 'Referenced by'"
        );
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
        let order1 = make_symbol(
            "Order",
            SymbolKind::Class,
            "src/models/order.rs",
            12,
            None,
            None,
            None,
        );
        let order2 = make_symbol(
            "Order",
            SymbolKind::Class,
            "src/models/order.rs",
            12,
            None,
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.outgoing = vec![
            make_ref(
                RelationshipKind::Parameter,
                "src/models/order.rs",
                12,
                Some(order1),
            ),
            make_ref(
                RelationshipKind::Returns,
                "src/models/order.rs",
                12,
                Some(order2),
            ),
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
            make_ref(
                RelationshipKind::Implements,
                "src/service.rs",
                20,
                Some(display_trait),
            ),
            make_ref(
                RelationshipKind::Implements,
                "src/service.rs",
                30,
                Some(serialize_trait),
            ),
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
        let order_sym = make_symbol(
            "Order",
            SymbolKind::Class,
            "src/models/order.rs",
            12,
            None,
            None,
            None,
        );
        let line_item_sym = make_symbol(
            "LineItem",
            SymbolKind::Class,
            "src/models/order.rs",
            50,
            None,
            None,
            None,
        );
        let money_sym = make_symbol(
            "Money",
            SymbolKind::Class,
            "src/models/money.rs",
            5,
            None,
            None,
            None,
        );

        let mut ctx = empty_context(sym);
        ctx.outgoing = vec![
            make_ref(
                RelationshipKind::Imports,
                "src/models/order.rs",
                12,
                Some(order_sym),
            ),
            make_ref(
                RelationshipKind::Imports,
                "src/models/order.rs",
                50,
                Some(line_item_sym),
            ),
            make_ref(
                RelationshipKind::Imports,
                "src/models/money.rs",
                5,
                Some(money_sym),
            ),
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
            output.contains("src/models/order.rs")
                && output.contains("Order")
                && output.contains("LineItem"),
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

    // === Semantic similarity section ===

    #[test]
    fn test_format_similar_section() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            Some("pub fn process(data: &[u8]) -> Result<()>"),
            Some(Visibility::Public),
            None,
        );
        let similar_sym = make_symbol(
            "handle_request",
            SymbolKind::Function,
            "src/handler.rs",
            100,
            Some("fn handle_request(req: Request) -> Response"),
            None,
            None,
        );
        let mut ctx = empty_context(sym);
        ctx.similar = vec![SimilarEntry {
            symbol: similar_sym,
            score: 0.85,
        }];

        let output = format_symbol_context(&ctx, "full");
        assert!(
            output.contains("Semantically Similar"),
            "should have Semantically Similar header, got: {}",
            output
        );
        assert!(
            output.contains("handle_request"),
            "should show similar symbol name, got: {}",
            output
        );
        assert!(
            output.contains("0.85"),
            "should show similarity score, got: {}",
            output
        );
        assert!(
            output.contains("src/handler.rs:100"),
            "should show file:line location, got: {}",
            output
        );
    }

    #[test]
    fn test_format_no_similar_section_when_empty() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            None,
            None,
            None,
        );
        let ctx = empty_context(sym);

        let output = format_symbol_context(&ctx, "full");
        assert!(
            !output.contains("Semantically Similar"),
            "should NOT have Semantically Similar header when empty, got: {}",
            output
        );
    }

    // === Test quality tier in test refs ===

    #[test]
    fn test_quality_tier_displayed_in_test_refs() {
        let sym = make_symbol(
            "process_payment",
            SymbolKind::Function,
            "src/payment.rs",
            10,
            Some("pub fn process_payment()"),
            None,
            None,
        );
        let mut test_sym = make_symbol(
            "test_process_payment",
            SymbolKind::Function,
            "tests/payment_tests.rs",
            45,
            Some("fn test_process_payment()"),
            None,
            None,
        );
        // Set test_quality metadata on the test symbol
        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "test_quality".to_string(),
            serde_json::json!({
                "quality_tier": "thorough",
                "assertion_count": 5
            }),
        );
        test_sym.metadata = Some(metadata);

        let mut ctx = empty_context(sym);
        ctx.test_refs = vec![make_ref(
            RelationshipKind::Calls,
            "tests/payment_tests.rs",
            45,
            Some(test_sym),
        )];

        let output = format_symbol_context(&ctx, "full");
        assert!(
            output.contains("[thorough]"),
            "should display quality tier tag, got: {}",
            output
        );
        assert!(
            output.contains("test_process_payment  [thorough]"),
            "quality tier should follow test name, got: {}",
            output
        );
    }

    #[test]
    fn test_no_quality_data_no_empty_brackets() {
        let sym = make_symbol(
            "process_payment",
            SymbolKind::Function,
            "src/payment.rs",
            10,
            None,
            None,
            None,
        );
        let test_sym = make_symbol(
            "test_process_payment",
            SymbolKind::Function,
            "tests/payment_tests.rs",
            45,
            None,
            None,
            None,
        );
        // No metadata set — test_sym.metadata is None

        let mut ctx = empty_context(sym);
        ctx.test_refs = vec![make_ref(
            RelationshipKind::Calls,
            "tests/payment_tests.rs",
            45,
            Some(test_sym),
        )];

        let output = format_symbol_context(&ctx, "full");
        assert!(
            output.contains("test_process_payment"),
            "should still show test name, got: {}",
            output
        );
        assert!(
            !output.contains("[]"),
            "should NOT show empty brackets when no quality data, got: {}",
            output
        );
        assert!(
            !output.contains("  ["),
            "should NOT show any quality tag bracket when no data, got: {}",
            output
        );
    }

    #[test]
    fn test_quality_info_shown_for_test_symbol_self() {
        let mut sym = make_symbol(
            "test_process_payment",
            SymbolKind::Function,
            "tests/payment_tests.rs",
            45,
            Some("fn test_process_payment()"),
            None,
            None,
        );
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("is_test".to_string(), serde_json::json!(true));
        metadata.insert(
            "test_quality".to_string(),
            serde_json::json!({
                "quality_tier": "thorough",
                "assertion_count": 5,
                "mock_count": 1,
                "assertion_density": 0.25
            }),
        );
        sym.metadata = Some(metadata);

        let ctx = empty_context(sym);
        let output = format_symbol_context(&ctx, "full");
        assert!(
            output.contains("Test quality: thorough"),
            "should show test quality line for test symbol, got: {}",
            output
        );
        assert!(
            output.contains("5 assertions"),
            "should show assertion count, got: {}",
            output
        );
        assert!(
            output.contains("1 mocks"),
            "should show mock count, got: {}",
            output
        );
        assert!(
            output.contains("0.25 density"),
            "should show assertion density, got: {}",
            output
        );
    }

    #[test]
    fn test_no_quality_info_for_non_test_symbol() {
        let mut sym = make_symbol(
            "process_payment",
            SymbolKind::Function,
            "src/payment.rs",
            10,
            Some("pub fn process_payment()"),
            None,
            None,
        );
        // Has metadata but is_test is false
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("is_test".to_string(), serde_json::json!(false));
        sym.metadata = Some(metadata);

        let ctx = empty_context(sym);
        let output = format_symbol_context(&ctx, "full");
        assert!(
            !output.contains("Test quality:"),
            "should NOT show test quality for non-test symbols, got: {}",
            output
        );
    }

    #[test]
    fn test_context_depth_shows_test_locations() {
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

        let output = format_symbol_context(&ctx, "context");
        assert!(
            output.contains("Test locations"),
            "context depth should show test locations, got: {}",
            output
        );
        assert!(
            output.contains("test_search_basic"),
            "should show test name at context depth, got: {}",
            output
        );
    }

    // === Token budget: UTF-8 safety ===

    #[test]
    fn test_truncation_does_not_panic_on_multibyte_utf8() {
        // Fill the body with emoji and CJK characters so that a naive byte-slice
        // at token_limit * 4 would land inside a multi-byte sequence.
        // "full" limit is 1800 tokens; target_chars = (1800-20)*4 = 7120 bytes.
        // Each emoji is 4 bytes, each CJK char is 3 bytes — a mix should reliably
        // hit a non-boundary if we do raw byte slicing.
        let body: String = (0..300)
            .map(|i| {
                if i % 2 == 0 {
                    "🦀 rust is great ".to_string()
                } else {
                    "中文代码注释 ".to_string()
                }
            })
            .collect();

        let sym = make_symbol(
            "unicode_heavy",
            SymbolKind::Function,
            "src/unicode.rs",
            1,
            Some("pub fn unicode_heavy()"),
            Some(Visibility::Public),
            Some(&body),
        );
        let mut ctx = empty_context(sym);
        // Add a pile of incoming refs so the pre-body output also contains multibyte chars.
        ctx.incoming = (0..30)
            .map(|i| {
                let caller = make_symbol(
                    &format!("caller_{}", i),
                    SymbolKind::Function,
                    &format!("src/file_{}.rs", i),
                    i as u32,
                    Some(&format!("fn caller_{}() // 日本語コメント", i)),
                    None,
                    None,
                );
                make_ref(
                    RelationshipKind::Calls,
                    &format!("src/file_{}.rs", i),
                    i as u32,
                    Some(caller),
                )
            })
            .collect();
        ctx.incoming_total = 30;

        // Must not panic regardless of where the byte boundary falls.
        let output = format_symbol_context(&ctx, "full");
        // Basic sanity: output is valid UTF-8 (the assert! forces the string to be
        // used; if we got here without panicking, the fix works).
        assert!(output.is_char_boundary(0), "output must be valid UTF-8");
    }

    // === Token budget enforcement ===

    #[test]
    fn test_full_depth_output_under_token_budget() {
        use crate::utils::token_estimation::TokenEstimator;

        // Build a worst-case SymbolContext for "full" depth:
        // - Primary symbol with 100-line body
        // - 50 incoming refs, each with a Symbol having a 10-line code_context
        let code_100_lines = (0..100)
            .map(|i| format!("    let x_{} = some_func_with_a_long_name_{}();", i, i))
            .collect::<Vec<_>>()
            .join("\n");

        let sym = make_symbol(
            "big_function",
            SymbolKind::Function,
            "src/engine.rs",
            1,
            Some(
                "pub fn big_function(a: &BigStruct, b: &mut OtherStruct) -> Result<LongReturnType>",
            ),
            Some(Visibility::Public),
            Some(&code_100_lines),
        );

        let code_10_lines = (0..10)
            .map(|i| format!("    let val_{} = big_function_called_here_{}();", i, i))
            .collect::<Vec<_>>()
            .join("\n");

        let incoming: Vec<RefEntry> = (0..50)
            .map(|i| {
                let caller = make_symbol(
                    &format!("caller_{}", i),
                    SymbolKind::Function,
                    &format!("src/callers/file_{}.rs", i),
                    (i * 10 + 1) as u32,
                    Some(&format!("fn caller_{}(arg: &SomeType) -> AnotherType", i)),
                    None,
                    Some(&code_10_lines),
                );
                make_ref(
                    RelationshipKind::Calls,
                    &format!("src/callers/file_{}.rs", i),
                    (i * 10 + 5) as u32,
                    Some(caller),
                )
            })
            .collect();

        let ctx = SymbolContext {
            symbol: sym,
            incoming,
            incoming_total: 50,
            outgoing: vec![],
            outgoing_total: 0,
            children: vec![],
            implementations: vec![],
            test_refs: vec![],
            similar: vec![],
        };

        let output = format_symbol_context(&ctx, "full");
        let estimator = TokenEstimator::new();
        let token_count = estimator.estimate_string(&output);

        assert!(
            token_count <= 2000,
            "full depth output exceeded token budget: {} tokens (limit 2000)\nOutput length: {} chars",
            token_count,
            output.len()
        );
        // Verify the output still contains the truncation notice
        assert!(
            output.contains("truncated"),
            "output should contain truncation notice when budget is exceeded, got:\n{}",
            &output[output.len().saturating_sub(200)..]
        );
    }

    #[test]
    fn test_overview_depth_under_token_budget() {
        use crate::utils::token_estimation::TokenEstimator;

        let sym = make_symbol(
            "tiny_fn",
            SymbolKind::Function,
            "src/lib.rs",
            1,
            Some("pub fn tiny_fn()"),
            Some(Visibility::Public),
            None,
        );

        let incoming: Vec<RefEntry> = (0..20)
            .map(|i| {
                let caller = make_symbol(
                    &format!("user_{}", i),
                    SymbolKind::Function,
                    &format!("src/users/file_{}.rs", i),
                    (i * 5 + 1) as u32,
                    None,
                    None,
                    None,
                );
                make_ref(
                    RelationshipKind::Calls,
                    &format!("src/users/file_{}.rs", i),
                    (i * 5 + 2) as u32,
                    Some(caller),
                )
            })
            .collect();

        let ctx = SymbolContext {
            symbol: sym,
            incoming,
            incoming_total: 20,
            outgoing: vec![],
            outgoing_total: 0,
            children: vec![],
            implementations: vec![],
            test_refs: vec![],
            similar: vec![],
        };

        let output = format_symbol_context(&ctx, "overview");
        let estimator = TokenEstimator::new();
        let token_count = estimator.estimate_string(&output);

        assert!(
            token_count <= 400,
            "overview depth output exceeded token budget: {} tokens (limit 400)",
            token_count
        );
    }
}

#[cfg(test)]
mod data_tests {
    use std::collections::HashMap;

    use crate::database::{FileInfo, SymbolDatabase};
    use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility};
    use crate::tools::deep_dive::data::{build_symbol_context, find_symbol};
    use crate::tools::deep_dive::deep_dive_query;
    use tempfile::TempDir;

    fn setup_db() -> (TempDir, SymbolDatabase) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Store file info (FK constraint requires this)
        for file in &[
            "src/engine.rs",
            "src/main.rs",
            "src/handler.rs",
            "src/tests/search_tests.rs",
        ] {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 500,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 2,
                line_count: 0,
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

    #[test]
    fn test_find_symbol_qualified_name() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![
            make_symbol(
                "sym-parent-a",
                "Engine",
                SymbolKind::Struct,
                "src/engine.rs",
                1,
                None,
                Some("pub struct Engine"),
                Some(Visibility::Public),
                None,
            ),
            make_symbol(
                "sym-method-a",
                "process",
                SymbolKind::Method,
                "src/engine.rs",
                10,
                Some("sym-parent-a"),
                Some("pub fn process(&self)"),
                Some(Visibility::Public),
                None,
            ),
            make_symbol(
                "sym-parent-b",
                "Pipeline",
                SymbolKind::Struct,
                "src/engine.rs",
                50,
                None,
                Some("pub struct Pipeline"),
                Some(Visibility::Public),
                None,
            ),
            make_symbol(
                "sym-method-b",
                "process",
                SymbolKind::Method,
                "src/engine.rs",
                60,
                Some("sym-parent-b"),
                Some("pub fn process(&self)"),
                Some(Visibility::Public),
                None,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        // Qualified lookup should resolve to exactly one symbol
        let found = find_symbol(&db, "Engine::process", None).unwrap();
        assert_eq!(
            found.len(),
            1,
            "qualified name should resolve to exactly one symbol"
        );
        assert_eq!(found[0].file_path, "src/engine.rs");
        assert_eq!(found[0].parent_id, Some("sym-parent-a".to_string()));

        // Dot-separated also works (for Python, JS, etc.)
        let found_dot = find_symbol(&db, "Pipeline.process", None).unwrap();
        assert_eq!(found_dot.len(), 1);
        assert_eq!(found_dot[0].parent_id, Some("sym-parent-b".to_string()));

        // Unqualified still returns both
        let found_all = find_symbol(&db, "process", None).unwrap();
        assert_eq!(found_all.len(), 2, "unqualified should still return both");
    }

    #[test]
    fn test_find_symbol_qualified_name_uses_impl_type_metadata() {
        let (_tmp, mut db) = setup_db();

        let mut metadata = HashMap::new();
        metadata.insert(
            "impl_type_name".to_string(),
            serde_json::Value::String("Worker".to_string()),
        );

        let mut method_symbol = make_symbol(
            "sym-run",
            "run",
            SymbolKind::Method,
            "src/engine.rs",
            5,
            None,
            Some("fn run(&self)"),
            Some(Visibility::Private),
            None,
        );
        method_symbol.metadata = Some(metadata);

        let symbols = vec![
            make_symbol(
                "sym-worker",
                "Worker",
                SymbolKind::Struct,
                "src/engine.rs",
                1,
                None,
                Some("pub struct Worker;"),
                Some(Visibility::Public),
                None,
            ),
            method_symbol,
        ];
        db.store_symbols(&symbols).unwrap();

        let found = find_symbol(&db, "Worker::run", None).unwrap();
        assert_eq!(
            found.len(),
            1,
            "qualified lookup should use impl_type_name metadata when parent_id is missing"
        );
        assert_eq!(found[0].id, "sym-run");
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

        let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

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

        let ctx = build_symbol_context(&db, &symbols[0], "context", 15, 15).unwrap();

        assert_eq!(ctx.incoming.len(), 1);
        // Context depth: should enrich with symbol data
        assert!(
            ctx.incoming[0].symbol.is_some(),
            "context depth should enrich refs"
        );
        let enriched = ctx.incoming[0].symbol.as_ref().unwrap();
        assert_eq!(enriched.name, "main");
        assert_eq!(enriched.signature.as_deref(), Some("fn main()"));
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

        let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

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

        let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

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

        let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

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

        let symbols = vec![make_symbol(
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
                None,
                None,
                None,
            ),
        ];
        db.store_symbols(&symbols).unwrap();

        // Relationship at src/main.rs:8
        let rels = vec![make_rel(
            "rel-1",
            "sym-caller",
            "sym-target",
            RelationshipKind::Calls,
            "src/main.rs",
            8,
        )];
        db.store_relationships(&rels).unwrap();

        // Identifier at the SAME location — should be deduped
        insert_identifier(&db, "process", "call", "src/main.rs", 8, Some("sym-caller"));

        // Plus one at a DIFFERENT location — should be added
        insert_identifier(&db, "process", "call", "src/handler.rs", 42, None);

        let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

        // 1 from relationship + 1 from identifier (the duplicate is deduped)
        assert_eq!(
            ctx.incoming.len(),
            2,
            "should have 1 relationship + 1 identifier ref, got {}",
            ctx.incoming.len()
        );
        assert_eq!(ctx.incoming_total, 2);
    }

    #[test]
    fn test_identifier_fallback_filters_own_file_definition_line() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![make_symbol(
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
        db.store_symbols(&symbols).unwrap();

        // Identifier at the definition site itself — should be skipped
        insert_identifier(&db, "process", "call", "src/engine.rs", 10, None);
        // Identifier at a different location — should be kept
        insert_identifier(&db, "process", "call", "src/main.rs", 30, None);

        let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

        assert_eq!(
            ctx.incoming.len(),
            1,
            "should skip definition-site identifiers"
        );
        assert_eq!(ctx.incoming[0].file_path, "src/main.rs");
    }

    // === Test refs (test file identifiers at full depth) ===

    #[test]
    fn test_build_context_populates_test_refs_at_full_depth() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![make_symbol(
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
        db.store_symbols(&symbols).unwrap();

        // Identifier in a test file
        insert_identifier(
            &db,
            "process",
            "call",
            "src/tests/search_tests.rs",
            42,
            None,
        );
        // Identifier in a non-test file (should NOT be in test_refs)
        insert_identifier(&db, "process", "call", "src/main.rs", 25, None);

        let ctx = build_symbol_context(&db, &symbols[0], "full", 10, 10).unwrap();

        assert_eq!(ctx.test_refs.len(), 1, "should have 1 test ref");
        assert_eq!(ctx.test_refs[0].file_path, "src/tests/search_tests.rs");
    }

    #[test]
    fn test_build_context_no_test_refs_at_overview() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![make_symbol(
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
        db.store_symbols(&symbols).unwrap();

        // Identifier in a test file
        insert_identifier(
            &db,
            "process",
            "call",
            "src/tests/search_tests.rs",
            42,
            None,
        );

        let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

        assert!(
            ctx.test_refs.is_empty(),
            "overview should not populate test_refs"
        );
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
        let ctx = build_symbol_context(&db, &symbols[0], "overview", 2, 10).unwrap();

        assert_eq!(ctx.incoming.len(), 2, "should cap at 2");
        assert_eq!(ctx.incoming_total, 5, "total should reflect all 5");
    }

    // === disambiguation threshold tests ===

    #[test]
    fn test_deep_dive_query_returns_compact_list_when_too_many_matches() {
        let (_tmp, mut db) = setup_db();

        // Register extra files beyond what setup_db provides
        let extra_files = [
            "src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs", "src/e.rs", "src/f.rs",
        ];
        for file in &extra_files {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 100,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 1,
                line_count: 0,
                content: None,
            })
            .unwrap();
        }

        // Create 6 symbols with the same name in different files (exceeds threshold of 5)
        let symbols: Vec<Symbol> = extra_files
            .iter()
            .enumerate()
            .map(|(i, file)| {
                make_symbol(
                    &format!("sym-extract-{}", i),
                    "extract",
                    SymbolKind::Function,
                    file,
                    10,
                    None,
                    Some("pub fn extract()"),
                    Some(Visibility::Public),
                    None,
                )
            })
            .collect();
        db.store_symbols(&symbols).unwrap();

        // Call deep_dive_query — should get compact disambiguation, not full contexts
        let result = deep_dive_query(&db, "extract", None, "overview", 10, 10).unwrap();

        // Should mention the count and disambiguation hint
        assert!(
            result.contains("Found 6 definitions"),
            "Should report 6 definitions, got: {}",
            result
        );
        assert!(
            result.contains("context_file"),
            "Should suggest using context_file"
        );

        // Should list file paths compactly
        for file in &extra_files {
            assert!(
                result.contains(file),
                "Should list file path '{}' in compact output",
                file
            );
        }

        // Should NOT contain full context markers (callers, callees, body sections)
        assert!(
            !result.contains("Callers"),
            "Should NOT build full context for 6+ matches"
        );
        assert!(
            !result.contains("Callees"),
            "Should NOT build full context for 6+ matches"
        );
    }

    #[test]
    fn test_deep_dive_query_shows_full_context_at_threshold() {
        let (_tmp, mut db) = setup_db();

        // Create exactly 5 symbols (at the threshold — should get full context)
        let files = ["src/engine.rs", "src/main.rs", "src/handler.rs"];
        let extra_files = ["src/a2.rs", "src/b2.rs"];
        for file in &extra_files {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 100,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 1,
                line_count: 0,
                content: None,
            })
            .unwrap();
        }

        let all_files: Vec<&str> = files.iter().chain(extra_files.iter()).copied().collect();
        let symbols: Vec<Symbol> = all_files
            .iter()
            .enumerate()
            .map(|(i, file)| {
                make_symbol(
                    &format!("sym-proc-{}", i),
                    "process",
                    SymbolKind::Function,
                    file,
                    10,
                    None,
                    Some("pub fn process()"),
                    Some(Visibility::Public),
                    None,
                )
            })
            .collect();
        db.store_symbols(&symbols).unwrap();

        // 5 matches — at threshold, should get full context (not compact list)
        let result = deep_dive_query(&db, "process", None, "overview", 10, 10).unwrap();

        assert!(
            result.contains("Found 5 definitions"),
            "Should report 5 definitions, got: {}",
            result
        );
        // Full context includes the definition header with signature
        assert!(
            result.contains("pub fn process()"),
            "Should include full context with signature at threshold of 5"
        );
    }

    // === Semantic similarity tests ===

    #[test]
    fn test_similar_symbols_at_full_depth() {
        let (_tmp, mut db) = setup_db();

        // Create two symbols with close embeddings
        let sym_a = make_symbol(
            "sym-a",
            "process_data",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            Some("fn process_data()"),
            Some(Visibility::Public),
            None,
        );
        let sym_b = make_symbol(
            "sym-b",
            "handle_data",
            SymbolKind::Function,
            "src/handler.rs",
            20,
            None,
            Some("fn handle_data()"),
            Some(Visibility::Public),
            None,
        );
        db.store_symbols(&[sym_a.clone(), sym_b.clone()]).unwrap();

        // Store close embeddings (small distance = high similarity)
        let emb_a: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
        let mut emb_b = emb_a.clone();
        // Nudge slightly so they're close but not identical
        emb_b[0] += 0.001;
        emb_b[1] += 0.001;
        db.store_embeddings(&[("sym-a".to_string(), emb_a), ("sym-b".to_string(), emb_b)])
            .unwrap();

        let ctx = build_symbol_context(&db, &sym_a, "full", 10, 10).unwrap();
        assert_eq!(ctx.similar.len(), 1, "Should find 1 similar symbol");
        assert_eq!(ctx.similar[0].symbol.name, "handle_data");
        assert!(ctx.similar[0].score > 0.0, "Score should be positive");
        assert!(ctx.similar[0].score <= 1.0, "Score should be <= 1.0");
    }

    #[test]
    fn test_similar_symbols_skipped_when_no_embeddings() {
        let (_tmp, mut db) = setup_db();

        let sym = make_symbol(
            "sym-no-emb",
            "lonely_func",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            Some("fn lonely_func()"),
            Some(Visibility::Public),
            None,
        );
        db.store_symbols(&[sym.clone()]).unwrap();
        // No embeddings stored at all

        let ctx = build_symbol_context(&db, &sym, "full", 10, 10).unwrap();
        assert!(
            ctx.similar.is_empty(),
            "Should be empty when no embeddings exist"
        );
    }

    #[test]
    fn test_similar_symbols_excludes_self() {
        let (_tmp, mut db) = setup_db();

        let sym = make_symbol(
            "sym-self",
            "self_func",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            Some("fn self_func()"),
            Some(Visibility::Public),
            None,
        );
        db.store_symbols(&[sym.clone()]).unwrap();

        // Store embedding for this symbol only
        let emb: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
        db.store_embeddings(&[("sym-self".to_string(), emb)])
            .unwrap();

        let ctx = build_symbol_context(&db, &sym, "full", 10, 10).unwrap();
        // The symbol should NOT appear in its own similar results
        assert!(
            ctx.similar.is_empty(),
            "Should not include self in similar results"
        );
    }

    #[test]
    fn test_similar_symbols_at_context_depth() {
        let (_tmp, mut db) = setup_db();

        let sym_a = make_symbol(
            "sym-c",
            "func_alpha",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            Some("fn func_alpha()"),
            Some(Visibility::Public),
            None,
        );
        let sym_b = make_symbol(
            "sym-d",
            "func_beta",
            SymbolKind::Function,
            "src/handler.rs",
            20,
            None,
            Some("fn func_beta()"),
            Some(Visibility::Public),
            None,
        );
        db.store_symbols(&[sym_a.clone(), sym_b.clone()]).unwrap();

        let emb_a: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
        let emb_b = emb_a.clone();
        db.store_embeddings(&[("sym-c".to_string(), emb_a), ("sym-d".to_string(), emb_b)])
            .unwrap();

        // At "context" depth, similar SHOULD be populated
        let ctx_context = build_symbol_context(&db, &sym_a, "context", 10, 10).unwrap();
        assert!(
            !ctx_context.similar.is_empty(),
            "similar should be populated at context depth"
        );

        // At "overview" depth, similar should NOT be populated
        let ctx_overview = build_symbol_context(&db, &sym_a, "overview", 10, 10).unwrap();
        assert!(
            ctx_overview.similar.is_empty(),
            "similar should be empty at overview depth"
        );
    }

    #[test]
    fn test_build_context_populates_test_refs_at_context_depth() {
        let (_tmp, mut db) = setup_db();

        let symbols = vec![make_symbol(
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
        db.store_symbols(&symbols).unwrap();

        // Identifier in a test file
        insert_identifier(
            &db,
            "process",
            "call",
            "src/tests/search_tests.rs",
            42,
            None,
        );

        // At "context" depth, test_refs SHOULD be populated
        let ctx = build_symbol_context(&db, &symbols[0], "context", 10, 10).unwrap();
        assert_eq!(
            ctx.test_refs.len(),
            1,
            "context depth should populate test_refs"
        );
        assert_eq!(ctx.test_refs[0].file_path, "src/tests/search_tests.rs");

        // At "overview" depth, test_refs should NOT be populated
        let ctx_overview = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();
        assert!(
            ctx_overview.test_refs.is_empty(),
            "overview should not populate test_refs"
        );
    }

    #[test]
    fn test_build_context_uses_test_symbol_metadata_for_test_refs() {
        let (_tmp, mut db) = setup_db();

        db.store_file_info(&FileInfo {
            path: "integration/auth_flow.rs".to_string(),
            language: "rust".to_string(),
            hash: "hash_integration_auth_flow".to_string(),
            size: 500,
            last_modified: 1_000_000,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 0,
            content: None,
        })
        .unwrap();

        let target = make_symbol(
            "sym-target",
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            None,
            None,
            None,
        );
        let mut test_symbol = make_symbol(
            "sym-test",
            "auth_flow_succeeds",
            SymbolKind::Function,
            "integration/auth_flow.rs",
            40,
            None,
            Some("fn auth_flow_succeeds()"),
            None,
            None,
        );
        test_symbol.metadata = Some(HashMap::from([(
            "is_test".to_string(),
            serde_json::Value::Bool(true),
        )]));
        db.store_symbols(&[target.clone(), test_symbol]).unwrap();

        insert_identifier(
            &db,
            "process",
            "call",
            "integration/auth_flow.rs",
            42,
            Some("sym-test"),
        );

        let ctx = build_symbol_context(&db, &target, "context", 10, 10).unwrap();
        assert_eq!(
            ctx.test_refs.len(),
            1,
            "test_refs should honor containing symbol metadata even when the file path lacks test markers"
        );
        assert_eq!(ctx.test_refs[0].file_path, "integration/auth_flow.rs");
        assert_eq!(
            ctx.test_refs[0].symbol.as_ref().unwrap().name,
            "auth_flow_succeeds"
        );
    }

    // === Same-file overload auto-selection tests ===

    #[test]
    fn test_deep_dive_auto_selects_class_from_same_file_overloads() {
        let (_tmp, mut db) = setup_db();

        // Register a C++ header file
        db.store_file_info(&FileInfo {
            path: "include/foo.hpp".to_string(),
            language: "cpp".to_string(),
            hash: "hash_foo_hpp".to_string(),
            size: 5000,
            last_modified: 1000000,
            last_indexed: 0,
            symbol_count: 8,
            line_count: 0,
            content: None,
        })
        .unwrap();

        // Create 8 symbols named "Foo" in the same file:
        // 1 class definition + 7 constructor overloads (Function kind)
        let class_sym = make_symbol(
            "sym-foo-class",
            "Foo",
            SymbolKind::Class,
            "include/foo.hpp",
            77,
            None,
            Some("class Foo"),
            Some(Visibility::Public),
            None,
        );

        let mut all_symbols = vec![class_sym];
        for i in 0..7 {
            all_symbols.push(make_symbol(
                &format!("sym-foo-ctor-{}", i),
                "Foo",
                SymbolKind::Function,
                "include/foo.hpp",
                100 + i * 20,
                Some("sym-foo-class"),
                Some(&format!("Foo(arg{})", i)),
                Some(Visibility::Public),
                None,
            ));
        }
        db.store_symbols(&all_symbols).unwrap();

        // Call deep_dive_query — 8 symbols > DISAMBIGUATION_THRESHOLD (5), all in same file
        // Should auto-select the class, not return disambiguation list
        let result = deep_dive_query(&db, "Foo", None, "overview", 10, 10).unwrap();

        // Should contain the auto-selection note
        assert!(
            result.contains("Auto-selected"),
            "Should contain auto-selection note, got:\n{}",
            result
        );

        // Should contain the class symbol's signature (proof we picked the class)
        assert!(
            result.contains("class Foo"),
            "Should show the class definition signature, got:\n{}",
            result
        );

        // Should contain the class's file:line location
        assert!(
            result.contains("include/foo.hpp:77"),
            "Should show class location, got:\n{}",
            result
        );

        // Should NOT contain the disambiguation prompt
        assert!(
            !result.contains("Use context_file to disambiguate"),
            "Should NOT ask for disambiguation when auto-selecting, got:\n{}",
            result
        );
    }

    #[test]
    fn test_deep_dive_still_disambiguates_when_results_span_multiple_files() {
        let (_tmp, mut db) = setup_db();

        // Register extra files
        let files = [
            "src/engine.rs",
            "src/main.rs",
            "src/handler.rs",
            "lib/a.rs",
            "lib/b.rs",
            "lib/c.rs",
        ];
        for file in &files {
            // Some files already registered by setup_db; store_file_info is idempotent
            let _ = db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 100,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 1,
                line_count: 0,
                content: None,
            });
        }

        // Create 6 symbols in 6 different files — no file dominates
        let symbols: Vec<Symbol> = files
            .iter()
            .enumerate()
            .map(|(i, file)| {
                make_symbol(
                    &format!("sym-multi-{}", i),
                    "handle",
                    SymbolKind::Function,
                    file,
                    10,
                    None,
                    Some("fn handle()"),
                    Some(Visibility::Public),
                    None,
                )
            })
            .collect();
        db.store_symbols(&symbols).unwrap();

        // 6 symbols across 6 files — should get normal disambiguation list
        let result = deep_dive_query(&db, "handle", None, "overview", 10, 10).unwrap();

        assert!(
            result.contains("Use context_file to disambiguate"),
            "Should ask for disambiguation when results span multiple files, got:\n{}",
            result
        );
        assert!(
            !result.contains("Auto-selected"),
            "Should NOT auto-select when results span multiple files, got:\n{}",
            result
        );
    }
}
