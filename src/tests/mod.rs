// Julie's Test Infrastructure
//
// This module contains test utilities and infrastructure for testing extractors,
// search functionality, editing tools, and other Julie components.
//
// Editing tools now center on edit_file and rewrite_symbol.
// EditingTransaction remains shared with rename_symbol and other write paths.

// ============================================================================
// ANALYSIS TESTS - Post-indexing analysis (test quality, risk scoring)
// ============================================================================
pub mod analysis; // Test quality metrics engine tests

// ============================================================================
// TEST FIXTURES - Pre-indexed databases and test data
// ============================================================================
pub mod fixtures; // Test fixtures (JulieTestFixture for fast dogfooding tests)
pub mod harness; // Plan B.3: InProcessDaemon fixture for in-process daemon tests

// ============================================================================
// CLI TESTS - Argument parsing and workspace resolution
// ============================================================================
pub mod cli; // End-to-end CLI integration tests (binary invocation via std::process::Command)
pub mod cli_execution_tests; // CLI execution core (daemon/standalone mode, handler bootstrap)
pub mod cli_tests; // CLI argument parsing (clap) and workspace resolution tests
pub mod cli_tools_tests; // CLI tool subcommand parsing (search, refs, symbols, etc.)
pub mod external_extract;

// ============================================================================
// CORE SYSTEM TESTS - Database, handlers, language support
// ============================================================================
pub mod core {
    pub mod annotation_storage;
    pub mod early_warning_report_cache;
    pub mod embedding_provider; // EmbeddingProvider trait and factory tests
    pub mod embedding_sidecar_provider; // Sidecar provider IPC + dimension guard tests
    pub mod engine_version; // Phase 5.3 — extractor contract / engine version composition
    pub mod handler; // MCP handler tests
    pub mod handler_telemetry; // search telemetry and downstream target metadata tests
    pub mod incremental_update_atomic; // incremental_update_atomic write path tests (TDD)
    pub mod language; // Language detection and support tests
    pub mod logging; // Local-time log formatting and rolling writer tests
    pub mod paths; // Path utility tests (display_path, UNC handling)
    pub mod performance_indexes; // SQLite performance guardrail indexes
    pub mod revision_changes; // Canonical revision file-delta persistence tests
    pub mod serde_lenient_tests; // Lenient MCP param deserializers (u32, bool, Vec<String>)
    pub mod sidecar_embedding_tests; // Top-crate-only: sidecar_root_path CARGO_MANIFEST_DIR branch
    pub mod workspace_init; // Workspace root detection and initialization tests // Batch pending relationship resolution tests // bulk_store_types integration tests (TDD)
}

// ============================================================================
// TOOLS TESTS - Search, editing, refactoring, navigation, exploration
// ============================================================================
pub mod tools {
    pub mod get_symbols; // GetSymbolsTool tests
    pub mod get_symbols_relative_paths; // GetSymbolsTool Phase 2 relative path tests (TDD)
    pub mod get_symbols_smart_read; // GetSymbolsTool Phase 2 - Smart Read with code bodies
    pub mod get_symbols_target_filtering; // GetSymbolsTool target filtering tests
    pub mod get_symbols_target_filtering_dogfood; // GetSymbolsTool dogfood test: indexes full repo (~164s)
    pub mod get_symbols_target_workspace; // GetSymbolsTool target-workspace bug test
    pub mod get_symbols_token; // GetSymbolsTool token optimization tests
    // syntax_validation removed - abandoned AutoFixSyntax feature (Oct 2025)

    pub mod editing; // EditingTransaction tests (used by rename_symbol)

    pub mod deep_dive_primary_rebind_tests; // DeepDiveTool current-primary rebound routing tests
    // deep_dive_regression_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    // deep_dive_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    pub mod search; // Search tool tests (line mode, quality, race conditions)
    pub mod search_context_lines;
    pub mod search_quality; // Search quality dogfooding tests (regression suite) // FastSearchTool context_lines parameter tests (token optimization)
    pub mod text_search_tantivy; // Tantivy-based text search implementation tests

    pub mod refactoring; // Refactoring tool tests (SmartRefactorTool with SOURCE/CONTROL)

