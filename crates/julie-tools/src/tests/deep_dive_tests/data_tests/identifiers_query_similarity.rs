use super::*;

// === Identifier fallback ===

/// Helper to insert an identifier into the test database
fn insert_identifier(
    db: &mut SymbolDatabase,
    name: &str,
    kind: IdentifierKind,
    file: &str,
    line: u32,
    containing_symbol_id: Option<&str>,
) {
    let mut builder = identifier_builder(format!("ident_{name}_{line}"), name, file)
        .kind(kind)
        .line(line)
        .column(0, 10)
        .bytes(0, 100)
        .confidence(0.9);
    if let Some(containing_symbol_id) = containing_symbol_id {
        builder = builder.containing_symbol_id(containing_symbol_id);
    }
    db.bulk_store_identifiers(&[builder.build()], "").unwrap();
}

#[test]
fn test_identifier_fallback_adds_refs() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![make_symbol(
        "sym-target",
        "process",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        None,
        None,
        None,
    )];
    db.store_symbols(&symbols).unwrap();

    // No relationships — only an identifier ref (no containing symbol)
    insert_identifier(
        &mut db,
        "process",
        IdentifierKind::Call,
        "src/main.rs",
        25,
        None,
    );

    let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

    assert_eq!(ctx.incoming.len(), 1, "identifier fallback should add ref");
    assert_eq!(ctx.incoming[0].file_path, "src/main.rs");
    assert_eq!(ctx.incoming[0].line_number, 25);
    assert_eq!(ctx.incoming_total, 1);
}

#[test]
fn test_identifier_fallback_deduplicates() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![
        make_symbol(
            "sym-target",
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            None,
            None,
            None,
        ),
        make_symbol(
            "sym-caller",
            "main",
            SymbolKind::Function,
            "src/main.rs",
            5,
            None,
            None,
            None,
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    // Relationship at src/main.rs:8
    let rels = vec![make_rel(
        "rel-1",
        "sym-caller",
        "sym-target",
        RelationshipKind::Calls,
        "src/main.rs",
        8,
    )];
    db.store_relationships(&rels).unwrap();

    // Identifier at the SAME location — should be deduped
    insert_identifier(
        &mut db,
        "process",
        IdentifierKind::Call,
        "src/main.rs",
        8,
        Some("sym-caller"),
    );

    // Plus one at a DIFFERENT location — should be added
    insert_identifier(
        &mut db,
        "process",
        IdentifierKind::Call,
        "src/handler.rs",
        42,
        None,
    );

    let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

    // 1 from relationship + 1 from identifier (the duplicate is deduped)
    assert_eq!(
        ctx.incoming.len(),
        2,
        "should have 1 relationship + 1 identifier ref, got {}",
        ctx.incoming.len()
    );
    assert_eq!(ctx.incoming_total, 2);
}

#[test]
fn test_identifier_fallback_filters_own_file_definition_line() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![make_symbol(
        "sym-target",
        "process",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        None,
        None,
        None,
    )];
    db.store_symbols(&symbols).unwrap();

    // Identifier at the definition site itself — should be skipped
    insert_identifier(
        &mut db,
        "process",
        IdentifierKind::Call,
        "src/engine.rs",
        10,
        None,
    );
    // Identifier at a different location — should be kept
    insert_identifier(
        &mut db,
        "process",
        IdentifierKind::Call,
        "src/main.rs",
        30,
        None,
    );

    let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

    assert_eq!(
        ctx.incoming.len(),
        1,
        "should skip definition-site identifiers"
    );
    assert_eq!(ctx.incoming[0].file_path, "src/main.rs");
}

// === Test refs (test file identifiers at full depth) ===

