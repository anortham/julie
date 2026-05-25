//! Edge-case tests for test-to-code linkage computation.

#[cfg(test)]
mod tests {
    use crate::database::SymbolDatabase;
    use tempfile::TempDir;

    /// Insert a file record (required by foreign key constraint on symbols.file_path).
    fn insert_file(db: &SymbolDatabase, path: &str) {
        db.conn.execute(
            "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified) VALUES (?1, 'rust', 'h', 100, 0)",
            rusqlite::params![path],
        ).unwrap();
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
        let db = SymbolDatabase::new(&db_path).unwrap();

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
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('prod_aaa', 'Helper', 'method', 'csharp', 'src/services/Helper.cs', 10, 0, 30, 0, 0, 0, NULL, 1.0, 'public');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('prod_zzz', 'Helper', 'method', 'csharp', 'src/services/Helper2.cs', 10, 0, 30, 0, 0, 0, NULL, 1.0, 'public');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_some', 'SomeTest', 'method', 'csharp', 'tests/services/SomeTest.cs', 5, 0, 15, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "adequate"}}', 0.0, 'private');

            INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id, target_symbol_id)
            VALUES ('ident_1', 'Helper', 'call', 'csharp', 'tests/services/SomeTest.cs', 10, 0, 10, 20, 'test_some', NULL);
        "#).unwrap();

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
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/payment_service.rs");
        insert_file(&db, "tests/payment_service_tests.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata)
            VALUES ('parent', 'PaymentService', 'class', 'rust', 'src/payment_service.rs', 1, 0, 40, 0, 0, 0, NULL);

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, parent_id)
            VALUES ('child', 'process_payment', 'function', 'rust', 'src/payment_service.rs', 5, 0, 20, 0, 0, 0, NULL, 'parent');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata)
            VALUES ('test_1', 'test_process_payment', 'function', 'rust', 'tests/payment_service_tests.rs', 1, 0, 10, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "thorough"}}');

            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_1', 'test_1', 'child', 'calls', 'tests/payment_service_tests.rs', 3);
        "#).unwrap();

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
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/core.rs");
        insert_file(&db, "tests/core_test.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('prod_core', 'do_work', 'function', 'rust', 'src/core.rs', 1, 0, 20, 0, 0, 0, NULL, 5.0, 'public');

            -- Scorable test case (test_role = "test_case")
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_case_1', 'test_do_work', 'function', 'rust', 'tests/core_test.rs', 5, 0, 15, 0, 0, 0,
                    '{"is_test": true, "test_role": "test_case", "test_quality": {"quality_tier": "thorough", "confidence": 0.85}}', 0.0, 'private');

            -- Fixture setup (test_role = "fixture_setup", is_test = true) => should be EXCLUDED
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('fixture_1', 'setup_db', 'function', 'rust', 'tests/core_test.rs', 20, 0, 30, 0, 0, 0,
                    '{"is_test": true, "test_role": "fixture_setup"}', 0.0, 'private');

            -- Legacy test (is_test = true, no test_role) => should be INCLUDED
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('legacy_test', 'test_do_work_legacy', 'function', 'rust', 'tests/core_test.rs', 35, 0, 45, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "adequate"}}', 0.0, 'private');

            -- All three call the production symbol
            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_tc', 'test_case_1', 'prod_core', 'calls', 'tests/core_test.rs', 10);
            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_fix', 'fixture_1', 'prod_core', 'calls', 'tests/core_test.rs', 25);
            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_leg', 'legacy_test', 'prod_core', 'calls', 'tests/core_test.rs', 40);
        "#).unwrap();

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
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/engine.rs");
        insert_file(&db, "tests/engine_test.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('prod_eng', 'run_engine', 'function', 'rust', 'src/engine.rs', 1, 0, 20, 0, 0, 0, NULL, 3.0, 'public');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_eng', 'test_run_engine', 'function', 'rust', 'tests/engine_test.rs', 1, 0, 10, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "thorough", "confidence": 0.92}}', 0.0, 'private');

            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_eng', 'test_eng', 'prod_eng', 'calls', 'tests/engine_test.rs', 5);
        "#).unwrap();

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
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/services.rs");
        insert_file(&db, "tests/services_test.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('class_agg', 'OrderService', 'class', 'csharp', 'src/services.rs', 1, 0, 60, 0, 0, 0, NULL, 5.0, 'public');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility, parent_id)
            VALUES ('method_agg1', 'CreateOrder', 'method', 'csharp', 'src/services.rs', 5, 0, 20, 0, 0, 0, NULL, 3.0, 'public', 'class_agg');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility, parent_id)
            VALUES ('method_agg2', 'CancelOrder', 'method', 'csharp', 'src/services.rs', 25, 0, 40, 0, 0, 0, NULL, 2.0, 'public', 'class_agg');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_agg1', 'test_create_order', 'method', 'csharp', 'tests/services_test.rs', 1, 0, 10, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "thorough", "confidence": 0.92}}', 0.0, 'private');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_agg2', 'test_cancel_order', 'method', 'csharp', 'tests/services_test.rs', 15, 0, 25, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "adequate", "confidence": 0.75}}', 0.0, 'private');

            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_agg1', 'test_agg1', 'method_agg1', 'calls', 'tests/services_test.rs', 5);

            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_agg2', 'test_agg2', 'method_agg2', 'calls', 'tests/services_test.rs', 18);
        "#).unwrap();

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
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/old.rs");
        insert_file(&db, "tests/old_test.rs");

        // Test symbol with no confidence field in test_quality
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('prod_old', 'old_function', 'function', 'rust', 'src/old.rs', 1, 0, 10, 0, 0, 0, NULL, 2.0, 'public');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_old', 'test_old_function', 'function', 'rust', 'tests/old_test.rs', 1, 0, 8, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "adequate"}}', 0.0, 'private');

            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_old', 'test_old', 'prod_old', 'calls', 'tests/old_test.rs', 3);
        "#).unwrap();

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
