# Global Workspace Targeting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement an explicit workspace activation flow so any daemon session can open any known or unknown workspace by path or workspace ID, freshness-gate it before query or edit use, and then keep all tool routing on canonical `workspace_id`.

**Architecture:** Add a single `manage_workspace(operation="open", path=<path>)` or `manage_workspace(operation="open", workspace_id=<id>)` front door backed by handler-level activation helpers. Each session handler owns an active workspace set plus a shared `WorkspacePool` handle. Opening a workspace resolves it, indexes or refreshes it, then activates it for the current session. Existing search, navigation, and editing tools keep routing by `workspace_id`, but daemon-mode resolution now requires the target workspace to be active in the current session. Legacy reference relationships remain as optional pairing metadata only, and daemon connect no longer auto-attaches them.

**Tech Stack:** Rust, tokio, anyhow, rusqlite, existing daemon registry, WorkspacePool, WatcherPool, workspace indexing pipeline

**Spec:** `docs/superpowers/specs/2026-04-10-global-workspace-targeting-design.md`

**Current worktree note:** There is already a partial uncommitted attempt touching `src/daemon/mod.rs`, `src/daemon/workspace_pool.rs`, `src/handler.rs`, `src/tests/daemon/handler.rs`, and `src/tests/dashboard/state.rs`. Start by salvaging the active-workspace teardown work and the daemon cleanup regression test, then remove accidental drift such as duplicated test attributes, missing `#[test]` coverage, and dead scaffolding before moving deeper into the plan.

---

### Task 1: Stabilize the partial active-workspace groundwork and make it intentional

**Files:**
- Modify: `src/handler.rs:145-306` (handler fields, constructors, and active-workspace helpers)
- Modify: `src/daemon/mod.rs:678-995` (pass pool into handler, preserve daemon cleanup coverage, disconnect all active workspaces on teardown)
- Modify: `src/daemon/workspace_pool.rs:83-247` (shared session-count updates and cleanup-safe teardown)
- Modify: `src/tests/daemon/handler.rs` (active-workspace set tests)
- Modify: `src/tests/dashboard/state.rs` (repair accidental duplicate and missing `#[test]` annotations)
- Test: `src/daemon/mod.rs` inline unix test `test_handle_ipc_session_cleans_up_references_on_serve_error`

- [ ] **Step 1: Write the failing tests**

Add to `src/tests/daemon/handler.rs`:

```rust
#[tokio::test]
async fn test_active_workspace_set_is_seeded_from_primary() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        None,
        None,
        None,
    ));

    let ws = pool
        .get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    let handler = JulieServerHandler::new_with_shared_workspace(
        ws,
        workspace_root.path().to_path_buf(),
        None,
        Some("test_ws".to_string()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should succeed");

    let active = handler.active_workspace_ids().await;
    assert_eq!(active.len(), 1);
    assert!(active.contains("test_ws"));
}

#[tokio::test]
async fn test_active_workspace_set_tracks_secondary_activation() {
    let indexes_dir = temp_indexes_dir();
    let primary_root = temp_workspace_root();
    let secondary_root = temp_workspace_root();
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        None,
        None,
        None,
    ));

    let ws = pool
        .get_or_init("primary_ws", primary_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    let handler = JulieServerHandler::new_with_shared_workspace(
        ws,
        primary_root.path().to_path_buf(),
        None,
        Some("primary_ws".to_string()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should succeed");

    handler
        .activate_workspace("secondary_ws", secondary_root.path().to_path_buf())
        .await
        .expect("secondary activation should succeed");

    let active = handler.active_workspace_ids().await;
    assert_eq!(active.len(), 2);
    assert!(active.contains("primary_ws"));
    assert!(active.contains("secondary_ws"));
}
```

- [ ] **Step 2: Run the focused checks to verify the starting state**

Run: `cargo test --lib test_active_workspace_set_ 2>&1 | tail -10`
Expected: on a clean pre-implementation tree, these fail to compile because the active-workspace helpers do not exist yet. On the current partial worktree, they may already pass, which means you must review and salvage the existing diff instead of reimplementing blindly.

Run: `cargo test --lib tests::dashboard::state -q`
Expected: current partial drift is visible here if duplicated `#[test]` attributes or missing test annotations remain.

