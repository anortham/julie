// Integration tests for julie-semantic CLI
//
// These tests verify the semantic embedding CLI that CodeSearch MCP uses for vector search.
// Critical for ensuring HNSW index generation is reliable across platforms.

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Get path to julie-semantic binary (release build)
fn get_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("release");
    path.push(if cfg!(windows) {
        "julie-semantic.exe"
    } else {
        "julie-semantic"
    });
    path
}

/// Helper to run julie-semantic embed command
fn run_embed(
    symbols_db: &std::path::Path,
    output_dir: Option<&std::path::Path>,
    limit: Option<usize>,
) -> Result<std::process::Output> {
    let mut cmd = Command::new(get_binary_path());
    cmd.arg("embed")
        .arg("--symbols-db")
        .arg(symbols_db);

    if let Some(output) = output_dir {
        cmd.arg("--output").arg(output);
    }

    if let Some(lim) = limit {
        cmd.arg("--limit").arg(lim.to_string());
    }

    Ok(cmd.output()?)
}

/// Helper to create a test database with symbols
fn create_test_db(workspace: &std::path::Path) -> Result<PathBuf> {
    let db_path = workspace.join("test.db");

    // Use julie-codesearch to create a real database with symbols
    let codesearch_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("release")
        .join(if cfg!(windows) {
            "julie-codesearch.exe"
        } else {
            "julie-codesearch"
        });

    // Create some code files
    std::fs::write(
        workspace.join("lib.rs"),
        r#"
pub struct User {
    pub name: String,
}

pub fn create_user(name: &str) -> User {
    User { name: name.to_string() }
}

pub fn get_user_name(user: &User) -> &str {
    &user.name
}
"#,
    )?;

    // Scan to create database
    Command::new(codesearch_path)
        .arg("scan")
        .arg("--dir")
        .arg(workspace)
        .arg("--db")
        .arg(&db_path)
        .output()?;

    Ok(db_path)
}

#[cfg(test)]
mod embed_tests {
    use super::*;

    #[test]
    fn test_embed_generates_embeddings() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = create_test_db(workspace)?;

        // Run embed without output (stats only)
        let output = run_embed(&db_path, None, Some(10))?;

        assert!(
            output.status.success(),
            "Embed failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Parse JSON output
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("success"), "Output should contain success field");
        assert!(stdout.contains("embeddings_generated"), "Output should contain embedding count");
        assert!(stdout.contains("dimensions"), "Output should contain dimensions");

        // Should indicate some embeddings were generated (at least a few)
        assert!(
            stdout.contains("\"embeddings_generated\"") && stdout.contains("\"success\": true"),
            "Should have generated some embeddings: {}",
            stdout
        );

