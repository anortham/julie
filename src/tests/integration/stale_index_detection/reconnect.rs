use super::*;

/// Given: A workspace with 3 indexed files, then 1 file is deleted from disk
/// When: check_if_indexing_needed() is called
/// Expected: Returns true (cleanup needed for deleted file)
#[tokio::test]
async fn test_deleted_file_detected_on_reconnect() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create 3 source files
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("a.rs"), "fn a() {}\n")?;
    fs::write(src_dir.join("b.rs"), "fn b() {}\n")?;
    fs::write(src_dir.join("c.rs"), "fn c() {}\n")?;

    // Index the workspace
    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Delete one file (simulating deletion while daemon was down)
    fs::remove_file(src_dir.join("b.rs"))?;

    // check_if_indexing_needed should detect the deleted file
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(
        needs_indexing,
        "Should detect deleted file b.rs needs cleanup"
    );

    Ok(())
}

/// Given: A workspace has a deleted file left in SQLite
/// When: startup repair runs
/// Expected: The orphan row is removed and the next startup check is clean
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_startup_repair_cleans_deleted_file_and_clears_next_check() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("a.rs"), "fn a() {}\n")?;
    fs::write(src_dir.join("b.rs"), "fn b() {}\n")?;
    fs::write(src_dir.join("c.rs"), "fn c() {}\n")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    fs::remove_file(src_dir.join("b.rs"))?;

    let repair = crate::startup::run_primary_workspace_repair(&handler).await?;
    let repair = repair.expect("deleted file should trigger startup repair");
    assert!(
        repair.reasons.contains(
            &crate::tools::workspace::indexing::state::IndexingRepairReason::DeletedFiles
        ),
        "repair reasons should include deleted_files: {:?}",
        repair.reasons
    );

    let db = handler.primary_database().await?;
    let files = {
        let db = db.lock().unwrap();
        db.get_all_indexed_files()?
    };
    assert!(
        !files.contains(&"src/b.rs".to_string()),
        "deleted file should be removed from SQLite: {:?}",
        files
    );

    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(
        !needs_indexing,
        "startup repair should clear the deleted-file condition"
    );

    Ok(())
}

#[tokio::test]
async fn test_check_if_indexing_needed_prefers_shared_anchor_over_local_julie_tree() -> Result<()> {
    use crate::registry::database::DaemonDatabase;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("daemon-indexes");
    fs::create_dir_all(&indexes_dir)?;

    let workspace_root = temp_dir.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    let test_file = workspace_root.join("main.rs");
    fs::write(&test_file, "fn shared_anchor() {}")?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);

    let workspace_path = workspace_root.canonicalize()?;
    let workspace_path_str = workspace_path.to_string_lossy().to_string();
    let workspace_id = crate::workspace::registry::generate_workspace_id(&workspace_path_str)?;
    let pooled_workspace = Arc::new(
        crate::workspace::JulieWorkspace::initialize_with_index_root(
            workspace_path.clone(),
            indexes_dir.join(&workspace_id),
        )
        .await?,
    );
    daemon_db.upsert_workspace(&workspace_id, &workspace_path_str, "ready")?;

    let mut handler = JulieServerHandler::new_with_shared_workspace(
        pooled_workspace,
        workspace_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(workspace_id.clone()),
        None,
        None,
    )
    .await?;
    // The shared anchor used to be carried by the WorkspacePool (removed in
    // Phase 3d.2b). Pin it directly so the force-reindex below re-initializes
    // db/tantivy under the shared `indexes_dir/{ws}` tree, not project-local.
    handler.in_process_index_root = Some(indexes_dir.join(&workspace_id));

    // Create a bogus local stdio tree with an old database mtime. The startup check
    // must ignore this and use the shared daemon anchor instead.
    let local_db_path = workspace_root
        .join(".julie")
        .join("indexes")
        .join(&workspace_id)
        .join("db")
        .join("symbols.db");
    fs::create_dir_all(local_db_path.parent().expect("local db parent"))?;
    fs::write(&local_db_path, b"bogus local db")?;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    index_workspace(&handler, &workspace_root).await?;

    let resolved_db_path = handler.workspace_db_file_path_for(&workspace_id).await?;
    assert_eq!(
        resolved_db_path,
        indexes_dir
            .join(&workspace_id)
            .join("db")
            .join("symbols.db"),
        "freshness check should resolve the shared daemon db path, not the local .julie decoy"
    );

    let _ = crate::startup::check_if_indexing_needed(&handler).await?;

    Ok(())
}

