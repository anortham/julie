//! Edge-case tests for test-to-code linkage computation.

#[cfg(test)]
mod tests {
    use julie_core::database::SymbolDatabase;
    use julie_extractors::{SymbolKind, Visibility};
    use julie_test_support::{
        file_info_builder, identifier_builder, relationship_builder, store_file_info_if_missing,
        symbol_builder,
    };
    use tempfile::TempDir;

    /// Insert a file record (required by foreign key constraint on symbols.file_path).
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
    fn test_name_match_fallback_is_deterministic_on_tied_scores() {
        // REGRESSION: when two prod symbols share a name and produce tied
        // dir_score + name_bonus, the name-match fallback must pick a
        // deterministic winner. Spec: smallest prod_id lexicographically,
        // then smallest prod_path. The query must also ORDER BY so SQLite
        // row order does not leak into the result across runs.
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        // Both prods live in the same directory so common_directory_depth
        // to the test path is identical. Neither file stem is a substring
        // of the test file stem, so name_bonus is 0 for both. Tied score.
        insert_file(&db, "src/services/Helper.cs");
        insert_file(&db, "src/services/Helper2.cs");
        insert_file(&db, "tests/services/SomeTest.cs");

        // Insertion order: prod_aaa first, prod_zzz second. Without ORDER BY,
        // SQLite returns rows in rowid order -> [prod_aaa, prod_zzz]. max_by_key
        // returns the LAST equal element (per std docs), so the unfixed code
        // picks prod_zzz - the wrong winner per spec.
        let test_metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "is_test": true,
                "test_quality": { "quality_tier": "adequate" }
            }))
            .unwrap();

        db.store_symbols(&[
            symbol_builder("prod_aaa", "Helper", "src/services/Helper.cs")
                .kind(SymbolKind::Method)
                .language("csharp")
                .span(10, 0, 30, 0)
                .visibility(Visibility::Public)
                .confidence(1.0)
                .build(),
            symbol_builder("prod_zzz", "Helper", "src/services/Helper2.cs")
                .kind(SymbolKind::Method)
                .language("csharp")
                .span(10, 0, 30, 0)
                .visibility(Visibility::Public)
                .confidence(1.0)
                .build(),
            symbol_builder("test_some", "SomeTest", "tests/services/SomeTest.cs")
                .kind(SymbolKind::Method)
                .language("csharp")
                .span(5, 0, 15, 0)
                .metadata(test_metadata)
                .visibility(Visibility::Private)
                .confidence(1.0)
                .build(),
        ])
        .unwrap();

        db.bulk_store_identifiers(
            &[
                identifier_builder("ident_1", "Helper", "tests/services/SomeTest.cs")
                    .language("csharp")
                    .line(10)
                    .column(0, 20)
                    .containing_symbol_id("test_some")
                    .build(),
            ],
            "",
        )
        .unwrap();

        // Run #1: populate metadata.
        crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();

        let cov_aaa_1: Option<String> = db
            .conn
            .query_row(
                "SELECT json_extract(metadata, '$.test_linkage') FROM symbols WHERE id = 'prod_aaa'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let cov_zzz_1: Option<String> = db
            .conn
            .query_row(
                "SELECT json_extract(metadata, '$.test_linkage') FROM symbols WHERE id = 'prod_zzz'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        // Spec: on tied score, smallest prod_id lexicographically wins.
        // 'prod_aaa' < 'prod_zzz' -> prod_aaa must receive the linkage.
        assert!(
            cov_aaa_1.is_some(),
            "prod_aaa (smaller id, tied score) should receive linkage; got None"
        );
        assert!(
            cov_zzz_1.is_none(),
            "prod_zzz (larger id, tied score) must NOT receive linkage; got {:?}",
            cov_zzz_1
        );

        // Run #2: clear all metadata, re-run. Output must match run #1 exactly.
        db.conn
            .execute(
                "UPDATE symbols SET metadata = NULL WHERE id IN ('prod_aaa', 'prod_zzz')",
                [],
            )
            .unwrap();
        crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();

        let cov_aaa_2: Option<String> = db
            .conn
            .query_row(
                "SELECT json_extract(metadata, '$.test_linkage') FROM symbols WHERE id = 'prod_aaa'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let cov_zzz_2: Option<String> = db
            .conn
            .query_row(
                "SELECT json_extract(metadata, '$.test_linkage') FROM symbols WHERE id = 'prod_zzz'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(
            cov_aaa_1, cov_aaa_2,
            "prod_aaa metadata must be byte-identical across runs"
        );
        assert_eq!(
            cov_zzz_1, cov_zzz_2,
            "prod_zzz metadata must be byte-identical across runs (both None)"
        );
    }

    #[test]
    fn test_compute_linkage_clears_stale_symbol_and_parent_linkage() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/payment_service.rs");
        insert_file(&db, "tests/payment_service_tests.rs");

        let test_metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "is_test": true,
                "test_quality": { "quality_tier": "thorough" }
            }))
            .unwrap();

        db.store_symbols(&[
            symbol_builder("parent", "PaymentService", "src/payment_service.rs")
                .kind(SymbolKind::Class)
                .span(1, 0, 40, 0)
                .confidence(1.0)
                .build(),
            symbol_builder("child", "process_payment", "src/payment_service.rs")
                .kind(SymbolKind::Function)
                .span(5, 0, 20, 0)
                .parent_id("parent")
                .confidence(1.0)
                .build(),
            symbol_builder(
                "test_1",
                "test_process_payment",
                "tests/payment_service_tests.rs",
            )
            .kind(SymbolKind::Function)
            .span(1, 0, 10, 0)
            .metadata(test_metadata)
            .confidence(1.0)
            .build(),
        ])
        .unwrap();
        db.store_relationships(&[relationship_builder("rel_1", "test_1", "child")
            .file_path("tests/payment_service_tests.rs")
            .line_number(3)
            .build()])
            .unwrap();

        crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();

        let child_before = db.get_symbol_by_id("child").unwrap().unwrap();
        let parent_before = db.get_symbol_by_id("parent").unwrap().unwrap();
        assert!(
            child_before
                .metadata
                .as_ref()
                .and_then(|meta| meta.get("test_linkage"))
                .is_some(),
            "child should gain linkage on first pass"
        );
        assert!(
            parent_before
                .metadata
                .as_ref()
                .and_then(|meta| meta.get("test_linkage"))
                .is_some(),
            "parent should inherit aggregated linkage on first pass"
        );

        db.conn
            .execute("DELETE FROM relationships WHERE id = 'rel_1'", [])
            .unwrap();

        crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();

        let child_after = db.get_symbol_by_id("child").unwrap().unwrap();
        let parent_after = db.get_symbol_by_id("parent").unwrap().unwrap();
        assert!(
            child_after
                .metadata
                .as_ref()
                .and_then(|meta| meta.get("test_linkage"))
                .is_none(),
            "child linkage should be cleared when no tests still point at it"
        );
        assert!(
            parent_after
                .metadata
                .as_ref()
                .and_then(|meta| meta.get("test_linkage"))
                .is_none(),
            "parent aggregation should also be cleared when child linkage disappears"
        );
    }

    #[test]
    fn test_scorable_filter_excludes_fixture_includes_test_case_and_legacy() {
        // Verifies the updated SQL filter:
        // - test_role = "test_case" => included
        // - test_role = "fixture_setup" with is_test = true => excluded
        // - is_test = true with no test_role => included (backward compat)
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/core.rs");
        insert_file(&db, "tests/core_test.rs");

        let test_case_metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "is_test": true,
                "test_role": "test_case",
                "test_quality": {
                    "quality_tier": "thorough",
                    "confidence": 0.85
                }
            }))
            .unwrap();
        let fixture_metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "is_test": true,
                "test_role": "fixture_setup"
            }))
            .unwrap();
        let legacy_metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "is_test": true,
                "test_quality": {
                    "quality_tier": "adequate"
                }
            }))
            .unwrap();

        db.store_symbols(&[
            symbol_builder("prod_core", "do_work", "src/core.rs")
                .kind(SymbolKind::Function)
                .span(1, 0, 20, 0)
                .visibility(Visibility::Public)
                .confidence(1.0)
                .build(),
            symbol_builder("test_case_1", "test_do_work", "tests/core_test.rs")
                .kind(SymbolKind::Function)
                .span(5, 0, 15, 0)
                .visibility(Visibility::Private)
                .metadata(test_case_metadata)
                .confidence(1.0)
                .build(),
            symbol_builder("fixture_1", "setup_db", "tests/core_test.rs")
                .kind(SymbolKind::Function)
                .span(20, 0, 30, 0)
                .visibility(Visibility::Private)
                .metadata(fixture_metadata)
                .confidence(1.0)
                .build(),
            symbol_builder("legacy_test", "test_do_work_legacy", "tests/core_test.rs")
                .kind(SymbolKind::Function)
                .span(35, 0, 45, 0)
                .visibility(Visibility::Private)
                .metadata(legacy_metadata)
                .confidence(1.0)
                .build(),
        ])
        .unwrap();

        db.store_relationships(&[
            relationship_builder("rel_tc", "test_case_1", "prod_core")
                .file_path("tests/core_test.rs")
                .line_number(10)
                .build(),
            relationship_builder("rel_fix", "fixture_1", "prod_core")
                .file_path("tests/core_test.rs")
                .line_number(25)
                .build(),
            relationship_builder("rel_leg", "legacy_test", "prod_core")
                .file_path("tests/core_test.rs")
                .line_number(40)
                .build(),
        ])
        .unwrap();

        let stats = crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();
        assert_eq!(
            stats.symbols_covered, 1,
            "One production symbol should be covered"
        );

        let prod = db.get_symbol_by_id("prod_core").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let linkage = meta.get("test_linkage").unwrap();

        // Fixture should be excluded, so only test_case_1 and legacy_test
        let test_count = linkage.get("test_count").unwrap().as_u64().unwrap();
        assert_eq!(
            test_count, 2,
            "Fixture should be excluded; only test_case and legacy test should link"
        );

        let linked_tests = linkage.get("linked_tests").unwrap().as_array().unwrap();
        let test_names: Vec<&str> = linked_tests.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            test_names.contains(&"test_do_work"),
            "test_case should be linked"
        );
        assert!(
            test_names.contains(&"test_do_work_legacy"),
            "legacy test should be linked (backward compat)"
        );
        assert!(
            !test_names.contains(&"setup_db"),
            "fixture should NOT be linked"
        );
    }

    #[test]
    fn test_best_confidence_present_in_linkage_output() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/engine.rs");
        insert_file(&db, "tests/engine_test.rs");

        let test_metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "is_test": true,
                "test_quality": {
                    "quality_tier": "thorough",
                    "confidence": 0.92
                }
            }))
            .unwrap();

        db.store_symbols(&[
            symbol_builder("prod_eng", "run_engine", "src/engine.rs")
                .kind(SymbolKind::Function)
                .span(1, 0, 20, 0)
                .visibility(Visibility::Public)
                .confidence(1.0)
                .build(),
            symbol_builder("test_eng", "test_run_engine", "tests/engine_test.rs")
                .kind(SymbolKind::Function)
                .span(1, 0, 10, 0)
                .visibility(Visibility::Private)
                .metadata(test_metadata)
                .confidence(1.0)
                .build(),
        ])
        .unwrap();

        db.store_relationships(&[relationship_builder("rel_eng", "test_eng", "prod_eng")
            .file_path("tests/engine_test.rs")
            .line_number(5)
            .build()])
            .unwrap();

        crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();

        let prod = db.get_symbol_by_id("prod_eng").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let linkage = meta.get("test_linkage").unwrap();

        let best_confidence = linkage.get("best_confidence").unwrap().as_f64().unwrap();
        assert!(
            (best_confidence - 0.92).abs() < 0.001,
            "best_confidence should be 0.92 from the test's metadata, got {}",
            best_confidence
        );
    }

    #[test]
    fn test_parent_aggregation_includes_best_confidence() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/services.rs");
        insert_file(&db, "tests/services_test.rs");

        let thorough_metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "is_test": true,
                "test_quality": {
                    "quality_tier": "thorough",
                    "confidence": 0.92
                }
            }))
            .unwrap();
        let adequate_metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "is_test": true,
                "test_quality": {
                    "quality_tier": "adequate",
                    "confidence": 0.75
                }
            }))
            .unwrap();

        db.store_symbols(&[
            symbol_builder("class_agg", "OrderService", "src/services.rs")
                .kind(SymbolKind::Class)
                .language("csharp")
                .span(1, 0, 60, 0)
                .visibility(Visibility::Public)
                .confidence(1.0)
                .build(),
            symbol_builder("method_agg1", "CreateOrder", "src/services.rs")
                .kind(SymbolKind::Method)
                .language("csharp")
                .span(5, 0, 20, 0)
                .visibility(Visibility::Public)
                .parent_id("class_agg")
                .confidence(1.0)
                .build(),
            symbol_builder("method_agg2", "CancelOrder", "src/services.rs")
                .kind(SymbolKind::Method)
                .language("csharp")
                .span(25, 0, 40, 0)
                .visibility(Visibility::Public)
                .parent_id("class_agg")
                .confidence(1.0)
                .build(),
            symbol_builder("test_agg1", "test_create_order", "tests/services_test.rs")
                .kind(SymbolKind::Method)
                .language("csharp")
                .span(1, 0, 10, 0)
                .visibility(Visibility::Private)
                .metadata(thorough_metadata)
                .confidence(1.0)
                .build(),
            symbol_builder("test_agg2", "test_cancel_order", "tests/services_test.rs")
                .kind(SymbolKind::Method)
                .language("csharp")
                .span(15, 0, 25, 0)
                .visibility(Visibility::Private)
                .metadata(adequate_metadata)
                .confidence(1.0)
                .build(),
        ])
        .unwrap();

        db.store_relationships(&[
            relationship_builder("rel_agg1", "test_agg1", "method_agg1")
                .file_path("tests/services_test.rs")
                .line_number(5)
                .build(),
            relationship_builder("rel_agg2", "test_agg2", "method_agg2")
                .file_path("tests/services_test.rs")
                .line_number(18)
                .build(),
        ])
        .unwrap();

        crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();

        // The parent class should inherit aggregated linkage from its methods
        let class_meta_str: Option<String> = db
            .conn
            .query_row(
                "SELECT json_extract(metadata, '$.test_linkage') FROM symbols WHERE id = 'class_agg'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            class_meta_str.is_some(),
            "Parent class should have aggregated test_linkage"
        );

        let linkage: serde_json::Value = serde_json::from_str(&class_meta_str.unwrap()).unwrap();

        // best_confidence should be present and reflect the best child's confidence
        let best_confidence = linkage
            .get("best_confidence")
            .expect("best_confidence must be present in parent linkage")
            .as_f64()
            .unwrap();
        assert!(
            (best_confidence - 0.92).abs() < 0.001,
            "Parent best_confidence should be 0.92 (from the thorough child), got {}",
            best_confidence
        );

        // best_tier should be "thorough" (the best among children)
        assert_eq!(
            linkage.get("best_tier").unwrap().as_str().unwrap(),
            "thorough"
        );
    }

    #[test]
    fn test_best_confidence_defaults_when_metadata_absent() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/old.rs");
        insert_file(&db, "tests/old_test.rs");

        let test_metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "is_test": true,
                "test_quality": {
                    "quality_tier": "adequate"
                }
            }))
            .unwrap();

        db.store_symbols(&[
            symbol_builder("prod_old", "old_function", "src/old.rs")
                .kind(SymbolKind::Function)
                .span(1, 0, 10, 0)
                .visibility(Visibility::Public)
                .confidence(1.0)
                .build(),
            symbol_builder("test_old", "test_old_function", "tests/old_test.rs")
                .kind(SymbolKind::Function)
                .span(1, 0, 8, 0)
                .visibility(Visibility::Private)
                .metadata(test_metadata)
                .confidence(1.0)
                .build(),
        ])
        .unwrap();

        db.store_relationships(&[relationship_builder("rel_old", "test_old", "prod_old")
            .file_path("tests/old_test.rs")
            .line_number(3)
            .build()])
            .unwrap();

        crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();

        let prod = db.get_symbol_by_id("prod_old").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let linkage = meta.get("test_linkage").unwrap();

        let best_confidence = linkage.get("best_confidence").unwrap().as_f64().unwrap();
        assert!(
            (best_confidence - 0.5).abs() < 0.001,
            "best_confidence should default to 0.5 when test has no confidence metadata, got {}",
            best_confidence
        );
    }
}
