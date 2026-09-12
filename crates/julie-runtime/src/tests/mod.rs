// julie-runtime test suite.
// Relocated from the top-crate: all handler-free watcher + workspace tests.

// ── Watcher tests ──────────────────────────────────────────────────────────
pub mod watcher_filtering; // Gitignore/julieignore filtering, blacklist, extension policy
pub mod watcher_mutation_gate; // Per-workspace mutation gate concurrency contract
pub mod watcher_runtime_boundary;
pub mod watcher_freshness;

// ── Workspace tests (handler-free) ────────────────────────────────────────
pub mod workspace_init; // env_paths.rs — workspace env-var and path init tests
// (root_detection.rs stays top-crate: uses ManageWorkspaceTool)
pub mod workspace; // registry + root_safety pure-unit tests

// ── Test helpers ───────────────────────────────────────────────────────────
pub mod helpers;
#[allow(unused_imports)]
pub use helpers::test_mutation_guard;
