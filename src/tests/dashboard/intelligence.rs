use crate::dashboard::routes::intelligence::{
    compute_donut_segments, format_duration_ms, format_number, generate_story_cards, kind_css_var,
};
use crate::database::SymbolDatabase;
use crate::database::analytics::{AggregateStats, CentralitySymbol, FileHotspot};
use crate::database::types::FileInfo;
use std::collections::HashMap;
use tempfile::TempDir;

fn test_db() -> (TempDir, SymbolDatabase) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();
    (tmp, db)
}

fn make_file(path: &str, language: &str, line_count: i64, size: i64) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: language.to_string(),
        hash: format!("hash_{}", path),
        size,
        last_modified: 1000000,
        last_indexed: 0,
        symbol_count: 0,
        line_count: line_count as i32,
        content: None,
    }
}

// --- get_top_symbols_by_centrality ---

#[test]
fn test_get_top_symbols_by_centrality_returns_empty_for_empty_db() {
    let (_tmp, db) = test_db();
    let result = db.get_top_symbols_by_centrality(10).unwrap();
    assert!(result.is_empty(), "expected empty result for empty db");
}

#[test]
fn test_get_top_symbols_by_centrality_excludes_zero_scores() {
    let (_tmp, db) = test_db();

    db.store_file_info(&make_file("src/main.rs", "rust", 100, 1000))
        .unwrap();
    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES ('sym1', 'zero_score_fn', 'function', 'rust', 'src/main.rs', 0.0, 1, 10, 0, 0, 0, 100, 0)",
        [],
    ).unwrap();

    let result = db.get_top_symbols_by_centrality(10).unwrap();
    assert!(
        result.is_empty(),
        "symbols with reference_score=0 must be excluded"
    );
}

#[test]
fn test_get_top_symbols_by_centrality_returns_ordered_by_score() {
    let (_tmp, db) = test_db();

    db.store_file_info(&make_file("src/lib.rs", "rust", 200, 2000))
        .unwrap();

    // Insert symbols directly with known reference_score values
    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, signature, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 10, 0, 0, 0, 100, 0)",
        rusqlite::params!["s1", "low_fn", "function", "rust", "src/lib.rs", "fn low_fn()", 1.5_f64],
    ).unwrap();
    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, signature, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 10, 0, 0, 0, 100, 0)",
        rusqlite::params!["s2", "high_fn", "function", "rust", "src/lib.rs", "fn high_fn()", 9.0_f64],
    ).unwrap();
    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, signature, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 10, 0, 0, 0, 100, 0)",
        rusqlite::params!["s3", "mid_fn", "function", "rust", "src/lib.rs", "fn mid_fn()", 4.0_f64],
    ).unwrap();

    let result = db.get_top_symbols_by_centrality(10).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].name, "high_fn", "highest score must come first");
    assert_eq!(result[1].name, "mid_fn", "second highest must be second");
    assert_eq!(
        result[2].name, "low_fn",
        "lowest non-zero score must be last"
    );
}

#[test]
fn test_get_top_symbols_by_centrality_respects_limit() {
    let (_tmp, db) = test_db();

    db.store_file_info(&make_file("src/lib.rs", "rust", 100, 1000))
        .unwrap();

    for i in 1..=5u32 {
        db.conn.execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, signature, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 10, 0, 0, 0, 100, 0)",
            rusqlite::params![
                format!("s{}", i),
                format!("fn_{}", i),
                "function",
                "rust",
                "src/lib.rs",
                format!("fn fn_{}()", i),
                i as f64,
            ],
        ).unwrap();
    }

    let result = db.get_top_symbols_by_centrality(3).unwrap();
    assert_eq!(result.len(), 3, "limit of 3 must return exactly 3 results");
}

#[test]
fn test_get_top_symbols_by_centrality_fields_populated() {
    let (_tmp, db) = test_db();

    db.store_file_info(&make_file("src/core.rs", "rust", 100, 1000))
        .unwrap();
    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, signature, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 10, 0, 0, 0, 100, 0)",
        rusqlite::params!["s1", "core_fn", "function", "rust", "src/core.rs", "fn core_fn()", 5.0_f64],
    ).unwrap();

    let result = db.get_top_symbols_by_centrality(1).unwrap();
    assert_eq!(result.len(), 1);
    let sym: &CentralitySymbol = &result[0];
    assert_eq!(sym.name, "core_fn");
    assert_eq!(sym.kind, "function");
    assert_eq!(sym.language, "rust");
    assert_eq!(sym.file_path, "src/core.rs");
    assert_eq!(sym.signature, Some("fn core_fn()".to_string()));
    assert!(
        (sym.reference_score - 5.0).abs() < 1e-9,
        "reference_score must be 5.0"
    );
}