    pub mod workspace {
        pub mod deferred_open; // Deferred-session workspace open path tests
        pub mod discovery; // Vendor pattern detection and .julieignore auto-generation tests
        pub mod embedding_deferred; // Deferred-embedding non-blocking semantics (Task 2)
        pub mod file_policy; // Shared watcher/indexer extraction and path policy parity tests
        pub mod global_targeting; // Explicit workspace open/activation tests
        pub mod index_embedding_tests; // Embedding pipeline fixes: embedding_count reflects DB total
        pub mod isolation; // Workspace isolation tests
        pub mod manage_workspace_request; // Typed internal manage_workspace request parsing tests
        pub mod management_token; // ManageWorkspaceTool token optimization tests
        pub mod mod_tests; // Workspace module functionality tests
        pub mod processor; // Indexing processor parser-failure handling tests
        pub mod refresh_routing; // Primary force-refresh should reuse full index path
        // registry.rs relocated to crates/julie-runtime/src/tests/ (T2c.3 — tests julie-runtime's workspace::registry)
        pub mod resolver; // Cross-file relationship resolution tests
        // root_safety.rs relocated to crates/julie-runtime/src/tests/ (T2c.3 — tests julie-runtime's workspace::root_safety)
        pub mod utils; // Workspace utilities tests // Registry service tests
    }

    // phase4_token_savings relocated to crates/julie-tools/src/tests/ (T2b.6)

    pub mod blast_radius_determinism_tests; // blast_radius identifier-walk + deterministic output tests (2026-04-21 fixup)
    // blast_radius_formatting_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    pub mod blast_radius_tests; // blast_radius impact ranking and revision-range tests
    pub mod call_path_disambiguation_tests; // call_path per-endpoint file-path disambiguation tests
    pub mod call_path_tests; // call_path shortest-path navigation tests
    // filtering_tests relocated to crates/julie-tools/src/tests/ (T2b.6)

    // get_context_allocation_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    // get_context_formatting_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    // get_context_graph_expansion_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    // get_context_pipeline_relevance_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    // get_context_pipeline_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    pub mod get_context_primary_rebind_tests; // GetContextTool current-primary rebound routing tests
    // get_context_quality_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    // get_context_relevance_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    // get_context_scoring_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    pub mod get_context_target_workspace_metrics_tests; // get_context target-workspace telemetry attribution tests
    // get_context_task_inputs_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    // get_context_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    // get_context_token_budget_tests relocated to crates/julie-tools/src/tests/ (T2b.6)

    // hybrid_search_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    // query_classification_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    pub mod spillover_tests; // Spillover store and spillover_get paging tests

    pub mod fast_refs_primary_rebind_tests; // FastRefsTool current-primary rebound routing tests
    // formatting_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
    pub mod metrics; // Search metrics tests (session_metrics stays handler-bound)
    pub mod target_workspace_fast_refs_tests; // Target-workspace fast_refs parity (limit, reference_kind, identifiers)
}

// ============================================================================
// UTILS TESTS - Cross-language intelligence, scoring, optimization utilities
// ============================================================================
pub mod utils {
    pub mod context_truncation; // Context truncation tests
    pub mod cross_language_intelligence; // Cross-language intelligence tests
    pub mod progressive_reduction; // Progressive reduction tests
    pub mod token_estimation; // Token estimation tests
    pub mod utf8_boundary_safety;
    pub mod utf8_truncation; // UTF-8 safe string truncation tests // UTF-8 boundary safety checks for unsafe slicing patterns

    pub mod exact_match_boost; // Exact match boost tests

    pub mod path_relevance; // Path relevance scoring tests

    pub mod walk; // Shared walker builder tests (ignore crate integration)
}

