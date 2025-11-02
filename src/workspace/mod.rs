// src/workspace/mod.rs
//! Julie Workspace Management
//!
//! This module manages the .julie workspace folder structure and initialization.
//! The workspace provides project-local storage for all Julie data including:
//! - SQLite database (source of truth with FTS5 search)
//! - FastEmbed vectors for semantic search
//! - Configuration and caching
//! - Workspace registry for multi-project indexing

pub mod registry;
pub mod registry_service;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock; // Async RwLock for embeddings and vector_store
use tracing::{debug, info, warn};
// Import IncrementalIndexer from watcher module
use crate::watcher::IncrementalIndexer;

// Forward declarations for types we'll implement later
pub type SqliteDB = crate::database::SymbolDatabase;
pub type EmbeddingStore = crate::embeddings::EmbeddingEngine;
pub type VectorIndex = crate::embeddings::vector_store::VectorStore;

/// The main Julie workspace structure
///
/// Manages all project-local data storage and provides a unified interface
/// to the two-tier architecture (SQLite FTS5 + Semantic/HNSW)
pub struct JulieWorkspace {
    /// Project root directory where MCP was started
    pub root: PathBuf,

    /// The .julie directory for all workspace data
    pub julie_dir: PathBuf,

    /// Database connection (source of truth with FTS5 search)
    /// 🚨 DEADLOCK FIX: Using std::sync::Mutex (not tokio::sync::Mutex)
    /// Database is accessed from spawn_blocking, so sync Mutex is correct
    pub db: Option<Arc<std::sync::Mutex<SqliteDB>>>,

    /// Embedding store (semantic bridge)
    /// Using tokio::sync::RwLock for async-safe access (GPU operations are async-friendly)
    /// Wrapped in Option for lazy initialization
    pub embeddings: Option<Arc<RwLock<Option<EmbeddingStore>>>>,

    /// Vector store with HNSW index (fast similarity search)
    pub vector_store: Option<Arc<RwLock<VectorIndex>>>,

    /// File watcher for incremental updates
    pub watcher: Option<IncrementalIndexer>,

    /// Workspace configuration
    pub config: WorkspaceConfig,
}

/// Configuration for a Julie workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Version of the workspace format
    pub version: String,

    /// Languages to index (empty = all supported)
    pub languages: Vec<String>,

    /// Patterns to ignore during indexing
    pub ignore_patterns: Vec<String>,

    /// Maximum file size to process (in bytes)
    pub max_file_size: usize,

    /// Embedding model to use
    pub embedding_model: String,

    /// Enable incremental updates
    pub incremental_updates: bool,
}

impl Clone for JulieWorkspace {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            julie_dir: self.julie_dir.clone(),
            db: self.db.clone(),
            embeddings: self.embeddings.clone(),
            vector_store: self.vector_store.clone(),
            watcher: None, // Don't clone file watcher - create new if needed
            config: self.config.clone(),
        }
    }
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            version: "0.1.0".to_string(),
            languages: vec![], // Empty = all supported languages
            ignore_patterns: vec![
                "**/node_modules/**".to_string(),
                "**/target/**".to_string(),
                "**/build/**".to_string(),
                "**/dist/**".to_string(),
                "**/.git/**".to_string(),
                "**/*.min.js".to_string(),
                "**/*.bundle.js".to_string(),
                "**/.julie/**".to_string(), // Don't index our own data
            ],
            max_file_size: 1024 * 1024, // 1MB default
            embedding_model: "bge-small".to_string(),
            incremental_updates: true,
        }
    }
}

impl JulieWorkspace {
    /// Initialize a new Julie workspace at the given root directory
    ///
    /// This creates the .julie folder structure and sets up initial configuration
    /// 🔥 CRITICAL FIX: Now async to handle ONNX model loading without blocking runtime
    pub async fn initialize(root: PathBuf) -> Result<Self> {
        info!("Initializing Julie workspace at: {}", root.display());
        debug!(
            "🔍 DEBUG: JulieWorkspace::initialize called with root: {}",
            root.display()
        );

        let julie_dir = root.join(".julie");
        debug!("🔍 DEBUG: Julie directory will be: {}", julie_dir.display());

        // Create the workspace folder structure
        Self::create_folder_structure(&julie_dir)?;

        // Create default configuration
        let config = WorkspaceConfig::default();
        Self::save_config(&julie_dir, &config)?;

        let mut workspace = Self {
            root,
            julie_dir,
            db: None,
            embeddings: None,
            vector_store: None,
            watcher: None,
            config,
        };

        // Initialize persistent components (now async due to ONNX fix)
        workspace.initialize_all_components().await?;

        info!("Julie workspace initialized successfully");
        Ok(workspace)
    }

