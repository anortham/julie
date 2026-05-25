use super::*;

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