#[test]
fn test_build_context_populates_test_refs_at_full_depth() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![make_symbol(
        "sym-target",
        "process",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        None,
        None,
        None,
    )];
    db.store_symbols(&symbols).unwrap();

    // Identifier in a test file
    insert_identifier(
        &mut db,
        "process",
        IdentifierKind::Call,
        "src/tests/search_tests.rs",
        42,
        None,
    );
    // Identifier in a non-test file (should NOT be in test_refs)
    insert_identifier(
        &mut db,
        "process",
        IdentifierKind::Call,
        "src/main.rs",
        25,
        None,
    );

    let ctx = build_symbol_context(&db, &symbols[0], "full", 10, 10).unwrap();

    assert_eq!(ctx.test_refs.len(), 1, "should have 1 test ref");
    assert_eq!(ctx.test_refs[0].file_path, "src/tests/search_tests.rs");
}

#[test]
fn test_build_context_no_test_refs_at_overview() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![make_symbol(
        "sym-target",
        "process",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        None,
        None,
        None,
    )];
    db.store_symbols(&symbols).unwrap();

    // Identifier in a test file
    insert_identifier(
        &mut db,
        "process",
        IdentifierKind::Call,
        "src/tests/search_tests.rs",
        42,
        None,
    );

    let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

    assert!(
        ctx.test_refs.is_empty(),
        "overview should not populate test_refs"
    );
}

#[test]
fn test_build_context_caps_incoming() {
    let (_tmp, mut db) = setup_db();

    let mut symbols = vec![make_symbol(
        "sym-target",
        "process",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        None,
        None,
        None,
    )];

    // Create 5 callers
    let mut rels = vec![];
    for i in 0..5 {
        let caller_id = format!("sym-caller-{}", i);
        symbols.push(make_symbol(
            &caller_id,
            &format!("caller_{}", i),
            SymbolKind::Function,
            "src/main.rs",
            (i * 10 + 1) as u32,
            None,
            None,
            None,
            None,
        ));
        rels.push(make_rel(
            &format!("rel-{}", i),
            &caller_id,
            "sym-target",
            RelationshipKind::Calls,
            "src/main.rs",
            (i * 10 + 5) as u32,
        ));
    }
    db.store_symbols(&symbols).unwrap();
    db.store_relationships(&rels).unwrap();

    // Cap at 2 incoming
    let ctx = build_symbol_context(&db, &symbols[0], "overview", 2, 10).unwrap();

    assert_eq!(ctx.incoming.len(), 2, "should cap at 2");
    assert_eq!(ctx.incoming_total, 5, "total should reflect all 5");
}

// === disambiguation threshold tests ===

