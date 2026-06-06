//! Julie daemon: background-process state, session tracking, and shutdown support.

pub mod database;

pub mod connection_pool;
pub mod discovery;
pub mod embedding_service;
pub mod lifecycle;
pub mod project_log;
pub mod session;
pub mod shutdown;
pub mod workspace_registry_store;
pub mod workspace_session_attachment;

use std::time::Duration;

pub use self::connection_pool::{PooledConn, WorkspaceConnectionPool};

use self::session::SessionTracker;

/// Wait for all active daemon sessions to finish, with a deadline.
///
/// Returns `true` if sessions drained cleanly, `false` if the timeout elapsed
/// while sessions were still active.
pub(crate) async fn drain_sessions(sessions: &SessionTracker, timeout: Duration) -> bool {
    tokio::time::timeout(timeout, async {
        loop {
            // Arm the notifier before checking count to avoid missing a wake-up
            // between the check and the await (standard condvar pattern).
            let notified = sessions.session_notify().notified();
            if sessions.is_idle() {
                return;
            }
            notified.await;
        }
    })
    .await
    .is_ok()
}
