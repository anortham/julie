// src/workspace/mod.rs
//! Julie Workspace Management
//!
//! This module manages the .julie workspace folder structure and initialization.
//! The workspace provides project-local storage for all Julie data including:
//! - Checkout store (`facts.sqlite` + Tantivy projection)
//! - Configuration and caching
//! - Workspace registry for multi-project indexing

pub mod mutation_gate;
pub mod registry;
pub mod root_safety;
pub mod startup_hint;

use anyhow::{Context, Result, anyhow};
use julie_core::health_types::{EmbeddingState, ProjectionState, WatcherState};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};
// Import IncrementalIndexer from watcher module
use crate::watcher::IncrementalIndexer;
use julie_index::checkout_store::{CheckoutStore, VersionMismatch};

/// The main Julie workspace structure
///
/// Manages all project-local data storage and provides a unified interface
/// to the search architecture (facts.sqlite + Tantivy projection)
pub struct JulieWorkspace {
    /// Project root directory where MCP was started
    pub root: PathBuf,

    /// The .julie directory for all workspace data
    pub julie_dir: PathBuf,

    /// Facts, graph, and Tantivy projection for this checkout.
    pub store: Arc<CheckoutStore>,

    /// File watcher for incremental updates
    pub watcher: Option<IncrementalIndexer>,

    /// Embedding provider for semantic vector generation (None if unavailable)
    pub embedding_provider: Option<Arc<dyn julie_pipeline::embeddings::EmbeddingProvider>>,

    /// Runtime status for embedding backend initialization.
    pub embedding_runtime_status: Option<julie_pipeline::embeddings::EmbeddingRuntimeStatus>,

    /// Workspace configuration
    pub config: WorkspaceConfig,

    /// Override for the indexes root directory.
    /// When set, `indexes_root_path()` returns this instead of `{julie_dir}/indexes`.
    /// Used by the daemon's WorkspacePool to redirect indexes to a shared location.
    pub index_root_override: Option<PathBuf>,

    /// Shared runtime indexing state used by health reporting and the dashboard.
    pub indexing_runtime: julie_core::indexing_state::SharedIndexingRuntime,
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

// Embedding runtime log-field helper re-exported for callers that reach it via
// `crate::workspace::build_embedding_runtime_log_fields`.  Production callers
// may also import directly from `julie_pipeline::embeddings::log_fields`.
pub use julie_pipeline::embeddings::log_fields::build_embedding_runtime_log_fields;

impl Clone for JulieWorkspace {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            julie_dir: self.julie_dir.clone(),
            store: Arc::clone(&self.store),
            watcher: None, // Don't clone file watcher - create new if needed
            embedding_provider: self.embedding_provider.clone(),
            embedding_runtime_status: self.embedding_runtime_status.clone(),
            config: self.config.clone(),
            index_root_override: self.index_root_override.clone(),
            indexing_runtime: Arc::clone(&self.indexing_runtime),
        }
    }
}

/// Open `indexes/<id>/`. Only a facts version mismatch deletes the directory
/// and reopens; every other failure propagates with the directory intact.
pub fn open_or_recreate_store(index_dir: &Path, root: &Path) -> Result<CheckoutStore> {
    match CheckoutStore::open(index_dir, root) {
        Ok(store) => Ok(store),
        Err(err) if err.downcast_ref::<VersionMismatch>().is_some() => {
            warn!(
                error = %err,
                index_dir = %index_dir.display(),
                "facts version mismatch; deleting indexes/<id>/ and reopening"
            );
            if index_dir.exists() {
                fs::remove_dir_all(index_dir)
                    .with_context(|| format!("delete index dir {}", index_dir.display()))?;
            }
            CheckoutStore::open(index_dir, root)
        }
        Err(err) => {
            Err(err).with_context(|| format!("open checkout store {}", index_dir.display()))
        }
    }
}