#[test]
fn test_deep_dive_query_returns_compact_list_when_too_many_matches() {
    let (_tmp, mut db) = setup_db();

    // Register extra files beyond what setup_db provides
    let extra_files = [
        "src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs", "src/e.rs", "src/f.rs",
    ];
    for file in &extra_files {
        db.store_file_info(&FileInfo {
            path: file.to_string(),
            language: "rust".to_string(),
            hash: format!("hash_{}", file),
            size: 100,
            last_modified: 1000000,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    // Create 6 symbols with the same name in different files (exceeds threshold of 5)
    let symbols: Vec<Symbol> = extra_files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            make_symbol(
                &format!("sym-extract-{}", i),
                "extract",
                SymbolKind::Function,
                file,
                10,
                None,
                Some("pub fn extract()"),
                Some(Visibility::Public),
                None,
            )
        })
        .collect();
    db.store_symbols(&symbols).unwrap();

    // Call deep_dive_query — should get compact disambiguation, not full contexts
    let result = deep_dive_query(&db, "extract", None, "overview", 10, 10).unwrap();

    // Should mention the count and disambiguation hint
    assert!(
        result.contains("Found 6 definitions"),
        "Should report 6 definitions, got: {}",
        result
    );
    assert!(
        result.contains("context_file"),
        "Should suggest using context_file"
    );

    // Should list file paths compactly
    for file in &extra_files {
        assert!(
            result.contains(file),
            "Should list file path '{}' in compact output",
            file
        );
    }

    // Should NOT contain full context markers (callers, callees, body sections)
    assert!(
        !result.contains("Callers"),
        "Should NOT build full context for 6+ matches"
    );
    assert!(
        !result.contains("Callees"),
        "Should NOT build full context for 6+ matches"
    );
}

#[test]
fn test_deep_dive_query_shows_full_context_at_threshold() {
    let (_tmp, mut db) = setup_db();

    // Create exactly 5 symbols (at the threshold — should get full context)
    let files = ["src/engine.rs", "src/main.rs", "src/handler.rs"];
    let extra_files = ["src/a2.rs", "src/b2.rs"];
    for file in &extra_files {
        db.store_file_info(&FileInfo {
            path: file.to_string(),
            language: "rust".to_string(),
            hash: format!("hash_{}", file),
            size: 100,
            last_modified: 1000000,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    let all_files: Vec<&str> = files.iter().chain(extra_files.iter()).copied().collect();
    let symbols: Vec<Symbol> = all_files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            make_symbol(
                &format!("sym-proc-{}", i),
                "process",
                SymbolKind::Function,
                file,
                10,
                None,
                Some("pub fn process()"),
                Some(Visibility::Public),
                None,
            )
        })
        .collect();
    db.store_symbols(&symbols).unwrap();

    // 5 matches — at threshold, should get full context (not compact list)
    let result = deep_dive_query(&db, "process", None, "overview", 10, 10).unwrap();

    assert!(
        result.contains("Found 5 definitions"),
        "Should report 5 definitions, got: {}",
        result
    );
    // Full context includes the definition header with signature
    assert!(
        result.contains("pub fn process()"),
        "Should include full context with signature at threshold of 5"
    );
}

// === Semantic similarity tests ===

#[test]
fn test_similar_symbols_at_full_depth() {
    let (_tmp, mut db) = setup_db();

    // Create two symbols with close embeddings
    let sym_a = make_symbol(
        "sym-a",
        "process_data",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        Some("fn process_data()"),
        Some(Visibility::Public),
        None,
    );
    let sym_b = make_symbol(
        "sym-b",
        "handle_data",
        SymbolKind::Function,
        "src/handler.rs",
        20,
        None,
        Some("fn handle_data()"),
        Some(Visibility::Public),
        None,
    );
    db.store_symbols(&[sym_a.clone(), sym_b.clone()]).unwrap();

    // Store close embeddings (small distance = high similarity)
    let emb_a: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
    let mut emb_b = emb_a.clone();
    // Nudge slightly so they're close but not identical
    emb_b[0] += 0.001;
    emb_b[1] += 0.001;
    db.store_embeddings(&[("sym-a".to_string(), emb_a), ("sym-b".to_string(), emb_b)])
        .unwrap();

    let ctx = build_symbol_context(&db, &sym_a, "full", 10, 10).unwrap();
    assert_eq!(ctx.similar.len(), 1, "Should find 1 similar symbol");
    assert_eq!(ctx.similar[0].symbol.name, "handle_data");
    assert!(ctx.similar[0].score > 0.0, "Score should be positive");
    assert!(ctx.similar[0].score <= 1.0, "Score should be <= 1.0");
}

#[test]
fn test_similar_symbols_skipped_when_no_embeddings() {
    let (_tmp, mut db) = setup_db();

    let sym = make_symbol(
        "sym-no-emb",
        "lonely_func",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        Some("fn lonely_func()"),
        Some(Visibility::Public),
        None,
    );
    db.store_symbols(&[sym.clone()]).unwrap();
    // No embeddings stored at all

    let ctx = build_symbol_context(&db, &sym, "full", 10, 10).unwrap();
    assert!(
        ctx.similar.is_empty(),
        "Should be empty when no embeddings exist"
    );
}

#[test]
fn test_similar_symbols_excludes_self() {
    let (_tmp, mut db) = setup_db();

    let sym = make_symbol(
        "sym-self",
        "self_func",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        Some("fn self_func()"),
        Some(Visibility::Public),
        None,
    );
    db.store_symbols(&[sym.clone()]).unwrap();

    // Store embedding for this symbol only
    let emb: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
    db.store_embeddings(&[("sym-self".to_string(), emb)])
        .unwrap();

    let ctx = build_symbol_context(&db, &sym, "full", 10, 10).unwrap();
    // The symbol should NOT appear in its own similar results
    assert!(
        ctx.similar.is_empty(),
        "Should not include self in similar results"
    );
}

#[test]
fn test_similar_symbols_at_context_depth() {
    let (_tmp, mut db) = setup_db();

    let sym_a = make_symbol(
        "sym-c",
        "func_alpha",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        Some("fn func_alpha()"),
        Some(Visibility::Public),
        None,
    );
    let sym_b = make_symbol(
        "sym-d",
        "func_beta",
        SymbolKind::Function,
        "src/handler.rs",
        20,
        None,
        Some("fn func_beta()"),
        Some(Visibility::Public),
        None,
    );
    db.store_symbols(&[sym_a.clone(), sym_b.clone()]).unwrap();

    let emb_a: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
    let emb_b = emb_a.clone();
    db.store_embeddings(&[("sym-c".to_string(), emb_a), ("sym-d".to_string(), emb_b)])
        .unwrap();

    // At "context" depth, similar SHOULD be populated
    let ctx_context = build_symbol_context(&db, &sym_a, "context", 10, 10).unwrap();
    assert!(
        !ctx_context.similar.is_empty(),
        "similar should be populated at context depth"
    );

    // At "overview" depth, similar should NOT be populated
    let ctx_overview = build_symbol_context(&db, &sym_a, "overview", 10, 10).unwrap();
    assert!(
        ctx_overview.similar.is_empty(),
        "similar should be empty at overview depth"
    );
}

#[test]
fn test_build_context_populates_test_refs_at_context_depth() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![make_symbol(
        "sym-target",
        "process",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        None,
        None,
        None,
    )];
    db.store_symbols(&symbols).unwrap();

    // Identifier in a test file
    insert_identifier(
        &mut db,
        "process",
        IdentifierKind::Call,
        "src/tests/search_tests.rs",
        42,
        None,
    );

    // At "context" depth, test_refs SHOULD be populated
    let ctx = build_symbol_context(&db, &symbols[0], "context", 10, 10).unwrap();
    assert_eq!(
        ctx.test_refs.len(),
        1,
        "context depth should populate test_refs"
    );
    assert_eq!(ctx.test_refs[0].file_path, "src/tests/search_tests.rs");

    // At "overview" depth, test_refs should NOT be populated
    let ctx_overview = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();
    assert!(
        ctx_overview.test_refs.is_empty(),
        "overview should not populate test_refs"
    );
}

#[test]
fn test_build_context_uses_test_symbol_metadata_for_test_refs() {
    let (_tmp, mut db) = setup_db();

    db.store_file_info(&FileInfo {
        path: "integration/auth_flow.rs".to_string(),
        language: "rust".to_string(),
        hash: "hash_integration_auth_flow".to_string(),
        size: 500,
        last_modified: 1_000_000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    })
    .unwrap();

    let target = make_symbol(
        "sym-target",
        "process",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        None,
        None,
        None,
    );
    let mut test_symbol = make_symbol(
        "sym-test",
        "auth_flow_succeeds",
        SymbolKind::Function,
        "integration/auth_flow.rs",
        40,
        None,
        Some("fn auth_flow_succeeds()"),
        None,
        None,
    );
    test_symbol.metadata = Some(HashMap::from([(
        "is_test".to_string(),
        serde_json::Value::Bool(true),
    )]));
    db.store_symbols(&[target.clone(), test_symbol]).unwrap();

    insert_identifier(
        &mut db,
        "process",
        IdentifierKind::Call,
        "integration/auth_flow.rs",
        42,
        Some("sym-test"),
    );

    let ctx = build_symbol_context(&db, &target, "context", 10, 10).unwrap();
    assert_eq!(
        ctx.test_refs.len(),
        1,
        "test_refs should honor containing symbol metadata even when the file path lacks test markers"
    );
    assert_eq!(ctx.test_refs[0].file_path, "integration/auth_flow.rs");
    assert_eq!(
        ctx.test_refs[0].symbol.as_ref().unwrap().name,
        "auth_flow_succeeds"
    );
}

// === Same-file overload auto-selection tests ===

#[test]
fn test_deep_dive_auto_selects_class_from_same_file_overloads() {
    let (_tmp, mut db) = setup_db();

    // Register a C++ header file
    db.store_file_info(&FileInfo {
        path: "include/foo.hpp".to_string(),
        language: "cpp".to_string(),
        hash: "hash_foo_hpp".to_string(),
        size: 5000,
        last_modified: 1000000,
        last_indexed: 0,
        symbol_count: 8,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // Create 8 symbols named "Foo" in the same file:
    // 1 class definition + 7 constructor overloads (Function kind)
    let class_sym = make_symbol(
        "sym-foo-class",
        "Foo",
        SymbolKind::Class,
        "include/foo.hpp",
        77,
        None,
        Some("class Foo"),
        Some(Visibility::Public),
        None,
    );

    let mut all_symbols = vec![class_sym];
    for i in 0..7 {
        all_symbols.push(make_symbol(
            &format!("sym-foo-ctor-{}", i),
            "Foo",
            SymbolKind::Function,
            "include/foo.hpp",
            100 + i * 20,
            Some("sym-foo-class"),
            Some(&format!("Foo(arg{})", i)),
            Some(Visibility::Public),
            None,
        ));
    }
    db.store_symbols(&all_symbols).unwrap();

    // Call deep_dive_query — 8 symbols > DISAMBIGUATION_THRESHOLD (5), all in same file
    // Should auto-select the class, not return disambiguation list
    let result = deep_dive_query(&db, "Foo", None, "overview", 10, 10).unwrap();

    // Should contain the auto-selection note
    assert!(
        result.contains("Auto-selected"),
        "Should contain auto-selection note, got:\n{}",
        result
    );

    // Should contain the class symbol's signature (proof we picked the class)
    assert!(
        result.contains("class Foo"),
        "Should show the class definition signature, got:\n{}",
        result
    );

    // Should contain the class's file:line location
    assert!(
        result.contains("include/foo.hpp:77"),
        "Should show class location, got:\n{}",
        result
    );

    // Should NOT contain the disambiguation prompt
    assert!(
        !result.contains("Use context_file to disambiguate"),
        "Should NOT ask for disambiguation when auto-selecting, got:\n{}",
        result
    );
}

#[test]
fn test_deep_dive_still_disambiguates_when_results_span_multiple_files() {
    let (_tmp, mut db) = setup_db();

    // Register extra files
    let files = [
        "src/engine.rs",
        "src/main.rs",
        "src/handler.rs",
        "lib/a.rs",
        "lib/b.rs",
        "lib/c.rs",
    ];
    for file in &files {
        // Some files already registered by setup_db; store_file_info is idempotent
        let _ = db.store_file_info(&FileInfo {
            path: file.to_string(),
            language: "rust".to_string(),
            hash: format!("hash_{}", file),
            size: 100,
            last_modified: 1000000,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 0,
            content: None,
        });
    }

    // Create 6 symbols in 6 different files — no file dominates
    let symbols: Vec<Symbol> = files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            make_symbol(
                &format!("sym-multi-{}", i),
                "handle",
                SymbolKind::Function,
                file,
                10,
                None,
                Some("fn handle()"),
                Some(Visibility::Public),
                None,
            )
        })
        .collect();
    db.store_symbols(&symbols).unwrap();

    // 6 symbols across 6 files — should get normal disambiguation list
    let result = deep_dive_query(&db, "handle", None, "overview", 10, 10).unwrap();

    assert!(
        result.contains("Use context_file to disambiguate"),
        "Should ask for disambiguation when results span multiple files, got:\n{}",
        result
    );
    assert!(
        !result.contains("Auto-selected"),
        "Should NOT auto-select when results span multiple files, got:\n{}",
        result
    );
}
