# Daemon Workspace Unification Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Unify per-project reference workspaces with the daemon project registry so `manage_workspace add` registers with the central daemon instead of creating duplicate local indexes.

**Architecture:** Three layers of changes gated on `handler.daemon_state.is_some()`: (1) registration commands delegate to daemon, (2) workspace ID resolution validates against DaemonState, (3) tools access daemon-loaded workspaces via handler helpers. Stdio mode behavior is fully preserved.

**Tech Stack:** Rust (tokio, axum, anyhow)

---

## Task 1: Extend DaemonState with daemon-level resources

**Files:**
- Modify: `src/daemon_state.rs:55-86` (DaemonState struct + new() + register_workspace)
- Modify: `src/server.rs:79-132` (pass new fields when constructing DaemonState and AppState)

**What to build:** Add four fields to `DaemonState` so MCP tool handlers can access daemon-level resources:
- `registry: Arc<RwLock<GlobalRegistry>>`
- `julie_home: PathBuf`
- `indexing_sender: IndexingSender`
- `cancellation_token: CancellationToken`

**Approach:**
- Add the fields to `DaemonState` struct (lines 55-69)
- Update `DaemonState::new()` to accept and store them (currently takes no args, lines 76-86)
- In `start_server` (src/server.rs:79), construct DaemonState with the resources BEFORE wrapping in Arc. The `registry_rw` Arc is created at line 110 — it needs to be created earlier so DaemonState can hold a clone. Similarly, `indexing_sender` is created at line 113 — move it before DaemonState construction, or set it after via a setter.
- The `register_workspace` method (line 243) currently takes `cancellation_token: &CancellationToken` and `daemon_state: Arc<...>` as params. Simplify: use `self.cancellation_token` instead of the param. The `daemon_state` Arc param is still needed because `create_workspace_mcp_service` needs it.
- `load_registered_projects` (line 99) similarly takes `cancellation_token` as param — use `self.cancellation_token`.

**Ordering constraint:** `indexing_sender` is created by `daemon_indexer::spawn_indexing_worker` which needs `registry_rw` and `daemon_state` Arc. This creates a chicken-and-egg: DaemonState needs indexing_sender, but indexing_sender needs DaemonState. Solution: use a two-phase init — create DaemonState without indexing_sender (use a tokio channel placeholder or Option), then set it after spawning the worker. Or use `Option<IndexingSender>` and set it post-construction via a method.

**Acceptance criteria:**
- [ ] `DaemonState` has `registry`, `julie_home`, `indexing_sender`, `cancellation_token` fields
- [ ] `start_server` constructs DaemonState with these resources
- [ ] `register_workspace` uses `self.cancellation_token` (param removed or deprecated)
- [ ] `load_registered_projects` uses `self.cancellation_token`
- [ ] `AppState` and `DaemonState` share the same `Arc<RwLock<GlobalRegistry>>`
- [ ] `cargo test --lib -- --skip search_quality` passes
- [ ] Committed

---

## Task 2: Extract shared project registration + update handle_add_command

**Files:**
- Modify: `src/daemon_state.rs` (add `register_project` method that encapsulates the full registration flow)
- Modify: `src/api/projects.rs:118-200` (refactor create_project to use shared logic)
- Modify: `src/tools/workspace/commands/registry/add_remove.rs:11-151` (daemon mode branch in handle_add_command)

**What to build:** A shared `DaemonState::register_project(&self, path: &Path, daemon_state_arc: Arc<RwLock<DaemonState>>) -> Result<ProjectRegistrationResult>` method that encapsulates:
1. Validate path exists and is a directory
2. Register in GlobalRegistry (via `self.registry`)
3. Register workspace in DaemonState (creates handler + MCP service)
4. Save registry to disk (via `self.julie_home`)
5. Start file watcher if ready
6. Queue background indexing (via `self.indexing_sender`)
7. Return workspace_id + name + whether it was newly created or already existed

Then:
- `api/projects.rs::create_project` calls this method (replace lines 128-195)
- `handle_add_command` checks `handler.daemon_state.is_some()`:
  - **Daemon mode:** call `daemon_state.register_project(path)` — returns result to agent
  - **Stdio mode:** current `WorkspaceRegistryService` behavior (unchanged)

**Approach:**
- The registration result should distinguish "newly registered" vs "already existed" (maps to 201 vs 409 in the API). Use an enum or struct: `ProjectRegistrationResult { workspace_id, name, path, is_new }`.
- `handle_add_command` currently takes `name: Option<String>` which is unused in daemon mode (daemon uses directory name). That's fine — ignore it in daemon mode.
- For the daemon mode branch, acquire `daemon_state` read lock to call register_project. The method itself will acquire write locks internally as needed.
- Keep the existing stdio-mode code path intact (the entire current body of handle_add_command becomes the `else` branch).