    /// Detect and load an existing Julie workspace
    ///
    /// Searches up the directory tree from the given path to find a .julie folder
    /// 🔥 CRITICAL FIX: Now async to handle ONNX model loading without blocking runtime
    pub async fn detect_and_load(start_path: PathBuf) -> Result<Option<Self>> {
        debug!(
            "🔍 DEBUG: detect_and_load called with start_path: {}",
            start_path.display()
        );
        let julie_dir = Self::find_workspace_root(&start_path)?;

        match julie_dir {
            Some(julie_path) => {
                debug!(
                    "🔍 DEBUG: find_workspace_root returned: {}",
                    julie_path.display()
                );
                let root = julie_path
                    .parent()
                    .ok_or_else(|| anyhow!("Invalid workspace structure"))?
                    .to_path_buf();

                info!("Found existing Julie workspace at: {}", root.display());
                debug!("🔍 DEBUG: Workspace root will be: {}", root.display());

                // Load configuration
                let config = Self::load_config(&julie_path)?;

                let mut workspace = Self {
                    root,
                    julie_dir: julie_path,
                    db: None,
                    embeddings: None,
                    vector_store: None,
                    watcher: None,
                    config,
                };

                // Validate workspace structure
                workspace.validate_structure()?;

                // Initialize persistent components (now async due to ONNX fix)
                workspace.initialize_all_components().await?;

                Ok(Some(workspace))
            }
            None => {
                debug!("No existing Julie workspace found");
                Ok(None)
            }
        }
    }

    /// Create the complete .julie folder hierarchy
    ///
    /// Creates all necessary subdirectories for the per-workspace architecture
    fn create_folder_structure(julie_dir: &Path) -> Result<()> {
        debug!(
            "Creating .julie folder structure at: {}",
            julie_dir.display()
        );

        // NOTE: Per-workspace directories (db/, tantivy/, vectors/) are created on-demand
        // when each workspace is indexed. Here we only create shared infrastructure.
        let folders = [
            julie_dir.join("indexes"), // Per-workspace root (workspaces created on demand)
            julie_dir.join("models"),  // Cached FastEmbed models (shared)
            julie_dir.join("cache"),   // File hashes and parse cache (shared)
            julie_dir.join("cache").join("embeddings"),
            julie_dir.join("cache").join("parse_cache"),
            julie_dir.join("logs"),   // Julie logs
            julie_dir.join("config"), // Configuration files
        ];

        for folder in &folders {
            fs::create_dir_all(folder)
                .map_err(|e| anyhow!("Failed to create directory {}: {}", folder.display(), e))?;
            debug!("Created directory: {}", folder.display());
        }

        // Create .gitignore to prevent accidental commits of Julie's data
        let gitignore_path = julie_dir.join(".gitignore");
        if !gitignore_path.exists() {
            fs::write(
                &gitignore_path,
                "# Julie code intelligence data - do not commit to version control\n\
                *\n\
                !.gitignore\n",
            )?;
            debug!("Created .gitignore in .julie directory");
        }

        info!("Created per-workspace .julie folder structure");
        Ok(())
    }

    /// Save workspace configuration to julie.toml
    fn save_config(julie_dir: &Path, config: &WorkspaceConfig) -> Result<()> {
        let config_path = julie_dir.join("config").join("julie.toml");
        let toml_content = toml::to_string_pretty(config)
            .map_err(|e| anyhow!("Failed to serialize config: {}", e))?;

        fs::write(&config_path, toml_content)
            .map_err(|e| anyhow!("Failed to write config file: {}", e))?;

        debug!("Saved configuration to: {}", config_path.display());
        Ok(())
    }

