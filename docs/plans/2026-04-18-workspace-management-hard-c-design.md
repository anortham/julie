# Workspace Management Hard C Design

**Date:** 2026-04-18  
**Status:** Design

## Goal

Replace the dead `reference workspace` model with one global workspace model, make the dashboard Projects page the control surface for index management, and auto-prune stale worktree indexes when their paths disappear.

## Why

The current state has four problems:

1. The approved global workspace design says routing happens by `workspace_id` after activation, but commands, dashboard copy, and tests still expose `reference workspace` language.
2. `manage_workspace(operation="add")` mixes two different intents, preload a workspace for later use and pair a workspace to the current primary, while the real behavior the product needs is `register` or `open`.
3. Deleted worktrees leave dead daemon entries and index directories behind until someone runs cleanup by hand.
4. The dashboard already shows the full workspace registry, but it is read-heavy and still reflects the old pairing story.

## Decision

### Workspace model

The product model uses four terms only:

- **Current workspace**: the workspace bound to the session's `primary` target
- **Known workspace**: a workspace registered in daemon metadata
- **Active workspace**: a known workspace opened for the current session
- **Target workspace**: the workspace chosen for a tool call after activation

`reference workspace` is removed from commands, dashboard, docs, and tests.

### Command surface

Use two explicit front doors:

- `manage_workspace(operation="register", path=...)`
- `manage_workspace(operation="open", path=... | workspace_id=...)`

`register` means:

- canonicalize the path
- create or update the daemon registry row
- index the workspace now
- update workspace stats and embeddings
- do **not** activate the workspace for the current session

`open` means:

- resolve by path or `workspace_id`
- index if missing
- refresh if stale
- activate the workspace for the current session
- return the canonical `workspace_id` for later tool routing

`manage_workspace(operation="add")` is removed with no alias.

### Pairing storage

Drop `workspace_references` from daemon storage in this pass. Pairings are not preserved as hidden compatibility state. This pass does not add favorites, recents, or any replacement relationship metadata.

### Dashboard role

The Projects page becomes the workspace management surface:

- `Add Workspace` uses `register`
- row actions provide `Open`, `Refresh`, and `Delete`
- detail panels show workspace status through the new model, not through relationship tags

This pass keeps the dashboard focused on index management. Editor launch, git status, favorites, recents, and broader code health expansion stay out of scope.

### Stale workspace cleanup

Dead workspace paths are auto-pruned when all of the following are true:

- the path no longer exists
- `session_count == 0`
- no watcher refcount remains
- no indexing, refresh, or repair job is in flight

Cleanup runs in two places:

1. Opportunistically during registry-facing actions such as `register`, `open`, `list`, dashboard project refresh, and explicit cleanup
2. In a low-frequency daemon sweep so dead worktree indexes disappear without a UI visit

### Cleanup transparency

Add a small capped cleanup-event log in daemon storage for dashboard visibility. The log records automatic prune events and manual delete events with timestamp, workspace id, path, and reason. Keep the latest 50 entries.

## Scope

### In scope

- Remove `reference workspace` from the user model
- Replace `add` with `register`
- Drop `workspace_references`
- Add dashboard mutation flows for `register`, `open`, `refresh`, and `delete`
- Auto-prune missing inactive workspaces and orphan index directories
- Add a small cleanup-event log for dashboard visibility
- Update docs and tests to the new model

### Out of scope

- Editor launch from the dashboard
- Git status from the dashboard
- Favorites, recents, or saved workspace sets
- Broad dashboard redesign outside workspace management
- Compatibility alias for `add`
- New stdio-only behavior beyond the existing daemon-first model

## File Map

### Workspace commands and routing

- `src/tools/workspace/commands/mod.rs`
- `src/tools/workspace/commands/registry/add_remove.rs`
- `src/tools/workspace/commands/registry/list_clean.rs`
- `src/tools/workspace/commands/registry/open.rs`
- `src/tools/workspace/commands/registry/refresh_stats.rs`
- `src/tools/workspace/commands/registry/mod.rs`

### Daemon storage and lifecycle

- `src/daemon/database.rs`
- `src/daemon/mod.rs`
- `src/daemon/workspace_pool.rs`
- `src/daemon/watcher_pool.rs`
- `src/handler.rs`

### Dashboard

- `src/dashboard/mod.rs`
- `src/dashboard/routes/projects.rs`
- `dashboard/templates/projects.html`
- `dashboard/templates/partials/project_table.html`
- `dashboard/templates/partials/project_row.html`
- `dashboard/templates/partials/project_detail.html`

### Docs

- `docs/WORKSPACE_ARCHITECTURE.md`
- `docs/plans/2026-04-18-workspace-management-hard-c-design.md`

### Tests

- `src/tests/daemon/database.rs`
- `src/tests/daemon/ipc_session.rs`
- `src/tests/daemon/workspace_pool.rs`
- `src/tests/tools/workspace/global_targeting.rs`
- `src/tests/tools/workspace/mod_tests.rs`
- `src/tests/dashboard/integration.rs`
- `src/tests/dashboard/state.rs`

## Design Details

### 1. Replace `add` with `register`

