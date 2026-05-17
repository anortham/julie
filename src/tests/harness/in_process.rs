//! `InProcessDaemon` — start a `DaemonApp` inside the test process.
//!
//! This is the canonical test fixture introduced by Plan Task B.3. It wraps
//! the boilerplate around `DaemonApp::new` + `serve` so individual tests
//! don't re-derive it.
//!
//! ## What you get
//!
//! - A live daemon bound to an ephemeral 127.0.0.1 port.
//! - A `JULIE_HOME` rooted at a tempdir that's torn down on `shutdown`.
//! - An idempotent tracing install via
//!   [`DaemonRuntimeContext::install_tracing`] (B.2) so spinning many
//!   in-process daemons in the same test binary doesn't panic.
//! - An isolated `mutation_gate::Registry` per fixture via
//!   `DaemonRuntimeContext::for_test()` (B.1) so concurrent fixtures don't
//!   contend on global per-workspace gates.
//!
//! ## When NOT to use this
//!
//! Tests that intentionally exercise the real stdio↔HTTP adapter path with
//! a real subprocess (legacy-migration coexistence gate, wiring smoke
//! tests, CLI binary invocation) stay on `tokio::process::Command`. Plan
//! B.3 acceptance criterion: ≤5 subprocess spawn sites total in tests.

#[cfg(test)]
use std::net::SocketAddr;

#[cfg(test)]
use anyhow::Result;
#[cfg(test)]
use tempfile::TempDir;
#[cfg(test)]
use tokio::net::TcpListener;

#[cfg(test)]
use crate::daemon::{DaemonApp, DaemonConfig, DaemonHandle, DaemonRuntimeContext};
#[cfg(test)]
use crate::paths::DaemonPaths;

/// Builder for an in-process daemon fixture.
///
/// Defaults are tuned for tests:
/// - `no_dashboard: true` — no `opener::open` browser launch.
/// - `runtime: DaemonRuntimeContext::for_test()` — isolated mutation-gate
///   registry, so two `InProcessDaemon`s in the same test binary don't
///   share workspace gate locks.
///
/// Future plan knobs (`embedding_provider: Disabled`, `watcher_quota: 4`)
/// will land here as `DaemonConfig` grows.
#[cfg(test)]
pub struct InProcessDaemonBuilder {
    no_dashboard: bool,
    runtime: DaemonRuntimeContext,
}

#[cfg(test)]
impl Default for InProcessDaemonBuilder {
    fn default() -> Self {
        Self {
            no_dashboard: true,
            runtime: DaemonRuntimeContext::for_test(),
        }
    }
}

#[cfg(test)]
impl InProcessDaemonBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the dashboard suppression flag. Production default is
    /// `false`; the fixture default is `true` so test runs don't spawn
    /// browser windows.
    #[allow(dead_code)]
    pub fn no_dashboard(mut self, no_dashboard: bool) -> Self {
        self.no_dashboard = no_dashboard;
        self
    }

    /// Inject a specific runtime context. Use this when the test needs
    /// to share a mutation-gate registry across multiple fixtures
    /// (rare); the default builds a fresh isolated registry per call.
    #[allow(dead_code)]
    pub fn runtime(mut self, runtime: DaemonRuntimeContext) -> Self {
        self.runtime = runtime;
        self
    }

    /// Start the in-process daemon. Returns a [`InProcessDaemon`] handle
    /// that owns the tempdir for `JULIE_HOME` and the bound listener.
    /// Drop or call [`InProcessDaemon::shutdown`] to tear everything down.
    pub async fn start(self) -> Result<InProcessDaemon> {
        let temp_dir = tempfile::tempdir()?;
        let paths = DaemonPaths::with_home(temp_dir.path().to_path_buf());
        paths.ensure_dirs()?;

        // Install tracing idempotently so a binary that spins many
        // fixtures doesn't panic on the second install.
        self.runtime.install_tracing(&paths)?;

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr()?;

        let config = DaemonConfig {
            paths: paths.clone(),
            port: local_addr.port(),
            no_dashboard: self.no_dashboard,
            runtime: self.runtime,
            // No pre-acquired lock: `DaemonApp::new` will acquire it itself.
            // The InProcessDaemon harness owns the full daemon lifecycle per
            // test, with isolated paths, so the lock-acquisition path is the
            // canonical one to exercise here.
            daemon_lock: None,
        };

        let app = DaemonApp::new(config)?;
        let daemon_handle = app.serve(listener).await?;

        // Read the bearer token the MCP transport wrote during `serve`.
        // The MCP-specific token lives at daemon-mcp.token (separate from
        // the A1.4 daemon.token). If missing, we still return — some tests
        // don't need auth and discovery file carries the same value.
        let token_path = paths.daemon_mcp_token();
        let token = std::fs::read_to_string(&token_path)
            .map(|s| s.trim().to_string())
            .ok();

        let url = format!("http://{}", local_addr);

        Ok(InProcessDaemon {
            url,
            token,
            paths,
            local_addr,
            daemon_handle: Some(daemon_handle),
            _temp_dir: temp_dir,
        })
    }
}

