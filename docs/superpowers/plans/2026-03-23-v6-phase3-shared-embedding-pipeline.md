# v6 Phase 3: Shared Embedding Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Julie MCP tools are MANDATORY for all code investigation.** Use `fast_search`, `deep_dive`, `get_symbols`, `fast_refs` before modifying any symbol. This is the dogfooding contract.
>
> **Test rules for subagents:** Run ONLY your specific test: `cargo test --lib <test_name> 2>&1 | tail -10`. Do NOT run `cargo xtask test dev` or any broad test suite. The orchestrator handles regression checks.

**Goal:** Enable daemon-mode sessions to share a single embedding provider (ORT model or sidecar process), restoring hybrid search and semantic features that are currently disabled in daemon mode.

**Architecture:** A new `EmbeddingService` struct owns the shared provider at the daemon level. The handler gets a reference to it and exposes a unified `embedding_provider()` method that tools call. The provider's internal mutex serializes GPU access. Stdio mode is fully preserved via fallback to the per-workspace provider.

**Tech Stack:** Rust, existing `EmbeddingProvider` trait, `OrtEmbeddingProvider`, `SidecarEmbeddingProvider`, daemon IPC infrastructure from Phase 1/2.

**Spec:** `docs/superpowers/specs/2026-03-23-v6-phase3-shared-embedding-pipeline-design.md`

---

## File Structure

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
| `src/workspace/mod.rs:720-967` | `initialize_embedding_provider()` becomes thin wrapper over `create_embedding_provider()` |
| `src/handler.rs:61-93` | Add `embedding_service` field |
| `src/handler.rs:100-118` | `new()`: set `embedding_service: None` |
| `src/handler.rs:131-176` | `new_with_shared_workspace()`: accept and store `embedding_service` param |
| `src/daemon/mod.rs:42-123` | `run_daemon()`: create `EmbeddingService`, pass through accept loop |
| `src/daemon/mod.rs:126-159` | `accept_loop()`: accept and forward `Arc<EmbeddingService>` |
| `src/daemon/mod.rs:162-276` | `handle_ipc_session()`: accept and pass `Arc<EmbeddingService>` to handler |
| `src/daemon/watcher_pool.rs:127-148` | `attach()`: accept `Option<Arc<dyn EmbeddingProvider>>` for `IncrementalIndexer` |
| `src/tools/search/text_search.rs:96,149` | Use `handler.embedding_provider().await` |
| `src/tools/search/nl_embeddings.rs:54-125` | Use `handler.embedding_provider().await`, simplify lazy init |
| `src/tools/get_context/pipeline.rs:537-538,585` | Use `handler.embedding_provider().await` |
| `src/tools/navigation/fast_refs.rs:86` | Use `handler.embedding_provider().await` |
| `src/tools/workspace/indexing/embeddings.rs:23-97` | Use `handler.embedding_provider().await`, remove lazy init in daemon mode |
| `src/tools/workspace/commands/registry/health.rs:164` | Use `handler.embedding_runtime_status()` |
| `src/tests/daemon/mod.rs` | Register `embedding_service` test module |

---

## Task 1: Extract `create_embedding_provider()` (Track A)

**Files:**
- Create: `src/embeddings/init.rs`
- Modify: `src/embeddings/mod.rs` (line 17, add `pub mod init`)
- Modify: `src/workspace/mod.rs:720-967`

This is the foundational refactor. The ~250-line `initialize_embedding_provider` method on `JulieWorkspace` doesn't actually need `self` for anything except writing results. We extract the pure logic into a standalone function.

- [ ] **Step 1: Write the failing test**

Create `src/embeddings/init.rs` with a test that calls `create_embedding_provider()`:

```rust
//! Standalone embedding provider initialization.
//!
//! Extracted from `JulieWorkspace::initialize_embedding_provider()` so the
//! daemon's `EmbeddingService` can initialize without a workspace instance.

use std::sync::Arc;
use anyhow::Result;
use super::{EmbeddingProvider, EmbeddingRuntimeStatus};

/// Initialize an embedding provider using environment configuration.
///
/// Returns `(provider, runtime_status)`. If initialization fails,
/// `provider` is `None` and `runtime_status` records the degraded reason.
/// This matches the graceful degradation pattern: keyword search is unaffected.
pub fn create_embedding_provider() -> (Option<Arc<dyn EmbeddingProvider>>, Option<EmbeddingRuntimeStatus>) {
    todo!("Extract from JulieWorkspace::initialize_embedding_provider")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_embedding_provider_returns_runtime_status() {
        // With JULIE_EMBEDDING_PROVIDER=none, should get None provider
        // and a runtime status indicating disabled
        std::env::set_var("JULIE_EMBEDDING_PROVIDER", "none");
        let (provider, status) = create_embedding_provider();
        assert!(provider.is_none());
        assert!(status.is_some());
        std::env::remove_var("JULIE_EMBEDDING_PROVIDER");
    }
}
```

- [ ] **Step 2: Register the module**

Add to `src/embeddings/mod.rs` after line 17 (`pub mod pipeline`):

```rust
pub mod init;
```

