//! LabHandbookV2 dogfood metric scaffolding.
//!
//! These helpers are intentionally pure and fast so we can iterate on query
//! sets and ranking behavior without requiring a real workspace fixture.

use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct DogfoodQueryRow {
    query: String,
    intent_tag: String,
    expected_symbol_ids: Option<Vec<String>>,
}

fn load_dogfood_queries_from_jsonl(
    path: &Path,
) -> Result<Vec<DogfoodQueryRow>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();

    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        let row: DogfoodQueryRow = serde_json::from_str(trimmed).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Failed to parse dogfood query JSONL at {}:{}: {}",
                    path.display(),
                    line_number,
                    error
                ),
            )
        })?;
        rows.push(row);
    }

    Ok(rows)
}

fn write_temp_jsonl_fixture(contents: &str) -> PathBuf {
    let unique_suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time is before UNIX epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "julie_labhandbook_dogfood_loader_{}_{}.jsonl",
        std::process::id(),
        unique_suffix
    ));

    fs::write(&path, contents).expect("Failed to write temp dogfood JSONL fixture");
    path
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RetrievedHit {
    symbol_id: String,
    language: String,
    off_topic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ExpectedCrossLangSymbol<'a> {
    symbol_id: &'a str,
    language: &'a str,
}

impl RetrievedHit {
    fn new(symbol_id: &str, language: &str, off_topic: bool) -> Self {
        Self {
            symbol_id: symbol_id.to_string(),
            language: language.to_string(),
            off_topic,
        }
    }
}

impl<'a> ExpectedCrossLangSymbol<'a> {
    fn new(symbol_id: &'a str, language: &'a str) -> Self {
        Self {
            symbol_id,
            language,
        }
    }
}

fn hit_at_k(expected_symbol_ids: &[&str], ranked_hits: &[RetrievedHit], k: usize) -> f64 {
    if expected_symbol_ids.is_empty() || ranked_hits.is_empty() || k == 0 {
        return 0.0;
    }

    let expected: HashSet<&str> = expected_symbol_ids.iter().copied().collect();
    if ranked_hits
        .iter()
        .take(k)
        .any(|hit| expected.contains(hit.symbol_id.as_str()))
    {
        1.0
    } else {
        0.0
    }
}

fn mrr_at_10(expected_symbol_ids: &[&str], ranked_hits: &[RetrievedHit]) -> f64 {
    if expected_symbol_ids.is_empty() || ranked_hits.is_empty() {
        return 0.0;
    }

    let expected: HashSet<&str> = expected_symbol_ids.iter().copied().collect();
    ranked_hits
        .iter()
        .take(10)
        .position(|hit| expected.contains(hit.symbol_id.as_str()))
        .map(|index| 1.0 / (index as f64 + 1.0))
        .unwrap_or(0.0)
}

fn off_topic_at_5(ranked_hits: &[RetrievedHit]) -> f64 {
    let top_k = ranked_hits.len().min(5);
    if top_k == 0 {
        return 0.0;
    }

    let off_topic_count = ranked_hits
        .iter()
        .take(top_k)
        .filter(|hit| hit.off_topic)
        .count();

    off_topic_count as f64 / top_k as f64
}

