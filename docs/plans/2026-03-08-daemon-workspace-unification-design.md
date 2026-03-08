# Design: Daemon Workspace Unification

**Date:** 2026-03-08
**Status:** Draft
**Scope:** Unify per-project reference workspaces with daemon project registry

## Problem

Julie has **two independent workspace registry systems** that don't talk to each other:

### Per-Project Reference Workspaces (v3)
- `manage_workspace add /path/to/project-b` → `WorkspaceRegistryService`
- Stores in `.julie/workspace_registry.json` within each project
- **Duplicates the entire index** inside primary's `.julie/indexes/{ref_id}/`
- Tools open a separate `SymbolDatabase` from this local copy
- `resolve_workspace_filter` validates IDs against `WorkspaceRegistryService`
- 15 callers of `workspace_db_path()` use this pattern

### Daemon Project Registry (v4)
- `POST /api/projects` (or `connect` command) → `GlobalRegistry`
- Each project indexed **in its own** `.julie/` directory — no duplication
- `DaemonState.workspaces` holds all loaded workspaces with DB + search index
- Federated search (`workspace="all"`) fans out across all daemon workspaces
- `resolve_workspace_filter` does NOT check this registry

### The Collision
When an agent calls `manage_workspace add /path/to/project-b` in daemon mode:
1. It goes through the per-project path — creates a **duplicate index** inside current project's `.julie/indexes/`
2. The daemon has **no idea** this happened — project-b isn't in `DaemonState`
3. `workspace="all"` won't find it
4. The user ends up with two copies of project-b's index

### User's Intent
> "If I'm in project A and I say add project B at ~/source/projectb, it should register it with the daemon and I should be able to use Julie's tools to search it."

## Design

In **daemon mode** (`handler.daemon_state.is_some()`), `manage_workspace add/remove/list` delegate to the daemon's project registry and `DaemonState`. In **stdio mode** (no daemon), the current reference workspace behavior is preserved as fallback.

### Three Layers of Change

#### Layer 1: Registration (`handle_add_command`, `handle_remove_command`, `handle_list_command`)

**Daemon mode (`handler.daemon_state.is_some()`):**

`add`:
1. Register project with daemon's `GlobalRegistry` (same as `POST /api/projects`)
2. Register in `DaemonState` (creates handler + MCP service)
3. Start file watcher if project already indexed
4. Trigger background indexing via `indexing_sender`
5. Return workspace_id to agent

`remove`:
1. Stop file watcher for the workspace
2. Remove from `DaemonState` (removes workspace + MCP service)
3. Deregister from `GlobalRegistry`
4. Persist registry
5. Do NOT delete the project's `.julie/` directory (belongs to the project)

`list`:
1. Read all projects from `DaemonState.workspaces`
2. Show status (Ready/Indexing/Pending), path, symbol count, file count
3. Mark the current session's workspace as `(current)`

**Stdio mode (`handler.daemon_state.is_none()`):**
- Current behavior preserved: `WorkspaceRegistryService` manages per-project reference workspaces

**Key dependency:** `handle_add_command` needs access to `GlobalRegistry`, `indexing_sender`, `julie_home`, and `CancellationToken` — these live on `AppState`, not `JulieServerHandler`.

**Solution:** Add daemon-level resources to `DaemonState`:

```rust
pub struct DaemonState {
    // Existing fields:
    pub workspaces: HashMap<String, LoadedWorkspace>,
    pub mcp_services: HashMap<String, StreamableHttpService<JulieServerHandler>>,
    pub watcher_manager: Arc<DaemonWatcherManager>,

    // New fields for tool access:
    pub registry: Arc<RwLock<GlobalRegistry>>,
    pub julie_home: PathBuf,
    pub indexing_sender: IndexingSender,
    pub cancellation_token: CancellationToken,
}
```

These are all cloneable/Arc-wrapped. `AppState` and `DaemonState` share the same `Arc<RwLock<GlobalRegistry>>` instance. The `register_workspace` method can use `self.cancellation_token` instead of taking it as a parameter.

