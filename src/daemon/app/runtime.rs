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
    /// Thin wrapper around `crate::logging::install_file_tracing` so the
    /// daemon and adapter share one appender + idempotency contract. Use this
    /// from `daemon_main` and from in-process test fixtures (B.3) where
    /// multiple daemons may run in the same process — the underlying helper's
    /// `OnceLock` ensures the second call is a no-op, not a panic.
    pub fn install_tracing(&self, paths: &crate::paths::DaemonPaths) -> Result<()> {
        crate::logging::install_file_tracing(&paths.julie_home(), "daemon.log", "julie=info")
    }
}
