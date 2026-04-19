// src/workspace/mod.rs
//! Julie Workspace Management
//!
//! This module manages the .julie workspace folder structure and initialization.
//! The workspace provides project-local storage for all Julie data including:
//! - SQLite database (source of truth for symbols and metadata)
//! - Tantivy full-text search index
//! - Configuration and caching
//! - Workspace registry for multi-project indexing

pub mod registry;
pub mod startup_hint;

use crate::health::{EmbeddingState, ProjectionState, WatcherState};
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};
// Import IncrementalIndexer from watcher module
use crate::watcher::IncrementalIndexer;

// Forward declarations for types we'll implement later
pub type SqliteDB = crate::database::SymbolDatabase;

/// The main Julie workspace structure
///
/// Manages all project-local data storage and provides a unified interface
/// to the search architecture (SQLite + Tantivy full-text search)
pub struct JulieWorkspace {
    /// Project root directory where MCP was started
    pub root: PathBuf,

    /// The .julie directory for all workspace data
    pub julie_dir: PathBuf,

    /// Database connection (source of truth)
    /// 🚨 DEADLOCK FIX: Using std::sync::Mutex (not tokio::sync::Mutex)
    /// Database is accessed from spawn_blocking, so sync Mutex is correct
    pub db: Option<Arc<std::sync::Mutex<SqliteDB>>>,

    /// Tantivy search index for full-text code search
    pub search_index: Option<Arc<std::sync::Mutex<crate::search::SearchIndex>>>,

    /// File watcher for incremental updates
    pub watcher: Option<IncrementalIndexer>,

    /// Embedding provider for semantic vector generation (None if unavailable)
    pub embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,

    /// Runtime status for embedding backend initialization.
    pub embedding_runtime_status: Option<crate::embeddings::EmbeddingRuntimeStatus>,

    /// Workspace configuration
    pub config: WorkspaceConfig,

    /// Override for the indexes root directory.
    /// When set, `indexes_root_path()` returns this instead of `{julie_dir}/indexes`.
    /// Used by the daemon's WorkspacePool to redirect indexes to a shared location.
    pub index_root_override: Option<PathBuf>,

    /// Shared runtime indexing state used by health reporting and the dashboard.
    pub(crate) indexing_runtime: crate::tools::workspace::indexing::state::SharedIndexingRuntime,
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

    /// Enable incremental updates
    pub incremental_updates: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddingRuntimeLogFields {
    pub requested_backend: String,
    pub resolved_backend: String,
    pub runtime: String,
    pub device: String,
    pub accelerated: bool,
    pub degraded_reason: String,
    pub telemetry_confidence: String,
    pub strict_mode: bool,
    pub fallback_used: bool,
}

fn embedding_telemetry_confidence(
    provider_info: Option<&crate::embeddings::DeviceInfo>,
) -> &'static str {
    let Some(info) = provider_info else {
        return "low";
    };

    let runtime = info.runtime.trim().to_ascii_lowercase();
    let device = info.device.trim().to_ascii_lowercase();
    if runtime.is_empty()
        || device.is_empty()
        || runtime.contains("unknown")
        || runtime.contains("unavailable")
        || device.contains("unknown")
        || device.contains("unavailable")
    {
        "low"
    } else {
        "high"
    }
}

pub(crate) fn build_embedding_runtime_log_fields(
    status: &crate::embeddings::EmbeddingRuntimeStatus,
    provider_info: Option<&crate::embeddings::DeviceInfo>,
    strict_mode: bool,
    fallback_used: bool,
) -> EmbeddingRuntimeLogFields {
    EmbeddingRuntimeLogFields {
        requested_backend: status.requested_backend.as_str().to_string(),
        resolved_backend: status.resolved_backend.as_str().to_string(),
        runtime: provider_info
            .map(|info| info.runtime.clone())
            .unwrap_or_else(|| "unavailable".to_string()),
        device: provider_info
            .map(|info| info.device.clone())
            .unwrap_or_else(|| "unavailable".to_string()),
        accelerated: status.accelerated,
        degraded_reason: status
            .degraded_reason
            .clone()
            .unwrap_or_else(|| "none".to_string()),
        telemetry_confidence: embedding_telemetry_confidence(provider_info).to_string(),
        strict_mode,
        fallback_used,
    }
}

