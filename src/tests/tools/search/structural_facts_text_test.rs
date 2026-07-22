/// Phase 3 tests: web structural-fact paths (HTTP route templates, SQL table
/// names) are indexed as searchable text on their containing symbol so
/// `fast_search` can find a handler by route path and a routine by table name.
///
/// Invariants:
///   1. `route_handler_findable_by_route_path` — a route-handler symbol is
///      surfaced by searching for a distinctive route segment that appears only
///      in the `symfony.route.v1` fact, not in the symbol name/body.
///   2. `sql_routine_findable_by_table_name` — a routine symbol is surfaced by
///      searching for the table name that appears only in its
///      `sql.update_statement.v1` fact.
///   3. `no_facts_preserves_name_search` — a symbol with no structural facts
///      is still findable by its name (parity: empty fact blob changes
///      nothing).

#[cfg(test)]
mod structural_facts_text_test {
    use tempfile::TempDir;

    use crate::database::SymbolDatabase;
    use crate::database::bulk::atomic::{AtomicPersistenceMetadata, CanonicalWriteSet};
    use crate::database::types::FileInfo;
    use crate::extractors::base::StructuralFact;
    use crate::extractors::{Symbol, SymbolKind};
    use crate::search::index::UnifiedHit;
    use crate::search::projection::apply_documents_with_db;
    use crate::search::projection::collect_structural_facts_text_bounded;
    use crate::search::{SearchFilter, SearchIndex, SearchProjection};

    fn make_db(dir: &TempDir) -> SymbolDatabase {
        let db_path = dir.path().join("symbols.db");
        SymbolDatabase::new(&db_path).expect("create test db")
    }

    fn make_index(dir: &TempDir) -> SearchIndex {
        let index_dir = dir.path().join("index");
        std::fs::create_dir_all(&index_dir).unwrap();
        SearchIndex::create(&index_dir).expect("create test index")
    }

