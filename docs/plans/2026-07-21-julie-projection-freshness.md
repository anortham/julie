# Julie Projection Freshness Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Report deterministic health for the `tantivy` and `web_edges` projections, preserve Tantivy-only search readiness, and close the startup reconciliation gap without reviving the detached dashboard.

**Architecture:** Replace the Tantivy-specific health record with one `ProjectionHealth` output type interpreted through a projection-specific policy. `DataPlaneHealth` owns a stable `[tantivy, web_edges]` list; only the Tantivy entry controls search readiness, while every entry contributes to overall health. Startup repair reconciles both projections after every successful outcome under a `MutationGuard<'_>` proof token.

**Tech Stack:** Rust, Tokio, SQLite projection metadata, Tantivy, rmcp JSON serialization, Tera dashboard templates, cargo-nextest, Julie xtask test tiers.

**Architecture Quality:** Medium-risk contract migration across health output and a high-value startup safety fix. Projection mechanics remain explicit behind one interpreter; no duplicate health structures or temporary compatibility field are allowed.

## Global Constraints

- `DataPlaneHealth.projections` is always ordered `tantivy`, then `web_edges`.
- Search readiness depends on canonical SQLite plus the current, physically ready Tantivy projection only.
- A lagging or rebuild-required `web_edges` projection degrades overall health and names `web_edges`, but search remains fully ready when Tantivy is current.
- `ProjectionHealth` contains `name`, `level`, `state`, `freshness`, `workspace_id`, `canonical_revision`, `projected_revision`, `revision_lag`, `repair_needed`, and `detail`.
- Missing canonical revision metadata is a canonical-store repair condition; projection detail must not claim a projection rebuild can create it.
- Tantivy policy injects a physical-readiness probe and keeps the legacy `Ready`-row revision fallback; `web_edges` relies on its durable row and never inherits those Tantivy-only rules.
- Projection reconciliation is a writer and requires a `&MutationGuard<'_>` proof token once a workspace identity exists.
- The dashboard remains detached and unavailable. This phase migrates its compile-only shape and templates without adding a database pool, live index handle, or in-process dependency.
- Every new implementation file remains at or below 500 lines. Task 3 brings the touched legacy `src/dashboard/state.rs` from 503 lines to at most 500 by simplifying only the regions already being changed; tests stay under `src/tests/` and fixtures under `fixtures/`.
- Follow TDD: one exact RED run and one exact GREEN run per worker slice. Workers never run xtask tiers or unfiltered `cargo nextest run --lib`.

---

## File Map

- `src/startup.rs` — gate-proved projection reconciliation after every successful startup repair outcome.
- `src/health/types.rs` — public `ProjectionHealth` and `DataPlaneHealth.projections` contract.
- `src/health/projection.rs` — shared durable-row interpreter plus explicit Tantivy/web-edge policy.
- `src/health/data_plane.rs` — deterministic projection construction and overall-health aggregation.
- `src/health/evaluation.rs` — Tantivy-only search-readiness decision.
- `src/health/checker.rs` — concise status text that names the degraded projection.
- `src/health/report.rs` — stable per-projection detailed rendering.
- `src/health/mod.rs` — exports for the new contract and internal policy.
- `src/dashboard/state.rs` — compile-only detached dashboard projection list.
- `dashboard/templates/partials/services_panel.html` — server-rendered rows for both projection names.
- `dashboard/templates/status.html` — live JSON updates for the projection array.
- `src/tests/tools/workspace/mod_tests/part6.rs` — MissingEmbeddings-only startup regression.
- `src/tests/integration/system_health.rs` and `src/tests/tools/workspace/mod_tests/part2.rs` — live health/readiness contract tests.
- `src/tests/dashboard/state.rs` and `src/tests/dashboard/integration/status.rs` — detached dashboard JSON/template contract tests.
- `docs/plans/2026-07-21-julie-projection-freshness-verification.md` — commit-SHA-bound evidence.

## Architecture Quality