Add `pub use init::create_embedding_provider;` to the re-exports.

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib test_create_embedding_provider_returns_runtime_status 2>&1 | tail -10`
Expected: FAIL (todo! panic)

- [ ] **Step 4: Extract the logic from `JulieWorkspace::initialize_embedding_provider()`**

Move the body of `src/workspace/mod.rs:720-967` into `create_embedding_provider()` in `src/embeddings/init.rs`. The function should:
1. Read env vars (`JULIE_EMBEDDING_PROVIDER`, `JULIE_EMBEDDING_STRICT_ACCEL`, etc.)
2. Resolve backend via `resolve_backend_preference()`
3. Create provider via `EmbeddingProviderFactory::create()`
4. Handle fallback chains
5. Return `(Option<Arc<dyn EmbeddingProvider>>, Option<EmbeddingRuntimeStatus>)`

The `log_runtime_status` closure and `build_embedding_runtime_log_fields` call should stay in the function (logging is part of init). The only thing removed is the `self.embedding_provider = ...` / `self.embedding_runtime_status = ...` assignments.

- [ ] **Step 5: Make `JulieWorkspace::initialize_embedding_provider()` a thin wrapper**

Replace the body of `src/workspace/mod.rs:720-967` with:

```rust
pub fn initialize_embedding_provider(&mut self) {
    let (provider, runtime_status) = crate::embeddings::init::create_embedding_provider();
    self.embedding_provider = provider;
    self.embedding_runtime_status = runtime_status;
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --lib test_create_embedding_provider_returns_runtime_status 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 7: Run existing embedding provider tests to verify no regressions**

Run: `cargo test --lib test_invalid_provider_sets_unresolved 2>&1 | tail -10`
Run: `cargo test --lib test_provider_none_disables 2>&1 | tail -10`
Expected: Both PASS (these test through the workspace wrapper, which now delegates)

- [ ] **Step 8: Commit**

```bash
git add src/embeddings/init.rs src/embeddings/mod.rs src/workspace/mod.rs
git commit -m "refactor(embeddings): extract create_embedding_provider() from workspace method"
```

---

## Task 2: Create `EmbeddingService` (Track A)

**Files:**
- Create: `src/daemon/embedding_service.rs`
- Modify: `src/daemon/mod.rs` (add `pub mod embedding_service` declaration)

- [ ] **Step 1: Write the failing test**

Create `src/daemon/embedding_service.rs`:

```rust
//! Daemon-level shared embedding service.
//!
//! Owns a single `EmbeddingProvider` instance shared across all sessions.
//! Initialized eagerly at daemon startup. Tools access it through the handler.

use std::sync::Arc;
use crate::embeddings::{EmbeddingProvider, EmbeddingRuntimeStatus};

/// Shared embedding service for daemon mode.
///
/// Holds one provider instance (ORT or sidecar) that all sessions share.
/// The provider's internal mutex serializes GPU access.
pub struct EmbeddingService {
    provider: Option<Arc<dyn EmbeddingProvider>>,
    runtime_status: Option<EmbeddingRuntimeStatus>,
}

impl EmbeddingService {
    /// Initialize the embedding service by creating a provider.
    ///
    /// This is a blocking operation (ORT model load or sidecar bootstrap).
    /// Call from `spawn_blocking`.
    pub fn initialize() -> Self {
        todo!()
    }

    /// Get the shared embedding provider, if available.
    pub fn provider(&self) -> Option<&Arc<dyn EmbeddingProvider>> {
        self.provider.as_ref()
    }

    /// Get the embedding runtime status for health reporting.
    pub fn runtime_status(&self) -> Option<&EmbeddingRuntimeStatus> {
        self.runtime_status.as_ref()
    }

    /// Whether the embedding provider is available.
    pub fn is_available(&self) -> bool {
        self.provider.is_some()
    }

    /// Shut down the provider, releasing any child processes.
    pub fn shutdown(&self) {
        if let Some(ref provider) = self.provider {
            provider.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_service_unavailable_when_provider_none() {
        let service = EmbeddingService {
            provider: None,
            runtime_status: None,
        };
        assert!(!service.is_available());
        assert!(service.provider().is_none());
        assert!(service.runtime_status().is_none());
    }
}
```

- [ ] **Step 2: Register the module**

Add to `src/daemon/mod.rs` after line 7 (`pub mod database`):

```rust
pub mod embedding_service;
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test --lib test_embedding_service_unavailable_when_provider_none 2>&1 | tail -10`
Expected: PASS (this test doesn't call `initialize()`)

- [ ] **Step 4: Implement `initialize()`**

```rust
pub fn initialize() -> Self {
    use tracing::info;
    use crate::embeddings::init::create_embedding_provider;
    use crate::workspace::build_embedding_runtime_log_fields;

    info!("Initializing shared embedding service...");

    let (provider, runtime_status) = create_embedding_provider();

    let available = provider.is_some();
    if let Some(ref status) = runtime_status {
        let provider_info = provider.as_ref().map(|p| p.device_info());
        let fields = build_embedding_runtime_log_fields(
            status,
            provider_info.as_ref(),
            false, // strict mode is handled inside create_embedding_provider
            false, // fallback_used is handled inside create_embedding_provider
        );
        info!(
            available,
            backend = %status.resolved_backend.as_str(),
            "{}", fields
        );
    }

    info!(available, "Shared embedding service ready");

    EmbeddingService {
        provider,
        runtime_status,
    }
}
```

Note: The `build_embedding_runtime_log_fields` call above is aspirational. Check the actual signature with `deep_dive` and adjust. If logging is already handled inside `create_embedding_provider()`, a simple `info!` with the available/backend state suffices.

- [ ] **Step 5: Write test for initialize with provider disabled**

```rust
#[test]
fn test_embedding_service_initialize_with_provider_disabled() {
    std::env::set_var("JULIE_EMBEDDING_PROVIDER", "none");
    let service = EmbeddingService::initialize();
    assert!(!service.is_available());
    assert!(service.runtime_status().is_some());
    std::env::remove_var("JULIE_EMBEDDING_PROVIDER");
}
```

- [ ] **Step 6: Run test to verify**

Run: `cargo test --lib test_embedding_service_initialize_with_provider_disabled 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/daemon/embedding_service.rs src/daemon/mod.rs
git commit -m "feat(v6): add EmbeddingService for daemon-level shared embedding provider"
```

---

## Task 3: Add `embedding_service` to Handler (Track B)

**Files:**
- Modify: `src/handler.rs:61-93` (struct fields)
- Modify: `src/handler.rs:100-118` (`new()`)
- Modify: `src/handler.rs:131-176` (`new_with_shared_workspace()`)
- Modify: `src/handler.rs:183-186` (`new_for_test()`)

- [ ] **Step 1: Add the field to `JulieServerHandler`**

After `workspace_id` (line 92), add:

```rust
/// Shared embedding service for daemon mode. None in stdio mode.
pub(crate) embedding_service: Option<Arc<crate::daemon::embedding_service::EmbeddingService>>,
```

- [ ] **Step 2: Set `embedding_service: None` in `new()` (line 116)**

Add `embedding_service: None,` after `workspace_id: None,`

- [ ] **Step 3: Set `embedding_service: None` in `new_for_test()`**

The `new_for_test()` calls `Self::new()`, so this is likely already covered. Verify.

- [ ] **Step 4: Add `embedding_service` parameter to `new_with_shared_workspace()`**

Change the signature at line 131:

```rust
pub async fn new_with_shared_workspace(
    workspace: Arc<JulieWorkspace>,
    workspace_root: PathBuf,
    daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
    workspace_id: Option<String>,
    embedding_service: Option<Arc<crate::daemon::embedding_service::EmbeddingService>>,
) -> Result<Self> {
```

Add `embedding_service,` to the `Self { ... }` block (after `workspace_id,`).

- [ ] **Step 5: Add `embedding_provider()` helper method**

Add to `impl JulieServerHandler`:

```rust
/// Get the embedding provider, preferring daemon shared service over per-workspace.
pub(crate) async fn embedding_provider(&self) -> Option<Arc<dyn crate::embeddings::EmbeddingProvider>> {
    // Daemon mode: use shared service
    if let Some(ref service) = self.embedding_service {
        return service.provider().cloned();
    }
    // Stdio mode: use per-workspace provider
    let ws = self.workspace.read().await;
    ws.as_ref().and_then(|ws| ws.embedding_provider.clone())
}

/// Get embedding runtime status, preferring daemon shared service.
pub(crate) async fn embedding_runtime_status(&self) -> Option<crate::embeddings::EmbeddingRuntimeStatus> {
    if let Some(ref service) = self.embedding_service {
        return service.runtime_status().cloned();
    }
    let ws = self.workspace.read().await;
    ws.as_ref().and_then(|ws| ws.embedding_runtime_status.clone())
}
```

- [ ] **Step 6: Fix all call sites of `new_with_shared_workspace`**

The signature changed, so all callers need the new `embedding_service` parameter:

**Production:** `src/daemon/mod.rs`, `handle_ipc_session()` (around line 210). Add `None` temporarily (wired properly in Task 4).

**Tests:** `src/tests/daemon/handler.rs` has ~5 tests that call `new_with_shared_workspace` directly (lines 27, 52, 94, 155, 179). Add `None` to each. Use `fast_refs` on `new_with_shared_workspace` to find all callers and update them.

- [ ] **Step 7: Verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 8: Commit**

```bash
git add src/handler.rs src/daemon/mod.rs
git commit -m "feat(v6): add embedding_service field and provider helper to handler"
```

---

## Task 4: Wire `EmbeddingService` into Daemon Lifecycle (Track A)

**Files:**
- Modify: `src/daemon/mod.rs:42-123` (`run_daemon()`)
- Modify: `src/daemon/mod.rs:126-159` (`accept_loop()`)
- Modify: `src/daemon/mod.rs:162-276` (`handle_ipc_session()`)

- [ ] **Step 1: Create `EmbeddingService` in `run_daemon()`**

After the `DaemonDatabase` setup block (around line 91) and before `WatcherPool` creation, add:

```rust
// Initialize shared embedding service (blocking: model load / sidecar bootstrap)
let embedding_service = Arc::new(
    tokio::task::spawn_blocking(|| {
        crate::daemon::embedding_service::EmbeddingService::initialize()
    })
    .await
    .context("Embedding service initialization panicked")?
);
info!(
    available = embedding_service.is_available(),
    "Shared embedding service initialized"
);
```

- [ ] **Step 2: Add shutdown call**

In the shutdown section (after the `tokio::select!` block, around line 112), before `listener.cleanup()`:

```rust
embedding_service.shutdown();
info!("Embedding service shut down");
```

- [ ] **Step 3: Pass `Arc<EmbeddingService>` through `accept_loop`**

Change `accept_loop` signature to accept `embedding_service: &Arc<EmbeddingService>`:

```rust
async fn accept_loop(
    listener: &IpcListener,
    pool: &Arc<WorkspacePool>,
    sessions: &Arc<SessionTracker>,
    daemon_db: &Option<Arc<DaemonDatabase>>,
    embedding_service: &Arc<EmbeddingService>,
) -> Result<()> {
```

Clone it into the spawned task alongside pool/sessions/daemon_db. Pass to `handle_ipc_session`.

Update the call site in `run_daemon()`.

- [ ] **Step 4: Pass through `handle_ipc_session` to the handler**

Change `handle_ipc_session` to accept `embedding_service: &Arc<EmbeddingService>`.

Replace the `None` we set in Task 3 Step 6 with `Some(Arc::clone(embedding_service))`:

```rust
let handler = JulieServerHandler::new_with_shared_workspace(
    workspace,
    workspace_path,
    daemon_db.clone(),
    Some(full_workspace_id.clone()),
    Some(Arc::clone(embedding_service)),
)
```

- [ ] **Step 5: Verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 6: Commit**

```bash
git add src/daemon/mod.rs
git commit -m "feat(v6): wire EmbeddingService into daemon startup and session lifecycle"
```

---

## Task 5: Wire Shared Provider into WatcherPool (Track A)

**Files:**
- Modify: `src/daemon/watcher_pool.rs:127-148` (`attach()`)
- Modify: `src/daemon/workspace_pool.rs` (calls to `watcher_pool.attach()`)

- [ ] **Step 1: Add `embedding_provider` parameter to `WatcherPool::attach()`**

Change signature at `src/daemon/watcher_pool.rs`:

```rust
pub async fn attach(
    &self,
    workspace_id: &str,
    workspace: &crate::workspace::JulieWorkspace,
    embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
) -> anyhow::Result<()> {
```

Replace `workspace.embedding_provider.clone()` (line 146) with `embedding_provider`.

- [ ] **Step 2: Update callers of `attach()`**

Use `fast_refs` to find all call sites of `WatcherPool::attach`. Update each to pass the embedding provider. The `WorkspacePool::get_or_init()` method likely calls `attach()`. It needs access to the embedding service's provider.

Options:
- `WorkspacePool` stores `Option<Arc<EmbeddingService>>` (set during construction)
- Or `get_or_init` accepts the provider as a parameter

Check `WorkspacePool::new()` and `get_or_init()` to determine which is cleaner. If `WorkspacePool` already stores `WatcherPool`, adding `EmbeddingService` follows the same pattern.

- [ ] **Step 3: Verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add src/daemon/watcher_pool.rs src/daemon/workspace_pool.rs src/daemon/mod.rs
git commit -m "feat(v6): pass shared embedding provider to watcher pool for incremental re-embedding"
```

---

## Task 6: Migrate Tool Call Sites (Track C)

**Depends on:** Task 3 (handler helper methods must exist)

**Files:**
- Modify: `src/tools/search/text_search.rs:96,149`
- Modify: `src/tools/search/nl_embeddings.rs:54-125`
- Modify: `src/tools/get_context/pipeline.rs:537-538,585`
- Modify: `src/tools/navigation/fast_refs.rs:86`
- Modify: `src/tools/workspace/indexing/embeddings.rs:23-97`
- Modify: `src/tools/workspace/commands/registry/health.rs:164`

Each tool site currently reads `workspace.embedding_provider.clone()` directly. Replace with `handler.embedding_provider().await`.

- [ ] **Step 1: Migrate `text_search.rs`**

Two sites to change:
1. Line 96: `let ref_embedding_provider = workspace.embedding_provider.clone();` (reference workspace search path)
2. Line 149: `let embedding_provider = workspace.embedding_provider.clone();` (primary search path)

Both become: `let embedding_provider = handler.embedding_provider().await;`

Note: `text_search_impl` receives `handler: &JulieServerHandler`. Verify the function signature accepts a handler reference (it should, based on existing code).

- [ ] **Step 2: Migrate `get_context/pipeline.rs`**

Two sites:
1. Line 537-538: `workspace.embedding_provider.clone()` in the primary path
2. Line 585: `workspace.embedding_provider.clone()` in the reference workspace path

Both become: `handler.embedding_provider().await`

The `run()` function at line 511 already receives `handler: &JulieServerHandler`.

- [ ] **Step 3: Migrate `fast_refs.rs`**

Line 86: `workspace.embedding_provider.as_ref()` in `try_semantic_fallback`.

This method receives `handler: &JulieServerHandler`. Change to:

```rust
let provider = handler.embedding_provider().await;
let provider = match provider.as_ref() {
    Some(p) => p,
    None => return Ok(vec![]),
};
```

- [ ] **Step 4: Migrate `health.rs`**

Line 164: `workspace.embedding_provider.as_ref()` and reads `workspace.embedding_runtime_status`.

Change to use `handler.embedding_runtime_status().await` and `handler.embedding_provider().await`. The `check_embedding_runtime_health` method receives `&self` (ManageWorkspaceTool). It needs the handler passed in. Check the call chain with `deep_dive` to see how to thread it.

- [ ] **Step 5: Migrate `nl_embeddings.rs`**

This is the most complex site. Lines 54-125 have interleaved lazy-init and embedding-trigger logic.

The key change: the `is_none()` check on the provider (line 54) should use `handler.embedding_provider().await`. If the provider is available (daemon mode), skip the entire lazy-init block. The "trigger embedding pipeline if workspace has no embeddings" logic stays.

Use `deep_dive` on `maybe_initialize_embeddings_for_nl_definitions` to understand the full flow before modifying.

- [ ] **Step 6: Migrate `indexing/embeddings.rs` (`spawn_workspace_embedding`)**

Lines 28-97: The lazy init block. In daemon mode (`handler.embedding_service.is_some()`), replace the entire init block with:

```rust
let provider = match handler.embedding_provider().await {
    Some(p) => p,
    None => {
        debug!("Embedding provider unavailable, skipping workspace embedding");
        return 0;
    }
};
```

The existing lazy-init path (lines 37-97) stays as an `else` branch for stdio mode.

- [ ] **Step 7: Verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 8: Commit**

```bash
git add src/tools/search/text_search.rs src/tools/search/nl_embeddings.rs \
  src/tools/get_context/pipeline.rs src/tools/navigation/fast_refs.rs \
  src/tools/workspace/indexing/embeddings.rs \
  src/tools/workspace/commands/registry/health.rs
git commit -m "feat(v6): migrate tool call sites to handler.embedding_provider()"
```

---

## Task 7: Tests (Track C)

**Files:**
- Create: `src/tests/daemon/embedding_service.rs`
- Modify: `src/tests/daemon/mod.rs`

- [ ] **Step 1: Register test module**

Add to `src/tests/daemon/mod.rs`:

```rust
pub mod embedding_service;
```

- [ ] **Step 2: Write `EmbeddingService` unit tests**

Create `src/tests/daemon/embedding_service.rs`:

```rust
use crate::daemon::embedding_service::EmbeddingService;

#[test]
fn test_embedding_service_provider_returns_none_when_unavailable() {
    let service = EmbeddingService::initialize_for_test(None);
    assert!(!service.is_available());
    assert!(service.provider().is_none());
}

#[test]
fn test_embedding_service_shutdown_is_safe_when_no_provider() {
    let service = EmbeddingService::initialize_for_test(None);
    service.shutdown(); // should not panic
}
```

This requires adding an `initialize_for_test` constructor to `EmbeddingService`:

```rust
#[cfg(test)]
pub fn initialize_for_test(
    provider: Option<Arc<dyn EmbeddingProvider>>,
) -> Self {
    EmbeddingService {
        provider,
        runtime_status: None,
    }
}
```

- [ ] **Step 3: Write handler `embedding_provider()` test**

Test that the handler helper method returns the service's provider in daemon mode and the workspace's provider in stdio mode:

```rust
#[tokio::test]
async fn test_handler_embedding_provider_prefers_service_over_workspace() {
    // Create handler with embedding_service = None (stdio mode)
    let handler = crate::handler::JulieServerHandler::new_for_test().await.unwrap();
    // Should return None (no workspace provider initialized)
    assert!(handler.embedding_provider().await.is_none());
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib test_embedding_service_ 2>&1 | tail -10`
Run: `cargo test --lib test_handler_embedding_provider 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/tests/daemon/embedding_service.rs src/tests/daemon/mod.rs \
  src/daemon/embedding_service.rs
git commit -m "test(v6): add EmbeddingService and handler embedding_provider tests"
```

---

## Task 8: Integration Verification

**Files:** None (verification only)

- [ ] **Step 1: Run dev test tier**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All buckets pass. No regressions from the refactoring.

- [ ] **Step 2: Fix any failures**

If any tests fail, investigate and fix. Common issues:
- Tests that construct `JulieServerHandler` manually may need the new `embedding_service` field
- Tests that call `new_with_shared_workspace` need the new parameter

- [ ] **Step 3: Build release for dogfood testing**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Clean build

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix(v6): address test regressions from Phase 3 shared embedding changes"
```

---

## Task 9: Documentation Update

**Files:**
- Modify: `CLAUDE.md` (daemon mode description)
- Modify: `docs/WORKSPACE_ARCHITECTURE.md` (if embedding section exists)

- [ ] **Step 1: Update CLAUDE.md**

In the Architecture Principles section, update the daemon mode description to reflect that embedding resources are now shared:

- Item 3 (Per-Workspace Isolation): mention that embedding provider is daemon-level shared
- Item 8 (Semantic Embeddings): note daemon mode shares a single provider instance

- [ ] **Step 2: Update WORKSPACE_ARCHITECTURE.md**

If there's an embedding section, update to reflect the `EmbeddingService` layer.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md docs/WORKSPACE_ARCHITECTURE.md
git commit -m "docs(v6): update architecture docs for Phase 3 shared embedding service"
```

---

## Parallel Execution Strategy

**Agent teams (3 Sonnet teammates + Opus lead):**

| Track | Tasks | Can Start | Dependencies |
|-------|-------|-----------|--------------|
| **A** | 1, 2, 4, 5 | Immediately | Task 4 depends on Task 1 (init.rs) and Task 2 (service struct) |
| **B** | 3 | Immediately | Independent |
| **C** | 6, 7 | After Task 3 | Task 6 needs handler helper methods from Task 3 |
| **Integration** | 8, 9 | After all above | Final verification |

**Recommended assignment:**
- **Teammate 1:** Tasks 1 + 2 (extract init, build service)
- **Teammate 2:** Task 3 (handler integration)
- **Teammate 3:** Task 5 (watcher pool wiring)
- **Lead:** Task 4 (daemon lifecycle wiring, depends on 1+2), then Tasks 6-9

Or, if Task 1 finishes fast, reassign Teammate 1 to help with Task 6.