Run: `cargo build -q`
Expected: current partial drift is visible here if Task 1 leaves new dead-code warnings such as unused workspace activation scaffolding.

- [ ] **Step 3: Implement handler activation state and teardown support**

If the current worktree already contains parts of this task, salvage the active-workspace set and the multi-workspace teardown loop instead of rewriting them. While doing that, repair `src/tests/dashboard/state.rs` so both dashboard embedding lifecycle tests are still real `#[test]` functions, and do not leave new dead-code warnings behind.

In `src/handler.rs`, add the new fields, constructor initialization, and helper methods:

```rust
use std::collections::{HashMap, HashSet};

pub(crate) workspace_pool: Option<Arc<crate::daemon::workspace_pool::WorkspacePool>>,
pub(crate) active_workspaces: Arc<RwLock<HashSet<String>>>,

pub async fn new_with_shared_workspace(
    workspace: Arc<JulieWorkspace>,
    workspace_root: PathBuf,
    daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
    workspace_id: Option<String>,
    embedding_service: Option<Arc<crate::daemon::embedding_service::EmbeddingService>>,
    restart_pending: Option<Arc<std::sync::atomic::AtomicBool>>,
    dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    watcher_pool: Option<Arc<crate::daemon::watcher_pool::WatcherPool>>,
    workspace_pool: Option<Arc<crate::daemon::workspace_pool::WorkspacePool>>,
) -> Result<Self> {
    let active_workspaces = Arc::new(RwLock::new(HashSet::new()));
    if let Some(ref id) = workspace_id {
        active_workspaces.write().await.insert(id.clone());
    }

    Ok(Self {
        workspace_root,
        workspace: Arc::new(RwLock::new(Some(ws_clone))),
        is_indexed: Arc::new(RwLock::new(already_indexed)),
        indexing_status: Arc::new(IndexingStatus::new()),
        session_metrics: Arc::new(SessionMetrics::new()),
        embedding_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        tool_router: Self::tool_router(),
        project_log,
        daemon_db,
        workspace_id,
        workspace_pool,
        active_workspaces,
        embedding_service,
        restart_pending,
        catchup_in_progress: Arc::new(AtomicBool::new(false)),
        watcher_pool,
        metrics_tx,
        ref_db_cache: Arc::new(RwLock::new(HashMap::new())),
        dashboard_tx,
    })
}

pub(crate) async fn active_workspace_ids(&self) -> HashSet<String> {
    self.active_workspaces.read().await.clone()
}

pub(crate) async fn is_workspace_active(&self, workspace_id: &str) -> bool {
    self.active_workspaces.read().await.contains(workspace_id)
}

pub(crate) async fn mark_workspace_active(&self, workspace_id: impl Into<String>) {
    self.active_workspaces.write().await.insert(workspace_id.into());
}

pub(crate) async fn activate_workspace(
    &self,
    workspace_id: &str,
    workspace_root: PathBuf,
) -> Result<()> {
    let pool = self
        .workspace_pool
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Workspace activation requires daemon mode"))?;

    pool.get_or_init(workspace_id, workspace_root).await?;
    self.mark_workspace_active(workspace_id.to_string()).await;
    Ok(())
}
```

In `src/daemon/mod.rs`, change `handle_ipc_session` to receive `pool: Arc<WorkspacePool>`, pass `Arc::clone(&pool)` into the handler, and disconnect every active workspace on session shutdown:

```rust
let handler = JulieServerHandler::new_with_shared_workspace(
    workspace,
    workspace_path,
    daemon_db.clone(),
    Some(full_workspace_id.clone()),
    Some(Arc::clone(embedding_service)),
    Some(Arc::clone(restart_pending)),
    dashboard_tx,
    watcher_pool,
    Some(Arc::clone(&pool)),
)
.await?;

let active_workspaces = handler.active_workspaces.clone();
let project_log = handler.project_log.clone();
let service = handler
    .serve(stream)
    .await
    .map_err(|e| anyhow::anyhow!("MCP serve failed: {}", e))?;
let session_wait_result = service.waiting().await;

let workspace_ids = active_workspaces.read().await.clone();
for workspace_id in workspace_ids {
    pool.sync_indexed_from_db(&workspace_id).await;
    pool.disconnect_session(&workspace_id).await;
}
```

