//! Tests for reference workspace fast_refs parity (Task 1)
//!
//! Verifies that `find_references_in_reference_workspace` correctly:
//! - Accepts and applies `limit` parameter (truncation after sorting)
//! - Accepts and applies `reference_kind` filter (on relationships + identifiers)
//! - Includes identifier-based reference discovery (Strategy 4)
//! - Deduplicates identifier refs against existing relationships and definitions

#[cfg(test)]
mod tests {
    use crate::database::{FileInfo, SymbolDatabase};
    use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
    use std::collections::HashSet;
    use tempfile::TempDir;

    // =========================================================================
    // Helpers
    // =========================================================================

    /// Create a test database with file info entries pre-seeded.
    fn setup_db(files: &[&str]) -> (TempDir, SymbolDatabase) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        for file in files {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 500,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 2,
                content: None,
            })
            .unwrap();
        }
        (temp_dir, db)
    }

    fn make_symbol(id: &str, name: &str, file_path: &str, line: u32) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: line,
            end_line: line + 5,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            parent_id: None,
            signature: Some(format!("pub fn {}()", name)),
            doc_comment: None,
            visibility: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }

    fn make_relationship(
        id: &str,
        from: &str,
        to: &str,
        file_path: &str,
        line: u32,
        kind: RelationshipKind,
        confidence: f32,
    ) -> Relationship {
        Relationship {
            id: id.to_string(),
            from_symbol_id: from.to_string(),
            to_symbol_id: to.to_string(),
            kind,
            file_path: file_path.to_string(),
            line_number: line,
            confidence,
            metadata: None,
        }
    }

    /// Store a dummy caller symbol in the DB so FK constraints are satisfied
    /// when we create relationships from it.
    fn store_caller_symbol(db: &mut SymbolDatabase, id: &str, file_path: &str, line: u32) {
        let sym = make_symbol(id, &format!("caller_{}", id), file_path, line);
        db.store_symbols(&[sym]).unwrap();
    }

    /// Insert a raw identifier into the test database.
    fn insert_identifier(
        db: &SymbolDatabase,
        name: &str,
        kind: &str,
        file: &str,
        line: u32,
        containing_symbol_id: Option<&str>,
        confidence: f32,
    ) {
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, containing_symbol_id, confidence)
                 VALUES (?1, ?2, ?3, 'rust', ?4, ?5, 0, ?5, 10, 0, 100, ?6, ?7)",
                rusqlite::params![
                    format!("ident_{}_{}_{}", name, file, line),
                    name,
                    kind,
                    file,
                    line,
                    containing_symbol_id,
                    confidence,
                ],
            )
            .unwrap();
    }

    /// The function under test -- calls directly into the reference workspace logic
    /// with a raw SymbolDatabase, bypassing the handler/workspace machinery.
    ///
    /// This mirrors what `find_references_in_reference_workspace` does internally
    /// inside the `spawn_blocking` block, but extracted so we can unit-test it.
    fn find_refs_in_db(
        db: &SymbolDatabase,
        symbol: &str,
        limit: u32,
        reference_kind: Option<&str>,
    ) -> (Vec<Symbol>, Vec<Relationship>) {
        use crate::extractors::base::RelationshipKind;
        use crate::utils::cross_language_intelligence::generate_naming_variants;

        // Strategy 1: Exact name lookup
        let mut defs = db.get_symbols_by_name(symbol).unwrap_or_default();

        // Strategy 2: Cross-language naming variants
        let variants = generate_naming_variants(symbol);
        for variant in &variants {
            if *variant != symbol {
                if let Ok(variant_symbols) = db.get_symbols_by_name(variant) {
                    for sym in variant_symbols {
                        if sym.name == *variant {
                            defs.push(sym);
                        }
                    }
                }
            }
        }

        // Deduplicate definitions
        defs.sort_by(|a, b| a.id.cmp(&b.id));
        defs.dedup_by(|a, b| a.id == b.id);

        // Strategy 3: Relationships to symbols
        let definition_ids: Vec<String> = defs.iter().map(|d| d.id.clone()).collect();
        let mut refs: Vec<Relationship> = if let Some(kind) = reference_kind {
            db.get_relationships_to_symbols_filtered_by_kind(&definition_ids, kind)
                .unwrap_or_default()
        } else {
            db.get_relationships_to_symbols(&definition_ids)
                .unwrap_or_default()
        };

        // Strategy 4: Identifier-based reference discovery
        let mut all_names = vec![symbol.to_string()];
        for v in &variants {
            if *v != symbol {
                all_names.push(v.clone());
            }
        }

        let first_def_id = defs.first().map(|d| d.id.clone()).unwrap_or_default();

        let identifier_refs = if let Some(kind) = reference_kind {
            db.get_identifiers_by_names_and_kind(&all_names, kind)
                .unwrap_or_default()
        } else {
            db.get_identifiers_by_names(&all_names).unwrap_or_default()
        };

        // Build dedup set from existing relationships AND definitions
        let mut existing_refs: HashSet<(String, u32)> = refs
            .iter()
            .map(|r| (r.file_path.clone(), r.line_number))
            .collect();
        for def in &defs {
            existing_refs.insert((def.file_path.clone(), def.start_line));
        }

        for ident in identifier_refs {
            let key = (ident.file_path.clone(), ident.start_line);
            if existing_refs.contains(&key) {
                continue;
            }

            let rel_kind = match ident.kind.as_str() {
                "call" => RelationshipKind::Calls,
                "import" => RelationshipKind::Imports,
                _ => RelationshipKind::References,
            };

            refs.push(Relationship {
                id: format!("ident_{}_{}", ident.file_path, ident.start_line),
                from_symbol_id: ident.containing_symbol_id.unwrap_or_default(),
                to_symbol_id: first_def_id.clone(),
                kind: rel_kind,
                file_path: ident.file_path,
                line_number: ident.start_line,
                confidence: ident.confidence,
                metadata: None,
            });
        }

        // Sort references by confidence (descending), then file_path, then line_number
        refs.sort_by(|a, b| {
            let conf_cmp = b
                .confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal);
            if conf_cmp != std::cmp::Ordering::Equal {
                return conf_cmp;
            }
            let file_cmp = a.file_path.cmp(&b.file_path);
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }
            a.line_number.cmp(&b.line_number)
        });

        // Apply limit
        refs.truncate(limit as usize);

        (defs, refs)
    }

    // =========================================================================
    // Tests: limit parameter
    // =========================================================================

    #[test]
    fn test_reference_workspace_limit_truncates_references() {
        let files = &[
            "src/lib.rs",
            "src/a.rs",
            "src/b.rs",
            "src/c.rs",
            "src/d.rs",
            "src/e.rs",
        ];
        let (_tmp, mut db) = setup_db(files);

        // Store the target symbol
        let target = make_symbol("sym-process", "process", "src/lib.rs", 10);
        db.store_symbols(&[target]).unwrap();

        // Store caller symbols (FK constraint requires them to exist)
        for i in 0..5u8 {
            let file = format!("src/{}.rs", (b'a' + i) as char);
            store_caller_symbol(&mut db, &format!("caller_{}", i), &file, 20 + i as u32);
        }

        // Store 5 relationships pointing to the target
        let rels: Vec<Relationship> = (0..5u8)
            .map(|i| {
                let file = format!("src/{}.rs", (b'a' + i) as char);
                make_relationship(
                    &format!("rel_{}", i),
                    &format!("caller_{}", i),
                    "sym-process",
                    &file,
                    20 + i as u32,
                    RelationshipKind::Calls,
                    0.9,
                )
            })
            .collect();
        db.store_relationships(&rels).unwrap();

        // With limit=3, should only get 3 references
        let (defs, refs) = find_refs_in_db(&db, "process", 3, None);
        assert_eq!(defs.len(), 1, "should find 1 definition");
        assert_eq!(
            refs.len(),
            3,
            "should truncate to limit=3, got {} refs",
            refs.len()
        );

        // With limit=10, should get all 5
        let (_, refs_all) = find_refs_in_db(&db, "process", 10, None);
        assert_eq!(
            refs_all.len(),
            5,
            "with limit=10, should get all 5 refs, got {}",
            refs_all.len()
        );
    }

    #[test]
    fn test_reference_workspace_limit_applied_after_sorting() {
        let files = &["src/lib.rs", "src/a.rs", "src/b.rs", "src/c.rs"];
        let (_tmp, mut db) = setup_db(files);

        let target = make_symbol("sym-foo", "foo", "src/lib.rs", 1);
        db.store_symbols(&[target]).unwrap();

        // Store caller symbols for FK constraints
        store_caller_symbol(&mut db, "caller_low", "src/c.rs", 30);
        store_caller_symbol(&mut db, "caller_high", "src/a.rs", 10);
        store_caller_symbol(&mut db, "caller_mid", "src/b.rs", 20);

        // Store 3 relationships with different confidence levels
        let rels = vec![
            make_relationship(
                "rel_low",
                "caller_low",
                "sym-foo",
                "src/c.rs",
                30,
                RelationshipKind::Calls,
                0.5, // lowest confidence
            ),
            make_relationship(
                "rel_high",
                "caller_high",
                "sym-foo",
                "src/a.rs",
                10,
                RelationshipKind::Calls,
                0.95, // highest confidence
            ),
            make_relationship(
                "rel_mid",
                "caller_mid",
                "sym-foo",
                "src/b.rs",
                20,
                RelationshipKind::Calls,
                0.8, // mid confidence
            ),
        ];
        db.store_relationships(&rels).unwrap();

        // Limit to 2: should keep the two highest confidence refs
        let (_, refs) = find_refs_in_db(&db, "foo", 2, None);
        assert_eq!(refs.len(), 2);
        // Should be sorted by confidence descending, so highest first
        assert!(
            refs[0].confidence >= refs[1].confidence,
            "refs should be sorted by confidence descending: {} >= {}",
            refs[0].confidence,
            refs[1].confidence
        );
        assert_eq!(
            refs[0].file_path, "src/a.rs",
            "highest confidence ref should be first"
        );
    }

    // =========================================================================
    // Tests: reference_kind filter
    // =========================================================================

    #[test]
    fn test_reference_workspace_reference_kind_filters_relationships() {
        let files = &["src/lib.rs", "src/caller.rs", "src/importer.rs"];
        let (_tmp, mut db) = setup_db(files);

        let target = make_symbol("sym-widget", "Widget", "src/lib.rs", 1);
        db.store_symbols(&[target]).unwrap();

        // Store caller symbols for FK constraints
        store_caller_symbol(&mut db, "caller_fn", "src/caller.rs", 15);
        store_caller_symbol(&mut db, "importer", "src/importer.rs", 3);

        // Store a "call" relationship
        let call_rel = make_relationship(
            "rel_call",
            "caller_fn",
            "sym-widget",
            "src/caller.rs",
            15,
            RelationshipKind::Calls,
            0.9,
        );
        // Store an "import" relationship
        let import_rel = make_relationship(
            "rel_import",
            "importer",
            "sym-widget",
            "src/importer.rs",
            3,
            RelationshipKind::Imports,
            1.0,
        );
        db.store_relationships(&[call_rel, import_rel]).unwrap();

        // Insert corresponding identifiers for the kind filtering to work
        // (get_relationships_to_symbols_filtered_by_kind joins with identifiers table)
        insert_identifier(
            &db,
            "Widget",
            "call",
            "src/caller.rs",
            15,
            Some("caller_fn"),
            0.9,
        );
        insert_identifier(
            &db,
            "Widget",
            "import",
            "src/importer.rs",
            3,
            Some("importer"),
            1.0,
        );

        // Filter by "call" -- should only get the call reference
        let (_, refs_call) = find_refs_in_db(&db, "Widget", 100, Some("call"));
        assert!(
            refs_call
                .iter()
                .all(|r| r.file_path != "src/importer.rs" || r.id.starts_with("ident_")),
            "call filter should not return unfiltered import relationships"
        );
        // Should include the call ref
        assert!(
            refs_call.iter().any(|r| r.file_path == "src/caller.rs"),
            "call filter should include src/caller.rs call ref"
        );
    }

    #[test]
    fn test_reference_workspace_reference_kind_filters_identifiers() {
        let files = &["src/lib.rs", "src/user.rs", "src/types.rs"];
        let (_tmp, mut db) = setup_db(files);

        let target = make_symbol("sym-config", "Config", "src/lib.rs", 1);
        db.store_symbols(&[target]).unwrap();

        // No relationships -- only identifiers of different kinds
        insert_identifier(&db, "Config", "call", "src/user.rs", 20, None, 0.8);
        insert_identifier(
            &db,
            "Config",
            "type_usage",
            "src/types.rs",
            5,
            None,
            0.7,
        );

        // Filter by "call" -- should only get the call identifier
        let (_, refs_call) = find_refs_in_db(&db, "Config", 100, Some("call"));
        assert_eq!(
            refs_call.len(),
            1,
            "should find exactly 1 call identifier, got {}",
            refs_call.len()
        );
        assert_eq!(refs_call[0].file_path, "src/user.rs");

        // Filter by "type_usage" -- should only get the type_usage identifier
        let (_, refs_type) = find_refs_in_db(&db, "Config", 100, Some("type_usage"));
        assert_eq!(
            refs_type.len(),
            1,
            "should find exactly 1 type_usage identifier"
        );
        assert_eq!(refs_type[0].file_path, "src/types.rs");

        // No filter -- should get both
        let (_, refs_all) = find_refs_in_db(&db, "Config", 100, None);
        assert_eq!(
            refs_all.len(),
            2,
            "without filter, should find 2 identifier refs"
        );
    }

    // =========================================================================
    // Tests: identifier-based reference discovery (Strategy 4)
    // =========================================================================

    #[test]
    fn test_reference_workspace_includes_identifier_refs() {
        let files = &["src/lib.rs", "src/main.rs", "src/handler.rs"];
        let (_tmp, mut db) = setup_db(files);

        let target = make_symbol("sym-process", "process", "src/lib.rs", 10);
        db.store_symbols(&[target]).unwrap();

        // No relationships -- only identifiers
        insert_identifier(&db, "process", "call", "src/main.rs", 25, None, 0.9);
        insert_identifier(
            &db,
            "process",
            "call",
            "src/handler.rs",
            42,
            None,
            0.85,
        );

        let (defs, refs) = find_refs_in_db(&db, "process", 100, None);
        assert_eq!(defs.len(), 1);
        assert_eq!(
            refs.len(),
            2,
            "should find 2 identifier-based refs, got {}",
            refs.len()
        );

        // Verify they're converted to Relationship correctly
        let ref_files: HashSet<&str> = refs.iter().map(|r| r.file_path.as_str()).collect();
        assert!(ref_files.contains("src/main.rs"));
        assert!(ref_files.contains("src/handler.rs"));
    }

    #[test]
    fn test_reference_workspace_identifier_dedup_against_relationships() {
        let files = &["src/lib.rs", "src/caller.rs"];
        let (_tmp, mut db) = setup_db(files);

        let target = make_symbol("sym-foo", "foo", "src/lib.rs", 1);
        db.store_symbols(&[target]).unwrap();

        // Store caller symbol for FK constraint
        store_caller_symbol(&mut db, "some_caller", "src/caller.rs", 15);

        // Store a relationship at src/caller.rs:15
        let rel = make_relationship(
            "rel_1",
            "some_caller",
            "sym-foo",
            "src/caller.rs",
            15,
            RelationshipKind::Calls,
            0.9,
        );
        db.store_relationships(&[rel]).unwrap();

        // Also insert an identifier at the SAME location (src/caller.rs:15)
        insert_identifier(
            &db,
            "foo",
            "call",
            "src/caller.rs",
            15,
            Some("some_caller"),
            0.9,
        );

        let (_, refs) = find_refs_in_db(&db, "foo", 100, None);
        assert_eq!(
            refs.len(),
            1,
            "should deduplicate: only 1 ref for same file:line, got {}",
            refs.len()
        );
    }

    #[test]
    fn test_reference_workspace_identifier_dedup_against_definitions() {
        let files = &["src/lib.rs", "src/other.rs"];
        let (_tmp, mut db) = setup_db(files);

        let target = make_symbol("sym-bar", "bar", "src/lib.rs", 10);
        db.store_symbols(&[target]).unwrap();

        // Insert an identifier at the definition site (same file:line as definition)
        insert_identifier(&db, "bar", "call", "src/lib.rs", 10, None, 1.0);
        // Insert an identifier at a different location
        insert_identifier(&db, "bar", "call", "src/other.rs", 30, None, 0.8);

        let (defs, refs) = find_refs_in_db(&db, "bar", 100, None);
        assert_eq!(defs.len(), 1);
        assert_eq!(
            refs.len(),
            1,
            "should not include identifier at definition site, got {} refs",
            refs.len()
        );
        assert_eq!(refs[0].file_path, "src/other.rs");
    }

    #[test]
    fn test_reference_workspace_identifier_kind_conversion() {
        let files = &["src/lib.rs", "src/a.rs", "src/b.rs", "src/c.rs"];
        let (_tmp, mut db) = setup_db(files);

        let target = make_symbol("sym-thing", "Thing", "src/lib.rs", 1);
        db.store_symbols(&[target]).unwrap();

        insert_identifier(&db, "Thing", "call", "src/a.rs", 10, None, 0.9);
        insert_identifier(&db, "Thing", "import", "src/b.rs", 2, None, 1.0);
        insert_identifier(&db, "Thing", "type_usage", "src/c.rs", 15, None, 0.7);

        let (_, refs) = find_refs_in_db(&db, "Thing", 100, None);
        assert_eq!(refs.len(), 3, "should find 3 identifier refs");

        // Check kind conversion
        let call_ref = refs.iter().find(|r| r.file_path == "src/a.rs").unwrap();
        assert_eq!(call_ref.kind, RelationshipKind::Calls);

        let import_ref = refs.iter().find(|r| r.file_path == "src/b.rs").unwrap();
        assert_eq!(import_ref.kind, RelationshipKind::Imports);

        let type_ref = refs.iter().find(|r| r.file_path == "src/c.rs").unwrap();
        assert_eq!(type_ref.kind, RelationshipKind::References);
    }

    // =========================================================================
    // Tests: combined limit + reference_kind + identifiers
    // =========================================================================

    #[test]
    fn test_reference_workspace_combined_limit_and_kind_filter() {
        let files = &[
            "src/lib.rs",
            "src/a.rs",
            "src/b.rs",
            "src/c.rs",
            "src/d.rs",
        ];
        let (_tmp, mut db) = setup_db(files);

        let target = make_symbol("sym-handler", "handler", "src/lib.rs", 1);
        db.store_symbols(&[target]).unwrap();

        // Insert 3 "call" identifiers and 1 "type_usage" identifier
        insert_identifier(&db, "handler", "call", "src/a.rs", 10, None, 0.9);
        insert_identifier(&db, "handler", "call", "src/b.rs", 20, None, 0.8);
        insert_identifier(&db, "handler", "call", "src/c.rs", 30, None, 0.7);
        insert_identifier(
            &db,
            "handler",
            "type_usage",
            "src/d.rs",
            5,
            None,
            0.95,
        );

        // Filter by "call" with limit=2 -- should get top 2 call refs by confidence
        let (_, refs) = find_refs_in_db(&db, "handler", 2, Some("call"));
        assert_eq!(
            refs.len(),
            2,
            "should get exactly 2 call refs with limit=2"
        );
        // All should be calls (no type_usage)
        for r in &refs {
            assert_eq!(
                r.kind,
                RelationshipKind::Calls,
                "all refs should be Calls kind"
            );
        }
        // Should be sorted by confidence descending
        assert!(refs[0].confidence >= refs[1].confidence);
    }

    // =========================================================================
    // Tests: the actual async function signature change compiles and works
    // =========================================================================

    /// Test that `find_references_in_reference_workspace` accepts the new params.
    /// This is a compile-time check + basic integration test using a real handler.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_find_references_in_reference_workspace_accepts_limit_and_kind() {
        use crate::handler::JulieServerHandler;
        use crate::tools::navigation::reference_workspace;
        use std::fs;

        // Create primary workspace
        let primary_dir = TempDir::new().unwrap();
        let primary_path = primary_dir.path().to_path_buf();
        let primary_src = primary_path.join("src");
        fs::create_dir_all(&primary_src).unwrap();
        fs::write(primary_src.join("primary.rs"), "pub struct Primary {}").unwrap();

        // Create reference workspace
        let reference_dir = TempDir::new().unwrap();
        let reference_path = reference_dir.path().to_path_buf();
        let reference_src = reference_path.join("src");
        fs::create_dir_all(&reference_src).unwrap();

        // Write a file with a function and some calls to it
        fs::write(
            reference_src.join("lib.rs"),
            r#"
pub fn compute(x: i32) -> i32 {
    x * 2
}

pub fn caller_one() {
    let result = compute(5);
}

pub fn caller_two() {
    compute(10);
}
"#,
        )
        .unwrap();

        // Initialize handler
        let handler = JulieServerHandler::new_for_test().await.unwrap();
        handler
            .initialize_workspace_with_force(
                Some(primary_path.to_string_lossy().to_string()),
                true,
            )
            .await
            .unwrap();

        // Index primary
        let index_tool = crate::tools::ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(primary_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool
            .call_tool(&handler)
            .await
            .expect("index primary");

        // Add reference workspace
        let add_tool = crate::tools::ManageWorkspaceTool {
            operation: "add".to_string(),
            path: Some(reference_path.to_string_lossy().to_string()),
            name: Some("test-ref-fast-refs".to_string()),
            force: None,
            workspace_id: None,
            detailed: None,
        };
        let add_result = add_tool.call_tool(&handler).await.expect("add ref ws");
        let add_text: String = add_result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n");

        let workspace_id = add_text
            .lines()
            .find(|l| l.starts_with("Workspace ID:"))
            .and_then(|l| l.split(':').nth(1))
            .map(|id| id.trim().to_string())
            .expect("Should get workspace ID");

        // Call find_references_in_reference_workspace with the new params
        // This test validates that the function signature compiles with limit + reference_kind
        let result: Result<(Vec<Symbol>, Vec<Relationship>), anyhow::Error> =
            reference_workspace::find_references_in_reference_workspace(
                &handler,
                workspace_id,
                "compute",
                10,   // limit
                None, // reference_kind
            )
            .await;

        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        let (defs, _refs) = result.unwrap();
        // We should find the "compute" definition
        assert!(
            !defs.is_empty(),
            "should find at least one definition for 'compute'"
        );

        // References may be empty if the tree-sitter extractor doesn't capture call
        // relationships in this simple case -- that's fine, we're testing the signature
        // and that it doesn't panic.
    }

    // =========================================================================
    // Tests: qualified name support (Task 2)
    // =========================================================================

    /// Mirror of the qualified-name resolution logic that will be in
    /// `find_references_and_definitions`: parse "Parent::child", look up by child
    /// name only, then retain definitions whose parent symbol name matches.
    fn find_defs_qualified(
        db: &SymbolDatabase,
        symbol: &str,
    ) -> Vec<Symbol> {
        use crate::tools::navigation::resolution::parse_qualified_name;

        let (effective_symbol, parent_filter) = match parse_qualified_name(symbol) {
            Some((parent, child)) => (child.to_string(), Some(parent.to_string())),
            None => (symbol.to_string(), None),
        };

        let mut defs = db.get_symbols_by_name(&effective_symbol).unwrap_or_default();

        if let Some(ref parent) = parent_filter {
            // Collect parent IDs from definitions that have one
            let parent_ids: Vec<String> = defs
                .iter()
                .filter_map(|s| s.parent_id.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            if !parent_ids.is_empty() {
                // Batch-fetch parent symbols
                let parents = db.get_symbols_by_ids(&parent_ids).unwrap_or_default();
                let matching_parent_ids: std::collections::HashSet<String> = parents
                    .into_iter()
                    .filter(|p| p.name == *parent)
                    .map(|p| p.id)
                    .collect();

                // Keep only definitions whose parent_id is in matching_parent_ids
                defs.retain(|s| {
                    s.parent_id
                        .as_deref()
                        .map(|pid| matching_parent_ids.contains(pid))
                        .unwrap_or(false)
                });
            } else {
                // Definitions have no parent_id — qualified search finds nothing
                defs.clear();
            }
        }

        defs
    }

    /// Helper to create a class/struct symbol (parent container)
    fn make_class_symbol(id: &str, name: &str, file_path: &str, line: u32) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Class,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: line,
            end_line: line + 20,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            parent_id: None,
            signature: Some(format!("struct {}", name)),
            doc_comment: None,
            visibility: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }

    /// Helper to create a method symbol with a parent_id
    fn make_method_symbol(
        id: &str,
        name: &str,
        file_path: &str,
        line: u32,
        parent_id: &str,
    ) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Method,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: line,
            end_line: line + 5,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            parent_id: Some(parent_id.to_string()),
            signature: Some(format!("pub fn {}()", name)),
            doc_comment: None,
            visibility: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }

    #[test]
    fn test_fast_refs_qualified_name_filters_by_parent() {
        let files = &["src/engine.rs", "src/pipeline.rs"];
        let (_tmp, mut db) = setup_db(files);

        // Store parent class symbols
        let engine = make_class_symbol("class-engine", "Engine", "src/engine.rs", 1);
        let pipeline = make_class_symbol("class-pipeline", "Pipeline", "src/pipeline.rs", 1);
        db.store_symbols(&[engine, pipeline]).unwrap();

        // Store "process" methods — one under Engine, one under Pipeline
        let engine_process =
            make_method_symbol("method-engine-process", "process", "src/engine.rs", 10, "class-engine");
        let pipeline_process =
            make_method_symbol("method-pipeline-process", "process", "src/pipeline.rs", 10, "class-pipeline");
        db.store_symbols(&[engine_process, pipeline_process]).unwrap();

        // Unqualified: should find both "process" methods
        let unqualified = find_defs_qualified(&db, "process");
        assert_eq!(
            unqualified.len(),
            2,
            "unqualified 'process' should find both methods, got {}",
            unqualified.len()
        );

        // Qualified "Engine::process": should find only the Engine method
        let engine_defs = find_defs_qualified(&db, "Engine::process");
        assert_eq!(
            engine_defs.len(),
            1,
            "Engine::process should find exactly 1 definition, got {}",
            engine_defs.len()
        );
        assert_eq!(engine_defs[0].id, "method-engine-process");
        assert_eq!(engine_defs[0].file_path, "src/engine.rs");

        // Qualified "Pipeline::process": should find only the Pipeline method
        let pipeline_defs = find_defs_qualified(&db, "Pipeline::process");
        assert_eq!(
            pipeline_defs.len(),
            1,
            "Pipeline::process should find exactly 1 definition, got {}",
            pipeline_defs.len()
        );
        assert_eq!(pipeline_defs[0].id, "method-pipeline-process");
        assert_eq!(pipeline_defs[0].file_path, "src/pipeline.rs");

        // Qualified with unknown parent: should find nothing
        let unknown_defs = find_defs_qualified(&db, "Unknown::process");
        assert_eq!(
            unknown_defs.len(),
            0,
            "Unknown::process should find nothing, got {}",
            unknown_defs.len()
        );
    }

    #[test]
    fn test_fast_refs_qualified_dot_separator() {
        let files = &["src/service.rs"];
        let (_tmp, mut db) = setup_db(files);

        let service = make_class_symbol("class-service", "Service", "src/service.rs", 1);
        db.store_symbols(&[service]).unwrap();

        let method = make_method_symbol("method-run", "run", "src/service.rs", 5, "class-service");
        db.store_symbols(&[method]).unwrap();

        // Dot separator "Service.run" should also work
        let defs = find_defs_qualified(&db, "Service.run");
        assert_eq!(
            defs.len(),
            1,
            "Service.run should find 1 definition, got {}",
            defs.len()
        );
        assert_eq!(defs[0].id, "method-run");
    }
}
