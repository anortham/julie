//! Tests for structural security risk analysis.

#[cfg(test)]
mod tests {
    use crate::analysis::security_risk::*;
    use crate::extractors::SymbolKind;

    // =========================================================================
    // Exposure signal
    // =========================================================================

    #[test]
    fn test_exposure_public_function() {
        let score = exposure_score(Some("public"), &SymbolKind::Function);
        assert_eq!(score, 1.0);
    }

    #[test]
    fn test_exposure_private_function() {
        let score = exposure_score(Some("private"), &SymbolKind::Function);
        assert!((score - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_exposure_public_struct() {
        // Container kind_weight = 0.3 for security
        let score = exposure_score(Some("public"), &SymbolKind::Struct);
        assert!((score - 0.3).abs() < 0.01);
    }

    #[test]
    fn test_exposure_null_visibility() {
        let score = exposure_score(None, &SymbolKind::Function);
        assert_eq!(score, 0.5); // NULL = moderate
    }

    // =========================================================================
    // Input handling signal
    // =========================================================================

    #[test]
    fn test_input_handling_rust_str_param() {
        assert!(has_input_handling(Some(
            "pub fn process(input: &str) -> Result<()>"
        )));
    }

    #[test]
    fn test_input_handling_java_request() {
        assert!(has_input_handling(Some(
            "public void handle(HttpServletRequest req, HttpServletResponse resp)"
        )));
    }

    #[test]
    fn test_input_handling_python_string() {
        assert!(has_input_handling(Some("def process(data: str) -> bool")));
    }

    #[test]
    fn test_input_handling_no_match() {
        assert!(!has_input_handling(Some(
            "pub fn compute(count: u32) -> u64"
        )));
    }

    #[test]
    fn test_input_handling_return_type_not_matched() {
        // String is in return type, not params — should NOT match
        assert!(!has_input_handling(Some(
            "pub fn get_name(id: u32) -> String"
        )));
    }

    #[test]
    fn test_input_handling_none_signature() {
        assert!(!has_input_handling(None));
    }

    #[test]
    fn test_input_handling_empty_signature() {
        assert!(!has_input_handling(Some("")));
    }

    #[test]
    fn test_input_handling_request_delegate_excluded() {
        // RequestDelegate is a DI type, not actual HTTP request handling
        assert!(!has_input_handling(Some(
            "(RequestDelegate next, ILogger<RoleClaimsMiddleware> logger)"
        )));
    }

    #[test]
    fn test_input_handling_real_http_request_still_matches() {
        assert!(has_input_handling(Some("(HttpRequest req, string id)")));
    }

    #[test]
    fn test_input_handling_ilogger_excluded() {
        // ILogger<String> and IOptions<Foo> are DI types — String inside them is not user input
        assert!(!has_input_handling(Some(
            "(ILogger<Foo> logger, IOptions<Bar> opts)"
        )));
    }

    // =========================================================================
    // Sink matching
    // =========================================================================

    #[test]
    fn test_sink_match_exact() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(
            matches_sink_pattern("exec", patterns),
            Some("exec".to_string())
        );
    }

    #[test]
    fn test_sink_match_qualified_name() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(
            matches_sink_pattern("db.execute", patterns),
            Some("execute".to_string())
        );
    }

    #[test]
    fn test_sink_match_rust_qualified() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(
            matches_sink_pattern("conn::execute", patterns),
            Some("execute".to_string())
        );
    }

