use crate::database::SymbolDatabase;

/// Empirical challenges verifying Finding 3: Generation Mutation Atomicity & Watcher Admission:
///
/// 1. publish_embedding_generation:
///    - Publishing with fewer stored vectors than declared embedded symbols fails with "does not match declared embedded symbols".
///    - Publishing an incomplete generation (`embedded_symbols < eligible_symbols`) fails with "Cannot publish incomplete embedding generation".
///    - Publishing a zero-eligible generation succeeds only when 0 vectors are stored.
/// 2. store_file_embeddings_for_ready_generation:
///    - If the expected generation revision has advanced, the update is strictly rejected.
///    - Successful updates advance `source_revision` and update `embedded_symbols` and `eligible_symbols` to match actual storage count.
#[test]
fn challenge_publish_embedding_generation_fewer_stored_than_declared_fails() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen_id = db.begin_embedding_generation("encoder-a", 1, 384).unwrap();
    let pair = vec![("sym1".to_string(), vec![0.1_f32; 384])];
    db.store_embeddings_for_generation(gen_id, &pair).unwrap();

    // Stored vectors in DB is 1, but caller declares 2 embedded symbols
    let err = db.publish_embedding_generation(gen_id, 1, 2, 2);
    assert!(
        err.is_err(),
        "Publishing with declared embedded > stored vectors must fail"
    );
    let err_msg = err.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not match declared embedded symbols"),
        "Error message must contain 'does not match declared embedded symbols', got: {err_msg}"
    );
}

#[test]
fn challenge_publish_embedding_generation_incomplete_fails() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen_id = db.begin_embedding_generation("encoder-a", 2, 384).unwrap();
    let pair = vec![("sym1".to_string(), vec![0.1_f32; 384])];
    db.store_embeddings_for_generation(gen_id, &pair).unwrap();

    // Stored vectors is 1, declared embedded is 1, but eligible is 2
    let err = db.publish_embedding_generation(gen_id, 2, 2, 1);
    assert!(err.is_err(), "Publishing incomplete generation must fail");
    let err_msg = err.unwrap_err().to_string();
    assert!(
        err_msg.contains("Cannot publish incomplete embedding generation"),
        "Error message must contain 'Cannot publish incomplete embedding generation', got: {err_msg}"
    );
}

#[test]
fn challenge_publish_embedding_generation_zero_eligible_succeeds_only_when_zero_vectors_stored() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    // Case A: 0 eligible, 0 vectors stored -> succeeds
    let gen_empty = db.begin_embedding_generation("encoder-a", 3, 384).unwrap();
    let res_empty = db.publish_embedding_generation(gen_empty, 3, 0, 0);
    assert!(
        res_empty.is_ok(),
        "Zero-eligible generation with 0 vectors stored must succeed"
    );
    assert!(db.embedding_generation_ready("encoder-a", 3).unwrap());

    // Case B: 0 eligible, but 1 vector stored -> fails
    let gen_nonempty = db.begin_embedding_generation("encoder-a", 4, 384).unwrap();
    let pair = vec![("sym1".to_string(), vec![0.1_f32; 384])];
    db.store_embeddings_for_generation(gen_nonempty, &pair)
        .unwrap();

    // Declaring (0 eligible, 0 embedded) when 1 vector stored fails
    let err_zero_embedded = db.publish_embedding_generation(gen_nonempty, 4, 0, 0);
    assert!(
        err_zero_embedded.is_err(),
        "Must fail when 1 vector stored but 0 declared"
    );

    // Declaring (0 eligible, 1 embedded) when 1 vector stored fails
    let err_one_embedded = db.publish_embedding_generation(gen_nonempty, 4, 0, 1);
    assert!(
        err_one_embedded.is_err(),
        "Must fail when eligible is 0 but vectors exist"
    );
    let err_msg = err_one_embedded.unwrap_err().to_string();
    assert!(
        err_msg.contains("eligible symbols is 0 but actual stored vector count is 1"),
        "Error message must report eligible symbols is 0 with positive vector count, got: {err_msg}"
    );
}

