# Plan 3: One bounded production runtime lifecycle

**Status:** 3A–3D implementation and post-change measurement are complete. The first Linux full gate failed the service coordination-word budget; its one allowed retry ran 2,292 tests with 2,291 pass, then hung only the obsolete non-cooperative force-reindex double. `eb75e4fc` fixes that double and its exact test passes. A third full gate requires owner approval under the retry policy and remains pending. Windows validation is deferred to final post-merge validation.
**Goal:** Opening a cold checkout does not block warm checkouts, and idle runtime resources are reclaimed safely.
**Depends on:** Plan 1 recovery and request-local semantics before lifecycle integration.
**Execution:** Follow the [roadmap contract](2026-09-11-revival-roadmap.md). Apply `razorback:diagnosing-performance`: baseline before optimization, same workload afterward.

## Architecture quality

`ServiceApp` constructs `request_engine::RuntimeFactory`. That factory remains the owner. The separate `src/workspace_runtime` manager/owner/lease/scheduler design is not imported wholesale. Risk is high because teardown, watcher work, embedding tasks and requests must agree on when a runtime is unused.

Use existing request-owned `Arc<RequestRuntime>` values and one small in-memory slot per registered checkout. The proposed slot serializes initialization and teardown for that checkout while allowing other checkouts to proceed. It holds an optional runtime; no global map guard is held across expensive work. This is the roadmap's explicit initialization synchronization exception, extended only to teardown of the same slot. There is no new public lease API, election, durable state, or second writer.

## Ownership and ordering

| Task | Files | Serialization / reason |
|---|---|---|
| 3A measurement | Proposed `docs/findings/revival-runtime-baseline.md`; extend existing service-status/replay evidence only if it cannot expose runtime/watcher counts | First; freezes workload and budgets |
| 3B initialization | `src/request_engine/runtime_factory.rs`; `src/tests/request_engine.rs` | After 3A; establishes the single slot lifecycle |
| 3C teardown/retention | `src/request_engine/runtime_factory.rs`, `src/handler.rs`, `src/service/mod.rs`, `src/tools/workspace/indexing/embeddings.rs`, `pipeline_runner.rs`, `src/tools/workspace/commands/force_safeguards.rs`, `src/startup.rs`, existing status fields in `src/service/http.rs` if necessary; `src/tests/request_engine.rs`, `src/tests/service/control.rs`, `src/tests/semantic_request_contract.rs` | After 3B and Plan 1; same ownership and lifetime |
| 3D deletion | Proven-unused files under `src/workspace_runtime/`, their registrations/reexports/tests and documentation references discovered by tracing | After 3C; remove only behavior replaced or proven unreachable |

All tasks are serial. The deletion task must enumerate its exact file list in the plan after tracing and before deleting; directory ownership does not authorize deleting live callers or tests of still-supported behavior.

## 3A. Record the cost before changing it

**Inputs:** Current release candidate, isolated home, fixed fixture/corpus commits, existing status and process-tree measurements.
**Produces:** A checked-in measurement protocol and before numbers that constrain tasks 3B/3C.

Baseline is the exact integrated commit immediately before Plan 3 production changes, including Plan 1. Record its SHA and compare with the exact post-change SHA. `eabf93cb` is the planning source reference, not automatically the runtime baseline. Use the same checkout set for both versions: one small warm checkout, one larger cold checkout, and a fixed sequence of at least 20 checkout opens/reopens. Record cold initialization wall time separately from warm-query p95 while the cold checkout initializes. Record live runtime count, watcher count, open handles and process-tree RSS after each settled stage. Warm once, then collect three repetitions. Keep semantic state and model identity fixed; report Off and ready semantics separately if the provider changes resource ownership.

The initial policy proposal is at most eight unused retained runtimes and expiry after 60 seconds of inactivity. These are hypotheses for the test workload, not evidence of adequate memory use. Record the selected policy and its budget after baseline measurement, before implementing retention; Astra may choose tighter fixed values from the workload. Do not add public configuration knobs without an operational need.

**Acceptance:**

- [x] Commands, binary/source identities, workload, cold/warm distinctions and three accepted measurements are recorded in `docs/findings/revival-runtime-baseline.md`.
- [x] Hot-checkout resource budget uses this same-workload before measurement plus 20%.
- [x] Retention bounds are eight runtimes and 60 seconds before 3C starts.
- [x] Unit tests use logical lifecycle assertions; CI does not depend on a noisy millisecond benchmark.

## 3B. Move cold initialization out of the global map guard

