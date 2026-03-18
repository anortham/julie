//! Tests for the query_by_metrics SQL engine.

#[cfg(test)]
mod tests {
    use crate::database::SymbolDatabase;
    use crate::tools::metrics::query::{format_metrics_output, query_by_metrics};
    use tempfile::TempDir;

    /// Insert a file row (required FK for symbols.file_path → files.path).
    fn insert_file(db: &SymbolDatabase, path: &str, language: &str) {
        db.conn
            .execute(
                "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified)
                 VALUES (?1, ?2, 'h', 100, 0)",
                rusqlite::params![path, language],
            )
            .unwrap();
    }

    /// Insert a symbol with full metadata.
    fn insert_symbol(
        db: &SymbolDatabase,
        id: &str,
        name: &str,
        kind: &str,
        language: &str,
        file_path: &str,
        reference_score: f64,
        metadata_json: Option<&str>,
    ) {
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path,
                 start_line, start_col, end_line, end_col, start_byte, end_byte,
                 reference_score, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, 10, 0, 20, 0, 0, 100, ?6, ?7)",
                rusqlite::params![id, name, kind, language, file_path, reference_score, metadata_json],
            )
            .unwrap();
    }

    /// Create a test database populated with 6 symbols of varying metrics.
    fn setup_test_db() -> (TempDir, SymbolDatabase) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/core/auth.rs", "rust");
        insert_file(&db, "src/core/db.rs", "rust");
        insert_file(&db, "src/utils/helpers.py", "python");
        insert_file(&db, "src/api/handler.ts", "typescript");
        insert_file(&db, "tests/test_auth.rs", "rust");

        // S1: High security risk, high change risk, well-tested, high centrality
        insert_symbol(
            &db, "s1", "authenticate", "function", "rust", "src/core/auth.rs", 25.0,
            Some(r#"{"security_risk":{"score":0.85,"label":"HIGH"},"change_risk":{"score":0.78,"label":"HIGH"},"test_coverage":{"test_count":3,"best_tier":"thorough"}}"#),
        );

        // S2: Medium security risk, medium change risk, thinly tested, medium centrality
        insert_symbol(
            &db, "s2", "DatabasePool", "class", "rust", "src/core/db.rs", 12.0,
            Some(r#"{"security_risk":{"score":0.55,"label":"MEDIUM"},"change_risk":{"score":0.50,"label":"MEDIUM"},"test_coverage":{"test_count":1,"best_tier":"thin"}}"#),
        );

        // S3: Low security risk, low change risk, untested, low centrality
        insert_symbol(
            &db, "s3", "format_name", "function", "python", "src/utils/helpers.py", 1.0,
            Some(r#"{"security_risk":{"score":0.15,"label":"LOW"},"change_risk":{"score":0.20,"label":"LOW"}}"#),
        );

        // S4: No security risk scored, no change risk, untested, zero centrality
        insert_symbol(
            &db, "s4", "RouteHandler", "class", "typescript", "src/api/handler.ts", 0.0,
            None,
        );

        // S5: High security risk, untested, high centrality
        insert_symbol(
            &db, "s5", "execute_query", "method", "rust", "src/core/db.rs", 18.0,
            Some(r#"{"security_risk":{"score":0.92,"label":"HIGH"},"change_risk":{"score":0.71,"label":"HIGH"}}"#),
        );

        // S6: Test symbol — should be excluded by default
        insert_symbol(
            &db, "s6", "test_auth_flow", "function", "rust", "tests/test_auth.rs", 0.0,
            Some(r#"{"is_test":true,"test_quality":{"quality_tier":"thorough"}}"#),
        );

        (tmp, db)
    }

    #[test]
    fn test_query_by_security_risk_desc() {
        let (_tmp, db) = setup_test_db();
        let results = query_by_metrics(
            &db,
            "security_risk",
            "desc",
            None,   // no min_risk
            None,   // no has_tests
            None,   // no kind
            None,   // no file_pattern
            None,   // no language
            true,   // exclude tests
            10,
        )
        .unwrap();

        // Should exclude test symbol s6
        assert!(
            results.iter().all(|r| r.name != "test_auth_flow"),
            "Test symbols should be excluded by default"
        );

        // Highest security risk should be first
        assert_eq!(results[0].name, "execute_query", "s5 has highest security_risk (0.92)");
        assert_eq!(results[1].name, "authenticate", "s1 has second highest (0.85)");

        // Verify scores parsed correctly
        assert_eq!(results[0].security_risk_score, Some(0.92));
        assert_eq!(results[0].security_risk_label.as_deref(), Some("HIGH"));
    }

    #[test]
    fn test_query_by_centrality_asc_for_dead_code() {
        let (_tmp, db) = setup_test_db();
        let results = query_by_metrics(
            &db,
            "centrality",
            "asc",
            None,
            None,
            None,
            None,
            None,
            true, // exclude tests
            10,
        )
        .unwrap();

        // Lowest centrality first — s4 has 0.0
        assert_eq!(results[0].name, "RouteHandler", "s4 has lowest centrality (0.0)");
        assert_eq!(results[0].reference_score, 0.0);

        // s3 has 1.0, should be second
        assert_eq!(results[1].name, "format_name", "s3 has centrality 1.0");
    }

    #[test]
    fn test_query_min_risk_filter() {
        let (_tmp, db) = setup_test_db();
        // Filter to MEDIUM+ security risk
        let results = query_by_metrics(
            &db,
            "security_risk",
            "desc",
            Some("MEDIUM"),
            None,
            None,
            None,
            None,
            true,
            10,
        )
        .unwrap();

        // Should only return HIGH + MEDIUM: s5, s1, s2
        assert_eq!(results.len(), 3, "Should have 3 results at MEDIUM+");
        for r in &results {
            let label = r.security_risk_label.as_deref().unwrap();
            assert!(
                label == "HIGH" || label == "MEDIUM",
                "Expected HIGH or MEDIUM, got {} for {}",
                label,
                r.name
            );
        }
    }

    #[test]
    fn test_query_has_tests_filter() {
        let (_tmp, db) = setup_test_db();
        // Only untested symbols (has_tests=false)
        let results = query_by_metrics(
            &db,
            "security_risk",
            "desc",
            None,
            Some(false), // untested only
            None,
            None,
            None,
            true,
            10,
        )
        .unwrap();

        // s3, s4, s5 have no test_coverage or test_count=0
        for r in &results {
            assert!(
                r.test_count.is_none() || r.test_count == Some(0),
                "Expected untested symbol, got test_count={:?} for {}",
                r.test_count,
                r.name
            );
        }

        // s1 and s2 have tests, should not appear
        assert!(
            results.iter().all(|r| r.name != "authenticate"),
            "authenticate has tests, should be filtered out"
        );
        assert!(
            results.iter().all(|r| r.name != "DatabasePool"),
            "DatabasePool has tests, should be filtered out"
        );
    }

    #[test]
    fn test_query_kind_filter() {
        let (_tmp, db) = setup_test_db();
        // Filter to "class" only
        let results = query_by_metrics(
            &db,
            "centrality",
            "desc",
            None,
            None,
            Some("class"),
            None,
            None,
            true,
            10,
        )
        .unwrap();

        // Only s2 (DatabasePool) and s4 (RouteHandler) are classes
        assert_eq!(results.len(), 2, "Should have 2 class results");
        for r in &results {
            assert_eq!(r.kind, "class", "Expected class, got {} for {}", r.kind, r.name);
        }
    }

    #[test]
    fn test_query_file_pattern_filter() {
        let (_tmp, db) = setup_test_db();
        // Filter by glob pattern matching core/
        let results = query_by_metrics(
            &db,
            "security_risk",
            "desc",
            None,
            None,
            None,
            Some("src/core/**"),
            None,
            true,
            10,
        )
        .unwrap();

        // Only s1, s2, s5 are in src/core/
        assert!(results.len() <= 3, "At most 3 results in src/core/");
        for r in &results {
            assert!(
                r.file_path.starts_with("src/core/"),
                "Expected src/core/ path, got {} for {}",
                r.file_path,
                r.name
            );
        }
    }

    #[test]
    fn test_format_metrics_output_security_risk() {
        let (_tmp, db) = setup_test_db();
        let results = query_by_metrics(
            &db, "security_risk", "desc", None, None, None, None, None, true, 3,
        )
        .unwrap();

        let output = format_metrics_output(&results, "security_risk", "desc");

        // Header
        assert!(
            output.contains("sorted by security_risk DESC"),
            "Output should contain sort header"
        );

        // First result name
        assert!(
            output.contains("execute_query"),
            "Should contain highest risk symbol"
        );

        // Risk labels
        assert!(
            output.contains("Security: HIGH"),
            "Should contain security risk label"
        );

        // Test status
        assert!(
            output.contains("untested"),
            "Untested symbols should show as untested"
        );
    }

    #[test]
    fn test_format_metrics_output_empty() {
        let output = format_metrics_output(&[], "security_risk", "desc");
        assert_eq!(output, "No symbols match the query filters.");
    }

    #[test]
    fn test_query_language_filter() {
        let (_tmp, db) = setup_test_db();
        let results = query_by_metrics(
            &db,
            "centrality",
            "desc",
            None,
            None,
            None,
            None,
            Some("python"),
            true,
            10,
        )
        .unwrap();

        // Only s3 (format_name) is Python
        assert_eq!(results.len(), 1, "Should have 1 Python result");
        assert_eq!(results[0].name, "format_name");
    }

    #[test]
    fn test_query_has_tests_true_filter() {
        let (_tmp, db) = setup_test_db();
        // Only symbols WITH test coverage
        let results = query_by_metrics(
            &db,
            "security_risk",
            "desc",
            None,
            Some(true), // tested only
            None,
            None,
            None,
            true,
            10,
        )
        .unwrap();

        // s1 (test_count=3) and s2 (test_count=1) are tested
        assert_eq!(results.len(), 2, "Should have 2 tested results");
        for r in &results {
            assert!(
                r.test_count.unwrap_or(0) > 0,
                "Expected tested symbol, got test_count={:?} for {}",
                r.test_count,
                r.name
            );
        }
    }

    #[test]
    fn test_format_change_risk_output() {
        let (_tmp, db) = setup_test_db();
        let results = query_by_metrics(
            &db, "change_risk", "desc", None, None, None, None, None, true, 5,
        )
        .unwrap();

        let output = format_metrics_output(&results, "change_risk", "desc");

        // Header should reflect change_risk sort
        assert!(
            output.contains("sorted by change_risk DESC"),
            "Output should reflect change_risk sort"
        );

        // Should include change risk labels
        assert!(
            output.contains("Change risk: HIGH"),
            "Should contain change risk label"
        );

        // Should include centrality for each result
        assert!(
            output.contains("Centrality:"),
            "Should include centrality"
        );
    }

    #[test]
    fn test_exclude_tests_false_includes_test_symbols() {
        let (_tmp, db) = setup_test_db();
        let results = query_by_metrics(
            &db,
            "centrality",
            "asc",
            None,
            None,
            None,
            None,
            None,
            false, // include tests
            10,
        )
        .unwrap();

        // Should include test_auth_flow (s6) when exclude_tests=false
        let has_test_symbol = results.iter().any(|r| r.name == "test_auth_flow");
        assert!(
            has_test_symbol,
            "With exclude_tests=false, test symbols should be included"
        );
    }
}
