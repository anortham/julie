// Integration tests for julie-codesearch CLI
//
// These tests verify the CLI interface that CodeSearch MCP server calls.
// Critical for ensuring cross-platform binary reliability.

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Get path to julie-codesearch binary (release build)
fn get_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("release");
    path.push(if cfg!(windows) {
        "julie-codesearch.exe"
    } else {
        "julie-codesearch"
    });
    path
}

/// Helper to run julie-codesearch scan command
fn run_scan(workspace: &std::path::Path, db: &std::path::Path) -> Result<std::process::Output> {
    let output = Command::new(get_binary_path())
        .arg("scan")
        .arg("--dir")
        .arg(workspace)
        .arg("--db")
        .arg(db)
        .output()?;
    Ok(output)
}

/// Helper to run julie-codesearch update command
fn run_update(file: &std::path::Path, db: &std::path::Path) -> Result<std::process::Output> {
    let output = Command::new(get_binary_path())
        .arg("update")
        .arg("--file")
        .arg(file)
        .arg("--db")
        .arg(db)
        .output()?;
    Ok(output)
}

#[cfg(test)]
mod scan_tests {
    use super::*;

    #[test]
    fn test_scan_creates_database() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = workspace.join("test.db");

        // Create a simple Rust file
        std::fs::write(
            workspace.join("test.rs"),
            r#"pub fn hello() { println!("Hello"); }"#,
        )?;

        // Run scan
        let output = run_scan(workspace, &db_path)?;

        // Verify command succeeded
        assert!(
            output.status.success(),
            "Scan failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Verify database was created
        assert!(db_path.exists(), "Database file was not created");

        // Verify database has content
        let conn = rusqlite::Connection::open(&db_path)?;
        let symbol_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM symbols",
            [],
            |row| row.get(0),
        )?;

        assert!(symbol_count > 0, "No symbols were extracted");

        Ok(())
    }

    #[test]
    fn test_scan_extracts_symbols_correctly() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = workspace.join("test.db");

        // Create file with known symbols
        std::fs::write(
            workspace.join("lib.rs"),
            r#"
pub struct User {
    pub name: String,
    pub email: String,
}

pub fn create_user(name: &str) -> User {
    User {
        name: name.to_string(),
        email: String::new(),
    }
}
"#,
        )?;

        // Run scan
        let output = run_scan(workspace, &db_path)?;
        assert!(output.status.success());

        // Verify symbols were extracted
        let conn = rusqlite::Connection::open(&db_path)?;

        // Should have extracted: User struct, create_user function, and likely the struct fields
        let symbol_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM symbols WHERE name IN ('User', 'create_user', 'name', 'email')",
            [],
            |row| row.get(0),
        )?;

        assert!(symbol_count >= 2, "Failed to extract expected symbols");

        Ok(())
    }

    #[test]
    fn test_scan_handles_empty_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = workspace.join("empty.db");

        // Run scan on empty directory
        let output = run_scan(workspace, &db_path)?;

        // Should succeed (empty result is valid)
        assert!(
            output.status.success(),
            "Scan failed on empty directory: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Database should exist but be empty
        let conn = rusqlite::Connection::open(&db_path)?;
        let file_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM files",
            [],
            |row| row.get(0),
        )?;

        assert_eq!(file_count, 0, "Empty directory should have no files");

        Ok(())
    }

    #[test]
    fn test_scan_stores_file_content() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = workspace.join("test.db");

        let file_content = r#"pub fn test() { println!("test"); }"#;
        std::fs::write(workspace.join("test.rs"), file_content)?;

        // Run scan
        let output = run_scan(workspace, &db_path)?;
        assert!(output.status.success());

        // Verify file content is stored
        let conn = rusqlite::Connection::open(&db_path)?;
        let stored_content: String = conn.query_row(
            "SELECT content FROM files WHERE path LIKE '%test.rs'",
            [],
            |row| row.get(0),
        )?;

        assert_eq!(stored_content, file_content, "File content not stored correctly");

        Ok(())
    }

    #[test]
    fn test_scan_calculates_blake3_hash() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = workspace.join("test.db");

        std::fs::write(workspace.join("test.rs"), "pub fn test() {}")?;

        // Run scan
        let output = run_scan(workspace, &db_path)?;
        assert!(output.status.success());

        // Verify hash is stored and non-empty
        let conn = rusqlite::Connection::open(&db_path)?;
        let hash: String = conn.query_row(
            "SELECT hash FROM files WHERE path LIKE '%test.rs'",
            [],
            |row| row.get(0),
        )?;

        assert!(!hash.is_empty(), "Hash should not be empty");
        assert_eq!(hash.len(), 64, "Blake3 hash should be 64 hex chars");

        Ok(())
    }
}

