# v6 Phase 3: Shared Embedding Pipeline

## Summary

Move embedding provider ownership from per-session workspaces to a single daemon-level `EmbeddingService`. One ORT model load (or one sidecar process) serves all connected sessions. Sessions access the shared provider through the handler, and the provider's internal mutex serializes GPU access naturally. Stdio mode is fully preserved.

## Problem

After Phase 2, daemon-mode sessions have `embedding_provider = None`. The `new_with_shared_workspace()` constructor explicitly skips it with a "Phase 3 handles shared embeddings" comment. This means:

- **No hybrid search** in daemon mode (keyword-only, no KNN semantic component)
- **No embedding pipeline** runs after indexing in daemon mode
- **No semantic fallback** in `fast_refs` (zero-reference symbols can't fall back to embedding similarity)

Meanwhile, stdio mode still works because each session initializes its own provider. The gap is visible: same codebase, different search quality depending on how Julie was started.

### VRAM Context

Each embedding provider instance consumes ~270MB VRAM (Jina-Code-v2 via ORT) or spawns a separate Python sidecar process. Without sharing, N sessions = N model loads. The daemon exists to solve exactly this kind of resource multiplication.

## Design

### EmbeddingService

A new struct at `src/daemon/embedding_service.rs`:

```rust
pub struct EmbeddingService {
    provider: Option<Arc<dyn EmbeddingProvider>>,
    runtime_status: Option<EmbeddingRuntimeStatus>,
}
```

**Lifecycle:**
- Created eagerly during `run_daemon()`, after `WorkspacePool` and `DaemonDatabase` setup
- Initialization runs in `spawn_blocking` (both ORT model loading and sidecar bootstrap are synchronous)
- If init succeeds: `provider = Some(...)`, `runtime_status` records backend/device info
- If init fails: `provider = None`, `runtime_status` records the degraded reason. Keyword search still works. Matches stdio mode's graceful degradation pattern.

**API:**
- `pub fn provider(&self) -> Option<&Arc<dyn EmbeddingProvider>>` - tools call this for query-time and pipeline embedding
- `pub fn runtime_status(&self) -> Option<&EmbeddingRuntimeStatus>` - health reporting
- `pub fn is_available(&self) -> bool` - convenience check
- `pub fn shutdown(&self)` - kills sidecar child process or drops ORT session

**Serialization:** No explicit queue or channel. Both `OrtEmbeddingProvider` and `SidecarEmbeddingProvider` already use internal `Mutex` for their stateful resources (ORT's `TextEmbedding` model and the sidecar's stdin/stdout pipe). Multiple concurrent callers block on the mutex. No fairness guarantee, but sufficient given low contention (embedding calls are infrequent relative to tool call frequency).

### Extracting Provider Initialization

`JulieWorkspace::initialize_embedding_provider()` is ~250 lines that reads env vars, resolves backends, handles fallback chains, and mutates workspace fields. The core logic doesn't depend on `self` except to write results.

**New file: `src/embeddings/init.rs`**

Extract steps into a standalone function:

```rust
pub fn create_embedding_provider()
    -> (Option<Arc<dyn EmbeddingProvider>>, Option<EmbeddingRuntimeStatus>)
```

No parameters; reads env vars the same way the current method does. Returns provider and status without mutating anything.

**Callers after refactor:**
- `EmbeddingService::initialize()` calls `create_embedding_provider()` directly
- `JulieWorkspace::initialize_embedding_provider()` becomes a thin wrapper: calls `create_embedding_provider()`, assigns results to `self.embedding_provider` / `self.embedding_runtime_status`

### Handler Integration

New field on `JulieServerHandler`:

```rust
pub(crate) embedding_service: Option<Arc<EmbeddingService>>,
```

**Wiring:**
- `new_with_shared_workspace()` gains a parameter: `embedding_service: Option<Arc<EmbeddingService>>`
- `handle_ipc_session()` passes the daemon's `Arc<EmbeddingService>` through
- Stdio mode passes `None` (per-workspace provider continues to work as-is)

**Unified access method on the handler:**

```rust
impl JulieServerHandler {
    pub(crate) async fn embedding_provider(&self) -> Option<Arc<dyn EmbeddingProvider>> {
        // Daemon mode: use shared service
        if let Some(ref service) = self.embedding_service {
            return service.provider().cloned();
        }
        // Stdio mode: use per-workspace provider
        let ws = self.workspace.read().await;
        ws.as_ref().and_then(|ws| ws.embedding_provider.clone())
    }

    pub(crate) async fn embedding_runtime_status(&self) -> Option<EmbeddingRuntimeStatus> {
        // Daemon mode: use shared service
        if let Some(ref service) = self.embedding_service {
            return service.runtime_status().cloned();
        }
        // Stdio mode: read from per-workspace status
        let ws = self.workspace.read().await;
        ws.as_ref().and_then(|ws| ws.embedding_runtime_status.clone())
    }
}
```

All tool call sites switch from direct `workspace.embedding_provider` reads to `handler.embedding_provider().await`.

### Daemon Lifecycle

**Startup order in `run_daemon()`:**

1. `DaemonPaths` / `PidFile`
2. `DaemonDatabase`
3. **`EmbeddingService::initialize()`** (in `spawn_blocking`)
4. `WatcherPool` + `WorkspacePool` (created together, as they are today)
5. IPC listener + accept loop

Embedding service initializes before the pools so it's ready when the first session connects. The provider `Arc` is passed into `WatcherPool` for incremental indexer creation.

**Accept loop changes:** `Arc<EmbeddingService>` passed alongside `Arc<WorkspacePool>` and `Option<Arc<DaemonDatabase>>` into each `handle_ipc_session`.

**Graceful shutdown:** Before dropping the PID file, call `embedding_service.shutdown()`. This kills the sidecar child process. ORT resources drop naturally.

**Logging:** Daemon startup logs embedding runtime status using the existing `build_embedding_runtime_log_fields()` helper.

### Tool Call Site Changes

15+ references to `workspace.embedding_provider` across tool code, all mechanical:

**Search path:**
- `src/tools/search/text_search.rs` - `hybrid_search()` provider parameter
- `src/tools/search/nl_embeddings.rs` - lazy init + embedding trigger for NL definition search
- `src/tools/get_context/pipeline.rs` - `run()` grabs provider for hybrid search

**Navigation path:**
- `src/tools/navigation/fast_refs.rs` - `try_semantic_fallback()` KNN fallback

**Indexing path:**
- `src/tools/workspace/indexing/embeddings.rs` - `spawn_workspace_embedding()` provider acquisition
- `src/tools/workspace/commands/registry/health.rs` - health check reads runtime status

**The mechanical change:** Every site that reads `workspace.embedding_provider.clone()` becomes `handler.embedding_provider().await`.

**Special case: `nl_embeddings.rs`** has logic that lazy-inits the provider AND triggers an embedding pipeline if the workspace has never been embedded. In daemon mode, the provider is already initialized (from the service), so the lazy-init branch is skipped. The "trigger embedding pipeline if needed" logic still applies and stays.

**Special case: `health.rs`** reads `workspace.embedding_runtime_status`. In daemon mode, reads from `handler.embedding_runtime_status()` instead.

**Special case: `spawn_workspace_embedding`** currently has a ~60-line lazy init block (lines 37-97). In daemon mode, this entire block is replaced by `handler.embedding_provider().await`. The lazy init path remains for stdio mode via the workspace fallback.

### Watcher / Incremental Indexer

The `WatcherPool` creates `IncrementalIndexer` instances that store their own `embedding_provider` field for re-embedding after file changes. Currently in `watcher_pool.rs:146`:

```rust
workspace.embedding_provider.clone(), // None in daemon mode!
```

Without fixing this, daemon mode gets bulk embedding on initial index but silently loses incremental re-embedding on file saves.

**Fix:** `WatcherPool::attach()` (or its internal watcher creation) needs access to the shared provider. Two options:

1. Pass `Option<Arc<dyn EmbeddingProvider>>` into `WatcherPool::attach()` from the daemon, sourced from `EmbeddingService`
2. Store `Option<Arc<EmbeddingService>>` on `WatcherPool` itself, read the provider when creating watchers

Option 1 is simpler and keeps `WatcherPool` unaware of `EmbeddingService`. The daemon already has both references at the point where watchers are attached (inside `handle_ipc_session` or `WorkspacePool::get_or_init`). The `IncrementalIndexer` struct doesn't need to change; it already takes `Option<Arc<dyn EmbeddingProvider>>` in its constructor.

**Modified files:** `src/daemon/watcher_pool.rs` needs to accept and pass through the provider when creating `IncrementalIndexer` instances.

### What Stays the Same

- `EmbeddingProvider` trait, `OrtEmbeddingProvider`, `SidecarEmbeddingProvider` - untouched
- `run_embedding_pipeline` / `run_embedding_pipeline_cancellable` - untouched
- `EmbeddingProviderFactory::create()` - untouched
- Sidecar bootstrap, venv management, protocol - untouched
- Stdio mode - fully preserved (`embedding_service = None`, tools fall through to workspace provider)
- `WorkspacePool` - no changes (watcher pool handles provider injection separately)
- `IncrementalIndexer` struct - no changes (already accepts `Option<Arc<dyn EmbeddingProvider>>`)
- Existing test suite - no tests should break

## Scope Exclusions

These were in the original Phase 3 spec outline but are explicitly out of scope:

- **Cross-workspace fairness scheduling**: FCFS via provider mutex is sufficient. Round-robin interleaving adds complexity for a scenario (multiple large workspaces competing for GPU) that rarely occurs in practice.
- **Priority-based cancellation**: Already implemented. `run_embedding_pipeline_cancellable` takes an `AtomicBool` cancel flag. No new work needed.
- **Explicit embedding queue (mpsc channel)**: Both backends serialize internally via mutex. A queue would add complexity without current benefit. Can be added later if metrics/backpressure are needed.

## Implementation Strategy

**Agent teams (3 Sonnet teammates + Opus lead)** for parallel execution. This also dogfoods the daemon's multi-session sharing, since multiple agents will be connected to the same daemon simultaneously.

**Parallel tracks:**
- **Track A:** Extract `create_embedding_provider()`, build `EmbeddingService` struct, add to daemon lifecycle
- **Track B:** Add `embedding_service` field to handler, add `embedding_provider()` / `embedding_runtime_status()` helper methods, update `new_with_shared_workspace()` signature
- **Track C:** Migrate tool call sites to use `handler.embedding_provider().await`

Track C depends on Track B (the helper method must exist). Track A and B can run in parallel.

## Dogfood Gate

From the v6 spec: "Run multiple sessions, verify VRAM usage, verify no queue starvation."

- **VRAM:** One provider in the daemon. `ps` or Activity Monitor confirms single ORT/sidecar process regardless of session count.
- **Starvation:** Provider mutex (`std::sync::Mutex`) serializes access. No fairness guarantee, but sufficient for this workload since embedding calls are infrequent and short-lived relative to contention windows.
- **Multi-session:** Open two Claude Code sessions on the same project; both get hybrid search results with semantic component.
- **Agent team dogfood:** The implementation itself exercises multi-session via parallel agent teammates.

## Files

### New Files

| File | Responsibility |
|------|---------------|
| `src/embeddings/init.rs` | Standalone `create_embedding_provider()` extracted from workspace method |
| `src/daemon/embedding_service.rs` | `EmbeddingService` struct: owns shared provider, exposes access API |
| `src/tests/daemon/embedding_service.rs` | Unit tests for `EmbeddingService` |

### Modified Files

| File | Changes |
|------|---------|
| `src/embeddings/mod.rs` | Add `pub mod init` |
| `src/workspace/mod.rs` | `initialize_embedding_provider()` becomes thin wrapper over `create_embedding_provider()` |
| `src/daemon/mod.rs` | Create `EmbeddingService` in `run_daemon()`, pass to accept loop and sessions, shutdown on exit |
| `src/handler.rs` | Add `embedding_service` field, `embedding_provider()` and `embedding_runtime_status()` helpers, update `new_with_shared_workspace()` signature |
| `src/tools/search/text_search.rs` | Use `handler.embedding_provider().await` |
| `src/tools/search/nl_embeddings.rs` | Use `handler.embedding_provider().await`, simplify lazy init |
| `src/tools/get_context/pipeline.rs` | Use `handler.embedding_provider().await` |
| `src/tools/navigation/fast_refs.rs` | Use `handler.embedding_provider().await` |
| `src/tools/workspace/indexing/embeddings.rs` | Use `handler.embedding_provider().await`, remove lazy init block in daemon mode |
| `src/tools/workspace/commands/registry/health.rs` | Use `handler.embedding_runtime_status()` |
| `src/daemon/watcher_pool.rs` | Pass shared embedding provider when creating `IncrementalIndexer` instances |
| `src/tests/daemon/mod.rs` | Register `embedding_service` test module |
