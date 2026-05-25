/// Hybrid search tests for reciprocal-rank fusion merge behavior.
///
/// Formula: RRF(d) = sum(1 / (k + rank)) where rank is 1-based position.
#[cfg(test)]
mod tests {
    use crate::search::SymbolSearchResult;
    use crate::search::hybrid::rrf_merge;

    /// Helper to build a minimal SymbolSearchResult for testing.
    fn make_result(id: &str, name: &str, score: f32) -> SymbolSearchResult {
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
            role: String::new(),
            test_role: String::new(),
        }
    }

    #[test]
    fn test_rrf_merge_disjoint_lists() {
        let tantivy = vec![
            make_result("1", "alpha", 10.0),
            make_result("2", "beta", 8.0),
        ];
        let semantic = vec![
            make_result("3", "gamma", 0.9),
            make_result("4", "delta", 0.7),
        ];

        let merged = rrf_merge(tantivy, semantic, 60, 10);

        assert_eq!(merged.len(), 4, "all 4 disjoint items should appear");
        let ids: Vec<&str> = merged.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"1"));
        assert!(ids.contains(&"2"));
        assert!(ids.contains(&"3"));
        assert!(ids.contains(&"4"));
    }

    #[test]
    fn test_rrf_merge_overlapping_lists() {
        // "a" appears in both lists — should get boosted to rank 1
        let tantivy = vec![
            make_result("a", "shared", 10.0),
            make_result("b", "tantivy_only", 8.0),
        ];
        let semantic = vec![
            make_result("a", "shared", 0.95),
            make_result("c", "semantic_only", 0.8),
        ];

        let merged = rrf_merge(tantivy, semantic, 60, 10);

        assert_eq!(merged.len(), 3, "3 unique items");
        assert_eq!(
            merged[0].id, "a",
            "overlapping item should rank first due to double RRF contribution"
        );
    }

    #[test]
    fn test_rrf_merge_respects_limit() {
        let tantivy = vec![
            make_result("1", "a", 10.0),
            make_result("2", "b", 8.0),
            make_result("3", "c", 6.0),
        ];
        let semantic = vec![make_result("4", "d", 0.9), make_result("5", "e", 0.7)];

        let merged = rrf_merge(tantivy, semantic, 60, 3);

        assert_eq!(merged.len(), 3, "should be capped at limit=3");
    }

    #[test]
    fn test_rrf_merge_empty_semantic() {
        let tantivy = vec![
            make_result("1", "alpha", 10.0),
            make_result("2", "beta", 8.0),
        ];
        let semantic = vec![];

        let merged = rrf_merge(tantivy, semantic, 60, 10);

        assert_eq!(
            merged.len(),
            2,
            "graceful degradation: tantivy results unchanged"
        );
        assert_eq!(merged[0].id, "1");
        assert_eq!(merged[1].id, "2");
    }

    #[test]
    fn test_rrf_merge_empty_tantivy() {
        let tantivy = vec![];
        let semantic = vec![
            make_result("3", "gamma", 0.9),
            make_result("4", "delta", 0.7),
        ];

        let merged = rrf_merge(tantivy, semantic, 60, 10);

        assert_eq!(
            merged.len(),
            2,
            "semantic results returned when tantivy empty"
        );
        assert_eq!(merged[0].id, "3");
        assert_eq!(merged[1].id, "4");
    }

    #[test]
    fn test_rrf_score_is_stored_in_result() {
        // Item in both lists at rank 1 (1-based), k=60
        // RRF score = 1/(60+1) + 1/(60+1) = 2/61
        let tantivy = vec![make_result("x", "shared", 10.0)];
        let semantic = vec![make_result("x", "shared", 0.95)];

        let merged = rrf_merge(tantivy, semantic, 60, 10);

        assert_eq!(merged.len(), 1);
        let expected_score = 2.0_f32 / 61.0;
        let actual_score = merged[0].score;
        assert!(
            (actual_score - expected_score).abs() < 1e-6,
            "RRF score should be 2/61 ≈ {:.6}, got {:.6}",
            expected_score,
            actual_score,
        );
    }

    #[test]
    fn test_rrf_merge_prefers_semantic_metadata_for_duplicates() {
        // When a symbol appears in both keyword and semantic results,
        // the semantic version's metadata should be preferred because
        // it comes from SQLite (source of truth), while Tantivy stored
        // fields may be stale after incremental updates.
        let mut keyword_version = make_result("dup", "my_func", 10.0);
        keyword_version.signature = "fn my_func(old: i32)".to_string();
        keyword_version.doc_comment = "Old doc comment".to_string();

        let mut semantic_version = make_result("dup", "my_func", 0.9);
        semantic_version.signature = "fn my_func(new: String)".to_string();
        semantic_version.doc_comment = "Updated doc comment".to_string();

        let tantivy = vec![keyword_version];
        let semantic = vec![semantic_version];

        let merged = rrf_merge(tantivy, semantic, 60, 10);

        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].signature, "fn my_func(new: String)",
            "should prefer semantic (SQLite) metadata over keyword (Tantivy) metadata"
        );
        assert_eq!(
            merged[0].doc_comment, "Updated doc comment",
            "should prefer semantic doc_comment"
        );
    }
}

