// Julie's Test Infrastructure
//
// This module contains test utilities and infrastructure for testing extractors,
// search functionality, editing tools, and other Julie components.

pub mod editing_safety_tests; // CRITICAL safety tests for editing tools
pub mod editing_tests;
pub mod fast_edit_search_replace_tests;
pub mod line_edit_control_tests; // SOURCE/CONTROL tests for LineEditTool
pub mod line_edit_tests;
pub mod refactoring_tests; // Smart refactoring tool tests
pub mod smart_refactor_control_tests; // SOURCE/CONTROL tests for SmartRefactorTool

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

// ALL 26 Extractor test modules - NO EXCEPTIONS, ALL MUST PASS
pub mod bash_tests; // Bash extractor tests
pub mod c; // C extractor tests
pub mod cpp; // C++ extractor tests
pub mod csharp; // C# extractor tests (modularized)
pub mod css; // CSS extractor tests
pub mod dart_tests; // Dart extractor tests
pub mod gdscript; // GDScript extractor tests
pub mod go_tests; // Go extractor tests
pub mod html; // HTML extractor tests
pub mod java; // Java extractor tests (split into modules)
pub mod javascript_tests; // JavaScript extractor tests
pub mod kotlin_tests; // Kotlin extractor tests
pub mod lua; // Lua extractor tests
pub mod php_tests; // PHP extractor tests
pub mod powershell_tests; // PowerShell extractor tests
pub mod python_tests; // Python extractor tests
pub mod razor_tests; // Razor extractor tests
pub mod regex_tests; // Regex extractor tests
pub mod ruby_tests; // Ruby extractor tests
pub mod rust_tests; // Rust extractor tests
pub mod sql; // SQL extractor tests
pub mod swift_tests; // Swift extractor tests
pub mod typescript_tests; // TypeScript extractor tests
pub mod vue_tests; // Vue extractor tests
pub mod zig_tests; // Zig extractor tests

// Debug-specific test modules for troubleshooting
// pub mod debug_c_failures;       // Debug C extractor specific failures - TEMP DISABLED

// Real-World Validation Tests (following Miller's proven methodology)
pub mod real_world_validation; // Tests all extractors against real-world code files

// Cross-Language Tracing Tests (Phase 5 - The Revolutionary Feature)
// pub mod tracing_tests; // Tests the killer feature - cross-language data flow tracing - TEMP DISABLED

// Phase 6.1 Intelligence Tools Tests (Heart of Codebase)
// pub mod intelligence_tools_tests; // Tests for AI-native code intelligence tools - TEMP DISABLED

// Token Optimization Tests (CRITICAL - Fixes 149K token explosion)
pub mod exact_match_boost_tests; // Tests for exact match boost with logarithmic scoring
pub mod exploration_tools_tests; // Tests for exploration tool token optimization (FastExploreTool, FindLogicTool)
pub mod find_logic_tests; // Tests for FindLogicTool token optimization
pub mod navigation_tools_tests; // Tests for navigation tool token optimization (FastRefsTool, FastGotoTool)
pub mod path_relevance_tests; // Tests for path relevance scoring system
pub mod search_quality_tests; // Tests for PathRelevanceScorer integration into search tools
pub mod search_tools_tests; // Tests for search tool token optimization and response formatting
pub mod watcher_tests;
pub mod workspace_mod_tests; // Tests for workspace module functionality // Tests extracted from the watcher implementation
