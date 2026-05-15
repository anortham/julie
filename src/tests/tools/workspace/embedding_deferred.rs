//! Tests for the deferred-embedding behavior of `spawn_workspace_embedding`.
//!
//! Task 2 (daemon reliability plan): the indexing command must NOT block on the
//! daemon embedding sidecar bootstrap (cold start ~36-39s, capped at 120s).
//! When the shared service is still `Initializing`, `spawn_workspace_embedding`
//! returns immediately with `EmbeddingOutcome { deferred: true, symbols: 0 }`
//! and queues a deferred task that runs the pipeline once the service settles.

use crate::daemon::embedding_service::{EmbeddingService, EmbeddingServiceSettled};
use crate::embeddings::{DeviceInfo, EmbeddingBackend, EmbeddingProvider, EmbeddingRuntimeStatus};
use std::sync::Arc;

/// `try_settled()` returns `None` while the service is still `Initializing` and
/// must do so without parking. This is the public non-blocking probe exposed
/// for `spawn_workspace_embedding`.
#[test]
fn try_settled_returns_none_while_initializing() {
    let service = EmbeddingService::initializing();
    assert!(
        service.try_settled().is_none(),
        "try_settled must return None when the service is still Initializing"
    );
}

/// `try_settled()` returns `Some(Ready)` once a provider has been published.
#[test]
fn try_settled_returns_ready_after_publish_ready() {
    let service = EmbeddingService::initializing();
    let provider: Arc<dyn EmbeddingProvider> = Arc::new(NoopProvider::default());
    let status = EmbeddingRuntimeStatus {
        requested_backend: EmbeddingBackend::Unresolved,
        resolved_backend: EmbeddingBackend::Unresolved,
        accelerated: false,
        degraded_reason: None,
    };
    service.publish_ready(provider, status);

    match service.try_settled() {
        Some(EmbeddingServiceSettled::Ready { .. }) => {}
        other => panic!(
            "expected try_settled() = Some(Ready) after publish_ready, got: {}",
            describe(&other)
        ),
    }
}

/// `try_settled()` returns `Some(Unavailable)` once the service is marked
/// degraded.
#[test]
fn try_settled_returns_unavailable_after_publish_unavailable() {
    let service = EmbeddingService::initializing();
    service.publish_unavailable("test: disabled".to_string(), None);

    match service.try_settled() {
        Some(EmbeddingServiceSettled::Unavailable { reason, .. }) => {
            assert_eq!(reason, "test: disabled");
        }
        other => panic!(
            "expected try_settled() = Some(Unavailable) after publish_unavailable, got: {}",
            describe(&other)
        ),
    }
}

/// Hot-path proof: `try_settled()` returns essentially instantly even when the
/// service is still `Initializing`, in contrast to `wait_until_settled(120s)`
/// which is the call this task removed from the index response path.
///
/// The threshold is generous (50ms) because we're protecting against the
/// regression of someone re-introducing a blocking call.
#[tokio::test]
async fn try_settled_is_non_blocking_while_initializing() {
    let service = EmbeddingService::initializing();
    let start = std::time::Instant::now();
    let result = service.try_settled();
    let elapsed = start.elapsed();

    assert!(
        result.is_none(),
        "try_settled must return None while Initializing"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(50),
        "try_settled must return immediately, took {elapsed:?}"
    );
}

/// `EmbeddingOutcome::deferred = true` is what signals callers to print the
/// "Embedding queued while provider initializes." message. This test ties the
/// struct contract down so renames don't silently break the response wording
/// agreed in Task 2.
#[test]
fn embedding_outcome_deferred_flag_is_observable() {
    // Constructing via the public field is the contract the callers depend on.
    // If you change the field names, callers in index.rs / register_remove.rs /
    // refresh_stats.rs that branch on `.deferred` and `.symbols` need to be
    // updated in lockstep.
    use crate::tools::workspace::indexing::embeddings::EmbeddingOutcome;
    let outcome = EmbeddingOutcome {
        symbols: 0,
        deferred: true,
    };
    assert!(outcome.deferred);
    assert_eq!(outcome.symbols, 0);
}