/// Weighted RRF merge tests (Phase 5, Task 2).
///
/// Verifies that per-source weighting correctly biases the merge:
/// - Equal weights = same output as uniform merge
/// - Higher weight = more influence on ranking
/// - Zero weight = effectively excluded
#[cfg(test)]
mod weighted_rrf_tests {
    use crate::search::SymbolSearchResult;
    use crate::search::hybrid::{rrf_merge, weighted_rrf_merge};
    use crate::search::weights::SearchWeightProfile;

    fn make_result(id: &str, name: &str, score: f32) -> SymbolSearchResult {
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: String::new(),
            doc_comment: String::new(),
            file_path: "test.rs".to_string(),
            kind: "function".to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
            role: String::new(),
            test_role: String::new(),
        }
    }

    #[test]
    fn test_weighted_equal_weights_matches_uniform() {
        let tantivy = vec![
            make_result("a", "alpha", 10.0),
            make_result("b", "beta", 8.0),
        ];
        let semantic = vec![
            make_result("b", "beta", 0.9),
            make_result("c", "gamma", 0.8),
        ];

        let tantivy_clone = tantivy.clone();
        let semantic_clone = semantic.clone();

        let uniform = rrf_merge(tantivy, semantic, 60, 10);
        let weighted = weighted_rrf_merge(tantivy_clone, semantic_clone, 60, 10, 1.0, 1.0);

        assert_eq!(uniform.len(), weighted.len());
        for (u, w) in uniform.iter().zip(weighted.iter()) {
            assert_eq!(u.id, w.id, "same order expected");
            assert!(
                (u.score - w.score).abs() < 1e-6,
                "scores should match: {} vs {}",
                u.score,
                w.score
            );
        }
    }

    #[test]
    fn test_weighted_higher_weight_increases_contribution() {
        // Two disjoint lists, each with one item
        let tantivy = vec![make_result("a", "alpha", 10.0)];
        let semantic = vec![make_result("b", "beta", 0.9)];

        // Heavy keyword weight
        let results = weighted_rrf_merge(tantivy.clone(), semantic.clone(), 60, 10, 2.0, 1.0);

        // "a" should rank higher because keyword weight is 2x
        assert_eq!(
            results[0].id, "a",
            "keyword result should rank first with 2x weight"
        );

        // Now flip: heavy semantic weight
        let results2 = weighted_rrf_merge(tantivy, semantic, 60, 10, 1.0, 2.0);
        assert_eq!(
            results2[0].id, "b",
            "semantic result should rank first with 2x weight"
        );
    }

    #[test]
    fn test_weighted_zero_weight_excludes_source() {
        let tantivy = vec![
            make_result("a", "alpha", 10.0),
            make_result("b", "beta", 8.0),
        ];
        let semantic = vec![make_result("c", "gamma", 0.9)];

        // Zero semantic weight — only keyword results should have nonzero scores
        let results = weighted_rrf_merge(tantivy, semantic, 60, 10, 1.0, 0.0);

        // "c" only appeared in semantic with weight 0, so its score should be 0
        let gamma = results.iter().find(|r| r.id == "c").unwrap();
        assert!(
            gamma.score < 1e-10,
            "zero-weighted source items should have ~0 score, got {}",
            gamma.score
        );
    }

    #[test]
    fn test_search_weight_presets_have_expected_values() {
        let code = SearchWeightProfile::fast_search();
        assert!(
            code.keyword_weight >= 1.0,
            "fast_search should weight keywords strongly"
        );
        assert!(
            code.semantic_weight > 0.0,
            "fast_search should still use semantic"
        );

        let recall = SearchWeightProfile::recall();
        assert!(recall.keyword_weight > 0.0, "recall should use keywords");
        assert!(
            recall.semantic_weight >= 0.8,
            "recall should weight semantic strongly"
        );

        let balanced = SearchWeightProfile::get_context();
        assert!(balanced.keyword_weight > 0.0);
        assert!(balanced.semantic_weight > 0.0);
    }
}
