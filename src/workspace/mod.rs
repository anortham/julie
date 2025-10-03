// src/workspace/mod.rs
//! Julie Workspace Management
//!
//! This module manages the .julie workspace folder structure and initialization.
//! The workspace provides project-local storage for all Julie data including:
//! - SQLite database (source of truth)
//! - Tantivy search index
//! - FastEmbed vectors
//! - Configuration and caching
//! - Workspace registry for multi-project indexing

pub mod registry;
pub mod registry_service;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};
// Import IncrementalIndexer from watcher module
use crate::watcher::IncrementalIndexer;

// Forward declarations for types we'll implement later
pub type SqliteDB = crate::database::SymbolDatabase;
pub type TantivyIndex = crate::search::SearchEngine;
pub type TantivyWriter = crate::search::SearchIndexWriter;
pub type EmbeddingStore = crate::embeddings::EmbeddingEngine;
pub type VectorIndex = crate::embeddings::vector_store::VectorStore;

/// The main Julie workspace structure
///
/// Manages all project-local data storage and provides a unified interface
/// to the four-pillar architecture (SQLite + Tantivy + FastEmbed + HNSW)
pub struct JulieWorkspace {
    /// Project root directory where MCP was started
    pub root: PathBuf,

    /// The .julie directory for all workspace data
    pub julie_dir: PathBuf,

    /// Database connection (source of truth)
    pub db: Option<Arc<Mutex<SqliteDB>>>,

    /// Search index (query accelerator) - READ ONLY
    pub search: Option<Arc<RwLock<TantivyIndex>>>,

    /// Search index writer (write operations) - SEPARATED to eliminate RwLock contention
    pub search_writer: Option<Arc<Mutex<TantivyWriter>>>,

    /// Embedding store (semantic bridge)
    pub embeddings: Option<Arc<Mutex<EmbeddingStore>>>,

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
            search: self.search.clone(),
            search_writer: self.search_writer.clone(),
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
    pub fn initialize(root: PathBuf) -> Result<Self> {
        info!("Initializing Julie workspace at: {}", root.display());

        let julie_dir = root.join(".julie");

        // Create the workspace folder structure
        Self::create_folder_structure(&julie_dir)?;

        // Create default configuration
        let config = WorkspaceConfig::default();
        Self::save_config(&julie_dir, &config)?;

        let mut workspace = Self {
            root,
            julie_dir,
            db: None,
            search: None,
            search_writer: None,
            embeddings: None,
            vector_store: None,
            watcher: None,
            config,
        };

        // Initialize persistent components
        workspace.initialize_all_components()?;

        info!("Julie workspace initialized successfully");
        Ok(workspace)
    }