    #[test]
    fn test_sink_match_case_insensitive() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(
            matches_sink_pattern("db.Exec", patterns),
            Some("exec".to_string())
        );
    }

    #[test]
    fn test_sink_no_match_substring() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(matches_sink_pattern("execution_context", patterns), None);
    }

    #[test]
    fn test_sink_no_match_prefix() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(matches_sink_pattern("executor", patterns), None);
    }

    #[test]
    fn test_sink_match_efcore_savechanges() {
        let callees = vec!["SaveChangesAsync".to_string()];
        let patterns: Vec<&str> = EXECUTION_SINKS
            .iter()
            .chain(DATABASE_SINKS.iter())
            .copied()
            .collect();
        let (score, matched) = compute_sink_signal(&callees, &[], &patterns);
        assert!(score > 0.0, "SaveChangesAsync should match a sink pattern");
        assert!(!matched.is_empty());
    }

    #[test]
    fn test_sink_match_django_raw() {
        let patterns: Vec<&str> = EXECUTION_SINKS
            .iter()
            .chain(DATABASE_SINKS.iter())
            .copied()
            .collect();
        let (score, _) = compute_sink_signal(&["queryset.raw".to_string()], &[], &patterns);
        assert!(score > 0.0, "Django raw() should match a sink pattern");
    }

    #[test]
    fn test_sink_match_prisma_findmany() {
        let patterns: Vec<&str> = EXECUTION_SINKS
            .iter()
            .chain(DATABASE_SINKS.iter())
            .copied()
            .collect();
        let (score, _) = compute_sink_signal(&["prisma.findMany".to_string()], &[], &patterns);
        assert!(score > 0.0, "Prisma findMany should match a sink pattern");
    }

    #[test]
    fn test_sink_match_jpa_persist() {
        let patterns: Vec<&str> = EXECUTION_SINKS
            .iter()
            .chain(DATABASE_SINKS.iter())
            .copied()
            .collect();
        let (score, _) = compute_sink_signal(&["em.persist".to_string()], &[], &patterns);
        assert!(score > 0.0, "JPA persist should match a sink pattern");
    }

    // =========================================================================
    // Sink signal computation
    // =========================================================================

    #[test]
    fn test_sink_signal_no_matches() {
        let (score, names) = compute_sink_signal(&["foo".into()], &[], &["exec", "execute"]);
        assert_eq!(score, 0.0);
        assert!(names.is_empty());
    }

    #[test]
    fn test_sink_signal_one_match() {
        let (score, names) = compute_sink_signal(&["db.execute".into()], &[], &["exec", "execute"]);
        assert!((score - 0.7).abs() < 0.01);
        assert_eq!(names, vec!["execute"]);
    }

    #[test]
    fn test_sink_signal_multiple_matches() {
        let (score, names) = compute_sink_signal(
            &["db.execute".into(), "os.exec".into()],
            &[],
            &["exec", "execute"],
        );
        assert_eq!(score, 1.0);
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn test_sink_signal_deduplicates() {
        let (score, names) = compute_sink_signal(
            &["db.execute".into()],
            &["execute".into()], // same sink from relationship
            &["exec", "execute"],
        );
        assert!((score - 0.7).abs() < 0.01); // Still just one unique sink
        assert_eq!(names.len(), 1);
    }

    // =========================================================================
    // Score computation
    // =========================================================================

    #[test]
    fn test_high_security_risk() {
        // Public function, accepts string input, calls execute, high centrality, untested
        let score = compute_score(1.0, 1.0, 0.7, 0.8, 1.0);
        assert!(score >= 0.7, "Should be HIGH, got {:.2}", score);
        assert_eq!(risk_label(score), "HIGH");
    }

    #[test]
    fn test_low_security_risk() {
        // Private, no input handling, no sinks, low centrality, tested
        let score = compute_score(0.2, 0.0, 0.0, 0.1, 0.0);
        assert!(score < 0.4, "Should be LOW, got {:.2}", score);
        assert_eq!(risk_label(score), "LOW");
    }

    #[test]
    fn test_risk_label_boundaries() {
        assert_eq!(risk_label(0.7), "HIGH");
        assert_eq!(risk_label(0.69), "MEDIUM");
        assert_eq!(risk_label(0.4), "MEDIUM");
        assert_eq!(risk_label(0.39), "LOW");
    }

    // =========================================================================
    // Parameter extraction
    // =========================================================================

    #[test]
    fn test_extract_params_rust() {
        let params = extract_parameter_portion("pub fn process(input: &str) -> Result<()>");
        assert!(params.contains("&str"));
        assert!(!params.contains("Result"));
    }

    #[test]
    fn test_extract_params_no_return_type() {
        let params = extract_parameter_portion("def process(data)");
        assert_eq!(params, "def process(data)");
    }

    #[test]
    fn test_extract_params_closing_paren() {
        let params = extract_parameter_portion("void handle(Request req)");
        assert!(params.contains("Request"));
    }

    // =========================================================================
    // Visibility string stored in metadata
    // =========================================================================

    #[test]
    fn test_security_risk_metadata_stores_visibility_string() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/handler.rs");

        // Public function — should store visibility: "public"
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, signature, metadata)
            VALUES ('pub_fn', 'handle', 'function', 'rust', 'src/handler.rs', 1, 0, 20, 0, 0, 0,
                    10.0, 'public', 'pub fn handle(input: &str)', NULL);
        "#).unwrap();

        // Protected method — should store visibility: "protected"
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, signature, metadata)
            VALUES ('prot_fn', 'on_update', 'method', 'csharp', 'src/handler.rs', 30, 0, 50, 0, 0, 0,
                    10.0, 'protected', 'protected void OnUpdate(string data)', NULL);
        "#).unwrap();

        crate::analysis::security_risk::compute_security_risk(&db).unwrap();

        // Check public symbol stores visibility: "public"
        let pub_sym = db.get_symbol_by_id("pub_fn").unwrap().unwrap();
        let pub_meta = pub_sym.metadata.unwrap();
        let pub_sec = pub_meta.get("security_risk").unwrap();
        let pub_vis = pub_sec
            .pointer("/signals/visibility")
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(
            pub_vis, "public",
            "public symbol should store visibility: 'public'"
        );

        // Check protected symbol stores visibility: "protected"
        let prot_sym = db.get_symbol_by_id("prot_fn").unwrap().unwrap();
        let prot_meta = prot_sym.metadata.unwrap();
        let prot_sec = prot_meta.get("security_risk").unwrap();
        let prot_vis = prot_sec
            .pointer("/signals/visibility")
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(
            prot_vis, "protected",
            "protected symbol should store visibility: 'protected'"
        );
    }

    // =========================================================================
    // filter is NOT a database sink (false-positive prone)
    // =========================================================================

    #[test]
    fn test_filter_is_not_a_database_sink() {
        // "filter" is too generic — Rust iterators, LINQ, JS array methods all use it.
        // It should NOT be in DATABASE_SINKS.
        assert_eq!(
            matches_sink_pattern("filter", &DATABASE_SINKS),
            None,
            "bare 'filter' should not match DATABASE_SINKS"
        );
        assert_eq!(
            matches_sink_pattern("items.filter", &DATABASE_SINKS),
            None,
            "'items.filter' should not match DATABASE_SINKS"
        );
    }

    // =========================================================================
    // Integration tests: compute_security_risk
    // =========================================================================

    use crate::database::SymbolDatabase;
    use tempfile::TempDir;

    fn insert_file(db: &SymbolDatabase, path: &str) {
        db.conn.execute(
            "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified) VALUES (?1, 'rust', 'h', 100, 0)",
            rusqlite::params![path],
        ).unwrap();
    }

    #[test]
    fn test_compute_security_risk_high_risk_symbol() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/handler.rs");
        insert_file(&db, "src/utils.rs");

        // High-risk: public function with string params that calls execute
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, signature, metadata)
            VALUES ('s1', 'process_request', 'function', 'rust', 'src/handler.rs', 1, 0, 20, 0, 0, 0,
                    15.0, 'public', 'pub fn process_request(input: &str) -> Result<()>', NULL);
        "#).unwrap();

        // The sink it calls
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, metadata)
            VALUES ('s_sink', 'execute', 'function', 'rust', 'src/utils.rs', 1, 0, 5, 0, 0, 0, 0.0, 'public', NULL);
        "#).unwrap();

        // Relationship: s1 calls execute
        db.conn.execute_batch(r#"
            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('r1', 's1', 's_sink', 'calls', 'src/handler.rs', 10);
        "#).unwrap();

        // Also an identifier call
        db.conn.execute_batch(r#"
            INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id)
            VALUES ('i1', 'execute', 'call', 'rust', 'src/handler.rs', 10, 0, 10, 15, 's1');
        "#).unwrap();

        let stats = crate::analysis::security_risk::compute_security_risk(&db).unwrap();
        assert!(stats.total_scored >= 1, "Should score at least s1");
        assert!(stats.high_risk >= 1, "s1 should be HIGH risk");

        // Verify metadata
        let s1 = db.get_symbol_by_id("s1").unwrap().unwrap();
        let meta = s1.metadata.unwrap();
        let security = meta.get("security_risk").unwrap();
        let label = security.get("label").unwrap().as_str().unwrap();
        assert_eq!(label, "HIGH");
        let signals = security.get("signals").unwrap();
        let sinks = signals.get("sink_calls").unwrap().as_array().unwrap();
        assert!(!sinks.is_empty(), "Should detect execute as a sink");
    }

    #[test]
    fn test_compute_security_risk_no_signals_no_key() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/lib.rs");

        // Private function with integer params, no sink calls
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, signature, metadata)
            VALUES ('safe', 'add_numbers', 'function', 'rust', 'src/lib.rs', 1, 0, 5, 0, 0, 0,
                    0.0, 'private', 'fn add_numbers(a: i32, b: i32) -> i32', NULL);
        "#).unwrap();

        let _stats = crate::analysis::security_risk::compute_security_risk(&db).unwrap();

        let sym = db.get_symbol_by_id("safe").unwrap().unwrap();
        if let Some(meta) = &sym.metadata {
            assert!(
                meta.get("security_risk").is_none(),
                "Symbol with no security signals should not have security_risk key"
            );
        }
    }

    #[test]
    fn test_compute_security_risk_excludes_test_symbols() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "tests/test.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, signature, metadata)
            VALUES ('t1', 'test_exec', 'function', 'rust', 'tests/test.rs', 1, 0, 5, 0, 0, 0,
                    0.0, 'private', 'fn test_exec()', '{"is_test": true}');
        "#).unwrap();

        let stats = crate::analysis::security_risk::compute_security_risk(&db).unwrap();
        assert_eq!(stats.total_scored, 0, "Test symbols should be excluded");
    }

    #[test]
    fn test_compute_security_risk_excludes_imports() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/lib.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, metadata)
            VALUES ('imp', 'use_exec', 'import', 'rust', 'src/lib.rs', 1, 0, 1, 0, 0, 0, 0.0, 'public', NULL);
        "#).unwrap();

        let stats = crate::analysis::security_risk::compute_security_risk(&db).unwrap();
        assert_eq!(stats.total_scored, 0, "Import symbols should be excluded");
    }
}
