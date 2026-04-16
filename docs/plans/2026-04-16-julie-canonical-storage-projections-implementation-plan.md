# Julie Canonical Storage And Projections Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use @razorback:executing-plans to implement this plan task-by-task.

**Goal:** Make SQLite the single canonical source of indexed truth and turn Tantivy, vectors, and analysis outputs into revisioned, rebuildable projections.

**Architecture:** Introduce canonical revision tracking in the database layer, split canonical persistence from projection work, and teach search plus repair flows to reason about projection freshness. Preserve the current product behavior while deleting restart-coupled repair paths.

**Tech Stack:** Rust, rusqlite, sqlite-vec, Tantivy

---

**Execution rules:** Use @razorback:test-driven-development and @razorback:verification-before-completion. Reproduce write-path and projection-drift bugs with targeted tests before refactoring schema or bulk-write paths.

### Task 1: Add Canonical Revision Tracking To The Database Layer

**Files:**
- Create: `src/database/revisions.rs`
- Modify: `src/database/mod.rs:15-57`
- Modify: `src/database/migrations.rs`
- Modify: `src/database/schema.rs`
- Modify: `src/database/workspace.rs:8-78`
- Test: `src/tests/core/incremental_update_atomic.rs`
- Modify: `src/tests/mod.rs`

**What to build:** Add revision metadata for canonical workspace writes so downstream projections can state which canonical revision they reflect. This revision ledger should live in the database layer and be cheap to query from repair, health, and search paths.

**Approach:** Keep the first pass narrow: introduce revision tables or revision metadata in migrations and expose it through `SymbolDatabase` rather than scattering SQL through callers. Make workspace cleanup and stats paths aware of the revision ledger from day one.

**Acceptance criteria:**
- [ ] canonical writes record a revision that can be queried per workspace
- [ ] migrations initialize revision state for fresh and rebuilt workspaces
- [ ] workspace cleanup handles revision metadata cleanly
- [ ] revision tests pin fresh write, incremental write, and cleanup behavior

### Task 2: Split Canonical Persistence From Projection Work

**Files:**
- Modify: `src/database/bulk_operations.rs:596-1352`
- Modify: `src/tools/workspace/indexing/processor.rs:18-511`
- Modify: `src/tools/workspace/indexing/index.rs:316-509`
- Test: `src/tests/integration/bulk_storage_atomicity.rs`
- Test: `src/tests/tools/workspace/processor.rs`

**What to build:** Stop treating canonical persistence and projection work as one transaction-shaped blob. Canonical writes should commit revision `N`; projection work should then advance Tantivy, vectors, and analysis outputs toward `N` with explicit lag reporting.

**Approach:** Keep atomicity on canonical writes, then push projection work into a second, explicit phase. Preserve safety first; shrinking `bulk_operations.rs` is a result of better structure, not the goal by itself.

**Acceptance criteria:**
- [ ] canonical write paths commit without depending on Tantivy success
- [ ] projection work knows which canonical revision it is trying to reach
- [ ] write-path tests pin behavior when projection work fails after a canonical commit
- [ ] `processor.rs` no longer owns canonical storage, projection, and analysis as one inseparable flow

### Task 3: Rework Tantivy Into A Revisioned Projection

**Files:**
- Create: `src/search/projection.rs`
- Modify: `src/search/index.rs:132-759`
- Modify: `src/search/mod.rs:6-26`
- Modify: `src/tools/workspace/indexing/index.rs:316-509`
- Test: `src/tests/tools/search/tantivy_index_tests.rs`
- Create: `src/tests/integration/projection_repair.rs`
- Modify: `src/tests/mod.rs`

**What to build:** Make Tantivy track projection freshness against canonical revisions and repair itself without daemon restart. Search should know whether it is serving the latest revision, a lagging revision, or a rebuilding projection.

**Approach:** Keep `SearchIndex` focused on search and low-level writes, then move revision and repair policy into `projection.rs`. Use the new projection tests to cover lag, rebuild, stale schema, and repair-after-delete flows.

**Acceptance criteria:**
- [ ] Tantivy stores or reports the canonical revision it reflects
- [ ] repair code can rebuild Tantivy to revision `N` without daemon restart
- [ ] search tests cover stale schema and revision lag through the projection layer
- [ ] integration tests cover projection rebuild after canonical state survives a failed projection pass

### Task 4: Surface Projection Freshness And Repair Through Health

**Files:**
- Modify: `src/health.rs:13-242`
- Modify: `src/tools/workspace/commands/registry/health.rs:11-262`
- Modify: `src/dashboard/state.rs:41-164`
- Modify: `src/dashboard/routes/status.rs:11-87`
- Test: `src/tests/integration/system_health.rs`
- Test: `src/tests/tools/search/tantivy_index_tests.rs`

**What to build:** Teach health and dashboard surfaces to report canonical revision, Tantivy revision, vector revision, and lag or repair-needed state per workspace.

**Approach:** Reuse the shared health vocabulary from the program plan. Keep the first pass operational: lag counts, stale markers, and repair-needed flags are worth more than polished phrasing.

**Acceptance criteria:**
- [ ] health output reports projection freshness relative to canonical revision
- [ ] dashboard shows projection lag and repair-needed state
- [ ] search and system-health tests pin stale, lagging, and repaired projection states
- [ ] restart is no longer the repair boundary for Tantivy drift