Update `WorkspaceCommand` and `ManageWorkspaceTool` to expose `register` instead of `add`.

Behavior:

- `register` requires daemon mode
- it accepts `path` and optional `name`
- it canonicalizes the path before workspace id generation
- if the workspace already exists, it refreshes index state and stats without activating it
- if the workspace is new, it creates the daemon row, indexes, updates stats, and returns the workspace id

Output text and schema comments must stop using `reference workspace` language.

### 2. Drop pairing storage

Add a daemon DB migration that:

- drops `workspace_references`
- removes database helpers that only exist for pairings
- keeps workspace rows, tool metrics, and codehealth data intact

Delete code paths and tests that preserve or display pairings.

### 3. Keep `open` as the activating front door

`open` remains the only workspace activation path.

Behavior:

- opening by path resolves an existing known workspace or creates one through the same indexing route
- opening by `workspace_id` validates that the workspace exists
- if a known workspace path is missing, prune it first and report that it was removed because the path is gone
- activation adds the workspace to the session active-workspace set and lets watcher coverage follow that state

Pairing metadata no longer affects activation, routing, or watcher behavior.

### 4. Dashboard actions

Add Projects-page mutation routes:

- `POST /projects/register`
- `POST /projects/:id/open`
- `POST /projects/:id/refresh`
- `POST /projects/:id/delete`

Dashboard behavior:

- `Add Workspace` accepts a path and starts indexing through `register`
- known inactive rows show `Open`, `Refresh`, and `Delete`
- current or active rows do not show `Open`
- detail panels show `current` and `active` state labels, not reference tags
- the page can show a compact recent cleanup list sourced from the cleanup-event log

This pass does not add git-status polling or editor-launch controls.

### 5. Auto-prune rules

Create one shared prune decision path used by explicit cleanup, opportunistic cleanup, and the daemon sweep.

Prune candidates:

- workspace row path is missing
- workspace is inactive across sessions
- watcher refcount is zero
- no index operation is running

Prune effects:

- delete the workspace row
- delete the workspace index directory under `~/.julie/indexes/<workspace_id>/`
- record a cleanup event

Keep orphan directory cleanup in the same shared path so daemon registry and index storage stay aligned.

### 6. Delete safety

Manual delete is blocked when a workspace is active:

- non-zero `session_count`
- non-zero watcher refcount
- indexing or refresh in progress

Delete returns a direct error that tells the user why removal is blocked.

### 7. Cleanup-event log

Add a small daemon DB table for cleanup events:

- `id`
- `workspace_id`
- `path`
- `action` (`auto_prune` or `manual_delete`)
- `reason`
- `timestamp`

Dashboard usage:

- show a compact recent cleanup list on the Projects page or in the detail panel
- do not turn this into a broad audit UI

Retention:

- keep the latest 50 rows
- trim older rows on insert

## Testing

Follow TDD for each slice.

### Required regression coverage

- `register` indexes a workspace without activating it
- `open` activates by path and by `workspace_id`
- a known workspace with a missing path is pruned before `open` reports failure
- a new session does not auto-activate unrelated known workspaces
- manual delete refuses active workspaces
- opportunistic prune removes a deleted inactive worktree row and its index directory
- daemon sweep removes deleted inactive workspaces without user interaction
- dashboard actions call the correct workspace operations and update visible state
- no dashboard detail path or stats path depends on `list_references`
- daemon DB migration removes `workspace_references` and keeps other workspace data intact

### Batch verification

- narrow `cargo nextest run --lib <test_name>` loops during RED and GREEN
- `cargo xtask test changed` after the localized batch
- `cargo xtask test dev` once after the full pass

### Dogfood verification

- add a workspace from the dashboard
- open a known workspace from the dashboard
- refresh a known workspace from the dashboard
- delete an inactive known workspace from the dashboard
- delete a worktree on disk and confirm auto-prune removes its row and index

## Acceptance Criteria

- [ ] `manage_workspace(operation="register")` exists and `add` does not
- [ ] no command output, dashboard text, or docs present `reference workspace` as a live concept
- [ ] `workspace_references` is removed from daemon storage and tests no longer depend on it
- [ ] `open` remains the only activating front door for cross-workspace use
- [ ] `register` indexes without activating
- [ ] Projects page supports `Add Workspace`, `Open`, `Refresh`, and `Delete`
- [ ] deleted inactive workspaces auto-prune without manual cleanup
- [ ] manual delete refuses active workspaces
- [ ] dashboard can show recent cleanup events from a capped log
- [ ] `docs/WORKSPACE_ARCHITECTURE.md` matches the new model and command surface

## Risks

- Destructive cleanup bugs could remove a workspace that still has live session or index activity
- Old prompts or docs that still say `add` could fail hard after the cut
- Dashboard mutation routes could sprawl if this pass tries to include editor launch or git-state features

## Mitigations

- Centralize prune eligibility in one helper path used by manual and automatic cleanup
- Block delete and prune on live session, watcher, or indexing state
- Update docs, dashboard copy, and tool descriptions in the same pass as the command rename
- Keep the first dashboard cut focused on workspace management only
