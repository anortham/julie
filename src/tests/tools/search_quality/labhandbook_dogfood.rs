//! LabHandbookV2 dogfood metric scaffolding.
//!
//! These helpers are intentionally pure and fast so we can iterate on query
//! sets and ranking behavior without requiring a real workspace fixture.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RetrievedHit {
    symbol_id: String,
    language: String,
    off_topic: bool,
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
    expected_cross_lang_symbol_ids: &[&str],
    ranked_hits: &[RetrievedHit],
) -> f64 {
    if expected_cross_lang_symbol_ids.is_empty() {
        return 0.0;
    }

    let expected: HashSet<&str> = expected_cross_lang_symbol_ids.iter().copied().collect();
    let found: HashSet<&str> = ranked_hits
        .iter()
        .take(5)
        .filter(|hit| hit.language != source_language)
        .filter_map(|hit| {
            let symbol_id = hit.symbol_id.as_str();
            expected.contains(symbol_id).then_some(symbol_id)
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

    let score = cross_lang_recall_at_5("rust", &["py_target", "ts_target", "rust_target"], &hits);
    assert!((score - (2.0 / 3.0)).abs() < f64::EPSILON);
}

#[test]
fn test_cross_lang_recall_at_5_deduplicates_symbols() {
    let hits = vec![
        RetrievedHit::new("py_target", "python", false),
        RetrievedHit::new("py_target", "python", false),
        RetrievedHit::new("ts_target", "typescript", false),
    ];

    let score = cross_lang_recall_at_5("rust", &["py_target", "ts_target"], &hits);
    assert_eq!(score, 1.0);
}

#[test]
#[ignore = "Requires real LabHandbookV2 reference workspace and live search pipeline"]
fn test_labhandbook_v2_integration_skeleton() {
    // Intentional scaffold for future integration wiring:
    // 1) Load fixture queries from fixtures/benchmarks/labhandbookv2_dogfood_queries.jsonl.
    // 2) Execute each query through fast_search against LabHandbookV2 workspace.
    // 3) Compute aggregate Hit@k, MRR@10, OffTopic@5, and CrossLangRecall@5.
    // 4) Assert minimum thresholds once baseline is calibrated.
    panic!("integration skeleton only");
}