#[tokio::test]
async fn test_check_if_indexing_needed_uses_rebound_current_primary_snapshot() -> Result<()> {
    use crate::database::types::FileInfo;
    use crate::extractors::{Symbol, SymbolKind};
    use crate::registry::database::DaemonDatabase;
    use crate::workspace::registry::generate_workspace_id;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("daemon-indexes");
    fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(original_root.join("src"))?;
    fs::create_dir_all(rebound_root.join("src"))?;
    fs::write(
        original_root.join("src").join("loaded.rs"),
        "fn loaded_primary_only() {}\n",
    )?;
    fs::write(
        rebound_root.join("src").join("rebound.rs"),
        "fn rebound_primary_only() {}\n",
    )?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);

    let original_path = original_root.canonicalize()?;
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str)?;
    let original_ws =
        Arc::new(crate::workspace::JulieWorkspace::initialize(original_path.clone()).await?);
    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        original_ws,
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id),
        None,
        None,
    )
    .await?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    let rebound_ws =
        Arc::new(crate::workspace::JulieWorkspace::initialize(rebound_path.clone()).await?);
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

    {
        let rebound_db = rebound_ws
            .db
            .as_ref()
            .expect("rebound workspace should have a db")
            .clone();
        let mut rebound_db = rebound_db.lock().unwrap();
        let file_info = FileInfo {
            path: "src/rebound.rs".to_string(),
            language: "rust".to_string(),
            hash: "rebound-primary-hash".to_string(),
            size: 28,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 1,
            content: Some("fn rebound_primary_only() {}\n".to_string()),
        };
        let symbol = Symbol {
            id: "rebound-primary-symbol".to_string(),
            name: "rebound_primary_only".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/rebound.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 26,
            start_byte: 0,
            end_byte: 28,
            signature: Some("fn rebound_primary_only()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("fn rebound_primary_only() {}".to_string()),
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        };
        rebound_db.bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &rebound_id)?;
        rebound_db
            .store_embeddings(&[("rebound-primary-symbol".to_string(), vec![0.1_f32; 384])])?;
        rebound_db.set_index_engine_version(
            &rebound_id,
            SEMANTIC_INDEX_ENGINE_COMPONENT,
            SEMANTIC_INDEX_ENGINE_VERSION,
        )?;
    }

    handler.set_current_primary_binding(rebound_id, rebound_path);

    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(
        !needs_indexing,
        "freshness check should use the rebound current-primary snapshot, not the stale loaded workspace"
    );

    Ok(())
}

#[tokio::test]
async fn test_current_primary_index_route_uses_rebound_current_primary_snapshot() -> Result<()> {
    use crate::registry::database::DaemonDatabase;
    use crate::workspace::registry::generate_workspace_id;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let loaded_root = temp_dir.path().join("loaded-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(loaded_root.join("src"))?;
    fs::create_dir_all(rebound_root.join("src"))?;
    fs::write(
        loaded_root.join("src").join("loaded.rs"),
        "fn loaded() {}\n",
    )?;
    fs::write(
        rebound_root.join("src").join("rebound.rs"),
        "fn rebound() {}\n",
    )?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);

    let loaded_path = loaded_root.canonicalize()?;
    let loaded_path_str = loaded_path.to_string_lossy().to_string();
    let loaded_id = generate_workspace_id(&loaded_path_str)?;
    let loaded_ws =
        Arc::new(crate::workspace::JulieWorkspace::initialize(loaded_path.clone()).await?);
    daemon_db.upsert_workspace(&loaded_id, &loaded_path_str, "ready")?;

    let mut handler = JulieServerHandler::new_with_shared_workspace(
        loaded_ws,
        loaded_path,
        Some(Arc::clone(&daemon_db)),
        Some(loaded_id.clone()),
        None,
        None,
    )
    .await?;
    // In-process leader replaces the deleted WorkspacePool as the shared-root
    // source: pin it so the rebound current primary keeps the shared
    // `indexes_dir` anchor instead of falling back to project-local `.julie`.
    handler.in_process_index_root = Some(indexes_dir.join(&loaded_id));

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;
    handler.set_current_primary_binding(rebound_id.clone(), rebound_path.clone());

    let route = IndexRoute::for_current_primary(&handler).await?;

    assert!(route.is_primary);
    assert_eq!(route.workspace_id, rebound_id);
    assert_eq!(route.workspace_root, rebound_path);
    // The in-process leader pins `in_process_index_root` to the shared
    // `indexes_dir`, so the rebound current primary keeps shared-root storage
    // (the deleted WorkspacePool used to carry this anchor).
    assert_eq!(
        route.db_path,
        indexes_dir
            .join(&route.workspace_id)
            .join("db")
            .join("symbols.db"),
        "in-process leader routes rebound current primary DB under the shared indexes_dir"
    );
    assert_eq!(
        route.tantivy_path,
        indexes_dir.join(&route.workspace_id).join("tantivy"),
        "in-process leader routes rebound current primary tantivy under the shared indexes_dir"
    );

    Ok(())
}

#[tokio::test]
async fn test_current_primary_index_route_reports_binding_unavailable_reason() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;

    let err = IndexRoute::for_current_primary(&handler)
        .await
        .expect_err("route resolution should fail without a bound primary");

    assert_eq!(
        err.reason,
        IndexRouteRepairReason::PrimaryBindingUnavailable
    );
    assert!(
        err.to_string().contains("primary binding"),
        "repair reason should stay visible in the route error: {err}"
    );

    Ok(())
}

// ============================================================================
// Test Helpers
// ============================================================================