#[test]
fn challenge_store_file_embeddings_rejects_when_generation_revision_advanced() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen_id = db.publish_test_generation("encoder-a", 10, 384).unwrap();

    // Advance canonical revision to 12 and perform a watcher update to advance generation revision to 12
    db.conn
        .execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at) VALUES (12, 'ws1', 'incremental', 0)",
            [],
        )
        .unwrap();
    let batch_init = vec![("sym_init".to_string(), vec![0.2_f32; 384])];
    db.store_file_embeddings_for_ready_generation(
        "src/init.rs",
        "encoder-a",
        gen_id,
        10,
        &batch_init,
    )
    .unwrap();

    let meta = db.get_embedding_generation(gen_id).unwrap().unwrap();
    assert_eq!(
        meta.source_revision, 12,
        "Generation revision should have advanced to 12"
    );

    // Now a concurrent watcher update prepared against old revision 10 tries to write
    let batch_concurrent = vec![("sym_stale".to_string(), vec![0.3_f32; 384])];
    let err = db.store_file_embeddings_for_ready_generation(
        "src/stale.rs",
        "encoder-a",
        gen_id,
        10,
        &batch_concurrent,
    );
    assert!(
        err.is_err(),
        "Watcher update with stale expected revision 10 must be rejected"
    );
    let err_msg = err.unwrap_err().to_string();
    assert!(
        err_msg.contains("revision advanced from 10 to 12 during inference"),
        "Error message must indicate revision advanced from 10 to 12, got: {err_msg}"
    );
}

#[test]
fn challenge_store_file_embeddings_advances_source_revision_and_updates_counts() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen_id = db.publish_test_generation("encoder-a", 10, 384).unwrap();

    // Insert files and symbols for a.rs
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified) VALUES ('src/a.rs', 'rust', 'ha', 10, 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('sym_a1', 'fn_a1', 'function', 'rust', 'src/a.rs')",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('sym_a2', 'fn_a2', 'function', 'rust', 'src/a.rs')",
            [],
        )
        .unwrap();

    // Advance canonical revision to 25
    db.conn
        .execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at) VALUES (25, 'ws1', 'incremental', 0)",
            [],
        )
        .unwrap();

    // Insert symbols for src/a.rs
    let batch_a = vec![
        ("sym_a1".to_string(), vec![0.1_f32; 384]),
        ("sym_a2".to_string(), vec![0.2_f32; 384]),
    ];
    db.store_file_embeddings_for_ready_generation("src/a.rs", "encoder-a", gen_id, 10, &batch_a)
        .unwrap();

    let meta_a = db.get_embedding_generation(gen_id).unwrap().unwrap();
    assert_eq!(
        meta_a.source_revision, 25,
        "source_revision must advance to 25"
    );
    assert_eq!(
        meta_a.embedded_symbols, 2,
        "embedded_symbols must match actual vector count"
    );
    assert_eq!(
        meta_a.eligible_symbols, 2,
        "eligible_symbols must match actual vector count"
    );

    // Advance canonical revision to 30 and insert files/symbols for src/b.rs
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified) VALUES ('src/b.rs', 'rust', 'hb', 10, 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('sym_b1', 'fn_b1', 'function', 'rust', 'src/b.rs')",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at) VALUES (30, 'ws1', 'incremental', 0)",
            [],
        )
        .unwrap();

    let batch_b = vec![("sym_b1".to_string(), vec![0.3_f32; 384])];
    db.store_file_embeddings_for_ready_generation("src/b.rs", "encoder-a", gen_id, 25, &batch_b)
        .unwrap();

    let meta_b = db.get_embedding_generation(gen_id).unwrap().unwrap();
    assert_eq!(
        meta_b.source_revision, 30,
        "source_revision must advance to 30"
    );

    let actual_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM symbol_vectors", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        actual_count, 3,
        "Total stored vectors must be 3 (2 from a.rs + 1 from b.rs)"
    );
    assert_eq!(meta_b.embedded_symbols, 3, "embedded_symbols must equal 3");
    assert_eq!(meta_b.eligible_symbols, 3, "eligible_symbols must equal 3");
}