Extract a shared function for the registration logic so both `api/projects.rs::create_project` and `handle_add_command` call the same code:

```rust
// In src/daemon_state.rs or a new src/daemon_registration.rs
pub async fn register_project_with_daemon(
    path: &Path,
    daemon_state: &Arc<RwLock<DaemonState>>,
) -> Result<ProjectRegistrationResult>
```

#### Layer 2: Resolution (`resolve_workspace_filter`)

Current logic validates workspace IDs against `WorkspaceRegistryService`. In daemon mode, validate against `DaemonState.workspaces` instead:

```rust
pub async fn resolve_workspace_filter(
    workspace_param: Option<&str>,
    handler: &JulieServerHandler,
) -> Result<WorkspaceTarget> {
    match workspace_param.unwrap_or("primary") {
        "primary" => Ok(WorkspaceTarget::Primary),
        "all" => Ok(WorkspaceTarget::All),
        workspace_id => {
            if let Some(daemon_state) = &handler.daemon_state {
                // Daemon mode: validate against DaemonState
                let state = daemon_state.read().await;
                if state.workspaces.contains_key(workspace_id) {
                    return Ok(WorkspaceTarget::Reference(workspace_id.to_string()));
                }
                // Fuzzy match against daemon workspace IDs...
            } else {
                // Stdio mode: validate against WorkspaceRegistryService
                // (current behavior, unchanged)
            }
        }
    }
}
```

`WorkspaceTarget::Reference(id)` continues to mean "a specific non-primary workspace." The enum doesn't change — only how the ID is validated and resolved.

#### Layer 3: Tool Access (6 tools with `WorkspaceTarget::Reference` handling)

Currently, each tool manually opens a reference workspace DB:
```rust
// deep_dive/mod.rs — current pattern
WorkspaceTarget::Reference(ref_workspace_id) => {
    let workspace = handler.get_workspace().await?...;
    let ref_db_path = workspace.workspace_db_path(&ref_workspace_id);
    let db = SymbolDatabase::new(ref_db_path)?;
    // query db...
}
```

Add handler helpers that abstract daemon vs stdio resolution:

```rust
impl JulieServerHandler {
    /// Get the SymbolDatabase for a non-primary workspace.
    /// Daemon mode: returns the loaded workspace's DB from DaemonState.
    /// Stdio mode: opens reference workspace DB from local .julie/indexes/.
    pub async fn get_database_for_workspace(
        &self, workspace_id: &str
    ) -> Result<Arc<Mutex<SymbolDatabase>>>;

    /// Get the SearchIndex for a non-primary workspace (daemon mode only).
    /// Returns None in stdio mode (reference workspaces don't have Tantivy).
    pub async fn get_search_index_for_workspace(
        &self, workspace_id: &str
    ) -> Result<Option<Arc<Mutex<SearchIndex>>>>;
}
```

Each tool's `Reference` arm simplifies to:
```rust
WorkspaceTarget::Reference(ref_workspace_id) => {
    let db = handler.get_database_for_workspace(&ref_workspace_id).await?;
    // query db... (same logic, different source)
}
```

**Tools affected** (6 match arms on `WorkspaceTarget::Reference`):
1. `src/tools/deep_dive/mod.rs:88` — opens SymbolDatabase
2. `src/tools/navigation/fast_refs.rs:115` — calls `database_find_references_in_reference`
3. `src/tools/symbols/mod.rs:77` — calls `reference::get_symbols_from_reference`
4. `src/tools/search/mod.rs:100,161` — passes workspace_id to `text_search_impl`
5. `src/tools/search/line_mode.rs` — passes workspace to line search
6. `src/tools/get_context/pipeline.rs:475` — opens SymbolDatabase

### Stdio Mode Fallback