Note: do not wrap `pool.clone()` in a new allocation. Use `Arc<WorkspacePool>` at the call site so the handler stores the same shared pool instance.

- [ ] **Step 4: Run the focused checks to verify the salvaged groundwork**

Run: `cargo test --lib test_active_workspace_set_ 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib test_handle_ipc_session_cleans_up_references_on_serve_error 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib tests::dashboard::state -q`
Expected: PASS, with no duplicate-attribute warning and with both dashboard lifecycle tests still running.

Run: `cargo build -q`
Expected: no new warnings from Task 1. If the pre-existing `src/adapter/mod.rs::forward_streams` warning remains, leave it for the separate warning-fix task.

- [ ] **Step 5: Commit**

```bash
git add src/handler.rs src/daemon/mod.rs src/daemon/workspace_pool.rs src/tests/daemon/handler.rs src/tests/dashboard/state.rs
git commit -m "fix(daemon): stabilize active workspace groundwork"
```

---

### Task 2: Add `manage_workspace open` as the explicit activation front door

**Files:**
- Modify: `src/tools/workspace/commands/mod.rs:16-173`
- Modify: `src/tools/workspace/commands/registry/mod.rs:1-16`
- Create: `src/tools/workspace/commands/registry/open.rs`
- Modify: `src/tests/mod.rs:85-94`
- Create: `src/tests/tools/workspace/global_targeting.rs`

- [ ] **Step 1: Write the failing tests**

Create `src/tests/tools/workspace/global_targeting.rs` with a focused daemon-mode setup helper and the first two open-command tests:

```rust
use anyhow::Result;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;

fn extract_text_from_result(result: &crate::mcp_compat::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn setup_daemon_handler() -> Result<(
    JulieServerHandler,
    Arc<DaemonDatabase>,
    Arc<WorkspacePool>,
    TempDir,
    TempDir,
)> {
    let daemon_home = tempfile::tempdir()?;
    let indexes_dir = daemon_home.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_home.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_root = tempfile::tempdir()?;
    fs::create_dir_all(primary_root.path().join("src"))?;
    fs::write(primary_root.path().join("src/main.rs"), "fn primary() {}\n")?;

    let target_root = tempfile::tempdir()?;
    fs::create_dir_all(target_root.path().join("src"))?;
    fs::write(target_root.path().join("src/lib.rs"), "pub fn target() {}\n")?;

    let primary_path = primary_root.path().to_string_lossy().to_string();
    let primary_id = crate::workspace::registry::generate_workspace_id(&primary_path)?;
    let primary_ws = pool
        .get_or_init(&primary_id, primary_root.path().to_path_buf())
        .await?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_root.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    Ok((handler, daemon_db, pool, primary_root, target_root))
}

#[tokio::test]
async fn test_manage_workspace_open_registers_missing_workspace_and_returns_workspace_id() -> Result<()> {
    let (handler, daemon_db, _pool, _primary_root, target_root) = setup_daemon_handler().await?;
    let target_path = target_root.path().to_string_lossy().to_string();
    let target_id = crate::workspace::registry::generate_workspace_id(&target_path)?;

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: Some(target_path.clone()),
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    assert!(text.contains(&target_id));
    assert!(handler.is_workspace_active(&target_id).await);
    assert!(daemon_db.get_workspace(&target_id)?.is_some());
    Ok(())
}

#[tokio::test]
async fn test_manage_workspace_open_by_workspace_id_marks_known_workspace_active() -> Result<()> {
    let (handler, daemon_db, _pool, _primary_root, target_root) = setup_daemon_handler().await?;
    let target_path = target_root.path().to_string_lossy().to_string();
    let target_id = crate::workspace::registry::generate_workspace_id(&target_path)?;

    daemon_db.upsert_workspace(&target_id, &target_path, "ready")?;

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    tool.call_tool(&handler).await?;

    assert!(handler.is_workspace_active(&target_id).await);
    Ok(())
}
```

Register the new test module in `src/tests/mod.rs`:

```rust
pub mod workspace {
    pub mod discovery;
    pub mod global_targeting;
    pub mod index_embedding_tests;
    pub mod isolation;
    pub mod management_token;
    pub mod mod_tests;
    pub mod registry;
    pub mod resolver;
    pub mod utils;
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib test_manage_workspace_open_ 2>&1 | tail -10`
Expected: compilation error because `open` is not a valid workspace operation.

- [ ] **Step 3: Implement the open command**

Update `src/tools/workspace/commands/mod.rs` so the schema and dispatcher include the new operation:

```rust
/// Operation to perform: "index", "open", "list", "add", "remove", "stats", "clean", "refresh", "health"
pub operation: String,

"open" => {
    self.handle_open_command(handler, self.path.clone(), self.workspace_id.clone())
        .await
}
```

Update `src/tools/workspace/commands/registry/mod.rs`:

```rust
mod add_remove;
mod health;
mod list_clean;
mod open;
mod refresh_stats;
```

Create `src/tools/workspace/commands/registry/open.rs`:

```rust
use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::workspace::registry::generate_workspace_id;
use anyhow::Result;
use std::path::PathBuf;

impl ManageWorkspaceTool {
    pub(crate) async fn handle_open_command(
        &self,
        handler: &JulieServerHandler,
        path: Option<String>,
        workspace_id: Option<String>,
    ) -> Result<CallToolResult> {
        let db = handler
            .daemon_db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Global workspace targeting requires daemon mode"))?;

        let current_id = handler.workspace_id.clone();
        let (target_id, target_path, bootstrap_needed) = match (path, workspace_id.as_deref()) {
            (Some(path), None) => {
                let canonical = PathBuf::from(&path)
                    .canonicalize()
                    .unwrap_or_else(|_| PathBuf::from(&path));
                let canonical_str = canonical.to_string_lossy().to_string();

                if let Some(row) = db.get_workspace_by_path(&canonical_str)? {
                    let bootstrap_needed = row.status != "ready";
                    (row.workspace_id, PathBuf::from(row.path), bootstrap_needed)
                } else {
                    let target_id = generate_workspace_id(&canonical_str)?;
                    (target_id, canonical, true)
                }
            }
            (None, Some("primary")) | (None, None) => {
                let target_id = current_id
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("Primary workspace is not bound to this session"))?;
                (target_id, handler.workspace_root.clone(), false)
            }
            (None, Some(target_id)) => {
                let row = db
                    .get_workspace(target_id)?
                    .ok_or_else(|| anyhow::anyhow!("Workspace '{}' not found", target_id))?;
                let bootstrap_needed = row.status != "ready";
                (row.workspace_id, PathBuf::from(row.path), bootstrap_needed)
            }
            (Some(_), Some(_)) => {
                return Err(anyhow::anyhow!("Provide either 'path' or 'workspace_id', not both"));
            }
        };

        if bootstrap_needed {
            self.handle_index_command(
                handler,
                Some(target_path.to_string_lossy().to_string()),
                false,
                true,
            )
            .await?;
        } else if current_id.as_deref() != Some(target_id.as_str()) {
            self.handle_refresh_command(handler, &target_id).await?;
        }

        handler
            .activate_workspace(&target_id, target_path.clone())
            .await?;

        let message = format!(
            "Workspace Opened\nWorkspace ID: {}\nPath: {}\nSession State: active",
            target_id,
            target_path.display(),
        );
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib test_manage_workspace_open_ 2>&1 | tail -10`
Expected: both tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/tools/workspace/commands/mod.rs src/tools/workspace/commands/registry/mod.rs src/tools/workspace/commands/registry/open.rs src/tests/mod.rs src/tests/tools/workspace/global_targeting.rs
git commit -m "feat(workspace): add explicit open command for session activation"
```

---

### Task 3: Require activation before daemon-mode workspace routing

**Files:**
- Modify: `src/tools/navigation/resolution.rs:33-103`
- Modify: `src/tests/tools/workspace/global_targeting.rs`

- [ ] **Step 1: Write the failing tests**

Append to `src/tests/tools/workspace/global_targeting.rs`:

```rust
use crate::tools::FastSearchTool;