#[test]
fn challenge_earlier_failed_file_update_prevents_subsequent_watcher_update_from_advancing_source_revision()
 {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen_id = db.publish_test_generation("encoder-a", 10, 384).unwrap();

    // File A modified in rev 11 with 2 symbols, but its embedding fails (0 vectors stored)
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified) VALUES ('src/a.rs', 'rust', 'ha', 10, 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('sym_a1', 'fn_a1', 'function', 'rust', 'src/a.rs')",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('sym_a2', 'fn_a2', 'function', 'rust', 'src/a.rs')",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at) VALUES (11, 'ws1', 'incremental', 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO revision_file_changes (revision, workspace_id, file_path, change_kind) VALUES (11, 'ws1', 'src/a.rs', 'modified')",
            [],
        )
        .unwrap();

    // File B modified in rev 12 with 1 symbol, and its embedding succeeds
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified) VALUES ('src/b.rs', 'rust', 'hb', 10, 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('sym_b1', 'fn_b1', 'function', 'rust', 'src/b.rs')",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at) VALUES (12, 'ws1', 'incremental', 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO revision_file_changes (revision, workspace_id, file_path, change_kind) VALUES (12, 'ws1', 'src/b.rs', 'modified')",
            [],
        )
        .unwrap();

    // Embed File B only (File A embedding previously failed / was omitted)
    let batch_b = vec![("sym_b1".to_string(), vec![0.3_f32; 384])];
    db.store_file_embeddings_for_ready_generation("src/b.rs", "encoder-a", gen_id, 10, &batch_b)
        .unwrap();

    let meta = db.get_embedding_generation(gen_id).unwrap().unwrap();
    // 1. source_revision must NOT advance to 12 because File A has un-embedded symbols
    assert_eq!(
        meta.source_revision, 10,
        "source_revision must remain 10 when earlier file A has outstanding un-embedded symbols"
    );
    // 2. embedded_symbols reflects actual vectors stored (1)
    assert_eq!(meta.embedded_symbols, 1, "embedded_symbols must equal 1");
    // 3. eligible_symbols retains File A in the denominator (3 total: a1, a2, b1)
    assert_eq!(
        meta.eligible_symbols, 3,
        "eligible_symbols must retain missing file A coverage in the denominator"
    );
    // 4. is_complete must be false
    assert!(
        !meta.is_complete(),
        "generation must not be complete when earlier file failed"
    );
    assert!(
        (meta.coverage_ratio() - (1.0 / 3.0)).abs() < 1e-4,
        "coverage ratio must be 1/3"
    );
}

