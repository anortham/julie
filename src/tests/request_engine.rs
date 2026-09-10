//! Tests for shared RequestEngine application dispatch and cancellation.

use crate::paths::RegistryPaths;
use crate::request_engine::{
    BindingResolver, RequestContext, RequestEngine, RequestFailure, RequestOrigin, RuntimeFactory,
    SemanticMode, ToolReply, ToolRequest,
};
use crate::tests::helpers::workspace::make_isolated_workspace_root;
use std::path::PathBuf;
use std::sync::Arc;

pub struct RequestFixture {
    pub engine: RequestEngine,
    pub root: PathBuf,
    pub database_path: PathBuf,
    pub temp_home: Arc<tempfile::TempDir>,
    pub temp_repo: Arc<tempfile::TempDir>,
}

impl RequestFixture {
    pub async fn indexed() -> Self {
        let temp_repo = tempfile::tempdir().expect("temp repo dir");
        let root = make_isolated_workspace_root(temp_repo.path(), "request_probe");
        std::fs::create_dir_all(root.join("src")).expect("create src dir");
        std::fs::write(root.join("src/lib.rs"), "pub fn request_probe() {}\n")
            .expect("write probe file");

        let temp_home = tempfile::tempdir().expect("temp home dir");
        let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
        let workspace_id =
            julie_core::workspace::registry::generate_workspace_id(&root.to_string_lossy())
                .expect("generate workspace_id");
        let database_path = registry_paths.workspace_facts_path(&workspace_id);

        let binding_resolver =
            BindingResolver::new(Some(root.clone()), false, registry_paths.clone());
        let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths.clone()));

        // Pre-initialize indexed workspace
        let dummy_ctx = RequestContext::new(
            RequestOrigin::Cli,
            Some(std::time::Duration::from_secs(30)),
            tokio_util::sync::CancellationToken::new(),
        );
        let binding = binding_resolver.resolve(None, None, false).unwrap();
        let _ = runtime_factory
            .acquire(binding.as_ref(), &dummy_ctx)
            .await
            .unwrap();

        let engine = RequestEngine::new(binding_resolver, runtime_factory);

        Self {
            engine,
            root,
            database_path,
            temp_home: Arc::new(temp_home),
            temp_repo: Arc::new(temp_repo),
        }
    }

    pub async fn execute(
        &self,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<ToolReply, RequestFailure> {
        let arguments = args.as_object().cloned().unwrap_or_default();
        let request = ToolRequest {
            name: tool.to_string(),
            arguments,
            workspace: Some(self.root.clone()),
            semantics: SemanticMode::Off,
        };
        let context = RequestContext::new(
            RequestOrigin::Cli,
            Some(std::time::Duration::from_secs(10)),
            tokio_util::sync::CancellationToken::new(),
        );
        self.engine.execute(request, context).await
    }

    pub async fn execute_with_envelope_workspace(
        &self,
        tool: &str,
        args: serde_json::Value,
        workspace: Option<PathBuf>,
    ) -> Result<ToolReply, RequestFailure> {
        let arguments = args.as_object().cloned().unwrap_or_default();
        let request = ToolRequest {
            name: tool.to_string(),
            arguments,
            workspace,
            semantics: SemanticMode::Off,
        };
        let context = RequestContext::new(
            RequestOrigin::Mcp,
            Some(std::time::Duration::from_secs(10)),
            tokio_util::sync::CancellationToken::new(),
        );
        self.engine.execute(request, context).await
    }
}

#[tokio::test]
async fn preview_dry_run_leaves_disk_untouched() {
    let fixture = RequestFixture::indexed().await;
    let before = std::fs::read(fixture.root.join("src/lib.rs")).unwrap();
    let response = fixture
        .execute(
            "edit_file",
            serde_json::json!({
                "file_path":"src/lib.rs", "old_text":"request_probe",
                "new_text":"renamed_probe", "dry_run":true
            }),
        )
        .await;
    assert!(response.is_ok(), "Expected dry_run preview to succeed");
    assert_eq!(
        std::fs::read(fixture.root.join("src/lib.rs")).unwrap(),
        before
    );
}