    /// Load workspace configuration from julie.toml
    fn load_config(julie_dir: &Path) -> Result<WorkspaceConfig> {
        let config_path = julie_dir.join("config").join("julie.toml");

        if !config_path.exists() {
            warn!("Configuration file not found, using defaults");
            return Ok(WorkspaceConfig::default());
        }

        let config_content = fs::read_to_string(&config_path)
            .map_err(|e| anyhow!("Failed to read config file: {}", e))?;

        let config: WorkspaceConfig = toml::from_str(&config_content)
            .map_err(|e| anyhow!("Failed to parse config file: {}", e))?;

        debug!("Loaded configuration from: {}", config_path.display());
        Ok(config)
    }

    /// Find workspace root by searching up the directory tree
    fn find_workspace_root(start_path: &Path) -> Result<Option<PathBuf>> {
        let mut current = start_path.to_path_buf();

        loop {
            let julie_dir = current.join(".julie");
            if julie_dir.exists() && julie_dir.is_dir() {
                debug!("Found .julie directory at: {}", julie_dir.display());
                return Ok(Some(julie_dir));
            }

            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => break,
            }
        }

        Ok(None)
    }

    /// Validate that workspace structure is intact
    pub fn validate_structure(&self) -> Result<()> {
        debug!("Validating per-workspace structure");

        let required_dirs = [
            "indexes", // Per-workspace root (individual workspaces created on demand)
            "models", "cache", "logs", "config",
        ];

        for dir in &required_dirs {
            let path = self.julie_dir.join(dir);
            if !path.exists() {
                info!("Creating missing directory: {}", path.display());
                std::fs::create_dir_all(&path)
                    .context(format!("Failed to create directory: {}", path.display()))?;
            }
        }

        // Check if config file exists
        let config_path = self.julie_dir.join("config").join("julie.toml");
        if !config_path.exists() {
            info!("Configuration file missing, creating with defaults");
            Self::save_config(&self.julie_dir, &self.config)?;
        }

        info!("Per-workspace structure validation passed");
        Ok(())
    }

    /// Perform health checks on the workspace
    pub fn health_check(&self) -> Result<WorkspaceHealth> {
        debug!("Performing workspace health check");

        let mut health = WorkspaceHealth::new();

        // Check folder structure
        match self.validate_structure() {
            Ok(_) => health.structure_valid = true,
            Err(e) => {
                health.structure_valid = false;
                health
                    .errors
                    .push(format!("Structure validation failed: {}", e));
            }
        }

        // Check disk space
        health.check_disk_space(&self.julie_dir)?;

        // Check permissions
        health.check_permissions(&self.julie_dir)?;

        // ✅ Comprehensive health checks implemented in ManageWorkspaceTool::health_command()
        // See src/tools/workspace/commands/registry.rs:
        // - check_database_health() - SQLite statistics and integrity
        // - check_search_engine_health() - FTS5 search status
        // - check_embedding_health() - HNSW semantic search status
        // This basic check only validates directory permissions.

        if health.errors.is_empty() {
            info!("Workspace health check passed");
        } else {
            warn!(
                "Workspace health check found {} issues",
                health.errors.len()
            );
        }

        Ok(health)
    }

    /// Get the path to the SQLite database file
    pub fn db_path(&self) -> PathBuf {
        self.julie_dir.join("db").join("symbols.db")
    }

    /// Get the root indexes directory (contains all workspace indexes)
    pub fn indexes_root_path(&self) -> PathBuf {
        self.julie_dir.join("indexes")
    }

    /// Get the path to a specific workspace's index directory (SQLite database)
    pub fn workspace_index_path(&self, workspace_id: &str) -> PathBuf {
        self.indexes_root_path().join(workspace_id).join("db")
    }

    /// Get the path to a specific workspace's vector store
    pub fn workspace_vectors_path(&self, workspace_id: &str) -> PathBuf {
        self.indexes_root_path().join(workspace_id).join("vectors")
    }

    /// Get the path to a specific workspace's SQLite database
    pub fn workspace_db_path(&self, workspace_id: &str) -> PathBuf {
        self.indexes_root_path()
            .join(workspace_id)
            .join("db")
            .join("symbols.db")
    }

    /// Get the path to the models cache (shared across all workspaces)
    pub fn models_path(&self) -> PathBuf {
        self.julie_dir.join("models")
    }

    /// Get the path to the general cache
    pub fn cache_path(&self) -> PathBuf {
        self.julie_dir.join("cache")
    }

    /// Get the embedding cache directory for ONNX model storage
    ///
    /// This directory stores downloaded ONNX embedding models and is persistent
    /// across server restarts. Located at `.julie/cache/embeddings/`
    pub fn get_embedding_cache_dir(&self) -> PathBuf {
        self.julie_dir.join("cache").join("embeddings")
    }

    /// Ensure embedding cache directory exists
    ///
    /// Creates the `.julie/cache/embeddings/` directory if it doesn't exist.
    /// This must be called before initializing the embedding engine.
    ///
    /// # Returns
    /// The path to the embedding cache directory
    ///
    /// # Example
    /// ```no_run
    /// let workspace = JulieWorkspace::initialize(root).await?;
    /// let cache_dir = workspace.ensure_embedding_cache_dir()?;
    /// let engine = EmbeddingEngine::new("bge-small", cache_dir, db).await?;
    /// ```
    pub fn ensure_embedding_cache_dir(&self) -> Result<PathBuf> {
        let cache_dir = self.get_embedding_cache_dir();
        std::fs::create_dir_all(&cache_dir)
            .context(format!(
                "Failed to create embedding cache directory: {}",
                cache_dir.display()
            ))?;
        debug!(
            "📁 Embedding cache directory ready: {}",
            cache_dir.display()
        );
        Ok(cache_dir)
    }

    /// Get all cache directories (for bulk operations like cleanup)
    ///
    /// Returns a list of all cache subdirectories managed by the workspace.
    /// Useful for cleanup operations, size monitoring, or validation.
    pub fn get_all_cache_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.get_embedding_cache_dir(),
            self.julie_dir.join("cache").join("parse_cache"),
        ]
    }

    /// Clear embedding cache (idempotent)
    ///
    /// Removes all embedding cache files and recreates the directory.
    /// This is useful for:
    /// - Recovery from corrupted cache files
    /// - Force re-downloading of embedding models
    /// - Freeing disk space (~200MB per model)
    ///
    /// This operation is idempotent - calling it multiple times is safe.
    ///
    /// # Example
    /// ```no_run
    /// workspace.clear_embedding_cache()?;
    /// // Cache is now empty but directory structure is ready for new models
    /// ```
    pub fn clear_embedding_cache(&self) -> Result<()> {
        let cache_dir = self.get_embedding_cache_dir();
        if cache_dir.exists() {
            std::fs::remove_dir_all(&cache_dir).context(format!(
                "Failed to remove embedding cache directory: {}",
                cache_dir.display()
            ))?;
            info!("🧹 Cleared embedding cache: {}", cache_dir.display());
        }

        // Recreate directory structure for next use
        std::fs::create_dir_all(&cache_dir).context(format!(
            "Failed to recreate embedding cache directory: {}",
            cache_dir.display()
        ))?;
        debug!("📁 Recreated embedding cache directory: {}", cache_dir.display());

        Ok(())
    }

    /// Initialize persistent database connection
    pub fn initialize_database(&mut self) -> Result<()> {
        if self.db.is_some() {
            return Ok(()); // Already initialized
        }

        // Compute workspace ID for per-workspace database
        let workspace_id = registry::generate_workspace_id(
            self.root
                .to_str()
                .ok_or_else(|| anyhow!("Invalid workspace path"))?,
        )?;

        let db_path = self.workspace_db_path(&workspace_id);
        info!(
            "Initializing SQLite database for workspace {} at: {}",
            workspace_id,
            db_path.display()
        );

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).context(format!(
                "Failed to create database directory: {}",
                parent.display()
            ))?;
        }

        let database = SqliteDB::new(&db_path)?;
        self.db = Some(Arc::new(std::sync::Mutex::new(database)));

        info!("Database initialized successfully");
        Ok(())
    }

    /// Initialize embedding engine
    /// 🔥 CRITICAL FIX: This function is now async because ONNX model loading is blocking
    /// We must use spawn_blocking to avoid blocking the tokio runtime
    pub async fn initialize_embeddings(&mut self) -> Result<()> {
        if self.embeddings.is_some() {
            return Ok(()); // Already initialized
        }

        // 🚀 PERFORMANCE: Skip embeddings if env override set (for tests/development)
        if std::env::var("JULIE_SKIP_EMBEDDINGS").is_ok() {
            info!("Skipping embedding engine initialization (env override)");
            return Ok(());
        }

        let db = self
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Database not initialized"))?
            .clone();

        let models_path = self.models_path();
        info!(
            "Initializing embedding engine with cache at: {}",
            models_path.display()
        );

        // ✅ EmbeddingEngine::new() is now async (downloads model from HuggingFace)
        // No need for spawn_blocking - async download is non-blocking
        let embedding_engine =
            crate::embeddings::EmbeddingEngine::new("bge-small", models_path, db)
                .await
                .context("Embedding engine initialization failed")?;

        self.embeddings = Some(Arc::new(RwLock::new(Some(embedding_engine))));

        info!("Embedding engine initialized successfully");
        Ok(())
    }

    /// Initialize HNSW vector store for fast semantic search
    /// Loads existing embeddings from database and builds HNSW index for immediate use
    pub fn initialize_vector_store(&mut self) -> Result<()> {
        if self.vector_store.is_some() {
            return Ok(()); // Already initialized
        }

        info!("🧠 Initializing HNSW vector store");

        // Create empty vector store (384 dimensions for BGE-Small model)
        let mut store = VectorIndex::new(384)?;

        // 🚀 CRITICAL FIX: Load embeddings and release DB lock BEFORE expensive HNSW build
        // The old code held database lock for 30-60s during HNSW build, blocking all tools
        let (embeddings_result, workspace_id) = if let Some(db) = &self.db {
            // Extract data with minimal lock hold time
            let result = {
                // 🚨 DEADLOCK FIX: Direct lock (no try_lock needed with std::sync::Mutex)
                let db_lock = db.lock().map_err(|_| {
                    anyhow::anyhow!("Could not acquire database lock during initialization")
                })?;
                let model_name = "bge-small"; // Match the embedding model from config
                db_lock.load_all_embeddings(model_name)
            }; // 🔓 Database lock released here!

            // Compute workspace ID (doesn't need database)
            let workspace_id = registry::generate_workspace_id(
                self.root
                    .to_str()
                    .ok_or_else(|| anyhow!("Invalid workspace path"))?,
            )?;

            (result, workspace_id)
        } else {
            warn!("Database not initialized. Vector store will start empty.");
            (Ok(std::collections::HashMap::new()), "default".to_string())
        };

        // Now process embeddings and build HNSW WITHOUT holding any locks
        match embeddings_result {
            Ok(embeddings) => {
                let count = embeddings.len();
                if count > 0 {
                    info!("📥 Loaded {} embeddings from database for HNSW", count);

                    // Now try to load HNSW index from disk (fast path)
                    let vectors_dir = self.workspace_vectors_path(&workspace_id);
                    let mut loaded_from_disk = false;

                    if vectors_dir.exists() {
                        info!("📂 Attempting to load HNSW index from disk...");
                        match store.load_hnsw_index(&vectors_dir) {
                            Ok(_) => {
                                info!("✅ HNSW index loaded from disk - semantic search ready!");
                                loaded_from_disk = true;
                            }
                            Err(e) => {
                                info!("⚠️  Failed to load HNSW from disk: {}. Rebuilding...", e);
                            }
                        }
                    }

                    // If disk load failed, build HNSW from embeddings (slower path)
                    // 🚀 CRITICAL: This 30-60s operation now runs WITHOUT holding database lock!
                    // 🔧 REFACTOR: Pass embeddings directly to build_hnsw_index (no HashMap storage)
                    if !loaded_from_disk {
                        info!("🏗️  Building HNSW index from {} embeddings...", count);
                        match store.build_hnsw_index(&embeddings) {
                            Ok(_) => {
                                info!("✅ HNSW index built successfully - semantic search ready!");

                                // Save HNSW index to disk for faster startup next time
                                match store.save_hnsw_index(&vectors_dir) {
                                    Ok(_) => {
                                        info!("💾 HNSW index persisted to disk successfully");
                                    }
                                    Err(e) => {
                                        warn!("Failed to save HNSW index: {}. Will rebuild next time.", e);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to build HNSW index: {}. Falling back to brute force search.", e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Could not load embeddings from database: {}. Starting with empty store.",
                    e
                );
            }
        }

        self.vector_store = Some(Arc::new(RwLock::new(store)));
        info!("✅ Vector store initialized and ready for semantic search");
        Ok(())
    }

    /// Initialize file watcher for incremental updates
    pub fn initialize_file_watcher(&mut self) -> Result<()> {
        if self.watcher.is_some() {
            return Ok(()); // Already initialized
        }

        if std::env::var("JULIE_SKIP_EMBEDDINGS").is_ok() {
            info!("Skipping file watcher initialization due to JULIE_SKIP_EMBEDDINGS");
            return Ok(());
        }

        // Ensure all required components are initialized
        if self.db.is_none() || self.embeddings.is_none() {
            return Err(anyhow::anyhow!(
                "Required components not initialized before file watcher"
            ));
        }

        // 🔧 FIX: Initialize VectorStore for incremental updates BEFORE file watcher starts
        // This loads HNSW from disk (if exists) so file watcher can update it incrementally
        // For primary workspace, keeping ~11MB in memory is acceptable for incremental updates
        if self.vector_store.is_none() {
            info!("🧠 Lazy-loading VectorStore for incremental updates");
            if let Err(e) = self.initialize_vector_store() {
                warn!("Failed to initialize VectorStore for file watcher: {}. Incremental semantic updates disabled.", e);
                // Continue anyway - file watcher will work for SQLite updates
            }
        }

        info!("Initializing file watcher for: {}", self.root.display());

        // Create placeholder extractor manager for now
        let extractor_manager = Arc::new(crate::extractors::ExtractorManager::new());

        let file_watcher = IncrementalIndexer::new(
            self.root.clone(),
            self.db.as_ref().unwrap().clone(),
            self.embeddings.as_ref().unwrap().clone(),
            extractor_manager,
            self.vector_store.clone(), // Pass vector_store for incremental HNSW updates
        )?;

        self.watcher = Some(file_watcher);

        info!("File watcher initialized successfully");
        Ok(())
    }

    /// Initialize all persistent components
    /// 🔥 CRITICAL FIX: Now async because initialize_embeddings() is async (ONNX blocking fix)
    pub async fn initialize_all_components(&mut self) -> Result<()> {
        self.initialize_database()?;
        self.initialize_embeddings().await?; // 🚨 Now async to avoid runtime deadlock
                                             // REMOVED: Vector store initialization moved to end of background embedding generation
                                             // HNSW index will be built AFTER embeddings are generated, not at startup
                                             // This allows MCP server to start immediately without blocking

        // Initialize file watcher last (requires other components)
        if self.config.incremental_updates {
            self.initialize_file_watcher()?;
        }

        info!("All workspace components initialized successfully");
        Ok(())
    }

    /// Start file watching if initialized
    pub async fn start_file_watching(&mut self) -> Result<()> {
        if let Some(ref mut watcher) = self.watcher {
            watcher.start_watching().await?;
            info!("File watching started");
        }
        Ok(())
    }
}

/// Health status of a Julie workspace
#[derive(Debug)]
pub struct WorkspaceHealth {
    pub structure_valid: bool,
    pub disk_space_mb: u64,
    pub has_write_permissions: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl WorkspaceHealth {
    fn new() -> Self {
        Self {
            structure_valid: false,
            disk_space_mb: 0,
            has_write_permissions: false,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn check_disk_space(&mut self, julie_dir: &Path) -> Result<()> {
        // Simple check - try to get available space
        // This is a basic implementation, could be enhanced with statvfs on Unix
        match fs::metadata(julie_dir) {
            Ok(_) => {
                self.disk_space_mb = 1000; // Placeholder - assume we have space
                if self.disk_space_mb < 100 {
                    self.warnings
                        .push("Low disk space (< 100MB available)".to_string());
                }
            }
            Err(e) => {
                self.errors
                    .push(format!("Could not check disk space: {}", e));
            }
        }
        Ok(())
    }

    fn check_permissions(&mut self, julie_dir: &Path) -> Result<()> {
        // Try to create a temporary file to test write permissions
        let test_file = julie_dir.join(".write_test");
        match fs::write(&test_file, "test") {
            Ok(_) => {
                self.has_write_permissions = true;
                let _ = fs::remove_file(&test_file); // Clean up
            }
            Err(e) => {
                self.has_write_permissions = false;
                self.errors.push(format!("No write permissions: {}", e));
            }
        }
        Ok(())
    }

    /// Check if workspace is healthy
    pub fn is_healthy(&self) -> bool {
        self.errors.is_empty() && self.structure_valid && self.has_write_permissions
    }
}

// Tests moved to `src/tests/workspace_mod_tests.rs`
