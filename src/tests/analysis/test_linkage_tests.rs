//! Tests for test-to-code linkage computation.

#[cfg(test)]
mod tests {
    use crate::analysis::test_linkage::tier_rank;
    use crate::database::SymbolDatabase;
    use crate::extractors::{RelationshipKind, SymbolKind, Visibility};
    use crate::tests::helpers::db::{
        file_info_builder, identifier_builder, relationship_builder, symbol_builder,
    };
    use tempfile::TempDir;

    #[test]
    fn test_tier_rank_ordering() {
        assert!(tier_rank("thorough") > tier_rank("adequate"));
        assert!(tier_rank("adequate") > tier_rank("thin"));
        assert!(tier_rank("thin") > tier_rank("stub"));
        assert_eq!(tier_rank("unknown"), 0);
    }

    #[test]
    fn test_tier_best_worst() {
        // "thorough" should be best, "stub" should be worst
        let tiers = vec!["thin", "thorough", "stub"];
        let best = tiers.iter().max_by_key(|t| tier_rank(t)).unwrap();
        let worst = tiers.iter().min_by_key(|t| tier_rank(t)).unwrap();
        assert_eq!(*best, "thorough");
        assert_eq!(*worst, "stub");
    }

    /// Insert a file record (required by foreign key constraint on symbols.file_path).
    fn insert_file(db: &SymbolDatabase, path: &str) {
        db.conn.execute(
            "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified) VALUES (?1, 'rust', 'h', 100, 0)",
            rusqlite::params![path],
        ).unwrap();
    }

    fn store_file(db: &SymbolDatabase, path: &str) {
        db.store_file_info(
            &file_info_builder(path)
                .hash("h")
                .size(100)
                .last_modified(0)
                .build(),
        )
        .unwrap();
    }

    /// Create a minimal database with test and production symbols + relationships.
    fn setup_test_db() -> (TempDir, SymbolDatabase) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        // Insert file records (FK constraint)
        store_file(&db, "src/payments.rs");
        store_file(&db, "src/tests/payments.rs");

