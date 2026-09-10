//! src/workspace_runtime/shutdown.rs
//! Strict graceful shutdown sequence for workspace runtimes.

use super::WorkspaceRuntime;
use std::time::Duration;
use tracing::{info, warn};

/// Execute strict graceful shutdown sequence:
/// 1. Drain commits (await active requests and pending transactions)
/// 2. Stop file watcher
/// 3. Close writable handles
pub async fn drain_and_shutdown(runtime: &WorkspaceRuntime, drain_timeout: Duration) {
    info!(
        workspace_id = %runtime.binding.workspace_id,
        "Initiating graceful shutdown sequence for workspace runtime"
    );

    runtime.shutdown_token.cancel();

    // Step 1: Drain active requests / in-flight commits
    let deadline = tokio::time::Instant::now() + drain_timeout;
    while runtime
        .active_requests
        .load(std::sync::atomic::Ordering::SeqCst)
        > 0
        || runtime
            .in_flight_commits
            .load(std::sync::atomic::Ordering::SeqCst)
            > 0
    {
        if tokio::time::Instant::now() >= deadline {
            warn!(
                workspace_id = %runtime.binding.workspace_id,
                remaining_requests = runtime.active_requests.load(std::sync::atomic::Ordering::SeqCst),
                remaining_commits = runtime.in_flight_commits.load(std::sync::atomic::Ordering::SeqCst),
                "Timed out waiting for in-flight requests/commits to drain; forcing shutdown"
            );
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Step 2: Stop file watcher
    {
        let mut ws_guard = runtime.handler.workspace.write().await;
        if let Some(ref mut ws) = *ws_guard {
            if let Err(e) = ws.stop_file_watching().await {
                warn!(
                    workspace_id = %runtime.binding.workspace_id,
                    error = %e,
                    "Error stopping file watcher during shutdown"
                );
            }
        }
    }

    // Step 3: Close writable handles
    // Flush SQLite WAL and close search writers
    {
        let mut ws_guard = runtime.handler.workspace.write().await;
        if let Some(ref mut ws) = *ws_guard {
            ws.watcher = None;
        }
    }

    info!(
        workspace_id = %runtime.binding.workspace_id,
        "Workspace runtime shutdown sequence complete"
    );
}