**Acceptance criteria:**
- [ ] `DaemonState::register_project` method exists and handles the full flow
- [ ] `api/projects.rs::create_project` uses the shared method (no logic duplication)
- [ ] `handle_add_command` in daemon mode registers with daemon, returns workspace_id
- [ ] `handle_add_command` in stdio mode unchanged (reference workspace behavior)
- [ ] Already-registered projects return success with existing workspace_id (not an error)
- [ ] `cargo test --lib -- --skip search_quality` passes
- [ ] Committed

---

## Task 3: Update handle_remove_command + handle_list_command for daemon mode

**Files:**
- Modify: `src/tools/workspace/commands/registry/add_remove.rs:154-223` (handle_remove_command daemon branch)
- Modify: `src/tools/workspace/commands/registry/list_clean.rs:12-154` (handle_list_command daemon branch)
- Modify: `src/daemon_state.rs` (add `deregister_project` method if needed)

**What to build:**

`handle_remove_command` daemon mode:
1. Get `daemon_state` from handler
2. Stop file watcher for the workspace
3. Remove from DaemonState (remove_workspace — already exists at line 287)
4. Deregister from GlobalRegistry (via `daemon_state.registry`)
5. Save registry
6. Do NOT delete the project's `.julie/` directory
7. Return confirmation message

`handle_list_command` daemon mode:
1. Get `daemon_state` from handler
2. Read all entries from `DaemonState.workspaces`
3. Format with: workspace_id, name (from path), path, status (Ready/Indexing/Registered/Error), symbol count if available
4. Mark the current session's workspace with `(current)`
5. Use existing `ProgressiveReducer` for token optimization

**Approach:**
- Both commands check `handler.daemon_state.is_some()` at the top
- For remove, use `DaemonState::remove_workspace` (existing) + registry deregistration. May need a new method `DaemonState::deregister_project` that handles both.
- For list, the daemon has `LoadedWorkspace.status` and `LoadedWorkspace.path`. Symbol/file counts can come from the GlobalRegistry entries.
- The current session's workspace_id can be determined from `handler.get_workspace().await?.workspace_id` or similar.
- Stdio mode: keep all existing code as-is (the `else` branch).

**Acceptance criteria:**
- [ ] `handle_remove_command` daemon mode removes from DaemonState + GlobalRegistry
- [ ] `handle_remove_command` does NOT delete `.julie/` directory in daemon mode
- [ ] `handle_list_command` daemon mode lists all daemon projects with status
- [ ] `handle_list_command` marks current workspace as `(current)`
- [ ] Both commands preserve stdio mode behavior unchanged
- [ ] `cargo test --lib -- --skip search_quality` passes
- [ ] Committed

---

## Task 4: Update resolve_workspace_filter for daemon mode

**Files:**
- Modify: `src/tools/navigation/resolution.rs:53-107` (resolve_workspace_filter)

**What to build:** In daemon mode, validate workspace IDs against `DaemonState.workspaces` instead of `WorkspaceRegistryService`.

**Approach:**
- At the `workspace_id` match arm (line 62), check `handler.daemon_state` first
- Daemon mode: `daemon_state.read().await` → check `state.workspaces.contains_key(workspace_id)`
  - Found → `Ok(WorkspaceTarget::Reference(workspace_id.to_string()))`
  - Not found → fuzzy match against `state.workspaces.keys()` (reuse existing `find_closest_match`)
- Stdio mode: current `WorkspaceRegistryService` logic (unchanged)
- The fuzzy matching logic (lines 73-106) can be extracted into a helper that takes a slice of workspace IDs, so both daemon and stdio paths share it.

**Acceptance criteria:**
- [ ] Daemon mode: valid daemon workspace IDs resolve to `WorkspaceTarget::Reference`
- [ ] Daemon mode: invalid IDs get fuzzy match suggestions from daemon workspaces
- [ ] Stdio mode: unchanged (validates against WorkspaceRegistryService)
- [ ] `workspace="primary"` and `workspace="all"` unchanged
- [ ] `cargo test --lib -- --skip search_quality` passes
- [ ] Committed

---

## Task 5: Add handler helpers for workspace DB/SearchIndex/root access

**Files:**
- Modify: `src/handler.rs` (add 3 new methods to JulieServerHandler impl)

**What to build:** Three helper methods that abstract daemon-vs-stdio workspace access:

```rust
impl JulieServerHandler {
    /// Get SymbolDatabase for a non-primary workspace.
    /// Daemon: returns loaded workspace's DB from DaemonState.
    /// Stdio: opens reference workspace DB from local .julie/indexes/.
    pub async fn get_database_for_workspace(
        &self, workspace_id: &str
    ) -> Result<Arc<Mutex<SymbolDatabase>>>;

    /// Get SearchIndex for a non-primary workspace.
    /// Daemon: returns loaded workspace's search index from DaemonState.
    /// Stdio: opens reference workspace's Tantivy from local .julie/indexes/.
    pub async fn get_search_index_for_workspace(
        &self, workspace_id: &str
    ) -> Result<Option<Arc<Mutex<SearchIndex>>>>;

    /// Get workspace root path for a non-primary workspace.
    /// Daemon: returns LoadedWorkspace.path from DaemonState.
    /// Stdio: returns WorkspaceEntry.original_path from WorkspaceRegistryService.
    pub async fn get_workspace_root_for_target(
        &self, workspace_id: &str
    ) -> Result<PathBuf>;
}
```

**Approach:**
- `get_database_for_workspace`:
  - Daemon path: `daemon_state.read().await` → `workspaces.get(id)` → `loaded.workspace.db.clone()` → return `Arc<Mutex<SymbolDatabase>>`
  - Stdio path: `handler.get_workspace()` → `workspace_db_path(id)` → `SymbolDatabase::new(path)` → wrap in Arc<Mutex<>>
  - Error if workspace not found or DB not initialized

- `get_search_index_for_workspace`:
  - Daemon path: `loaded.workspace.search_index.clone()` (already `Option<Arc<Mutex<SearchIndex>>>`)
  - Stdio path: `workspace_tantivy_path(id)` → `SearchIndex::open_with_language_configs` → wrap in Arc<Mutex<>>. Return None if tantivy dir doesn't exist.

- `get_workspace_root_for_target`:
  - Daemon path: `loaded.path.clone()`
  - Stdio path: `WorkspaceRegistryService::get_workspace(id)` → `entry.original_path`

**Acceptance criteria:**
- [ ] All three helper methods exist on JulieServerHandler
- [ ] Daemon mode uses DaemonState loaded workspaces (shared Arc, no re-opening)
- [ ] Stdio mode opens local reference workspace DB/Tantivy (current behavior)
- [ ] Proper error messages for missing/not-ready workspaces
- [ ] `cargo test --lib -- --skip search_quality` passes
- [ ] Committed

---

## Task 6: Update tool Reference arms to use handler helpers

**Files:**
- Modify: `src/tools/deep_dive/mod.rs:88-111` (Reference arm)
- Modify: `src/tools/navigation/fast_refs.rs:115-120` (Reference arm + `database_find_references_in_reference`)
- Modify: `src/tools/symbols/mod.rs:77-89` (Reference arm)
- Modify: `src/tools/symbols/reference.rs:16-141` (get_symbols_from_reference — update to accept DB/root instead of opening)
- Modify: `src/tools/search/mod.rs:100,161` (Reference arm in definition search)
- Modify: `src/tools/search/line_mode.rs:152-179` (reference workspace branch)
- Modify: `src/tools/search/text_search.rs:~100` (reference workspace Tantivy opening)
- Modify: `src/tools/get_context/pipeline.rs:475-503` (Reference arm)

**What to build:** Replace the manual "open local reference DB/Tantivy" pattern in each tool's `WorkspaceTarget::Reference` arm with calls to the handler helpers from Task 5.

**Approach — tool by tool:**

1. **deep_dive/mod.rs** (lines 88-111): Currently opens `SymbolDatabase::new(ref_db_path)`. Replace with `handler.get_database_for_workspace(&ref_workspace_id).await?`. Then `db.lock().unwrap()` to get the guard for `deep_dive_query`.

2. **navigation/fast_refs.rs** (lines 115-120): Calls `self.database_find_references_in_reference(handler, ref_workspace_id)`. That helper internally opens the DB. Update that helper to use `handler.get_database_for_workspace()` instead of `workspace.workspace_db_path()`.

3. **symbols/mod.rs + reference.rs**: `get_symbols_from_reference` opens DB and uses `WorkspaceRegistryService` for workspace root. Refactor to:
   - Get DB via `handler.get_database_for_workspace()`
   - Get workspace root via `handler.get_workspace_root_for_target()`
   - Remove direct `WorkspaceRegistryService` usage from this function