/// A running in-process daemon fixture.
///
/// Owns the `JULIE_HOME` tempdir and the underlying `DaemonHandle`. Drop
/// or call [`InProcessDaemon::shutdown`] for graceful teardown.
#[cfg(test)]
pub struct InProcessDaemon {
    /// HTTP base URL: `http://127.0.0.1:<port>`.
    pub url: String,
    /// Bearer token the MCP transport wrote, if any.
    pub token: Option<String>,
    /// `DaemonPaths` rooted at the fixture's tempdir.
    pub paths: DaemonPaths,
    /// Bound MCP HTTP socket.
    pub local_addr: SocketAddr,
    /// The underlying daemon handle. Option-wrapped so `shutdown` can
    /// move it out without partial-move conflicts.
    daemon_handle: Option<DaemonHandle>,
    /// Tempdir for `JULIE_HOME` — cleaned up on drop.
    _temp_dir: TempDir,
}

#[cfg(test)]
impl InProcessDaemon {
    /// Default builder. Equivalent to `InProcessDaemonBuilder::new()`.
    #[allow(dead_code)]
    pub fn builder() -> InProcessDaemonBuilder {
        InProcessDaemonBuilder::new()
    }

    /// Trigger graceful shutdown and wait for teardown. Drops the
    /// tempdir after the daemon releases its file handles.
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(handle) = self.daemon_handle.take() {
            handle.shutdown().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::daemon::transport::{TransportEndpoint, TransportProbe};

    /// Self-test: the fixture spins a real daemon, exposes a reachable
    /// MCP HTTP endpoint, and tears down cleanly. Mirrors the existing
    /// `test_daemon_app_serve_and_shutdown` test but goes through the
    /// fixture so future tests can copy this 3-line pattern.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_in_process_daemon_starts_and_shuts_down() {
        let fixture = InProcessDaemon::builder()
            .start()
            .await
            .expect("fixture should start");

        // URL is well-formed and matches the bound address.
        assert!(
            fixture.url.starts_with("http://127.0.0.1:"),
            "url should be http://127.0.0.1:<port>, got {}",
            fixture.url
        );
        assert_eq!(
            fixture.url,
            format!("http://{}", fixture.local_addr),
            "url and local_addr must agree"
        );

        // MCP discovery file is published and the readiness probe
        // succeeds — proves the listener is actually serving.
        let discovery_path = fixture.paths.daemon_mcp_transport();
        let endpoint = TransportEndpoint::read_discovery(&discovery_path)
            .expect("discovery file should be readable");
        let endpoint_for_probe = endpoint.clone();
        let probe = tokio::task::spawn_blocking(move || endpoint_for_probe.probe_readiness())
            .await
            .expect("probe task join");
        assert_eq!(
            probe,
            TransportProbe::Ready,
            "mcp transport at {} must be ready",
            endpoint.mcp_url().unwrap_or_default()
        );

        // Bearer token written.
        assert!(
            fixture.token.is_some(),
            "MCP transport should have written a bearer token to {}",
            fixture.paths.token_file().display()
        );

        tokio::time::timeout(Duration::from_secs(10), fixture.shutdown())
            .await
            .expect("shutdown should complete within 10s")
            .expect("shutdown should not error");
    }

    /// Two concurrent in-process daemons in the same test binary must not
    /// panic on tracing init and must not contend on workspace gate
    /// locks. This is the B.1 + B.2 promise: shared global state is
    /// either idempotent (tracing) or per-fixture (gate registry).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_two_fixtures_in_same_process_are_independent() {
        let a = InProcessDaemon::builder()
            .start()
            .await
            .expect("fixture A start");
        let b = InProcessDaemon::builder()
            .start()
            .await
            .expect("fixture B start (would panic if tracing init wasn't idempotent)");

        assert_ne!(a.local_addr.port(), b.local_addr.port());
        assert_ne!(a.paths.julie_home(), b.paths.julie_home());

        tokio::time::timeout(Duration::from_secs(10), a.shutdown())
            .await
            .expect("A shutdown timed out")
            .expect("A shutdown errored");
        tokio::time::timeout(Duration::from_secs(10), b.shutdown())
            .await
            .expect("B shutdown timed out")
            .expect("B shutdown errored");
    }
}