fn open_store_for(
    root: &Path,
    julie_dir: &Path,
    index_root_override: Option<&Path>,
) -> Result<Arc<CheckoutStore>> {
    let workspace_id = registry::generate_workspace_id(
        root.to_str()
            .ok_or_else(|| anyhow!("Invalid workspace path"))?,
    )?;
    let shared = if let Some(override_root) = index_root_override {
        override_root
            .parent()
            .unwrap_or(override_root)
            .to_path_buf()
    } else {
        julie_dir.join("indexes")
    };
    let index_dir = shared.join(workspace_id);
    Ok(Arc::new(open_or_recreate_store(&index_dir, root)?))
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

        let store = open_store_for(&root, &julie_dir, None)?;
        let mut workspace = Self {
            root,
            julie_dir,
            store,
            watcher: None,
            embedding_provider: None,
            embedding_runtime_status: None,
            config,
            index_root_override: None,
            indexing_runtime: julie_core::indexing_state::IndexingRuntimeState::shared(),
        };

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
                let store = open_store_for(&root, &julie_path, None)?;

                let mut workspace = Self {
                    root,
                    julie_dir: julie_path,
                    store,
                    watcher: None,
                    embedding_provider: None,
                    embedding_runtime_status: None,
                    config,
                    index_root_override: None,
                    indexing_runtime: julie_core::indexing_state::IndexingRuntimeState::shared(),
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
    /// Stops at VCS-repository-root boundary markers (`julie_core::paths::VCS_ROOT_MARKERS`
    /// — `.git`, `.hg`, `.svn`, `.jj`, `.bzr`, `_darcs`; each matched as a file OR a
    /// directory, so a git worktree/submodule `.git` *file* still counts) to prevent
    /// walking past worktrees or sibling projects into unrelated `.julie/` dirs (e.g. a
    /// non-project `.julie/` directory from a parent/temp path). Build manifests
    /// (`Cargo.toml`, `package.json`) are deliberately NOT boundaries here: they also
    /// appear in monorepo sub-packages and would falsely halt the walk inside a workspace
    /// member. See `VCS_ROOT_MARKERS` for the pre-1.7-SVN nesting caveat.
    pub fn find_workspace_root(start_path: &Path) -> Result<Option<PathBuf>> {
        let mut current = start_path.to_path_buf();

        // We use `RegistryPaths::is_any_known_julie_home` to detect the global
        // Julie home. Unlike a plain `RegistryPaths::try_new().ok()` lookup, this
        // helper ALWAYS treats `~/.julie` as a known home — even when
        // `JULIE_HOME` is set to an empty/invalid value. Without this defense,
        // walking up from a temp dir or any path without a `.git` boundary
        // would find `~/.julie/` and treat the entire home directory as a
        // workspace — potentially walking OneDrive-synced folders and
        // triggering mass file downloads on Windows.

        loop {
            let julie_dir = current.join(".julie");
            if julie_dir.exists() && julie_dir.is_dir() {
                // Skip the global Julie home config dir — it's not a workspace.
                // The helper handles canonicalize-with-fallback AND combines
                // the configured override with the conventional ~/.julie
                // default, so the guard cannot be silently disabled by a
                // broken env.
                let is_global =
                    julie_core::paths::RegistryPaths::is_any_known_julie_home(&julie_dir);
                if is_global {
                    debug!(
                        "Skipping global Julie home config dir at: {}",
                        current.display()
                    );
                } else {
                    debug!("Found .julie directory at: {}", julie_dir.display());
                    return Ok(Some(julie_dir));
                }
            }

            // Treat any VCS repository root as a project boundary. If this directory
            // is a VCS root but has no .julie/, it's a project root that hasn't been
            // indexed yet — stop here so the caller creates a new .julie/ instead of
            // walking into an unrelated one (e.g. ~/.julie/, or a stray .julie left in
            // a parent/temp dir). Build manifests (Cargo.toml/package.json) are
            // intentionally excluded as boundaries because they also appear in monorepo
            // sub-packages and would falsely halt the walk inside a workspace member crate.
            for marker in julie_core::paths::VCS_ROOT_MARKERS {
                let boundary = current.join(marker);
                if boundary.exists() {
                    debug!(
                        "Hit {} boundary at {} without finding .julie — stopping walk",
                        marker,
                        current.display()
                    );
                    return Ok(None);
                }
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
        health.search_state = ProjectionState::Ready;
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

    /// Get the root indexes directory (contains all workspace indexes).
    /// When `index_root_override` is set, returns that path directly instead of
    /// the default `{julie_dir}/indexes`.
    pub fn indexes_root_path(&self) -> PathBuf {
        self.index_root_override
            .clone()
            .unwrap_or_else(|| self.julie_dir.join("indexes"))
    }

    /// Alias for `indexes_root_path`.
    pub fn index_root(&self) -> PathBuf {
        self.indexes_root_path()
    }

    /// Override the indexes root directory.
    /// Used by the daemon's WorkspacePool to redirect database/search index
    /// storage to a shared location (e.g. `~/.julie/indexes/{workspace_id}`).
    pub fn set_index_root(&mut self, path: PathBuf) {
        self.index_root_override = Some(path);
    }

    /// Initialize a workspace with facts/tantivy redirected to `index_root`.
    pub async fn initialize_with_index_root(root: PathBuf, index_root: PathBuf) -> Result<Self> {
        let julie_dir = root.join(".julie");
        std::fs::create_dir_all(&julie_dir).with_context(|| {
            format!("Failed to create workspace dir at {}", julie_dir.display())
        })?;

        let store = open_store_for(&root, &julie_dir, Some(&index_root))?;
        let mut workspace = JulieWorkspace {
            root,
            julie_dir,
            store,
            watcher: None,
            embedding_provider: None,
            embedding_runtime_status: None,
            config: Default::default(),
            index_root_override: Some(index_root),
            indexing_runtime: julie_core::indexing_state::IndexingRuntimeState::shared(),
        };
        workspace.initialize_file_watcher()?;
        Ok(workspace)
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

    /// Directory for a workspace's facts.sqlite and tantivy projection.
    pub fn workspace_index_path(&self, workspace_id: &str) -> PathBuf {
        self.shared_indexes_dir().join(workspace_id)
    }

    /// Get the path to a specific workspace's Tantivy search index
    pub fn workspace_tantivy_path(&self, workspace_id: &str) -> PathBuf {
        self.shared_indexes_dir().join(workspace_id).join("tantivy")
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

    /// Initialize file watcher for incremental updates
    pub fn initialize_file_watcher(&mut self) -> Result<()> {
        if self.watcher.is_some() {
            return Ok(());
        }

        info!("Initializing file watcher for: {}", self.root.display());

        let shared_provider = Arc::new(std::sync::RwLock::new(self.embedding_provider.clone()));
        let file_watcher = IncrementalIndexer::new(
            self.root.clone(),
            Arc::clone(&self.store),
            shared_provider,
            Arc::clone(&self.indexing_runtime),
        )?;

        self.watcher = Some(file_watcher);

        info!("File watcher initialized successfully");
        Ok(())
    }

    /// Initialize the file watcher when incremental updates are enabled.
    pub async fn initialize_all_components(&mut self) -> Result<()> {
        if self.config.incremental_updates {
            self.initialize_file_watcher()?;
        }

        info!("All workspace components initialized successfully");
        Ok(())
    }

    /// Start file watching if initialized.
    ///
    /// `should_watch` lets callers skip the OS notify watcher (for example,
    /// when incremental updates are disabled).
    pub async fn start_file_watching(&mut self, should_watch: bool) -> Result<()> {
        if !should_watch {
            return Ok(());
        }
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
    pub watcher_state: WatcherState,
    pub search_state: ProjectionState,
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
            search_state: ProjectionState::Missing,
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