        db.store_symbols(&[
            symbol_builder("prod_1", "process_payment", "src/payments.rs")
                .span(10, 0, 30, 0)
                .visibility(Visibility::Public)
                .build(),
            symbol_builder("test_1", "test_process_payment", "src/tests/payments.rs")
                .span(5, 0, 20, 0)
                .metadata(serde_json::from_str(r#"{"is_test":true,"test_quality":{"quality_tier":"thorough","assertion_count":3}}"#).unwrap())
                .visibility(Visibility::Private)
                .build(),
            symbol_builder("test_2", "test_payment_edge_case", "src/tests/payments.rs")
                .span(25, 0, 40, 0)
                .metadata(serde_json::from_str(r#"{"is_test":true,"test_quality":{"quality_tier":"thin","assertion_count":1}}"#).unwrap())
                .visibility(Visibility::Private)
                .build(),
        ])
        .unwrap();

        db.store_relationships(&[
            relationship_builder("rel_1", "test_1", "prod_1")
                .kind(RelationshipKind::Calls)
                .file_path("src/tests/payments.rs")
                .line_number(10)
                .build(),
            relationship_builder("rel_2", "test_2", "prod_1")
                .kind(RelationshipKind::Calls)
                .file_path("src/tests/payments.rs")
                .line_number(30)
                .build(),
        ])
        .unwrap();

        (temp_dir, db)
    }

    #[test]
    fn test_compute_linkage_relationship_linkage() {
        let (_temp, db) = setup_test_db();
        let stats = crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();

        assert_eq!(
            stats.symbols_covered, 1,
            "One production symbol should be covered"
        );
        assert!(stats.total_linkages >= 2, "Two test→prod relationships");

        // Verify metadata was written
        let prod = db.get_symbol_by_id("prod_1").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let linkage = meta.get("test_linkage").unwrap();
        let test_count = linkage.get("test_count").unwrap().as_u64().unwrap();
        assert_eq!(test_count, 2);
        let best = linkage.get("best_tier").unwrap().as_str().unwrap();
        assert_eq!(best, "thorough");
        let worst = linkage.get("worst_tier").unwrap().as_str().unwrap();
        assert_eq!(worst, "thin");
        let linked_tests = linkage.get("linked_tests").unwrap().as_array().unwrap();
        assert_eq!(linked_tests.len(), 2);
    }

    #[test]
    fn test_identifier_only_linkage() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        store_file(&db, "src/utils.rs");
        store_file(&db, "tests/utils_test.rs");

        // Production symbol — no relationship edges to it
        db.store_symbols(&[
            symbol_builder("prod_u", "validate_input", "src/utils.rs")
                .span(1, 0, 10, 0)
                .build(),
            symbol_builder("test_u", "test_validate", "tests/utils_test.rs")
                .span(1, 0, 5, 0)
                .metadata(
                    serde_json::from_str(
                        r#"{"is_test":true,"test_quality":{"quality_tier":"adequate"}}"#,
                    )
                    .unwrap(),
                )
                .build(),
        ])
        .unwrap();

        db.bulk_store_identifiers(
            &[
                identifier_builder("id_u", "validate_input", "tests/utils_test.rs")
                    .line(3)
                    .column(0, 20)
                    .containing_symbol_id("test_u")
                    .target_symbol_id("prod_u")
                    .build(),
            ],
            "",
        )
        .unwrap();

        let stats = crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();
        assert_eq!(
            stats.symbols_covered, 1,
            "identifier-only linkage should create test linkage"
        );

        let prod = db.get_symbol_by_id("prod_u").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let linkage = meta.get("test_linkage").unwrap();
        assert_eq!(linkage.get("test_count").unwrap().as_u64().unwrap(), 1);
        assert_eq!(
            linkage.get("best_tier").unwrap().as_str().unwrap(),
            "adequate"
        );
    }

    #[test]
    fn test_uncovered_symbol_has_no_test_linkage_key() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        store_file(&db, "src/lib.rs");

        let symbols = [symbol_builder("lonely", "lonely_function", "src/lib.rs")
            .span(1, 0, 5, 0)
            .confidence(1.0)
            .build()];
        db.store_symbols(&symbols).unwrap();

        let _stats = crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();

        let sym = db.get_symbol_by_id("lonely").unwrap().unwrap();
        if let Some(meta) = &sym.metadata {
            assert!(
                meta.get("test_linkage").is_none(),
                "uncovered symbol should not have test_linkage key"
            );
        }
    }

    #[test]
    fn test_test_to_test_relationships_excluded() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        store_file(&db, "tests/a.rs");
        store_file(&db, "tests/b.rs");

        db.store_symbols(&[
            symbol_builder("t1", "test_a", "tests/a.rs")
                .span(1, 0, 5, 0)
                .metadata(
                    serde_json::from_str(
                        r#"{"is_test": true, "test_quality": {"quality_tier": "thin"}}"#,
                    )
                    .unwrap(),
                )
                .confidence(1.0)
                .build(),
            symbol_builder("t2", "test_b", "tests/b.rs")
                .span(1, 0, 5, 0)
                .metadata(
                    serde_json::from_str(
                        r#"{"is_test": true, "test_quality": {"quality_tier": "thin"}}"#,
                    )
                    .unwrap(),
                )
                .confidence(1.0)
                .build(),
        ])
        .unwrap();

        let relationships = [relationship_builder("r1", "t1", "t2")
            .file_path("tests/a.rs")
            .line_number(3)
            .build()];
        db.store_relationships(&relationships).unwrap();

        let stats = crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();
        assert_eq!(
            stats.symbols_covered, 0,
            "test-to-test calls should not create linkage"
        );
    }

    #[test]
    fn test_linked_tests_capped_at_five() {
        let (_temp, mut db) = setup_test_db();

        insert_file(&db, "tests/extra.rs");

        // Add 5 more test symbols → 7 total tests for prod_1
        let symbols = (3..=7)
            .map(|i| {
                symbol_builder(
                    format!("test_{i}"),
                    format!("test_extra_{i}"),
                    "tests/extra.rs",
                )
                .span(i * 10, 0, i * 10 + 5, 0)
                .metadata(
                    serde_json::from_str(
                        r#"{"is_test": true, "test_quality": {"quality_tier": "adequate"}}"#,
                    )
                    .unwrap(),
                )
                .confidence(1.0)
                .build()
            })
            .collect::<Vec<_>>();
        db.store_symbols(&symbols).unwrap();

        let relationships = (3..=7)
            .map(|i| {
                relationship_builder(format!("rel_{i}"), format!("test_{i}"), "prod_1")
                    .file_path("tests/extra.rs")
                    .line_number(i * 10)
                    .build()
            })
            .collect::<Vec<_>>();
        db.store_relationships(&relationships).unwrap();

        let _stats = crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();
        let prod = db.get_symbol_by_id("prod_1").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let linkage = meta.get("test_linkage").unwrap();
        let names = linkage.get("linked_tests").unwrap().as_array().unwrap();
        assert!(
            names.len() <= 5,
            "linked_tests should be capped at 5, got {}",
            names.len()
        );
        let count = linkage.get("test_count").unwrap().as_u64().unwrap();
        assert_eq!(
            count, 7,
            "test_count should reflect all 7 tests even though linked_tests are capped"
        );
    }

