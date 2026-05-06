#[tokio::test]
async fn test_workspace_initialization() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }
    let temp_dir = TempDir::new().unwrap();
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();

    // Check that .julie directory was created
    assert!(workspace.julie_dir.exists());

    // Check that all required root subdirectories exist
    // Note: Per-workspace directories (indexes/{workspace_id}/) are created on-demand during indexing
    assert!(
        workspace.julie_dir.join("indexes").exists(),
        "indexes/ root directory should exist"
    );
    // Note: models/ directory was removed in v2.0 (embeddings replaced by Tantivy)
    assert!(workspace.julie_dir.join("cache").exists());
    assert!(workspace.julie_dir.join("logs").exists());
    assert!(workspace.julie_dir.join("config").exists());

    // Check that config file was created
    assert!(workspace.julie_dir.join("config/julie.toml").exists());

    // Check that .gitignore was created to prevent accidental commits
    assert!(
        workspace.julie_dir.join(".gitignore").exists(),
        ".gitignore should be created in .julie directory"
    );
}

#[tokio::test]
async fn test_workspace_index_records_parse_diagnostics_for_recovered_file() -> anyhow::Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("main.rs"),
        "fn recovered() {\n    let value = ;\n}\n",
    )?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let db = handler.primary_database().await?;
    let diagnostics = {
        let db = db.lock().unwrap();
        db.get_file_parse_diagnostics("main.rs")?
    };

    assert!(
        diagnostics.iter().any(
            |diagnostic| diagnostic.kind == crate::extractors::base::ParseDiagnosticKind::Error
        ),
        "workspace indexing should persist parse diagnostics for recovered malformed files: {diagnostics:?}"
    );

    Ok(())
}

#[tokio::test]
async fn test_workspace_detection() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();
    drop(workspace);

    // Clean up any per-workspace directories if they exist
    let indexes_dir = temp_dir.path().join(".julie").join("indexes");
    if indexes_dir.exists() {
        let _ = fs::remove_dir_all(&indexes_dir);
        fs::create_dir_all(&indexes_dir).unwrap();
    }

    // Test detection from same directory
    let detected = JulieWorkspace::detect_and_load(temp_dir.path().to_path_buf())
        .await
        .unwrap();
    assert!(detected.is_some());

    // Test detection from subdirectory
    let subdir = temp_dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    let detected = JulieWorkspace::detect_and_load(subdir).await.unwrap();
    assert!(detected.is_some());
}

#[tokio::test]
async fn test_health_check() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }
    let temp_dir = TempDir::new().unwrap();
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();

    let health = workspace.health_check().unwrap();
    assert!(health.is_healthy());
    assert!(health.structure_valid);
    assert!(health.has_write_permissions);
}

