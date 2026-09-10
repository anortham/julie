//! Pre-indexed snapshot of Julie's codebase for testing.
//! Git-ignored and built on demand (about 40 s once per checkout); the
//! `search-quality` bucket runs `ensure_julie_fixture` before its tests.

use anyhow::{Result, bail};
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
    /// facts.sqlite schema version the snapshot was built with
    #[serde(default)]
    pub schema_version: i32,
    /// Engine version the snapshot was built with
    #[serde(default)]
    pub engine_version: String,
}

impl JulieTestFixture {
    /// Build the fixture database into the git-ignored snapshot directory.
    pub async fn build() -> Result<Self> {
        use crate::tests::helpers::workspace::create_isolated_storage_handler;
        use crate::tools::workspace::ManageWorkspaceTool;

        let fixture_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/databases/julie-snapshot");

        // Clean any existing fixture
        if fixture_dir.exists() {
            fs::remove_dir_all(&fixture_dir)?;
        }
        fs::create_dir_all(&fixture_dir)?;

        println!("🔨 Building Julie test fixture...");

        // Index Julie into temp-backed storage so fixture generation does not
        // repopulate the repo's own .julie/indexes tree.
        let julie_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let handler = create_isolated_storage_handler(julie_root.clone()).await?;

        // Use ManageWorkspaceTool to index (this handles workspace initialization)
        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(env!("CARGO_MANIFEST_DIR").to_string()), // Explicit Julie root
            force: Some(true),                                  // Force rebuild
            name: None,
            workspace_id: None,
            detailed: None,
        };

        index_tool.call_tool(&handler).await?;

        println!("✅ Indexing complete, extracting database...");

        // For Julie, we want the primary workspace (not reference workspaces)
        // The primary workspace ID is generated from the Julie root path
        use crate::workspace::registry::generate_workspace_id;
        let expected_workspace_id = generate_workspace_id(&julie_root.to_string_lossy())?;
        let source_dir = handler
            .workspace_index_dir_for(&expected_workspace_id)
            .await?;

        if !source_dir.join("facts.sqlite").exists() {
            bail!("facts.sqlite not found at: {}", source_dir.display());
        }

        println!("⏳ Checkpointing WAL before copy...");
        {
            let conn = rusqlite::Connection::open(source_dir.join("facts.sqlite"))?;
            let _: (i64, i64, i64) =
                conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
            println!("✅ WAL checkpointed and truncated");
        }

        copy_index_dir(&source_dir, &fixture_dir)?;
        let fixture_db = fixture_dir.join("facts.sqlite");
        println!("✅ Index copied to fixture location");

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
        let fixture_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/databases/julie-snapshot");
        let fixture_db = fixture_dir.join("facts.sqlite");
        let metadata_path = fixture_dir.join("metadata.json");

        if !fixture_db.exists() {
            bail!(
                "Fixture facts.sqlite not found at: {}\nRun: cargo test --lib build_julie_fixture -- --ignored --nocapture",
                fixture_db.display()
            );
        }

        let metadata: FixtureMetadata = serde_json::from_str(&fs::read_to_string(&metadata_path)?)?;
        if metadata.schema_version != julie_facts::version::FACTS_SCHEMA_VERSION
            || metadata.engine_version
                != crate::tools::workspace::indexing::engine_version::SEMANTIC_INDEX_ENGINE_VERSION
        {
            bail!(
                "Fixture at {} was built for schema {} / engine {}; current is schema {}. Indexes are rebuilt, never migrated.\nRun: cargo test --lib ensure_julie_fixture -- --ignored --nocapture",
                fixture_dir.display(),
                metadata.schema_version,
                metadata.engine_version,
                julie_facts::version::FACTS_SCHEMA_VERSION
            );
        }

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
        copy_index_dir(
            self.fixture_db_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("fixture path has no parent"))?,
            temp.path(),
        )?;
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

    /// Build metadata from indexed workspace (helper)
    async fn build_metadata(
        handler: &crate::handler::JulieServerHandler,
    ) -> Result<FixtureMetadata> {
        // Access workspace to get database
        let workspace_guard = handler.workspace.read().await;
        let workspace = workspace_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Workspace not initialized"))?;
        let snapshot = workspace.store.current();
        let graph = snapshot.graph();
        let indexed_files = graph.paths().to_vec();
        let mut known_symbols: HashMap<String, Vec<String>> = HashMap::new();
        for path in &indexed_files {
            let mut names: Vec<String> = graph
                .symbols_in_path(path)
                .iter()
                .map(|id| graph.symbol(*id).name.clone())
                .collect();
            names.sort();
            known_symbols.insert(path.clone(), names);
        }

        Ok(FixtureMetadata {
            created_at: chrono::Utc::now().to_rfc3339(),
            file_count: indexed_files.len(),
            symbol_count: graph.len(),
            indexed_files,
            known_symbols,
            schema_version: julie_facts::version::FACTS_SCHEMA_VERSION,
            engine_version:
                crate::tools::workspace::indexing::engine_version::SEMANTIC_INDEX_ENGINE_VERSION
                    .to_string(),
        })
    }
}

fn copy_index_dir(src: &std::path::Path, dest: &std::path::Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_index_dir(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Run by the search-quality bucket: cargo test --lib ensure_julie_fixture -- --ignored --nocapture
    async fn ensure_julie_fixture() -> Result<()> {
        match JulieTestFixture::load() {
            Ok(fixture) => println!(
                "✅ Fixture current: {} files, {} symbols",
                fixture.metadata.file_count, fixture.metadata.symbol_count
            ),
            Err(reason) => {
                println!("🔨 Rebuilding fixture: {reason}");
                JulieTestFixture::build().await?;
            }
        }
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore] // Run manually: cargo test --lib build_julie_fixture -- --ignored --nocapture
    async fn build_julie_fixture() -> Result<()> {
        println!("🔨 Building Julie test fixture database...");
        println!("The snapshot is git-ignored; ensure_julie_fixture rebuilds it when stale");

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

        let copied_db = temp.path().join("facts.sqlite");
        assert!(copied_db.exists(), "Copied facts store should exist");

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
