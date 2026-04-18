# Workspace Management Hard C Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use @razorback:subagent-driven-development to implement this plan. Fall back to @razorback:executing-plans for single-task or tightly-sequential work.

**Goal:** Replace `reference workspace` semantics with a clean `register` plus `open` model, add automatic stale-workspace pruning, and turn the dashboard Projects page into the control surface for workspace management.

**Architecture:** Cut the work in four slices. First, replace the public command surface so `register` is the non-activating front door and `open` remains the activation boundary. Second, remove pairing storage and add one shared cleanup engine plus a capped cleanup-event log. Third, add dashboard mutation routes and a tighter Projects UI that reflects known, active, and current workspace state. Fourth, purge stale `reference workspace` terminology from on-path helpers, tests, and docs so the old model stops leaking back in.

**Tech Stack:** Rust, Tokio, Axum, Tera, HTMX, rusqlite, Tantivy, watcher pool

---

**Execution rules:** Use @razorback:test-driven-development and @razorback:systematic-debugging. This is a light plan for same-session implementation. Run narrow tests during each task, then `cargo xtask test changed`, then `cargo xtask test dev` once after the full batch.

### Task 1: Replace `add` With `register`

**Files:**
- Create: `src/tools/workspace/commands/registry/register_remove.rs`
- Delete: `src/tools/workspace/commands/registry/add_remove.rs`
- Modify: `src/tools/workspace/commands/mod.rs:18-184`
- Modify: `src/tools/workspace/commands/registry/mod.rs:1-10`
- Modify: `src/handler.rs:2642-2667`
- Test: `src/tests/tools/workspace/management_token.rs`
- Test: `src/tests/tools/workspace/global_targeting.rs`
- Test: `src/tests/tools/workspace/mod_tests.rs`

**What to build:** Replace the public `manage_workspace(operation="add")` path with `manage_workspace(operation="register")`, remove the primary-pairing requirement, and make `register` index or refresh a workspace without activating it for the current session.

**Approach:** Move the current add and remove logic into `register_remove.rs` so the file name no longer bakes in the dead command. Update `WorkspaceCommand`, `ManageWorkspaceTool`, schema comments, and operation dispatch to accept `register` and reject `add`. In the register path, canonicalize before workspace-id generation, require daemon mode, treat an existing known workspace as a refresh-in-place path, and return clear failures for bad paths or indexing errors without leaving half-baked rows behind. Keep `open` unchanged as the only activation boundary.

**Acceptance criteria:**
- [ ] `ManageWorkspaceTool` accepts `register` and rejects `add`
- [ ] `register` indexes or refreshes a workspace without activating it and without requiring a primary workspace binding
- [ ] register output, schema comments, and command docs stop using `reference workspace` language
- [ ] narrow workspace command tests pass, committed

### Task 2: Remove Pairing Storage And Add Shared Cleanup

**Files:**
- Create: `src/tools/workspace/commands/registry/cleanup.rs`
- Modify: `src/tools/workspace/commands/registry/mod.rs:1-10`
- Modify: `src/daemon/database.rs:59-167`
- Modify: `src/daemon/database.rs:410-784`
- Modify: `src/daemon/mod.rs`
- Modify: `src/tools/workspace/commands/registry/list_clean.rs:9-185`
- Modify: `src/tools/workspace/commands/registry/open.rs:29-151`
- Modify: `src/tools/workspace/commands/registry/refresh_stats.rs:228-326`
- Modify: `src/tools/workspace/commands/registry/register_remove.rs`
- Modify: `src/workspace/registry.rs`
- Test: `src/tests/daemon/database.rs`
- Create: `src/tests/daemon/workspace_cleanup.rs`
- Modify: `src/tests/daemon/ipc_session.rs`
- Modify: `src/tests/daemon/mod.rs`
- Modify: `src/tests/integration/daemon_lifecycle.rs`
- Modify: `src/tests/tools/workspace/registry.rs`

**What to build:** Remove `workspace_references` from daemon storage, add a capped cleanup-event log, and route manual cleanup, opportunistic cleanup, and lazy daemon sweep behavior through one shared prune decision path. Manual delete must refuse active or in-flight workspaces.

**Approach:** Add a forward daemon-db migration that drops `workspace_references` and creates a `workspace_cleanup_events` table. Do not rewrite the historical migrations. Put prune eligibility and prune execution in `cleanup.rs`, then call that helper from `list_clean.rs`, `open.rs`, and delete handling so the same rules decide when missing-path workspaces can be removed. Use existing session-count and watcher-refcount data instead of inventing a second liveness model. Update `workspace/registry.rs` and its tests so any remaining stdio-era registry types stop encoding primary versus reference expiration semantics.

