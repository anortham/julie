//! Tests for change risk scoring.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::analysis::change_risk::*;
    use crate::extractors::{SymbolKind, Visibility};
    use crate::tests::helpers::db::{
        file_info_builder, set_symbol_reference_scores, store_file_info_if_missing, symbol_builder,
    };

    #[test]
    fn test_visibility_scores() {
        assert_eq!(visibility_score(Some("public")), 1.0);
        assert_eq!(visibility_score(Some("protected")), 0.5);
        assert_eq!(visibility_score(Some("private")), 0.2);
        assert_eq!(visibility_score(None), 0.5); // NULL → moderate
    }

    #[test]
    fn test_kind_weights() {
        assert_eq!(kind_weight(&SymbolKind::Function), Some(1.0));
        assert_eq!(kind_weight(&SymbolKind::Method), Some(1.0));
        assert_eq!(kind_weight(&SymbolKind::Constructor), Some(1.0));
        assert_eq!(kind_weight(&SymbolKind::Class), Some(0.7));
        assert_eq!(kind_weight(&SymbolKind::Struct), Some(0.7));
        assert_eq!(kind_weight(&SymbolKind::Trait), Some(0.7));
        assert_eq!(kind_weight(&SymbolKind::Variable), Some(0.3));
        assert_eq!(kind_weight(&SymbolKind::Constant), Some(0.3));
        assert_eq!(kind_weight(&SymbolKind::Import), None); // Excluded
        assert_eq!(kind_weight(&SymbolKind::Export), None);
    }

    #[test]
    fn test_test_weakness_scores_full_confidence() {
        // With confidence=1.0, raw weakness passes through unchanged
        let eps = 0.01;
        assert!((test_weakness_score(None, 1.0) - 1.0).abs() < eps); // Untested
        assert!((test_weakness_score(Some("stub"), 1.0) - 0.9).abs() < eps);
        assert!((test_weakness_score(Some("thin"), 1.0) - 0.6).abs() < eps);
        assert!((test_weakness_score(Some("adequate"), 1.0) - 0.3).abs() < eps);
        assert!((test_weakness_score(Some("thorough"), 1.0) - 0.1).abs() < eps);
    }

    #[test]
    fn test_weakness_high_confidence_thorough() {
        let score = test_weakness_score(Some("thorough"), 0.9);
        // raw = 0.1, confidence 0.9: 0.5 + (0.1 - 0.5) * 0.9 = 0.14
        assert!((score - 0.14).abs() < 0.02);
    }

    #[test]
    fn test_weakness_low_confidence_converges_to_neutral() {
        let score = test_weakness_score(Some("stub"), 0.0);
        // confidence 0 -> always neutral
        assert!((score - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_weakness_unknown_tier() {
        let score = test_weakness_score(Some("unknown"), 0.3);
        // raw = 0.5, so result = 0.5 + (0.5 - 0.5) * 0.3 = 0.5
        assert!((score - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_weakness_no_linkage_full_penalty() {
        let score = test_weakness_score(None, 1.0);
        assert!((score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_weakness_na_tier_is_neutral() {
        let score = test_weakness_score(Some("n/a"), 1.0);
        assert!((score - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_normalize_centrality() {
        assert_eq!(normalize_centrality(0.0, 20.0), 0.0);
        // P95=20 → score of 20 should be ~1.0
        let at_p95 = normalize_centrality(20.0, 20.0);
        assert!(
            (at_p95 - 1.0).abs() < 0.01,
            "Score at P95 should be ~1.0, got {}",
            at_p95
        );
        // Score above P95 should be capped at 1.0
        assert_eq!(normalize_centrality(100.0, 20.0), 1.0);
        // P95=0 → everything is 0
        assert_eq!(normalize_centrality(5.0, 0.0), 0.0);
    }

    #[test]
    fn test_risk_labels() {
        assert_eq!(risk_label(0.85), "HIGH");
        assert_eq!(risk_label(0.70), "HIGH");
        assert_eq!(risk_label(0.55), "MEDIUM");
        assert_eq!(risk_label(0.40), "MEDIUM");
        assert_eq!(risk_label(0.39), "LOW");
        assert_eq!(risk_label(0.0), "LOW");
    }

    #[test]
    fn test_high_risk_scenario() {
        // Public function, high centrality, untested → HIGH risk
        let score = compute_risk_score(0.95, 1.0, 1.0, 1.0);
        assert!(
            score >= 0.7,
            "Public untested function with high centrality should be HIGH, got {:.2}",
            score
        );
        assert_eq!(risk_label(score), "HIGH");
    }

    #[test]
    fn test_low_risk_scenario() {
        // Private constant, no centrality, thoroughly tested → LOW risk
        let score = compute_risk_score(0.0, 0.2, 0.1, 0.3);
        assert!(
            score < 0.4,
            "Private tested constant should be LOW, got {:.2}",
            score
        );
        assert_eq!(risk_label(score), "LOW");
    }

    use crate::database::SymbolDatabase;
    use tempfile::TempDir;

    fn insert_file(db: &SymbolDatabase, path: &str) {
        store_file_info_if_missing(
            db,
            &file_info_builder(path)
                .hash("h")
                .size(100)
                .last_modified(0)
                .symbol_count(0)
                .line_count(0)
                .build(),
        )
        .unwrap();
    }

    #[test]
    fn test_compute_change_risk_scores() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/core.rs");
        insert_file(&db, "src/config.rs");
        insert_file(&db, "tests/test.rs");
        insert_file(&db, "src/lib.rs");

        let test_linkage_metadata = HashMap::from([(
            "test_linkage".to_string(),
            serde_json::json!({
                "test_count": 2,
                "best_tier": "thorough",
                "worst_tier": "adequate",
                "best_confidence": 0.9,
                "linked_tests": ["test_a", "test_b"],
                "evidence_sources": ["relationship"],
            }),
        )]);
        let test_symbol_metadata =
            HashMap::from([("is_test".to_string(), serde_json::json!(true))]);

        db.store_symbols(&[
            // High-risk: public function, high centrality, untested
            symbol_builder("s1", "important_func", "src/core.rs")
                .kind(SymbolKind::Function)
                .span(1, 0, 10, 0)
                .visibility(Visibility::Public)
                .confidence(1.0)
                .build(),
            // Low-risk: private constant, no centrality, thoroughly tested
            symbol_builder("s2", "MY_CONST", "src/config.rs")
                .kind(SymbolKind::Constant)
                .span(1, 0, 1, 0)
                .visibility(Visibility::Private)
                .metadata(test_linkage_metadata)
                .confidence(1.0)
                .build(),
            // Test symbol — should be excluded from risk scoring
            symbol_builder("t1", "test_thing", "tests/test.rs")
                .kind(SymbolKind::Function)
                .span(1, 0, 5, 0)
                .metadata(test_symbol_metadata)
                .confidence(1.0)
                .build(),
            // Import — should be excluded (kind_weight returns None)
            symbol_builder("imp", "use_thing", "src/lib.rs")
                .kind(SymbolKind::Import)
                .span(1, 0, 1, 0)
                .confidence(1.0)
                .build(),
        ])
        .unwrap();
        set_symbol_reference_scores(&db, &[("s1", 20.0), ("s2", 0.0), ("t1", 0.0), ("imp", 0.0)])
            .unwrap();

        let stats = crate::analysis::change_risk::compute_change_risk_scores(&db).unwrap();
        assert_eq!(
            stats.total_scored, 2,
            "Should score s1 and s2 only (not test or import)"
        );
        assert!(stats.high_risk >= 1, "important_func should be HIGH risk");

        // Verify s1 metadata
        let s1 = db.get_symbol_by_id("s1").unwrap().unwrap();
        let meta = s1.metadata.unwrap();
        let risk = meta.get("change_risk").unwrap();
        let label = risk.get("label").unwrap().as_str().unwrap();
        assert_eq!(label, "HIGH");
        assert_eq!(
            risk.get("factors")
                .unwrap()
                .get("test_weakness")
                .unwrap()
                .as_f64()
                .unwrap(),
            1.0,
            "symbols with no linkage metadata should get the full untested penalty"
        );

        // Verify s2 is LOW risk
        let s2 = db.get_symbol_by_id("s2").unwrap().unwrap();
        let meta2 = s2.metadata.unwrap();
        let risk2 = meta2.get("change_risk").unwrap();
        let label2 = risk2.get("label").unwrap().as_str().unwrap();
        assert_eq!(label2, "LOW");

        // Verify test symbol has no change_risk
        let t1 = db.get_symbol_by_id("t1").unwrap().unwrap();
        if let Some(meta_t) = &t1.metadata {
            assert!(
                meta_t.get("change_risk").is_none(),
                "Test symbols should not have change_risk"
            );
        }
    }
}
