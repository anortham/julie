/// Invariant: while `plan_primary_workspace_repair` is in its read-only
/// planning phase (blocked on the SQLite mutex), the
/// `indexing_runtime` flags `catchup_active`, `watcher_paused`, and
/// `active_operation` MUST remain unset. Those flags only flip on once
/// the body decides to execute a repair plan.
///
/// Test setup: build a workspace that is *genuinely* up to date —
/// symbols indexed AND embeddings stored — so the planner returns
/// `None` and we know any flag changes would have to come from the
/// planning path itself, not from execution. The earlier "set
/// JULIE_SKIP_EMBEDDINGS and expect None" baseline was broken because
/// (a) JULIE_SKIP_EMBEDDINGS is never read in production code, and
/// (b) `spawn_workspace_embedding`'s stdio-mode fallback initialises a
/// real sidecar provider on the test host, leaving a live embedding
/// task in `handler.embedding_tasks` that the Finding-2 guard
/// (correctly) refuses to re-trigger.
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_startup_noop_repair_does_not_mark_catchup_active_while_planning() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    // Inject a deterministic provider so the initial index produces
    // embeddings without depending on a real sidecar being present.
    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await.unwrap();
    wait_for_embedding_tasks_to_finish(&handler).await;
    assert!(
        embedding_count_for_primary(&handler).await > 0,
        "test setup: workspace must have embeddings before planning runs"
    );

    let database = handler.primary_database().await.unwrap();
    let database_guard = database.lock().unwrap();
    let handler_for_thread = handler.clone();
    let repair_thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(run_primary_workspace_repair(&handler_for_thread))
    });

    std::thread::sleep(std::time::Duration::from_millis(50));

    let snapshot = {
        let workspace = handler.get_workspace().await.unwrap().unwrap();
        workspace
            .indexing_runtime
            .read()
            .unwrap()
            .snapshot()
    };
    assert!(
        !snapshot.catchup_active,
        "no-op startup repair must not report catch-up active before a repair plan exists"
    );
    assert!(
        !snapshot.watcher_paused,
        "no-op startup repair must not report watcher pause before a repair plan exists"
    );
    assert!(
        snapshot.active_operation.is_none(),
        "no-op startup repair must not expose an active operation while only checking freshness"
    );

    drop(database_guard);
    let repair = repair_thread.join().unwrap().unwrap();
    assert!(
        repair.is_none(),
        "a workspace with symbols AND embeddings AND no file changes is truly \
         up-to-date — planner must return None; got {:?}",
        repair,
    );
}

#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_startup_semantic_repair_runs_embeddings_after_full_reindex() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() {}\nfn beta() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let result = index_tool.call_tool(&handler).await.unwrap();
    let message = extract_text_from_result(&result);
    assert!(
        message.contains("Workspace indexing complete"),
        "initial index should succeed: {message}"
    );
    wait_for_embedding_tasks_to_finish(&handler).await;
    assert!(
        embedding_count_for_primary(&handler).await > 0,
        "initial index should embed symbols before the semantic drift repair"
    );

    let workspace_id = handler
        .current_workspace_id()
        .expect("test handler should have a workspace id");
    {
        let workspace = handler
            .get_workspace()
            .await
            .unwrap()
            .expect("workspace should be initialized");
        let db = workspace.db.as_ref().expect("workspace db should exist");
        let db_lock = db.lock().unwrap();
        db_lock
            .set_index_engine_version(
                &workspace_id,
                SEMANTIC_INDEX_ENGINE_COMPONENT,
                "stale-startup-test-version",
            )
            .unwrap();
    }

    let plan = run_primary_workspace_repair(&handler)
        .await
        .unwrap()
        .expect("semantic drift should produce a startup repair plan");
    assert!(
        plan.reasons.contains(
            &crate::tools::workspace::indexing::state::IndexingRepairReason::SemanticVersionChanged
        ),
        "startup repair should report semantic-version drift"
    );

    wait_for_embedding_tasks_to_finish(&handler).await;
    assert!(
        embedding_count_for_primary(&handler).await > 0,
        "startup semantic full reindex should catch embeddings back up even though auto-index normally skips them"
    );
}