**Acceptance criteria:**
- [ ] old daemon installs migrate forward by dropping `workspace_references` and adding cleanup-event storage
- [ ] missing inactive workspaces are pruned through one shared path on list, open, explicit clean, and daemon sweep
- [ ] manual delete refuses active or indexing workspaces and records `manual_delete` events on success
- [ ] legacy registry structs and tests stop describing primary versus reference TTL behavior as a live model
- [ ] narrow daemon cleanup tests pass, committed

### Task 3: Turn Projects Into The Workspace Control Surface

**Files:**
- Create: `src/dashboard/routes/projects_actions.rs`
- Modify: `src/dashboard/routes/mod.rs`
- Modify: `src/dashboard/mod.rs:121-159`
- Modify: `src/dashboard/routes/projects.rs:141-377`
- Modify: `dashboard/templates/projects.html`
- Modify: `dashboard/templates/partials/project_summary.html`
- Modify: `dashboard/templates/partials/project_table.html`
- Modify: `dashboard/templates/partials/project_row.html`
- Modify: `dashboard/templates/partials/project_detail.html`
- Create: `dashboard/templates/partials/project_cleanup_log.html`
- Test: `src/tests/dashboard/integration.rs`
- Create: `src/tests/dashboard/projects_actions.rs`
- Modify: `src/tests/dashboard/mod.rs`
- Modify: `src/tests/dashboard/router.rs`

**What to build:** Add dashboard routes for `register`, `open`, `refresh`, and `delete`, add an `Add Workspace` path input, surface current and active state in the Projects table and detail panel, and show a compact recent cleanup log. Remove reference tags from the dashboard.

**Approach:** Keep read-only rendering in `projects.rs`, and put mutation handlers in `projects_actions.rs` so the dashboard route code stays under the repo’s file-size limit. Reuse the `ManageWorkspaceTool` flows from Task 1 and Task 2 instead of duplicating indexing or activation logic in HTTP handlers. Update the templates to show current and active state, Add Workspace form submission, row action buttons, and recent cleanup events. Defer editor launch and git-state integration.

**Acceptance criteria:**
- [ ] Projects page supports Add Workspace, Open, Refresh, and Delete through dedicated dashboard routes
- [ ] Add Workspace registers and indexes without activating the workspace
- [ ] project rows and detail panels show current and active state with no reference-workspace tags
- [ ] dashboard shows a compact recent cleanup log sourced from cleanup events
- [ ] dashboard action tests pass, committed

### Task 4: Purge Stale Terminology And Update Cross-Workspace Tests

**Files:**
- Create: `src/tools/navigation/target_workspace.rs`
- Delete: `src/tools/navigation/reference_workspace.rs`
- Modify: `src/tools/navigation/mod.rs`
- Modify: `src/tools/navigation/fast_refs.rs:563-576`
- Create: `src/tests/tools/target_workspace_fast_refs_tests.rs`
- Delete: `src/tests/tools/reference_workspace_fast_refs_tests.rs`
- Create: `src/tests/tools/get_symbols_target_workspace.rs`
- Delete: `src/tests/tools/get_symbols_reference_workspace.rs`
- Create: `src/tests/integration/target_workspace.rs`
- Delete: `src/tests/integration/reference_workspace.rs`
- Modify: `src/tests/mod.rs`
- Modify: `src/tests/tools/workspace/global_targeting.rs`
- Modify: `docs/WORKSPACE_ARCHITECTURE.md`

**What to build:** Remove the stale `reference workspace` name from the cross-workspace navigation helper, rename the focused test modules that still preserve the old concept, and update architecture docs so the shipped language matches `register`, `open`, known workspaces, active workspaces, and auto-prune behavior.

**Approach:** Rename the helper module and the focused test files to `target workspace` or `known workspace` language while keeping the explicit `workspace=<id>` behavior intact. Update `fast_refs` to call the renamed helper, update test module registration in `src/tests/mod.rs`, and revise `WORKSPACE_ARCHITECTURE.md` examples and cleanup sections to match the new command surface and lifecycle rules. Leave deeper internal naming cleanup outside the touched paths alone.

**Acceptance criteria:**
- [ ] workspace-facing docs describe current, known, active, and target workspaces only
- [ ] explicit cross-workspace fast_refs and get_symbols tests still pass after the rename
- [ ] touched helper modules, test files, and test comments stop presenting `reference workspace` as a live concept
- [ ] docs and narrow terminology-regression tests pass, committed

## Final Verification

- Run the narrowest task tests during each slice
- Run `cargo xtask test changed`
- Run `cargo xtask test dev`
- Dogfood the dashboard flow:
  - add a workspace from Projects
  - open a known workspace from Projects
  - refresh a known workspace from Projects
  - delete an inactive workspace from Projects
  - remove a worktree on disk and confirm auto-prune removes its row and index

## Review Gate

- Request a pre-merge reviewer choice after implementation-plan approval
- Use @razorback:subagent-driven-development for execution unless the work collapses into one tightly-sequential slice