- `ProjectionHealth` is the only serialized projection shape; do not retain `SearchProjectionHealth` or a `search_projection` compatibility field.
- Introduce `ProjectionPolicy` in `src/health/projection.rs` with explicit variants `Tantivy { physical_ready: bool }` and `WebEdges`. The policy owns the stable name, physical-readiness semantics, legacy revision fallback, and detail wording.
- Add `DataPlaneHealth::projection(&self, name: &str) -> Option<&ProjectionHealth>` so readiness and status code select Tantivy by the durable `TANTIVY_PROJECTION_NAME` constant instead of vector position.
- Keep the standalone-dashboard seam future-proof: a later on-disk Tantivy probe can supply the same `physical_ready` input without changing durable-row interpretation.
- Reconciliation remains in startup ownership. The health layer observes and explains state; it does not mutate or repair projections.
- If live code contradicts these boundaries, report a plan mismatch rather than introducing a duplicate interpreter or dashboard dependency.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, and `docs/plans/verification-ledger-template.md`.

**Worker red/green scope:** Each worker runs only the exact test named in its task with `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`, once for RED and once for GREEN.

**Worker ceiling:** Exact named tests only, at most two runs per slice. Workers do not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test system`, or any unfiltered nextest command.

**Worker gate invariant:** Task 1 proves every successful startup repair converges both projections under the mutation gate; Task 2 proves web-edge degradation affects overall health but not Tantivy search readiness; Task 3 proves the detached dashboard serializes and renders both unavailable projection records without gaining a live dependency.

**Lead affected-change scope:** Run `cargo xtask test changed` after Tasks 1-3 form one coherent batch. If it reports OverBudget, record that result and run `cargo xtask test changed --scale` as the explicit mapped-plus-dev escalation.

**Branch gate:** Run `cargo fmt --check`, `cargo check`, `cargo xtask test dev`, and `cargo xtask test system` once at the final code commit.

**Replay/metric evidence:** Projection order, names, revisions, overall level, readiness, and repair ownership are hard assertions. Warm/cold timing and the known macOS object-version warning are report-only evidence for Phase 4.

**Escalation triggers:** Any search/scoring behavior change requires `cargo xtask test dogfood`; any new watcher/runtime mutation requires `cargo xtask test reliability`; neither is expected in this plan.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp in `docs/plans/2026-07-21-julie-projection-freshness-verification.md`. Reuse evidence only when the required scope label and current HEAD SHA both match.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Gate-proved startup convergence | None - serial | `src/startup.rs`; `src/tests/tools/workspace/mod_tests/part6.rs` | Yes | Repository rules allow only one Cargo test command at a time; finish the risk-first repair slice before the contract migration. |
| Task 2: Live generic projection health | None - serial | `src/health/types.rs`; `src/health/projection.rs`; `src/health/data_plane.rs`; `src/health/evaluation.rs`; `src/health/checker.rs`; `src/health/report.rs`; `src/health/mod.rs`; `src/tests/integration/system_health.rs`; `src/tests/tools/workspace/mod_tests/part2.rs` | Yes | The serialized type replacement must land atomically with every live consumer so each task ends compilable. |
| Task 3: Detached dashboard migration | None - serial | `src/dashboard/state.rs`; `dashboard/templates/partials/services_panel.html`; `dashboard/templates/status.html`; `src/tests/dashboard/state.rs`; `src/tests/dashboard/integration/status.rs` | Yes | Depends on Task 2's exact `ProjectionHealth` and `projections` contracts. |
| Task 4: Lead verification and evidence | None - serial | `docs/plans/2026-07-21-julie-projection-freshness.md`; `docs/plans/2026-07-21-julie-projection-freshness-verification.md` | Yes | Runs only after all production/test slices are committed and owns the final SHA-bound evidence. |

### Task 1: Gate-Proved Startup Convergence

**Files:**
- Modify: `src/startup.rs:77-367`
- Test: `src/tests/tools/workspace/mod_tests/part6.rs:191-296`

**Interfaces:**
- Consumes: `MutationGuard<'_>`, `JulieServerHandler::acquire_mutation_gate`, `run_primary_workspace_repair_body`, and the existing `ensure_web_edges_current`/Tantivy reconciliation path.
- Produces: `reconcile_projection_lag_if_needed(_guard: &MutationGuard<'_>, handler: &JulieServerHandler) -> Result<()>` plus a post-repair helper that reuses an existing guard or acquires the newly resolved workspace gate.

**Contract inputs:** Successful `Some(plan)` and `None` outcomes both reconcile; failed repair outcomes do not. First-run repair may begin without an identity but must resolve and gate before post-repair writes.

**File ownership:** `src/startup.rs`; `src/tests/tools/workspace/mod_tests/part6.rs`

**Serialization required:** Yes

**Dependency reason:** Repository rules allow only one Cargo test command at a time; finish the risk-first repair slice before the contract migration.

