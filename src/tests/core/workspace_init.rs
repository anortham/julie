// Workspace initialization and root detection tests
//
// Tests for the workspace root detection logic that determines where
// Julie creates its .julie directory based on CLI args, environment
// variables, and current working directory.

use serial_test::serial;
use std::env;
use std::fs;
#[allow(unused_imports)] // consumed by sub-mods via `use super::*`
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// env_paths — relocated to julie-runtime (T2c.3); only handler-free tests moved
mod handler_binding;
mod indexing_env;
mod instructions_paths;
mod root_detection;

/// Helper to create a test directory structure
fn setup_test_workspace() -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp dir");
    fs::create_dir_all(temp.path().join("src")).expect("Failed to create src dir");
    temp
}