// --- get_file_hotspots ---

#[test]
fn test_get_file_hotspots_empty_db() {
    let (_tmp, db) = test_db();
    let result = db.get_file_hotspots(10).unwrap();
    assert!(result.is_empty(), "expected empty result for empty db");
}

#[test]
fn test_get_file_hotspots_returns_ordered_by_composite_score() {
    let (_tmp, db) = test_db();

    // Composite = line_count + symbol_count * 10
    // Files with 0 symbols are excluded (data files, not code)
    // File A: 50 lines, 1 symbol = 60
    // File B: 20 lines, 5 symbols = 70  <- highest
    // File C: 100 lines, 2 symbols = 120 <- actually highest
    db.store_file_info(&make_file("src/a.rs", "rust", 50, 500))
        .unwrap();
    db.store_file_info(&make_file("src/b.rs", "rust", 20, 200))
        .unwrap();
    db.store_file_info(&make_file("src/c.rs", "rust", 100, 1000))
        .unwrap();

    // Add 1 symbol to a.rs (score: 50 + 10 = 60)
    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES (?1, ?2, ?3, ?4, ?5, 0.0, 1, 10, 0, 0, 0, 100, 0)",
        rusqlite::params!["s1", "fn_a", "function", "rust", "src/a.rs"],
    ).unwrap();

    // Add 5 symbols to b.rs (score: 20 + 50 = 70)
    for i in 1..=5u32 {
        db.conn.execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
             VALUES (?1, ?2, ?3, ?4, ?5, 0.0, 1, 10, 0, 0, 0, 100, 0)",
            rusqlite::params![format!("sb{}", i), format!("fn_b{}", i), "function", "rust", "src/b.rs"],
        ).unwrap();
    }

    // Add 2 symbols to c.rs (score: 100 + 20 = 120)
    for i in 1..=2u32 {
        db.conn.execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
             VALUES (?1, ?2, ?3, ?4, ?5, 0.0, 1, 10, 0, 0, 0, 100, 0)",
            rusqlite::params![format!("sc{}", i), format!("fn_c{}", i), "function", "rust", "src/c.rs"],
        ).unwrap();
    }

    let result = db.get_file_hotspots(10).unwrap();
    assert_eq!(result.len(), 3);
    // c.rs = 120, b.rs = 70, a.rs = 60
    assert_eq!(
        result[0].path, "src/c.rs",
        "c.rs with 100 lines and 2 symbols should rank first"
    );
    assert_eq!(
        result[1].path, "src/b.rs",
        "b.rs with 5 symbols should rank second"
    );
    assert_eq!(
        result[2].path, "src/a.rs",
        "a.rs with 1 symbol should rank third"
    );
}

#[test]
fn test_get_file_hotspots_respects_limit() {
    let (_tmp, db) = test_db();

    for i in 1..=5u32 {
        db.store_file_info(&make_file(
            &format!("src/file{}.rs", i),
            "rust",
            (i * 10) as i64,
            1000,
        ))
        .unwrap();
        // Each file needs at least one symbol to not be filtered out
        db.conn.execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
             VALUES (?1, ?2, 'function', 'rust', ?3, 0.0, 1, 10, 0, 0, 0, 100, 0)",
            rusqlite::params![format!("sl{}", i), format!("fn_{}", i), format!("src/file{}.rs", i)],
        ).unwrap();
    }

    let result = db.get_file_hotspots(2).unwrap();
    assert_eq!(result.len(), 2, "limit of 2 must return exactly 2 results");
}

#[test]
fn test_get_file_hotspots_fields_populated() {
    let (_tmp, db) = test_db();

    db.store_file_info(&make_file("src/hot.rs", "rust", 200, 4096))
        .unwrap();
    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES ('sym1', 'hot_fn', 'function', 'rust', 'src/hot.rs', 0.0, 1, 10, 0, 0, 0, 100, 0)",
        [],
    ).unwrap();

    let result = db.get_file_hotspots(1).unwrap();
    assert_eq!(result.len(), 1);
    let hotspot: &FileHotspot = &result[0];
    assert_eq!(hotspot.path, "src/hot.rs");
    assert_eq!(hotspot.language, "rust");
    assert_eq!(hotspot.line_count, 200);
    assert_eq!(hotspot.size, 4096);
    assert_eq!(hotspot.symbol_count, 1);
}