/// Regression: 2026-05-12 auto-catch-up gap.
///
/// When a workspace was indexed in a previous daemon run BEFORE the
/// embedding sidecar finished bootstrapping (e.g. user installed Python
/// after the first index), the symbols are on disk but no vectors exist.
/// On a fresh session connect with no file changes and no semantic
/// version drift, `plan_primary_workspace_repair` returned `None`
/// ("Index is up-to-date — no indexing needed") and nothing ever
/// scheduled the missing embeddings. The user saw GPU activity only
/// during model warm-up; the dashboard showed 0 vectors indefinitely.
///
/// `run_primary_workspace_repair` MUST detect "symbols present + 0
/// embeddings + provider available" and surface it as a
/// `MissingEmbeddings` repair reason so the consumer triggers
/// `spawn_workspace_embedding` (without forcing a full re-index).
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_startup_repair_schedules_embeddings_when_workspace_has_symbols_but_no_vectors() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() {}\nfn beta() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    // First index with force=true populates symbols AND embeddings.
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await.unwrap();
    wait_for_embedding_tasks_to_finish(&handler).await;
    assert!(
        embedding_count_for_primary(&handler).await > 0,
        "test setup: initial index should produce embeddings"
    );

    // Simulate "indexed before sidecar was ready": clear all embeddings,
    // keep all symbols. This is the exact disk state the user hit after
    // re-launching with Python finally installed.
    {
        let workspace = handler
            .get_workspace()
            .await
            .unwrap()
            .expect("workspace should be initialized");
        let db = workspace.db.as_ref().expect("workspace db should exist");
        let mut db_lock = db.lock().unwrap();
        db_lock
            .clear_all_embeddings()
            .expect("clearing embeddings should succeed");
        assert_eq!(
            db_lock
                .embedding_count()
                .expect("embedding_count should succeed"),
            0,
            "test setup: embeddings must be cleared"
        );
        assert!(
            db_lock
                .count_symbols_for_workspace()
                .expect("symbol count should succeed")
                > 0,
            "test setup: symbols must remain after clearing embeddings"
        );
    }

    let plan = run_primary_workspace_repair(&handler)
        .await
        .unwrap()
        .expect(
            "workspace with symbols but no embeddings should produce a startup repair plan — \
             returning None here means the auto-catch-up gap is still present (regression)",
        );
    assert!(
        plan.reasons.contains(
            &crate::tools::workspace::indexing::state::IndexingRepairReason::MissingEmbeddings
        ),
        "startup repair should report MissingEmbeddings when symbols are present but no \
         embeddings exist; got reasons: {:?}",
        plan.reasons,
    );

    wait_for_embedding_tasks_to_finish(&handler).await;
    assert!(
        embedding_count_for_primary(&handler).await > 0,
        "MissingEmbeddings repair should restore embeddings without requiring a full \
         re-index of symbols"
    );
}

#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_startup_missing_embeddings_only_repair_reconciles_web_edges() {
    use crate::database::ProjectionStatus;
    use crate::search::projection::TANTIVY_PROJECTION_NAME;
    use julie_pipeline::indexing_core::web_edges::WEB_EDGES_PROJECTION_NAME;

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() {}\nfn beta() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await.unwrap();
    wait_for_embedding_tasks_to_finish(&handler).await;

    let workspace_id = handler.require_primary_workspace_identity().unwrap();
    {
        let workspace = handler
            .get_workspace()
            .await
            .unwrap()
            .expect("workspace should be initialized");
        let db = workspace.db.as_ref().expect("workspace db should exist");
        let mut db_lock = db.lock().unwrap();
        let canonical_revision = db_lock
            .get_current_canonical_revision(&workspace_id)
            .unwrap()
            .expect("initial index should publish a canonical revision");

        for projection in [TANTIVY_PROJECTION_NAME, WEB_EDGES_PROJECTION_NAME] {
            let state = db_lock
                .get_projection_state(projection, &workspace_id)
                .unwrap()
                .unwrap_or_else(|| panic!("{projection} projection state should exist"));
            assert_eq!(state.canonical_revision, Some(canonical_revision));
            assert_eq!(state.projected_revision, Some(canonical_revision));
        }

        db_lock
            .upsert_projection_state(
                WEB_EDGES_PROJECTION_NAME,
                &workspace_id,
                ProjectionStatus::Ready,
                Some(canonical_revision),
                Some(canonical_revision - 1),
                None,
            )
            .unwrap();
        db_lock.clear_all_embeddings().unwrap();
        assert_eq!(db_lock.embedding_count().unwrap(), 0);
    }

    let plan = run_primary_workspace_repair(&handler)
        .await
        .unwrap()
        .expect("missing embeddings should require startup repair");
    assert_eq!(
        plan.reasons,
        vec![crate::tools::workspace::indexing::state::IndexingRepairReason::MissingEmbeddings]
    );

    wait_for_embedding_tasks_to_finish(&handler).await;
    let workspace = handler
        .get_workspace()
        .await
        .unwrap()
        .expect("workspace should remain initialized");
    let db = workspace.db.as_ref().expect("workspace db should exist");
    let db_lock = db.lock().unwrap();
    let canonical_revision = db_lock
        .get_current_canonical_revision(&workspace_id)
        .unwrap()
        .expect("canonical revision should remain available");

    for projection in [TANTIVY_PROJECTION_NAME, WEB_EDGES_PROJECTION_NAME] {
        let state = db_lock
            .get_projection_state(projection, &workspace_id)
            .unwrap()
            .unwrap_or_else(|| panic!("{projection} projection state should exist"));
        assert_eq!(state.canonical_revision, Some(canonical_revision));
        assert_eq!(state.projected_revision, Some(canonical_revision));
    }
}

