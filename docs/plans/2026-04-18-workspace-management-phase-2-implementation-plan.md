# Workspace Management Phase 2 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:team-driven-development (on Claude Code) or razorback:subagent-driven-development (elsewhere) to implement this plan. Fall back to razorback:executing-plans for single-task or tightly-sequential plans.

**Goal:** Finish the remaining workspace-management work by locking down worktree lifecycle semantics, surfacing blocked cleanup state in the dashboard, and proving the behavior with scenario-driven tests and dogfooding.

**Architecture:** Keep the hard-C model intact and treat worktrees as normal known workspaces with shorter-lived paths. Build one lifecycle layer on top of the existing cleanup, watcher, and session machinery, then teach the dashboard to render those states honestly. Finish with scenario coverage and docs so the product contract matches the code.

**Tech Stack:** Rust, Tokio, Axum, Tera, rusqlite, dashboard templates, daemon pools

---

**Execution rules:** Use @razorback:test-driven-development and @razorback:systematic-debugging. This is a light plan. Run narrow tests during each task, then `cargo xtask test changed`, then `cargo xtask test dev` once after the batch.

### Task 1: Lock Down Worktree Lifecycle And Watcher Semantics

**Files:**
- Modify: `src/tools/workspace/commands/registry/cleanup.rs`
- Modify: `src/tools/workspace/commands/registry/open.rs`
- Modify: `src/tools/workspace/commands/registry/register_remove.rs`
- Modify: `src/daemon/watcher_pool.rs`
- Modify: `src/daemon/workspace_pool.rs`
- Modify: `src/daemon/session.rs`
- Modify: `src/handler.rs`
- Test: `src/tests/daemon/workspace_cleanup.rs`
- Test: `src/tests/daemon/ipc_session.rs`
- Test: `src/tests/integration/daemon_lifecycle.rs`

**What to build:** Make the remaining worktree lifecycle rules explicit in code paths and tests. A missing inactive worktree should prune. A missing active worktree should stay visible and report the blocking reason. Watcher reuse and delayed detach across sessions should be covered by focused tests instead of implied by current helper code.

**Approach:** Keep the existing cleanup engine and watcher grace-window model. Add or extract lifecycle helpers only where the current code repeats the same liveness reasoning. Strengthen the cleanup result surface so callers can distinguish prunable, blocked, and missing cases without re-deriving them ad hoc. Expand daemon tests to cover delete-while-active, delete-while-inactive, and cross-session reuse.

**Acceptance criteria:**
- [ ] missing inactive workspaces prune through the shared cleanup path
- [ ] missing active workspaces remain visible and report why cleanup is blocked
- [ ] watcher and pooled workspace reuse across multiple sessions are covered by tests
- [ ] grace-window detach behavior stays intact and tested
- [ ] lifecycle-related daemon tests pass, committed

### Task 2: Surface Lifecycle Truth On The Projects Dashboard

**Files:**
- Modify: `src/dashboard/routes/projects.rs`
- Modify: `src/dashboard/routes/projects_actions.rs`
- Modify: `src/dashboard/state.rs`
- Modify: `dashboard/templates/projects.html`
- Modify: `dashboard/templates/partials/project_table.html`
- Modify: `dashboard/templates/partials/project_row.html`
- Modify: `dashboard/templates/partials/project_detail.html`
- Modify: `dashboard/templates/partials/project_cleanup_log.html`
- Modify: `dashboard/static/app.css`
- Test: `src/tests/dashboard/projects_actions.rs`
- Test: `src/tests/dashboard/integration.rs`

**What to build:** Teach the Projects page to distinguish `current`, `active`, `known`, `missing`, and blocked cleanup states without forcing users to infer daemon behavior. Keep the compact row layout and place richer lifecycle detail in the expanded panel.

**Approach:** Extend the existing `ProjectWorkspaceView` and session-state derivation so the UI can render a blocked cleanup reason and a clearer missing-path state. Reuse daemon state that already exists instead of inventing new dashboard-only heuristics. Keep the quick inline `Open` or `Prune` action, keep secondary controls in the detail panel, and make the recent cleanup view explain blocked or completed cleanup outcomes.

**Acceptance criteria:**
- [ ] project rows remain compact and laptop-width friendly
- [ ] detail panels surface blocked cleanup reason when a workspace cannot be pruned yet
- [ ] missing-path workspaces are visually distinct from healthy inactive workspaces
- [ ] dashboard tests cover the new lifecycle states and compact-row behavior
- [ ] dashboard tests pass, committed

### Task 3: Verify With Scenario Coverage And Dogfood The Real Flows

**Files:**
- Modify: `src/tests/daemon/workspace_cleanup.rs`
- Modify: `src/tests/integration/daemon_lifecycle.rs`
- Modify: `src/tests/dashboard/projects_actions.rs`
- Modify: `docs/WORKSPACE_ARCHITECTURE.md`
- Modify: `TODO.md`

**What to build:** Finish the list with scenario-driven verification and doc updates. The remaining work should end with a documented lifecycle model and tests that mirror the real worktree flows users care about.

**Approach:** Add scenario tests that cover the product contract end to end instead of only helper-level assertions. Update `WORKSPACE_ARCHITECTURE.md` to describe watcher ownership, missing-path behavior, blocked cleanup, and prune timing in the post-hard-C model. Mark the original list items complete or partially complete in `TODO.md` with a short note on what remains out of scope.

**Acceptance criteria:**
- [ ] docs explain the worktree lifecycle and watcher rules in current product language
- [ ] scenario-driven tests cover inactive delete, active delete, stale reopen, and watcher reuse
- [ ] the original workspace-management TODO items are updated to reflect what this phase completed
- [ ] `cargo xtask test changed` and `cargo xtask test dev` pass, committed

## Final Verification

- Run the narrowest lifecycle and dashboard tests during each task
- Run `cargo xtask test changed`
- Run `cargo xtask test dev`
- Dogfood with real worktrees:
  - create a worktree
  - register or open it
  - attach a second session
  - delete the worktree path
  - confirm dashboard state flips to missing and blocked while active
  - disconnect sessions and confirm cleanup removes the row and index

## Review Gate

- Request a pre-merge reviewer choice after plan approval
- Use razorback execution only after the user approves this plan