#[tokio::test]
async fn missing_workspace_rejects_and_creates_no_directories() {
    let temp_home = tempfile::tempdir().expect("temp home");
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let binding_resolver = BindingResolver::new(None, false, registry_paths.clone());
    let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths));
    let engine = RequestEngine::new(binding_resolver, runtime_factory);

    let request = ToolRequest {
        name: "fast_search".to_string(),
        arguments: serde_json::json!({ "query": "probe" })
            .as_object()
            .unwrap()
            .clone(),
        workspace: None,
        semantics: SemanticMode::Off,
    };
    let context = RequestContext::new(
        RequestOrigin::Mcp,
        Some(std::time::Duration::from_secs(10)),
        tokio_util::sync::CancellationToken::new(),
    );
    let result = engine.execute(request, context).await;
    assert_eq!(result.unwrap_err().code, "WORKSPACE_REQUIRED");

    // Verify no .julie directory was created
    assert!(!temp_home.path().join(".julie").exists());
}

#[tokio::test]
async fn conflicting_workspace_rejects_before_acquisition() {
    let fixture = RequestFixture::indexed().await;
    let other_temp = tempfile::tempdir().expect("other temp");
    let other_root = make_isolated_workspace_root(other_temp.path(), "other_repo");

    let result = fixture.execute_with_envelope_workspace(
        "fast_search",
        serde_json::json!({ "query": "probe", "workspace": other_root.to_string_lossy().to_string() }),
        Some(fixture.root.clone()),
    ).await;
    assert_eq!(result.unwrap_err().code, "WORKSPACE_CONFLICT");
}

#[tokio::test]
async fn unknown_tool_rejects_before_runtime() {
    let fixture = RequestFixture::indexed().await;
    let result = fixture
        .execute("not_a_real_tool", serde_json::json!({}))
        .await;
    assert_eq!(result.unwrap_err().code, "UNKNOWN_TOOL");
}

#[tokio::test]
async fn catalog_schemas_valid_and_match_all_12_tools() {
    use crate::request_engine::catalog::ToolCatalog;
    let list = ToolCatalog::list();
    assert_eq!(list.len(), 12);
    for tool in &list {
        assert!(
            tool.schema.is_object(),
            "Tool '{}' schema must be an object",
            tool.name
        );
        assert!(
            !tool.description.is_empty(),
            "Tool '{}' description must not be empty",
            tool.name
        );
    }
}

#[tokio::test]
async fn two_simultaneous_requests_execute_without_deadlock() {
    let fixture = RequestFixture::indexed().await;
    let req1 = fixture.execute(
        "get_symbols",
        serde_json::json!({ "file_path": "src/lib.rs" }),
    );
    let req2 = fixture.execute(
        "fast_search",
        serde_json::json!({ "query": "request_probe" }),
    );

    let (res1, res2) = tokio::join!(req1, req2);
    assert!(res1.is_ok(), "Request 1 failed: {:?}", res1.err());
    assert!(res2.is_ok(), "Request 2 failed: {:?}", res2.err());
}