impl Clone for JulieWorkspace {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            julie_dir: self.julie_dir.clone(),
            db: self.db.clone(),
            search_index: self.search_index.clone(),
            watcher: None, // Don't clone file watcher - create new if needed
            embedding_provider: self.embedding_provider.clone(),
            embedding_runtime_status: self.embedding_runtime_status.clone(),
            config: self.config.clone(),
            index_root_override: self.index_root_override.clone(),
            indexing_runtime: Arc::clone(&self.indexing_runtime),
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
                "**/.worktrees/**".to_string(),
                "**/*.min.js".to_string(),
                "**/*.bundle.js".to_string(),
                "**/.julie/**".to_string(), // Don't index our own data
            ],
            max_file_size: 1024 * 1024, // 1MB default
            incremental_updates: true,
        }
    }
}

impl JulieWorkspace {
    /// Initialize a new Julie workspace at the given root directory
    ///
    /// This creates the .julie folder structure and sets up initial configuration
    pub async fn initialize(root: PathBuf) -> Result<Self> {
        info!("Initializing Julie workspace at: {}", root.display());
        debug!(
            "JulieWorkspace::initialize called with root: {}",
            root.display()
        );

        let julie_dir = root.join(".julie");
        debug!("Julie directory will be: {}", julie_dir.display());

        // Create the workspace folder structure
        Self::create_folder_structure(&julie_dir)?;

        // Create default configuration
        let config = WorkspaceConfig::default();
        Self::save_config(&julie_dir, &config)?;

        // .julieignore creation now handled by discovery.rs during indexing
        // (auto-generates with smart vendor detection instead of generic template)

        let mut workspace = Self {
            root,
            julie_dir,
            db: None,
            search_index: None,
            watcher: None,
            embedding_provider: None,
            embedding_runtime_status: None,
            config,
            index_root_override: None,
            indexing_runtime:
                crate::tools::workspace::indexing::state::IndexingRuntimeState::shared(),
        };

        // Initialize persistent components
        workspace.initialize_all_components().await?;

        info!("Julie workspace initialized successfully");
        Ok(workspace)
    }