#[tokio::test]
async fn test_health_snapshot_classifies_plane_states_for_local_workspace() {
    use crate::health::{
        DaemonLifecycleState, EmbeddingState, HealthChecker, HealthLevel, ProjectionFreshness,
        ProjectionState, WatcherState,
    };
    use crate::tools::workspace::indexing::state::{
        IndexingOperation, IndexingRepairReason, IndexingStage,
    };

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn plane_snapshot_target() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();

    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_path.to_string_lossy())
            .unwrap();
    let tantivy_dir = handler
        .workspace_tantivy_dir_for(&workspace_id)
        .await
        .unwrap();
    let meta_path = tantivy_dir.join("meta.json");
    if meta_path.exists() {
        fs::remove_file(meta_path).unwrap();
    }

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.search_index = None;
        ws.embedding_provider = None;
        ws.embedding_runtime_status = None;
        let mut indexing = ws.indexing_runtime.write().unwrap();
        indexing.begin_operation(IndexingOperation::Incremental);
        indexing.transition_stage(IndexingStage::Projecting);
        indexing.set_catchup_active(true);
        indexing.set_watcher_paused(true);
        indexing.set_dirty_projection_count(2);
        indexing.record_repair_reason(IndexingRepairReason::ProjectionFailure);
    }

    let snapshot = HealthChecker::system_snapshot(&handler).await.unwrap();

    assert_eq!(snapshot.overall, HealthLevel::Degraded);
    assert_eq!(
        snapshot.control_plane.daemon_state,
        DaemonLifecycleState::Direct
    );
    assert_eq!(snapshot.control_plane.watcher_state, WatcherState::Local);
    assert_eq!(
        snapshot.data_plane.canonical_store.level,
        HealthLevel::Ready
    );
    assert_eq!(
        snapshot.data_plane.search_projection.state,
        ProjectionState::Missing
    );
    assert_eq!(
        snapshot.data_plane.search_projection.freshness,
        ProjectionFreshness::RebuildRequired
    );
    assert_eq!(
        snapshot.data_plane.search_projection.level,
        HealthLevel::Degraded
    );
    assert_eq!(snapshot.data_plane.indexing.level, HealthLevel::Degraded);
    assert_eq!(
        snapshot.data_plane.indexing.active_operation.as_deref(),
        Some("catch_up")
    );
    assert_eq!(
        snapshot.data_plane.indexing.stage.as_deref(),
        Some("projecting")
    );
    assert!(snapshot.data_plane.indexing.catchup_active);
    assert!(snapshot.data_plane.indexing.watcher_paused);
    assert_eq!(snapshot.data_plane.indexing.dirty_projection_count, 2);
    assert!(snapshot.data_plane.indexing.repair_needed);
    assert!(
        snapshot
            .data_plane
            .indexing
            .repair_reasons
            .contains(&"projection_failure".to_string())
    );
    assert_eq!(
        snapshot.runtime_plane.embeddings.state,
        EmbeddingState::NotInitialized
    );
}

