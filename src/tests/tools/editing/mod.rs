//! Editing tool tests
//!
//! This module contains tests for editing tools:
//! - FuzzyReplaceTool: Fuzzy pattern matching and replacement
//! - EditLinesTool: Line-level editing operations
//! - EditingTransaction: Transaction-based editing safety
//!
//! Also includes legacy EditingTransaction tests extracted from inline tests.

// Test submodules
pub mod edit_lines;
pub mod fuzzy_replace; // FuzzyReplaceTool comprehensive tests // EditLinesTool tests
pub mod transactional_editing_tests; // EditingTransaction and MultiFileTransaction tests

// Legacy EditingTransaction tests
use crate::tools::editing::EditingTransaction;
use std::env;
use std::fs;

struct DirGuard {
    original: std::path::PathBuf,
}

impl DirGuard {
    fn change_to(path: &std::path::Path) -> Self {
        let original = env::current_dir().expect("cwd");
        env::set_current_dir(path).expect("set cwd");
        Self { original }
    }
}

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = env::set_current_dir(&self.original);
    }
}

#[test]
fn commit_creates_temp_file_in_same_directory_for_relative_paths() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let _guard = DirGuard::change_to(temp_dir.path());

    fs::create_dir_all("src").expect("create dir");

    let transaction = EditingTransaction::begin("src/lib.rs").expect("begin");
    transaction
        .commit("pub fn demo() {}")
        .expect("commit should succeed");

    let written = fs::read_to_string("src/lib.rs").expect("read file");
    assert_eq!(written, "pub fn demo() {}");
}