    /// Detect and load an existing Julie workspace
    ///
    /// Searches up the directory tree from the given path to find a .julie folder
    pub async fn detect_and_load(start_path: PathBuf) -> Result<Option<Self>> {
        debug!(
            "detect_and_load called with start_path: {}",
            start_path.display()
        );
        let julie_dir = Self::find_workspace_root(&start_path)?;

        match julie_dir {
            Some(julie_path) => {
                debug!("find_workspace_root returned: {}", julie_path.display());
                let root = julie_path
                    .parent()
                    .ok_or_else(|| anyhow!("Invalid workspace structure"))?
                    .to_path_buf();

                info!("Found existing Julie workspace at: {}", root.display());
                debug!("Workspace root will be: {}", root.display());

                // .julieignore creation now handled by discovery.rs during indexing
                // (auto-generates with smart vendor detection instead of generic template)

                // Load configuration
                let config = Self::load_config(&julie_path)?;

                let mut workspace = Self {
                    root,
                    julie_dir: julie_path,
                    db: None,
                    search_index: None,
                    watcher: None,
                    embedding_provider: None,
                    embedding_runtime_status: None,
                    config,
                    index_root_override: None,
                    indexing_runtime:
                        crate::tools::workspace::indexing::state::IndexingRuntimeState::shared(),
                };

                // Validate workspace structure
                workspace.validate_structure()?;

                // Initialize persistent components
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

        // NOTE: Per-workspace directories (db/, tantivy/) are created on-demand
        // when each workspace is indexed. Here we only create shared infrastructure.
        let folders = [
            julie_dir.join("indexes"), // Per-workspace root (workspaces created on demand)
            julie_dir.join("cache"),   // File hashes and parse cache (shared)
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

    /// Find workspace root by searching up the directory tree.
    ///
    /// Stops at project boundary markers (`.git` file or directory) to prevent
    /// walking past worktrees or sibling projects into unrelated `.julie/` dirs
    /// (e.g. a non-project `.julie/` directory from a parent path).
    fn find_workspace_root(start_path: &Path) -> Result<Option<PathBuf>> {
        let mut current = start_path.to_path_buf();

        // Resolve the global Julie config dir (~/.julie/) so we can skip it.
        // Without this guard, walking up from a temp dir or any path without
        // a .git boundary would find ~/.julie/ and treat the entire home
        // directory as a workspace — potentially walking OneDrive-synced
        // folders and triggering mass file downloads on Windows.
        let global_julie_home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
            .map(|h| PathBuf::from(h).join(".julie"));

        loop {
            let julie_dir = current.join(".julie");
            if julie_dir.exists() && julie_dir.is_dir() {
                // Skip the global ~/.julie/ config dir — it's not a workspace
                let is_global = global_julie_home.as_ref().map_or(false, |home| {
                    let julie_canon = julie_dir
                        .canonicalize()
                        .unwrap_or_else(|_| julie_dir.clone());
                    let home_canon = home.canonicalize().unwrap_or_else(|_| home.clone());
                    // macOS has a case-insensitive FS by default; compare with lowercased paths.
                    if cfg!(target_os = "macos") {
                        julie_canon.to_string_lossy().to_lowercase()
                            == home_canon.to_string_lossy().to_lowercase()
                    } else {
                        julie_canon == home_canon
                    }
                });
                if is_global {
                    debug!(
                        "Skipping global ~/.julie/ config dir at: {}",
                        current.display()
                    );
                } else {
                    debug!("Found .julie directory at: {}", julie_dir.display());
                    return Ok(Some(julie_dir));
                }
            }

            // Treat .git (file or directory) as a project boundary.
            // If this directory has .git but no .julie/, it's a project root
            // that hasn't been indexed yet — stop here so the caller creates
            // a new .julie/ instead of walking into an unrelated one (e.g. ~/.julie/).
            let git_path = current.join(".git");
            if git_path.exists() {
                debug!(
                    "Hit .git boundary at {} without finding .julie — stopping walk",
                    current.display()
                );
                return Ok(None);
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
            "cache", "logs", "config",
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

        // Ensure .gitignore exists to prevent accidental commits
        let gitignore_path = self.julie_dir.join(".gitignore");
        if !gitignore_path.exists() {
            fs::write(
                &gitignore_path,
                "# Julie code intelligence data - do not commit to version control\n\
                *\n\
                !.gitignore\n",
            )?;
            info!("Created .gitignore in .julie directory");
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

        health.watcher_state = if self.watcher.is_some() {
            WatcherState::Local
        } else {
            WatcherState::Unavailable
        };
        health.search_projection_state = if self.search_index.is_some() {
            ProjectionState::Ready
        } else {
            ProjectionState::Missing
        };
        health.embedding_state = match (
            self.embedding_runtime_status.as_ref(),
            self.embedding_provider.as_ref(),
        ) {
            (Some(runtime), Some(_)) if runtime.degraded_reason.is_some() => {
                EmbeddingState::Degraded
            }
            (Some(_), Some(_)) => EmbeddingState::Initialized,
            (Some(_), None) => EmbeddingState::Unavailable,
            (None, Some(_)) => EmbeddingState::NotInitialized,
            (None, None) => EmbeddingState::NotInitialized,
        };

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

    /// Get the root indexes directory (contains all workspace indexes).
    /// When `index_root_override` is set, returns that path directly instead of
    /// the default `{julie_dir}/indexes`.
    pub fn indexes_root_path(&self) -> PathBuf {
        self.index_root_override
            .clone()
            .unwrap_or_else(|| self.julie_dir.join("indexes"))
    }

    /// Override the indexes root directory.
    /// Used by the daemon's WorkspacePool to redirect database/search index
    /// storage to a shared location (e.g. `~/.julie/indexes/{workspace_id}`).
    pub fn set_index_root(&mut self, path: PathBuf) {
        self.index_root_override = Some(path);
    }

    /// Get the shared indexes parent directory.
    ///
    /// When `index_root_override` is set (daemon mode), the override points to
    /// a workspace-specific dir (e.g., `~/.julie/indexes/julie_528d4264/`).
    /// The shared parent is one level up (`~/.julie/indexes/`).
    /// Without override, `indexes_root_path()` already IS the shared parent.
    fn shared_indexes_dir(&self) -> PathBuf {
        if self.index_root_override.is_some() {
            let root = self.indexes_root_path();
            root.parent().unwrap_or(&root).to_path_buf()
        } else {
            self.indexes_root_path()
        }
    }

    /// Get the path to a specific workspace's index directory (SQLite database)
    pub fn workspace_index_path(&self, workspace_id: &str) -> PathBuf {
        self.shared_indexes_dir().join(workspace_id).join("db")
    }

    /// Get the path to a specific workspace's Tantivy search index
    pub fn workspace_tantivy_path(&self, workspace_id: &str) -> PathBuf {
        self.shared_indexes_dir().join(workspace_id).join("tantivy")
    }

    /// Get the path to a specific workspace's SQLite database
    pub fn workspace_db_path(&self, workspace_id: &str) -> PathBuf {
        self.shared_indexes_dir()
            .join(workspace_id)
            .join("db")
            .join("symbols.db")
    }

    /// Get the path to the general cache
    pub fn cache_path(&self) -> PathBuf {
        self.julie_dir.join("cache")
    }

    /// Get all cache directories (for bulk operations like cleanup)
    ///
    /// Returns a list of all cache subdirectories managed by the workspace.
    /// Useful for cleanup operations, size monitoring, or validation.
    pub fn get_all_cache_dirs(&self) -> Vec<PathBuf> {
        vec![self.julie_dir.join("cache").join("parse_cache")]
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

    /// Initialize Tantivy search index for full-text code search
    pub fn initialize_search_index(&mut self) -> Result<()> {
        if self.search_index.is_some() {
            return Ok(()); // Already initialized
        }

        let workspace_id = registry::generate_workspace_id(
            self.root
                .to_str()
                .ok_or_else(|| anyhow!("Invalid workspace path"))?,
        )?;

        let tantivy_path = self.workspace_tantivy_path(&workspace_id);
        info!(
            "Initializing Tantivy search index at: {}",
            tantivy_path.display()
        );

        // If the database has 0 symbols but a Tantivy index exists on disk,
        // the index contains stale segments from a previous session (e.g.,
        // after a crash or DB migration).  Delete them before opening —
        // Tantivy's background merge threads can hit IO errors on corrupted
        // or Windows-locked stale segments, killing the writer permanently.
        if tantivy_path.exists() {
            let db_empty = self
                .db
                .as_ref()
                .and_then(|db| {
                    db.lock()
                        .ok()
                        .and_then(|g| g.count_symbols_for_workspace().ok())
                })
                .is_some_and(|count| count == 0);

            if db_empty {
                info!(
                    "Database is empty — deleting stale Tantivy index at {}",
                    tantivy_path.display()
                );
                if let Err(e) = std::fs::remove_dir_all(&tantivy_path) {
                    warn!("Failed to delete stale Tantivy index: {e}");
                }
            }
        }

        // Ensure directory exists (create_dir_all handles parents)
        std::fs::create_dir_all(&tantivy_path).context(format!(
            "Failed to create Tantivy index directory: {}",
            tantivy_path.display()
        ))?;

        let configs = crate::search::LanguageConfigs::load_embedded();
        let open_outcome =
            crate::search::SearchIndex::open_or_create_with_language_configs_outcome(
                &tantivy_path,
                &configs,
            )
            .context("Failed to open or create Tantivy search index")?;

        let repair_required = open_outcome.repair_required();
        let index = open_outcome.into_index();

        if repair_required {
            warn!(
                "Tantivy search index at {} was recreated empty during open; rebuilding projection from canonical SQLite state",
                tantivy_path.display()
            );

            let db = self.db.as_ref().ok_or_else(|| {
                anyhow!("Database must be initialized before repairing recreated Tantivy index")
            })?;
            let mut db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let projection = crate::search::SearchProjection::tantivy(workspace_id.clone());
            projection.repair_recreated_open_if_needed(&mut db, &index, repair_required, None)?;
        }

        self.search_index = Some(Arc::new(std::sync::Mutex::new(index)));
        info!("Tantivy search index initialized successfully");
        Ok(())
    }

    /// Initialize file watcher for incremental updates
    pub fn initialize_file_watcher(&mut self) -> Result<()> {
        if self.watcher.is_some() {
            return Ok(()); // Already initialized
        }

        // Ensure database is initialized before file watcher
        if self.db.is_none() {
            return Err(anyhow::anyhow!(
                "Database not initialized before file watcher"
            ));
        }

        info!("Initializing file watcher for: {}", self.root.display());

        // Create placeholder extractor manager for now
        let extractor_manager = Arc::new(crate::extractors::ExtractorManager::new());

        let shared_provider = Arc::new(std::sync::RwLock::new(self.embedding_provider.clone()));
        let file_watcher = IncrementalIndexer::new(
            self.root.clone(),
            self.db.as_ref().unwrap().clone(),
            extractor_manager,
            self.search_index.clone(),
            shared_provider,
            Arc::clone(&self.indexing_runtime),
        )?;

        self.watcher = Some(file_watcher);

        info!("File watcher initialized successfully");
        Ok(())
    }

    /// Initialize all persistent components (database, search index, file watcher).
    ///
    /// Embedding provider initialization is intentionally deferred — it can take
    /// 30-60s on cold start (venv bootstrap, pip install, model download) and
    /// nothing in the indexing pipeline needs it.  The embedding provider is
    /// initialized lazily in [`initialize_embedding_provider`] which is called
    /// by the embedding pipeline after indexing completes.
    pub async fn initialize_all_components(&mut self) -> Result<()> {
        self.initialize_database()?;
        self.initialize_search_index()?;

        // Initialize file watcher (requires database)
        if self.config.incremental_updates {
            self.initialize_file_watcher()?;
        }

        info!("All workspace components initialized successfully");
        Ok(())
    }

    /// Initialize the embedding provider (best-effort).
    ///
    /// This is called lazily by the embedding pipeline after indexing completes,
    /// NOT during workspace initialization. Cold starts (venv bootstrap, pip
    /// install, model download) can take 30-60s — deferring this lets keyword
    /// search and navigation become available immediately.
    ///
    /// If initialization fails, `embedding_provider` stays `None` and keyword
    /// search continues to work without embeddings.
    pub fn initialize_embedding_provider(&mut self) {
        let (provider, runtime_status) = crate::embeddings::create_embedding_provider();
        self.embedding_provider = provider.clone();
        self.embedding_runtime_status = runtime_status;
        // Propagate to file watcher so incremental updates use the new provider
        if let Some(ref watcher) = self.watcher {
            watcher.update_embedding_provider(provider);
        }
    }

    /// Start file watching if initialized
    pub async fn start_file_watching(&mut self) -> Result<()> {
        if let Some(ref mut watcher) = self.watcher {
            watcher.start_watching().await?;
            info!("File watching started");
        }
        Ok(())
    }

    /// Stop file watching and signal background tasks to exit.
    pub async fn stop_file_watching(&mut self) -> Result<()> {
        if let Some(ref mut watcher) = self.watcher {
            watcher.stop().await?;
            info!("File watching stopped");
        }
        Ok(())
    }

    /// Pause the file watcher's event dispatch (Fix C part a).
    /// Events continue to accumulate but are not processed until `resume_file_watching`.
    pub fn pause_file_watching(&self) {
        if let Some(ref watcher) = self.watcher {
            watcher.pause();
        }
    }

    /// Resume the file watcher after a `pause_file_watching()` call.
    pub fn resume_file_watching(&self) {
        if let Some(ref watcher) = self.watcher {
            watcher.resume();
        }
    }
}

/// Health status of a Julie workspace
#[derive(Debug)]
pub struct WorkspaceHealth {
    pub structure_valid: bool,
    pub disk_space_mb: u64,
    pub has_write_permissions: bool,
    pub watcher_state: WatcherState,
    pub search_projection_state: ProjectionState,
    pub embedding_state: EmbeddingState,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl WorkspaceHealth {
    fn new() -> Self {
        Self {
            structure_valid: false,
            disk_space_mb: 0,
            has_write_permissions: false,
            watcher_state: WatcherState::Unavailable,
            search_projection_state: ProjectionState::Missing,
            embedding_state: EmbeddingState::NotInitialized,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn check_disk_space(&mut self, julie_dir: &Path) -> Result<()> {
        let path = if julie_dir.exists() {
            julie_dir
        } else if let Some(p) = julie_dir.parent() {
            p
        } else {
            julie_dir
        };
        match fs2::available_space(path) {
            Ok(bytes) => {
                self.disk_space_mb = bytes / (1024 * 1024);
                if self.disk_space_mb < 100 {
                    self.warnings.push(format!(
                        "Low disk space: only {}MB available",
                        self.disk_space_mb
                    ));
                }
            }
            Err(e) => {
                // Non-fatal: log it but don't fail the health check
                self.warnings
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
