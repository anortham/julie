// Julie's Test Infrastructure
//
// This module contains test utilities and infrastructure for testing extractors,
// search functionality, editing tools, and other Julie components.
//
// 📝 NOTE: 7 SafeEditTool integration test files (3,714 lines) have been preserved
// in src/tests/tools/editing/disabled/ for migration to FuzzyReplaceTool and
// EditLinesTool integration tests. See README.md in that directory for details.
//
// Current coverage: 24 unit tests passing (fuzzy_replace.rs + edit_lines.rs)
// TODO: Add integration tests for concurrency, permissions, UTF-8, security

// ============================================================================
// CLI TESTS - Command-line interface integration tests
// ============================================================================
pub mod cli {
    pub mod codesearch; // CLI integration tests for julie-codesearch (scan/update)
    pub mod output; // CLI output formatting tests
    pub mod parallel; // CLI parallel execution tests
    pub mod progress;
    pub mod semantic; // CLI integration tests for julie-semantic (embed with HNSW)
                      // CLI progress indicator tests
}

// ============================================================================
// CORE SYSTEM TESTS - Database, embeddings, handlers, language support
// ============================================================================
pub mod core {
    pub mod database; // Database operations and SQLite tests
    pub mod handler; // MCP handler tests
    pub mod language; // Language detection and support tests
    pub mod tracing; // Tracing and logging tests
    pub mod workspace_init; // Workspace root detection and initialization tests

    pub mod embeddings; // Embedding tests with cross-language support
}

// ============================================================================
// TOOLS TESTS - Search, editing, refactoring, navigation, exploration
// ============================================================================
pub mod tools {
    pub mod ast_symbol_finder; // AST symbol finder tests
    pub mod get_symbols; // GetSymbolsTool tests
    pub mod get_symbols_reference_workspace; // GetSymbolsTool reference workspace bug test
    pub mod get_symbols_relative_paths; // GetSymbolsTool Phase 2 relative path tests (TDD)
    pub mod get_symbols_smart_read; // GetSymbolsTool Phase 2 - Smart Read with code bodies
    pub mod get_symbols_target_filtering; // GetSymbolsTool target filtering tests
    pub mod get_symbols_token; // GetSymbolsTool token optimization tests
    pub mod smart_read; // Smart Read token optimization tests
    // syntax_validation removed - abandoned AutoFixSyntax feature (Oct 2025)

    pub mod editing; // Editing tool tests (FuzzyReplaceTool, EditLinesTool)

    pub mod search; // Search tool tests (line mode, quality, race conditions)
    pub mod search_quality; // Search quality dogfooding tests (regression suite)
    pub mod search_context_lines; // FastSearchTool context_lines parameter tests (token optimization)

    pub mod refactoring; // Refactoring tool tests (SmartRefactorTool with SOURCE/CONTROL)

    pub mod workspace {
        pub mod isolation; // Workspace isolation tests
        pub mod management_token; // ManageWorkspaceTool token optimization tests
        pub mod mod_tests; // Workspace module functionality tests
        pub mod registry; // Workspace registry tests
        pub mod registry_service;
        pub mod utils; // Workspace utilities tests // Registry service tests
    }

    pub mod navigation; // Navigation tool tests (FastRefsTool, FastGotoTool)

    pub mod exploration; // Exploration tool tests (FastExploreTool, FindLogicTool)

    pub mod trace_call_path; // TraceCallPathTool tests (core + comprehensive)
}

// ============================================================================
// UTILS TESTS - Cross-language intelligence, scoring, optimization utilities
// ============================================================================
pub mod utils {
    pub mod context_truncation; // Context truncation tests
    pub mod cross_language_intelligence; // Cross-language intelligence tests
    pub mod progressive_reduction; // Progressive reduction tests
    pub mod query_expansion; // Query expansion tests
    pub mod token_estimation; // Token estimation tests
    pub mod utf8_truncation; // UTF-8 safe string truncation tests
    pub mod utf8_boundary_safety; // UTF-8 boundary safety checks for unsafe slicing patterns

    pub mod exact_match_boost; // Exact match boost tests

    pub mod path_relevance; // Path relevance scoring tests
}

// ============================================================================
// INTEGRATION TESTS - End-to-end and cross-component tests
// ============================================================================
pub mod integration {
    pub mod bulk_storage_atomicity; // Bulk storage atomicity tests (TDD) - verify transaction safety
    pub mod fts5_integrity; // FTS5 integrity check and auto-rebuild tests (TDD)
    pub mod fts5_minimal_repro; // FTS5 corruption minimal reproduction test
    pub mod fts5_rowid_corruption; // FTS5 rowid corruption from unnecessary rebuild (TDD)
    pub mod fts5_sanitization; // FTS5 query sanitization tests
    pub mod lock_contention; // Lock contention regression tests
    pub mod query_preprocessor_tests; // Query preprocessor comprehensive test suite (TDD)
    pub mod real_world_validation; // Real-world code validation tests
    pub mod reference_workspace; // Reference workspace tests
    pub mod search_regression_tests; // Regression tests for recurring search issues (glob patterns, FTS5 syntax, limit/ranking)
    pub mod stale_index_detection; // Stale index detection tests
    pub mod watcher; // File watcher tests
    pub mod workspace_isolation_smoke; // Fast workspace isolation smoke tests
                                       // pub mod tracing;                 // Tracing integration tests - DISABLED
                                       // pub mod intelligence_tools;      // Intelligence tools integration tests - DISABLED
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

// ============================================================================
// EXTRACTOR TESTS - All 26 language extractors (100% Miller parity)
// ============================================================================
pub mod extractors {
    pub mod base; // BaseExtractor tests

    // Bash extractor tests (single mod.rs file)
    pub mod bash;

    // C extractor tests (multiple submodules)
    pub mod c;

    // C++ extractor tests (multiple submodules)
    pub mod cpp;

    // C# extractor tests (has own mod.rs with submodule declarations)
    pub mod csharp;

    // CSS extractor tests (multiple submodules)
    pub mod css;

    // Dart extractor tests (has own mod.rs with submodule declarations)
    pub mod dart;

    // GDScript extractor tests (multiple submodules)
    pub mod gdscript;

    // Go extractor tests (single mod.rs file)
    pub mod go;

    // HTML extractor tests (multiple submodules)
    pub mod html;

    // Java extractor tests
    pub mod java;

    // JavaScript extractor tests
    pub mod javascript;

    // Kotlin extractor tests (single mod.rs file)
    pub mod kotlin;

    // Lua extractor tests (multiple submodules)
    pub mod lua;

    // PHP extractor tests (single mod.rs file)
    pub mod php;

    // PowerShell extractor tests (single mod.rs file)
    pub mod powershell;

    // Python extractor tests (multiple submodules - declarations in python/mod.rs)
    pub mod python;

    // Razor extractor tests (single mod.rs file)
    pub mod razor;

    // Regex extractor tests (multiple submodules)
    pub mod regex;

    // Ruby extractor tests
    pub mod ruby;

    // Rust extractor tests (multiple submodules)
    pub mod rust;

    // SQL extractor tests (multiple submodules)
    pub mod sql;

    // Swift extractor tests (single mod.rs file)
    pub mod swift;

    // TypeScript extractor tests (multiple submodules - declarations in typescript/mod.rs)
    pub mod typescript;

    // Vue extractor tests
    pub mod vue;

    // Zig extractor tests
    pub mod zig;
}
