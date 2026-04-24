//! Tests for change risk scoring.

#[cfg(test)]
mod tests {
    use crate::analysis::change_risk::*;
    use crate::extractors::SymbolKind;

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
        db.conn
            .execute(
                "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified) VALUES (?1, 'rust', 'h', 100, 0)",
                rusqlite::params![path],
            )
            .unwrap();
    }

    #[test]
    fn test_compute_change_risk_scores() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/core.rs");
        insert_file(&db, "src/config.rs");
        insert_file(&db, "tests/test.rs");
        insert_file(&db, "src/lib.rs");

        // High-risk: public function, high centrality, untested
        db.conn
            .execute_batch(
                r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, visibility, metadata)
            VALUES ('s1', 'important_func', 'function', 'rust', 'src/core.rs', 1, 0, 10, 0, 0, 0, 20.0, 'public', NULL);
        "#,
            )
            .unwrap();

        // Low-risk: private constant, no centrality, thoroughly tested
        db.conn
            .execute_batch(
                r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, visibility, metadata)
            VALUES ('s2', 'MY_CONST', 'constant', 'rust', 'src/config.rs', 1, 0, 1, 0, 0, 0, 0.0, 'private',
                    '{"test_linkage": {"test_count": 2, "best_tier": "thorough", "worst_tier": "adequate", "best_confidence": 0.9, "linked_tests": ["test_a", "test_b"], "evidence_sources": ["relationship"]}}');
        "#,
            )
            .unwrap();

        // Test symbol — should be excluded from risk scoring
        db.conn
            .execute_batch(
                r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, metadata)
            VALUES ('t1', 'test_thing', 'function', 'rust', 'tests/test.rs', 1, 0, 5, 0, 0, 0, 0.0, '{"is_test": true}');
        "#,
            )
            .unwrap();

        // Import — should be excluded (kind_weight returns None)
        db.conn
            .execute_batch(
                r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, reference_score, metadata)
            VALUES ('imp', 'use_thing', 'import', 'rust', 'src/lib.rs', 1, 0, 1, 0, 0, 0, 0.0, NULL);
        "#,
            )
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