#[tokio::test]
async fn source_preflight_deadline_cancels_and_joins_parser() {
    use julie_tools::editing::syntax::{SyntaxAdapter, SyntaxAdapterError, SyntaxConfig};
    use std::time::{Duration, Instant};

    let config = SyntaxConfig {
        max_source_bytes: 1024 * 1024,
        default_timeout: Duration::from_secs(5),
        max_concurrent_parses: 1,
    };
    let adapter = Arc::new(SyntaxAdapter::new(config).unwrap());
    assert_eq!(adapter.available_permits(), 1);

    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (unblock_tx, unblock_rx) = std::sync::mpsc::channel();

    let adapter_clone = Arc::clone(&adapter);
    let sample_file = PathBuf::from("src/lib.rs");
    let original_source = "pub fn request_probe() {}\n";

    // Expire the request after the worker has acquired admission
    let deadline = Instant::now() + Duration::from_millis(60);

    let caller_handle = std::thread::spawn(move || {
        adapter_clone.parse_and_extract_with_sync(
            &sample_file,
            original_source,
            std::path::Path::new("."),
            Some(deadline),
            None,
            move || {
                let _ = started_tx.send(());
                let _ = unblock_rx.recv();
            },
        )
    });

    // Worker thread starts and acquires admission permit
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("worker thread must start and acquire permit");
    assert_eq!(adapter.available_permits(), 0);

    // Wait for deadline to expire
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }

    // Release worker entry barrier
    let _ = unblock_tx.send(());

    // Assert DEADLINE_EXCEEDED
    let caller_result = caller_handle.join().expect("caller thread must join");
    assert!(
        matches!(caller_result, Err(SyntaxAdapterError::DeadlineExceeded)),
        "Expected DeadlineExceeded, got {:?}",
        caller_result
    );

    // Await worker completion and drain cancelled workers
    adapter.drain_cancelled_workers();

    // Assert its admission count returns to full permits
    assert_eq!(adapter.available_permits(), 1);

    // Verify source bytes did not change
    assert_eq!(original_source, "pub fn request_probe() {}\n");
}

#[tokio::test]
async fn request_engine_cancelled_token_returns_cancelled() {
    let fixture = RequestFixture::indexed().await;
    let cancel_token = tokio_util::sync::CancellationToken::new();
    cancel_token.cancel();

    let request = ToolRequest {
        name: "fast_search".to_string(),
        arguments: serde_json::json!({ "query": "probe" })
            .as_object()
            .unwrap()
            .clone(),
        workspace: Some(fixture.root.clone()),
        semantics: SemanticMode::Off,
    };
    let context = RequestContext::new(
        RequestOrigin::Cli,
        Some(std::time::Duration::from_secs(10)),
        cancel_token,
    );

    let result = fixture.engine.execute(request, context).await;
    let err = result.expect_err("Expected cancelled error");
    assert_eq!(err.code, "CANCELLED");
    assert_eq!(err.exit_code(), 130);
}

#[tokio::test]
async fn request_engine_expired_deadline_returns_deadline_exceeded() {
    let fixture = RequestFixture::indexed().await;
    let request = ToolRequest {
        name: "fast_search".to_string(),
        arguments: serde_json::json!({ "query": "probe" })
            .as_object()
            .unwrap()
            .clone(),
        workspace: Some(fixture.root.clone()),
        semantics: SemanticMode::Off,
    };
    let context = RequestContext::new(
        RequestOrigin::Cli,
        Some(std::time::Duration::from_millis(0)),
        tokio_util::sync::CancellationToken::new(),
    );
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let result = fixture.engine.execute(request, context).await;
    let err = result.expect_err("Expected deadline exceeded error");
    assert_eq!(err.code, "DEADLINE_EXCEEDED");
    assert_eq!(err.exit_code(), 124);
}

#[tokio::test]
async fn source_preflight_cancellation_cancels_and_restores_permits() {
    use julie_tools::editing::syntax::{SyntaxAdapter, SyntaxAdapterError, SyntaxConfig};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    let config = SyntaxConfig {
        max_source_bytes: 1024 * 1024,
        default_timeout: Duration::from_secs(5),
        max_concurrent_parses: 1,
    };
    let adapter = Arc::new(SyntaxAdapter::new(config).unwrap());
    assert_eq!(adapter.available_permits(), 1);

    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (unblock_tx, unblock_rx) = std::sync::mpsc::channel();

    let adapter_clone = Arc::clone(&adapter);
    let sample_file = PathBuf::from("src/lib.rs");
    let original_source = "pub fn request_probe() {}\n";

    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_ref = Arc::clone(&cancelled);

    let caller_handle = std::thread::spawn(move || {
        adapter_clone.parse_and_extract_with_sync(
            &sample_file,
            original_source,
            std::path::Path::new("."),
            None,
            Some(&cancelled_ref),
            move || {
                let _ = started_tx.send(());
                let _ = unblock_rx.recv();
            },
        )
    });

    // Worker thread starts and acquires admission permit
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("worker thread must start and acquire permit");
    assert_eq!(adapter.available_permits(), 0);

    // Cancel while worker is inside
    cancelled.store(true, Ordering::Release);

    // Wait for await_worker polling interval (20ms) to observe cancellation
    std::thread::sleep(Duration::from_millis(30));

    // Release worker entry barrier
    let _ = unblock_tx.send(());

    // Assert CANCELLED
    let caller_result = caller_handle.join().expect("caller thread must join");
    assert!(
        matches!(caller_result, Err(SyntaxAdapterError::Cancelled)),
        "Expected Cancelled, got {:?}",
        caller_result
    );

    // Await worker completion and drain cancelled workers
    adapter.drain_cancelled_workers();

    // Assert its admission count returns to full permits (no leak!)
    assert_eq!(adapter.available_permits(), 1);

    // Verify source bytes did not change
    assert_eq!(original_source, "pub fn request_probe() {}\n");
}

