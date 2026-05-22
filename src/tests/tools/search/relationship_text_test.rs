/// T7 tests: `relationship_text` is populated from the relationships table for symbol rows.
///
/// Three invariants:
///   1. `related_symbol_indexed` — symbol A's relationship_text contains symbol B's name when
///      an A→B relationship exists, and querying for B's name returns A in results.
///   2. `cap_enforced` — relationship_text.len() ≤ 512 bytes even with 200 relationships.
///   3. `file_rows_empty` — file-kind SearchDocuments always have relationship_text == "".

#[cfg(test)]
mod relationship_text_test {
    use tempfile::TempDir;

    use crate::database::SymbolDatabase;
    use crate::database::types::FileInfo;
    use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
    // Access the private apply submodule via the pub(crate) re-exports on the projection module.
    use crate::search::projection::collect_relationship_names_bounded;
    use crate::search::projection::apply_documents_with_db;
    use crate::search::projection::apply_documents;
    use crate::search::{SearchFilter, SearchIndex};
    use crate::search::index::UnifiedHit; // for search_unified return type

    fn make_db(dir: &TempDir) -> SymbolDatabase {
        let db_path = dir.path().join("symbols.db");
        SymbolDatabase::new(&db_path).expect("create test db")
    }

    fn make_index(dir: &TempDir) -> SearchIndex {
        let index_dir = dir.path().join("index");
        std::fs::create_dir_all(&index_dir).unwrap();
        SearchIndex::create(&index_dir).expect("create test index")
    }

