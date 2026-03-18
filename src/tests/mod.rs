// Julie's Test Infrastructure
//
// This module contains test utilities and infrastructure for testing extractors,
// search functionality, editing tools, and other Julie components.
//
// Editing tools (edit_lines, fuzzy_replace, edit_symbol) were removed in the
// toolset redesign (2026-02-07). Only EditingTransaction remains (used by rename_symbol).

// ============================================================================
// ANALYSIS TESTS - Post-indexing analysis (test quality, risk scoring)
// ============================================================================
pub mod analysis; // Test quality metrics engine tests

// ============================================================================
// TEST FIXTURES - Pre-indexed databases and test data
// ============================================================================
pub mod fixtures; // Test fixtures (JulieTestFixture for fast dogfooding tests)

// ============================================================================
// CLI TESTS - Argument parsing and workspace resolution
// ============================================================================
pub mod cli_tests; // CLI argument parsing (clap) and workspace resolution tests

// ============================================================================
// MAIN SERVER TESTS - Entry point error handling
// ============================================================================
pub mod main_error_handling; // MCP server initialization and runtime error handling

// ============================================================================
// CORE SYSTEM TESTS - Database, handlers, language support
// ============================================================================
pub mod core {
    pub mod batch_resolver;
    pub mod database; // Database operations and SQLite tests
    pub mod database_lightweight_query; // Lightweight query optimization tests
    pub mod embedding_deps; // Embedding dependency smoke tests (fastembed + sqlite-vec)
    pub mod embedding_metadata; // Symbol metadata formatting for embeddings
    pub mod embedding_provider; // EmbeddingProvider trait + OrtEmbeddingProvider tests
    pub mod embedding_sidecar_protocol; // Sidecar protocol contracts + validation tests
    pub mod embedding_sidecar_provider; // Sidecar provider IPC + dimension guard tests
    pub mod handler; // MCP handler tests
    pub mod language; // Language detection and support tests
    pub mod memory_vectors; // Memory embedding vector storage (migration 012 + CRUD + KNN)
    pub mod paths; // Path utility tests (display_path, UNC handling)
    pub mod sidecar_embedding_tests; // Embedded sidecar extraction + root path fallback tests
    pub mod sidecar_supervisor_tests; // Sidecar supervisor config, launch, and utility tests
    pub mod tracing; // Tracing and logging tests
    pub mod vector_storage; // sqlite-vec vector storage CRUD tests
    pub mod windows_embedding_policy; // Windows ORT policy + DirectML adapter selection tests
    pub mod workspace_init; // Workspace root detection and initialization tests // Batch pending relationship resolution tests
    pub mod incremental_update_atomic; // incremental_update_atomic write path tests (TDD)
}

// ============================================================================
// REGRESSION PREVENTION TESTS - Catch recurring bugs before they ship
// ============================================================================
pub mod regression_prevention_tests; // Tests for bugs that have regressed multiple times

// ============================================================================
// TOOLS TESTS - Search, editing, refactoring, navigation, exploration
// ============================================================================
// pub mod test_git_context; // Git context capture tests (debugging crashes) - TODO: File missing

pub mod tools {
    pub mod get_symbols; // GetSymbolsTool tests
    pub mod get_symbols_reference_workspace; // GetSymbolsTool reference workspace bug test
    pub mod get_symbols_relative_paths; // GetSymbolsTool Phase 2 relative path tests (TDD)
    pub mod get_symbols_smart_read; // GetSymbolsTool Phase 2 - Smart Read with code bodies
    pub mod get_symbols_target_filtering; // GetSymbolsTool target filtering tests
    pub mod get_symbols_token; // GetSymbolsTool token optimization tests
    pub mod smart_read; // Smart Read token optimization tests
    // syntax_validation removed - abandoned AutoFixSyntax feature (Oct 2025)

    pub mod editing; // EditingTransaction tests (used by rename_symbol)

    pub mod deep_dive_tests; // DeepDiveTool tests (formatting + data layer)
    pub mod search; // Search tool tests (line mode, quality, race conditions)
    pub mod search_context_lines;
    pub mod search_quality; // Search quality dogfooding tests (regression suite) // FastSearchTool context_lines parameter tests (token optimization)
    pub mod text_search_tantivy; // Tantivy-based text search implementation tests

    pub mod refactoring; // Refactoring tool tests (SmartRefactorTool with SOURCE/CONTROL)

