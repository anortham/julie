//! A2.3 — Concurrent MCP regression test (deadlock detector).
//!
//! Daemon-backed handler with real WorkspacePool + WatcherPool. Drives 8
//! concurrent tool requests against a single indexed workspace (4 reads + 4
//! writes including one real mutation) while a background task continually
//! modifies a file in that workspace, keeping the file watcher's
//! event-processor contended with the mutation_gate. All 8 must complete
//! within a 30s budget and produce non-error `CallToolResult` payloads.
//!
//! **The point is to catch deadlocks.** The connection-pool, mutation_gate,
//! and watcher event-processor are the only things serializing writers; if
//! there's a lock-order bug between them this test wedges and the wrapping
//! `timeout` fires.
//!
//! Strengthened through two codex review passes (8 findings addressed total):
//!   1. **Daemon-backed handler** via `new_with_shared_workspace` so tool
//!      calls actually traverse the real `WorkspaceConnectionPool` and
//!      shared `WatcherPool` — not `new_for_test`'s stdio-only fallback.
//!   2. **Watcher proof-of-life**: write a sentinel file, poll the DB until
//!      its symbol appears, only then start the workload. If the watcher
//!      never indexes the sentinel, the test fails fast.
//!   3. **One real mutation** (`dry_run=false` against `src/disposable.rs`)
//!      so at least one write genuinely crosses the watcher write path.
//!   4. **Non-identical content** in dry-run rewrites/renames so they don't
//!      bail in early-return validation paths.
//!   5. **`tokio::sync::Barrier(8)`** so the 8 tasks release simultaneously
//!      and actually contend for the gate + pool.
//!   6. **Explicit `workspace_id` routing** instead of `"primary"`. The
//!      `"primary"` short-form falls through `handler.primary_database()`
//!      which still uses the legacy `Arc<Mutex<SymbolDatabase>>`, bypassing
//!      the very `WorkspaceConnectionPool` surface this test exists to
//!      cover. Routing by id forces the pooled `get_pooled_database_for_workspace`
//!      path (see `src/tools/search/text_search.rs:90-96` vs `:140-150`).
//!   7. **DB post-condition for the real mutation**: poll for
//!      `disposable_marker_v2` in the symbol DB after the workload completes.
//!      `EditFileTool` commits via `EditingTransaction` which does NOT acquire
//!      the mutation gate — only the watcher's event-processor does (see
//!      `src/watcher/runtime.rs`). The DB observing the new symbol proves the
//!      watcher actually picked up the edit AND crossed the gate.
//!   8. **`CallToolResult.is_error` check**: a tool returning
//!      `Ok(CallToolResult::error(...))` (e.g. `manage_workspace index` on a
//!      lock-order regression) would otherwise count as success. Each task
//!      now ships its result back and the completion loop rejects error
//!      payloads explicitly.
//!
//! Plan reference: `docs/plans/2026-05-16-daemon-split-and-search-reranker-plan.md`
//! Task A2.3 (escalation-tier owned).

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use anyhow::{Result, anyhow};
    use tempfile::TempDir;
    use tokio::sync::{Barrier, Notify};
    use tokio::task::JoinSet;
    use tokio::time::timeout;

    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::watcher_pool::WatcherPool;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::handler::JulieServerHandler;
    use crate::mcp_compat::CallToolResult;
    use crate::tools::deep_dive::{DeepDiveDepth, DeepDiveTool};
    use crate::tools::editing::edit_file::{EditFileTool, EditOccurrence};
    use crate::tools::navigation::FastRefsTool;
    use crate::tools::refactoring::RenameSymbolTool;
    use crate::tools::search::FastSearchTool;
    use crate::tools::{GetSymbolsTool, ManageWorkspaceTool};
    use crate::workspace::registry::generate_workspace_id;

    struct ConcurrentFixture {
        _temp_dir: TempDir,
        ws_root: PathBuf,
        workspace_id: String,
        handler: Arc<JulieServerHandler>,
        workspace_pool: Arc<WorkspacePool>,
    }

    /// Poll the legacy DB (via the shared workspace handle in the pool) for a
    /// symbol name until it appears or `timeout_dur` elapses. Used both for
    /// the watcher proof-of-life precondition AND for the post-workload
    /// assertion that the real mutation's watcher event got processed
    /// through the mutation_gate.
    async fn wait_for_symbol_via_pool(
        pool: &Arc<WorkspacePool>,
        workspace_id: &str,
        symbol_name: &str,
        timeout_dur: Duration,
    ) -> Result<()> {
        let start = Instant::now();
        loop {
            let ws = pool
                .get(workspace_id)
                .await
                .ok_or_else(|| anyhow!("workspace `{}` not in pool", workspace_id))?;
            let found = {
                let db_handle = ws
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow!("workspace `{}` has no db", workspace_id))?;
                let guard = db_handle.lock().unwrap();
                guard
                    .find_symbols_by_name(symbol_name)?
                    .into_iter()
                    .any(|s| s.name == symbol_name)
            };
            if found {
                return Ok(());
            }
            if start.elapsed() >= timeout_dur {
                return Err(anyhow!(
                    "watcher never indexed `{}` after {:?}",
                    symbol_name,
                    timeout_dur
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn setup_concurrent_workspace() -> Result<ConcurrentFixture> {
        let temp_dir = tempfile::tempdir()?;
        let ws_root = temp_dir.path().canonicalize()?;
        let src = ws_root.join("src");
        std::fs::create_dir_all(&src)?;

        // Initial workspace contents.
        std::fs::write(
            src.join("lib.rs"),
            "pub mod alpha;\npub mod beta;\npub mod disposable;\n\
             pub fn root_entry() { let _ = 0; }\n",
        )?;
        std::fs::write(
            src.join("alpha.rs"),
            "pub fn alpha_func() { let _ = 1; }\n\
             pub fn alpha_helper() -> i32 { 42 }\n",
        )?;
        std::fs::write(
            src.join("beta.rs"),
            "pub fn beta_func() { let _ = 2; }\n\
             pub struct BetaState { pub count: i32 }\n",
        )?;
        std::fs::write(
            src.join("disposable.rs"),
            "pub fn disposable_marker() { let _ = 7; }\n",
        )?;

        // Daemon-backed setup: DaemonDatabase + WorkspacePool + WatcherPool.
        let indexes_dir = temp_dir.path().join("indexes");
        std::fs::create_dir_all(&indexes_dir)?;
        let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
        let workspace_pool = Arc::new(WorkspacePool::new(
            indexes_dir,
            Some(Arc::clone(&daemon_db)),
        ));
        let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(60)));

        let ws_root_str = ws_root.to_string_lossy().to_string();
        let workspace_id = generate_workspace_id(&ws_root_str)?;
        let shared_ws = timeout(Duration::from_secs(20), workspace_pool
            .get_or_init(&workspace_id, ws_root.clone()))
            .await
            .map_err(|_| anyhow!("setup hung in pool.get_or_init (>20s)"))??;

        // Pass watcher_pool to handler so its session_attachment implicit
        // attach fires the watcher at construction time. An explicit
        // post-index attach didn't reliably start a working watcher on
        // macOS (notify settle latency); the implicit one runs before the
        // initial index, giving the FSEvents stream time to warm up.
        let handler = Arc::new(
            timeout(Duration::from_secs(20), JulieServerHandler::new_with_shared_workspace(
                Arc::clone(&shared_ws),
                ws_root.clone(),
                Some(Arc::clone(&daemon_db)),
                Some(workspace_id.clone()),
                None,
                None,
                None,
                Some(Arc::clone(&watcher_pool)),
                Some(Arc::clone(&workspace_pool)),
            ))
            .await
            .map_err(|_| anyhow!("setup hung in new_with_shared_workspace (>20s)"))??,
        );

        daemon_db.upsert_workspace(&workspace_id, &ws_root_str, "ready")?;

        // Initial index. The watcher (attached implicitly above) is already
        // running and observing the workspace root; the .rs files already
        // exist so FSEvents won't fire create events for them, and the
        // index step does not modify any source file.
        timeout(Duration::from_secs(30), ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(ws_root_str.clone()),
            name: None,
            workspace_id: None,
            force: Some(true),
            detailed: None,
        }
        .call_tool(&handler))
        .await
        .map_err(|_| anyhow!("setup hung in initial manage_workspace index (>30s)"))??;

        Ok(ConcurrentFixture {
            _temp_dir: temp_dir,
            ws_root,
            workspace_id,
            handler,
            workspace_pool,
        })
    }

    /// A2.3 deadlock-detector. 8 concurrent tool requests + a background
    /// watcher driver, all under a 30s wall-clock budget. Routes by explicit
    /// workspace_id (not "primary") so the workload exercises the pooled
    /// connection path that this test exists to guard.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_concurrent_mcp_requests_do_not_wedge() -> Result<()> {
        let fixture = setup_concurrent_workspace().await?;
        let ws_root = fixture.ws_root.clone();
        let handler = Arc::clone(&fixture.handler);
        let workspace_pool = Arc::clone(&fixture.workspace_pool);
        let workspace_id = fixture.workspace_id.clone();
        // Tools route by explicit id, NOT "primary". Primary routing in
        // text_search/get_symbols still uses the legacy Arc<Mutex<DB>>; we
        // want the pooled path on every call.
        let ws_filter = workspace_id.clone();

        // ── Watcher proof-of-life ──
        // Write a sentinel symbol; if the watcher is alive it'll be in DB
        // within 10s. If not, the test surface is meaningless and we fail
        // loudly before the timing-sensitive workload runs.
        let sentinel_path = ws_root.join("src").join("sentinel.rs");
        std::fs::write(
            &sentinel_path,
            "pub fn sentinel_watcher_proof() { let _ = 9; }\n",
        )?;
        wait_for_symbol_via_pool(
            &workspace_pool,
            &workspace_id,
            "sentinel_watcher_proof",
            Duration::from_secs(10),
        )
        .await
        .map_err(|e| {
            anyhow!(
                "concurrent_mcp setup precondition failed: {e}. The file \
                 watcher must observe sentinel.rs before the workload fires; \
                 otherwise we're not exercising the watcher event-processor \
                 path that this test exists to cover."
            )
        })?;

        // ── Background watcher driver ──
        // After proof-of-life, drive alpha.rs at 100ms cadence so the event
        // processor keeps the mutation_gate contended throughout the workload.
        let watcher_stop = Arc::new(Notify::new());
        let watcher_stop_for_task = Arc::clone(&watcher_stop);
        let ws_root_for_task = ws_root.clone();
        let watcher_task = tokio::spawn(async move {
            let alpha_path = ws_root_for_task.join("src").join("alpha.rs");
            let mut tick: u32 = 0;
            loop {
                tokio::select! {
                    _ = watcher_stop_for_task.notified() => break,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        // Identity-shaped rewrite (same symbols, different
                        // body) so reads always find something.
                        let body = format!(
                            "pub fn alpha_func() {{ let _ = {tick}u32; }}\n\
                             pub fn alpha_helper() -> i32 {{ {tick}i32 }}\n"
                        );
                        let _ = std::fs::write(&alpha_path, body);
                        tick = tick.wrapping_add(1);
                    }
                }
            }
        });

        // ── Barrier ensures all 8 tasks release simultaneously ──
        let barrier = Arc::new(Barrier::new(8));
        // Each task ships back its label + the CallToolResult so we can reject
        // is_error payloads in the completion loop (codex finding #3: an
        // Ok(CallToolResult::error(...)) would otherwise silently count as
        // success).
        let mut set: JoinSet<Result<(&'static str, CallToolResult)>> = JoinSet::new();

        // ── 4 reads ────────────────────────────────────────────────────
        {
            let h = Arc::clone(&handler);
            let b = Arc::clone(&barrier);
            let ws = ws_filter.clone();
            set.spawn(async move {
                b.wait().await;
                let r = FastSearchTool {
                    query: "alpha".to_string(),
                    limit: 10,
                    workspace: Some(ws),
                    search_target: "definitions".to_string(),
                    ..Default::default()
                }
                .call_tool(&h)
                .await?;
                Ok(("fast_search", r))
            });
        }
        {
            let h = Arc::clone(&handler);
            let b = Arc::clone(&barrier);
            let ws = ws_filter.clone();
            set.spawn(async move {
                b.wait().await;
                let r = GetSymbolsTool {
                    file_path: "src/beta.rs".to_string(),
                    max_depth: 2,
                    target: None,
                    limit: Some(50),
                    mode: Some("structure".to_string()),
                    workspace: Some(ws),
                }
                .call_tool(&h)
                .await?;
                Ok(("get_symbols", r))
            });
        }
        {
            let h = Arc::clone(&handler);
            let b = Arc::clone(&barrier);
            let ws = ws_filter.clone();
            set.spawn(async move {
                b.wait().await;
                let r = DeepDiveTool {
                    symbol: "alpha_func".to_string(),
                    depth: DeepDiveDepth::Overview,
                    context_file: None,
                    workspace: Some(ws),
                }
                .call_tool(&h)
                .await?;
                Ok(("deep_dive", r))
            });
        }
        {
            let h = Arc::clone(&handler);
            let b = Arc::clone(&barrier);
            let ws = ws_filter.clone();
            set.spawn(async move {
                b.wait().await;
                let r = FastRefsTool {
                    symbol: "alpha_helper".to_string(),
                    include_definition: true,
                    limit: 10,
                    workspace: Some(ws),
                    reference_kind: None,
                }
                .call_tool(&h)
                .await?;
                Ok(("fast_refs", r))
            });
        }

        // ── 4 writes (3 dry-runs with non-identical content + 1 REAL) ──
        {
            // Dry-run edit_file with a real diff (old_text != new_text) so the
            // engine actually runs the apply path.
            let h = Arc::clone(&handler);
            let b = Arc::clone(&barrier);
            let ws = ws_filter.clone();
            set.spawn(async move {
                b.wait().await;
                let r = EditFileTool {
                    file_path: "src/beta.rs".to_string(),
                    old_text: "beta_func".to_string(),
                    new_text: "beta_func_preview".to_string(),
                    workspace: Some(ws),
                    dry_run: true,
                    occurrence: EditOccurrence::First,
                }
                .call_tool(&h)
                .await?;
                Ok(("edit_file_dry_run", r))
            });
        }
        {
            // Second manage_workspace index — exercises the gated write-side
            // plumbing in parallel with the watcher's event-processor.
            let h = Arc::clone(&handler);
            let b = Arc::clone(&barrier);
            let ws_str = ws_root.to_string_lossy().to_string();
            set.spawn(async move {
                b.wait().await;
                let r = ManageWorkspaceTool {
                    operation: "index".to_string(),
                    path: Some(ws_str),
                    name: None,
                    workspace_id: None,
                    force: Some(false),
                    detailed: None,
                }
                .call_tool(&h)
                .await?;
                Ok(("manage_workspace_index", r))
            });
        }
        {
            // Dry-run rename_symbol with old != new — actually runs the rename
            // engine (validation rejects old_name == new_name).
            let h = Arc::clone(&handler);
            let b = Arc::clone(&barrier);
            let ws = ws_filter.clone();
            set.spawn(async move {
                b.wait().await;
                let r = RenameSymbolTool {
                    old_name: "beta_func".to_string(),
                    new_name: "beta_func_renamed".to_string(),
                    scope: None,
                    dry_run: true,
                    workspace: Some(ws),
                }
                .call_tool(&h)
                .await?;
                Ok(("rename_symbol_dry_run", r))
            });
        }
        {
            // REAL mutation against the disposable file. EditFileTool commits
            // via EditingTransaction which does NOT acquire the gate; the
            // gate is acquired downstream by the watcher event-processor when
            // it re-indexes the changed file. We verify that downstream
            // re-index landed (post-condition below).
            let h = Arc::clone(&handler);
            let b = Arc::clone(&barrier);
            let ws = ws_filter.clone();
            set.spawn(async move {
                b.wait().await;
                let r = EditFileTool {
                    file_path: "src/disposable.rs".to_string(),
                    old_text: "disposable_marker".to_string(),
                    new_text: "disposable_marker_v2".to_string(),
                    workspace: Some(ws),
                    dry_run: false,
                    occurrence: EditOccurrence::First,
                }
                .call_tool(&h)
                .await?;
                Ok(("edit_file_real_mutation", r))
            });
        }

        // Drive the JoinSet under a 30s ceiling. The plan picked this budget
        // because real lock-order wedges block for the full timeout, while a
        // healthy concurrent run completes well under it.
        type TaskOutcome = (&'static str, Option<CallToolResult>, Option<String>);
        let drive_result = timeout(Duration::from_secs(30), async {
            let mut completed: Vec<TaskOutcome> = Vec::with_capacity(8);
            while let Some(handle) = set.join_next().await {
                match handle {
                    Ok(Ok((label, result))) => completed.push((label, Some(result), None)),
                    Ok(Err(err)) => completed.push(("<errored>", None, Some(err.to_string()))),
                    Err(join_err) => {
                        completed.push(("<panicked>", None, Some(join_err.to_string())));
                    }
                }
            }
            completed
        })
        .await;

        // Stop the watcher driver so the test exits cleanly even if a tool
        // call failed — we still want a clean drop.
        watcher_stop.notify_waiters();
        let _ = watcher_task.await;

        let completed = drive_result.expect(
            "8 concurrent MCP requests must complete within 30s — if this \
             times out there's a lock-order regression between \
             connection_pool, mutation_gate, and the watcher event-processor.",
        );

        // Reject Result errors AND CallToolResult.is_error payloads. The
        // latter is the failure mode codex flagged: a tool returning
        // Ok(CallToolResult::error(...)) would otherwise count as success
        // even though indexing/etc actually failed under the lock-order
        // regression we're hunting.
        let errors: Vec<_> = completed
            .iter()
            .filter_map(|(label, result, err)| {
                if let Some(e) = err {
                    Some((*label, e.clone()))
                } else if result
                    .as_ref()
                    .and_then(|r| r.is_error)
                    .unwrap_or(false)
                {
                    Some((*label, "tool returned is_error=true".to_string()))
                } else {
                    None
                }
            })
            .collect();
        assert!(
            errors.is_empty(),
            "tools must complete without errors; failures: {errors:?}"
        );
        assert_eq!(
            completed.len(),
            8,
            "all 8 tasks must complete: {:?}",
            completed.iter().map(|(l, _, _)| l).collect::<Vec<_>>()
        );

        // ── Real-mutation post-condition (codex finding #2) ──
        // The disposable.rs edit committed via EditingTransaction — but
        // EditingTransaction does NOT acquire the mutation_gate. The gate is
        // acquired downstream when the watcher event-processor picks up the
        // file change and re-indexes. If we only assert the file content
        // (which the previous version did), a regression that broke watcher
        // re-indexing under load would false-pass. Asserting the new symbol
        // landed in the DB proves the watcher event-processor DID process
        // the edit AND crossed the gate.
        wait_for_symbol_via_pool(
            &workspace_pool,
            &workspace_id,
            "disposable_marker_v2",
            Duration::from_secs(10),
        )
        .await
        .map_err(|e| {
            anyhow!(
                "real-mutation post-condition failed: {e}. The watcher did \
                 not re-index src/disposable.rs after the edit_file commit, \
                 which means the mutation_gate write path didn't actually \
                 cross under load — exactly the lock-order regression this \
                 test exists to catch."
            )
        })?;

        Ok(())
    }
}
