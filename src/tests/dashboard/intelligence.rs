use crate::dashboard::routes::intelligence::{
    compute_donut_segments, format_duration_ms, format_number, generate_story_cards, kind_css_var,
    AggregateStats, CentralitySymbol, FileHotspot,
};
use std::collections::HashMap;

// --- kind_css_var ---

#[test]
fn test_kind_css_var_known_kinds() {
    assert_eq!(kind_css_var("function"), "--kind-function");
    assert_eq!(kind_css_var("method"), "--kind-method");
    assert_eq!(kind_css_var("struct"), "--kind-struct");
    assert_eq!(kind_css_var("class"), "--kind-class");
    assert_eq!(kind_css_var("trait"), "--kind-trait");
    assert_eq!(kind_css_var("interface"), "--kind-interface");
    assert_eq!(kind_css_var("enum"), "--kind-enum");
    assert_eq!(kind_css_var("enum_member"), "--kind-enum");
    assert_eq!(kind_css_var("type"), "--kind-type");
    assert_eq!(kind_css_var("constant"), "--kind-constant");
    assert_eq!(kind_css_var("variable"), "--kind-variable");
    assert_eq!(kind_css_var("module"), "--kind-module");
    assert_eq!(kind_css_var("namespace"), "--kind-namespace");
    assert_eq!(kind_css_var("property"), "--kind-property");
    assert_eq!(kind_css_var("field"), "--kind-property");
    assert_eq!(kind_css_var("import"), "--kind-import");
    assert_eq!(kind_css_var("export"), "--kind-import");
}

#[test]
fn test_kind_css_var_case_insensitive() {
    assert_eq!(kind_css_var("Function"), "--kind-function");
    assert_eq!(kind_css_var("STRUCT"), "--kind-struct");
    assert_eq!(kind_css_var("Method"), "--kind-method");
}

#[test]
fn test_kind_css_var_unknown_falls_back() {
    assert_eq!(kind_css_var("unknown_thing"), "--kind-other");
    assert_eq!(kind_css_var(""), "--kind-other");
    assert_eq!(kind_css_var("macro"), "--kind-other");
    assert_eq!(kind_css_var("decorator"), "--kind-other");
}

// --- compute_donut_segments ---

#[test]
fn test_compute_donut_segments_empty() {
    let by_kind: HashMap<String, usize> = HashMap::new();
    let segments = compute_donut_segments(&by_kind);
    assert!(
        segments.is_empty(),
        "empty map should produce empty segments"
    );
}

#[test]
fn test_compute_donut_segments_basic() {
    let mut by_kind = HashMap::new();
    by_kind.insert("function".to_string(), 50);
    by_kind.insert("struct".to_string(), 30);
    by_kind.insert("trait".to_string(), 20);

    let segments = compute_donut_segments(&by_kind);

    assert_eq!(segments.len(), 3, "should produce one segment per kind");

    // Sorted descending: function (50), struct (30), trait (20)
    assert_eq!(segments[0].label, "function");
    assert_eq!(segments[0].count, 50);
    assert_eq!(segments[1].label, "struct");
    assert_eq!(segments[1].count, 30);
    assert_eq!(segments[2].label, "trait");
    assert_eq!(segments[2].count, 20);

    // First segment offset must be 0 (starts at origin)
    assert!(
        segments[0].dash_offset.abs() < 1e-9,
        "first segment dash_offset must be 0.0, got {}",
        segments[0].dash_offset
    );

    // Dash lengths should sum to approximately CIRCUMFERENCE (2*pi*0.7 ≈ 4.3982)
    let total_dash: f64 = segments.iter().map(|s| s.dash_length).sum();
    let circumference = 2.0 * std::f64::consts::PI * 0.7;
    assert!(
        (total_dash - circumference).abs() < 1e-6,
        "dash lengths should sum to circumference ({:.6}), got {:.6}",
        circumference,
        total_dash
    );

    // Percentages should sum to 100.0
    let total_pct: f64 = segments.iter().map(|s| s.percentage).sum();
    assert!(
        (total_pct - 100.0).abs() < 1e-6,
        "percentages should sum to 100, got {:.6}",
        total_pct
    );
}

#[test]
fn test_compute_donut_segments_color_vars_assigned() {
    let mut by_kind = HashMap::new();
    by_kind.insert("function".to_string(), 10);
    by_kind.insert("unknown_kind".to_string(), 5);

    let segments = compute_donut_segments(&by_kind);
    let fn_seg = segments.iter().find(|s| s.label == "function").unwrap();
    let unk_seg = segments.iter().find(|s| s.label == "unknown_kind").unwrap();

    assert_eq!(fn_seg.color_var, "--kind-function");
    assert_eq!(unk_seg.color_var, "--kind-other");
}

