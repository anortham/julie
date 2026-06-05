//! In-process server helpers (Phase 3c.2, T6 + T8).
//!
//! This module provides two things:
//!
//! 1. **`acquire_in_process_embedding_provider`** (T6) — acquires the shared
//!    3b resident embedding host.  Per-session spawning via
//!    `create_embedding_provider()` is deliberately NOT used here (it would OOM
//!    under N concurrent sessions each spawning their own sidecar).
//!
//! 2. **`run_in_process_server`** (T8) — the in-process MCP entry point.
//!    Wins or loses a per-workspace OS advisory leader lock, builds a
//!    `JulieServerHandler` with F2-coupled storage (db/tantivy and the leader
//!    lock share `~/.julie/indexes/{ws}/`), and serves over rmcp stdio with no
//!    HTTP, no fork, and no `discovery.json`.

use std::sync::Arc;

use anyhow::Context;
use tracing::{info, warn};

use crate::embeddings::EmbeddingProvider;
use crate::paths::DaemonPaths;

/// Acquire the shared resident embedding provider for an in-process session.
///
/// **Default-on:** uses the shared 3b resident host unless explicitly disabled.
///
/// ## Source
///
/// Calls [`connect_or_spawn_host`] — the shared host path — NOT
/// `create_embedding_provider()`, which would spawn a new sidecar per session
/// and cause OOM under N concurrent sessions.
///
/// ## Readiness gate (F2 lesson from Phase 3b)
///
/// The host connection is promoted to `Arc<dyn EmbeddingProvider>` ONLY if
/// [`RpcEmbeddingProvider::ensure_ready`] succeeds — the **fallible** health
/// handshake.  Silent-default getters (`device_info()`, `accelerated()`) are
/// deliberately NOT used as gates; they silently return defaults even when the
/// host is unhealthy (this is the exact failure mode from Phase 3b that the
/// `ensure_ready` gate was introduced to prevent).
///
/// ## Degradation
///
/// Returns `None` — degrade to keyword-only — when any of the following hold:
/// - `JULIE_EMBEDDING_PROVIDER=none/disabled/off` — explicit force-disable
/// - The host connect/spawn fails (e.g. binary missing, socket error)
/// - The host health handshake fails (host down, unresponsive, `ready=false`)
/// - The blocking task panics or is cancelled
///
/// None of these conditions block the caller; the session starts and serves
/// normally, just without semantic search.
///
/// ## Thread safety
///
/// All blocking I/O (`connect_or_spawn_host` + `ensure_ready`) runs in a
/// `tokio::task::spawn_blocking` worker thread, keeping the async runtime free.
///
/// [`connect_or_spawn_host`]: crate::embedding_host_launch::connect_or_spawn_host
/// [`RpcEmbeddingProvider::ensure_ready`]: julie_pipeline::embeddings::rpc_client::RpcEmbeddingProvider::ensure_ready
pub async fn acquire_in_process_embedding_provider(
    paths: &DaemonPaths,
) -> Option<Arc<dyn EmbeddingProvider>> {
    // Force-disable check: reuse the same JULIE_EMBEDDING_PROVIDER=none logic
    // as create_embedding_provider() (crates/julie-pipeline/src/embeddings/init.rs).
    // Do NOT invent a new env name — same knob, consistent behaviour.
    if let Ok(v) = std::env::var("JULIE_EMBEDDING_PROVIDER") {
        if matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "none" | "disabled" | "off"
        ) {
            info!(
                provider = %v,
                "In-process embedding disabled via JULIE_EMBEDDING_PROVIDER; \
                 degrading to keyword-only"
            );
            return None;
        }
    }

    let paths = paths.clone();

    // All blocking I/O off the async runtime thread.
    // host_spawn_timeout() inside connect_or_spawn_host bounds the wait (up to
    // 180s cold) — any error or timeout produces None; startup is never hung.
    let result = tokio::task::spawn_blocking(move || {
        match crate::embedding_host_launch::connect_or_spawn_host(&paths) {
            Ok(rpc) => {
                // HARD GATE: ensure_ready() runs the full health handshake and
                // surfaces errors + ready=false BEFORE we promote to
                // Arc<dyn EmbeddingProvider>.
                //
                // NEVER gate on device_info() / accelerated() here — they
                // silently return defaults even when the host is unhealthy,
                // masking an unready host as "Ready" (the F2-class failure from
                // Phase 3b that this gate was specifically introduced to prevent).
                match rpc.ensure_ready() {
                    Ok(()) => {
                        // OnceLock already populated by ensure_ready(); no extra I/O.
                        let provider: Arc<dyn EmbeddingProvider> = Arc::new(rpc);
                        Some(provider)
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            "In-process embedding: host health handshake failed; \
                             degrading to keyword-only"
                        );
                        None
                    }
                }
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "In-process embedding: host connect/spawn failed; \
                     degrading to keyword-only"
                );
                None
            }
        }
    })
    .await;

    match result {
        Ok(provider) => provider,
        Err(join_err) => {
            warn!(
                error = %join_err,
                "In-process embedding: init task panicked or was cancelled; \
                 degrading to keyword-only"
            );
            None
        }
    }
}

