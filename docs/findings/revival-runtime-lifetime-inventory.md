# Revival runtime lifetime inventory

**Scope:** Plan 3C implementation contract, inspected at `1cd52b4b`.

## Current ownership

### Request runtime

- `src/service/http.rs::api_call` creates one `RequestContext` and awaits `RequestEngine::execute`; `src/request_engine/dispatch.rs::RequestEngine::execute` resolves the binding, awaits `RuntimeFactory::acquire`, and holds its returned `Arc<RequestRuntime>` through semantic readiness and tool dispatch.
- `src/request_engine/runtime_factory.rs::RuntimeFactory` owns the bound-runtime map. Its `RuntimeKey` maps to one `Arc<RuntimeSlot>`; `RuntimeSlot::runtime` contains the `Arc<RequestRuntime>`. `acquire` creates an empty slot under the map lock, then holds that slot's write lock while initializing or cloning its runtime. The map lock is not held during cold initialization.
- `RequestRuntime` owns an `Arc<JulieServerHandler>`. The factory slot and the executing request each retain it. Strong count proves only those request/runtime references; it does not account for spawned watcher or embedding ownership.
- `src/request_engine/dispatch.rs::RequestEngine::execute` can schedule `spawn_workspace_embedding` before dispatch completes. The spawned task owns the provider, `Arc<CheckoutStore>`, daemon DB, cancellation flag, and registry mutex even after the request returns.

### Watcher and queued mutations

- `crates/julie-runtime/src/workspace/mod.rs::JulieWorkspace` owns `Option<IncrementalIndexer>` and its `Arc<CheckoutStore>`. `start_file_watching` delegates to `IncrementalIndexer::start_watching`.
- `crates/julie-runtime/src/watcher/mod.rs::IncrementalIndexer::start_watching` owns the `notify::RecommendedWatcher`, unbounded notify producer/channel, event `JoinHandle`, queue `JoinHandle`, shared queue, cancellation flag, and a `QueueRuntime` clone that owns the store and mutation-gate registry.
- The event task ends when dropping `RecommendedWatcher` closes its channel. The queue task sees `cancel_flag`, calls `QueueRuntime::drain_for_shutdown`, then exits. `IncrementalIndexer::stop` drops the producer and awaits event then queue handles; `JulieWorkspace::stop_file_watching` calls it. This is the existing watcher drain proof.
- `QueueRuntime::process_queue_batch`, `drain_for_shutdown_inner`, and `reconcile_workspace` acquire the global per-workspace `MutationGuard`. `apply_store_changes` and reconciliation use `spawn_blocking` but await it before releasing the guard. `MutationGuard` therefore proves a finished writer only after its owning future has joined its blocking work.
- The gate serializes mutations; it is not a lifecycle lock and does not prevent a new runtime acquisition or scheduling new embedding work.

### Embedding task and store handles

- `src/tools/workspace/indexing/embeddings.rs::spawn_workspace_embedding` inserts `(Arc<AtomicBool>, JoinHandle<()>)` in `JulieServerHandler::embedding_tasks`, keyed by workspace ID. It refuses a second job for that key while the entry exists.
- Its outer task calls `src/tools/workspace/indexing/pipeline_runner.rs::run_pipeline_body`, which retains `Arc<CheckoutStore>` and awaits a nested `spawn_blocking(run_embedding_pipeline_cancellable(...))`. On completion it updates daemon metadata and removes only its own registry entry while holding `embedding_tasks` briefly.
- `src/tools/workspace/commands/force_safeguards.rs::cancel_embedding_tasks` removes entries under the mutex, sets the flag, and aborts the outer handle without awaiting it. `src/startup.rs::cancel_primary_embedding_task` removes its entry, sets the flag, waits at most five seconds for the outer handle, then aborts it. Neither retains a joinable handle to the nested blocking writer after abort.
- Callers: force index uses `cancel_embedding_tasks` in `commands/index.rs::handle_index_command_internal`; force refresh uses it in `commands/registry/refresh.rs::refresh_workspace_internal`; startup repair calls `cancel_primary_embedding_task`; `RequestEngine::execute`, index, and refresh schedule `spawn_workspace_embedding`.
- `CheckoutStore` is retained by `JulieWorkspace`, watcher/queue clones, `RequestEngine` semantic snapshots, the embedding outer/blocking work, and `JulieServerHandler::ref_store_cache`. `CheckoutStore::current` returns an `Arc<Snapshot>`; dropping snapshots releases only their in-memory projection reference. Releasing files/Tantivy ownership requires every owning `Arc<CheckoutStore>` above to drop. `checkout_store_for_workspace` returns the loaded workspace store or a cache entry; `invalidate_checkout_store` tears down the loaded workspace then removes the cache, but cannot release task-held Arcs.