#[test]
fn challenge_watcher_eligibility_excludes_tests_and_non_embeddable_languages() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen_id = db.publish_test_generation("encoder-a", 10, 384).unwrap();

    // 1. Production file with 1 embeddable function
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified) VALUES ('src/lib.rs', 'rust', 'h1', 10, 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('prod_fn', 'process', 'function', 'rust', 'src/lib.rs')",
            [],
        )
        .unwrap();

    // 2. Test file in tests/ with a function (excluded by path)
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified) VALUES ('tests/lib_test.rs', 'rust', 'h2', 10, 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('test_fn', 'test_process', 'function', 'rust', 'tests/lib_test.rs')",
            [],
        )
        .unwrap();

    // 3. Markdown file with a heading/function (excluded by non-embeddable language)
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified) VALUES ('README.md', 'markdown', 'h3', 10, 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('doc_heading', 'Overview', 'function', 'markdown', 'README.md')",
            [],
        )
        .unwrap();

    // 4. Production file with an annotated test symbol (excluded by metadata)
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified) VALUES ('src/test_helpers.rs', 'rust', 'h4', 10, 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, metadata) VALUES ('test_role_fn', 'helper', 'function', 'rust', 'src/test_helpers.rs', '{\"test_role\":\"test_case\"}')",
            [],
        )
        .unwrap();

    // Advance canonical revision to 20
    db.conn
        .execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at) VALUES (20, 'ws1', 'incremental', 0)",
            [],
        )
        .unwrap();

    // Watcher embeds the single production function
    let batch_prod = vec![("prod_fn".to_string(), vec![0.1_f32; 384])];
    db.store_file_embeddings_for_ready_generation(
        "src/lib.rs",
        "encoder-a",
        gen_id,
        10,
        &batch_prod,
    )
    .unwrap();

    let meta = db.get_embedding_generation(gen_id).unwrap().unwrap();
    assert_eq!(
        meta.embedded_symbols, 1,
        "embedded_symbols must be 1 (only production function has vector)"
    );
    assert_eq!(
        meta.eligible_symbols, 1,
        "eligible_symbols must be 1 (tests, markdown, and test metadata are excluded!)"
    );
    assert_eq!(
        meta.source_revision, 20,
        "source_revision must advance to 20 when all eligible symbols are embedded"
    );
    assert!(
        meta.is_complete(),
        "generation must be complete (1/1 eligible embedded)"
    );

    // Now test has_outstanding_changed_files with a modified test file at rev 22
    db.conn
        .execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at) VALUES (25, 'ws1', 'incremental', 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO revision_file_changes (revision, workspace_id, file_path, change_kind) VALUES (22, 'ws1', 'tests/lib_test.rs', 'modified')",
            [],
        )
        .unwrap();

    // Embed another production file at rev 25
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified) VALUES ('src/prod2.rs', 'rust', 'h5', 10, 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('prod2_fn', 'render', 'function', 'rust', 'src/prod2.rs')",
            [],
        )
        .unwrap();

    let batch_prod2 = vec![("prod2_fn".to_string(), vec![0.2_f32; 384])];
    db.store_file_embeddings_for_ready_generation(
        "src/prod2.rs",
        "encoder-a",
        gen_id,
        20,
        &batch_prod2,
    )
    .unwrap();

    let meta2 = db.get_embedding_generation(gen_id).unwrap().unwrap();
    assert_eq!(
        meta2.source_revision, 25,
        "source_revision must advance to 25 ignoring outstanding test file changes"
    );
    assert_eq!(meta2.embedded_symbols, 2);
    assert_eq!(meta2.eligible_symbols, 2);

    // 5. Configured extra_kinds: add a 'constant' symbol
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('const_sym', 'MAX_RETRIES', 'constant', 'rust', 'src/prod2.rs')",
            [],
        )
        .unwrap();

    // Advance canonical revision to 30
    db.conn
        .execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at) VALUES (30, 'ws1', 'incremental', 0)",
            [],
        )
        .unwrap();

    // Without extra_kinds, 'constant' is excluded -> eligible stays 2
    let batch_prod3 = vec![("prod2_fn".to_string(), vec![0.25_f32; 384])];
    db.store_file_embeddings_for_ready_generation(
        "src/prod2.rs",
        "encoder-a",
        gen_id,
        25,
        &batch_prod3,
    )
    .unwrap();
    let meta3 = db.get_embedding_generation(gen_id).unwrap().unwrap();
    assert_eq!(
        meta3.eligible_symbols, 2,
        "without extra_kinds, constant is excluded"
    );
    assert_eq!(meta3.source_revision, 30);

    // Advance canonical revision to 35
    db.conn
        .execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at) VALUES (35, 'ws1', 'incremental', 0)",
            [],
        )
        .unwrap();

    // With extra_kinds including 'constant', 'const_sym' becomes eligible -> eligible becomes 3
    let batch_with_const = vec![
        ("prod2_fn".to_string(), vec![0.25_f32; 384]),
        ("const_sym".to_string(), vec![0.30_f32; 384]),
    ];
    let extra_rust = vec![("rust".to_string(), vec!["constant".to_string()])];
    db.store_file_embeddings_for_ready_generation_with_extra_kinds(
        "src/prod2.rs",
        "encoder-a",
        gen_id,
        30,
        &batch_with_const,
        &extra_rust,
    )
    .unwrap();
    let meta4 = db.get_embedding_generation(gen_id).unwrap().unwrap();
    assert_eq!(meta4.embedded_symbols, 3);
    assert_eq!(
        meta4.eligible_symbols, 3,
        "with extra_kinds = ['constant'] for rust, constant is included in eligible count"
    );
    assert_eq!(meta4.source_revision, 35);
    assert!(meta4.is_complete());
}

