# Plan 1: Index freshness and request isolation

**Status:** Implementation authorized by the owner on 2026-09-11. Active worktree `.worktrees/revival-correctness`, branch `fix/revival-correctness`. Local commits authorized; push/tag/release not authorized.
**Goal:** Acknowledged source changes become searchable, semantic requests cannot affect one another, and interrupted embedding work resumes with truthful readiness.
**Depends on:** No other implementation plan.
**Execution:** The [roadmap execution and verification contract](2026-09-11-revival-roadmap.md) applies in full. Sol implements; Astra reviews and runs integration gates.

## Architecture quality

The affected interfaces are `QueueRuntime::run_cycle_with_retry_age`, `CheckoutStore::apply`, `RequestEngine::execute`, `check_facts_vectors`, and startup embedding scheduling. Recovery stays inside the active checkout-store mutation path; semantic policy stays in the request while provider ownership stays in the service. Tests use real store results and request-engine responses. Risk is high because freshness, concurrency, and readiness cross module boundaries.

Reject separate repair workers, persisted retry journals, per-request provider replacement, raw symbol-count coverage, and wiring the inactive owner/lease runtime into the service. The roadmap explicitly permits restoring the existing rescan obligation under the existing mutation gate.

## Ownership and ordering

| Task | Owned implementation files | Owned tests | Parallel batch / serialization |
|---|---|---|---|
| 1A watcher | `crates/julie-runtime/src/watcher/runtime.rs`, `runtime/processing.rs`, `events.rs`, `mod.rs`; proposed shared `crates/julie-runtime/src/workspace/reconcile.rs`, registration in its workspace module; `src/tools/workspace/indexing/incremental.rs` | Proposed `crates/julie-runtime/src/tests/watcher_freshness.rs`; registration in `crates/julie-runtime/src/tests/mod.rs` | A; independent editing from 1B, Cargo serialized |
| 1B semantics | `src/request_engine/dispatch.rs`, `src/handler.rs`, semantic-mode consumers in the nine scoped tools only if an existing request parameter is dropped | `src/tests/semantic_request_contract.rs` | A; independent editing from 1A, Cargo serialized |
| 1C coverage | `src/request_engine/semantic_store.rs`, `semantic.rs`, `dispatch.rs`, `src/startup_repair_plan.rs`, `src/tools/workspace/indexing/embeddings.rs`, `crates/julie-pipeline/src/embeddings/metadata.rs`, `pipeline/mod.rs`; `src/handler/mcp_adapter.rs` only for proven dropped readiness fields | `src/tests/semantic_request_contract.rs`, `src/tests/tools/workspace/mod_tests/part6.rs`, `src/tests/request_transport_parity.rs`, `crates/julie-pipeline/src/tests/embedding_metadata.rs` | B; after 1B, shared semantics contract/tests |

The fact schema/reader are evidence inputs, not owners of eligibility policy. `prepare_batch_for_embedding` and `select_budgeted_variables` in the pipeline define eligibility. Factor their common selection once and use it for pipeline, readiness and startup scheduling; do not duplicate it in SQL.

## 1A. Preserve and recover watcher updates

**Inputs:** Existing `PathChange`, `CheckoutStore`, filesystem policy, mutation gate, `needs_rescan`, queue, and indexing readiness. Inspect historical repair code for reusable behavior, but carry over only what the current store requires.

**Produces:** The existing watcher cycle processes ordinary changes and fulfills an outstanding rescan obligation; it reports pending work until reconciliation actually succeeds.

**Implementation sequence:**

