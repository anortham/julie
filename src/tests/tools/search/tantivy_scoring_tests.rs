//! Tests for post-search scoring boosts (important_patterns + graph centrality)
//!
//! Verifies that search results whose signatures match language-specific
//! important_patterns (e.g., "pub fn", "public class") get a score boost,
//! and that symbols with higher graph centrality (reference_score) rank higher.

#[cfg(test)]
mod tests {
    use crate::search::index::SymbolSearchResult;
    use crate::search::language_config::LanguageConfigs;
    use crate::search::scoring::{apply_centrality_boost, apply_important_patterns_boost};
    use std::collections::HashMap;

    fn make_result(name: &str, signature: &str, language: &str, score: f32) -> SymbolSearchResult {
        SymbolSearchResult {
            id: format!("test_{}", name),
            name: name.to_string(),
            signature: signature.to_string(),
            doc_comment: String::new(),
            file_path: format!("src/{}.rs", name),
            kind: "function".to_string(),
            language: language.to_string(),
            start_line: 1,
            score,
        }
    }

    #[test]
    fn test_important_patterns_boost_pub_fn() {
        let configs = LanguageConfigs::load_embedded();

        let mut results = vec![
            make_result("process", "fn process()", "rust", 1.0),
            make_result("process", "pub fn process()", "rust", 1.0),
        ];

        apply_important_patterns_boost(&mut results, &configs);

        // "pub fn" matches an important pattern in rust.toml — should be boosted
        assert!(
            results[0].signature.contains("pub fn"),
            "pub fn should rank first after boost: {:?}",
            results.iter().map(|r| &r.signature).collect::<Vec<_>>()
        );
        assert!(
            results[0].score > results[1].score,
            "Boosted result should have higher score: {} vs {}",
            results[0].score,
            results[1].score
        );
    }

    #[test]
    fn test_important_patterns_boost_preserves_order_when_no_match() {
        let configs = LanguageConfigs::load_embedded();

        let mut results = vec![
            make_result("foo", "fn foo()", "rust", 2.0),
            make_result("bar", "fn bar()", "rust", 1.0),
        ];

        apply_important_patterns_boost(&mut results, &configs);

        // Neither matches an important pattern — relative order preserved
        assert_eq!(results[0].name, "foo");
        assert_eq!(results[1].name, "bar");
    }