/// Run the in-process MCP server (T8 serve entry).
///
/// No fork, no HTTP endpoint, no `discovery.json`.  Serves `JulieServerHandler`
/// directly over rmcp stdio.  Auto-indexing is driven by the handler's
/// `on_initialized` callback.
///
/// ## Leader election + F2 storage coupling
///
/// Resolves `workspace_id` from `startup_hint.path`, creates (if needed) the
/// per-workspace index directory at `~/.julie/indexes/{ws}/`, then tries to
/// acquire `~/.julie/indexes/{ws}/leader.lock`:
///
/// - **Win** → `LeadershipState::leader(guard)` — owns all writes.
/// - **Lose** → `LeadershipState::follower()` — pure reader; T7 write-refusal
///   fires automatically.  **Must NOT use `none()`** — that would skip the
///   write-refusal gate and allow a loser to race the leader.
///
/// The `in_process_index_root` field threads the index directory into
/// `initialize_workspace_with_force`, ensuring the leader lock and the
/// workspace db/tantivy live under the same `~/.julie/indexes/{ws}/` tree
/// (the F2 hard gate — see `src/handler.rs` and `workspace/mod.rs`).
///
/// ## Embedding timeout (Part D)
///
/// `acquire_in_process_embedding_provider` can block up to 180 s on a cold
/// host.  This function wraps the acquire in a bounded timeout (default 5 s,
/// overridable via `JULIE_INPROCESS_EMBED_WAIT_SECS`) so `serve()` is never
/// delayed on startup.  On timeout the session degrades to keyword-only; the
/// background `spawn_blocking` task keeps running and warms the host for later
/// sessions.
pub async fn run_in_process_server(
    startup_hint: crate::workspace::startup_hint::WorkspaceStartupHint,
) -> anyhow::Result<()> {
    use crate::daemon::discovery::{AcquireError, DaemonLockGuard};
    use crate::handler::JulieServerHandler;
    use crate::leadership::LeadershipState;
    use crate::workspace::registry::generate_workspace_id;
    use rmcp::ServiceExt;

    // 1. Resolve the daemon paths (respects $JULIE_HOME).
    let paths = DaemonPaths::try_new().context("Failed to resolve Julie home directory")?;

    // 2. Derive workspace ID from the startup hint path.
    let workspace_id = generate_workspace_id(&startup_hint.path.to_string_lossy())
        .context("Failed to generate workspace ID")?;

    // 3. Create the per-workspace index directory BEFORE acquiring the lock —
    //    the lock file lives inside it (`{index_root}/leader.lock`).
    let index_root = paths.workspace_index_dir(&workspace_id);
    std::fs::create_dir_all(&index_root).with_context(|| {
        format!(
            "Failed to create workspace index dir at {}",
            index_root.display()
        )
    })?;

    // 4. Try to acquire the per-workspace leader lock.
    let lock_path = paths.workspace_leader_lock(&workspace_id);
    let leadership = match DaemonLockGuard::try_acquire(&lock_path) {
        Ok(guard) => {
            info!(
                workspace_id = %workspace_id,
                lock_path = %lock_path.display(),
                "Acquired workspace leader lock — serving as leader"
            );
            LeadershipState::leader(guard)
        }
        Err(AcquireError::AlreadyHeld(_)) => {
            // Another process (or in-process holder) owns the lock.
            // MUST use follower() — not none() — so T7 write-refusal fires.
            warn!(
                workspace_id = %workspace_id,
                lock_path = %lock_path.display(),
                "Workspace leader lock already held — serving as follower (read-only)"
            );
            LeadershipState::follower()
        }
        Err(AcquireError::Io { path, source }) => {
            return Err(anyhow::anyhow!(
                "Failed to acquire workspace leader lock at {}: {source}",
                path.display()
            ));
        }
    };

    // 5. Acquire embedding provider with a bounded timeout (Part D).
    //    A cold host can take up to 180 s to spawn — we must NOT block serve()
    //    on startup.  On timeout the session degrades to keyword-only; the
    //    background spawn_blocking task keeps running and warms the host for
    //    later sessions.
    let embed_wait_secs: u64 = std::env::var("JULIE_INPROCESS_EMBED_WAIT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let embedding_provider = match tokio::time::timeout(
        std::time::Duration::from_secs(embed_wait_secs),
        acquire_in_process_embedding_provider(&paths),
    )
    .await
    {
        Ok(provider) => provider,
        Err(_elapsed) => {
            warn!(
                timeout_secs = embed_wait_secs,
                "Embedding provider not ready within timeout — serving without \
                 semantic search (keyword-only). The host continues warming in \
                 the background for later sessions."
            );
            None
        }
    };

    // 6. Build handler.  Passing `Some(index_root)` threads the daemon index
    //    directory into initialize_workspace_with_force so db/tantivy land
    //    next to the leader lock — the F2 inode-coupling invariant.
    let handler = JulieServerHandler::new_in_process(
        startup_hint,
        embedding_provider,
        leadership,
        Some(index_root),
    )
    .await
    .context("Failed to build in-process handler")?;

    // 7. Serve over stdio.  Auto-index is triggered by on_initialized callback.
    //    No fork, no HTTP, no discovery.json.
    handler
        .serve(rmcp::transport::stdio())
        .await
        .context("MCP stdio serve failed")?
        .waiting()
        .await
        .map_err(|e| anyhow::anyhow!("In-process server task panicked: {e}"))?;

    Ok(())
}
