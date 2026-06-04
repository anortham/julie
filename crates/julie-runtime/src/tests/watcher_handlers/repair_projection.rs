use super::*;

#[tokio::test]
async fn test_extractor_failure_is_persisted_durably() {
    let temp_dir = julie_test_support::unique_temp_dir("watcher_extractor_failure_repair");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let test_file = workspace_root.join("broken.rs");
    fs::write(&test_file, "fn stable_symbol() {}\n").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let guard = acquire_gate("test_extractor_failure_durable").await;

    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("initial parser-backed indexing should succeed");

    fs::write(
        &test_file,
        "// Parser-backed content with no symbols should trip the drop-safeguard\n",
    )
    .unwrap();

    let outcome = handle_file_created_or_modified_static(
        absolute_path,
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("Extractor failure should surface as repair-needed, not a hard error");

    assert_eq!(
        outcome.repair_reason,
        Some(IndexingRepairReason::ExtractorFailure),
        "parser-backed empty extraction after existing symbols should persist repair"
    );

    drop(db);

    let reopened = SymbolDatabase::new(&db_path).expect("Failed to reopen database");
    let persisted = reopened
        .conn
        .query_row(
            "SELECT reason, detail FROM indexing_repairs WHERE path = ?1",
            rusqlite::params!["broken.rs"],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .expect("repair state should persist across database reopen");

    assert_eq!(persisted.0, "extractor_failure");
    assert!(
        persisted
            .1
            .as_deref()
            .unwrap_or_default()
            .contains("refused to drop"),
        "repair detail should preserve the extractor failure context"
    );
}

// test_transaction_leak_on_error was removed: the raw begin_transaction /
// rollback_transaction helpers were deleted in favour of conn.transaction()
// (RAII). Transaction leaks from that pattern are structurally impossible now.

/// Test: handle_file_deleted_static always cleans up regardless of disk state.
///
/// Fix B-a: The old path.exists() guard has been removed from handle_file_deleted_static.
/// The atomic-save check (skip if file still exists) now lives exclusively in
/// dispatch_file_event, which guards before calling this function. This eliminates
/// the TOCTOU window where embeddings could be deleted by the caller while symbols
/// survived because the file was recreated between the two independent checks.
///
/// The handler now trusts the caller's decision to proceed with deletion.
#[tokio::test]
async fn test_delete_handler_always_cleans_up() {
    let temp_dir = julie_test_support::unique_temp_dir("atomic_save");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    // Create and index a file
    let test_file = workspace_root.join("edited_not_deleted.rs");
    fs::write(&test_file, "fn original() {}").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let guard = acquire_gate("test_delete_always_cleans_up").await;

    // Index the file
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("Initial indexing should succeed");

    // Verify symbol exists
    {
        let db_lock = db.lock().unwrap();
        let count = db_lock.count_symbols_for_workspace().unwrap();
        assert_eq!(count, 1, "Should have 1 symbol before deletion event");
    }

    // File still exists on disk — but we call handle_file_deleted_static directly,
    // simulating the scenario where the caller (dispatch_file_event) has already
    // decided to proceed with deletion. The handler must trust the caller.
    assert!(
        test_file.exists(),
        "File still exists — handler must clean up regardless (trust caller)"
    );

    handle_file_deleted_static(absolute_path, &db, &workspace_root, None, &guard)
        .await
        .expect("Delete handler should succeed");

    // Symbols MUST be deleted — the handler no longer has its own path.exists() guard
    {
        let db_lock = db.lock().unwrap();
        let count = db_lock.count_symbols_for_workspace().unwrap();
        assert_eq!(
            count, 0,
            "handle_file_deleted_static must clean up symbols regardless of disk state (Fix B-a)"
        );
    }
}

/// Regression test for Bug: File watcher drops Tantivy file content documents
///
/// Bug: handle_file_created_or_modified_static calls remove_by_file_path() which
/// deletes BOTH symbol docs AND file content docs from Tantivy, but only re-adds
/// symbol docs via add_symbol(). The add_file_content() call is missing.
///
/// Impact: Every file save progressively erodes the content search index. After
/// hours of editing, fast_search with search_target="content" returns zero results
/// for commonly-edited files because their content documents have been deleted.
///
/// Root cause:
/// - Line 164: idx.remove_by_file_path() — deletes ALL docs (symbols + file content)
/// - Lines 167-170: Only adds symbol docs back
/// - MISSING: idx.add_file_content() call to re-add file content doc
///
/// Compare with populate_tantivy_index() in processor.rs which correctly adds both.
#[tokio::test]
async fn test_incremental_indexing_preserves_tantivy_file_content() {
    use julie_index::search::index::{SearchDocument, SearchFilter, SearchIndex};

    let temp_dir = julie_test_support::unique_temp_dir("watcher_tantivy_content");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    // Create a Rust file with a distinctive identifier for content search
    let test_file = workspace_root.join("rich_component.rs");
    let initial_content = r#"
fn render_rich_text_field() {
    let widget = RichTextField::new();
    widget.display();
}
"#;
    fs::write(&test_file, initial_content).unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    // Initialize database
    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    // Create Tantivy search index
    let tantivy_dir = workspace_root.join("tantivy");
    fs::create_dir_all(&tantivy_dir).unwrap();
    let search_index = Arc::new(Mutex::new(
        SearchIndex::create(&tantivy_dir).expect("Failed to create search index"),
    ));

    // Seed Tantivy with initial file content (simulating what initial indexing does)
    {
        let idx = search_index.lock().unwrap();
        idx.add_search_doc(&SearchDocument::file_from_parts(
            "rich_component.rs",
            initial_content,
            "rust",
        ))
        .unwrap();
        idx.commit().unwrap();
    }

    // Verify content search works BEFORE incremental update
    {
        let idx = search_index.lock().unwrap();
        let results = idx
            .search_content("RichTextField", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Content search should find 'RichTextField' before incremental update"
        );
    }

    // Now simulate a file modification via the watcher handler
    let modified_content = r#"
fn render_rich_text_field() {
    let widget = RichTextField::new();
    widget.set_value("hello");
    widget.display();
}
"#;
    fs::write(&test_file, modified_content).unwrap();
    let guard = acquire_gate("test_tantivy_file_content").await;

    // Call the watcher handler WITH the search index (this is the code path that has the bug)
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        Some(&search_index),
        &guard,
    )
    .await
    .expect("Incremental indexing should succeed");

    // The handler intentionally defers Tantivy commit (production caller
    // process_pending_changes batch-commits after processing all events).
    // We must commit here to make the changes visible to the reader.
    {
        let idx = search_index.lock().unwrap();
        idx.commit().unwrap();
    }

    // Verify content search STILL works after incremental update
    {
        let idx = search_index.lock().unwrap();
        let results = idx
            .search_content("RichTextField", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Content search for 'RichTextField' should still work after file modification"
        );
        assert_eq!(
            results[0].file_path, "rich_component.rs",
            "Content search should find the correct file"
        );
    }

    // Also verify the NEW content is searchable (not just the old content)
    {
        let idx = search_index.lock().unwrap();
        let results = idx
            .search_content("set_value", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Content search should find new content 'set_value' added in the modification"
        );
    }

    // Verify symbol search still works too (sanity check)
    {
        let idx = search_index.lock().unwrap();
        let results = idx
            .search_symbols("render_rich_text_field", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Symbol search should still find 'render_rich_text_field' after incremental update"
        );
    }
}

#[tokio::test]
async fn test_incremental_indexing_preserves_tantivy_annotation_fields() {
    use julie_index::search::index::{SearchFilter, SearchIndex};

    let temp_dir = julie_test_support::unique_temp_dir("watcher_tantivy_annotations");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let test_file = workspace_root.join("annotated_test.rs");
    let content = r#"
#[test]
fn watched_annotation_marker() {
    assert_eq!(2 + 2, 4);
}
"#;
    fs::write(&test_file, content).unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    let tantivy_dir = workspace_root.join("tantivy");
    fs::create_dir_all(&tantivy_dir).unwrap();
    let search_index = Arc::new(Mutex::new(
        SearchIndex::create(&tantivy_dir).expect("Failed to create search index"),
    ));
    let guard = acquire_gate("test_annotation_fields").await;

    handle_file_created_or_modified_static(
        absolute_path,
        &db,
        &extractor_manager,
        &workspace_root,
        Some(&search_index),
        &guard,
    )
    .await
    .expect("incremental indexing should succeed");

    let idx = search_index.lock().unwrap();
    idx.commit().unwrap();
    let results = idx
        .search_symbols("@test", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        results
            .iter()
            .any(|result| result.name == "watched_annotation_marker"),
        "annotation search should find the watched test function after incremental indexing: {:?}",
        results
            .iter()
            .map(|result| result.name.as_str())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_incremental_indexing_projection_failure_reports_repair_reason() {
    use julie_index::search::index::SearchIndex;

    let temp_dir = julie_test_support::unique_temp_dir("watcher_projection_repair");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let test_file = workspace_root.join("projection_failure.rs");
    fs::write(&test_file, "fn projection_failure() {}\n").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    let tantivy_dir = workspace_root.join("tantivy");
    fs::create_dir_all(&tantivy_dir).unwrap();
    let search_index = Arc::new(Mutex::new(
        SearchIndex::create(&tantivy_dir).expect("Failed to create search index"),
    ));
    {
        let idx = search_index.lock().unwrap();
        idx.shutdown()
            .expect("search index should shut down cleanly");
    }
    let guard = acquire_gate("test_projection_failure").await;

    let outcome = handle_file_created_or_modified_static(
        absolute_path,
        &db,
        &extractor_manager,
        &workspace_root,
        Some(&search_index),
        &guard,
    )
    .await
    .expect("SQLite update should still succeed when projection fails");

    assert!(
        !outcome.tantivy_ok,
        "projection failure should surface a failed Tantivy status"
    );
    assert_eq!(
        outcome.repair_reason,
        Some(IndexingRepairReason::ProjectionFailure),
        "projection failure should use the shared repair vocabulary"
    );
}

/// Regression test: hash-match early return must clear stale repair entries.
///
/// Bug: When `retry_persisted_repairs` dispatches a file whose content hash
/// matches the stored hash, the handler returns early at the Blake3 check
/// without reaching the `clear_indexing_repair` call in the extraction
/// success path. The repair entry persists forever, causing an infinite
/// retry loop (every 1-second cycle) that bloats logs and wastes CPU.
#[tokio::test]
async fn test_hash_match_clears_stale_repair_entry() {
    let temp_dir = julie_test_support::unique_temp_dir("watcher_hash_match_repair_clear");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let test_file = workspace_root.join("stable.rs");
    fs::write(&test_file, "fn stable_symbol() {}\n").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let guard = acquire_gate("test_hash_match_repair_clear").await;

    // First pass: index the file (stores hash + symbols)
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("initial indexing should succeed");

    // Seed a stale repair entry (simulates a previous transient failure)
    {
        let db_lock = db.lock().unwrap();
        db_lock
            .record_indexing_repair("stable.rs", "extractor_failure", Some("stale repair"))
            .expect("seeding repair should succeed");
    }

    // Second pass: same file, unchanged content (hash will match -> early return)
    let outcome = handle_file_created_or_modified_static(
        absolute_path,
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("hash-match pass should succeed");

    assert_eq!(
        outcome.repair_reason, None,
        "hash-match should return clean outcome"
    );

    // The stale repair entry must be cleared
    let db_lock = db.lock().unwrap();
    let remaining: i64 = db_lock
        .conn
        .query_row(
            "SELECT COUNT(*) FROM indexing_repairs WHERE path = ?1",
            rusqlite::params!["stable.rs"],
            |row| row.get(0),
        )
        .expect("repair count query should succeed");
    assert_eq!(
        remaining, 0,
        "hash-match early return must clear stale repair entries to prevent infinite retry loops"
    );
}