// ============================================================================
// INTEGRATION TESTS - End-to-end and cross-component tests
// ============================================================================
pub mod integration {
    pub mod bulk_storage_atomicity; // Bulk storage atomicity tests (TDD) - verify transaction safety
    pub mod concurrent_mcp; // A2.3 concurrent MCP regression test (deadlock detector across pool + gate + watcher)
    pub mod daemon_lifecycle;
    pub mod documentation_indexing;
    pub mod in_process_boundary; // T12: in-process boundary tripwire (cutover bypasses, not deletes, daemon/adapter)
    pub mod indexing_pipeline;
    pub mod lock_contention; // Lock contention regression tests
    pub mod projection_repair;
    pub mod t11_kill_writer; // T11: kill-the-writer HARD GATE (lock kernel-release + crash-gap recovery)
    pub mod t9_handoff_recovery; // T9: leader handoff recovery + follower structural gate
    pub mod query_preprocessor_tests; // Query preprocessor comprehensive test suite (TDD)
    pub mod real_world_contract; // Real-world parser-upgrade expected output contracts
    pub mod real_world_validation; // Real-world code validation tests
    pub mod search_regression_tests; // Regression tests for recurring search glob pattern issues
    #[cfg(feature = "embeddings-sidecar")]
    pub mod sidecar_embedding_incremental; // Sidecar equivalents of embedding_incremental tests
    #[cfg(feature = "embeddings-sidecar")]
    pub mod sidecar_embedding_pipeline; // Sidecar equivalents of embedding_pipeline tests
    pub mod sidecar_test_helpers; // Shared fake-sidecar helpers for integration tests
    pub mod stale_index_detection; // Stale index detection tests
    pub mod system_health;
    pub mod target_workspace; // Target-workspace tests
    // watcher, watcher_filtering, watcher_handlers, watcher_mutation_gate,
    // watcher_observability, watcher_queue — relocated to julie-runtime (T2c.3)
    pub mod wiring_a1_8; // A1.8 end-to-end wiring tests (adapter + daemon split)
    pub mod workspace_isolation_smoke; // Fast workspace isolation smoke tests // Tracing integration tests (dogfooding tests) // Daemon + adapter integration tests (lifecycle, pool sharing, IPC, migration)
    pub mod zero_hit_replay_task3; // Task 3 diagnostic harness (ignored): replay content zero-hit fixture.
    pub mod zero_hit_replay_tests; // Task 12 acceptance harness (ignored): replay zero-hit fixture end-to-end and assert rates.
}

#[cfg(test)]
pub mod test_helpers {
    use anyhow::Result;
    use std::path::Path;
    use tempfile::TempDir;

    /// Create a temporary test workspace
    pub fn create_test_workspace() -> Result<TempDir> {
        Ok(tempfile::tempdir()?)
    }

    /// Create a test file with content
    pub fn create_test_file(
        dir: &Path,
        filename: &str,
        content: &str,
    ) -> Result<std::path::PathBuf> {
        use std::fs;
        let file_path = dir.join(filename);
        fs::write(&file_path, content)?;
        Ok(file_path)
    }

    // Moved to julie-test-support crate — re-export for existing callers.
    pub use julie_test_support::open_test_connection;

    /// Common test code snippets for various languages
    pub mod test_code {
        /// TypeScript test code
        pub const TYPESCRIPT_SAMPLE: &str = r#"
interface User {
    id: number;
    name: string;
    email: string;
}

class UserService {
    private users: User[] = [];

    constructor(private apiUrl: string) {}

    async getUser(id: number): Promise<User | null> {
        const response = await fetch(`${this.apiUrl}/users/${id}`);
        return response.json();
    }

    addUser(user: User): void {
        this.users.push(user);
    }
}

export { User, UserService };
        "#;

        /// Python test code
        pub const PYTHON_SAMPLE: &str = r#"
from typing import List, Optional
import asyncio

class User:
    def __init__(self, id: int, name: str, email: str):
        self.id = id
        self.name = name
        self.email = email

class UserService:
    def __init__(self, api_url: str):
        self.api_url = api_url
        self.users: List[User] = []

    async def get_user(self, id: int) -> Optional[User]:
        # Simulate API call
        await asyncio.sleep(0.1)
        return next((u for u in self.users if u.id == id), None)

    def add_user(self, user: User) -> None:
        self.users.append(user)
        "#;

        /// Rust test code
        pub const RUST_SAMPLE: &str = r#"
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
}

pub struct UserService {
    api_url: String,
    users: HashMap<u64, User>,
}

impl UserService {
    pub fn new(api_url: String) -> Self {
        Self {
            api_url,
            users: HashMap::new(),
        }
    }

    pub async fn get_user(&self, id: u64) -> Option<&User> {
        self.users.get(&id)
    }

    pub fn add_user(&mut self, user: User) {
        self.users.insert(user.id, user);
    }
}
        "#;
    }
}

// Test utilities
pub mod test_utils;

// Test helpers for isolation and cleanup
pub mod helpers;

// ============================================================================
// DASHBOARD TESTS - Error ring buffer, dashboard state, views
// ============================================================================
pub mod dashboard;

// ============================================================================
// DAEMON TESTS - v6 daemon infrastructure (paths, PID, lifecycle)
// ============================================================================
pub mod daemon;

// ============================================================================
// MIGRATION TESTS - v6 index migration from per-project to centralized
// ============================================================================
pub mod migration;

// ============================================================================
// EXTRACTOR TESTS - Live in the external julie-extractors repo
// ============================================================================
// All 34 language extractor tests now live upstream in anortham/julie-extractors
// (consumed here as a pinned git dependency). Run them in that repo's checkout.
