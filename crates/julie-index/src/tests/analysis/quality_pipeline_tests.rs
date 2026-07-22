//! Pipeline integration tests for `compute_test_quality_metrics`.

#[cfg(test)]
mod tests {
    use crate::analysis::test_quality::compute_test_quality_metrics;
    use crate::search::LanguageConfigs;
    use julie_core::database::SymbolDatabase;
    use julie_test_support::{file_info_builder, identifier_builder, symbol_builder};
    use serde_json::json;
    use std::collections::HashMap;
    use tempfile::TempDir;

    type Metadata = HashMap<String, serde_json::Value>;

    struct TestQualityDb {
        _tmp: TempDir,
        db: SymbolDatabase,
        configs: LanguageConfigs,
    }

    fn build_test_db(file_path: &str, language: &str) -> TestQualityDb {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();
        let configs = LanguageConfigs::load_embedded();

        db.store_file_info(
            &file_info_builder(file_path)
                .language(language)
                .hash("abc123")
                .size(100)
                .last_modified(0)
                .build(),
        )
        .unwrap();

        TestQualityDb {
            _tmp: tmp,
            db,
            configs,
        }
    }

    fn metadata_from_json(value: serde_json::Value) -> Metadata {
        serde_json::from_value(value).unwrap()
    }

    fn metadata_for_test() -> Metadata {
        metadata_from_json(json!({ "is_test": true }))
    }

    fn symbol_metadata(db: &SymbolDatabase, symbol_id: &str) -> serde_json::Value {
        let metadata: String = db
            .conn
            .query_row(
                "SELECT metadata FROM symbols WHERE id = ?1",
                [symbol_id],
                |row| row.get(0),
            )
            .unwrap();

        serde_json::from_str(&metadata).unwrap()
    }

    #[test]
    fn test_pipeline_integration_updates_metadata() {
        let mut ctx = build_test_db("test_file.rs", "rust");
        let code_body = r#"fn test_something() {
    let result = compute(42);
    assert_eq!(result, 84);
    assert!(result > 0);
}"#;

        ctx.db
            .store_symbols(&[
                symbol_builder("sym-test-1", "test_something", "test_file.rs")
                    .code_context(code_body)
                    .metadata(metadata_for_test())
                    .build(),
                symbol_builder("sym-regular-1", "compute", "test_file.rs")
                    .code_context("fn compute(x: i32) -> i32 { x * 2 }")
                    .metadata(metadata_from_json(json!({})))
                    .build(),
            ])
            .unwrap();

        let stats = compute_test_quality_metrics(&ctx.db, &ctx.configs).unwrap();

        assert_eq!(stats.total_tests, 1, "Should have analyzed 1 test symbol");

        let meta = symbol_metadata(&ctx.db, "sym-test-1");
        assert!(
            meta["is_test"].as_bool().unwrap(),
            "is_test should still be true"
        );
        assert!(
            meta["test_quality"].is_object(),
            "test_quality should be added"
        );
        assert_eq!(meta["test_quality"]["assertion_count"].as_u64().unwrap(), 2);
        assert_eq!(
            meta["test_quality"]["quality_tier"].as_str().unwrap(),
            "adequate"
        );
        assert!(
            meta["test_quality"]["confidence"].as_f64().is_some(),
            "confidence should be present"
        );
        assert_eq!(
            meta["test_quality"]["assertion_source"].as_str().unwrap(),
            "regex",
            "No identifiers inserted, so should be regex path"
        );