fn cross_lang_recall_at_5(
    source_language: &str,
    expected_cross_lang_symbols: &[ExpectedCrossLangSymbol<'_>],
    ranked_hits: &[RetrievedHit],
) -> f64 {
    // Semantics:
    // - Denominator: unique expected (symbol_id, language) pairs that are cross-language
    //   relative to the source language.
    // - Numerator: how many of those expected cross-language pairs appear in the top-5,
    //   deduplicated by the same (symbol_id, language) key.
    // This avoids ambiguity when identical symbol IDs exist in multiple languages.
    let expected: HashSet<(&str, &str)> = expected_cross_lang_symbols
        .iter()
        .filter(|expected| expected.language != source_language)
        .map(|expected| (expected.symbol_id, expected.language))
        .collect();
    if expected.is_empty() {
        return 0.0;
    }

    let found: HashSet<(&str, &str)> = ranked_hits
        .iter()
        .take(5)
        .filter(|hit| hit.language != source_language)
        .filter_map(|hit| {
            let key = (hit.symbol_id.as_str(), hit.language.as_str());
            expected.contains(&key).then_some(key)
        })
        .collect();

    found.len() as f64 / expected.len() as f64
}

#[test]
fn test_hit_at_k_detects_relevant_hit_in_top_k() {
    let hits = vec![
        RetrievedHit::new("a", "rust", false),
        RetrievedHit::new("target", "rust", false),
        RetrievedHit::new("b", "rust", false),
    ];

    assert_eq!(hit_at_k(&["target"], &hits, 2), 1.0);
    assert_eq!(hit_at_k(&["target"], &hits, 1), 0.0);
}

#[test]
fn test_hit_at_k_returns_zero_for_empty_inputs() {
    assert_eq!(hit_at_k(&[], &[], 5), 0.0);
    assert_eq!(hit_at_k(&["x"], &[], 5), 0.0);
    assert_eq!(
        hit_at_k(&["x"], &[RetrievedHit::new("x", "rust", false)], 0),
        0.0
    );
}

#[test]
fn test_mrr_at_10_uses_first_relevant_rank() {
    let hits = vec![
        RetrievedHit::new("noise-1", "rust", false),
        RetrievedHit::new("noise-2", "rust", false),
        RetrievedHit::new("expected", "rust", false),
        RetrievedHit::new("expected-late", "rust", false),
    ];

    let score = mrr_at_10(&["expected", "expected-late"], &hits);
    assert!((score - (1.0 / 3.0)).abs() < f64::EPSILON);
}

#[test]
fn test_mrr_at_10_ignores_hits_after_rank_10() {
    let mut hits = Vec::new();
    for i in 0..12 {
        let id = if i == 10 { "expected" } else { "noise" };
        hits.push(RetrievedHit::new(id, "rust", false));
    }

    assert_eq!(mrr_at_10(&["expected"], &hits), 0.0);
}

#[test]
fn test_off_topic_at_5_uses_ratio_in_top_five() {
    let hits = vec![
        RetrievedHit::new("a", "rust", true),
        RetrievedHit::new("b", "rust", false),
        RetrievedHit::new("c", "rust", true),
        RetrievedHit::new("d", "rust", false),
        RetrievedHit::new("e", "rust", false),
        RetrievedHit::new("f", "rust", true),
    ];

    let score = off_topic_at_5(&hits);
    assert!((score - 0.4).abs() < f64::EPSILON);
}

#[test]
fn test_off_topic_at_5_handles_short_lists() {
    let hits = vec![
        RetrievedHit::new("a", "rust", true),
        RetrievedHit::new("b", "rust", true),
    ];

    assert_eq!(off_topic_at_5(&hits), 1.0);
    assert_eq!(off_topic_at_5(&[]), 0.0);
}

#[test]
fn test_cross_lang_recall_at_5_counts_only_cross_language_hits() {
    let hits = vec![
        RetrievedHit::new("rust_only", "rust", false),
        RetrievedHit::new("py_target", "python", false),
        RetrievedHit::new("ts_target", "typescript", false),
        RetrievedHit::new("other", "go", false),
        RetrievedHit::new("rust_target", "rust", false),
    ];

    let score = cross_lang_recall_at_5(
        "rust",
        &[
            ExpectedCrossLangSymbol::new("py_target", "python"),
            ExpectedCrossLangSymbol::new("ts_target", "typescript"),
            ExpectedCrossLangSymbol::new("rust_target", "rust"),
        ],
        &hits,
    );
    assert_eq!(score, 1.0);
}

#[test]
fn test_cross_lang_recall_at_5_deduplicates_symbols() {
    let hits = vec![
        RetrievedHit::new("py_target", "python", false),
        RetrievedHit::new("py_target", "python", false),
        RetrievedHit::new("ts_target", "typescript", false),
    ];

    let score = cross_lang_recall_at_5(
        "rust",
        &[
            ExpectedCrossLangSymbol::new("py_target", "python"),
            ExpectedCrossLangSymbol::new("ts_target", "typescript"),
        ],
        &hits,
    );
    assert_eq!(score, 1.0);
}

#[test]
fn test_cross_lang_recall_at_5_matches_symbol_and_language_pair() {
    let hits = vec![RetrievedHit::new("shared_symbol", "go", false)];

    let score = cross_lang_recall_at_5(
        "rust",
        &[ExpectedCrossLangSymbol::new("shared_symbol", "python")],
        &hits,
    );
    assert_eq!(score, 0.0);
}

#[test]
fn test_cross_lang_recall_at_5_returns_zero_when_no_cross_language_expectations() {
    let hits = vec![RetrievedHit::new("py_target", "python", false)];

    let score = cross_lang_recall_at_5(
        "rust",
        &[ExpectedCrossLangSymbol::new("rust_target", "rust")],
        &hits,
    );
    assert_eq!(score, 0.0);
}

#[test]
fn test_load_dogfood_queries_from_jsonl_ignores_blank_and_comment_lines() {
    let fixture_path = write_temp_jsonl_fixture(
        "\n# full line comment\n// second comment style\n{\"query\":\"q1\",\"intent_tag\":\"intent-1\"}\n   \n{\"query\":\"q2\",\"intent_tag\":\"intent-2\",\"expected_symbol_ids\":[\"a\",\"b\"]}\n",
    );

    let rows = load_dogfood_queries_from_jsonl(&fixture_path)
        .expect("Loader should parse valid non-comment lines");

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].query, "q1");
    assert_eq!(rows[1].intent_tag, "intent-2");

    fs::remove_file(&fixture_path).expect("Failed to remove temporary fixture");
}