#[cfg(test)]
mod update_tests {
    use super::*;

    #[test]
    fn test_update_adds_new_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = workspace.join("test.db");
        let test_file = workspace.join("new.rs");

        // Create initial database
        run_scan(workspace, &db_path)?;

        // Add new file
        std::fs::write(&test_file, "pub fn new_function() {}")?;

        // Run update
        let output = run_update(&test_file, &db_path)?;
        assert!(
            output.status.success(),
            "Update failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Verify file was added
        let conn = rusqlite::Connection::open(&db_path)?;
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM files WHERE path LIKE '%new.rs')",
            [],
            |row| row.get(0),
        )?;

        assert!(exists, "New file was not added to database");

        Ok(())
    }

    #[test]
    fn test_update_detects_unchanged_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = workspace.join("test.db");
        let test_file = workspace.join("test.rs");

        std::fs::write(&test_file, "pub fn test() {}")?;

        // Initial scan
        run_scan(workspace, &db_path)?;

        // Update same file (no changes)
        let output = run_update(&test_file, &db_path)?;
        assert!(output.status.success());

        // Check stderr for "skipped" or "unchanged" message
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("skipped") || stderr.contains("unchanged") || stderr.contains("0."),
            "Should indicate file was skipped: {}",
            stderr
        );

        Ok(())
    }

    #[test]
    fn test_update_detects_changes() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = workspace.join("test.db");
        let test_file = workspace.join("test.rs");

        // Initial version
        std::fs::write(&test_file, "pub fn test_v1() {}")?;
        run_scan(workspace, &db_path)?;

        // Get initial symbol count
        let initial_symbols: Vec<String> = {
            let conn = rusqlite::Connection::open(&db_path)?;
            let symbols: Vec<String> = conn
                .prepare("SELECT name FROM symbols")?
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            drop(conn); // Close connection before CLI update
            symbols
        };

        // Modify file
        std::fs::write(&test_file, "pub fn test_v2() {}")?;

        // Update
        let output = run_update(&test_file, &db_path)?;
        assert!(
            output.status.success(),
            "Update failed: {}\nStderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        // Verify symbols were updated
        let updated_symbols: Vec<String> = {
            let conn = rusqlite::Connection::open(&db_path)?;
            let symbols: Vec<String> = conn
                .prepare("SELECT name FROM symbols")?
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            symbols
        };

        assert_ne!(
            initial_symbols, updated_symbols,
            "Symbols should have been updated"
        );

        Ok(())
    }

    #[test]
    fn test_update_nonexistent_file_fails() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = workspace.join("test.db");

        // Create database
        run_scan(workspace, &db_path)?;

        // Try to update non-existent file
        let fake_file = workspace.join("nonexistent.rs");
        let output = run_update(&fake_file, &db_path)?;

        // Should fail gracefully
        assert!(
            !output.status.success(),
            "Update should fail for non-existent file"
        );

        Ok(())
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_scan_invalid_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let invalid_dir = temp_dir.path().join("nonexistent");

        let output = run_scan(&invalid_dir, &db_path)?;

        // Should fail gracefully
        assert!(
            !output.status.success(),
            "Should fail on invalid directory"
        );

        Ok(())
    }

    #[test]
    fn test_scan_readonly_db_path() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();

        // Create file to scan
        std::fs::write(workspace.join("test.rs"), "pub fn test() {}")?;

        // Try to create DB in readonly location (e.g., root on unix)
        let readonly_db = if cfg!(unix) {
            PathBuf::from("/test.db")
        } else {
            PathBuf::from("C:\\test.db")
        };

        let output = run_scan(workspace, &readonly_db)?;

        // Should fail due to permissions
        assert!(
            !output.status.success(),
            "Should fail when DB path is readonly"
        );

        Ok(())
    }
}