#[test]
fn test_get_file_hotspots_null_line_count_treated_as_zero() {
    let (_tmp, db) = test_db();

    // Insert a file with NULL line_count directly (shouldn't happen via FileInfo but
    // verify robustness of the query's COALESCE/IFNULL handling)
    db.conn.execute(
        "INSERT INTO files (path, language, hash, size, last_modified, last_indexed, symbol_count, line_count)
         VALUES ('src/no_lines.rs', 'rust', 'hash_x', 500, 0, 0, 0, NULL)",
        [],
    ).unwrap();
    // File needs at least one symbol to appear in hotspots
    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES ('snl1', 'fn_no_lines', 'function', 'rust', 'src/no_lines.rs', 0.0, 1, 10, 0, 0, 0, 100, 0)",
        [],
    ).unwrap();

    let result = db.get_file_hotspots(10).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].line_count, 0,
        "NULL line_count should be treated as 0"
    );
}

// --- get_aggregate_stats ---

#[test]
fn test_get_aggregate_stats_empty_db() {
    let (_tmp, db) = test_db();
    let stats = db.get_aggregate_stats().unwrap();
    assert_eq!(stats.total_files, 0);
    assert_eq!(stats.total_symbols, 0);
    assert_eq!(stats.total_lines, 0);
    assert_eq!(stats.total_relationships, 0);
    assert_eq!(stats.language_count, 0);
}

#[test]
fn test_get_aggregate_stats_counts_files_and_symbols() {
    let (_tmp, db) = test_db();

    db.store_file_info(&make_file("src/a.rs", "rust", 100, 1000))
        .unwrap();
    db.store_file_info(&make_file("src/b.rs", "rust", 50, 500))
        .unwrap();
    db.store_file_info(&make_file("lib/main.py", "python", 200, 2000))
        .unwrap();

    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES ('s1', 'fn_a', 'function', 'rust', 'src/a.rs', 0.0, 1, 10, 0, 0, 0, 100, 0)",
        [],
    ).unwrap();
    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES ('s2', 'fn_b', 'function', 'rust', 'src/b.rs', 0.0, 1, 10, 0, 0, 0, 100, 0)",
        [],
    ).unwrap();

    let stats = db.get_aggregate_stats().unwrap();
    assert_eq!(stats.total_files, 3);
    assert_eq!(stats.total_symbols, 2);
    assert_eq!(stats.total_lines, 350, "100 + 50 + 200 = 350");
    assert_eq!(
        stats.language_count, 2,
        "rust + python = 2 distinct languages"
    );
}

#[test]
fn test_get_aggregate_stats_total_lines_sums_line_count() {
    let (_tmp, db) = test_db();

    db.store_file_info(&make_file("a.rs", "rust", 30, 100))
        .unwrap();
    db.store_file_info(&make_file("b.rs", "rust", 70, 200))
        .unwrap();

    let stats = db.get_aggregate_stats().unwrap();
    assert_eq!(stats.total_lines, 100, "30 + 70 = 100");
}

#[test]
fn test_get_aggregate_stats_language_count_ignores_empty_language() {
    let (_tmp, db) = test_db();

    db.store_file_info(&make_file("a.rs", "rust", 10, 100))
        .unwrap();
    // Insert a file with empty language directly
    db.conn.execute(
        "INSERT INTO files (path, language, hash, size, last_modified, last_indexed, symbol_count, line_count)
         VALUES ('unknown_file', '', 'hash_empty', 100, 0, 0, 0, 5)",
        [],
    ).unwrap();

    let stats = db.get_aggregate_stats().unwrap();
    // Files with empty language must NOT count toward language_count
    assert_eq!(
        stats.language_count, 1,
        "only 'rust' counts; empty string excluded"
    );
    // But total_files should include all files
    assert_eq!(stats.total_files, 2);
}

#[test]
fn test_get_aggregate_stats_counts_relationships() {
    let (_tmp, db) = test_db();

    db.store_file_info(&make_file("src/a.rs", "rust", 10, 100))
        .unwrap();

    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES ('s1', 'fn_a', 'function', 'rust', 'src/a.rs', 0.0, 1, 10, 0, 0, 0, 100, 0)",
        [],
    ).unwrap();
    db.conn.execute(
        "INSERT INTO symbols (id, name, kind, language, file_path, reference_score, start_line, end_line, start_col, end_col, start_byte, end_byte, last_indexed)
         VALUES ('s2', 'fn_b', 'function', 'rust', 'src/a.rs', 0.0, 2, 20, 0, 0, 0, 100, 0)",
        [],
    ).unwrap();

    db.conn.execute(
        "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
         VALUES ('r1', 's1', 's2', 'calls', 'src/a.rs', 5)",
        [],
    ).unwrap();
    db.conn.execute(
        "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
         VALUES ('r2', 's2', 's1', 'calls', 'src/a.rs', 15)",
        [],
    ).unwrap();

    let stats = db.get_aggregate_stats().unwrap();
    assert_eq!(stats.total_relationships, 2);
}

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

    // Card 2: largest file
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