**What to build:** Add `test_startup_missing_embeddings_only_repair_reconciles_web_edges`. Seed a clean indexed workspace, make only the `web_edges` projection row lag, clear embeddings, prove the plan reason is `MissingEmbeddings`, and assert startup repair restores both projection revisions. Move reconciliation after every successful repair result and require the proof token at the mutation helper boundary.

**Approach:** Keep the outer gate for already-bound workspaces. When `run_primary_workspace_repair_body` began without a guard, resolve `require_primary_workspace_identity()` after indexing, acquire that gate, then reconcile. Preserve follower no-op behavior and avoid gate re-entry.

**Acceptance criteria:**
- [x] `cargo nextest run --lib test_startup_missing_embeddings_only_repair_reconciles_web_edges 2>&1 | tail -10` fails before implementation and passes after it.
- [x] Both `tantivy` and `web_edges` equal the canonical revision after a MissingEmbeddings-only repair.
- [x] Reconciliation cannot compile without a `MutationGuard<'_>` proof token once a workspace identity exists.
- [x] Worker-scope verification passes and the worker creates a `serial-worker-commit` commit.

### Task 2: Live Generic Projection Health

**Files:**
- Modify: `src/health/types.rs:50-82`
- Modify: `src/health/projection.rs:1-193`
- Modify: `src/health/data_plane.rs:11-174`
- Modify: `src/health/evaluation.rs:26-42`
- Modify: `src/health/checker.rs:158-227`
- Modify: `src/health/report.rs:5-274`
- Modify: `src/health/mod.rs:14-23`
- Test: `src/tests/integration/system_health.rs:145-333`
- Test: `src/tests/tools/workspace/mod_tests/part2.rs:140-258`

**Interfaces:**
- Consumes: `TANTIVY_PROJECTION_NAME`, `WEB_EDGES_PROJECTION_NAME`, `ProjectionStatus`, canonical revision rows, and the live `search_index_ready` signal.
- Produces: public `ProjectionHealth`; internal `ProjectionPolicy::{Tantivy { physical_ready }, WebEdges}`; `projection_health_for_workspace(workspace_id, db, symbol_count, policy) -> Result<ProjectionHealth>`; `DataPlaneHealth.projections`; `DataPlaneHealth::projection(name)`.

**Contract inputs:** Stable order and fields from Global Constraints; web-edge lag contributes to overall level but never to `readiness_from_data_plane`; missing canonical metadata names canonical-store repair ownership.

**File ownership:** `src/health/types.rs`; `src/health/projection.rs`; `src/health/data_plane.rs`; `src/health/evaluation.rs`; `src/health/checker.rs`; `src/health/report.rs`; `src/health/mod.rs`; `src/tests/integration/system_health.rs`; `src/tests/tools/workspace/mod_tests/part2.rs`

**Serialization required:** Yes

**Dependency reason:** The serialized type replacement must land atomically with every live consumer so each task ends compilable.

**What to build:** Replace `SearchProjectionHealth` and `search_projection` with the generic list and policy-driven interpreter. Construct both records in cold-start, error, and ready paths. Update readiness, concise status, and detailed report code to select Tantivy by name while naming any degraded non-search projection.

**Approach:** Add `test_system_health_web_edge_lag_degrades_overall_without_closing_search`. Keep existing Tantivy loss/lag tests but change them to select by name. Add assertions for stable list order, current Tantivy, lagging web edges, degraded overall level, fully ready search, and canonical-store wording when revision metadata is absent.

**Acceptance criteria:**
- [ ] `cargo nextest run --lib test_system_health_web_edge_lag_degrades_overall_without_closing_search 2>&1 | tail -10` fails before implementation and passes after it.
- [ ] JSON and report output contain exactly one `tantivy` and one `web_edges` record in stable order.
- [ ] Tantivy loss/lag still produces SQLite-only readiness; web-edge loss/lag leaves search fully ready but degrades overall health and names `web_edges`.
- [ ] Empty workspaces request no repair; missing canonical metadata points to canonical-store repair.
- [ ] No `SearchProjectionHealth` type or `search_projection` compatibility field remains.
- [ ] Worker-scope verification passes and the worker creates a `serial-worker-commit` commit.

### Task 3: Detached Dashboard Migration

