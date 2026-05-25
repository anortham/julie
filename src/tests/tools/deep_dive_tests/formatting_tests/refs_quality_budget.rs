use super::*;

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
    metadata.insert("test_role".to_string(), serde_json::json!("test_case"));
    metadata.insert(
        "test_quality".to_string(),
        serde_json::json!({
            "quality_tier": "thorough",
            "confidence": 0.85,
            "assertion_count": 5,
            "mock_count": 1,
            "assertion_density": 0.25
        }),
    );
    sym.metadata = Some(metadata);

    let ctx = empty_context(sym);
    let output = format_symbol_context(&ctx, "full");
    assert!(
        output.contains("[test_case]"),
        "should show test role, got: {}",
        output
    );
    assert!(
        output.contains("[thorough confidence:85%]"),
        "should show quality tier with confidence, got: {}",
        output
    );
}

#[test]
fn test_quality_info_includes_stored_metrics_for_test_symbol_self() {
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
    metadata.insert("test_role".to_string(), serde_json::json!("test_case"));
    metadata.insert(
        "test_quality".to_string(),
        serde_json::json!({
            "quality_tier": "thorough",
            "confidence": 0.85,
            "assertion_count": 5,
            "mock_count": 1,
            "assertion_density": 0.25
        }),
    );
    sym.metadata = Some(metadata);

    let ctx = empty_context(sym);
    let output = format_symbol_context(&ctx, "full");
    assert!(
        output.contains("[test_case]"),
        "should retain test role, got: {}",
        output
    );
    assert!(
        output.contains("[thorough confidence:85%]"),
        "should retain quality tier and confidence, got: {}",
        output
    );
    assert!(
        output.contains("5 assertions"),
        "should show stored assertion count, got: {}",
        output
    );
    assert!(
        output.contains("1 mocks"),
        "should show stored mock count, got: {}",
        output
    );
    assert!(
        output.contains("0.25 density"),
        "should show stored assertion density, got: {}",
        output
    );
}

#[test]
fn test_quality_info_no_confidence_shows_tier_only() {
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
            "quality_tier": "adequate"
        }),
    );
    sym.metadata = Some(metadata);

    let ctx = empty_context(sym);
    let output = format_symbol_context(&ctx, "full");
    // With is_test but no test_role, role defaults to "test"
    assert!(
        output.contains("[test]"),
        "should show default role 'test' when test_role not set, got: {}",
        output
    );
    assert!(
        output.contains("[adequate]"),
        "should show quality tier without confidence, got: {}",
        output
    );
    assert!(
        !output.contains("confidence"),
        "should NOT show confidence when not present, got: {}",
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
        Some("pub fn big_function(a: &BigStruct, b: &mut OtherStruct) -> Result<LongReturnType>"),
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
        incoming_calls_total: 50,
        outgoing: vec![],
        outgoing_total: 0,
        outgoing_calls_total: 0,
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
        incoming_calls_total: 20,
        outgoing: vec![],
        outgoing_total: 0,
        outgoing_calls_total: 0,
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
