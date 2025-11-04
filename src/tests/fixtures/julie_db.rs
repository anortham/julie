//! Pre-indexed snapshot of Julie's codebase for testing
//! Eliminates 60s reindexing per test, loads in <100ms

use anyhow::{bail, Result};
use crate::tests::test_helpers::open_test_connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use tempfile::TempDir;

/// Pre-indexed snapshot of Julie's codebase for testing
pub struct JulieTestFixture {
    /// Path to read-only fixture database
    fixture_db_path: PathBuf,
    /// Metadata about indexed content
    pub metadata: FixtureMetadata,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FixtureMetadata {
    /// Snapshot creation date
    pub created_at: String,
    /// Number of files indexed
    pub file_count: usize,
    /// Number of symbols indexed
    pub symbol_count: usize,
    /// Indexed file paths (for assertions)
    pub indexed_files: Vec<String>,
    /// Known symbols per file (for test assertions)
    pub known_symbols: HashMap<String, Vec<String>>,
}

impl JulieTestFixture {
    /// Build fixture database (run once manually, checked into git)
    pub async fn build() -> Result<Self> {
        use crate::handler::JulieServerHandler;
        use crate::tools::workspace::ManageWorkspaceTool;

        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/databases/julie-snapshot");

        // Clean any existing fixture
        if fixture_dir.exists() {
            fs::remove_dir_all(&fixture_dir)?;
        }
        fs::create_dir_all(&fixture_dir)?;

        println!("🔨 Building Julie test fixture...");

        // Create handler and index Julie's codebase (matches dogfooding tests)
        let handler = JulieServerHandler::new().await?;

        // Use ManageWorkspaceTool to index (this handles workspace initialization)
        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(env!("CARGO_MANIFEST_DIR").to_string()),  // Explicit Julie root
            force: Some(true),  // Force rebuild
            name: None,
            workspace_id: None,
            detailed: None,
        };

        index_tool.call_tool(&handler).await?;

        println!("✅ Indexing triggered, waiting for completion...");

        // Wait for SQLite FTS5 indexing to complete
        Self::wait_for_indexing(&handler).await?;

        println!("✅ Indexing complete, extracting database...");

        // Find the database (workspace ID is deterministic based on path)
        let julie_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".julie");
        let indexes_dir = julie_dir.join("indexes");

        // Find the workspace directory (should be only one for primary workspace)
        let workspace_dirs: Vec<_> = fs::read_dir(&indexes_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        if workspace_dirs.is_empty() {
            bail!("No workspace directory found in .julie/indexes/");
        }

        // For Julie, we want the primary workspace (not reference workspaces)
        // The primary workspace ID is generated from the Julie root path
        use crate::workspace::registry::generate_workspace_id;
        let julie_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let expected_workspace_id = generate_workspace_id(&julie_root.to_string_lossy())?;

        let workspace_dir = indexes_dir.join(&expected_workspace_id);
        if !workspace_dir.exists() {
            bail!(
                "Primary workspace directory not found. Expected: {}, Available: {:?}",
                &expected_workspace_id,
                workspace_dirs.iter().map(|d| d.file_name()).collect::<Vec<_>>()
            );
        }

        let source_db = workspace_dir.join("db/symbols.db");

        if !source_db.exists() {
            bail!("Database not found at: {}", source_db.display());
        }

        // Checkpoint WAL before copying to ensure single-file database
        // This consolidates WAL changes into the main DB file
        println!("⏳ Checkpointing WAL before copy...");
        {
            let conn = open_test_connection(&source_db)?;
            // PRAGMA wal_checkpoint returns (busy, log, checkpointed) as results
            let _: (i64, i64, i64) = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            println!("✅ WAL checkpointed and truncated");
        }

        // Copy database to fixture location
        let fixture_db = fixture_dir.join("symbols.db");
        fs::copy(&source_db, &fixture_db)?;

        println!("✅ Database copied to fixture location");

        // Build metadata
        let metadata = Self::build_metadata(&handler).await?;
        let metadata_path = fixture_dir.join("metadata.json");
        fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;

        println!("✅ Metadata extracted:");
        println!("   Files indexed: {}", metadata.file_count);
        println!("   Symbols indexed: {}", metadata.symbol_count);
        println!(
            "   Database size: {} KB",
            fs::metadata(&fixture_db)?.len() / 1024
        );

        Ok(Self {
            fixture_db_path: fixture_db,
            metadata,
        })
    }

    /// Load existing fixture (fast - no indexing)
    pub fn load() -> Result<Self> {
        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/databases/julie-snapshot");
        let fixture_db = fixture_dir.join("symbols.db");
        let metadata_path = fixture_dir.join("metadata.json");

        if !fixture_db.exists() {
            bail!(
                "Fixture database not found at: {}\nRun: cargo test --lib build_julie_fixture -- --ignored --nocapture",
                fixture_db.display()
            );
        }

        let metadata: FixtureMetadata =
            serde_json::from_str(&fs::read_to_string(&metadata_path)?)?;

        Ok(Self {
            fixture_db_path: fixture_db,
            metadata,
        })
    }