    #[test]
    fn test_name_match_prefers_class_name_similarity() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        store_file(&db, "src/Services/LabTestService.cs");
        store_file(&db, "src/Services/MediaService.cs");
        store_file(&db, "tests/Services/LabTestServiceTests.cs");

        db.store_symbols(&[
            symbol_builder(
                "prod_labtest",
                "ListAsync",
                "src/Services/LabTestService.cs",
            )
            .kind(SymbolKind::Method)
            .language("csharp")
            .span(20, 0, 50, 0)
            .confidence(1.0)
            .visibility(Visibility::Public)
            .build(),
            symbol_builder("prod_media", "ListAsync", "src/Services/MediaService.cs")
                .kind(SymbolKind::Method)
                .language("csharp")
                .span(20, 0, 50, 0)
                .confidence(1.0)
                .visibility(Visibility::Public)
                .build(),
            symbol_builder(
                "test_1",
                "ListAsync_ReturnsResults",
                "tests/Services/LabTestServiceTests.cs",
            )
            .kind(SymbolKind::Method)
            .language("csharp")
            .span(30, 0, 45, 0)
            .metadata(
                serde_json::from_str(
                    r#"{"is_test": true, "test_quality": {"quality_tier": "adequate"}}"#,
                )
                .unwrap(),
            )
            .confidence(1.0)
            .visibility(Visibility::Private)
            .build(),
        ])
        .unwrap();

        db.bulk_store_identifiers(
            &[identifier_builder(
                "ident_1",
                "ListAsync",
                "tests/Services/LabTestServiceTests.cs",
            )
            .language("csharp")
            .line(41)
            .column(0, 20)
            .containing_symbol_id("test_1")
            .build()],
            "",
        )
        .unwrap();

        let stats = crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();
        assert_eq!(stats.symbols_covered, 1, "Should cover exactly one symbol");

        let cov: Option<String> = db.conn.query_row(
            "SELECT json_extract(metadata, '$.test_linkage') FROM symbols WHERE id = 'prod_labtest'",
            [], |row| row.get(0)
        ).unwrap();
        assert!(
            cov.is_some(),
            "LabTestService.ListAsync should have test linkage"
        );