1. Reuse `watcher_mutation_gate.rs` fixtures and existing store setup to write `watcher_second_save_inside_debounce_window_indexes_latest_content`. Process content A, replace it with B, process a second queued event immediately, and assert facts plus Tantivy represent B. Do not wait for OS notifications.
2. Run `cargo nextest run -p julie-runtime --lib watcher_second_save_inside_debounce_window_indexes_latest_content` and verify RED.
3. Remove time-only event dropping. Prefer letting the store's hash reuse handle redundant notifications. Coalesce only if it preserves create/delete/rename ordering and the final disk state; do not add another debounce protocol.
4. Run `cargo check -p julie-runtime`, then the same exact test for GREEN.
5. Add and cycle `watcher_overflow_rescan_reconciles_dropped_path`: overflow the existing queue seam, mutate/create/delete paths not retained by the queue, execute one explicit cycle, and assert the store matches disk. This contract performs one pending full reconciliation after the retained batch; no sleep or polling is needed.
6. Add and cycle `watcher_failed_apply_keeps_rescan_pending_until_success`: inject a transient store failure through an existing fault seam, assert pending status, allow a later cycle to succeed, and assert both actual data and status recover. Inspect the existing commit-failure hook before using it; a flag with no active consumer is not a valid fault injection.
7. Move the reusable scan/hash/diff portion of `src/tools/workspace/indexing/incremental.rs` below the root binary into the proposed runtime workspace reconciliation module. Reuse `julie_core::workspace_scan::scan_workspace_files`; root indexing and the watcher call the same policy-aware implementation. A runtime crate must not import the root binary or copy its policy. Keep root operation routing outside the lower helper.
8. `CheckoutStore::apply` already updates facts, graph, Tantivy and snapshot, but facts can commit before a projection failure. Exceptional rescan must therefore replay all current disk paths and remove paths absent from disk from the union of stored facts and the published graph, not skip them because fact hashes look current. This rare full reconciliation trades bounded extra work for restoring consistency without a second projection-repair subsystem. Normal incremental work still uses hashes.
9. Claim pending work with `needs_rescan.swap(false, AcqRel)` before reconciliation. Any scan/apply/publication failure sets it back to true; success performs no false-store. A new overflow during reconciliation consequently survives for the next cycle.
10. Run synchronous scan/diff/extraction/apply on the blocking pool, with the existing mutation guard owned by that work through publication. Preserve the event producer's ability to queue changes. Distinguish confirmed deletion from a discovered path that cannot be read or changes during scanning: retain its prior facts and retry, rather than treating skipped reads as deleted files. Add `watcher_rescan_preserves_unreadable_discovered_path` through an injected read failure.

**Acceptance:**

- [ ] Rapid distinct saves, duplicate notifications, deletes, renames, and delete/recreate finish at the latest filesystem state.
- [ ] Overflow and transient apply failure recover without user refresh or service restart.
- [ ] Ignored files stay ignored; deleted stored files are removed; unrelated workspaces are untouched.
- [ ] Rescan pending/error status reflects the remaining obligation and clears only after success.
- [ ] Cancellation preserves recoverable work; no retry busy loop or second writer is introduced.
- [ ] Facts, graph/snapshot publication, and Tantivy results agree after the cycle.

## 1B. Keep semantic policy per request

**Inputs:** `ToolRequest.semantics`, decoded tool parameters, existing embedding request budget, `SemanticRuntime`, and the cached handler. Reuse `SemanticFixture` and `MockReadyProvider` in `semantic_request_contract.rs`.

**Produces:** A stable shared provider with request-local Off/Auto/Required behavior on every semantic-capable path.

1. Write `concurrent_off_request_cannot_disable_required_request_provider`. Authorize a test-only barrier in `RequestEngine::execute` immediately after the current ambient-mode mutation and before tool dispatch/provider lookup. A barrier inside provider inference is too late; a readiness barrier is too early. Pause Required at that seam, execute Off against the same runtime, release Required, and assert its correct result. Assert Off made zero provider preparation/inference calls attributable to that request. The hook stays test-only and has no production coordination behavior.
2. RED: `cargo nextest run -p julie --lib concurrent_off_request_cannot_disable_required_request_provider`.
3. Remove per-request mutation of the shared provider and `semantics_disabled` flag from dispatch. Trace search, refs, context, deep-dive, and other decoded consumers; retain or thread their existing per-request mode. Keep service-wide provider disable configuration separate from an individual Off request.
4. `cargo check`, then GREEN with the same exact test.
5. Retain the existing sequential Off/Required, backend and cancellation tests for the other contracts. Add another focused regression only if tracing finds a distinct untested defect. Two different workspace handlers do not reproduce this same-handler race and are not a substitute for the regression above.

**Acceptance:**

- [ ] Off never initializes, waits for, or invokes the provider for that request.
- [ ] Off does not disable the provider or stop background work owned by another request/workspace.
- [ ] Required succeeds when its compatible capability is ready and reports the established structured error when it is not.
- [ ] Auto remains nonblocking during provider startup and labels fallback/degradation.
- [ ] The concurrent result is independent of request ordering; MCP and JSON API use the same policy.
- [ ] No ambient mutable request-mode flag remains on the shared execution path.