#[tokio::test]
async fn test_known_workspace_requires_open_before_fast_search() -> Result<()> {
    let (handler, daemon_db, _pool, _primary_root, target_root) = setup_daemon_handler().await?;
    let target_path = target_root.path().to_string_lossy().to_string();
    let target_id = crate::workspace::registry::generate_workspace_id(&target_path)?;

    daemon_db.upsert_workspace(&target_id, &target_path, "ready")?;

    let search = FastSearchTool {
        query: "target".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some(target_id.clone()),
        search_target: "content".to_string(),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let error = search.call_tool(&handler).await.unwrap_err().to_string();
    assert!(error.contains("not active for this session"));
    Ok(())
}

#[tokio::test]
async fn test_opened_workspace_routes_fast_search_by_workspace_id() -> Result<()> {
    let (handler, _daemon_db, _pool, _primary_root, target_root) = setup_daemon_handler().await?;
    let target_path = target_root.path().to_string_lossy().to_string();
    let target_id = crate::workspace::registry::generate_workspace_id(&target_path)?;

    let open = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: Some(target_path),
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    };
    open.call_tool(&handler).await?;

    let search = FastSearchTool {
        query: "target".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some(target_id.clone()),
        search_target: "content".to_string(),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result = search.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);
    assert!(text.contains("target"));
    Ok(())
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib test_known_workspace_requires_open_before_fast_search 2>&1 | tail -10`
Expected: first test fails because known workspaces still route without activation.

- [ ] **Step 3: Enforce active-session membership in shared workspace resolution**

Update `src/tools/navigation/resolution.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceTarget {
    Primary,
    /// Use a specific active non-primary workspace by ID.
    Reference(String),
}

pub async fn resolve_workspace_filter(
    workspace_param: Option<&str>,
    handler: &JulieServerHandler,
) -> Result<WorkspaceTarget> {
    let workspace_param = workspace_param.unwrap_or("primary");

    match workspace_param {
        "primary" => Ok(WorkspaceTarget::Primary),
        workspace_id => {
            if let Some(ref db) = handler.daemon_db {
                return match db.get_workspace(workspace_id)? {
                    Some(_) => {
                        if !handler.is_workspace_active(workspace_id).await {
                            Err(anyhow::anyhow!(
                                "Workspace '{}' is known but not active for this session. Call manage_workspace(operation=\"open\", workspace_id=\"{}\") first.",
                                workspace_id,
                                workspace_id,
                            ))
                        } else {
                            Ok(WorkspaceTarget::Reference(workspace_id.to_string()))
                        }
                    }
                    None => {
                        let all_workspaces = db.list_workspaces().unwrap_or_default();
                        let workspace_ids: Vec<&str> = all_workspaces
                            .iter()
                            .map(|w| w.workspace_id.as_str())
                            .collect();
                        suggest_closest_workspace(workspace_id, &workspace_ids)
                    }
                };
            }

            Ok(WorkspaceTarget::Reference(workspace_id.to_string()))
        }
    }
}
```

No tool-specific routing code should change in this task. Search, refs, deep_dive, get_context, get_symbols, rename, and metrics already call the shared resolver. This change is the contract boundary.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib test_known_workspace_requires_open_before_fast_search 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib test_opened_workspace_routes_fast_search_by_workspace_id 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/tools/navigation/resolution.rs src/tests/tools/workspace/global_targeting.rs
git commit -m "fix(tools): gate daemon workspace routing on session activation"
```

---

### Task 4: Demote reference pairings to convenience metadata and fix removal semantics

**Files:**
- Modify: `src/daemon/mod.rs:914-948` (remove auto-attach of pairings on session connect)
- Modify: `src/tools/workspace/commands/registry/add_remove.rs:8-223`
- Modify: `src/tools/workspace/commands/registry/list_clean.rs:8-74`
- Modify: `src/tools/workspace/commands/registry/refresh_stats.rs:134-228`
- Modify: `src/tests/tools/workspace/global_targeting.rs`

- [ ] **Step 1: Write the failing tests**

Append to `src/tests/tools/workspace/global_targeting.rs`:

```rust
#[tokio::test]
async fn test_manage_workspace_list_includes_unpaired_known_workspace() -> Result<()> {
    let (handler, daemon_db, _pool, _primary_root, target_root) = setup_daemon_handler().await?;
    let target_path = target_root.path().to_string_lossy().to_string();
    let target_id = crate::workspace::registry::generate_workspace_id(&target_path)?;

    daemon_db.upsert_workspace(&target_id, &target_path, "ready")?;

    let list = ManageWorkspaceTool {
        operation: "list".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = list.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);
    assert!(text.contains(&target_id));
    assert!(text.contains("KNOWN"));
    Ok(())
}

#[test]
fn test_remove_workspace_uses_global_index_dir_shape() {
    let home = tempfile::tempdir().unwrap();
    let paths = crate::paths::DaemonPaths::with_home(home.path().to_path_buf());
    let target_dir = crate::tools::workspace::paths::daemon_workspace_index_dir(&paths, "lib_deadbeef");
    assert_eq!(target_dir, home.path().join("indexes").join("lib_deadbeef"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib test_manage_workspace_list_includes_unpaired_known_workspace 2>&1 | tail -10`
Expected: FAIL because `list` only reports primary plus pairings.

Run: `cargo test --lib test_remove_workspace_uses_global_index_dir_shape 2>&1 | tail -10`
Expected: compilation error because `daemon_workspace_index_dir` does not exist.

- [ ] **Step 3: Update list, add, remove, and connect semantics**

Create a small helper in `src/tools/workspace/paths.rs` and use it from remove logic:

```rust
pub(crate) fn daemon_workspace_index_dir(
    paths: &crate::paths::DaemonPaths,
    workspace_id: &str,
) -> PathBuf {
    paths.indexes_dir().join(workspace_id)
}
```

Remove the auto-attach loop from `src/daemon/mod.rs`. The session should start with the current workspace only. Pairings no longer imply activation.

Update `src/tools/workspace/commands/registry/add_remove.rs`:

```rust
// keep db.add_reference(primary_workspace_id, &target_id) as optional metadata
let message = format!(
    "Workspace Registered\nWorkspace ID: {}\nPath: {}\nPairing: saved for current workspace\nNext Step: call manage_workspace(operation=\"open\", workspace_id=\"{}\") to activate it in this session.",
    target_id,
    path_str,
    target_id,
);
```

Also change remove to delete the global index directory shape instead of looking under the primary workspace root:

```rust
let paths = crate::paths::DaemonPaths::try_new()?;
let index_dir = crate::tools::workspace::paths::daemon_workspace_index_dir(&paths, workspace_id);
if index_dir.exists() {
    tokio::fs::remove_dir_all(&index_dir).await?;
}

let _ = db.remove_reference(primary_workspace_id, workspace_id);
db.delete_workspace(workspace_id)?;
```

Update `src/tools/workspace/commands/registry/list_clean.rs` to list all known workspaces, then annotate paired ones for the current workspace:

```rust
let known = db.list_workspaces()?;
let paired_ids: std::collections::HashSet<String> = db
    .list_references(primary_workspace_id)
    .unwrap_or_default()
    .into_iter()
    .map(|row| row.workspace_id)
    .collect();

for ws in &known {
    let label = if ws.workspace_id == primary_workspace_id {
        "CURRENT"
    } else if paired_ids.contains(&ws.workspace_id) {
        "KNOWN | PAIRED"
    } else {
        "KNOWN"
    };
    let path_exists = std::path::Path::new(&ws.path).exists();
    let status_str = if !path_exists { "MISSING" } else { &ws.status };
    output.push_str(&format!(
        "{} ({}) [{}]\nPath: {}\nStatus: {}\nFiles: {} | Symbols: {}\n\n",
        ws.workspace_id
            .split('_')
            .next()
            .unwrap_or(&ws.workspace_id),
        ws.workspace_id,
        label,
        ws.path,
        status_str,
        ws.file_count.unwrap_or(0),
        ws.symbol_count.unwrap_or(0),
    ));
}
```

Update `src/tools/workspace/commands/registry/refresh_stats.rs` so the overall stats path also uses `db.list_workspaces()` instead of `primary + references`:

```rust
let known = db.list_workspaces()?;
let total_files: i64 = known.iter().map(|ws| ws.file_count.unwrap_or(0)).sum();
let total_symbols: i64 = known.iter().map(|ws| ws.symbol_count.unwrap_or(0)).sum();
let paired_count = db
    .list_references(primary_workspace_id)
    .unwrap_or_default()
    .len();

let message = format!(
    "Overall Workspace Statistics\n\nRegistry Status\nCurrent Workspace: {}\nKnown Workspaces: {}\nPaired Workspaces: {}\n\nStorage Usage\nTotal Files: {}\nTotal Symbols: {}",
    primary_workspace_id,
    known.len(),
    paired_count,
    total_files,
    total_symbols,
);
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib test_manage_workspace_list_includes_unpaired_known_workspace 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib test_remove_workspace_uses_global_index_dir_shape 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/daemon/mod.rs src/tools/workspace/paths.rs src/tools/workspace/commands/registry/add_remove.rs src/tools/workspace/commands/registry/list_clean.rs src/tools/workspace/commands/registry/refresh_stats.rs src/tests/tools/workspace/global_targeting.rs
git commit -m "refactor(workspace): treat pairings as convenience metadata"
```

---

### Task 5: Update docs and run branch-level verification

**Files:**
- Modify: `docs/WORKSPACE_ARCHITECTURE.md`
- Modify: `JULIE_AGENT_INSTRUCTIONS.md`
- Modify: `src/tools/workspace/commands/mod.rs:61-100` (tool schema comments and examples)

- [ ] **Step 1: Update the public workflow docs**

In `docs/WORKSPACE_ARCHITECTURE.md`, replace the reference-centric explanation with the explicit open flow:

```md
## Global Workspace Targeting

Daemon mode distinguishes four workspace states:

- **Current workspace**: the workspace bound to the session's project root
- **Known workspace**: any workspace present in daemon metadata
- **Active workspace**: a known workspace opened for the current session
- **Target workspace**: the active workspace selected for a tool call

Cross-workspace usage now follows one front door:

1. Call `manage_workspace(operation="open", path=<path>)` or `manage_workspace(operation="open", workspace_id=<id>)`
2. Julie resolves, indexes or refreshes, and activates the workspace
3. Subsequent tools route by the returned `workspace_id`

Watcher coverage follows active workspaces only.
```

In `JULIE_AGENT_INSTRUCTIONS.md`, update the workspace tool summary:

```md
- `manage_workspace`: Index, open, pair, remove, list, refresh, and health-check workspaces. For cross-workspace work in daemon mode, call `operation="open"` first, then pass the returned `workspace_id` to search, navigation, and editing tools.
```

Update the schema comments in `src/tools/workspace/commands/mod.rs` so the examples mention `open` and stop describing `add` as routing authority.

- [ ] **Step 2: Run the required verification tiers**

Run: `cargo xtask test dev`
Expected: PASS.

Run: `cargo xtask test system`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add docs/WORKSPACE_ARCHITECTURE.md JULIE_AGENT_INSTRUCTIONS.md src/tools/workspace/commands/mod.rs
git commit -m "docs(workspace): document global workspace targeting flow"
```

---

## Spec Coverage Check

- **Explicit front-door activation by path or workspace ID:** Task 2 adds `manage_workspace open`.
- **Index missing workspaces before use:** Task 2 bootstraps unknown or non-ready targets.
- **Freshness-gated query and edit access:** Task 2 refreshes known workspaces before activation, Task 3 blocks daemon-mode routing until activation succeeds.
- **Session active-workspace set:** Task 1 adds handler-owned active workspace tracking.
- **Watcher lifecycle follows active session workspaces:** Task 1 teardown disconnects every active workspace, Task 4 removes pair-driven auto-attach.
- **Route by `workspace_id` after activation:** Task 3 keeps routing in `resolve_workspace_filter` and only changes the precondition.
- **Legacy reference metadata stays optional:** Task 4 keeps pairing storage but removes its routing authority.
- **Remove stale nested-layout assumptions:** Task 4 fixes remove-path resolution and global list/stats semantics.

## Placeholder Scan

- No `TODO`, `TBD`, or "similar to Task N" markers remain.
- Every task names exact files.
- Every behavior change has a concrete test or verification command.
