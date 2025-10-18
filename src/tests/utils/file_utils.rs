//! Tests for file_utils utilities
//! Focus on security-critical functions like secure_path_resolution

#[cfg(test)]
mod file_utils_tests {
    use super::super::*;
    use crate::utils::file_utils::secure_path_resolution;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_secure_path_resolution_valid_relative_path() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        // Create a test file
        let test_file = workspace_root.join("test.txt");
        fs::write(&test_file, "test content")?;

        // Valid relative path should resolve correctly
        let result = secure_path_resolution("test.txt", workspace_root)?;
        assert_eq!(result, test_file.canonicalize()?);

        Ok(())
    }

    #[test]
    fn test_secure_path_resolution_valid_absolute_path_within_workspace() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        // Create a test file
        let test_file = workspace_root.join("test.txt");
        fs::write(&test_file, "test content")?;
        let canonical_path = test_file.canonicalize()?;

        // Valid absolute path within workspace should work
        let result = secure_path_resolution(&canonical_path.to_string_lossy(), workspace_root)?;
        assert_eq!(result, canonical_path);

        Ok(())
    }

    #[test]
    fn test_secure_path_resolution_blocks_absolute_path_outside_workspace() {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        // Try to access /etc/passwd
        let result = secure_path_resolution("/etc/passwd", workspace_root);
        assert!(result.is_err(), "Should block absolute path outside workspace");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains("Security") || error_msg.contains("traversal"));
    }

    #[test]
    fn test_secure_path_resolution_blocks_relative_traversal() {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        // Try relative path traversal
        let result = secure_path_resolution("../../../../etc/passwd", workspace_root);
        assert!(result.is_err(), "Should block relative path traversal");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains("Security") || error_msg.contains("traversal"));
    }

    #[test]
    fn test_secure_path_resolution_blocks_symlink_outside_workspace() {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        // Create a symlink pointing outside workspace
        let symlink_path = workspace_root.join("evil_link");
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/passwd", &symlink_path).unwrap();

        #[cfg(unix)]
        {
            let result = secure_path_resolution("evil_link", workspace_root);
            assert!(result.is_err(), "Should block symlink outside workspace");
            let error_msg = format!("{}", result.unwrap_err());
            assert!(error_msg.contains("Security") || error_msg.contains("traversal"));
        }
    }

    #[test]
    fn test_secure_path_resolution_nonexistent_file() {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        // Try to access a file that doesn't exist
        let result = secure_path_resolution("nonexistent.txt", workspace_root);
        assert!(result.is_err(), "Should fail for nonexistent files");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains("does not exist"));
    }

    #[test]
    fn test_secure_path_resolution_nested_directories() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        // Create nested directory structure
        let nested_dir = workspace_root.join("src").join("tools");
        fs::create_dir_all(&nested_dir)?;
        let test_file = nested_dir.join("test.rs");
        fs::write(&test_file, "test content")?;

        // Valid nested path should work
        let result = secure_path_resolution("src/tools/test.rs", workspace_root)?;
        assert_eq!(result, test_file.canonicalize()?);

        Ok(())
    }

    #[test]
    fn test_secure_path_resolution_dot_navigation() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path();

        // Create nested structure
        let nested_dir = workspace_root.join("src").join("tools");
        fs::create_dir_all(&nested_dir)?;
        let test_file = nested_dir.join("test.rs");
        fs::write(&test_file, "test content")?;

        // Path with ./ should work
        let result = secure_path_resolution("./src/tools/test.rs", workspace_root)?;
        assert_eq!(result, test_file.canonicalize()?);

        Ok(())
    }
}