#[test]
fn challenge_sql_eligibility_filter_does_not_exclude_contest_or_testing_production_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();
    let conn = &mut db.conn;

    conn.execute(
        "INSERT INTO files (path, language, hash, size, last_modified) VALUES
         ('pkg/contest.go', 'go', 'h1', 10, 1),
         ('pkg/testing.py', 'python', 'h2', 10, 1),
         ('src/not_tests/app.ts', 'typescript', 'h3', 10, 1),
          ('pkg/foo_test.go', 'go', 'h4', 10, 1),
          ('test_bar.py', 'python', 'h5', 10, 1),
          ('pkg/test_baz.py', 'python', 'h6', 10, 1),
          ('__tests__/component.tsx', 'typescript', 'h7', 10, 1),
          ('pkg/test_helpers/production.py', 'python', 'h8', 10, 1),
          ('pkg/quote\"dir/production.py', 'python', 'h9', 10, 1),
          ('pkg/quote\"dir/test_foo.py', 'python', 'h10', 10, 1)",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO symbols (id, file_path, name, kind, language) VALUES
         ('s1', 'pkg/contest.go', 'ContestFn', 'function', 'go'),
         ('s2', 'pkg/testing.py', 'TestingFn', 'function', 'python'),
         ('s3', 'src/not_tests/app.ts', 'AppFn', 'function', 'typescript'),
         ('s4', 'pkg/foo_test.go', 'TestFoo', 'function', 'go'),
         ('s5', 'test_bar.py', 'TestBar', 'function', 'python'),
         ('s6', 'pkg/test_baz.py', 'TestBaz', 'function', 'python'),
         ('s7', '__tests__/component.tsx', 'Comp', 'function', 'typescript'),
         ('s8', 'pkg/test_helpers/production.py', 'ProdHelper', 'function', 'python'),
         ('s9', 'pkg/quote\"dir/production.py', 'QuoteProd', 'function', 'python'),
         ('s10', 'pkg/quote\"dir/test_foo.py', 'QuoteTest', 'function', 'python')",
        [],
    )
    .unwrap();

    let filter =
        crate::database::embedding_generation_eligibility::sql_symbol_eligibility_filter("s", &[]);
    let query = format!("SELECT id, file_path FROM symbols s WHERE {filter} ORDER BY id");
    let mut stmt = conn.prepare(&query).unwrap();
    let included_paths: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(
        included_paths.contains(&"pkg/quote\"dir/production.py".to_string()),
        "pkg/quote\"dir/production.py must NOT be excluded by quote or test pattern: got {:?}",
        included_paths
    );
    assert!(
        !included_paths.contains(&"pkg/quote\"dir/test_foo.py".to_string()),
        "pkg/quote\"dir/test_foo.py must be excluded"
    );
    assert!(
        included_paths.contains(&"pkg/test_helpers/production.py".to_string()),
        "pkg/test_helpers/production.py must NOT be excluded by python test prefix pattern: got {:?}",
        included_paths
    );
    assert!(
        included_paths.contains(&"pkg/contest.go".to_string()),
        "pkg/contest.go must NOT be excluded by '_' wildcard: got {:?}",
        included_paths
    );
    assert!(
        included_paths.contains(&"pkg/testing.py".to_string()),
        "pkg/testing.py must NOT be excluded by '_' wildcard: got {:?}",
        included_paths
    );
    assert!(
        included_paths.contains(&"src/not_tests/app.ts".to_string()),
        "src/not_tests/app.ts must NOT be excluded: got {:?}",
        included_paths
    );
    assert!(
        !included_paths.contains(&"pkg/foo_test.go".to_string()),
        "pkg/foo_test.go must be excluded"
    );
    assert!(
        !included_paths.contains(&"test_bar.py".to_string()),
        "test_bar.py must be excluded"
    );
    assert!(
        !included_paths.contains(&"pkg/test_baz.py".to_string()),
        "pkg/test_baz.py must be excluded"
    );
    assert!(
        !included_paths.contains(&"__tests__/component.tsx".to_string()),
        "__tests__/component.tsx must be excluded"
    );
}