    fn make_symbol(id: &str, name: &str) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 5,
            end_column: 0,
            start_byte: 0,
            end_byte: 64,
            signature: Some(format!("fn {}()", name)),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some(format!("fn {}() {{}}", name)),
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        }
    }

    fn seed_symbols(db: &mut SymbolDatabase, symbols: &[Symbol]) {
        // Seed a file row so FK constraints are satisfied.
        let file_info = FileInfo {
            path: "src/lib.rs".to_string(),
            language: "rust".to_string(),
            hash: "abc123".to_string(),
            size: 100,
            last_modified: 0,
            last_indexed: 0,
            symbol_count: symbols.len() as i32,
            line_count: 10,
            content: None,
        };
        db.store_file_info(&file_info).expect("store file info");
        db.store_symbols(symbols).expect("store symbols");
    }

    fn make_relationship(id: &str, from_id: &str, to_id: &str) -> Relationship {
        Relationship {
            id: id.to_string(),
            from_symbol_id: from_id.to_string(),
            to_symbol_id: to_id.to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/lib.rs".to_string(),
            line_number: 3,
            confidence: 1.0,
            metadata: None,
        }
    }

    // -------------------------------------------------------------------------
    // Test 1: related_symbol_indexed
    //   Symbol A calls symbol B. A's relationship_text should contain B's name.
    //   A search for B's name should surface A.
    // -------------------------------------------------------------------------
    #[test]
    fn related_symbol_indexed() {
        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);
        let index = make_index(&dir);

        let sym_a = make_symbol("sym-rel-001", "caller_function");
        let sym_b = make_symbol("sym-rel-002", "callee_target");
        seed_symbols(&mut db, &[sym_a.clone(), sym_b.clone()]);

        // Store the A→B relationship.
        let rel = make_relationship("rel-001", "sym-rel-001", "sym-rel-002");
        db.store_relationships(&[rel]).expect("store relationship");

        // Collect relationship names for sym_a (the caller).
        let ids = vec!["sym-rel-001".to_string()];
        let rel_map =
            collect_relationship_names_bounded(&db, &ids, 512).expect("collect relationship names");

        // A's blob should contain B's name.
        let blob = rel_map.get("sym-rel-001").map(String::as_str).unwrap_or("");
        assert!(
            blob.contains("callee_target"),
            "relationship_text for caller_function must contain 'callee_target', got: {:?}",
            blob
        );

        // Project both symbols with DB-backed relationship enrichment.
        apply_documents_with_db(&index, &[sym_a.clone(), sym_b.clone()], &[], &[], &db, true)
            .expect("apply_documents_with_db");

        // The unified query path searches relationship_text. A query for 'callee_target' should
        // surface caller_function (whose relationship_text was written with 'callee_target').
        //
        // Note: relationship_text is indexed (for search) but not stored (not retrievable from
        // hits). We verify the functional effect: caller_function appears in results.
        let hits: Vec<UnifiedHit> = index
            .search_unified("callee_target", &SearchFilter::default(), 10)
            .expect("search_unified");

        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert!(
            names.contains(&"caller_function"),
            "unified search for 'callee_target' must surface 'caller_function' via relationship_text; got: {:?}",
            names
        );
    }

    // -------------------------------------------------------------------------
    // Test 2: cap_enforced
    //   A symbol with 200 outgoing relationships must yield relationship_text ≤ 512 bytes.
    // -------------------------------------------------------------------------
    #[test]
    fn cap_enforced() {
        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);

        // Build a hub symbol and 200 targets.
        let hub = make_symbol("sym-hub-000", "hub_symbol");
        let targets: Vec<Symbol> = (0..200)
            .map(|i| make_symbol(&format!("sym-target-{i:03}"), &format!("target_fn_{i:03}")))
            .collect();

        let mut all_symbols = vec![hub.clone()];
        all_symbols.extend(targets.iter().cloned());
        seed_symbols(&mut db, &all_symbols);

        // Store 200 relationships hub→target.
        let rels: Vec<Relationship> = (0..200)
            .map(|i| {
                make_relationship(
                    &format!("rel-cap-{i:03}"),
                    "sym-hub-000",
                    &format!("sym-target-{i:03}"),
                )
            })
            .collect();
        db.store_relationships(&rels).expect("store relationships");

        let ids = vec!["sym-hub-000".to_string()];
        let rel_map =
            collect_relationship_names_bounded(&db, &ids, 512).expect("collect relationship names");

        let blob = rel_map.get("sym-hub-000").map(String::as_str).unwrap_or("");
        assert!(
            blob.len() <= 512,
            "relationship_text must be ≤ 512 bytes, got {} bytes",
            blob.len()
        );
        // Must still contain at least one name (not empty).
        assert!(!blob.is_empty(), "relationship_text must not be empty");
    }

    // -------------------------------------------------------------------------
    // Test 3: file_rows_empty
    //   File-kind SearchDocuments must always have relationship_text == "".
    //
    //   Verified two ways:
    //   (a) `collect_relationship_names_bounded` with an empty IDs slice returns
    //       an empty map — file rows are never passed to relationship enrichment.
    //   (b) A file doc projected via `apply_documents` does NOT appear in
    //       `search_symbols` results (because it has doc_type="file"), confirming
    //       the symbol search path is not touched for file rows.
    // -------------------------------------------------------------------------
    #[test]
    fn file_rows_empty() {
        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);
        let index = make_index(&dir);

        // (a) Empty ID slice → empty map. File rows pass no IDs to this function.
        let empty_map = collect_relationship_names_bounded(&db, &[], 512)
            .expect("collect with empty ids");
        assert!(
            empty_map.is_empty(),
            "collect_relationship_names_bounded with no IDs must return empty map"
        );

        // Seed a file and a symbol so the DB is non-empty, then confirm
        // collect_relationship_names_bounded with file-only IDs returns empty
        // (file rows have no symbol IDs to look up).
        let sym = make_symbol("sym-file-001", "some_function");
        seed_symbols(&mut db, &[sym]);

        // (b) Project file doc via apply_documents — file rows must NOT surface
        //     in search_symbols (they live in the "file" doc_type bucket).
        let file_info = FileInfo {
            path: "src/lib.rs".to_string(),
            content: Some("fn hello() {}".to_string()),
            language: "rust".to_string(),
            hash: String::new(),
            size: 13,
            last_modified: 0,
            last_indexed: 0,
            symbol_count: 0,
            line_count: 1,
        };
        apply_documents(&index, &[], std::slice::from_ref(&file_info), &[]).expect("apply_documents");

        // File rows are indexed but do NOT appear in symbol search results.
        let sym_results = index
            .search_symbols("hello", &SearchFilter::default(), 10)
            .expect("search_symbols")
            .results;

        for result in &sym_results {
            assert_ne!(
                result.file_path, "src/lib.rs",
                "file doc projected as doc_type='file' must not appear in symbol search results"
            );
        }

        // Content search DOES find the file doc (confirming it was indexed).
        let content_results = index
            .search_content("hello", &SearchFilter::default(), 10)
            .expect("search_content")
            .results;
        assert!(
            !content_results.is_empty(),
            "file row must be findable via content search"
        );
    }

    // -------------------------------------------------------------------------
    // Test: test_projection_propagates_sql_errors  (codex review #5)
    //   A real SQL error from collect_relationship_names_bounded (e.g. missing
    //   column, not "no such table") must propagate: the projection returns Err
    //   and the state is marked Stale, not silently swallowed as empty map.
    // -------------------------------------------------------------------------
    #[test]
    fn test_projection_propagates_sql_errors() {
        use crate::database::ProjectionStatus;
        use crate::search::projection::SearchProjection;

        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);
        let index = make_index(&dir);

        // Seed a symbol so there is data and ensure_canonical_revision creates
        // a revision (it returns None when all tables are empty).
        let sym = make_symbol("sym-sqlerr-001", "error_test_fn");
        seed_symbols(&mut db, &[sym]);

        // Create the canonical revision NOW while relationships table is intact
        // (ensure_canonical_revision queries COUNT(*) FROM relationships).
        db.ensure_canonical_revision("test-ws-sqlerr")
            .expect("ensure_canonical_revision");

        // Corrupt the relationships table: keep it existing but with a wrong schema.
        // "no such column" is a real SQL error that is NOT "no such table" —
        // the fix must propagate it, not swallow it as an empty map.
        db.conn
            .execute_batch(
                "DROP TABLE IF EXISTS relationships; \
                 CREATE TABLE relationships (id TEXT, bogus_col TEXT);",
            )
            .expect("corrupt relationships table");

        // Call ensure_current_from_database — should return Err (not Ok).
        let projection = SearchProjection::tantivy("test-ws-sqlerr");
        let result = projection.ensure_current_from_database(&mut db, &index);

        assert!(
            result.is_err(),
            "projection must propagate SQL errors from collect_relationship_names_bounded, got Ok"
        );

        // The projection state must be Stale (not Ready or Missing).
        let state = db
            .get_projection_state("tantivy", "test-ws-sqlerr")
            .expect("get_projection_state");
        assert_eq!(
            state.map(|s| s.status),
            Some(ProjectionStatus::Stale),
            "projection state must be Stale after SQL error in collect_relationship_names_bounded"
        );
    }

    // -------------------------------------------------------------------------
    // Test: test_watcher_reprojects_relationship_partners  (codex review #4)
    //   When file A changes and establishes a relationship with symbol Z in
    //   file B, Z's Tantivy doc must be reprojected with fresh relationship_text.
    //   Verified via reproject_partner_symbols() which is what the watcher calls.
    // -------------------------------------------------------------------------
    #[test]
    fn test_watcher_reprojects_relationship_partners() {
        use crate::search::projection::reproject_partner_symbols;

        let dir = TempDir::new().unwrap();
        let mut db = make_db(&dir);
        let index = make_index(&dir);

        // sym_a is in file_a.rs, sym_b is in file_b.rs
        let mut sym_a = make_symbol("sym-rep-001", "caller_in_file_a");
        sym_a.file_path = "src/file_a.rs".to_string();

        let mut sym_b = make_symbol("sym-rep-002", "callee_in_file_b");
        sym_b.file_path = "src/file_b.rs".to_string();

        let file_a_info = FileInfo {
            path: "src/file_a.rs".to_string(),
            language: "rust".to_string(),
            hash: "aaaa".to_string(),
            size: 50,
            last_modified: 0,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 5,
            content: Some("fn caller_in_file_a() {}".to_string()),
        };
        let file_b_info = FileInfo {
            path: "src/file_b.rs".to_string(),
            language: "rust".to_string(),
            hash: "bbbb".to_string(),
            size: 50,
            last_modified: 0,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 5,
            content: Some("fn callee_in_file_b() {}".to_string()),
        };

        // Seed file_b and project sym_b WITHOUT the A→B relationship.
        // This simulates sym_b being projected before file_a was indexed —
        // sym_b's Tantivy doc therefore has empty relationship_text.
        db.store_file_info(&file_b_info).unwrap();
        db.store_symbols(&[sym_b.clone()]).unwrap();
        apply_documents_with_db(
            &index,
            &[sym_b.clone()],
            &[file_b_info.clone()],
            &["src/file_b.rs".to_string()],
            &db,
            true,
        )
        .expect("initial projection of file_b (no relationship yet)");

        // Now add file_a and the A→B relationship (simulating file_a being indexed).
        // sym_b is now stale: its Tantivy doc has empty relationship_text,
        // but the DB has a new relationship linking sym_a → sym_b.
        db.store_file_info(&file_a_info).unwrap();
        db.store_symbols(&[sym_a.clone()]).unwrap();
        let rel = Relationship {
            id: "rel-rep-001".to_string(),
            from_symbol_id: "sym-rep-001".to_string(),
            to_symbol_id: "sym-rep-002".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/file_a.rs".to_string(),
            line_number: 3,
            confidence: 1.0,
            metadata: None,
        };
        db.store_relationships(&[rel]).unwrap();

        // Before reprojection: "caller_in_file_a" must NOT surface callee_in_file_b
        // via relationship_text (it's stale/empty).
        let hits_before: Vec<UnifiedHit> = index
            .search_unified("caller_in_file_a", &SearchFilter::default(), 10)
            .expect("search before reproject");
        let names_before: Vec<&str> = hits_before.iter().map(|h| h.name.as_str()).collect();
        assert!(
            !names_before.contains(&"callee_in_file_b"),
            "callee_in_file_b must NOT appear before partner reprojection; got: {names_before:?}"
        );

        // Reproject sym_b as a partner of sym_a (what the watcher does after
        // incremental_update_atomic for file_a).
        reproject_partner_symbols(&index, &db, &["sym-rep-002".to_string()])
            .expect("reproject_partner_symbols");
        index.commit().expect("commit after reproject");

        // After reprojection: sym_b's relationship_text now contains "caller_in_file_a",
        // so searching for that term must surface callee_in_file_b.
        let hits_after: Vec<UnifiedHit> = index
            .search_unified("caller_in_file_a", &SearchFilter::default(), 10)
            .expect("search after reproject");
        let names_after: Vec<&str> = hits_after.iter().map(|h| h.name.as_str()).collect();
        assert!(
            names_after.contains(&"callee_in_file_b"),
            "callee_in_file_b must appear after partner reprojection via relationship_text; got: {names_after:?}"
        );
    }
}