/// Codex finding #2 (high, 2026-05-12 review of cascade-fix branch).
///
/// `run_primary_workspace_repair_body` calls `cancel_primary_embedding_task`
/// unconditionally before the index step. With the new `MissingEmbeddings`
/// repair reason, this opens a self-cancelling race:
///
///   1. Session A connects → workspace has 0 vectors → planner returns
///      MissingEmbeddings → repair starts → spawns embedding task.
///   2. Session B connects before the first batch stores any vector →
///      workspace still has 0 vectors → planner WOULD return
///      MissingEmbeddings again → cancels A's in-flight task → starts
///      its own. Repeat indefinitely with each new session.
///
/// The fix: the planner checks `handler.embedding_tasks` before pushing
/// MissingEmbeddings. If a task is already running for the primary
/// workspace, the repair has nothing new to do — return no plan (or at
/// least no MissingEmbeddings reason).
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_startup_repair_does_not_schedule_missing_embeddings_when_task_already_running() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() {}\nfn beta() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    // Initial index produces symbols + embeddings.
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await.unwrap();
    wait_for_embedding_tasks_to_finish(&handler).await;

    // Clear embeddings to set up the catch-up scenario.
    {
        let workspace = handler
            .get_workspace()
            .await
            .unwrap()
            .expect("workspace should be initialized");
        let db = workspace.db.as_ref().expect("workspace db should exist");
        let mut db_lock = db.lock().unwrap();
        db_lock.clear_all_embeddings().unwrap();
    }

    // Insert a sentinel embedding task for the primary workspace.
    // Represents "a previous repair already started catch-up; this task
    // is mid-flight but has not yet stored its first batch". The planner
    // MUST see this and skip MissingEmbeddings to avoid cancel-restart
    // cycling.
    let workspace_id = handler
        .current_workspace_id()
        .expect("test handler should have a workspace id");
    let sentinel_cancel = Arc::new(AtomicBool::new(false));
    let sentinel_cancel_for_task = Arc::clone(&sentinel_cancel);
    let sentinel_handle = tokio::spawn(async move {
        // Hold the slot until cancelled. Mirrors the real embedding
        // pipeline which would be running its blocking inference.
        while !sentinel_cancel_for_task.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    {
        let mut tasks = handler.embedding_tasks.lock().await;
        tasks.insert(workspace_id.clone(), (sentinel_cancel, sentinel_handle));
    }

    let repair = run_primary_workspace_repair(&handler).await.unwrap();

    // Inspect the result first, only then tear the sentinel down. If
    // we tore down before asserting, a cancellation race could mask the
    // outcome.
    let still_running = {
        let tasks = handler.embedding_tasks.lock().await;
        tasks.contains_key(&workspace_id)
    };

    if let Some(plan) = &repair {
        assert!(
            !plan.reasons.contains(
                &crate::tools::workspace::indexing::state::IndexingRepairReason::MissingEmbeddings
            ),
            "planner must NOT push MissingEmbeddings while an embedding task is \
             already running — would cancel and restart that task in a loop. \
             Got plan reasons: {:?}",
            plan.reasons,
        );
    }
    assert!(
        still_running,
        "the sentinel embedding task must still be in `handler.embedding_tasks` \
         after planning — `cancel_primary_embedding_task` must not have run \
         (or, equivalently, planner returned None and the body never ran)"
    );

    // Clean up the sentinel.
    if let Some((flag, handle)) = handler.embedding_tasks.lock().await.remove(&workspace_id) {
        flag.store(true, Ordering::Release);
        let _ = handle.await;
    }
}