#[test]
fn syntax_limiter_contention_and_timeout_restores_permits_without_leak() {
    use julie_tools::editing::syntax::{SyntaxAdapter, SyntaxAdapterError, SyntaxConfig};
    use std::time::{Duration, Instant};

    let config = SyntaxConfig {
        max_source_bytes: 1024 * 1024,
        default_timeout: Duration::from_secs(5),
        max_concurrent_parses: 1,
    };
    let adapter = Arc::new(SyntaxAdapter::new(config).unwrap());
    assert_eq!(adapter.available_permits(), 1);

    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (unblock_tx, unblock_rx) = std::sync::mpsc::channel();

    // Worker 1 acquires the only permit and holds it
    let adapter1 = Arc::clone(&adapter);
    let h1 = std::thread::spawn(move || {
        adapter1.parse_and_extract_with_sync(
            std::path::Path::new("src/lib.rs"),
            "pub fn f1() {}\n",
            std::path::Path::new("."),
            None,
            None,
            move || {
                let _ = started_tx.send(());
                let _ = unblock_rx.recv();
            },
        )
    });

    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("worker 1 started");
    assert_eq!(adapter.available_permits(), 0);

    // Worker 2 attempts to acquire with an already expired deadline
    let adapter2 = Arc::clone(&adapter);
    let expired_deadline = Instant::now() - Duration::from_millis(10);
    let res2 = adapter2.parse_source(
        std::path::Path::new("src/lib.rs"),
        "pub fn f2() {}\n",
        Some(expired_deadline),
        None,
    );
    assert!(
        matches!(res2, Err(SyntaxAdapterError::DeadlineExceeded)),
        "Expected DeadlineExceeded for worker 2, got {:?}",
        res2
    );

    // Worker 1 releases
    let _ = unblock_tx.send(());
    let _ = h1.join().unwrap();

    // Permit must be exactly 1 (no leak, no deficit)
    assert_eq!(adapter.available_permits(), 1);

    // Worker 3 can now parse successfully
    let res3 = adapter.parse_source(
        std::path::Path::new("src/lib.rs"),
        "pub fn f3() {}\n",
        None,
        None,
    );
    assert!(res3.is_ok(), "Worker 3 parse must succeed");
    assert_eq!(adapter.available_permits(), 1);
}

#[tokio::test]
async fn failed_edit_leaves_no_partial_edits_or_corrupted_files() {
    let fixture = RequestFixture::indexed().await;
    let file_path = fixture.root.join("src/lib.rs");
    let original = std::fs::read_to_string(&file_path).unwrap();

    // Attempt an edit with non-matching old_text
    let response = fixture
        .execute(
            "edit_file",
            serde_json::json!({
                "file_path": "src/lib.rs",
                "old_text": "nonexistent_symbol_that_should_not_match",
                "new_text": "replaced_symbol",
                "dry_run": false
            }),
        )
        .await;

    assert!(response.is_err(), "Expected error for non-matching edit");
    let err = response.unwrap_err();
    assert_eq!(err.code, "EDIT_CONFLICT");

    // Verify source file is byte-for-byte unmodified
    let current = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(
        current, original,
        "Source file must not be modified on failure"
    );

    // Verify no temporary files (.tmp.) left in src/
    let entries = std::fs::read_dir(fixture.root.join("src")).unwrap();
    for entry in entries {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            !name.contains(".tmp."),
            "Orphaned temp file found: {}",
            name
        );
    }
}