**Inputs:** `RuntimeKey`, `RuntimeFactory::acquire`, `RequestRuntime`, request cancellation and existing builder path.
**Produces:** One initialized runtime for concurrent requests to the same key, independent acquisition for different keys, and a retryable slot after failed initialization.

Use `RequestFixture` and a deterministic initialization barrier. Proposed root-package tests:

- `cold_workspace_initialization_does_not_block_warm_workspace`
- `same_workspace_cold_requests_share_one_initialization`
- `cancelled_or_failed_initialization_does_not_poison_runtime_slot`

For each, run `cargo nextest run -p julie --lib <exact_name>` RED, implement, `cargo check`, then the same GREEN test. Warm-checkout requests must complete while the cold barrier remains held, not merely finish before a generous timeout after it is released.

Install/retrieve the lightweight per-key slot under the map guard, then release that guard. Initialization occurs inside the slot. Cancellation must not publish a half-initialized handler or make all future callers inherit the cancelled request's failure. Reuse existing cancellation and cleanup paths rather than inventing a retry supervisor.

**Acceptance:**

- [x] Same-key concurrent opens perform exactly one successful initialization and own one watcher.
- [x] A cold key does not prevent another warm key from returning useful results.
- [x] Failed/cancelled initialization can be retried without replacing the whole service.
- [x] No broad map lock is held across index I/O, startup reconciliation, or model preparation.

## 3C. Reclaim idle resources without duplicate writers

See the pre-implementation [lifetime inventory](../findings/revival-runtime-lifetime-inventory.md) for the current ownership and quiescence contract.

**Inputs:** The 3B slot, measured retention policy, request-owned runtime Arcs, existing background-task activity, `JulieServerHandler::teardown_loaded_workspace`, and `stop_file_watching`.
**Produces:** Safe explicit runtime teardown and bounded idle retention, used by service maintenance and shutdown.

Before implementing, record a concrete lifetime inventory covering RequestRuntime Arcs, watcher event/queue handles, queued/store mutations, `handler.embedding_tasks`, the blocking pipeline task, and held store snapshots. RequestRuntime strong count proves request ownership only: background embedding work currently holds handler/store Arcs. Idle eviction requires no request-owned runtime reference and no active embedding task; teardown itself is the final quiescence barrier. Do not invent another active-request counter or lease system.

The proposed runtime teardown is an explicit extension of the current private `teardown_loaded_workspace`, not an already complete API. Block new acquisitions through the per-key slot; prevent new work from being scheduled for that retiring runtime through the same lifecycle boundary. Stop/drop the notify producer, await watcher event processing and queue drain, then cooperatively cancel and join remaining embedding work, then release snapshots/store references and clear the loaded workspace. Never hold the `embedding_tasks` mutex while awaiting a task whose cleanup needs that mutex.

`cancel_embedding_tasks` and the replacement path currently abort outer task handles without joining blocking work. Replace that behavior with one shared cancel-and-join contract in the active embedding task ownership path, covering force refresh and startup cancellation too. A completed/aborted outer JoinHandle is not proof that `spawn_blocking` returned. Preserve a joinable ownership path through the blocking pipeline and its final registry update; cancellation remains cooperative between batches. Add `embedding_cancel_waits_for_blocking_writer_before_runtime_reopen` to prove the boundary.

A teardown keeps the per-key slot exclusively held until the old watcher/tasks have stopped and outstanding mutation work has settled. A new acquisition for that key waits for teardown before initialization; unrelated keys proceed. A waiting request may hit its existing deadline, but that must not cancel teardown and publish a half-stopped runtime. Do not remove the slot from the map before teardown, which could create a second slot and writer. Lightweight empty slots remain and are reused on reopen in this slice. This deliberately bounds expensive runtimes, not all metadata: record bytes per retained slot and growth with distinct roots. Do not delete workspace registrations to control this map. Safe empty-slot removal is a separate measured follow-up if metadata becomes material.

Use one service-owned maintenance task with the existing shutdown cancellation, or an existing suitable tick if discovered. It scans idle slots and applies the selected count/age policy. It does not delete facts/Tantivy, registry membership, or compatible vectors. Reopening reacquires and reconciles the stored checkout normally. Do not stop the shared semantic child merely because one checkout is evicted. On service shutdown, stop accepting new requests, stop/join maintenance, and run the same teardown for every live slot. Use the existing service shutdown deadline; if joins cannot finish, report the failure and do not claim graceful shutdown or launch a replacement while the old process is alive.

