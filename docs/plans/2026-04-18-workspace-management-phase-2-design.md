# Workspace Management Phase 2 Design

**Date:** 2026-04-18  
**Status:** Design

## Goal

Finish the remaining workspace-management work after the hard-C cut by defining the full worktree lifecycle model, tightening dashboard state visibility around that model, and verifying the behavior against real daemon and dashboard flows.

## Why

The hard-C pass solved the fake `reference workspace` model and shipped the first dashboard controls, but two important pieces are still loose:

1. Worktrees now prune when their paths disappear, but the daemon lifecycle story is still implicit. Watcher reuse, active-session behavior, and missing-path states need an explicit contract.
2. The dashboard can mutate workspaces, but it still does not tell the whole truth about why a missing workspace can or cannot be cleaned up.

This phase is the cleanup pass that turns the current behavior into a stable product model instead of a pile of helper functions that happen to work.

## Current State

### Already done

- `reference workspace` is gone as a live product concept
- `register` and `open` are the front doors
- dashboard supports `register`, `open`, `refresh`, and `delete`
- missing inactive workspaces auto-prune
- cleanup events are stored and shown on the dashboard

### Still open

- worktree lifecycle semantics are not written down as a first-class model
- watcher behavior across sessions is only implicit in the pool code
- the dashboard does not distinguish `missing but blocked` from `missing and ready to prune`
- the dashboard summary and detail views do not surface enough cleanup context to debug stale rows

## Decision

### Worktrees stay inside the global workspace model

There is still one workspace model only:

- **Current** workspace
- **Active** workspace
- **Known** workspace
- **Target** workspace

A worktree is a known workspace whose path often has a shorter lifetime than a long-lived repo clone. It does not get its own semantic category or its own storage model.

### Lifecycle states

The product behavior should treat each known workspace as one of these effective states:

- **Present**: path exists and workspace is usable
- **Missing**: path is gone
- **Blocked**: missing or requested for deletion, but cleanup cannot proceed because sessions, watchers, or indexing work still hold the workspace live
- **Prunable**: path is gone and cleanup may remove the row and index safely

These are behavior states, not a new persisted enum in this pass. The daemon can derive them from existing registry, watcher, and indexing state.

### Watcher model

Watcher semantics must be explicit:

- a watcher belongs to an active workspace, not to a known workspace
- attaching the same workspace from another session reuses the pooled watcher and increments refcount
- disconnecting the last attached session starts the existing grace timer
- if a workspace path disappears while sessions are still attached, the workspace becomes missing and blocked, not pruned
- once sessions detach and watcher refcount drains, cleanup may prune the row and index

### Missing-path semantics

Missing-path behavior should be uniform across commands and dashboard flows:

- opening a missing inactive workspace prunes it and explains why
- opening a missing active workspace reports that cleanup is blocked and why
- deleting a missing inactive workspace behaves as a normal prune path
- deleting a missing active workspace reports the blocking reason
- dashboard rows and detail panels must surface the blocked reason, not only the fact that the path is missing

### Dashboard role

The Projects page stays the workspace management surface, but this phase expands visibility rather than adding new integrations.

The dashboard should:

- show whether a workspace is `current`, `active`, `known`, or `missing`
- distinguish `missing and blocked` from `missing and prunable`
- surface cleanup block reasons in the detail panel
- keep recent cleanup events visible
- keep the compact row layout and push secondary controls into the detail panel

### Out of scope

This phase does not add:

- git status in the dashboard
- open in editor or open path actions
- favorites, recents, or saved workspace groups
- a new persisted lifecycle enum unless the implementation proves it is needed

## Scope

### In scope

- define and document worktree lifecycle semantics
- make watcher and session behavior observable through tests and dashboard state
- surface blocked cleanup reasons in dashboard views
- expand lifecycle tests around missing paths, active sessions, watcher reuse, and delayed prune
- dogfood the behavior against real worktree flows

### Out of scope

- editor launch
- git status
- broad dashboard redesign
- changes to the hard-C command surface beyond lifecycle clarity

## File Map

### Lifecycle and cleanup

- `src/tools/workspace/commands/registry/cleanup.rs`
- `src/tools/workspace/commands/registry/open.rs`
- `src/tools/workspace/commands/registry/register_remove.rs`
- `src/daemon/watcher_pool.rs`
- `src/daemon/workspace_pool.rs`
- `src/daemon/session.rs`
- `src/handler.rs`

### Dashboard state and rendering

- `src/dashboard/routes/projects.rs`
- `src/dashboard/routes/projects_actions.rs`
- `src/dashboard/state.rs`
- `dashboard/templates/projects.html`
- `dashboard/templates/partials/project_table.html`
- `dashboard/templates/partials/project_row.html`
- `dashboard/templates/partials/project_detail.html`
- `dashboard/templates/partials/project_cleanup_log.html`
- `dashboard/static/app.css`

### Tests and docs

- `src/tests/daemon/workspace_cleanup.rs`
- `src/tests/daemon/ipc_session.rs`
- `src/tests/integration/daemon_lifecycle.rs`
- `src/tests/dashboard/projects_actions.rs`
- `src/tests/dashboard/integration.rs`
- `docs/WORKSPACE_ARCHITECTURE.md`
- `TODO.md`

## Design Details

### 1. Worktree lifecycle contract

Document the daemon rules in product terms:

- known workspace paths may disappear
- disappearance does not imply immediate prune
- active sessions and watcher refs block cleanup
- missing inactive workspaces become prunable
- missing active workspaces remain visible until liveness drains

The implementation may continue deriving these states on demand unless repeated call sites make a dedicated helper or view-model object cleaner.

### 2. Watcher reuse and detach behavior

Strengthen tests around the watcher pool and workspace pool interaction:

- same workspace opened from multiple sessions reuses pooled state
- watcher refcount increments and decrements with session attachment
- last disconnect enters grace period instead of deleting the watcher immediately
- cleanup waits for refcount and in-flight indexing to drain

This phase should not change the grace-window model unless tests show a real bug.

### 3. Dashboard truthfulness

Upgrade dashboard state text so a user can answer three questions without reading logs:

1. Is this workspace usable right now?
2. If not, is it missing, indexing, or blocked?
3. If cleanup is blocked, what is holding it live?

The row view stays compact. The detail panel is where blocked-reason and cleanup detail belong.

### 4. Verification as a product feature

The remaining risk is not missing code, it is missing scenario coverage. This phase should prove the lifecycle with scenario-driven tests:

- delete worktree while inactive
- delete worktree while active
- reconnect from another session and confirm reuse
- reopen a stale missing row
- clean orphan directories

The final dogfood pass should repeat the same flows with real worktrees to confirm the dashboard and daemon agree.

## Acceptance Criteria

- [ ] Worktree lifecycle rules are documented in `docs/WORKSPACE_ARCHITECTURE.md`
- [ ] Missing active workspaces stay visible and explain why cleanup is blocked
- [ ] Missing inactive workspaces prune through the shared cleanup path
- [ ] Watcher reuse and delayed detach behavior are covered by tests
- [ ] Dashboard rows and detail panels distinguish current, active, known, missing, and blocked cleanup states
- [ ] Scenario-driven daemon and dashboard tests cover the remaining lifecycle flows
