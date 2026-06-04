use super::*;

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
fn test_callable_groups_same_file_callers() {
    let sym = make_symbol(
        "process",
        SymbolKind::Function,
        "src/engine.rs",
        42,
        None,
        None,
        None,
    );
    let caller_one = make_symbol(
        "main",
        SymbolKind::Function,
        "src/main.rs",
        10,
        None,
        None,
        None,
    );
    let caller_two = make_symbol(
        "retry",
        SymbolKind::Function,
        "src/main.rs",
        20,
        None,
        None,
        None,
    );
    let mut ctx = empty_context(sym);
    ctx.incoming = vec![
        make_ref(RelationshipKind::Calls, "src/main.rs", 15, Some(caller_one)),
        make_ref(RelationshipKind::Calls, "src/main.rs", 25, Some(caller_two)),
    ];
    ctx.incoming_total = 2;

    let output = format_symbol_context(&ctx, "overview");

    assert_eq!(
        output.matches("src/main.rs").count(),
        1,
        "same-file callers should print the file path once: {output}",
    );
    assert!(
        output.contains("src/main.rs:"),
        "missing grouped file header: {output}"
    );
    assert!(
        output.contains(":15  main (Calls)"),
        "missing first caller: {output}"
    );
    assert!(
        output.contains(":25  retry (Calls)"),
        "missing second caller: {output}"
    );
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