Maintenance/service ownership must outlive request waiters: it owns each started teardown future until completion. Acquisitions only observe/wait on the slot and may time out independently. The request that encounters an idle runtime must not own a teardown future that disappears when its cancellation branch wins.

Proposed exact tests in `src/tests/request_engine.rs`:

- `runtime_cache_evicts_oldest_idle_checkout_and_stops_watcher`
- `runtime_cache_never_evicts_active_request_or_background_writer`
- `runtime_reacquire_waits_for_teardown_without_duplicate_writer`
- `evicted_runtime_reopens_with_fresh_results_and_existing_vectors`
- `service_shutdown_joins_maintenance_watchers_and_embedding_writers`

Use explicit maintenance calls and an injected/current test clock for expiry; no wall-clock sleeps. Extend service shutdown tests to prove the maintenance task terminates. Each new test receives the root-package exact RED/GREEN cycle.

**Acceptance:**

- [x] After work settles, resident unused runtimes meet the selected count/age bounds and their watcher resources are released.
- [x] Active calls, queued writes and semantic backfill are not discarded by eviction.
- [x] Teardown/reopen cannot overlap writers for one workspace.
- [x] Disk stores remain intact and subsequent queries are current (`evicted_runtime_reopens_with_fresh_results_and_existing_vectors`).
- [x] Three repeated checkout-churn runs show bounded live runtime resources; before/after RSS, retained slot metadata and warm-query p95 are recorded on the same workload.
- [x] Any resource growth or hot-checkout regression beyond the recorded budget is investigated before retaining the change.

## 3D. Delete the unused lifecycle alternative

Trace `WorkspaceRuntimeManager`, `ProcessFairScheduler`, public reexports, builders, and tests. Compare its behaviors with the production requirements covered above. Remove dead manager/owner/lease/scheduler code and tests that only prove the discarded implementation. Preserve tests of required behavior by moving their assertions to the production interface, not deleting the requirements.

Traced deletion set, removed in `1a1b700a`: `src/workspace_runtime/{builder,dirty_queue,manager,mod,owner,scheduler,shutdown}.rs`. The associated documentation references were updated in the same commit; production lifecycle behavior remains covered through `RuntimeFactory` and service tests.

Do not claim fair scheduling has shipped merely because a scheduler type exists. If a useful uncovered behavior remains, record it explicitly and either implement the minimal needed production behavior or leave that specific live module outside deletion. This task does not authorize new fairness/admission features.

**Acceptance:**

- [x] One documented runtime owner governs initialization, requests, teardown and service shutdown.
- [x] Removed symbols have no live consumers or dangling docs/reexports.
- [x] Required behavior still has tests against the active production path.
- [x] Deleted code outweighs any equivalent replacement machinery; no second lifecycle remains as a future fallback.

## Verification and handoff

The isolated multi-checkout lifecycle probe and ADR are complete (`6849556a`, `8de8c015`). The Linux full gate has one documented failed run and one documented retry; the next run is a policy-controlled third attempt requiring owner approval. Windows is intentionally deferred to final post-merge validation.