    #[test]
    fn test_important_patterns_boost_multiple_languages() {
        let configs = LanguageConfigs::load_embedded();

        let mut results = vec![
            make_result("User", "class User", "csharp", 1.0),
            make_result("User", "public class User", "csharp", 1.0),
        ];

        apply_important_patterns_boost(&mut results, &configs);

        // C# "public class" is an important pattern
        assert!(
            results[0].signature.contains("public class"),
            "public class should rank first for C#: {:?}",
            results.iter().map(|r| &r.signature).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_important_patterns_boost_only_once() {
        let configs = LanguageConfigs::load_embedded();

        // A signature that matches multiple patterns should only be boosted once
        let mut results = vec![
            make_result("process", "pub async fn process()", "rust", 1.0),
        ];

        apply_important_patterns_boost(&mut results, &configs);

        // "pub fn" matches, "async fn" matches — but boost only applies once (1.5x)
        assert!(
            (results[0].score - 1.5).abs() < 0.01,
            "Should boost exactly once (1.5x), got: {}",
            results[0].score
        );
    }

    #[test]
    fn test_important_patterns_boost_unknown_language() {
        let configs = LanguageConfigs::load_embedded();

        let mut results = vec![
            make_result("foo", "some signature", "unknown_lang", 1.0),
        ];

        let score_before = results[0].score;
        apply_important_patterns_boost(&mut results, &configs);

        // Unknown language — no config, no boost
        assert_eq!(
            results[0].score, score_before,
            "Unknown language should not be boosted"
        );
    }

    // ============================================================
    // Centrality boost tests
    // ============================================================

    fn make_symbol_result(id: &str, name: &str, score: f32) -> SymbolSearchResult {
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: String::new(),
            doc_comment: String::new(),
            file_path: format!("src/{}.rs", name),
            kind: "function".to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        }
    }

    #[test]
    fn test_centrality_boost_increases_scores() {
        let mut results = vec![
            make_symbol_result("sym1", "foo", 1.0),
            make_symbol_result("sym2", "bar", 1.0),
        ];

        let mut ref_scores = HashMap::new();
        ref_scores.insert("sym1".to_string(), 10.0); // high reference score
        // sym2 has no entry => no boost

        apply_centrality_boost(&mut results, &ref_scores);

        // sym1 should have a boosted score > 1.0
        let sym1 = results.iter().find(|r| r.id == "sym1").unwrap();
        assert!(
            sym1.score > 1.0,
            "Symbol with reference_score=10 should be boosted, got {}",
            sym1.score
        );

        // sym2 should remain at 1.0 (no entry in ref_scores)
        let sym2 = results.iter().find(|r| r.id == "sym2").unwrap();
        assert!(
            (sym2.score - 1.0).abs() < f32::EPSILON,
            "Symbol without reference_score should be unchanged, got {}",
            sym2.score
        );
    }

    #[test]
    fn test_centrality_boost_re_sorts_results() {
        // Initially bar has higher Tantivy score, but foo has more references
        let mut results = vec![
            make_symbol_result("sym1", "foo", 0.5),  // low Tantivy score, high references
            make_symbol_result("sym2", "bar", 1.0),  // high Tantivy score, no references
        ];

        let mut ref_scores = HashMap::new();
        ref_scores.insert("sym1".to_string(), 100.0); // very high reference score

        apply_centrality_boost(&mut results, &ref_scores);

        // After boost, foo should be ranked first (higher score)
        assert_eq!(
            results[0].id, "sym1",
            "Symbol with high reference_score should be re-ranked to top"
        );
        assert!(
            results[0].score > results[1].score,
            "First result ({}) should have higher score than second ({})",
            results[0].score, results[1].score
        );
    }

    #[test]
    fn test_centrality_boost_zero_ref_score_no_effect() {
        let mut results = vec![
            make_symbol_result("sym1", "foo", 1.0),
        ];

        let mut ref_scores = HashMap::new();
        ref_scores.insert("sym1".to_string(), 0.0); // zero reference score

        apply_centrality_boost(&mut results, &ref_scores);

        // Score should remain unchanged when reference_score is 0.0
        assert!(
            (results[0].score - 1.0).abs() < f32::EPSILON,
            "Symbol with zero reference_score should be unchanged, got {}",
            results[0].score
        );
    }

    #[test]
    fn test_centrality_boost_logarithmic_scaling() {
        // Verify the formula: boosted = score * (1.0 + ln(1 + ref_score) * CENTRALITY_WEIGHT)
        let weight = crate::search::scoring::CENTRALITY_WEIGHT;
        let base_score: f32 = 2.0;
        let ref_score: f64 = 20.0;

        let mut results = vec![
            make_symbol_result("sym1", "foo", base_score),
        ];

        let mut ref_scores = HashMap::new();
        ref_scores.insert("sym1".to_string(), ref_score);

        apply_centrality_boost(&mut results, &ref_scores);

        let expected_boost = 1.0 + (1.0 + ref_score as f32).ln() * weight;
        let expected_score = base_score * expected_boost;

        assert!(
            (results[0].score - expected_score).abs() < 0.001,
            "Expected score ~{:.4}, got {:.4} (boost factor: {:.4})",
            expected_score, results[0].score, expected_boost
        );
    }

    #[test]
    fn test_centrality_boost_empty_ref_scores() {
        let mut results = vec![
            make_symbol_result("sym1", "foo", 1.5),
            make_symbol_result("sym2", "bar", 1.0),
        ];

        let ref_scores = HashMap::new(); // empty

        let original_scores: Vec<f32> = results.iter().map(|r| r.score).collect();

        apply_centrality_boost(&mut results, &ref_scores);

        // All scores should remain unchanged
        for (i, result) in results.iter().enumerate() {
            assert!(
                (result.score - original_scores[i]).abs() < f32::EPSILON,
                "Score for {} should be unchanged with empty ref_scores",
                result.id
            );
        }

        // Order should be preserved (already sorted by score desc)
        assert_eq!(results[0].id, "sym1");
        assert_eq!(results[1].id, "sym2");
    }
}