    fn make_symbol(
        id: &str,
        name: &str,
        kind: SymbolKind,
        file_path: &str,
        language: &str,
    ) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            language: language.to_string(),
            file_path: file_path.to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 5,
            end_column: 0,
            start_byte: 0,
            end_byte: 64,
            signature: Some(format!("fn {name}()")),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            // Empty body so name/body matches do not pollute the fact-isolation
            // assertions (the searched token appears ONLY in the fact).
            code_context: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        }
    }

    fn make_file_info(path: &str, language: &str, symbol_count: i32) -> FileInfo {
        FileInfo {
            path: path.to_string(),
            language: language.to_string(),
            hash: format!("hash-{path}"),
            size: 50,
            last_modified: 0,
            last_indexed: 0,
            symbol_count,
            line_count: 5,
            content: Some(format!("fn symbol() {{}}")),
        }
    }

    fn route_fact(
        id: &str,
        file_path: &str,
        line: u32,
        symbol_id: &str,
        verb: &str,
        template: &str,
    ) -> StructuralFact {
        StructuralFact {
            id: id.to_string(),
            file_path: file_path.to_string(),
            language: "php".to_string(),
            pattern_id: "symfony.route.v1".to_string(),
            capture_name: "route".to_string(),
            node_kind: "route".to_string(),
            containing_symbol_id: Some(symbol_id.to_string()),
            start_line: line,
            start_column: 0,
            end_line: line,
            end_column: 50,
            start_byte: line * 10,
            end_byte: line * 10 + 50,
            confidence: 0.9,
            metadata: Some(std::collections::HashMap::from([
                ("verb".to_string(), serde_json::json!(verb)),
                (
                    "normalized_route_template".to_string(),
                    serde_json::json!(template),
                ),
            ])),
        }
    }

    fn update_fact(
        id: &str,
        file_path: &str,
        line: u32,
        symbol_id: &str,
        table_name: &str,
    ) -> StructuralFact {
        StructuralFact {
            id: id.to_string(),
            file_path: file_path.to_string(),
            language: "sql".to_string(),
            pattern_id: "sql.update_statement.v1".to_string(),
            capture_name: "update".to_string(),
            node_kind: "update".to_string(),
            containing_symbol_id: Some(symbol_id.to_string()),
            start_line: line,
            start_column: 0,
            end_line: line,
            end_column: 40,
            start_byte: line * 10,
            end_byte: line * 10 + 40,
            confidence: 1.0,
            metadata: Some(std::collections::HashMap::from([
                ("table_name".to_string(), serde_json::json!(table_name)),
                ("has_where".to_string(), serde_json::json!(true)),
            ])),
        }
    }

    fn metadata_fact(
        id: &str,
        pattern_id: &str,
        file_path: &str,
        line: u32,
        symbol_id: &str,
        metadata: std::collections::HashMap<String, serde_json::Value>,
    ) -> StructuralFact {
        StructuralFact {
            id: id.to_string(),
            file_path: file_path.to_string(),
            language: "typescript".to_string(),
            pattern_id: pattern_id.to_string(),
            capture_name: "node".to_string(),
            node_kind: "call_expression".to_string(),
            containing_symbol_id: Some(symbol_id.to_string()),
            start_line: line,
            start_column: 0,
            end_line: line,
            end_column: 20,
            start_byte: line * 10,
            end_byte: line * 10 + 20,
            confidence: 1.0,
            metadata: Some(metadata),
        }
    }

    /// Seed a file + symbols + structural facts atomically, then project symbols
    /// into the search index with DB-backed enrichment (the path that
    /// populates relationship_text + structural-facts text).
    fn seed_and_project(
        db: &mut SymbolDatabase,
        index: &SearchIndex,
        file: FileInfo,
        symbols: Vec<Symbol>,
        facts: Vec<StructuralFact>,
    ) {
        let write_set = CanonicalWriteSet {
            files: std::slice::from_ref(&file),
            symbols: &symbols,
            structural_facts: &facts,
            ..Default::default()
        };
        db.incremental_update_atomic_with_metadata(
            std::slice::from_ref(&file.path),
            &write_set,
            "structural-facts-test",
            AtomicPersistenceMetadata::default(),
        )
        .expect("incremental_update_atomic_with_metadata");
        apply_documents_with_db(index, &symbols, &[], &[], db, true)
            .expect("apply_documents_with_db");
    }

    fn search(index: &SearchIndex, query: &str) -> Vec<String> {
        let hits: Vec<UnifiedHit> = index
            .search_unified(query, &SearchFilter::default(), 20)
            .expect("search_unified");
        hits.into_iter().map(|h| h.name).collect()
    }

    /// Direct parity guard on the new projection helper: for symbols with NO
    /// structural facts, `collect_structural_facts_text_bounded` must return an
    /// empty map — which makes the merge into `relationship_text` a no-op,
    /// preserving byte-identical search behavior for the no-facts case.
    fn facts_text_map(
        db: &SymbolDatabase,
        symbol_ids: &[String],
    ) -> std::collections::HashMap<String, String> {
        collect_structural_facts_text_bounded(db, symbol_ids, 512)
            .expect("collect_structural_facts_text_bounded")
    }

    #[test]
    fn route_handler_findable_by_route_path() {
        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);
        let index = make_index(&dir);

        // Handler symbol named "indexAction" — the searched token "profile"
        // appears ONLY in the route fact's template, not in the symbol name,
        // signature, or body (code_context is None). This isolates fact
        // indexing from name/signature/body stemming matches.
        let handler = make_symbol(
            "sym-route-handler-001",
            "indexAction",
            SymbolKind::Function,
            "src/Controller.php",
            "php",
        );
        let file = make_file_info("src/Controller.php", "php", 1);
        let fact = route_fact(
            "fact-route-001",
            "src/Controller.php",
            4,
            "sym-route-handler-001",
            "GET",
            "/api/v2/profile/{id}",
        );

        seed_and_project(&mut db, &index, file, vec![handler.clone()], vec![fact]);

        let names = search(&index, "profile");
        assert!(
            names.contains(&"indexAction".to_string()),
            "searching the route segment 'profile' must surface the handler via the indexed route fact (not via name/body); got: {names:?}"
        );
    }

    #[test]
    fn sql_routine_findable_by_table_name() {
        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);
        let index = make_index(&dir);

        // Routine named "touchProc" — the searched token "accounts" appears
        // ONLY in the update fact, not in the symbol name/body.
        let routine = make_symbol(
            "sym-sql-routine-001",
            "touchProc",
            SymbolKind::Method,
            "schema/routines.sql",
            "sql",
        );
        let file = make_file_info("schema/routines.sql", "sql", 1);
        let fact = update_fact(
            "fact-sql-001",
            "schema/routines.sql",
            6,
            "sym-sql-routine-001",
            "accounts",
        );

        seed_and_project(&mut db, &index, file, vec![routine.clone()], vec![fact]);

        let names = search(&index, "accounts");
        assert!(
            names.contains(&"touchProc".to_string()),
            "searching the table name 'accounts' must surface the routine via the indexed sql fact; got: {names:?}"
        );
    }

    #[test]
    fn no_facts_preserves_name_search() {
        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);
        let index = make_index(&dir);

        // A symbol with NO structural facts must still be findable by name —
        // the structural-facts text blob is empty and changes nothing.
        let sym = make_symbol(
            "sym-nofacts-001",
            "lonelyFunction",
            SymbolKind::Function,
            "src/lib.rs",
            "rust",
        );
        let file = make_file_info("src/lib.rs", "rust", 1);

        seed_and_project(&mut db, &index, file, vec![sym.clone()], vec![]);

        let names = search(&index, "lonelyFunction");
        assert!(
            names.contains(&"lonelyFunction".to_string()),
            "symbol with no facts must still be findable by name (parity); got: {names:?}"
        );
    }

    #[test]
    fn no_facts_yields_byte_identical_empty_relationship_text() {
        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);

        // A symbol with NO structural facts must produce an EMPTY facts-text
        // map — which makes the merge into `relationship_text` a no-op. This
        // is the byte-identical search parity guarantee: symbols without web
        // facts project exactly the same `relationship_text` as before.
        let sym = make_symbol(
            "sym-nofacts-002",
            "lonelyFunction",
            SymbolKind::Function,
            "src/lib.rs",
            "rust",
        );
        let file = make_file_info("src/lib.rs", "rust", 1);

        let write_set = CanonicalWriteSet {
            files: std::slice::from_ref(&file),
            symbols: std::slice::from_ref(&sym),
            structural_facts: &[],
            ..Default::default()
        };
        db.incremental_update_atomic_with_metadata(
            std::slice::from_ref(&file.path),
            &write_set,
            "structural-facts-test",
            AtomicPersistenceMetadata::default(),
        )
        .expect("incremental_update_atomic_with_metadata");

        let map = facts_text_map(&db, std::slice::from_ref(&sym.id));
        assert!(
            map.is_empty(),
            "no structural facts must yield an empty facts-text map (byte-identical parity); got: {map:?}"
        );
    }

    #[test]
    fn facts_populate_relationship_text_with_kind_tag() {
        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);

        // A symbol WITH a route fact must produce a facts-text blob carrying
        // the kind tag, verb, and route template tokens — proving the fact
        // text is folded into `relationship_text` (which powers lexical search).
        let handler = make_symbol(
            "sym-route-handler-002",
            "indexAction",
            SymbolKind::Function,
            "src/Controller.php",
            "php",
        );
        let file = make_file_info("src/Controller.php", "php", 1);
        let fact = route_fact(
            "fact-route-002",
            "src/Controller.php",
            4,
            "sym-route-handler-002",
            "GET",
            "/api/v2/profile/{id}",
        );

        let write_set = CanonicalWriteSet {
            files: std::slice::from_ref(&file),
            symbols: std::slice::from_ref(&handler),
            structural_facts: std::slice::from_ref(&fact),
            ..Default::default()
        };
        db.incremental_update_atomic_with_metadata(
            std::slice::from_ref(&file.path),
            &write_set,
            "structural-facts-test",
            AtomicPersistenceMetadata::default(),
        )
        .expect("incremental_update_atomic_with_metadata");

        let map = facts_text_map(&db, std::slice::from_ref(&handler.id));
        let blob = map
            .get(&handler.id)
            .expect("route fact must produce a facts-text blob for its containing symbol");
        assert!(
            blob.contains("route"),
            "facts-text blob must carry the route kind tag; got: {blob:?}"
        );
        assert!(
            blob.contains("profile"),
            "facts-text blob must carry the route template token; got: {blob:?}"
        );
        assert!(
            blob.contains("GET"),
            "facts-text blob must carry the verb; got: {blob:?}"
        );
    }

    #[test]
    fn endpoint_and_table_metadata_variants_are_searchable() {
        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);
        let index = make_index(&dir);
        let file = make_file_info("src/web.ts", "typescript", 3);
        let symbols = vec![
            make_symbol(
                "sym-target-path",
                "clientRequest",
                SymbolKind::Function,
                "src/web.ts",
                "typescript",
            ),
            make_symbol(
                "sym-target-table",
                "targetMutation",
                SymbolKind::Function,
                "src/web.ts",
                "typescript",
            ),
            make_symbol(
                "sym-source-tables",
                "sourceView",
                SymbolKind::Function,
                "src/web.ts",
                "typescript",
            ),
        ];
        let facts = vec![
            metadata_fact(
                "fact-target-path",
                "http.client_request.v1",
                "src/web.ts",
                2,
                "sym-target-path",
                std::collections::HashMap::from([(
                    "target_path".to_string(),
                    serde_json::json!("/canary/zqxclientpath"),
                )]),
            ),
            metadata_fact(
                "fact-target-table",
                "sql.update_statement.v1",
                "src/web.ts",
                4,
                "sym-target-table",
                std::collections::HashMap::from([(
                    "target_table".to_string(),
                    serde_json::json!("zqxtargettable"),
                )]),
            ),
            metadata_fact(
                "fact-source-tables",
                "sql.view_definition.v1",
                "src/web.ts",
                6,
                "sym-source-tables",
                std::collections::HashMap::from([(
                    "source_tables".to_string(),
                    serde_json::json!(["zqxsourceone", "zqxsourcetwo"]),
                )]),
            ),
        ];

        seed_and_project(&mut db, &index, file, symbols, facts);

        assert_eq!(search(&index, "zqxclientpath"), vec!["clientRequest"]);
        assert_eq!(search(&index, "zqxtargettable"), vec!["targetMutation"]);
        assert_eq!(search(&index, "zqxsourcetwo"), vec!["sourceView"]);
    }

    #[test]
    fn full_and_repair_projection_preserve_structural_fact_searchability() {
        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);
        let route_symbol = make_symbol(
            "sym-route-rebuild",
            "routeEntry",
            SymbolKind::Function,
            "src/routes.php",
            "php",
        );
        let table_symbol = make_symbol(
            "sym-table-rebuild",
            "tableEntry",
            SymbolKind::Method,
            "schema/update.sql",
            "sql",
        );
        let files = vec![
            make_file_info("src/routes.php", "php", 1),
            make_file_info("schema/update.sql", "sql", 1),
        ];
        let symbols = vec![route_symbol, table_symbol];
        let facts = vec![
            route_fact(
                "fact-route-rebuild",
                "src/routes.php",
                4,
                "sym-route-rebuild",
                "GET",
                "/tenant/route-canary/{id}",
            ),
            update_fact(
                "fact-table-rebuild",
                "schema/update.sql",
                6,
                "sym-table-rebuild",
                "ledger_canary",
            ),
        ];
        let file_paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
        let write_set = CanonicalWriteSet {
            files: &files,
            symbols: &symbols,
            structural_facts: &facts,
            ..Default::default()
        };
        db.incremental_update_atomic_with_metadata(
            &file_paths,
            &write_set,
            "structural-facts-rebuild",
            AtomicPersistenceMetadata::default(),
        )
        .unwrap();

        let full_path = dir.path().join("full-index");
        std::fs::create_dir_all(&full_path).unwrap();
        let full_index = SearchIndex::create(&full_path).unwrap();
        let projection = SearchProjection::tantivy("structural-facts-rebuild");
        projection
            .ensure_current_from_database(&mut db, &full_index)
            .unwrap();
        assert_eq!(search(&full_index, "route-canary"), vec!["routeEntry"]);
        assert_eq!(search(&full_index, "ledger_canary"), vec!["tableEntry"]);

        let repair_path = dir.path().join("repair-index");
        std::fs::create_dir_all(&repair_path).unwrap();
        let repair_index = SearchIndex::create(&repair_path).unwrap();
        projection
            .ensure_current_from_database(&mut db, &repair_index)
            .unwrap();
        assert_eq!(search(&repair_index, "route-canary"), vec!["routeEntry"]);
        assert_eq!(search(&repair_index, "ledger_canary"), vec!["tableEntry"]);
    }
}
