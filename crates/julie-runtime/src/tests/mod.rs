// julie-runtime test suite.
// Relocated from the top-crate: all handler-free watcher + workspace tests.

// ── Watcher tests ──────────────────────────────────────────────────────────
pub mod watcher; // Core watcher tests (IncrementalIndexer, real-time indexing)
pub mod watcher_filtering; // Gitignore/julieignore filtering, blacklist, extension policy
pub mod watcher_handlers; // Incremental-indexing handler tests (create/modify/delete/rename)
pub mod watcher_mutation_gate; // Per-workspace mutation gate concurrency contract
pub mod watcher_observability; // INFO-level observability (rate limiter, gate timing)
pub mod watcher_queue; // Queue coalescing, overflow, repair-retry policy
pub mod watcher_runtime_boundary;

// ── Workspace tests (handler-free) ────────────────────────────────────────
pub mod workspace_init; // env_paths.rs — workspace env-var and path init tests
// (root_detection.rs stays top-crate: uses ManageWorkspaceTool)
pub mod workspace; // registry + root_safety pure-unit tests
