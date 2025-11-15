/// Test helper for creating unique temporary directories
///
/// Prevents parallel test conflicts by ensuring each test gets a unique temp directory
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Create a uniquely named temporary directory for parallel test execution
///
/// This prevents flaky tests caused by temp directory conflicts when tests run in parallel.
/// Each invocation gets a unique counter-based directory name.
///
/// # Examples
///
/// ```no_run
/// use crate::tests::helpers::unique_temp_dir;
///
/// let temp_dir = unique_temp_dir("my_test");
/// let workspace_root = temp_dir.path();
/// // Use workspace_root for test...
/// ```
pub fn unique_temp_dir(test_name: &str) -> TempDir {
    let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
    let unique_name = format!("julie_test_{}_{}", test_name, counter);

    tempfile::Builder::new()
        .prefix(&unique_name)
        .tempdir()
        .expect("Failed to create unique temp directory")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_unique_temp_dirs_are_different() {
        let dir1 = unique_temp_dir("test");
        let dir2 = unique_temp_dir("test");
        let dir3 = unique_temp_dir("test");

        let path1 = dir1.path().to_path_buf();
        let path2 = dir2.path().to_path_buf();
        let path3 = dir3.path().to_path_buf();

        // All paths must be different
        let mut paths = HashSet::new();
        paths.insert(path1);
        paths.insert(path2);
        paths.insert(path3);

        assert_eq!(paths.len(), 3, "All temp dirs should be unique");
    }

    #[test]
    fn test_unique_temp_dir_names_contain_test_name() {
        let dir = unique_temp_dir("my_awesome_test");
        let path_str = dir.path().to_string_lossy();

        assert!(
            path_str.contains("julie_test_my_awesome_test"),
            "Temp dir name should contain test name: {}",
            path_str
        );
    }
}
