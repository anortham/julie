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
pub mod registry_service;

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

        // ✅ Comprehensive health checks implemented in ManageWorkspaceTool::health_command()
        // See src/tools/workspace/commands/registry.rs:
        // - check_database_health() - SQLite statistics and integrity
        // - check_search_engine_health() - Tantivy search status
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

    /// Get the path to a specific workspace's Tantivy search index
    pub fn workspace_tantivy_path(&self, workspace_id: &str) -> PathBuf {
        self.indexes_root_path().join(workspace_id).join("tantivy")
    }

    /// Get the path to a specific workspace's SQLite database
    pub fn workspace_db_path(&self, workspace_id: &str) -> PathBuf {
        self.indexes_root_path()
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
        let index = crate::search::SearchIndex::open_or_create_with_language_configs(
            &tantivy_path,
            &configs,
        )
        .context("Failed to open or create Tantivy search index")?;

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

        let file_watcher = IncrementalIndexer::new(
            self.root.clone(),
            self.db.as_ref().unwrap().clone(),
            extractor_manager,
            self.search_index.clone(),
            self.embedding_provider.clone(),
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
        use crate::embeddings::{
            BackendResolverCapabilities, EmbeddingBackend, EmbeddingConfig,
            EmbeddingProviderFactory, EmbeddingRuntimeStatus, fallback_backend_after_init_failure,
            parse_provider_preference, resolve_backend_preference,
            should_disable_for_strict_acceleration, strict_acceleration_enabled_from_env_value,
        };

        let strict_accel = std::env::var("JULIE_EMBEDDING_STRICT_ACCEL")
            .ok()
            .is_some_and(|value| strict_acceleration_enabled_from_env_value(&value));

        let strict_reason = |base_reason: &str| {
            format!(
                "Embedding disabled by strict acceleration mode (JULIE_EMBEDDING_STRICT_ACCEL): {base_reason}"
            )
        };

        let log_runtime_status = |workspace: &JulieWorkspace, fallback_used: bool| {
            let Some(status) = workspace.embedding_runtime_status.as_ref() else {
                info!(
                    strict_mode = strict_accel,
                    fallback_used = fallback_used,
                    "Embedding runtime status unavailable"
                );
                return;
            };

            let provider_info = workspace
                .embedding_provider
                .as_ref()
                .map(|provider| provider.device_info());
            let fields = build_embedding_runtime_log_fields(
                status,
                provider_info.as_ref(),
                strict_accel,
                fallback_used,
            );

            info!(
                requested_backend = %fields.requested_backend,
                resolved_backend = %fields.resolved_backend,
                runtime = %fields.runtime,
                device = %fields.device,
                accelerated = fields.accelerated,
                degraded_reason = %fields.degraded_reason,
                telemetry_confidence = %fields.telemetry_confidence,
                strict_mode = fields.strict_mode,
                fallback_used = fields.fallback_used,
                "Embedding runtime status"
            );
        };

        let mut config = EmbeddingConfig::default();
        if let Ok(provider) = std::env::var("JULIE_EMBEDDING_PROVIDER") {
            config.provider = provider;
        }
        config.cache_dir = std::env::var("JULIE_EMBEDDING_CACHE_DIR")
            .ok()
            .map(std::path::PathBuf::from);

        let requested_backend = match parse_provider_preference(&config.provider) {
            Ok(backend) => backend,
            Err(err) => {
                self.embedding_provider = None;
                self.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
                    requested_backend: EmbeddingBackend::Invalid(config.provider.clone()),
                    resolved_backend: EmbeddingBackend::Unresolved,
                    accelerated: false,
                    degraded_reason: Some(err.to_string()),
                });
                log_runtime_status(self, false);
                warn!(
                    "Embedding provider unavailable (keyword search unaffected): {}",
                    err
                );
                return;
            }
        };
        let capabilities = BackendResolverCapabilities::current();
        let resolved_backend =
            match resolve_backend_preference(requested_backend.clone(), &capabilities) {
                Ok(backend) => backend,
                Err(err) => {
                    let reason = if strict_accel {
                        strict_reason(&err.to_string())
                    } else {
                        err.to_string()
                    };
                    self.embedding_provider = None;
                    self.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
                        requested_backend,
                        resolved_backend: EmbeddingBackend::Unresolved,
                        accelerated: false,
                        degraded_reason: Some(reason.clone()),
                    });
                    log_runtime_status(self, false);
                    warn!(
                        "Embedding provider unavailable (keyword search unaffected): {}",
                        reason
                    );
                    return;
                }
            };

        match EmbeddingProviderFactory::create(&config) {
            Ok(provider) => {
                let info = provider.device_info();
                info!(
                    "Embedding provider initialized: {} ({}, {}d)",
                    info.model_name, info.device, info.dimensions
                );

                let degraded_reason = provider.degraded_reason();
                let accelerated = provider
                    .accelerated()
                    .unwrap_or_else(|| info.is_accelerated());

                if should_disable_for_strict_acceleration(
                    strict_accel,
                    &resolved_backend,
                    accelerated,
                    degraded_reason.as_deref(),
                ) {
                    let strict_degraded_reason =
                        strict_reason(degraded_reason.as_deref().unwrap_or("degraded runtime"));
                    warn!(
                        "Embedding provider unavailable (keyword search unaffected): {}",
                        strict_degraded_reason
                    );
                    self.embedding_provider = None;
                    self.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
                        requested_backend,
                        resolved_backend,
                        accelerated: false,
                        degraded_reason: Some(strict_degraded_reason),
                    });
                    log_runtime_status(self, false);
                    return;
                }

                self.embedding_provider = Some(provider);
                self.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
                    requested_backend,
                    resolved_backend,
                    accelerated,
                    degraded_reason,
                });
                log_runtime_status(self, false);
            }
            Err(e) => {
                if let Some(fallback_backend) = fallback_backend_after_init_failure(
                    requested_backend.clone(),
                    resolved_backend.clone(),
                    strict_accel,
                    capabilities,
                ) {
                    let mut fallback_config = config.clone();
                    fallback_config.provider = fallback_backend.as_str().to_string();

                    match EmbeddingProviderFactory::create(&fallback_config) {
                        Ok(provider) => {
                            let info = provider.device_info();
                            info!(
                                "Embedding provider initialized via fallback: {} ({}, {}d)",
                                info.model_name, info.device, info.dimensions
                            );

                            let provider_degraded_reason = provider.degraded_reason();
                            let accelerated = provider
                                .accelerated()
                                .unwrap_or_else(|| info.is_accelerated());
                            let fallback_reason = format!(
                                "Auto backend '{}' failed to initialize, fell back to '{}': {}",
                                resolved_backend.as_str(),
                                fallback_backend.as_str(),
                                e
                            );
                            let degraded_reason = provider_degraded_reason
                                .map(|reason| {
                                    format!("{fallback_reason}; fallback runtime detail: {reason}")
                                })
                                .or(Some(fallback_reason));

                            self.embedding_provider = Some(provider);
                            self.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
                                requested_backend,
                                resolved_backend: fallback_backend,
                                accelerated,
                                degraded_reason,
                            });
                            log_runtime_status(self, true);
                            return;
                        }
                        Err(fallback_error) => {
                            warn!(
                                "Embedding fallback to '{}' failed (keyword search unaffected): {}",
                                fallback_backend.as_str(),
                                fallback_error
                            );
                        }
                    }
                }

                warn!(
                    "Embedding provider unavailable (keyword search unaffected): {}",
                    e
                );
                self.embedding_provider = None;
                self.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
                    requested_backend,
                    resolved_backend,
                    accelerated: false,
                    degraded_reason: Some(e.to_string()),
                });
                log_runtime_status(self, false);
            }
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
