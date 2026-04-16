# Julie Indexing Engine Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use @razorback:executing-plans to implement this plan task-by-task.

**Goal:** Replace the current cluster of full-index, catch-up, and incremental indexing flows with one explicit indexing engine and one workspace-routing model.

**Architecture:** Add indexing state and routing modules under `src/tools/workspace/indexing/`, then refactor `index.rs`, `incremental.rs`, and `processor.rs` into stages that share one engine. Keep primary and reference workspaces on the same core path and report repair reasons through the shared health model.

**Tech Stack:** Rust, Tokio, tree-sitter extraction layer, rusqlite, Tantivy, file watcher

---

**Execution rules:** Use @razorback:test-driven-development and @razorback:systematic-debugging. Reproduce stale-index and wrong-workspace regressions with narrow tests first, then refactor toward shared routing and staged indexing.

### Task 1: Add Shared Workspace Routing For Index Jobs

**Files:**
- Create: `src/tools/workspace/indexing/route.rs`
- Modify: `src/tools/workspace/indexing/mod.rs`
- Modify: `src/tools/workspace/indexing/index.rs:35-308`
- Modify: `src/tools/workspace/indexing/incremental.rs:17-244`
- Modify: `src/startup.rs:55-290`
- Test: `src/tests/integration/stale_index_detection.rs`
- Test: `src/tests/tools/workspace/refresh_routing.rs`

**What to build:** Centralize the rules that decide which workspace ID, database, Tantivy directory, and watcher state belong to an index operation. Remove hot-path branches that recompute "am I primary or reference?" in multiple places.

**Approach:** Put routing and identity decisions in `route.rs`, return an immutable route snapshot, and make `index.rs`, `incremental.rs`, and `startup.rs` consume it. The route snapshot should carry enough information to remove ad hoc database opens and path rewrites inside the indexing hot path.

**Acceptance criteria:**
- [ ] full index, stale scan, and incremental scan all use the same routing snapshot
- [ ] primary and reference workspaces stop opening or selecting databases through duplicated logic
- [ ] stale-index detection tests cover rebound primary and reference-workspace routing through the shared route path
- [ ] route selection failures surface explicit repair reasons

### Task 2: Introduce Indexing State And Stage Boundaries

**Files:**
- Create: `src/tools/workspace/indexing/state.rs`
- Create: `src/tools/workspace/indexing/pipeline.rs`
- Modify: `src/tools/workspace/indexing/mod.rs`
- Modify: `src/tools/workspace/indexing/processor.rs:18-511`
- Modify: `src/tools/workspace/indexing/index.rs:35-308`
- Test: `src/tests/tools/workspace/processor.rs`
- Create: `src/tests/integration/indexing_pipeline.rs`
- Modify: `src/tests/mod.rs`

**What to build:** Add explicit file and batch states to the indexing engine, then split `process_files_optimized` into stage boundaries such as extract, persist, project, resolve, and analyze. The result should be one engine that reports where a batch is, not one monolith with side effects woven through it.

**Approach:** Keep the stage split inside `pipeline.rs` and `state.rs`, with `processor.rs` shrinking into stage implementations. Preserve current extraction behavior while making each stage observable and testable.

**Acceptance criteria:**
- [ ] indexing stages are represented explicitly and can be logged or surfaced through health
- [ ] `process_files_optimized` loses ownership of routing, state transitions, and cross-stage coordination
- [ ] integration tests can assert stage transitions for both parser-backed and text-only files
- [ ] file-level failures leave repair-needed state instead of silent fallbacks

### Task 3: Unify Full Index, Catch-Up, And Watcher Repair Paths

**Files:**
- Modify: `src/handler.rs:1652-1728`
- Modify: `src/tools/workspace/indexing/index.rs:35-509`
- Modify: `src/tools/workspace/indexing/incremental.rs:17-447`
- Modify: `src/startup.rs:55-290`
- Modify: `src/watcher/handlers.rs`
- Test: `src/tests/integration/stale_index_detection.rs`
- Test: `src/tests/integration/watcher.rs`
- Test: `src/tests/integration/watcher_handlers.rs`

**What to build:** Route cold start, catch-up scans, and watcher-driven updates through the same engine and repair model. Pause and resume logic should become engine policy, not scattered fixes around specific call sites.

**Approach:** Use the routing snapshot and indexing state from Tasks 1 and 2 to drive watcher and catch-up behavior. Remove cases where "reindex everything" is the emergency exit unless the engine records the reason and exposes it through health.

**Acceptance criteria:**
- [ ] `run_auto_indexing` becomes a thin orchestrator over the shared engine
- [ ] watcher pause and resume behavior is tied to engine state rather than one-off handler calls
- [ ] cold-start and watcher repair share repair reasons and state transitions
- [ ] watcher integration tests pin duplicate-event and stale-scan edge cases

### Task 4: Surface Indexing Repair And Lag Through Health

**Files:**
- Modify: `src/health.rs:13-242`
- Modify: `src/tools/workspace/commands/registry/health.rs:11-262`
- Modify: `src/dashboard/state.rs:41-164`
- Modify: `src/dashboard/routes/status.rs:11-87`
- Test: `src/tests/integration/system_health.rs`
- Test: `src/tests/tools/workspace/mod_tests.rs`

**What to build:** Make the indexing engine report dirty-file counts, failed files, paused watcher state, active catch-up, and repair-needed state through the shared health contract and dashboard.

**Approach:** Reuse the stage and route data from the earlier tasks. Keep the first UI pass blunt and operational so a person can answer "what is Julie indexing, and why is it blocked?" in one glance.

**Acceptance criteria:**
- [ ] health output includes indexing state, watcher state, dirty count, and repair-needed reason
- [ ] dashboard shows current indexing stage and queued repair work
- [ ] indexing failures are visible without log inspection
- [ ] health tests pin indexing-ready and indexing-degraded states