// --- generate_story_cards ---

#[test]
fn test_generate_story_cards_produces_expected_cards() {
    let top_symbols = vec![CentralitySymbol {
        name: "my_top_fn".to_string(),
        kind: "function".to_string(),
        language: "rust".to_string(),
        file_path: "src/lib.rs".to_string(),
        signature: None,
        reference_score: 42.5,
    }];

    let hotspots = vec![FileHotspot {
        path: "src/big_file.rs".to_string(),
        language: "rust".to_string(),
        line_count: 800,
        size: 20000,
        symbol_count: 60,
    }];

    let mut by_kind = HashMap::new();
    by_kind.insert("function".to_string(), 80);
    by_kind.insert("struct".to_string(), 20);

    let stats = AggregateStats {
        total_files: 100,
        total_symbols: 500,
        total_lines: 10000,
        total_relationships: 1500,
        language_count: 2,
    };

    let lang_counts = vec![("rust".to_string(), 70_i64), ("python".to_string(), 30_i64)];

    let cards = generate_story_cards(&top_symbols, &hotspots, &by_kind, &stats, &lang_counts);

    assert!(
        cards.len() >= 3 && cards.len() <= 5,
        "expected 3-5 cards, got {}",
        cards.len()
    );

    // Card 1: top symbol
    assert!(
        cards[0].contains("my_top_fn"),
        "first card should mention top symbol name, got: {}",
        cards[0]
    );
    assert!(
        cards[0].contains("42.5"),
        "first card should mention score 42.5, got: {}",
        cards[0]
    );

    // Card 2: most complex file
    assert!(
        cards[1].contains("src/big_file.rs"),
        "second card should mention hotspot path, got: {}",
        cards[1]
    );
    assert!(
        cards[1].contains("800"),
        "second card should mention line count, got: {}",
        cards[1]
    );

    // Card 3: dominant language
    assert!(
        cards[2].contains("rust"),
        "third card should mention dominant language, got: {}",
        cards[2]
    );

    // Card 5: total references (only if > 100, which 1500 is)
    let has_refs_card = cards.iter().any(|c| c.contains("1,500"));
    assert!(
        has_refs_card,
        "should have a card about total references (1,500), cards: {:?}",
        cards
    );
}

#[test]
fn test_generate_story_cards_skips_refs_card_when_low() {
    let top_symbols = vec![CentralitySymbol {
        name: "fn_a".to_string(),
        kind: "function".to_string(),
        language: "rust".to_string(),
        file_path: "src/a.rs".to_string(),
        signature: None,
        reference_score: 1.0,
    }];

    let hotspots = vec![FileHotspot {
        path: "src/a.rs".to_string(),
        language: "rust".to_string(),
        line_count: 100,
        size: 1000,
        symbol_count: 5,
    }];

    let mut by_kind = HashMap::new();
    by_kind.insert("function".to_string(), 10);

    let stats = AggregateStats {
        total_files: 5,
        total_symbols: 10,
        total_lines: 100,
        total_relationships: 50, // <= 100, no refs card
        language_count: 1,
    };

    let lang_counts = vec![("rust".to_string(), 5_i64)];

    let cards = generate_story_cards(&top_symbols, &hotspots, &by_kind, &stats, &lang_counts);

    let has_refs_card = cards
        .iter()
        .any(|c| c.to_lowercase().contains("references tracked"));
    assert!(
        !has_refs_card,
        "should NOT have a references card when total_relationships <= 100, cards: {:?}",
        cards
    );
}

// --- format_number ---

#[test]
fn test_format_number() {
    assert_eq!(format_number(0), "0");
    assert_eq!(format_number(999), "999");
    assert_eq!(format_number(1000), "1,000");
    assert_eq!(format_number(12847), "12,847");
    assert_eq!(format_number(1_000_000), "1,000,000");
    assert_eq!(format_number(-12847), "-12,847");
}

// --- format_duration_ms ---

#[test]
fn test_format_duration_ms() {
    assert_eq!(format_duration_ms(0), "0ms");
    assert_eq!(format_duration_ms(500), "500ms");
    assert_eq!(format_duration_ms(999), "999ms");
    assert_eq!(format_duration_ms(1000), "1.0s");
    assert_eq!(format_duration_ms(1500), "1.5s");
    assert_eq!(format_duration_ms(59_999), "60.0s");
    assert_eq!(format_duration_ms(60_000), "1m 0.0s");
    assert_eq!(format_duration_ms(90_000), "1m 30.0s");
    assert_eq!(format_duration_ms(125_500), "2m 5.5s");
}