        let non_test_meta = symbol_metadata(&ctx.db, "sym-regular-1");
        assert!(
            non_test_meta.get("test_quality").is_none(),
            "Non-test symbol should not have test_quality"
        );
    }

    #[test]
    fn test_pipeline_integration_no_body() {
        let mut ctx = build_test_db("test_file.rs", "rust");

        ctx.db
            .store_symbols(&[
                symbol_builder("sym-test-no-body", "test_empty", "test_file.rs")
                    .metadata(metadata_for_test())
                    .build(),
            ])
            .unwrap();

        let stats = compute_test_quality_metrics(&ctx.db, &ctx.configs).unwrap();
        assert_eq!(stats.total_tests, 1);
        assert_eq!(
            stats.no_body, 1,
            "Symbol with NULL code_context should be counted as no_body"
        );
        assert_eq!(stats.stub, 1, "No body means stub tier");
    }

    #[test]
    fn test_pipeline_integration_preserves_existing_metadata() {
        let mut ctx = build_test_db("test_file.rs", "rust");

        ctx.db
            .store_symbols(&[
                symbol_builder("sym-test-preserve", "test_preserve", "test_file.rs")
                    .code_context("assert_eq!(1, 1);")
                    .metadata(metadata_from_json(
                        json!({ "is_test": true, "custom_flag": "keep_me" }),
                    ))
                    .build(),
            ])
            .unwrap();

        compute_test_quality_metrics(&ctx.db, &ctx.configs).unwrap();

        let meta = symbol_metadata(&ctx.db, "sym-test-preserve");
        assert_eq!(
            meta["custom_flag"].as_str().unwrap(),
            "keep_me",
            "Existing metadata should be preserved"
        );
        assert!(
            meta["test_quality"].is_object(),
            "test_quality should be added"
        );
    }

    #[test]
    fn test_pipeline_integration_with_identifier_evidence() {
        let mut ctx = build_test_db("test_file.rs", "rust");
        let code_body =
            "fn test_with_identifiers() {\n    let x = compute();\n    assert_eq!(x, 42);\n}";

        ctx.db
            .store_symbols(&[symbol_builder(
                "sym-test-ids",
                "test_with_identifiers",
                "test_file.rs",
            )
            .code_context(code_body)
            .metadata(metadata_for_test())
            .build()])
            .unwrap();

        ctx.db
            .bulk_store_identifiers(
                &[
                    identifier_builder("id-1", "assert_eq", "test_file.rs")
                        .line(3)
                        .column(4, 20)
                        .containing_symbol_id("sym-test-ids")
                        .build(),
                    identifier_builder("id-2", "assert", "test_file.rs")
                        .line(4)
                        .column(4, 15)
                        .containing_symbol_id("sym-test-ids")
                        .build(),
                    identifier_builder("id-3", "assert_ne", "test_file.rs")
                        .line(5)
                        .column(4, 15)
                        .containing_symbol_id("sym-test-ids")
                        .build(),
                ],
                "test-workspace",
            )
            .unwrap();

        let stats = compute_test_quality_metrics(&ctx.db, &ctx.configs).unwrap();
        assert_eq!(stats.total_tests, 1);

        let meta = symbol_metadata(&ctx.db, "sym-test-ids");
        let tq = &meta["test_quality"];
        assert_eq!(
            tq["assertion_source"].as_str().unwrap(),
            "identifier",
            "Should use identifier evidence path"
        );
        assert!(
            tq["confidence"].as_f64().unwrap() >= 0.85,
            "Identifier path should have high confidence"
        );
        assert!(
            tq["assertion_count"].as_u64().unwrap() >= 2,
            "Should have counted identifier assertions"
        );
    }

    #[test]
    fn test_pipeline_integration_fixture_not_applicable() {
        let mut ctx = build_test_db("test_file.py", "python");

        ctx.db
            .store_symbols(&[symbol_builder("sym-fixture", "setUp", "test_file.py")
                .language("python")
                .code_context("self.db = create_test_db()")
                .metadata(metadata_from_json(
                    json!({ "is_test": true, "test_role": "fixture_setup" }),
                ))
                .build()])
            .unwrap();

        let stats = compute_test_quality_metrics(&ctx.db, &ctx.configs).unwrap();
        assert_eq!(stats.total_tests, 1);
        assert_eq!(stats.not_applicable, 1, "Fixture should be not_applicable");

        let meta = symbol_metadata(&ctx.db, "sym-fixture");
        assert_eq!(
            meta["test_quality"]["quality_tier"].as_str().unwrap(),
            "n/a"
        );
        assert_eq!(meta["test_quality"]["confidence"].as_f64().unwrap(), 1.0);
    }
}