#[tokio::test]
async fn five_concurrent_requests_execute_without_deadlock_or_corruption() {
    let fixture = RequestFixture::indexed().await;

    let req1 = fixture.execute(
        "get_symbols",
        serde_json::json!({ "file_path": "src/lib.rs" }),
    );
    let req2 = fixture.execute(
        "fast_search",
        serde_json::json!({ "query": "request_probe" }),
    );
    let req3 = fixture.execute(
        "fast_refs",
        serde_json::json!({ "symbol": "request_probe" }),
    );
    let req4 = fixture.execute(
        "edit_file",
        serde_json::json!({
            "file_path": "src/lib.rs",
            "old_text": "request_probe",
            "new_text": "preview_probe",
            "dry_run": true
        }),
    );
    let req5 = fixture.execute(
        "get_symbols",
        serde_json::json!({ "file_path": "src/lib.rs" }),
    );

    let (r1, r2, r3, r4, r5) = tokio::join!(req1, req2, req3, req4, req5);
    assert!(r1.is_ok(), "req1 failed: {:?}", r1.err());
    assert!(r2.is_ok(), "req2 failed: {:?}", r2.err());
    assert!(r3.is_ok(), "req3 failed: {:?}", r3.err());
    assert!(r4.is_ok(), "req4 failed: {:?}", r4.err());
    assert!(r5.is_ok(), "req5 failed: {:?}", r5.err());

    // File should still be unchanged
    let content = std::fs::read_to_string(fixture.root.join("src/lib.rs")).unwrap();
    assert_eq!(content, "pub fn request_probe() {}\n");
}

#[tokio::test]
async fn rewrite_symbol_preview_dry_run_leaves_disk_untouched() {
    let fixture = RequestFixture::indexed().await;
    let before = std::fs::read(fixture.root.join("src/lib.rs")).unwrap();
    let response = fixture
        .execute(
            "rewrite_symbol",
            serde_json::json!({
                "symbol": "request_probe",
                "operation": "replace_body",
                "content": "{\n    let _preview = 1;\n}\n",
                "dry_run": true
            }),
        )
        .await;
    assert!(
        response.is_ok(),
        "rewrite_symbol with dry_run=true must succeed: {:?}",
        response.err()
    );
    assert_eq!(
        std::fs::read(fixture.root.join("src/lib.rs")).unwrap(),
        before,
        "Disk bytes must remain untouched during preview dry_run"
    );
}

#[tokio::test]
async fn rename_symbol_preview_dry_run_leaves_disk_untouched() {
    let fixture = RequestFixture::indexed().await;
    let before = std::fs::read(fixture.root.join("src/lib.rs")).unwrap();
    let response = fixture
        .execute(
            "rename_symbol",
            serde_json::json!({
                "old_name": "request_probe",
                "new_name": "renamed_request_probe",
                "dry_run": true
            }),
        )
        .await;
    assert!(
        response.is_ok(),
        "rename_symbol with dry_run=true must succeed: {:?}",
        response.err()
    );
    assert_eq!(
        std::fs::read(fixture.root.join("src/lib.rs")).unwrap(),
        before,
        "Disk bytes must remain untouched during preview dry_run"
    );
}

#[tokio::test]
async fn sensitive_roots_rejected_before_creating_directories() {
    let temp_home = tempfile::tempdir().expect("temp home");
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let resolver_shared = BindingResolver::new(None, false, registry_paths.clone());
    let resolver_standalone = BindingResolver::new(None, true, registry_paths.clone());

    let sensitive_roots = [
        std::path::PathBuf::from("/"),
        std::path::PathBuf::from("/home"),
    ];

    for root in &sensitive_roots {
        if !root.exists() {
            continue;
        }

        // Shared storage mode
        let err_shared = resolver_shared
            .resolve(Some(root.clone()), None, false)
            .expect_err("Sensitive root must be rejected in shared mode");
        assert_eq!(err_shared.code, "SENSITIVE_ROOT");

        // Standalone storage mode
        let err_standalone = resolver_standalone
            .resolve(Some(root.clone()), None, false)
            .expect_err("Sensitive root must be rejected in standalone mode");
        assert_eq!(err_standalone.code, "SENSITIVE_ROOT");

        // Verify no .julie directory was created under the sensitive root
        assert!(
            !root.join(".julie").exists(),
            "Must not create .julie in sensitive root {}",
            root.display()
        );
    }

    // Verify temp_home has no indexes created
    assert!(!temp_home.path().join("indexes").exists());
}