All changes are gated on `handler.daemon_state.is_some()`. In stdio mode:
- `handle_add_command` → current reference workspace behavior (local copy)
- `resolve_workspace_filter` → validates against `WorkspaceRegistryService`
- Tools → open reference DB from `workspace_db_path()`
- No behavioral change for stdio users

### Migration

**Breaking change for daemon mode users with existing reference workspaces:**
- Old `.julie/workspace_registry.json` entries are ignored in daemon mode
- Agent must re-register projects via `manage_workspace add`
- Old reference workspace indexes in `.julie/indexes/` become orphaned (cleaned up by `manage_workspace clean`)

This is acceptable for v4.0. Reference workspaces in daemon mode never shipped.

### What's NOT Changing

- `WorkspaceTarget` enum — `Primary`, `Reference(String)`, `All` stay the same
- `federated_search` / `workspace="all"` — already works via DaemonState
- Per-project `.julie/` directory structure — each project keeps its own indexes
- Stdio mode behavior — fully preserved
- `workspace_db_path()` method — still used by stdio mode reference workspaces

## Edge Cases

1. **Project already registered with daemon**: `add` returns existing workspace_id (like 409 Conflict in the API). Not an error.
2. **Project not yet indexed**: `add` registers and triggers indexing. Tools get "not ready" until indexing completes. Agent sees a message like "Registered project_b. Indexing in progress..."
3. **Remove current session's project**: Allowed but warns that this will affect all sessions using this project.
4. **Daemon restart**: Projects are loaded from `GlobalRegistry` on startup, re-indexed if needed. Workspace IDs are stable (derived from path hash).
5. **Same project registered via both systems**: In daemon mode, the daemon registry wins. Old reference workspace indexes are orphaned.

## Acceptance Criteria

### Registration
- [ ] `manage_workspace add /path/to/project-b` in daemon mode registers with daemon (not local reference copy)
- [ ] Project appears in `DaemonState.workspaces` and `GlobalRegistry`
- [ ] Background indexing triggers automatically
- [ ] File watcher starts after indexing completes
- [ ] `manage_workspace remove {id}` in daemon mode deregisters from daemon
- [ ] `manage_workspace list` in daemon mode shows all daemon projects with status
- [ ] Stdio mode `add/remove/list` unchanged

### Resolution
- [ ] `resolve_workspace_filter("project_b_id")` succeeds in daemon mode
- [ ] Invalid workspace IDs get fuzzy match suggestions from daemon workspaces
- [ ] `workspace="all"` continues to work (unchanged)

### Tool Access
- [ ] `fast_search(workspace="project_b_id")` searches project-b's index (daemon-loaded)
- [ ] `deep_dive(workspace="project_b_id")` queries project-b's database (daemon-loaded)
- [ ] `fast_refs(workspace="project_b_id")` finds references in project-b (daemon-loaded)
- [ ] `get_symbols(workspace="project_b_id")` shows file structure from project-b
- [ ] `get_context(workspace="project_b_id")` returns context from project-b
- [ ] All tools work correctly in stdio mode with reference workspaces (unchanged)

### Integration
- [ ] `DaemonState` has `registry`, `julie_home`, `indexing_sender`, `cancellation_token` fields
- [ ] `api/projects.rs::create_project` uses shared registration logic (no duplication)
- [ ] `handler.get_database_for_workspace()` abstracts daemon vs stdio
- [ ] `handler.get_search_index_for_workspace()` returns daemon workspace's search index
- [ ] `cargo test --lib -- --skip search_quality` passes

### End-to-End
- [ ] Start daemon, connect from project A
- [ ] `manage_workspace(operation="add", path="/path/to/project-b")` → returns workspace_id
- [ ] `fast_search(query="some_symbol", workspace="{workspace_id}")` → returns results from project-b
- [ ] `manage_workspace(operation="list")` → shows both project A and project B
- [ ] `manage_workspace(operation="remove", workspace_id="{workspace_id}")` → removes project B
- [ ] `fast_search(query="some_symbol", workspace="{workspace_id}")` → error: workspace not found