## Verification ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Post-change isolated warm/cold replay remains within the accepted resource budget | `JULIE_HOME=<mktemp> target/release/julie-server service --semantics off` with the recorded authenticated 20-open replay | live | `c3096cf3` binary | 3/3 complete; 60 alternating opens, 15 concurrent searches, and 12 status reads all HTTP 200; maximum after-open RSS 797179904 bytes / 49 FDs; warm p95 at most 18.104 ms | 2026-09-12T20:43:24Z | no |
| Isolated warm/cold runtime baseline has bounded release evidence | `JULIE_HOME=<mktemp> target/release/julie-server service --semantics off` with the recorded authenticated 20-open replay | live | `86a4101f` binary; `b499ee33` documented source | 3/3 complete; 60 alternating opens, 15 concurrent searches, and 12 status reads all HTTP 200; maximum after-open RSS 778272768 bytes / 48 FDs | 2026-09-12 | no |
| Cold initialization releases the global runtime map before bound startup | `cargo nextest run -p julie --lib cold_workspace_initialization_does_not_block_warm_workspace` | worker-red-green | `7925798d` | RED: warm query timed out while cold initialization held the map lock | 2026-09-12T19:32:00Z | no |
| Cold initialization releases the global runtime map before bound startup | `cargo nextest run -p julie --lib cold_workspace_initialization_does_not_block_warm_workspace` | worker-red-green | `1cd52b4b` | GREEN: passed | 2026-09-12T19:33:00Z | no |
| Same-key cold callers publish one runtime and watcher | `cargo nextest run -p julie --lib same_workspace_cold_requests_share_one_initialization` | worker-red-green | `1cd52b4b` | Characterization/non-regression: passed | 2026-09-12T19:33:00Z | no |
| Cancelled initialization leaves an empty retryable slot | `cargo nextest run -p julie --lib cancelled_or_failed_initialization_does_not_poison_runtime_slot` | worker-red-green | `1cd52b4b` | Characterization/non-regression: passed | 2026-09-12T19:33:00Z | no |
| Test barrier stores initialization entry before the test awaits it | `cargo nextest run -p julie --lib cold_workspace_initialization_does_not_block_warm_workspace` | worker-red-green | `1dcb2495` | RED: `notify_waiters` lost the entry notification after initializer scheduling; bounded wait timed out | 2026-09-12T19:37:00Z | no |
| Test barrier stores initialization entry before the test awaits it | `cargo nextest run -p julie --lib cold_workspace_initialization_does_not_block_warm_workspace` | worker-red-green | `f051b546` | GREEN: `notify_one` stored the entry notification; passed | 2026-09-12T19:37:00Z | no |
| Test barrier fails fast when initialization never reaches it | `cargo nextest run -p julie --lib cold_workspace_initialization_does_not_block_warm_workspace` | worker-red-green | `f051b546` | Focused correction: bounded attempt polling passed | 2026-09-12T19:39:00Z | no |
| Embedding cancellation joins the blocking writer before reopen | `cargo nextest run -p julie --lib embedding_cancel_waits_for_blocking_writer_before_runtime_reopen` | worker-red-green | `1e111ae6` | RED then GREEN: cooperative cancel-and-join waits for the blocked writer | 2026-09-12T20:00:00Z | no |
| Active request and registered embedding writer are retained by explicit retirement | `cargo nextest run -p julie --lib runtime_cache_never_evicts_active_request_or_background_writer` | worker-red-green | `be1a77a0` | GREEN: passed | 2026-09-12T20:00:00Z | no |
| Oldest idle checkout retires before newer idle slots | `cargo nextest run -p julie --lib runtime_cache_evicts_oldest_idle_checkout_and_stops_watcher` | worker-red-green | `be1a77a0` | GREEN: passed | 2026-09-12T20:00:00Z | no |
| Same-key reacquisition waits for teardown | `cargo nextest run -p julie --lib runtime_reacquire_waits_for_teardown_without_duplicate_writer` | worker-red-green | `be1a77a0` | GREEN: passed | 2026-09-12T20:00:00Z | no |
| Evicted runtime reopens with current facts and preserved vectors | `cargo nextest run -p julie --lib evicted_runtime_reopens_with_fresh_results_and_existing_vectors` | worker-red-green | `4545849c` | Characterization: passed; source changes reconcile while persisted compatible vectors remain | 2026-09-12T20:28:12Z | no |
| Service shutdown joins maintenance, watcher, and embedding writer work | `cargo nextest run -p julie --lib service_shutdown_joins_maintenance_watchers_and_embedding_writers` | worker-red-green | `572affc8` | GREEN: passed after shutdown-signal correction | 2026-09-12T20:49:10Z | no |
| Force reindex test double cooperates with embedding cancellation | `cargo nextest run -p julie --lib test_force_reindex_cancels_embedding_task_when_explicit_path_resolves_to_primary_root` | worker-red-green | `eb75e4fc` | GREEN: passed; the cooperative double observes cancellation and exits | 2026-09-12T20:55:13Z | no |
| First Linux branch gate rejects the service coordination-word budget | `cargo xtask test full` | full | `c3096cf3` | Failed only `service_modules_contain_no_coordination_words`; corrected in `572affc8` | 2026-09-12T20:48:00Z | no |
| Service budget and shutdown behavior after the first full-gate failure | `cargo nextest run -p julie --lib service_modules_contain_no_coordination_words` and `cargo nextest run -p julie --lib service_shutdown_joins_maintenance_watchers_and_embedding_writers` | worker-red-green | `572affc8` | Both exact tests passed | 2026-09-12T20:49:10Z | no |
| Allowed Linux full-gate retry reaches the cooperative force-reindex double | `cargo xtask test full` | full | `572affc8` | 2,291/2,292 passed; only `test_force_reindex_cancels_embedding_task_when_explicit_path_resolves_to_primary_root` hung. `eb75e4fc` corrects it; a third full run requires owner approval. | 2026-09-12T20:55:13Z | no |