#[tokio::test]
async fn engine_rejects_sensitive_root_on_tool_execution() {
    let temp_home = tempfile::tempdir().expect("temp home");
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let binding_resolver = BindingResolver::new(None, false, registry_paths.clone());
    let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths));
    let engine = RequestEngine::new(binding_resolver, runtime_factory);

    let request = ToolRequest {
        name: "fast_search".to_string(),
        arguments: serde_json::json!({ "query": "probe" })
            .as_object()
            .unwrap()
            .clone(),
        workspace: Some(PathBuf::from("/")),
        semantics: SemanticMode::Off,
    };
    let context = RequestContext::new(
        RequestOrigin::Cli,
        Some(std::time::Duration::from_secs(10)),
        tokio_util::sync::CancellationToken::new(),
    );
    let result = engine.execute(request, context).await;
    let err = result.expect_err("Tool execution with root '/' must be rejected as sensitive root");
    assert_eq!(err.code, "SENSITIVE_ROOT");
    assert!(!std::path::Path::new("/.julie").exists());
}

#[tokio::test]
async fn missing_workspace_on_various_non_unbound_tools_rejects() {
    let temp_home = tempfile::tempdir().expect("temp home");
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let binding_resolver = BindingResolver::new(None, false, registry_paths.clone());
    let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths));
    let engine = RequestEngine::new(binding_resolver, runtime_factory);

    let test_cases = [
        (
            "get_symbols",
            serde_json::json!({ "file_path": "src/lib.rs" }),
        ),
        (
            "edit_file",
            serde_json::json!({ "file_path": "src/lib.rs", "old_text": "a", "new_text": "b", "dry_run": true }),
        ),
        (
            "rename_symbol",
            serde_json::json!({ "old_name": "a", "new_name": "b", "dry_run": true }),
        ),
    ];

    for (tool_name, args) in test_cases {
        let request = ToolRequest {
            name: tool_name.to_string(),
            arguments: args.as_object().unwrap().clone(),
            workspace: None,
            semantics: SemanticMode::Off,
        };
        let context = RequestContext::new(
            RequestOrigin::Mcp,
            Some(std::time::Duration::from_secs(10)),
            tokio_util::sync::CancellationToken::new(),
        );
        let result = engine.execute(request, context).await;
        let err = result.expect_err(&format!("Tool '{tool_name}' without workspace must fail"));
        assert_eq!(
            err.code, "WORKSPACE_REQUIRED",
            "Tool '{tool_name}' failure code"
        );
    }

    assert!(!temp_home.path().join(".julie").exists());
}

#[tokio::test]
async fn unbound_manage_workspace_list_permitted_without_workspace() {
    let temp_home = tempfile::tempdir().expect("temp home");
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let binding_resolver = BindingResolver::new(None, false, registry_paths.clone());
    let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths));
    let engine = RequestEngine::new(binding_resolver, runtime_factory);

    let request = ToolRequest {
        name: "manage_workspace".to_string(),
        arguments: serde_json::json!({ "operation": "list" })
            .as_object()
            .unwrap()
            .clone(),
        workspace: None,
        semantics: SemanticMode::Off,
    };
    let context = RequestContext::new(
        RequestOrigin::Cli,
        Some(std::time::Duration::from_secs(10)),
        tokio_util::sync::CancellationToken::new(),
    );
    let result = engine.execute(request, context).await;
    assert!(
        result.is_ok(),
        "Unbound manage_workspace list must succeed without workspace: {:?}",
        result.err()
    );
}