    /// Detect and load an existing Julie workspace
    ///
    /// Searches up the directory tree from the given path to find a .julie folder
    pub fn detect_and_load(start_path: PathBuf) -> Result<Option<Self>> {
        let julie_dir = Self::find_workspace_root(&start_path)?;

        match julie_dir {
            Some(julie_path) => {
                let root = julie_path
                    .parent()
                    .ok_or_else(|| anyhow!("Invalid workspace structure"))?
                    .to_path_buf();

                info!("Found existing Julie workspace at: {}", root.display());

                // Load configuration
                let config = Self::load_config(&julie_path)?;

                let mut workspace = Self {
                    root,
                    julie_dir: julie_path,
                    db: None,
                    search: None,
                    search_writer: None,
                    embeddings: None,
                    vector_store: None,
                    watcher: None,
                    config,
                };

                // Validate workspace structure
                workspace.validate_structure()?;

                // Initialize persistent components
                workspace.initialize_all_components()?;

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
            julie_dir.join("indexes"),               // Per-workspace root (workspaces created on demand)
            julie_dir.join("models"),                // Cached FastEmbed models (shared)
            julie_dir.join("cache"),                 // File hashes and parse cache (shared)
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
                !.gitignore\n"
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
            "indexes",  // Per-workspace root (individual workspaces created on demand)
            "models",
            "cache",
            "logs",
            "config",
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

        // TODO: Check database integrity when db module is ready
        // TODO: Check search index when Tantivy module is ready
        // TODO: Check embedding store when embeddings module is ready

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

    /// Get the path to a specific workspace's index directory
    pub fn workspace_index_path(&self, workspace_id: &str) -> PathBuf {
        self.indexes_root_path()
            .join(workspace_id)
            .join("tantivy")
    }

    /// Get the path to a specific workspace's vector store
    pub fn workspace_vectors_path(&self, workspace_id: &str) -> PathBuf {
        self.indexes_root_path()
            .join(workspace_id)
            .join("vectors")
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

    /// Initialize persistent database connection
    pub fn initialize_database(&mut self) -> Result<()> {
        if self.db.is_some() {
            return Ok(()); // Already initialized
        }

        // Compute workspace ID for per-workspace database
        let workspace_id = registry::generate_workspace_id(
            self.root.to_str()
                .ok_or_else(|| anyhow!("Invalid workspace path"))?
        )?;

        let db_path = self.workspace_db_path(&workspace_id);
        info!(
            "Initializing SQLite database for workspace {} at: {}",
            workspace_id,
            db_path.display()
        );

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)
                .context(format!("Failed to create database directory: {}", parent.display()))?;
        }

        let database = SqliteDB::new(&db_path)?;
        self.db = Some(Arc::new(Mutex::new(database)));

        info!("Database initialized successfully");
        Ok(())
    }

    /// Initialize persistent search index
    pub fn initialize_search_index(&mut self) -> Result<()> {
        if self.search.is_some() {
            return Ok(()); // Already initialized
        }

        if matches!(std::env::var("JULIE_SKIP_SEARCH_INDEX"), Ok(ref v) if v == "1") {
            info!("Skipping Tantivy search index initialization (env override)");
            return Ok(());
        }

        // Compute workspace ID from root path
        let workspace_id = registry::generate_workspace_id(
            self.root.to_str()
                .ok_or_else(|| anyhow!("Invalid workspace path"))?
        )?;

        let index_path = self.workspace_index_path(&workspace_id);
        info!(
            "Initializing Tantivy search index for workspace {} at: {}",
            workspace_id,
            index_path.display()
        );

        // Ensure the Tantivy directory itself exists (not just parent)
        fs::create_dir_all(&index_path)
            .context(format!("Failed to create Tantivy index directory: {}", index_path.display()))?;

        let search_engine = TantivyIndex::new(&index_path)?;

        // Create search writer from the same index
        // CRITICAL: Writer and reader must share the same Index instance
        let search_writer = TantivyWriter::new(
            search_engine.index(),
            search_engine.schema().clone(),
        )?;

        self.search = Some(Arc::new(RwLock::new(search_engine)));
        self.search_writer = Some(Arc::new(Mutex::new(search_writer)));

        info!("Search index and writer initialized successfully");
        Ok(())
    }

    /// Initialize embedding engine
    pub fn initialize_embeddings(&mut self) -> Result<()> {
        if self.embeddings.is_some() {
            return Ok(()); // Already initialized
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

        let embedding_engine = EmbeddingStore::new("bge-small", models_path, db)?;
        self.embeddings = Some(Arc::new(Mutex::new(embedding_engine)));

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

        // ALWAYS load embeddings from database first (needed for vector lookups and id_mapping)
        if let Some(db) = &self.db {
            // Use blocking lock since this is a synchronous initialization function
            match db.try_lock() {
                Ok(db_lock) => {
                    let model_name = "bge-small"; // Match the embedding model from config

                    match db_lock.load_all_embeddings(model_name) {
                        Ok(embeddings) => {
                            let count = embeddings.len();
                            info!("📥 Loading {} embeddings from database into vector store", count);

                            // Store each embedding in the vector store
                            for (symbol_id, vector) in embeddings {
                                if let Err(e) = store.store_vector(symbol_id.clone(), vector) {
                                    warn!("Failed to store embedding for symbol {}: {}", symbol_id, e);
                                }
                            }

                            info!("✅ Loaded {} embeddings into vector store", count);

                            // Compute workspace ID for per-workspace vectors path
                            let workspace_id = registry::generate_workspace_id(
                                self.root.to_str()
                                    .ok_or_else(|| anyhow!("Invalid workspace path"))?
                            )?;

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
                            if !loaded_from_disk {
                                info!("🏗️  Building HNSW index from {} embeddings...", count);
                                match store.build_hnsw_index() {
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
                        Err(e) => {
                            warn!("Could not load embeddings from database: {}. Starting with empty store.", e);
                        }
                    }
                }
                Err(_) => {
                    warn!("Could not acquire database lock during initialization. Starting with empty store.");
                }
            }
        } else {
            warn!("Database not initialized. Vector store will start empty.");
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
        if self.db.is_none() || self.search.is_none() || self.embeddings.is_none() {
            return Err(anyhow::anyhow!(
                "Required components not initialized before file watcher"
            ));
        }

        info!("Initializing file watcher for: {}", self.root.display());

        // Now we can properly import and use IncrementalIndexer

        // Create placeholder extractor manager for now
        let extractor_manager = Arc::new(crate::extractors::ExtractorManager::new());

        let file_watcher = IncrementalIndexer::new(
            self.root.clone(),
            self.db.as_ref().unwrap().clone(),
            self.search.as_ref().unwrap().clone(),
            self.search_writer.as_ref().unwrap().clone(),
            self.embeddings.as_ref().unwrap().clone(),
            extractor_manager,
        )?;

        self.watcher = Some(file_watcher);

        info!("File watcher initialized successfully");
        Ok(())
    }

    /// Initialize all persistent components
    pub fn initialize_all_components(&mut self) -> Result<()> {
        self.initialize_database()?;
        self.initialize_search_index()?;
        self.initialize_embeddings()?;
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