    pub mod workspace {
        pub mod discovery; // Vendor pattern detection and .julieignore auto-generation tests
        pub mod isolation; // Workspace isolation tests
        pub mod management_token; // ManageWorkspaceTool token optimization tests
        pub mod mod_tests; // Workspace module functionality tests
        pub mod registry; // Workspace registry tests
        pub mod registry_service;
        pub mod resolver; // Cross-file relationship resolution tests
        pub mod runtime_status_stats; // Stats output embedding runtime status tests
        pub mod utils; // Workspace utilities tests // Registry service tests
    }

    pub mod phase4_token_savings; // Phase 4: Data structure optimization token savings tests (skip_serializing_if)

    pub mod filtering_tests; // Symbol filter pipeline tests (index-based refactor TDD)

    pub mod get_context_allocation_tests; // get_context token allocation tests
    pub mod get_context_formatting_tests; // get_context output formatting tests
    pub mod get_context_graph_expansion_tests; // get_context graph expansion tests
    pub mod get_context_pipeline_relevance_tests;
    pub mod get_context_pipeline_tests; // get_context pipeline integration tests
    pub mod get_context_quality_tests; // get_context fixed-query quality regression tests
    pub mod get_context_relevance_tests; // get_context fallback relevance guardrail tests
    pub mod get_context_scoring_tests; // get_context namespace/module de-boost scoring tests
    pub mod get_context_tests; // get_context tool pipeline tests (pivot selection, scoring)
    pub mod get_context_token_budget_tests; // get_context token truncation tests // get_context run_pipeline fallback relevance tests

    pub mod hybrid_search_tests; // RRF merge algorithm tests (hybrid keyword + semantic search)

    pub mod formatting_tests; // Navigation formatting tests (lean refs, qualified name parsing)
    pub mod metrics; // QueryMetricsTool metrics query tests
    pub mod reference_workspace_fast_refs_tests; // Reference workspace fast_refs parity (limit, reference_kind, identifiers)
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
    pub mod documentation_indexing; // Documentation indexing E2E tests (RAG POC)
    pub mod embedding_incremental; // Incremental embedding via file watcher tests
    pub mod embedding_pipeline; // Background embedding pipeline integration tests
    pub mod lock_contention; // Lock contention regression tests
    pub mod query_preprocessor_tests; // Query preprocessor comprehensive test suite (TDD)
    pub mod real_world_validation; // Real-world code validation tests
    pub mod reference_workspace; // Reference workspace tests
    pub mod search_regression_tests; // Regression tests for recurring search issues (glob patterns, Tantivy query semantics, limit/ranking)
    pub mod sidecar_test_helpers; // Shared fake-sidecar helpers for integration tests
    pub mod stale_index_detection; // Stale index detection tests
    pub mod tracing;
    pub mod watcher; // File watcher tests
    pub mod watcher_handlers; // File watcher handler tests (incremental indexing)
    pub mod workspace_isolation_smoke; // Fast workspace isolation smoke tests // Tracing integration tests (dogfooding tests)
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

    /// Open a SQLite database connection with proper configuration for tests
    ///
    /// **CRITICAL**: Always use this helper instead of `Connection::open()` directly!
    ///
    /// This ensures proper concurrent access configuration:
    /// - `busy_timeout`: 5 seconds (waits for locks instead of failing immediately)
    /// - `wal_autocheckpoint`: 2000 pages (~8MB) to prevent WAL corruption
    ///
    /// Without these settings, tests can corrupt databases when run concurrently
    /// with MCP server operations or other tests.
    ///
    /// # Example
    /// ```rust
    /// use crate::tests::test_helpers::open_test_connection;
    ///
    /// let conn = open_test_connection(&db_path)?;
    /// // Connection is properly configured for concurrent access
    /// ```
    pub fn open_test_connection<P: AsRef<Path>>(db_path: P) -> Result<rusqlite::Connection> {
        use rusqlite::Connection;

        let conn = Connection::open(db_path.as_ref())?;

        // Set busy timeout - wait up to 5 seconds for locks
        // This prevents immediate failures when another connection holds a lock
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        // Configure WAL autocheckpoint to prevent large WAL files
        // This prevents "database malformed" errors from WAL corruption
        conn.pragma_update(None, "wal_autocheckpoint", 2000)?;

        Ok(conn)
    }

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
// EXTRACTOR TESTS - Moved to julie-extractors crate
// ============================================================================
// All 31 language extractor tests are now in crates/julie-extractors/src/tests/
// Run with: cargo test -p julie-extractors