## 1C. Resume partial embeddings and report actual coverage

**Inputs:** The embedding pipeline's existing eligibility predicate, live facts, compatible encoder identity, stored vectors, and startup repair plan. Current code checks nonzero vectors and passes total symbols as eligible coverage; do not preserve those approximations.

**Produces:** One authoritative eligible/present/missing coverage calculation shared by readiness and scheduling. Reuse existing Ready/Degraded/error conventions and coverage fields; do not create another lifecycle state machine.

1. Record the following interface decision in the implementation checkpoint before coding: eligibility is computed from the current Snapshot/Graph and LanguageConfigs, including metadata filters and budgeted variables, before excluding already embedded symbols. Coverage counts distinct storage keys `(blob_hash, symbol_ordinal, encoder_id)` in the eligible set; a shared blob at two paths must not create an impossible denominator. The encoder is the full active compatible identity. No schema migration or persisted eligibility table is added.
2. Factor the pipeline's eligible-symbol selection into one reusable function without formatting every symbol's embedding text merely to check readiness. Supply the acquired runtime's snapshot/config to the existing semantic readiness path instead of relying on `facts_path` alone. Update the existing trait/callers and startup scheduling together; readiness must not create a second runtime to inspect coverage.
3. Add `required_semantics_refuses_partial_vector_generation`, `coverage_ignores_ineligible_symbols_and_incompatible_vectors`, and `coverage_deduplicates_shared_blob_symbol_keys`, using one embedded eligible symbol, one missing eligible symbol, deliberately ineligible symbols, and identical content at two paths. Run each exact owning-package test RED then GREEN around its smallest change.
4. Required symbol-search capability reports incomplete coverage until all eligible targets are represented. Zero eligible targets is a complete empty index. Query-only capabilities must not wait for irrelevant workspace vectors.
5. Auto keeps lexical service available while coverage is partial and reports existing `Degraded`/reason conventions. If existing compatible partial semantic results are used, label their coverage explicitly; never claim complete readiness. Keep provider availability separate from coverage and do not add another status schema.
6. Add `startup_repair_schedules_missing_embeddings_when_vector_count_is_nonzero_but_partial` and `restart_resumes_only_missing_embeddings`. Reuse startup-plan fixtures in `part6.rs`; simulate restarting the scheduler over the same temporary facts instead of spawning a real process in the unit test.
7. Resume only missing/stale eligible vectors through the existing pipeline. Its batching already retains partial successful work and skips existing IDs on a later run. Preserve valid vectors, handle deletion and edits during backfill, and prevent duplicate scheduling through existing in-flight state.
8. Verify existing request readiness and health/status fields agree. Trace `to_mcp_tool_result`; only if conversion demonstrably drops existing readiness data, preserve that object in structured MCP output. No unrelated transport redesign or new status schema is authorized.

**Acceptance:**

- [ ] Any partial eligible coverage remains visibly incomplete after restart, even with nonzero vectors.
- [ ] Valid vectors survive restart; work resumes and reaches full current coverage without a forced reindex.
- [ ] Off schedules no semantic work on behalf of its request; a machine-wide disabled provider stays disabled.
- [ ] Incompatible encoder or incomplete coverage uses the existing error/reason contract, not an empty success labeled ready; real network/model preparation belongs to Plan 4C.
- [ ] Tests cover empty workspace, no eligible symbols, stale vectors, partial vectors, complete coverage, edits, and deleted symbols.
- [ ] Required semantics and health agree across MCP and JSON API; terminal exit-code behavior remains consistent.

## Verification and handoff

Lead runs dev once per coherent batch and dogfood for index/search changes, then the full branch gate on the final commit. Use one isolated live two-client probe after unit gates: rapid edit freshness, concurrent Off/Required, service restart with partial coverage. Real provider/model behavior belongs to Plan 4C; fake-provider tests here prove policy without downloads.

Record source deltas, the root causes proven by RED, and the minimal architecture exceptions in the handoff. No source-derived finding is marked fixed without the corresponding behavioral GREEN result.

## Verification ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