#[tokio::test]
async fn test_manage_workspace_health_reports_control_data_and_runtime_planes() {
    use crate::tools::workspace::indexing::state::{
        IndexingOperation, IndexingRepairReason, IndexingStage,
    };

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn plane_report_target() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();

    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_path.to_string_lossy())
            .unwrap();
    let tantivy_dir = handler
        .workspace_tantivy_dir_for(&workspace_id)
        .await
        .unwrap();
    let meta_path = tantivy_dir.join("meta.json");
    if meta_path.exists() {
        fs::remove_file(meta_path).unwrap();
    }

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.search_index = None;
        ws.embedding_provider = None;
        ws.embedding_runtime_status = None;
        let mut indexing = ws.indexing_runtime.write().unwrap();
        indexing.begin_operation(IndexingOperation::Incremental);
        indexing.transition_stage(IndexingStage::Projecting);
        indexing.set_catchup_active(true);
        indexing.set_watcher_paused(true);
        indexing.set_dirty_projection_count(2);
        indexing.record_repair_reason(IndexingRepairReason::ProjectionFailure);
    }

    let result = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    }
    .call_tool(&handler)
    .await
    .unwrap();
    let health = extract_text_from_result(&result);

    assert!(health.contains("Control Plane"), "{health}");
    assert!(health.contains("Data Plane"), "{health}");
    assert!(health.contains("Runtime Plane"), "{health}");
    assert!(health.contains("Daemon Status: DIRECT"), "{health}");
    assert!(health.contains("Watcher Status: LOCAL"), "{health}");
    assert!(health.contains("Projection Status: MISSING"), "{health}");
    assert!(
        health.contains("Projection Freshness: REBUILD REQUIRED"),
        "{health}"
    );
    assert!(
        health.contains("Projection Repair Needed: true"),
        "{health}"
    );
    assert!(health.contains("Indexing Operation: CATCH_UP"), "{health}");
    assert!(health.contains("Indexing Stage: PROJECTING"), "{health}");
    assert!(health.contains("Catch-Up Active: true"), "{health}");
    assert!(health.contains("Watcher Paused: true"), "{health}");
    assert!(health.contains("Dirty Projection Entries: 2"), "{health}");
    assert!(
        health.contains("Repair Reasons: projection_failure"),
        "{health}"
    );
    assert!(
        health.contains("Embedding Status: NOT INITIALIZED"),
        "{health}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_surfaces_embedding_runtime_status() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
        ws.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Sidecar,
            accelerated: false,
            degraded_reason: Some("CPU only: no GPU detected in sidecar runtime".to_string()),
        });
    }

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let health = extract_text_from_result(&result);
    let health_lower = health.to_ascii_lowercase();

    assert!(
        health.contains("Embedding Runtime"),
        "health output should include embedding runtime section: {health}"
    );
    assert!(
        health.contains("Embedding Status: DEGRADED"),
        "health output should mark degraded runtime as DEGRADED: {health}"
    );
    assert!(
        health.contains("runtime: pytorch-sidecar") || health.contains("Runtime: pytorch-sidecar"),
        "health output should include sidecar runtime identity: {health}"
    );
    assert!(
        health.contains("backend: sidecar") || health.contains("Backend: sidecar"),
        "health output should include resolved backend: {health}"
    );
    assert!(
        health.contains("device: cpu") || health.contains("Device: cpu"),
        "health output should include runtime device: {health}"
    );
    assert!(
        health.contains("accelerated: false") || health.contains("Accelerated: false"),
        "health output should include acceleration flag: {health}"
    );
    assert!(
        health_lower.contains("degraded") && health.contains("CPU only"),
        "health output should include degraded reason: {health}"
    );
    assert!(
        health.contains("Query Fallback: semantic"),
        "health output should describe the query fallback mode when embeddings are available: {health}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_reports_unavailable_when_provider_missing() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = None;
        ws.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Sidecar,
            accelerated: false,
            degraded_reason: Some("provider init failed".to_string()),
        });
    }

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let health = extract_text_from_result(&result);

    assert!(
        health.contains("Embedding Status: UNAVAILABLE"),
        "health output should report missing provider as UNAVAILABLE: {health}"
    );
    assert!(
        health.contains("runtime: unavailable") || health.contains("Runtime: unavailable"),
        "health output should include standardized runtime field when provider is missing: {health}"
    );
    assert!(
        health.contains("Degraded: provider init failed"),
        "health output should include standardized degraded field when provider is missing: {health}"
    );
    assert!(
        health.contains("Query Fallback: keyword-only"),
        "health output should describe keyword-only fallback when embeddings are unavailable: {health}"
    );
    assert!(
        health.contains("provider") || health.contains("Provider"),
        "health output should explain provider is missing: {health}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_reports_not_initialized_when_runtime_status_missing() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = None;
        ws.embedding_runtime_status = None;
    }

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let health = extract_text_from_result(&result);

    assert!(health.contains("Embedding Runtime"), "{health}");
    assert!(
        health.contains("Embedding Status: NOT INITIALIZED"),
        "{health}"
    );
    assert!(health.contains("Runtime: unavailable"), "{health}");
    assert!(health.contains("Backend: unresolved"), "{health}");
    assert!(health.contains("Device: unavailable"), "{health}");
    assert!(health.contains("Accelerated: false"), "{health}");
    assert!(health.contains("Degraded: none"), "{health}");
    assert!(health.contains("Query Fallback: keyword-only"), "{health}");
}

#[tokio::test]
async fn test_manage_workspace_health_reports_initialized_when_not_degraded() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
        ws.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Sidecar,
            accelerated: false,
            degraded_reason: None,
        });
    }

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let health = extract_text_from_result(&result);

    assert!(health.contains("Embedding Runtime"), "{health}");
    assert!(health.contains("Embedding Status: INITIALIZED"), "{health}");
    assert!(health.contains("Runtime: pytorch-sidecar"), "{health}");
    assert!(health.contains("Backend: sidecar"), "{health}");
    assert!(health.contains("Device: cpu"), "{health}");
    assert!(health.contains("Accelerated: false"), "{health}");
    assert!(health.contains("Degraded: none"), "{health}");
    assert!(health.contains("Query Fallback: semantic"), "{health}");
}