#[test]
fn challenge_extra_kinds_scoped_strictly_to_configured_language() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();
    let conn = &mut db.conn;

    conn.execute(
        "INSERT INTO files (path, language, hash, size, last_modified) VALUES
         ('src/index.ts', 'typescript', 'h1', 10, 1),
         ('src/main.rs', 'rust', 'h2', 10, 1),
         ('src/app.js', 'javascript', 'h3', 10, 1),
         ('src/foo.ts', 'typescript', 'h4', 10, 1)",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO symbols (id, file_path, name, kind, language) VALUES
         ('ts_exp', 'src/index.ts', 'MyExport', 'export', 'typescript'),
         ('rs_exp', 'src/main.rs', 'RustExport', 'export', 'rust'),
         ('js_ctor', 'src/app.js', 'JsCtor', 'constructor', 'javascript'),
         ('ts_ctor', 'src/foo.ts', 'TsCtor', 'constructor', 'typescript')",
        [],
    )
    .unwrap();

    // Configure extra_kinds = ['export'] strictly for typescript
    let per_lang_config = vec![("typescript".to_string(), vec!["export".to_string()])];
    let filter = crate::database::embedding_generation_eligibility::sql_symbol_eligibility_filter(
        "s",
        &per_lang_config,
    );
    let query = format!("SELECT id FROM symbols s WHERE {filter}");
    let mut stmt = conn.prepare(&query).unwrap();
    let eligible_ids: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(
        eligible_ids,
        vec!["ts_exp".to_string()],
        "Only typescript export should be eligible; rust export and JS/TS constructor must be excluded: got {:?}",
        eligible_ids
    );
}

#[test]
fn challenge_publish_and_store_propagate_query_errors_and_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();
    let gen_id = db.begin_embedding_generation("encoder-a", 1, 384).unwrap();

    // 1. In publish_embedding_generation:
    // Corrupt symbol_vectors schema to cause a query_row error on actual vector count check
    db.conn.execute("DROP TABLE symbol_vectors", []).unwrap();
    let pub_res = db.publish_embedding_generation(gen_id, 1, 0, 0);
    assert!(
        pub_res.is_err(),
        "publish_embedding_generation must fail closed on query error"
    );

    // 2. In store_file_embeddings_for_ready_generation:
    // Create a clean DB with a published, READY generation at source_revision 1
    let dir2 = tempfile::tempdir().unwrap();
    let mut db2 = SymbolDatabase::new(&dir2.path().join("symbols.db")).unwrap();
    let ready_gen_id = db2.begin_embedding_generation("encoder-a", 1, 384).unwrap();
    db2.publish_embedding_generation(ready_gen_id, 1, 0, 0)
        .unwrap();

    // Invalidate revision_file_changes table so outstanding_query fails
    db2.conn
        .execute("DROP TABLE revision_file_changes", [])
        .unwrap();

    // With generation status='ready', store_file_embeddings reaches outstanding_query,
    // fails closed on the query error (via ? propagation), and rolls back
    let store_res = db2.store_file_embeddings_for_ready_generation(
        "test.py",
        "encoder-a",
        ready_gen_id,
        1,
        &[],
    );
    assert!(
        store_res.is_err(),
        "store_file_embeddings_for_ready_generation must fail closed on outstanding_query error"
    );

    // Verify rollback: source_revision in embedding_generations was NOT mutated
    let rev: i64 = db2
        .conn
        .query_row(
            "SELECT source_revision FROM embedding_generations WHERE id = ?1",
            [ready_gen_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rev, 1, "source_revision must remain rolled back to 1");
}