**Files:**
- Modify: `src/dashboard/state.rs:13-16,66-79,236-414,479-493`
- Modify: `dashboard/templates/partials/services_panel.html:104-146`
- Modify: `dashboard/templates/status.html:119-147`
- Test: `src/tests/dashboard/state.rs:437-531`
- Test: `src/tests/dashboard/integration/status.rs:133-232`

**Interfaces:**
- Consumes: Task 2's `ProjectionHealth` fields and deterministic `projections` list.
- Produces: `DashboardDataPlaneHealth.projections: Vec<ProjectionHealth>` containing detached `tantivy` and `web_edges` unavailable records; HTML/JavaScript keyed by projection name.

**Contract inputs:** The dashboard pool remains detached; every projection reports `Unavailable`, `repair_needed = false`, and detail explaining that projection visibility is unavailable until the standalone reader exists.

**File ownership:** `src/dashboard/state.rs`; `dashboard/templates/partials/services_panel.html`; `dashboard/templates/status.html`; `src/tests/dashboard/state.rs`; `src/tests/dashboard/integration/status.rs`

**Serialization required:** Yes

**Dependency reason:** Depends on Task 2's exact `ProjectionHealth` and `projections` contracts.

**What to build:** Mechanically migrate the dashboard state, `/status/live` JSON, services panel, and live-update JavaScript from one `search_projection` object to named projection rows. Remove stale tests that imply the detached dashboard can observe a live Tantivy lag; replace them with explicit detached-contract assertions.

**Approach:** Rename the integration test to `test_status_live_exposes_projection_list_contract` and use it for RED/GREEN. Render stable DOM identifiers from the projection name and update rows by name rather than array position.

**Acceptance criteria:**
- [ ] `cargo nextest run --lib test_status_live_exposes_projection_list_contract 2>&1 | tail -10` fails before implementation and passes after it.
- [ ] `/status/live` exposes two named unavailable records and no `search_projection` field.
- [ ] The status page renders and updates distinct Tantivy and web-edge rows without duplicate DOM IDs.
- [ ] No live workspace database or Tantivy handle is added to `DashboardState`.
- [ ] `src/dashboard/state.rs` is at most 500 lines without an unrelated module move.
- [ ] Worker-scope verification passes and the worker creates a `serial-worker-commit` commit.

### Task 4: Lead Verification and Evidence

**Files:**
- Modify: `docs/plans/2026-07-21-julie-projection-freshness.md`
- Modify: `docs/plans/2026-07-21-julie-projection-freshness-verification.md`

**Interfaces:**
- Consumes: committed outputs from Tasks 1-3 and `docs/plans/verification-ledger-template.md`.
- Produces: checked task boxes plus commit-SHA-bound affected-change and branch-gate evidence.

**Contract inputs:** Evidence reuse requires the same scope label and exact HEAD SHA; the known macOS object-version warning is report-only and remains assigned to Phase 4 of the roadmap.

**File ownership:** `docs/plans/2026-07-21-julie-projection-freshness.md`; `docs/plans/2026-07-21-julie-projection-freshness-verification.md`

**Serialization required:** Yes

**Dependency reason:** Runs only after all production/test slices are committed and owns the final SHA-bound evidence.

**What to build:** Review the combined diff with Miller impact, run the required lead gates once, record every command at the exact code HEAD, then update task acceptance boxes without changing production behavior.

**Approach:** Run `cargo fmt --check`, `cargo check`, `cargo xtask test changed` with the documented OverBudget escalation, `cargo xtask test dev`, and `cargo xtask test system`. Add dogfood or reliability only if an escalation trigger actually fires.

**Acceptance criteria:**
- [ ] Miller impact finds no unplanned production surface or missing likely test.
- [ ] Formatting, check, affected-change, dev, and system gates pass at the recorded code HEAD.
- [ ] The ledger records invariant, command, scope label, full commit SHA, result, UTC timestamp, and reuse status for every run.
- [ ] The final worktree is clean and every task checkbox reflects verified reality.

## Execution Order

1. Commit this approved plan and its baseline ledger on `codex/julie-improvement-roadmap`.
2. Execute Tasks 1-3 serially with `razorback:subagent-driven-development`; each worker uses TDD, exact tests, and `serial-worker-commit`.
3. Lead reviews each commit before dispatching the next task.
4. Lead runs Task 4 once against the final code commit and commits the evidence update.
5. Use `razorback:verification-before-completion`, then `razorback:finishing-a-development-branch`. Do not push without explicit approval.