        Ok(())
    }

    #[test]
    fn test_embed_with_hnsw_output() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = create_test_db(workspace)?;
        let vectors_dir = workspace.join("vectors");

        // Run embed with output directory
        let output = run_embed(&db_path, Some(&vectors_dir), Some(20))?;

        assert!(
            output.status.success(),
            "Embed with output failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Verify HNSW index files were created
        assert!(vectors_dir.exists(), "Vectors directory should be created");

        let data_file = vectors_dir.join("hnsw_index.hnsw.data");
        let graph_file = vectors_dir.join("hnsw_index.hnsw.graph");

        assert!(data_file.exists(), "HNSW data file should be created");
        assert!(graph_file.exists(), "HNSW graph file should be created");

        // Files should not be empty
        assert!(
            std::fs::metadata(&data_file)?.len() > 0,
            "HNSW data file should not be empty"
        );
        assert!(
            std::fs::metadata(&graph_file)?.len() > 0,
            "HNSW graph file should not be empty"
        );

        Ok(())
    }

    #[test]
    fn test_embed_respects_limit() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = create_test_db(workspace)?;

        // Embed with limit of 5
        let output = run_embed(&db_path, None, Some(5))?;
        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse the JSON to check symbols_processed
        let json: serde_json::Value = serde_json::from_str(&stdout)?;
        let processed = json["symbols_processed"].as_u64().unwrap_or(0);

        // Should process at most 5 symbols (might be less if DB has fewer)
        assert!(
            processed <= 5,
            "Should process at most 5 symbols, got: {}",
            processed
        );
        assert!(processed > 0, "Should process at least 1 symbol");

        Ok(())
    }

    #[test]
    fn test_embed_empty_database() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();

        // Create empty database
        let db_path = workspace.join("empty.db");
        let codesearch_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("release")
            .join(if cfg!(windows) {
                "julie-codesearch.exe"
            } else {
                "julie-codesearch"
            });

        Command::new(codesearch_path)
            .arg("scan")
            .arg("--dir")
            .arg(workspace)
            .arg("--db")
            .arg(&db_path)
            .output()?;

        // Try to embed from empty database
        let output = run_embed(&db_path, None, None)?;

        // Should succeed but with 0 embeddings
        assert!(output.status.success(), "Embed should handle empty database gracefully");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("\"symbols_processed\": 0") || stdout.contains("\"symbols_processed\":0"),
            "Should indicate 0 symbols processed"
        );

        Ok(())
    }

    #[test]
    fn test_embed_invalid_database() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let fake_db = workspace.join("nonexistent.db");

        // Try to embed from non-existent database
        let output = run_embed(&fake_db, None, Some(10))?;

        // julie-semantic may succeed but report 0 symbols, or fail - either is acceptable
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let json: serde_json::Value = serde_json::from_str(&stdout)?;
            let processed = json["symbols_processed"].as_u64().unwrap_or(999);
            assert_eq!(processed, 0, "Non-existent DB should have 0 symbols");
        } else {
            // Also acceptable - explicit error
            assert!(!output.status.success());
        }

        Ok(())
    }

    #[test]
    fn test_embed_output_creates_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = create_test_db(workspace)?;

        // Specify nested output directory that doesn't exist
        let vectors_dir = workspace.join("nested").join("vectors").join("index");

        let output = run_embed(&db_path, Some(&vectors_dir), Some(10))?;

        assert!(output.status.success(), "Embed should create nested directories");
        assert!(vectors_dir.exists(), "Nested directory should be created");

        Ok(())
    }

    #[test]
    fn test_embed_consistent_dimensions() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = create_test_db(workspace)?;

        // Run embed
        let output = run_embed(&db_path, None, Some(10))?;
        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);

        // BGE-small should produce 384-dimensional embeddings
        assert!(
            stdout.contains("\"dimensions\": 384") || stdout.contains("\"dimensions\":384"),
            "Should use 384-dimensional embeddings (BGE-small), got: {}",
            stdout
        );

        Ok(())
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[test]
    fn test_embed_reasonable_speed() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = create_test_db(workspace)?;

        let start = std::time::Instant::now();
        let output = run_embed(&db_path, None, Some(50))?;
        let duration = start.elapsed();

        assert!(output.status.success());

        // 50 embeddings should complete in < 30 seconds (very conservative)
        assert!(
            duration.as_secs() < 30,
            "Embedding 50 symbols took too long: {:?}",
            duration
        );

        // Parse avg time from output
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(avg_line) = stdout.lines().find(|l| l.contains("avg_embedding_time_ms")) {
            println!("Performance: {}", avg_line);
        }

        Ok(())
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_embed_corrupted_database() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let bad_db = workspace.join("corrupt.db");

        // Create invalid database file
        std::fs::write(&bad_db, "This is not a valid SQLite database")?;

        let output = run_embed(&bad_db, None, Some(10))?;

        // Should fail gracefully with clear error
        assert!(
            !output.status.success(),
            "Should fail on corrupted database"
        );

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.to_lowercase().contains("database") || stderr.to_lowercase().contains("sqlite"),
            "Error message should mention database issue: {}",
            stderr
        );

        Ok(())
    }

    #[test]
    fn test_embed_readonly_output() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let db_path = create_test_db(workspace)?;

        // Try to write to readonly location
        let readonly_output = if cfg!(unix) {
            PathBuf::from("/vectors")
        } else {
            PathBuf::from("C:\\vectors")
        };

        let output = run_embed(&db_path, Some(&readonly_output), Some(10))?;

        // Should fail due to permissions
        assert!(
            !output.status.success(),
            "Should fail when output directory is readonly"
        );

        Ok(())
    }
}