#[test]
fn test_load_dogfood_queries_from_jsonl_supports_optional_expected_symbol_ids() {
    let fixture_path = write_temp_jsonl_fixture(
        "{\"query\":\"with expected\",\"intent_tag\":\"intent-a\",\"expected_symbol_ids\":[\"symbol::a\"]}\n{\"query\":\"without expected\",\"intent_tag\":\"intent-b\"}\n{\"query\":\"null expected\",\"intent_tag\":\"intent-c\",\"expected_symbol_ids\":null}\n",
    );

    let rows = load_dogfood_queries_from_jsonl(&fixture_path)
        .expect("Loader should parse optional expected_symbol_ids values");

    assert_eq!(
        rows[0].expected_symbol_ids.as_deref(),
        Some(&["symbol::a".to_string()][..])
    );
    assert_eq!(rows[1].expected_symbol_ids, None);
    assert_eq!(rows[2].expected_symbol_ids, None);

    fs::remove_file(&fixture_path).expect("Failed to remove temporary fixture");
}

#[test]
fn test_load_dogfood_queries_from_jsonl_returns_error_for_malformed_json() {
    let fixture_path = write_temp_jsonl_fixture(
        "{\"query\":\"ok\",\"intent_tag\":\"intent\"}\n{\"query\":\"broken\"\n",
    );

    let error = load_dogfood_queries_from_jsonl(&fixture_path)
        .expect_err("Loader should fail when a JSONL row is malformed");
    let error_message = error.to_string();
    assert!(error_message.contains(":2:"));

    fs::remove_file(&fixture_path).expect("Failed to remove temporary fixture");
}

#[test]
#[ignore = "Requires real LabHandbookV2 reference workspace and live search pipeline"]
fn test_labhandbook_v2_integration_skeleton() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("benchmarks")
        .join("labhandbookv2_dogfood_queries.jsonl");

    let rows = load_dogfood_queries_from_jsonl(&fixture_path)
        .expect("Fixture loader should parse LabHandbookV2 dogfood queries");
    assert!(
        !rows.is_empty(),
        "LabHandbookV2 dogfood fixture should not be empty"
    );

    // Future integration wiring:
    // 1) Execute each query through fast_search against LabHandbookV2 workspace.
    // 2) Compute aggregate Hit@k, MRR@10, OffTopic@5, and CrossLangRecall@5.
    // 3) Assert minimum thresholds once baseline is calibrated.
}
