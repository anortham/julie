/// Hybrid Search Tests — RRF Merge Algorithm
///
/// Tests for the Reciprocal Rank Fusion merge function that combines
/// keyword (Tantivy) and semantic (KNN) search results.
///
/// Formula: RRF(d) = Σ 1/(k + rank) where rank is 1-based position.
#[cfg(test)]
mod tests {
    use crate::search::hybrid::rrf_merge;
    use crate::search::SymbolSearchResult;

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
        let semantic = vec![
            make_result("4", "d", 0.9),
            make_result("5", "e", 0.7),
        ];

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

        assert_eq!(merged.len(), 2, "graceful degradation: tantivy results unchanged");
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

        assert_eq!(merged.len(), 2, "semantic results returned when tantivy empty");
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
}