    /// Get singleton instance (load once, reuse for all tests)
    pub fn get_instance() -> &'static Self {
        static INSTANCE: OnceLock<JulieTestFixture> = OnceLock::new();
        INSTANCE.get_or_init(|| Self::load().expect("Failed to load Julie test fixture"))
    }

    /// Create a test-scoped copy of the fixture for read-write tests
    pub fn copy_to_temp(&self) -> Result<TempDir> {
        let temp = TempDir::new()?;
        let dest_db = temp.path().join("symbols.db");
        fs::copy(&self.fixture_db_path, &dest_db)?;
        Ok(temp)
    }

    /// Get path to fixture database
    pub fn db_path(&self) -> &PathBuf {
        &self.fixture_db_path
    }

    /// Get known file paths for assertions
    pub fn known_files(&self) -> &[String] {
        &self.metadata.indexed_files
    }

    /// Get known symbols for a file
    pub fn known_symbols(&self, file_path: &str) -> Option<&Vec<String>> {
        self.metadata.known_symbols.get(file_path)
    }

    /// Wait for indexing to complete (helper)
    async fn wait_for_indexing(handler: &crate::handler::JulieServerHandler) -> Result<()> {
        use std::sync::atomic::Ordering;
        use tokio::time::{sleep, Duration};

        // Wait for SQLite FTS5 indexing to complete
        for _ in 0..60 {
            if handler
                .indexing_status
                .sqlite_fts_ready
                .load(Ordering::Relaxed)
            {
                println!("✅ SQLite FTS5 indexing complete");
                return Ok(());
            }
            sleep(Duration::from_millis(500)).await;
        }

        bail!("Indexing timeout after 30 seconds");
    }

    /// Build metadata from indexed workspace (helper)
    async fn build_metadata(
        handler: &crate::handler::JulieServerHandler,
    ) -> Result<FixtureMetadata> {
        // Access workspace to get database
        let workspace_guard = handler.workspace.read().await;
        let workspace = workspace_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Workspace not initialized"))?;

        let db_arc = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Database not initialized"))?;

        let db_lock = db_arc.lock().unwrap();

        // Query database for metadata
        // Count files
        let file_count: i64 = db_lock
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;

        // Count symbols
        let symbol_count: i64 = db_lock
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;

        // Get all file paths
        let mut stmt = db_lock
            .conn
            .prepare("SELECT path FROM files ORDER BY path")?;
        let indexed_files: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        // Get symbols per file (for test assertions)
        let mut known_symbols: HashMap<String, Vec<String>> = HashMap::new();
        for file_path in &indexed_files {
            let mut stmt = db_lock.conn.prepare(
                "SELECT name FROM symbols WHERE file_path = ? ORDER BY name",
            )?;
            let symbols: Vec<String> = stmt
                .query_map([file_path], |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            known_symbols.insert(file_path.clone(), symbols);
        }

        Ok(FixtureMetadata {
            created_at: chrono::Utc::now().to_rfc3339(),
            file_count: file_count as usize,
            symbol_count: symbol_count as usize,
            indexed_files,
            known_symbols,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore] // Run manually: cargo test --lib build_julie_fixture -- --ignored --nocapture
    async fn build_julie_fixture() -> Result<()> {
        println!("🔨 Building Julie test fixture database...");
        println!("This is a ONE-TIME operation - fixture will be checked into git");

        let fixture = JulieTestFixture::build().await?;

        println!("\n✅ Fixture built successfully:");
        println!("  📁 Files indexed: {}", fixture.metadata.file_count);
        println!("  🔤 Symbols indexed: {}", fixture.metadata.symbol_count);
        println!("  💾 Database location: {}", fixture.db_path().display());
        println!(
            "  📏 Database size: {} KB",
            fs::metadata(fixture.db_path())?.len() / 1024
        );

        println!("\n📝 Next steps:");
        println!("  1. Verify fixture works: cargo test test_fixture_loads --lib");
        println!("  2. Commit to git: git add fixtures/databases/julie-snapshot/");
        println!("  3. Convert dogfooding tests to use fixture (Phase 3)");

        Ok(())
    }

    #[test]
    fn test_fixture_loads() -> Result<()> {
        let start = std::time::Instant::now();
        let fixture = JulieTestFixture::load()?;
        let elapsed = start.elapsed();

        assert!(fixture.metadata.file_count > 0, "Fixture should have files");
        assert!(
            fixture.metadata.symbol_count > 0,
            "Fixture should have symbols"
        );
        assert!(fixture.db_path().exists(), "Database file should exist");

        // Verify we can load known data
        assert!(
            fixture.known_files().contains(&"src/main.rs".to_string()),
            "Fixture should contain src/main.rs"
        );

        println!("✅ Fixture loads successfully:");
        println!("   Files: {}", fixture.metadata.file_count);
        println!("   Symbols: {}", fixture.metadata.symbol_count);
        println!("   Load time: {:?}", elapsed);

        // Assert load time is fast (<100ms)
        assert!(
            elapsed.as_millis() < 100,
            "Fixture should load in <100ms, took {:?}",
            elapsed
        );

        Ok(())
    }

    #[test]
    fn test_fixture_singleton() {
        let instance1 = JulieTestFixture::get_instance();
        let instance2 = JulieTestFixture::get_instance();

        // Should be same instance (same pointer)
        assert!(std::ptr::eq(instance1, instance2), "Should be singleton");

        println!("✅ Singleton pattern works correctly");
    }

    #[test]
    fn test_fixture_copy_to_temp() -> Result<()> {
        let fixture = JulieTestFixture::load()?;
        let temp = fixture.copy_to_temp()?;

        let copied_db = temp.path().join("symbols.db");
        assert!(copied_db.exists(), "Copied database should exist");

        // Verify size matches
        let original_size = fs::metadata(fixture.db_path())?.len();
        let copied_size = fs::metadata(&copied_db)?.len();
        assert_eq!(
            original_size, copied_size,
            "Copied database should match original size"
        );

        println!("✅ Temp copy works correctly");
        println!("   Temp location: {}", temp.path().display());
        println!("   Size: {} KB", copied_size / 1024);

        Ok(())
    }
}
