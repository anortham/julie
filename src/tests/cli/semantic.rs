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
    cmd.arg("embed").arg("--symbols-db").arg(symbols_db);

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
        assert!(
            stdout.contains("success"),
            "Output should contain success field"
        );
        assert!(
            stdout.contains("embeddings_generated"),
            "Output should contain embedding count"
        );
        assert!(
            stdout.contains("dimensions"),
            "Output should contain dimensions"
        );

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
        assert!(
            output.status.success(),
            "Embed should handle empty database gracefully"
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("\"symbols_processed\": 0")
                || stdout.contains("\"symbols_processed\":0"),
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

        assert!(
            output.status.success(),
            "Embed should create nested directories"
        );
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
    #[cfg_attr(windows, ignore = "Windows permissions for C:\\ are unpredictable")]
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

#[cfg(test)]
mod query_tests {
    use super::*;

    /// Helper to run julie-semantic query command
    fn run_query(text: &str, model: Option<&str>) -> Result<std::process::Output> {
        let mut cmd = Command::new(get_binary_path());
        cmd.arg("query").arg("--text").arg(text);

        if let Some(m) = model {
            cmd.arg("--model").arg(m);
        }

        Ok(cmd.output()?)
    }

    #[test]
    fn test_query_generates_embedding() -> Result<()> {
        // Run query for a simple text
        let output = run_query("function getUserData", None)?;

        assert!(
            output.status.success(),
            "Query failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Parse JSON output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let embedding: Vec<f32> = serde_json::from_str(&stdout)?;

        // Should have 384 dimensions (BGE-small)
        assert_eq!(
            embedding.len(),
            384,
            "Query embedding should have 384 dimensions"
        );

        // Embedding should be normalized (approximately)
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            magnitude > 0.0 && magnitude < 2.0,
            "Embedding magnitude should be reasonable: {}",
            magnitude
        );

        Ok(())
    }

    #[test]
    fn test_query_same_text_same_embedding() -> Result<()> {
        let text = "class UserRepository";

        // Run query twice with same text
        let output1 = run_query(text, None)?;
        let output2 = run_query(text, None)?;

        assert!(output1.status.success());
        assert!(output2.status.success());

        let embedding1: Vec<f32> = serde_json::from_str(&String::from_utf8_lossy(&output1.stdout))?;
        let embedding2: Vec<f32> = serde_json::from_str(&String::from_utf8_lossy(&output2.stdout))?;

        // Should be identical
        assert_eq!(
            embedding1, embedding2,
            "Same query should produce identical embeddings"
        );

        Ok(())
    }

    #[test]
    fn test_query_different_text_different_embeddings() -> Result<()> {
        let text1 = "function getUserData";
        let text2 = "class DatabaseConnection";

        let output1 = run_query(text1, None)?;
        let output2 = run_query(text2, None)?;

        assert!(output1.status.success());
        assert!(output2.status.success());

        let embedding1: Vec<f32> = serde_json::from_str(&String::from_utf8_lossy(&output1.stdout))?;
        let embedding2: Vec<f32> = serde_json::from_str(&String::from_utf8_lossy(&output2.stdout))?;

        // Should be different
        assert_ne!(
            embedding1, embedding2,
            "Different queries should produce different embeddings"
        );

        Ok(())
    }

    #[test]
    fn test_query_semantic_similarity() -> Result<()> {
        // Similar concepts should have higher cosine similarity
        let output1 = run_query("authentication function", None)?;
        let output2 = run_query("login method", None)?;
        let output3 = run_query("database table", None)?;

        assert!(output1.status.success());
        assert!(output2.status.success());
        assert!(output3.status.success());

        let emb1: Vec<f32> = serde_json::from_str(&String::from_utf8_lossy(&output1.stdout))?;
        let emb2: Vec<f32> = serde_json::from_str(&String::from_utf8_lossy(&output2.stdout))?;
        let emb3: Vec<f32> = serde_json::from_str(&String::from_utf8_lossy(&output3.stdout))?;

        // Calculate cosine similarities
        let sim_12 = cosine_similarity(&emb1, &emb2);
        let sim_13 = cosine_similarity(&emb1, &emb3);

        // "authentication" and "login" should be more similar than "authentication" and "database"
        assert!(
            sim_12 > sim_13,
            "Semantic similarity should be higher for related concepts (auth/login: {}, auth/db: {})",
            sim_12,
            sim_13
        );

        // Should have reasonable similarity (>0.3 for related concepts)
        assert!(
            sim_12 > 0.3,
            "Related concepts should have >0.3 similarity, got: {}",
            sim_12
        );

        Ok(())
    }

    /// Calculate cosine similarity between two vectors (helper for tests)
    fn cosine_similarity(vec_a: &[f32], vec_b: &[f32]) -> f32 {
        if vec_a.len() != vec_b.len() {
            return 0.0;
        }

        let dot_product: f32 = vec_a.iter().zip(vec_b.iter()).map(|(a, b)| a * b).sum();
        let norm_a: f32 = vec_a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = vec_b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a * norm_b)
    }

    #[test]
    fn test_query_empty_text() -> Result<()> {
        let output = run_query("", None)?;

        // Should succeed (empty text is valid, just produces a generic embedding)
        assert!(
            output.status.success(),
            "Query with empty text should succeed"
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let embedding: Vec<f32> = serde_json::from_str(&stdout)?;

        // Should still produce 384-dimensional embedding
        assert_eq!(embedding.len(), 384);

        Ok(())
    }

    #[test]
    fn test_query_long_text() -> Result<()> {
        // Test with text longer than typical token limit
        let long_text = "function getUserData ".repeat(100);

        let output = run_query(&long_text, None)?;

        assert!(
            output.status.success(),
            "Query with long text should succeed (model will truncate)"
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let embedding: Vec<f32> = serde_json::from_str(&stdout)?;

        assert_eq!(embedding.len(), 384);

        Ok(())
    }

    #[test]
    fn test_query_special_characters() -> Result<()> {
        // Test with special characters and code symbols
        let text = "function(user: User): Promise<Data> => { return data; }";

        let output = run_query(text, None)?;

        assert!(
            output.status.success(),
            "Query with special characters should succeed"
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let embedding: Vec<f32> = serde_json::from_str(&stdout)?;

        assert_eq!(embedding.len(), 384);

        Ok(())
    }

    #[test]
    fn test_query_model_parameter() -> Result<()> {
        // Test explicit model parameter
        let output = run_query("getUserData", Some("bge-small"))?;

        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);
        let embedding: Vec<f32> = serde_json::from_str(&stdout)?;

        assert_eq!(embedding.len(), 384);

        Ok(())
    }

    #[test]
    fn test_query_json_format() -> Result<()> {
        let output = run_query("test query", None)?;

        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should be valid JSON
        let parse_result = serde_json::from_str::<Vec<f32>>(&stdout);
        assert!(
            parse_result.is_ok(),
            "Output should be valid JSON array: {}",
            stdout
        );

        // Should be a JSON array
        assert!(
            stdout.trim().starts_with('[') && stdout.trim().ends_with(']'),
            "Output should be a JSON array"
        );

        Ok(())
    }

    #[test]
    fn test_query_performance() -> Result<()> {
        let start = std::time::Instant::now();
        let output = run_query("function getUserData", None)?;
        let duration = start.elapsed();

        assert!(output.status.success());

        // Query should complete in < 5 seconds (includes model loading)
        assert!(
            duration.as_secs() < 5,
            "Query took too long: {:?}",
            duration
        );

        Ok(())
    }

    #[test]
    fn test_query_cross_language_similarity() -> Result<()> {
        // Test that similar concepts in different languages have high similarity
        let ts_query = "interface User { id: string; name: string; }";
        let cs_query = "class User { public string Id; public string Name; }";
        let py_query = "class User: def __init__(self, id, name): ...";

        let output_ts = run_query(ts_query, None)?;
        let output_cs = run_query(cs_query, None)?;
        let output_py = run_query(py_query, None)?;

        assert!(output_ts.status.success());
        assert!(output_cs.status.success());
        assert!(output_py.status.success());

        let emb_ts: Vec<f32> = serde_json::from_str(&String::from_utf8_lossy(&output_ts.stdout))?;
        let emb_cs: Vec<f32> = serde_json::from_str(&String::from_utf8_lossy(&output_cs.stdout))?;
        let emb_py: Vec<f32> = serde_json::from_str(&String::from_utf8_lossy(&output_py.stdout))?;

        // Cross-language similarity for same concept (User class) should be >0.5
        let sim_ts_cs = cosine_similarity(&emb_ts, &emb_cs);
        let sim_ts_py = cosine_similarity(&emb_ts, &emb_py);

        assert!(
            sim_ts_cs > 0.5,
            "TypeScript/C# User class similarity should be >0.5, got: {}",
            sim_ts_cs
        );
        assert!(
            sim_ts_py > 0.4,
            "TypeScript/Python User class similarity should be >0.4, got: {}",
            sim_ts_py
        );

        Ok(())
    }
}