4. **search/mod.rs** (definition search, lines 155-162): Currently passes `Some(vec![ref_id])` to `text_search_impl`. In daemon mode, the reference workspace's data is in a SEPARATE Tantivy index (not the primary's). Need to either:
   - Pass the daemon workspace's search_index to text_search_impl, or
   - Have text_search_impl check if a workspace_id refers to a daemon workspace and use its index
   - Simplest: in the Reference arm, if daemon mode, get the search index via handler helper and call `text_search_impl` with it directly. If stdio, keep current behavior.

5. **search/line_mode.rs** (lines 152-179): Currently opens ref Tantivy + DB separately. Replace with handler helpers. The line search logic itself doesn't change — just where the SearchIndex and DB come from.

6. **get_context/pipeline.rs** (lines 475-503): Opens both DB and SearchIndex from ref paths. Replace with handler helpers. `run_pipeline` takes `&SymbolDatabase` and `&SearchIndex` — just get them from the loaded workspace's Arcs instead.

**Key consideration:** In daemon mode, the DB and SearchIndex are **shared Arc<Mutex<>>** (same instance used by all sessions). In stdio mode, they're opened fresh per request. The handler helpers return `Arc<Mutex<>>` in both cases, so the calling code is identical. The only difference is that daemon mode reuses the cached instance (faster, less I/O).

**Acceptance criteria:**
- [ ] All 6 tool Reference arms use handler helpers instead of direct DB/Tantivy opening
- [ ] `get_symbols_from_reference` no longer uses `WorkspaceRegistryService` directly
- [ ] Daemon mode: tools use shared loaded workspace (no re-opening DB/Tantivy per request)
- [ ] Stdio mode: tools open local reference DB/Tantivy (same behavior as before)
- [ ] No `workspace_db_path()` or `workspace_tantivy_path()` calls remain in tool Reference arms (they move into the handler helpers)
- [ ] `cargo test --lib -- --skip search_quality` passes
- [ ] Committed

---

## Task 7: Tests

**Files:**
- Modify: `src/tests/tools/workspace_target_tests.rs` (add daemon-mode resolution tests)
- Create: `src/tests/tools/daemon_workspace_tests.rs` (integration tests for daemon workspace operations)
- Modify: `src/tests/mod.rs` (register new test module)

**What to build:** Tests covering the three layers of changes.

**Tests to add:**

Resolution tests (in workspace_target_tests.rs):
- `test_resolve_workspace_filter_daemon_mode_valid_id` — known daemon workspace resolves to Reference
- `test_resolve_workspace_filter_daemon_mode_invalid_id` — unknown ID returns error with suggestion
- `test_resolve_workspace_filter_daemon_mode_primary_unchanged` — "primary" still returns Primary
- `test_resolve_workspace_filter_daemon_mode_all_unchanged` — "all" still returns All

Registration tests (in daemon_workspace_tests.rs):
- `test_daemon_register_project_creates_workspace` — register_project adds to DaemonState + GlobalRegistry
- `test_daemon_register_project_idempotent` — re-registering same path returns existing ID
- `test_daemon_deregister_project_removes_workspace` — deregister removes from both registries

Handler helper tests (in daemon_workspace_tests.rs):
- `test_get_database_for_workspace_daemon_mode` — returns loaded workspace's DB
- `test_get_database_for_workspace_missing` — returns error for unknown workspace

**Approach:**
- Resolution tests need a mock handler with `daemon_state = Some(...)` containing test workspaces. Follow the pattern in existing workspace_target_tests.rs.
- Registration tests need a real DaemonState with GlobalRegistry. Can be tested without a running HTTP server.
- Handler helper tests need a handler with daemon_state populated. Check existing test patterns in `src/tests/core/handler.rs`.

**Acceptance criteria:**
- [ ] All resolution tests pass for daemon mode
- [ ] Registration + deregistration tests pass
- [ ] Handler helper tests pass for daemon mode
- [ ] Existing tests unchanged and passing
- [ ] `cargo test --lib -- --skip search_quality` passes
- [ ] Committed

---

## Execution Order

Tasks are sequential — each builds on the previous:

```
Task 1 (DaemonState extension) → Task 2 (add command + shared logic) → Task 3 (remove/list)
                                                                          ↓
Task 4 (resolve_workspace_filter) ←───────────────────────────────────────┘
                                                                          ↓
Task 5 (handler helpers) → Task 6 (update tool arms) → Task 7 (tests)
```

Tasks 4 and 5 are independent of each other but both depend on Tasks 1-3 being done (for DaemonState to have the needed fields). Task 6 depends on Task 5 (uses the handler helpers). Task 7 can be written incrementally alongside Tasks 2-6 but is listed last for clarity.

**Parallelism opportunity:** Tasks 4 and 5 can be done in parallel after Task 3.