        let no_cov: Option<String> = db.conn.query_row(
            "SELECT json_extract(metadata, '$.test_linkage') FROM symbols WHERE id = 'prod_media'",
            [], |row| row.get(0)
        ).unwrap();
        assert!(
            no_cov.is_none(),
            "MediaService.ListAsync should NOT have test linkage from LabTestServiceTests"
        );
    }

    #[test]
    fn test_name_match_ambiguity_guard_is_language_scoped() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/widgets.rs");
        insert_file(&db, "tests/widgets_test.rs");
        for index in 0..11 {
            insert_file(&db, &format!("python/widget_{index}.py"));
        }

        let mut symbols = vec![
            symbol_builder("prod_rust", "render_widget", "src/widgets.rs")
                .kind(SymbolKind::Function)
                .language("rust")
                .span(1, 0, 10, 0)
                .visibility(Visibility::Public)
                .confidence(1.0)
                .build(),
        ];

        symbols.extend((0..11).map(|index| {
            symbol_builder(
                format!("prod_python_{index}"),
                "render_widget",
                format!("python/widget_{index}.py"),
            )
            .kind(SymbolKind::Function)
            .language("python")
            .span(1, 0, 10, 0)
            .visibility(Visibility::Public)
            .confidence(1.0)
            .build()
        }));

        symbols.push(
            symbol_builder("test_rust", "test_render_widget", "tests/widgets_test.rs")
                .kind(SymbolKind::Function)
                .language("rust")
                .span(1, 0, 5, 0)
                .metadata(
                    serde_json::from_str(
                        r#"{"is_test": true, "test_quality": {"quality_tier": "adequate"}}"#,
                    )
                    .unwrap(),
                )
                .visibility(Visibility::Private)
                .confidence(1.0)
                .build(),
        );

        db.store_symbols(&symbols).unwrap();
        db.bulk_store_identifiers(
            &[
                identifier_builder("ident_rust", "render_widget", "tests/widgets_test.rs")
                    .language("rust")
                    .line(3)
                    .column(0, 20)
                    .containing_symbol_id("test_rust")
                    .build(),
            ],
            "",
        )
        .unwrap();

        let stats = crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();
        assert_eq!(
            stats.symbols_covered, 1,
            "cross-language symbols with the same name must not make a Rust fallback ambiguous"
        );

        let rust_linkage: Option<String> = db
            .conn
            .query_row(
                "SELECT json_extract(metadata, '$.test_linkage') FROM symbols WHERE id = 'prod_rust'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            rust_linkage.is_some(),
            "Rust production symbol should receive same-language fallback linkage"
        );

        let python_linked_count: u32 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM symbols
                 WHERE language = 'python'
                   AND json_extract(metadata, '$.test_linkage') IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            python_linked_count, 0,
            "Python production symbols must not receive linkage from a Rust test"
        );
    }

    #[test]
    fn test_class_inherits_method_linkage() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/services.rs");
        insert_file(&db, "tests/services_test.rs");

        db.store_symbols(&[
            symbol_builder("class_1", "PaymentService", "src/services.rs")
                .kind(SymbolKind::Class)
                .language("csharp")
                .span(1, 0, 50, 0)
                .visibility(Visibility::Public)
                .confidence(1.0)
                .build(),
            symbol_builder("method_1", "ProcessPayment", "src/services.rs")
                .kind(SymbolKind::Method)
                .language("csharp")
                .span(10, 0, 30, 0)
                .visibility(Visibility::Public)
                .parent_id("class_1")
                .confidence(1.0)
                .build(),
            symbol_builder("test_1", "test_process_payment", "tests/services_test.rs")
                .kind(SymbolKind::Method)
                .language("csharp")
                .span(5, 0, 20, 0)
                .metadata(
                    serde_json::from_str(
                        r#"{"is_test": true, "test_quality": {"quality_tier": "thorough"}}"#,
                    )
                    .unwrap(),
                )
                .visibility(Visibility::Private)
                .confidence(1.0)
                .build(),
        ])
        .unwrap();

        db.store_relationships(&[relationship_builder("rel_1", "test_1", "method_1")
            .file_path("tests/services_test.rs")
            .line_number(10)
            .build()])
            .unwrap();

        let _stats = crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();

        let method_cov: Option<String> = db.conn.query_row(
            "SELECT json_extract(metadata, '$.test_linkage') FROM symbols WHERE id = 'method_1'",
            [], |row| row.get(0)
        ).unwrap();
        assert!(method_cov.is_some(), "Method should have test linkage");

        let class_cov: Option<String> = db
            .conn
            .query_row(
                "SELECT json_extract(metadata, '$.test_linkage') FROM symbols WHERE id = 'class_1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            class_cov.is_some(),
            "Class should inherit test linkage from its methods"
        );
    }

    #[test]
    fn test_deduplication_across_strategies() {
        let (_temp, mut db) = setup_test_db();

        // Add an identifier that links test_1 → prod_1 (same linkage as the relationship)
        db.bulk_store_identifiers(
            &[
                identifier_builder("ident_1", "process_payment", "src/tests/payments.rs")
                    .line(12)
                    .column(0, 20)
                    .containing_symbol_id("test_1")
                    .target_symbol_id("prod_1")
                    .build(),
            ],
            "",
        )
        .unwrap();

        let _stats = crate::analysis::test_linkage::compute_test_linkage(&db).unwrap();
        let prod = db.get_symbol_by_id("prod_1").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let linkage = meta.get("test_linkage").unwrap();
        let count = linkage.get("test_count").unwrap().as_u64().unwrap();
        assert_eq!(
            count, 2,
            "Duplicate test_1→prod_1 from identifier should be deduped"
        );
    }
}
