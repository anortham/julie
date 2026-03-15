//! Tests for test-to-code linkage computation.

#[cfg(test)]
mod tests {
    use crate::analysis::test_coverage::{tier_rank, TestCoverageInfo};
    use crate::database::SymbolDatabase;
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

    /// Create a minimal database with test and production symbols + relationships.
    fn setup_test_db() -> (TempDir, SymbolDatabase) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Insert file records (FK constraint)
        insert_file(&db, "src/payments.rs");
        insert_file(&db, "src/tests/payments.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('prod_1', 'process_payment', 'function', 'rust', 'src/payments.rs', 10, 0, 30, 0, 0, 0, NULL, 5.0, 'public');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_1', 'test_process_payment', 'function', 'rust', 'src/tests/payments.rs', 5, 0, 20, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "thorough", "assertion_count": 3}}', 0.0, 'private');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_2', 'test_payment_edge_case', 'function', 'rust', 'src/tests/payments.rs', 25, 0, 40, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "thin", "assertion_count": 1}}', 0.0, 'private');

            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_1', 'test_1', 'prod_1', 'calls', 'src/tests/payments.rs', 10);

            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_2', 'test_2', 'prod_1', 'calls', 'src/tests/payments.rs', 30);
        "#).unwrap();

        (temp_dir, db)
    }

    #[test]
    fn test_compute_coverage_relationship_linkage() {
        let (_temp, db) = setup_test_db();
        let stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();

        assert_eq!(stats.symbols_covered, 1, "One production symbol should be covered");
        assert!(stats.total_linkages >= 2, "Two test→prod relationships");

        // Verify metadata was written
        let prod = db.get_symbol_by_id("prod_1").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let coverage = meta.get("test_coverage").unwrap();
        let test_count = coverage.get("test_count").unwrap().as_u64().unwrap();
        assert_eq!(test_count, 2);
        let best = coverage.get("best_tier").unwrap().as_str().unwrap();
        assert_eq!(best, "thorough");
        let worst = coverage.get("worst_tier").unwrap().as_str().unwrap();
        assert_eq!(worst, "thin");
    }

    #[test]
    fn test_identifier_only_linkage() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/utils.rs");
        insert_file(&db, "tests/utils_test.rs");

