# Julie World-Class Systems Program Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use @razorback:executing-plans to implement this plan task-by-task.

**Goal:** Land the cross-cutting contracts, health surfaces, harnesses, and phase gates that let the control-plane, indexing, storage, and embedding tracks move fast without drifting apart.

**Architecture:** Treat this as the program spine. Land shared health, lifecycle, revision, and dashboard contracts first; then run the subsystem track plans against those contracts. Keep SQLite canonical, projections rebuildable, and the dashboard tied to the same runtime truth the tools consume.

**Tech Stack:** Rust, Tokio, rusqlite, Tantivy, Axum/Tera dashboard, Python embeddings sidecar, xtask

---

**Execution rules:** Use @razorback:test-driven-development, @razorback:systematic-debugging, and @razorback:verification-before-completion on every task. Write failing targeted tests first, use narrow `cargo test --lib <name>` runs while iterating, and run `cargo xtask test dev` after each completed task batch in the main session.

### Task 1: Land Shared Health And Contract Vocabulary

**Files:**
- Modify: `src/health.rs:13-242`
- Modify: `src/workspace/mod.rs:461-461`
- Modify: `src/workspace/mod.rs:774-837`
- Modify: `src/tools/workspace/commands/registry/health.rs:11-262`
- Modify: `src/dashboard/state.rs:41-164`
- Modify: `src/dashboard/routes/status.rs:11-87`
- Test: `src/tests/dashboard/state.rs`
- Test: `src/tests/tools/workspace/mod_tests.rs`

**What to build:** Replace the narrow ready/not-ready health path with shared control-plane, data-plane, and runtime-plane health vocabulary that can represent daemon state, workspace state, projection lag, watcher state, and embedding degradation. Keep `src/health.rs` as the entry surface for the first pass; split into a directory only if file size and cohesion demand it.

**Approach:** Extend the existing `SystemStatus`, `PrimaryWorkspaceHealth`, and `WorkspaceHealth` surfaces into richer snapshot types instead of creating a second health system beside them. Make the dashboard and `manage_workspace(operation="health")` read the same snapshot so humans and tools see one truth.

**Acceptance criteria:**
- [ ] `src/health.rs` can express daemon lifecycle state, canonical-store health, projection freshness, watcher status, and embedding status
- [ ] `manage_workspace(operation="health")` reports the shared model instead of piecing together ad hoc strings
- [ ] dashboard state and live status route expose the same fields the health command uses
- [ ] tests pin ready, degraded, and unavailable states for dashboard and health output

### Task 2: Add Reliability And Benchmark Harness Entry Points

**Files:**
- Modify: `xtask/src/cli.rs:6-127`
- Modify: `xtask/src/runner.rs:206-564`
- Modify: `xtask/src/lib.rs:1-62`
- Create: `src/tests/integration/system_health.rs`
- Modify: `src/tests/mod.rs`

**What to build:** Add explicit xtask entry points for the new reliability and benchmark suites that will guard this program. The harness must cover daemon lifecycle, projection repair, indexing repair, and embedding degradation, while keeping output concise and aligned with the repo's existing xtask style.

**Approach:** Extend the current xtask parser and runner rather than bolting on custom shell scripts. Keep reliability scenarios as deterministic integration tests under `src/tests/integration/`, then use xtask to group them into buckets that map to the program phases.

**Acceptance criteria:**
- [ ] xtask exposes named entry points for reliability and benchmark runs tied to this program
- [ ] `src/tests/integration/system_health.rs` exercises shared health and repair scenarios
- [ ] new harness output stays concise and only expands on failure
- [ ] the new buckets can be added to branch-level verification without ad hoc commands

### Task 3: Promote The Dashboard Into The System Truth Surface

**Files:**
- Modify: `src/dashboard/state.rs:41-164`
- Modify: `src/dashboard/routes/status.rs:11-87`
- Modify: `src/handler.rs:1569-1608`
- Test: `src/tests/dashboard/integration.rs`
- Test: `src/tests/dashboard/state.rs`

**What to build:** Turn the dashboard into the visible face of the shared health contract. It should show lifecycle state, restart-required state, workspace count, projection freshness, embedding mode, and recent tool activity without forcing anyone into the logs.

**Approach:** Reuse the existing `DashboardEvent` channel and `record_tool_call` hook, then add structured status fields rather than free-form dashboard-only logic. Keep the first pass blunt and operational: summary cards, lag indicators, and recent events beat fancy charts.

**Acceptance criteria:**
- [ ] dashboard status page shows control-plane, data-plane, and runtime-plane health fields
- [ ] live polling response returns the same structured fields as the rendered page
- [ ] dashboard event stream includes enough signal to debug restart, indexing, and projection repair flows
- [ ] tests cover new status fields and live-route behavior

### Task 4: Enforce Phase Gates Across The Track Plans

**Files:**
- Review: `docs/plans/2026-04-16-julie-control-plane-lifecycle-implementation-plan.md`
- Review: `docs/plans/2026-04-16-julie-indexing-engine-implementation-plan.md`
- Review: `docs/plans/2026-04-16-julie-canonical-storage-projections-implementation-plan.md`
- Review: `docs/plans/2026-04-16-julie-embedding-runtime-implementation-plan.md`
- Test: `src/tests/integration/daemon_lifecycle.rs`
- Test: `src/tests/integration/stale_index_detection.rs`
- Test: `src/tests/tools/search/tantivy_index_tests.rs`
- Test: `src/tests/integration/sidecar_embedding_pipeline.rs`

**What to build:** Turn the phase boundaries from the design doc into execution gates. Track 1 and Track 2 cannot proceed without Task 1. Track 3 cannot land behind a fuzzy indexing contract. Track 4 cannot ship behind a silent health model.

**Approach:** Use the integration suites named above as gate checks, not as an afterthought. Treat this task as the program coordinator: make sure each subsystem lane reports back into the same test and dashboard surfaces before the next lane starts leaning on it.

**Acceptance criteria:**
- [ ] Track 1 and Track 2 start only after shared health and harness work lands
- [ ] Track 3 starts only after indexing state transitions are explicit and testable
- [ ] Track 4 reports through the shared health model before query-path changes are considered done
- [ ] branch-level verification has one clear gate per phase

### Task 5: Run Convergence And Dogfood Week

**Files:**
- Modify: `xtask/src/runner.rs:228-564`
- Test: `src/tests/integration/daemon_lifecycle.rs`
- Test: `src/tests/integration/stale_index_detection.rs`
- Test: `src/tests/integration/system_health.rs`
- Test: `src/tests/integration/sidecar_embedding_pipeline.rs`
- Test: `src/tests/tools/search/tantivy_index_tests.rs`

**What to build:** Add the final convergence pass that proves the program improved Julie in daily use. This pass should exercise rebuilds, restarts, degraded modes, projection repair, and the dashboard truth surface on the same branch, with generated state blown away when needed.

**Approach:** Treat this as a product pass, not a unit-test pass. Run the harnesses, rebuild generated state, dogfood Julie on real workflows, and cut dead repair code or compatibility scaffolding that no longer belongs after the redesign.

**Acceptance criteria:**
- [ ] convergence runs cover restart, rebuild, repair, and degraded-mode scenarios
- [ ] generated state can be discarded and rebuilt without hidden manual steps
- [ ] the dashboard explains system state during the convergence pass
- [ ] dogfooding notes identify remaining friction, not mysteries about state

