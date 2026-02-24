//! Tests for the get_context tool pipeline — pivot selection with centrality-weighted scoring.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::search::index::SymbolSearchResult;
    use crate::tools::get_context::pipeline::{select_pivots, Pivot};

    /// Helper to create a test SymbolSearchResult with minimal boilerplate.
    fn make_result(id: &str, name: &str, score: f32) -> SymbolSearchResult {
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: format!("fn {}()", name),
            doc_comment: String::new(),
            file_path: format!("src/{}.rs", name),
            kind: "function".to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        }
    }

    #[test]
    fn test_select_pivots_empty_results() {
        let results = vec![];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);
        assert!(pivots.is_empty());
    }

    #[test]
    fn test_select_pivots_single_result() {
        let results = vec![make_result("a", "alpha", 5.0)];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);
        assert_eq!(pivots.len(), 1);
        assert_eq!(pivots[0].result.name, "alpha");
    }

    #[test]
    fn test_select_pivots_clear_winner() {
        // Top result is 2x+ above second → should return 1 pivot
        let results = vec![
            make_result("a", "dominant", 10.0),
            make_result("b", "weak", 4.0),
            make_result("c", "weaker", 3.0),
        ];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);
        assert_eq!(pivots.len(), 1, "clear winner should yield 1 pivot");
        assert_eq!(pivots[0].result.name, "dominant");
    }

    #[test]
    fn test_select_pivots_cluster() {
        // Top 3 within 30% of each other → should return 3 pivots
        // Scores: 10.0, 9.0, 8.0 — third (8.0) >= top (10.0) * 0.7 (7.0)
        let results = vec![
            make_result("a", "first", 10.0),
            make_result("b", "second", 9.0),
            make_result("c", "third", 8.0),
        ];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);
        assert_eq!(pivots.len(), 3, "cluster should yield 3 pivots");
        assert_eq!(pivots[0].result.name, "first");
        assert_eq!(pivots[1].result.name, "second");
        assert_eq!(pivots[2].result.name, "third");
    }

    #[test]
    fn test_select_pivots_default_two() {
        // Neither clear winner nor cluster → should return 2 pivots
        // Scores: 10.0, 7.0, 5.0 — top is NOT 2x+ second (10 < 14), third (5) < top*0.7 (7)
        let results = vec![
            make_result("a", "first", 10.0),
            make_result("b", "second", 7.0),
            make_result("c", "third", 5.0),
        ];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);
        assert_eq!(pivots.len(), 2, "default case should yield 2 pivots");
        assert_eq!(pivots[0].result.name, "first");
        assert_eq!(pivots[1].result.name, "second");
    }

    #[test]
    fn test_select_pivots_centrality_reranks() {
        // Result "b" has lower text score but high reference_score,
        // so the centrality boost should promote it above "a".
        //
        // "a": score=5.0, ref_score=0 → combined = 5.0 * 1.0 = 5.0
        // "b": score=4.0, ref_score=50.0 → boost = 1 + ln(51) * 0.3 ≈ 1 + 3.93*0.3 ≈ 2.18
        //       combined = 4.0 * 2.18 ≈ 8.72
        let results = vec![
            make_result("a", "no_refs", 5.0),
            make_result("b", "many_refs", 4.0),
        ];
        let mut ref_scores = HashMap::new();
        ref_scores.insert("b".to_string(), 50.0_f64);

        let pivots = select_pivots(results, &ref_scores);
        assert_eq!(
            pivots[0].result.name, "many_refs",
            "high-centrality result should be promoted to top"
        );
        assert!(
            pivots[0].combined_score > pivots[1].combined_score,
            "many_refs ({:.2}) should score higher than no_refs ({:.2})",
            pivots[0].combined_score,
            pivots[1].combined_score,
        );
    }
}