        // Production symbol — no relationship edges to it
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score)
            VALUES ('prod_u', 'validate_input', 'function', 'rust', 'src/utils.rs', 1, 0, 10, 0, 0, 0, NULL, 2.0);

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score)
            VALUES ('test_u', 'test_validate', 'function', 'rust', 'tests/utils_test.rs', 1, 0, 5, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "adequate"}}', 0.0);

            INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id, target_symbol_id)
            VALUES ('id_u', 'validate_input', 'call', 'rust', 'tests/utils_test.rs', 3, 0, 3, 20, 'test_u', 'prod_u');
        "#).unwrap();

        let stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();
        assert_eq!(stats.symbols_covered, 1, "Identifier-only linkage should create coverage");

        let prod = db.get_symbol_by_id("prod_u").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let coverage = meta.get("test_coverage").unwrap();
        assert_eq!(coverage.get("test_count").unwrap().as_u64().unwrap(), 1);
        assert_eq!(coverage.get("best_tier").unwrap().as_str().unwrap(), "adequate");
    }

    #[test]
    fn test_uncovered_symbol_has_no_test_coverage_key() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/lib.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score)
            VALUES ('lonely', 'lonely_function', 'function', 'rust', 'src/lib.rs', 1, 0, 5, 0, 0, 0, NULL, 0.0);
        "#).unwrap();

        let _stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();

        let sym = db.get_symbol_by_id("lonely").unwrap().unwrap();
        if let Some(meta) = &sym.metadata {
            assert!(meta.get("test_coverage").is_none(), "Uncovered symbol should not have test_coverage key");
        }
    }

    #[test]
    fn test_test_to_test_relationships_excluded() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "tests/a.rs");
        insert_file(&db, "tests/b.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata)
            VALUES ('t1', 'test_a', 'function', 'rust', 'tests/a.rs', 1, 0, 5, 0, 0, 0, '{"is_test": true, "test_quality": {"quality_tier": "thin"}}');
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata)
            VALUES ('t2', 'test_b', 'function', 'rust', 'tests/b.rs', 1, 0, 5, 0, 0, 0, '{"is_test": true, "test_quality": {"quality_tier": "thin"}}');
            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('r1', 't1', 't2', 'calls', 'tests/a.rs', 3);
        "#).unwrap();

        let stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();
        assert_eq!(stats.symbols_covered, 0, "Test-to-test calls should not create coverage");
    }

    #[test]
    fn test_covering_tests_capped_at_five() {
        let (_temp, db) = setup_test_db();

        insert_file(&db, "tests/extra.rs");

        // Add 5 more test symbols → 7 total tests for prod_1
        for i in 3..=7 {
            db.conn.execute(&format!(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata)
                 VALUES ('test_{}', 'test_extra_{}', 'function', 'rust', 'tests/extra.rs', {}, 0, {}, 0, 0, 0,
                         '{{\"is_test\": true, \"test_quality\": {{\"quality_tier\": \"adequate\"}}}}')",
                i, i, i * 10, i * 10 + 5
            ), []).unwrap();
            db.conn.execute(&format!(
                "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
                 VALUES ('rel_{}', 'test_{}', 'prod_1', 'calls', 'tests/extra.rs', {})",
                i, i, i * 10
            ), []).unwrap();
        }

        let _stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();
        let prod = db.get_symbol_by_id("prod_1").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let coverage = meta.get("test_coverage").unwrap();
        let names = coverage.get("covering_tests").unwrap().as_array().unwrap();
        assert!(names.len() <= 5, "covering_tests should be capped at 5, got {}", names.len());
        let count = coverage.get("test_count").unwrap().as_u64().unwrap();
        assert_eq!(count, 7, "test_count should reflect all 7 tests even though names are capped");
    }

    #[test]
    fn test_name_match_prefers_class_name_similarity() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/Services/LabTestService.cs");
        insert_file(&db, "src/Services/MediaService.cs");
        insert_file(&db, "tests/Services/LabTestServiceTests.cs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('prod_labtest', 'ListAsync', 'method', 'csharp', 'src/Services/LabTestService.cs', 20, 0, 50, 0, 0, 0, NULL, 3.0, 'public');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('prod_media', 'ListAsync', 'method', 'csharp', 'src/Services/MediaService.cs', 20, 0, 50, 0, 0, 0, NULL, 3.0, 'public');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_1', 'ListAsync_ReturnsResults', 'method', 'csharp', 'tests/Services/LabTestServiceTests.cs', 30, 0, 45, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "adequate"}}', 0.0, 'private');

            INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id, target_symbol_id)
            VALUES ('ident_1', 'ListAsync', 'call', 'csharp', 'tests/Services/LabTestServiceTests.cs', 41, 0, 41, 20, 'test_1', NULL);
        "#).unwrap();

        let stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();
        assert_eq!(stats.symbols_covered, 1, "Should cover exactly one symbol");

        let cov: Option<String> = db.conn.query_row(
            "SELECT json_extract(metadata, '$.test_coverage') FROM symbols WHERE id = 'prod_labtest'",
            [], |row| row.get(0)
        ).unwrap();
        assert!(cov.is_some(), "LabTestService.ListAsync should have test coverage");

        let no_cov: Option<String> = db.conn.query_row(
            "SELECT json_extract(metadata, '$.test_coverage') FROM symbols WHERE id = 'prod_media'",
            [], |row| row.get(0)
        ).unwrap();
        assert!(no_cov.is_none(), "MediaService.ListAsync should NOT have test coverage from LabTestServiceTests");
    }

    #[test]
    fn test_class_inherits_method_coverage() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/services.rs");
        insert_file(&db, "tests/services_test.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('class_1', 'PaymentService', 'class', 'csharp', 'src/services.rs', 1, 0, 50, 0, 0, 0, NULL, 5.0, 'public');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility, parent_id)
            VALUES ('method_1', 'ProcessPayment', 'method', 'csharp', 'src/services.rs', 10, 0, 30, 0, 0, 0, NULL, 3.0, 'public', 'class_1');

            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, metadata, reference_score, visibility)
            VALUES ('test_1', 'test_process_payment', 'method', 'csharp', 'tests/services_test.rs', 5, 0, 20, 0, 0, 0,
                    '{"is_test": true, "test_quality": {"quality_tier": "thorough"}}', 0.0, 'private');

            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('rel_1', 'test_1', 'method_1', 'calls', 'tests/services_test.rs', 10);
        "#).unwrap();

        let _stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();

        let method_cov: Option<String> = db.conn.query_row(
            "SELECT json_extract(metadata, '$.test_coverage') FROM symbols WHERE id = 'method_1'",
            [], |row| row.get(0)
        ).unwrap();
        assert!(method_cov.is_some(), "Method should have test coverage");

        let class_cov: Option<String> = db.conn.query_row(
            "SELECT json_extract(metadata, '$.test_coverage') FROM symbols WHERE id = 'class_1'",
            [], |row| row.get(0)
        ).unwrap();
        assert!(class_cov.is_some(), "Class should inherit test coverage from its methods");
    }

    #[test]
    fn test_deduplication_across_strategies() {
        let (_temp, db) = setup_test_db();

        // Add an identifier that links test_1 → prod_1 (same linkage as the relationship)
        db.conn.execute(
            "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id, target_symbol_id)
             VALUES ('ident_1', 'process_payment', 'call', 'rust', 'src/tests/payments.rs', 12, 0, 12, 20, 'test_1', 'prod_1')",
            [],
        ).unwrap();

        let _stats = crate::analysis::test_coverage::compute_test_coverage(&db).unwrap();
        let prod = db.get_symbol_by_id("prod_1").unwrap().unwrap();
        let meta = prod.metadata.unwrap();
        let coverage = meta.get("test_coverage").unwrap();
        let count = coverage.get("test_count").unwrap().as_u64().unwrap();
        assert_eq!(count, 2, "Duplicate test_1→prod_1 from identifier should be deduped");
    }
}