## Current replacement and shutdown behavior

- `JulieServerHandler::teardown_loaded_workspace` holds `workspace.write()` while awaiting `stop_file_watching`, then clears the loaded workspace and ID. It neither cancels nor joins `embedding_tasks`, releases reference-store cache entries, or coordinates with a factory slot.
- `initialize_workspace_with_force` calls that teardown before rebuilding the primary workspace. `invalidate_checkout_store` also calls it for the loaded workspace. `handle_rebuild_command` invalidates the store before acquiring the mutation gate and deleting the index directory; it has no embedding cancellation/join boundary.
- `ServiceApp::serve` uses the service cancellation token only to stop Axum graceful acceptance and remove its discovery record. It has no runtime-factory teardown, maintenance task, join deadline, or watcher/embedding drain ordering. HTTP shutdown waits 50ms before cancelling that token.

## Required 3C seam and quiescence order

`RuntimeFactory` and the existing per-key `RuntimeSlot` must own retirement. Do not add leases, active-request counters, a second manager, durable lifecycle state, configuration, or empty-slot deletion.

1. A service-owned maintenance/shutdown owner marks the existing key-local slot retiring while retaining exclusive slot access. It owns teardown to completion; a timed-out request only stops waiting for it.
2. Block same-key acquisition and new same-runtime scheduling through that slot, without holding the factory map lock. Other slots remain independent. Keep the empty slot in the map after success.
3. Stop the watcher: drop notify producer, join event task, set cancellation, drain queue/mutation work, and join queue task. Do not hold `handler.workspace` while awaiting joins; move the workspace out first so no lock-dependent cleanup is blocked.
4. Remove matching embedding entries under `embedding_tasks`, release that mutex, signal cooperative cancellation, then await a joinable ownership chain that includes the nested blocking pipeline. The pipeline must finalize its registry entry without needing a mutex held by its joiner.
5. Await or otherwise prove all gate-owned mutation futures have released their guards. Only then drop workspace/store/snapshot/cache references and clear the populated slot.
6. Service shutdown first stops intake, then maintenance, then starts and owns teardown for every live slot under its existing deadline. A failed deadline reports failure and must not launch a replacement beside a live writer.

## Tests and architecture quality

| Boundary | Existing evidence/helper | Plan 3C test |
| --- | --- | --- |
| Key-local initialization and request ownership | `src/tests/request_engine.rs` dispatch barriers and concurrent-request tests | `runtime_reacquire_waits_for_teardown_without_duplicate_writer` |
| Watcher stop/drain and mutation serialization | `crates/julie-runtime/src/tests/watcher_freshness.rs`, `watcher_mutation_gate.rs`, `IncrementalIndexer::stop` | `runtime_cache_evicts_oldest_idle_checkout_and_stops_watcher` |
| Background writer survives request | `src/tests/tools/workspace/mod_tests/part6.rs::concurrent_schedule_keeps_existing_embedding_job` | `runtime_cache_never_evicts_active_request_or_background_writer` and `embedding_cancel_waits_for_blocking_writer_before_runtime_reopen` |
| Store reopen preserves durable data/vectors | existing workspace refresh/rebuild tests and `semantic_request_contract.rs` fixtures | `evicted_runtime_reopens_with_fresh_results_and_existing_vectors` |
| Service ownership and shutdown | `src/tests/service/control.rs::OwnedService` and shutdown tests | `service_shutdown_joins_maintenance_watchers_and_embedding_writers` |

**Affected modules:** `runtime_factory`, request dispatch, handler store/workspace lifecycle, runtime watcher, embedding scheduler/pipeline, workspace commands, and service shutdown.

**Caller-facing interface:** retain `RuntimeFactory::acquire`; add private lifecycle operations at its existing slot boundary. Service/maintenance owns retirement; callers only await acquisition subject to their existing deadline.

**Locality and earned seams:** reuse `RuntimeSlot`, `IncrementalIndexer::stop`, mutation guards, and the embedding registry. The only earned new seam is one private cancel-and-join embedding ownership path that preserves the blocking join handle.

**Rejected shortcuts:** outer-handle abort as quiescence, `Arc<RequestRuntime>` count as background-work proof, deleting map slots, and a second lifecycle manager. Each permits overlap or duplicate writers.

**Risk:** current teardown holds the workspace lock while awaiting watcher shutdown, while current embedding cancellation cannot prove its blocking writer stopped. Moving the workspace/task handles out before joins is required to avoid lock-order and self-cleanup deadlocks.
