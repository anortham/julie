use std::sync::Arc;

use anyhow::Result;

/// Injectable runtime context for a daemon instance.
///
/// Holds cross-cutting singletons that tests can swap out for isolated
/// alternatives.  Production code constructs this via `Default`, which wires
/// in the process-wide singletons.  Test code calls `for_test()` to get an
/// independent set of registries so concurrent test daemons do not contend.
#[derive(Clone)]
pub struct DaemonRuntimeContext {
    /// Mutation-gate registry used by all workspace writers in this daemon
    /// instance.  In production this is the global singleton; in tests it is
    /// an isolated instance so concurrent test daemons do not contend.
    pub mutation_gate_registry: Arc<crate::workspace::mutation_gate::Registry>,
}

impl Default for DaemonRuntimeContext {
    fn default() -> Self {
        Self {
            mutation_gate_registry: Arc::clone(crate::workspace::mutation_gate::Registry::global()),
        }
    }
}

impl DaemonRuntimeContext {
    /// Test-only constructor with an isolated mutation-gate registry.
    /// Two `for_test()` instances do not share locks.
    pub fn for_test() -> Self {
        Self {
            mutation_gate_registry: Arc::new(crate::workspace::mutation_gate::Registry::new()),
        }
    }

    /// Install the global tracing subscriber for this process.
    ///
    /// Idempotent: a second call within the same process is a no-op (returns
    /// `Ok(())` without re-initializing). Use this from `daemon_main` and
    /// from in-process test fixtures (B.3) where multiple daemons may run
    /// in the same process — `tracing_subscriber::registry().init()` panics
    /// on second call, so the in-process fixture must call this.
    ///
    /// The `WorkerGuard` for the non-blocking file appender is stored in a
    /// process-wide `OnceLock` so it lives for the process lifetime; without
    /// it the background worker thread would shut down and log writes would
    /// silently drop.
    pub fn install_tracing(&self, paths: &crate::paths::DaemonPaths) -> Result<()> {
        use std::sync::OnceLock;
        use tracing_appender::non_blocking;
        use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
        use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

        static TRACING_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

        if TRACING_GUARD.get().is_some() {
            // Already installed in this process. No-op.
            return Ok(());
        }

        let filter = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new("julie=info"))
            .map_err(|e| anyhow::anyhow!("Failed to initialize logging filter: {}", e))?;

        let log_dir = paths.julie_home();
        std::fs::create_dir_all(&log_dir).unwrap_or_else(|e| {
            eprintln!("Failed to create log directory at {:?}: {}", log_dir, e);
        });

        let writer = crate::logging::LocalRollingWriter::new(&log_dir, "daemon.log");
        let (non_blocking_file, file_guard): (NonBlocking, WorkerGuard) = non_blocking(writer);

        let try_init_result = tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .with_writer(non_blocking_file)
                    .with_timer(crate::logging::LocalTimer)
                    .with_target(true)
                    .with_ansi(false)
                    .with_file(true)
                    .with_line_number(true),
            )
            .try_init();

        match try_init_result {
            Ok(()) => {
                // Park the worker guard for process lifetime. If another
                // thread raced and won set(), drop our guard — its writer
                // is the active one, ours never installed a subscriber.
                let _ = TRACING_GUARD.set(file_guard);
                Ok(())
            }
            Err(_err) => {
                // A subscriber was already installed (e.g. by an earlier
                // install_tracing() call or by an in-process test). That's
                // acceptable — the guarantee is "second call doesn't panic",
                // not "second call replaces the subscriber".
                Ok(())
            }
        }
    }
}