// ---- sync_vector_count_on_terminal tests ----

/// When `daemon_db` is `None`, `sync_vector_count_on_terminal` is a no-op
/// (no panic, no side effect).
#[tokio::test]
async fn sync_vector_count_noop_when_no_daemon_db() {
    use crate::tools::workspace::indexing::embeddings::sync_vector_count_on_terminal;

    let db_path = std::path::PathBuf::from("/nonexistent/path/symbols.db");
    // Should complete without error even though daemon_db is None.
    sync_vector_count_on_terminal(&None, "ws-test", &db_path).await;
}

/// When `daemon_db` is `Some` but `db_path` does not exist, vector count
/// should be set to 0 (the DB can't be read, so there are no embeddings).
#[tokio::test]
async fn sync_vector_count_zero_when_db_missing() {
    use crate::daemon::database::DaemonDatabase;
    use crate::tools::workspace::indexing::embeddings::sync_vector_count_on_terminal;

    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_path = tmp.path().join("daemon.db");
    let daemon = DaemonDatabase::open(&daemon_path).unwrap();
    daemon.upsert_workspace("ws-test", "/fake", "ready").unwrap();
    // Seed a stale vector count to prove the sync overwrites it.
    daemon.update_vector_count("ws-test", 999).unwrap();

    let daemon_db: Option<Arc<DaemonDatabase>> = Some(Arc::new(daemon));
    let nonexistent = tmp.path().join("does-not-exist/symbols.db");

    sync_vector_count_on_terminal(&daemon_db, "ws-test", &nonexistent).await;

    let daemon_ref = daemon_db.as_ref().unwrap();
    let ws = daemon_ref.get_workspace("ws-test").unwrap().unwrap();
    assert_eq!(
        ws.vector_count,
        Some(0),
        "vector_count must be 0 when workspace DB is missing"
    );
}

/// When `daemon_db` is `Some` and `db_path` exists with a valid (empty)
/// SymbolDatabase, vector count should reflect the actual embedding count (0).
#[tokio::test]
async fn sync_vector_count_reads_actual_from_existing_db() {
    use crate::daemon::database::DaemonDatabase;
    use crate::database::SymbolDatabase;
    use crate::tools::workspace::indexing::embeddings::sync_vector_count_on_terminal;

    let tmp = tempfile::TempDir::new().unwrap();

    // Create a real SymbolDatabase (empty -- 0 embeddings).
    let sym_db_path = tmp.path().join("symbols.db");
    let _sym_db = SymbolDatabase::new(&sym_db_path).unwrap();

    // Create daemon DB with a stale vector count.
    let daemon_path = tmp.path().join("daemon.db");
    let daemon = DaemonDatabase::open(&daemon_path).unwrap();
    daemon.upsert_workspace("ws-test", "/fake", "ready").unwrap();
    daemon.update_vector_count("ws-test", 42).unwrap();

    let daemon_db: Option<Arc<DaemonDatabase>> = Some(Arc::new(daemon));

    sync_vector_count_on_terminal(&daemon_db, "ws-test", &sym_db_path).await;

    let daemon_ref = daemon_db.as_ref().unwrap();
    let ws = daemon_ref.get_workspace("ws-test").unwrap().unwrap();
    assert_eq!(
        ws.vector_count,
        Some(0),
        "vector_count must match actual embedding count in the workspace DB (0 for empty DB)"
    );
}

// ---- helpers ----

fn describe(s: &Option<EmbeddingServiceSettled>) -> &'static str {
    match s {
        Some(EmbeddingServiceSettled::Ready { .. }) => "Some(Ready)",
        Some(EmbeddingServiceSettled::Unavailable { .. }) => "Some(Unavailable)",
        Some(EmbeddingServiceSettled::Timeout) => "Some(Timeout)",
        None => "None",
    }
}

#[derive(Default)]
struct NoopProvider;

impl EmbeddingProvider for NoopProvider {
    fn embed_query(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(Vec::new())
    }

    fn embed_batch(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "test".to_string(),
            device: "test".to_string(),
            model_name: "test-noop".to_string(),
            dimensions: 0,
        }
    }
}
