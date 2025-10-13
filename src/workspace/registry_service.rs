// src/workspace/registry_service.rs
//! Workspace Registry Service for Julie
//!
//! High-performance workspace registry service with async I/O and memory caching.
//! Provides centralized workspace metadata management with atomic operations,
//! automatic cleanup, and intelligent workspace lifecycle management.

use super::registry::{current_timestamp, *};
use anyhow::{anyhow, Result};
use serde_json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// High-performance workspace registry service with caching and atomic operations
#[derive(Clone)]
pub struct WorkspaceRegistryService {
    /// Path to the primary workspace directory
    workspace_path: PathBuf,

    /// In-memory cache of the registry
    cached_registry: Arc<RwLock<Option<(WorkspaceRegistry, Instant)>>>,

    /// Mutex for atomic registry operations
    registry_lock: Arc<AsyncMutex<()>>,

    /// Cache duration (5 seconds like COA CodeSearch)
    cache_duration: Duration,
}

impl WorkspaceRegistryService {
    /// Create a new registry service for the given workspace
    pub fn new(workspace_path: PathBuf) -> Self {
        Self {
            workspace_path,
            cached_registry: Arc::new(RwLock::new(None)),
            registry_lock: Arc::new(AsyncMutex::new(())),
            cache_duration: Duration::from_secs(5),
        }
    }

    /// Get the path to the registry file
    fn registry_path(&self) -> PathBuf {
        self.workspace_path
            .join(".julie")
            .join("workspace_registry.json")
    }

    /// Get the path to the backup registry file
    fn backup_registry_path(&self) -> PathBuf {
        let registry_path = self.registry_path();
        registry_path.with_extension("json.backup")
    }

    /// Load the registry from cache or disk
    pub async fn load_registry(&self) -> Result<WorkspaceRegistry> {
        // Check memory cache first
        {
            let cache = self.cached_registry.read().unwrap();
            if let Some((registry, cached_at)) = cache.as_ref() {
                if cached_at.elapsed() < self.cache_duration {
                    debug!("Registry loaded from cache");
                    return Ok(registry.clone());
                }
            }
        }

        // Cache miss or expired - load from disk with lock
        let _lock = self.registry_lock.lock().await;

        // Double-check cache after acquiring lock
        {
            let cache = self.cached_registry.read().unwrap();
            if let Some((registry, cached_at)) = cache.as_ref() {
                if cached_at.elapsed() < self.cache_duration {
                    return Ok(registry.clone());
                }
            }
        }

        let registry = self.load_registry_from_disk().await?;

        // Update cache
        {
            let mut cache = self.cached_registry.write().unwrap();
            *cache = Some((registry.clone(), Instant::now()));
        }

        debug!(
            "Registry loaded from disk and cached. Workspaces: {}, Orphans: {}",
            registry.reference_workspaces.len()
                + if registry.primary_workspace.is_some() {
                    1
                } else {
                    0
                },
            registry.orphaned_indexes.len()
        );

        Ok(registry)
    }

    /// Save the registry to disk with atomic operations
    pub async fn save_registry(&self, registry: WorkspaceRegistry) -> Result<()> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static CALL_ID: AtomicU64 = AtomicU64::new(0);
        let call_id = CALL_ID.fetch_add(1, Ordering::SeqCst);

        println!(
            "🐛 [CALL {}] save_registry() ENTRY - acquiring lock...",
            call_id
        );
        let _lock = self.registry_lock.lock().await;
        println!(
            "🐛 [CALL {}] save_registry() - lock acquired, calling save_registry_internal",
            call_id
        );
        let result = self.save_registry_internal(registry).await;
        println!(
            "🐛 [CALL {}] save_registry() - save_registry_internal returned, releasing lock",
            call_id
        );
        result
    }

    /// Internal save function that assumes lock is already held
    /// Used by both save_registry() and load_registry_from_disk() to prevent deadlock
    async fn save_registry_internal(&self, mut registry: WorkspaceRegistry) -> Result<()> {
        // Get caller information using backtrace
        let caller = std::panic::Location::caller();
        println!(
            "🐛 save_registry_internal() ENTRY - called from {}:{}:{}",
            caller.file(),
            caller.line(),
            caller.column()
        );
        // Update metadata
        registry.last_updated = current_timestamp();
        self.update_registry_statistics(&mut registry).await?;

        // Ensure .julie directory exists (avoid redundant create_dir_all calls)
        let julie_dir = self.workspace_path.join(".julie");
        println!(
            "🐛 save_registry: workspace_path = {:?}",
            self.workspace_path
        );
        println!(
            "🐛 save_registry: julie_dir = {:?}, exists = {}",
            julie_dir,
            julie_dir.exists()
        );
        if !julie_dir.exists() {
            println!("🐛 save_registry: Creating julie_dir");
            fs::create_dir_all(&julie_dir).await?;
            println!("🐛 save_registry: julie_dir created successfully");
        }

        // Atomic write with temp file
        let registry_path = self.registry_path();
        let temp_path =
            registry_path.with_file_name(format!("workspace_registry.{}.tmp", Uuid::new_v4()));
        println!(
            "🐛 save_registry: registry_path = {:?}, temp_path = {:?}",
            registry_path, temp_path
        );

        let json = serde_json::to_string_pretty(&registry)
            .map_err(|e| anyhow!("Failed to serialize registry: {}", e))?;

        fs::write(&temp_path, &json)
            .await
            .map_err(|e| anyhow!("Failed to write temp registry file: {}", e))?;
        println!("🐛 save_registry: temp file written, checking existence...");
        println!(
            "🐛 save_registry: temp_path exists = {}",
            temp_path.exists()
        );
        println!(
            "🐛 save_registry: registry_path exists = {}",
            registry_path.exists()
        );

        // Atomic rename
        println!(
            "🐛 save_registry: About to rename {} -> {}",
            temp_path.display(),
            registry_path.display()
        );
        match fs::rename(&temp_path, &registry_path).await {
            Ok(_) => {
                println!("🐛 save_registry: Rename succeeded!");
            }
            Err(e) => {
                println!("🐛 save_registry: Rename FAILED: {}", e);
                println!(
                    "🐛 save_registry: After failure - temp_path exists = {}",
                    temp_path.exists()
                );
                println!(
                    "🐛 save_registry: After failure - registry_path exists = {}",
                    registry_path.exists()
                );
                println!(
                    "🐛 save_registry: After failure - julie_dir exists = {}",
                    julie_dir.exists()
                );

                // Attempt cleanup of unique temp file before returning error
                if temp_path.exists() {
                    let _ = std::fs::remove_file(&temp_path);
                }

                return Err(anyhow!("Failed to rename temp registry file: {}", e));
            }
        }

        // 🐛 VALIDATION: Verify the written file is valid JSON (catches corruption bugs)
        let written_content = fs::read_to_string(&registry_path)
            .await
            .map_err(|e| anyhow!("Failed to read back registry file: {}", e))?;

        if let Err(e) = serde_json::from_str::<WorkspaceRegistry>(&written_content) {
            error!("🚨 BUG DETECTED: Registry file corrupted after write!");
            error!("Written JSON length: {} bytes", written_content.len());
            error!("Expected JSON length: {} bytes", json.len());
            error!("Parse error: {}", e);

            // Try to restore from the valid JSON we just generated
            warn!("Attempting to repair corrupted registry...");
            fs::write(&registry_path, &json).await?;
        }

        // Create backup
        let backup_path = self.backup_registry_path();
        if let Err(e) = fs::copy(&registry_path, &backup_path).await {
            warn!("Failed to create registry backup: {}", e);
        }

        // Update cache
        {
            let mut cache = self.cached_registry.write().unwrap();
            *cache = Some((registry, Instant::now()));
        }

        debug!("Registry saved successfully");
        Ok(())
    }

    /// Load registry from disk with error recovery
    async fn load_registry_from_disk(&self) -> Result<WorkspaceRegistry> {
        let registry_path = self.registry_path();

        if !registry_path.exists() {
            info!("Creating new workspace registry");
            return Ok(WorkspaceRegistry::default());
        }

        // Try to load main registry file
        match self.try_load_registry_file(&registry_path).await {
            Ok(registry) => Ok(registry),
            Err(e) => {
                warn!("Failed to load main registry file: {}", e);

                // Try backup file
                let backup_path = self.backup_registry_path();
                if backup_path.exists() {
                    info!("Attempting to restore from backup");
                    match self.try_load_registry_file(&backup_path).await {
                        Ok(registry) => {
                            // Save restored registry as main file (using save_registry_internal - lock already held)
                            // DEADLOCK FIX: Use save_registry_internal() since load_registry() already holds the lock
                            // Calling save_registry() would try to acquire the lock again → deadlock
                            self.save_registry_internal(registry.clone()).await?;
                            info!("Registry restored from backup successfully");
                            Ok(registry)
                        }
                        Err(backup_err) => {
                            error!("Both main and backup registry files are corrupted");
                            Err(anyhow!(
                                "Registry corrupted: main ({}), backup ({})",
                                e,
                                backup_err
                            ))
                        }
                    }
                } else {
                    Err(anyhow!(
                        "Registry file corrupted and no backup available: {}",
                        e
                    ))
                }
            }
        }
    }

    /// Load registry while holding the registry lock (avoids double-lock deadlocks)
    async fn load_registry_locked(&self) -> Result<WorkspaceRegistry> {
        // Try cache first (safe because caller holds lock)
        {
            let cache = self.cached_registry.read().unwrap();
            if let Some((registry, cached_at)) = cache.as_ref() {
                if cached_at.elapsed() < self.cache_duration {
                    return Ok(registry.clone());
                }
            }
        }

        // Fallback to disk and refresh cache
        let registry = self.load_registry_from_disk().await?;
        {
            let mut cache = self.cached_registry.write().unwrap();
            *cache = Some((registry.clone(), Instant::now()));
        }
        Ok(registry)
    }

    /// Try to load a specific registry file
    async fn try_load_registry_file(&self, path: &Path) -> Result<WorkspaceRegistry> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| anyhow!("Failed to read registry file: {}", e))?;

        let registry: WorkspaceRegistry = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse registry JSON: {}", e))?;

        Ok(registry)
    }

    /// Get or create a registry
    pub async fn get_or_create_registry(&self) -> Result<WorkspaceRegistry> {
        match self.load_registry().await {
            Ok(registry) => Ok(registry),
            Err(_) => {
                info!("Creating new workspace registry");
                let new_registry = WorkspaceRegistry::default();
                self.save_registry(new_registry.clone()).await?;
                Ok(new_registry)
            }
        }
    }

    /// Register a new workspace
    /// 🔒 CRITICAL: Holds lock for entire load-modify-save cycle to prevent race conditions
    pub async fn register_workspace(
        &self,
        workspace_path: String,
        workspace_type: WorkspaceType,
    ) -> Result<WorkspaceEntry> {
        println!(
            "🐛 register_workspace ENTRY: path={}, type={:?}",
            workspace_path, workspace_type
        );

        // RACE CONDITION FIX: Hold lock for ENTIRE operation to prevent concurrent modifications
        // Same pattern as update_last_accessed() - lock → load → modify → save_internal
        let _lock = self.registry_lock.lock().await;
        let mut registry = self.load_registry_locked().await?;
        println!("🐛 register_workspace: Loaded registry");

        // Create new workspace entry
        let workspace =
            WorkspaceEntry::new(workspace_path, workspace_type.clone(), &registry.config)?;

        // Check if already registered
        match &workspace_type {
            WorkspaceType::Primary => {
                if registry.primary_workspace.is_some() {
                    return Err(anyhow!("Primary workspace already registered"));
                }
                registry.primary_workspace = Some(workspace.clone());
            }
            WorkspaceType::Reference | WorkspaceType::Session => {
                if registry.reference_workspaces.contains_key(&workspace.id) {
                    return Err(anyhow!("Workspace already registered: {}", workspace.id));
                }
                registry
                    .reference_workspaces
                    .insert(workspace.id.clone(), workspace.clone());
            }
        }

        println!(
            "🐛 register_workspace: About to save registry (workspace_id={})",
            workspace.id
        );
        // Use save_registry_internal since we already hold the lock
        self.save_registry_internal(registry).await?;
        println!("🐛 register_workspace: Registry saved successfully");

        info!(
            "Registered new workspace: {} (type: {:?}, id: {})",
            workspace.original_path, workspace_type, workspace.id
        );

        Ok(workspace)
    }

    /// Unregister a workspace
    /// 🔒 CRITICAL: Holds lock for entire load-modify-save cycle to prevent race conditions
    pub async fn unregister_workspace(&self, workspace_id: &str) -> Result<bool> {
        // RACE CONDITION FIX: Hold lock for ENTIRE operation
        let _lock = self.registry_lock.lock().await;
        let mut registry = self.load_registry_locked().await?;

        // Check if it's the primary workspace
        if let Some(ref primary) = registry.primary_workspace {
            if primary.id == workspace_id {
                registry.primary_workspace = None;
                // Use save_registry_internal since we already hold the lock
                self.save_registry_internal(registry).await?;
                info!("Unregistered primary workspace: {}", workspace_id);
                return Ok(true);
            }
        }

        // Check reference workspaces
        if registry.reference_workspaces.remove(workspace_id).is_some() {
            // Use save_registry_internal since we already hold the lock
            self.save_registry_internal(registry).await?;
            info!("Unregistered reference workspace: {}", workspace_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get workspace by ID
    pub async fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceEntry>> {
        let registry = self.load_registry().await?;

        // Check primary workspace
        if let Some(ref primary) = registry.primary_workspace {
            if primary.id == workspace_id {
                return Ok(Some(primary.clone()));
            }
        }

        // Check reference workspaces
        Ok(registry.reference_workspaces.get(workspace_id).cloned())
    }

    /// Get workspace by path
    pub async fn get_workspace_by_path(&self, path: &str) -> Result<Option<WorkspaceEntry>> {
        let workspace_id = generate_workspace_id(path)?;
        self.get_workspace(&workspace_id).await
    }

    /// Get all workspaces
    pub async fn get_all_workspaces(&self) -> Result<Vec<WorkspaceEntry>> {
        let registry = self.load_registry().await?;

        let mut workspaces = Vec::new();

        if let Some(primary) = registry.primary_workspace {
            workspaces.push(primary);
        }

        workspaces.extend(registry.reference_workspaces.values().cloned());

        Ok(workspaces)
    }

    /// Get the primary workspace ID from the registry
    pub async fn get_primary_workspace_id(&self) -> Result<Option<String>> {
        let registry = self.load_registry().await?;

        if let Some(ref primary) = registry.primary_workspace {
            Ok(Some(primary.id.clone()))
        } else {
            Ok(None)
        }
    }

    /// Update workspace last accessed time
    /// 🔒 CRITICAL: Holds lock for entire load-modify-save cycle to prevent race conditions
    pub async fn update_last_accessed(&self, workspace_id: &str) -> Result<()> {
        let _lock = self.registry_lock.lock().await;
        let mut registry = self.load_registry_locked().await?;
        let mut updated = false;

        if let Some(ref mut primary) = registry.primary_workspace {
            if primary.id == workspace_id {
                primary.update_last_accessed();
                updated = true;
            }
        }

        if let Some(workspace) = registry.reference_workspaces.get_mut(workspace_id) {
            workspace.update_last_accessed();
            updated = true;
        }

        if updated {
            self.save_registry_internal(registry).await?;
        }

        Ok(())
    }

    /// Update workspace statistics
    /// 🔒 CRITICAL: Holds lock for entire load-modify-save cycle to prevent race conditions
    pub async fn update_workspace_statistics(
        &self,
        workspace_id: &str,
        symbol_count: usize,
        file_count: usize,
        index_size_bytes: u64,
    ) -> Result<()> {
        let _lock = self.registry_lock.lock().await;
        let mut registry = self.load_registry_locked().await?;
        let mut updated = false;

        // Update primary workspace
        if let Some(ref mut primary) = registry.primary_workspace {
            if primary.id == workspace_id {
                primary.symbol_count = symbol_count;
                primary.file_count = file_count;
                primary.index_size_bytes = index_size_bytes;
                updated = true;
            }
        }

        // Update reference workspace
        if let Some(workspace) = registry.reference_workspaces.get_mut(workspace_id) {
            workspace.symbol_count = symbol_count;
            workspace.file_count = file_count;
            workspace.index_size_bytes = index_size_bytes;
            updated = true;
        }

        if updated {
            self.save_registry_internal(registry).await?;
        }

        Ok(())
    }

    /// Update workspace index size only (called by background Tantivy task)
    /// 🔒 CRITICAL: Holds lock for entire load-modify-save cycle to prevent race conditions
    pub async fn update_index_size(&self, workspace_id: &str, index_size_bytes: u64) -> Result<()> {
        let _lock = self.registry_lock.lock().await;
        let mut registry = self.load_registry_locked().await?;
        let mut updated = false;

        if let Some(ref mut primary) = registry.primary_workspace {
            if primary.id == workspace_id {
                primary.index_size_bytes = index_size_bytes;
                updated = true;
            }
        }

        if let Some(workspace) = registry.reference_workspaces.get_mut(workspace_id) {
            workspace.index_size_bytes = index_size_bytes;
            updated = true;
        }

        if updated {
            self.save_registry_internal(registry).await?;
        }

        Ok(())
    }

    /// Update embedding status for a workspace
    /// 🔒 CRITICAL: Holds lock for entire load-modify-save cycle to prevent race conditions
    pub async fn update_embedding_status(
        &self,
        workspace_id: &str,
        status: crate::workspace::registry::EmbeddingStatus,
    ) -> Result<()> {
        let _lock = self.registry_lock.lock().await;
        let mut registry = self.load_registry_locked().await?;
        let mut updated = false;

        if let Some(ref mut primary) = registry.primary_workspace {
            if primary.id == workspace_id {
                primary.embedding_status = status.clone();
                updated = true;
            }
        }

        if let Some(workspace) = registry.reference_workspaces.get_mut(workspace_id) {
            workspace.embedding_status = status;
            updated = true;
        }

        if updated {
            self.save_registry_internal(registry).await?;
        }

        Ok(())
    }

    /// Get workspaces that have expired
    pub async fn get_expired_workspaces(&self) -> Result<Vec<WorkspaceEntry>> {
        let registry = self.load_registry().await?;

        Ok(registry
            .reference_workspaces
            .values()
            .filter(|w| w.is_expired())
            .cloned()
            .collect())
    }

    /// Clean up expired workspaces
    pub async fn cleanup_expired_workspaces(&self) -> Result<Vec<String>> {
        let expired = self.get_expired_workspaces().await?;
        let mut cleaned = Vec::new();

        for workspace in expired {
            if self.unregister_workspace(&workspace.id).await? {
                cleaned.push(workspace.id);
            }
        }

        if !cleaned.is_empty() {
            info!("Cleaned up {} expired workspaces", cleaned.len());
        }

        Ok(cleaned)
    }

    /// Update registry statistics
    async fn update_registry_statistics(&self, registry: &mut WorkspaceRegistry) -> Result<()> {
        let mut total_workspaces = 0;
        let mut total_symbols = 0;
        let mut total_files = 0;
        let mut total_size = 0;

        if registry.primary_workspace.is_some() {
            total_workspaces += 1;
        }

        total_workspaces += registry.reference_workspaces.len();

        for workspace in registry.reference_workspaces.values() {
            total_symbols += workspace.symbol_count;
            total_files += workspace.file_count;
            total_size += workspace.index_size_bytes;
        }

        if let Some(ref primary) = registry.primary_workspace {
            total_symbols += primary.symbol_count;
            total_files += primary.file_count;
            total_size += primary.index_size_bytes;
        }

        registry.statistics.total_workspaces = total_workspaces;
        registry.statistics.total_orphans = registry.orphaned_indexes.len();
        registry.statistics.total_symbols = total_symbols;
        registry.statistics.total_files = total_files;
        registry.statistics.total_index_size_bytes = total_size;

        Ok(())
    }

    /// Invalidate cache (useful for testing or manual refresh)
    pub fn invalidate_cache(&self) {
        let mut cache = self.cached_registry.write().unwrap();
        *cache = None;
        debug!("Registry cache invalidated");
    }

    /// Comprehensive cleanup with database and search index data removal
    pub async fn cleanup_expired_workspaces_with_data(
        &self,
        _database: Option<&std::sync::Arc<tokio::sync::Mutex<crate::database::SymbolDatabase>>>,
    ) -> Result<WorkspaceCleanupReport> {
        let expired = self.get_expired_workspaces().await?;
        let mut report = WorkspaceCleanupReport {
            workspaces_removed: Vec::new(),
            database_stats: Vec::new(),
            total_symbols_deleted: 0,
            total_files_deleted: 0,
            total_relationships_deleted: 0,
        };

        for workspace in expired {
            // Delete entire workspace directory: .julie/indexes/{workspace_id}/
            // This removes the separate database and all index data for this workspace
            let workspace_index_path = self
                .workspace_path
                .join(".julie")
                .join("indexes")
                .join(&workspace.id);

            if workspace_index_path.exists() {
                match tokio::fs::remove_dir_all(&workspace_index_path).await {
                    Ok(()) => {
                        info!(
                            "Deleted expired workspace directory: {} ({:?})",
                            workspace.id, workspace_index_path
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Failed to delete workspace directory {}: {}",
                            workspace.id, e
                        );
                    }
                }
            }

            // Remove from registry
            if self.unregister_workspace(&workspace.id).await? {
                report.workspaces_removed.push(workspace.id);
            }
        }

        if !report.workspaces_removed.is_empty() {
            info!(
                "Comprehensive cleanup completed: {} workspaces, {} symbols, {} files",
                report.workspaces_removed.len(),
                report.total_symbols_deleted,
                report.total_files_deleted
            );
        }

        Ok(report)
    }

    /// Enforce storage size limits using LRU eviction
    pub async fn enforce_size_limits(
        &self,
        _database: Option<&std::sync::Arc<tokio::sync::Mutex<crate::database::SymbolDatabase>>>,
    ) -> Result<WorkspaceCleanupReport> {
        let registry = self.load_registry().await?;
        let max_size = registry.config.max_total_size_bytes;
        let current_size = registry.statistics.total_index_size_bytes;

        if current_size <= max_size {
            debug!(
                "Storage within limits: {} / {} bytes",
                current_size, max_size
            );
            return Ok(WorkspaceCleanupReport::empty());
        }

        info!(
            "Storage limit exceeded: {} / {} bytes. Starting LRU eviction.",
            current_size, max_size
        );

        let mut report = WorkspaceCleanupReport::empty();
        let mut remaining_size = current_size;

        // Get reference workspaces sorted by last accessed (LRU)
        let registry_snapshot = self.load_registry().await?;
        let mut lru_workspaces: Vec<_> = registry_snapshot
            .reference_workspaces
            .values()
            .cloned()
            .collect();
        lru_workspaces.sort_by_key(|w| w.last_accessed);

        // Evict least recently used workspaces until under limit
        for workspace in lru_workspaces {
            if remaining_size <= max_size {
                break;
            }

            // Delete entire workspace directory: .julie/indexes/{workspace_id}/
            // This removes the separate database and all index data for this workspace
            let workspace_index_path = self
                .workspace_path
                .join(".julie")
                .join("indexes")
                .join(&workspace.id);

            if workspace_index_path.exists() {
                match tokio::fs::remove_dir_all(&workspace_index_path).await {
                    Ok(()) => {
                        info!(
                            "Deleted LRU workspace directory: {} ({:?})",
                            workspace.id, workspace_index_path
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Failed to delete workspace directory {}: {}",
                            workspace.id, e
                        );
                    }
                }
            }

            // Remove from registry
            remaining_size = remaining_size.saturating_sub(workspace.index_size_bytes);
            if self.unregister_workspace(&workspace.id).await? {
                report.workspaces_removed.push(workspace.id.clone());
                info!(
                    "Evicted workspace {} (LRU), saved {} bytes",
                    workspace.id, workspace.index_size_bytes
                );
            }
        }

        Ok(report)
    }

    /// Detect orphaned index directories that don't have registry entries
    pub async fn detect_orphaned_indexes(&self) -> Result<Vec<OrphanedIndexInfo>> {
        let registry = self.load_registry().await?;

        // Get all index directories (per-workspace architecture: .julie/indexes/{workspace_id}/)
        let indexes_dir = self
            .workspace_path
            .join(".julie")
            .join("indexes");
        if !indexes_dir.exists() {
            return Ok(Vec::new());
        }

        //  🚨 CRITICAL: Move blocking filesystem operations to spawn_blocking
        // std::fs::read_dir() and calculate_dir_size() are synchronous blocking I/O
        let indexes_dir_clone = indexes_dir.clone();
        let registry_clone = registry.clone();

        let orphans = tokio::task::spawn_blocking(move || {
            let mut orphans = Vec::new();
            let entries = std::fs::read_dir(&indexes_dir_clone)?;

            for entry in entries {
                let entry = entry?;
                let dir_name = entry.file_name().to_string_lossy().to_string();

                // Skip if this directory has a registry entry
                if registry_clone.reference_workspaces.contains_key(&dir_name) {
                    continue;
                }

                // Check if it's already marked as orphaned
                if registry_clone.orphaned_indexes.contains_key(&dir_name) {
                    continue;
                }

                // Calculate directory size using shared utility function
                let size = crate::tools::workspace::calculate_dir_size(entry.path())?;
                let metadata = entry.metadata()?;
                let last_modified = metadata
                    .modified()?
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs();

                orphans.push(OrphanedIndexInfo {
                    directory_name: dir_name,
                    size_bytes: size,
                    last_modified,
                    discovered_at: crate::workspace::registry::current_timestamp(),
                    reason: crate::workspace::registry::OrphanReason::NoRegistryEntry,
                });
            }

            Ok::<Vec<OrphanedIndexInfo>, anyhow::Error>(orphans)
        })
        .await
        .map_err(|e| anyhow!("Orphan detection task failed: {}", e))??;

        info!("Detected {} orphaned index directories", orphans.len());
        Ok(orphans)
    }

    /// Mark indexes as orphaned in registry for later cleanup
    pub async fn mark_indexes_as_orphaned(&self, orphans: Vec<OrphanedIndexInfo>) -> Result<()> {
        if orphans.is_empty() {
            return Ok(());
        }

        let mut registry = self.load_registry().await?;
        let current_time = crate::workspace::registry::current_timestamp();

        for orphan in orphans {
            let orphaned_index = crate::workspace::registry::OrphanedIndex {
                directory_name: orphan.directory_name.clone(),
                discovered_at: orphan.discovered_at,
                last_modified: orphan.last_modified,
                reason: orphan.reason,
                scheduled_for_deletion: current_time + (24 * 60 * 60), // 24 hours grace period
                size_bytes: orphan.size_bytes,
                attempted_path: None,
            };

            registry
                .orphaned_indexes
                .insert(orphan.directory_name, orphaned_index);
        }

        self.save_registry(registry).await?;
        Ok(())
    }

    /// Clean up orphaned indexes that are past their grace period
    pub async fn cleanup_orphaned_indexes(&self) -> Result<Vec<String>> {
        let mut registry = self.load_registry().await?;
        let current_time = crate::workspace::registry::current_timestamp();
        let mut cleaned = Vec::new();

        let orphaned_to_remove: Vec<_> = registry
            .orphaned_indexes
            .iter()
            .filter(|(_, orphan)| orphan.scheduled_for_deletion <= current_time)
            .map(|(name, _)| name.clone())
            .collect();

        for orphan_name in orphaned_to_remove {
            // Remove the physical directory (per-workspace architecture: .julie/indexes/{workspace_id}/)
            let orphan_path = self
                .workspace_path
                .join(".julie")
                .join("indexes")
                .join(&orphan_name);
            if orphan_path.exists() {
                match std::fs::remove_dir_all(&orphan_path) {
                    Ok(()) => {
                        info!("Deleted orphaned index directory: {}", orphan_name);
                        cleaned.push(orphan_name.clone());
                    }
                    Err(e) => {
                        warn!(
                            "Failed to delete orphaned index directory {}: {}",
                            orphan_name, e
                        );
                    }
                }
            }

            // Remove from registry
            registry.orphaned_indexes.remove(&orphan_name);
        }

        if !cleaned.is_empty() {
            self.save_registry(registry).await?;
        }

        Ok(cleaned)
    }

    /// Comprehensive cleanup: TTL + Size Limits + Orphans
    pub async fn comprehensive_cleanup(
        &self,
        database: Option<&std::sync::Arc<tokio::sync::Mutex<crate::database::SymbolDatabase>>>,
    ) -> Result<ComprehensiveCleanupReport> {
        let mut report = ComprehensiveCleanupReport::default();

        // Step 1: TTL-based cleanup
        info!("Starting TTL-based cleanup...");
        let ttl_report = self.cleanup_expired_workspaces_with_data(database).await?;
        report.ttl_cleanup = ttl_report;

        // Step 2: Size-based LRU eviction
        info!("Checking storage limits...");
        let size_report = self.enforce_size_limits(database).await?;
        report.size_cleanup = size_report;

        // Step 3: Orphan detection and cleanup
        info!("Detecting orphaned indexes...");
        let orphans = self.detect_orphaned_indexes().await?;
        if !orphans.is_empty() {
            self.mark_indexes_as_orphaned(orphans).await?;
        }
        let cleaned_orphans = self.cleanup_orphaned_indexes().await?;
        report.orphaned_cleaned = cleaned_orphans;

        let total_workspaces = report.ttl_cleanup.workspaces_removed.len()
            + report.size_cleanup.workspaces_removed.len();
        let total_orphans = report.orphaned_cleaned.len();

        if total_workspaces > 0 || total_orphans > 0 {
            info!(
                "Comprehensive cleanup completed: {} workspaces, {} orphans",
                total_workspaces, total_orphans
            );
        } else {
            debug!("No cleanup needed");
        }

        Ok(report)
    }
}

/// Report from workspace cleanup operations
#[derive(Debug, Clone, Default)]
pub struct WorkspaceCleanupReport {
    pub workspaces_removed: Vec<String>,
    pub database_stats: Vec<(String, crate::database::WorkspaceCleanupStats)>,
    pub total_symbols_deleted: i64,
    pub total_files_deleted: i64,
    pub total_relationships_deleted: i64,
}

impl WorkspaceCleanupReport {
    fn empty() -> Self {
        Self {
            workspaces_removed: Vec::new(),
            database_stats: Vec::new(),
            total_symbols_deleted: 0,
            total_files_deleted: 0,
            total_relationships_deleted: 0,
        }
    }
}

/// Information about an orphaned index directory
#[derive(Debug, Clone)]
pub struct OrphanedIndexInfo {
    pub directory_name: String,
    pub size_bytes: u64,
    pub last_modified: u64,
    pub discovered_at: u64,
    pub reason: crate::workspace::registry::OrphanReason,
}

/// Comprehensive cleanup report
#[derive(Debug, Clone, Default)]
pub struct ComprehensiveCleanupReport {
    pub ttl_cleanup: WorkspaceCleanupReport,
    pub size_cleanup: WorkspaceCleanupReport,
    pub orphaned_cleaned: Vec<String>,
}

// calculate_dir_size moved to shared utility: src/tools/workspace/utils.rs
// Use crate::tools::workspace::calculate_dir_size() instead

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::Barrier;

    #[tokio::test]
    async fn test_registry_creation() {
        let temp_dir = TempDir::new().unwrap();
        let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

        let registry = service.get_or_create_registry().await.unwrap();
        assert_eq!(registry.version, "1.0");
        assert!(registry.primary_workspace.is_none());
        assert!(registry.reference_workspaces.is_empty());
    }

    #[tokio::test]
    async fn test_workspace_registration() {
        let temp_dir = TempDir::new().unwrap();
        let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

        // Register primary workspace
        let primary = service
            .register_workspace(
                temp_dir.path().to_string_lossy().to_string(),
                WorkspaceType::Primary,
            )
            .await
            .unwrap();

        assert_eq!(primary.workspace_type, WorkspaceType::Primary);
        assert!(primary.expires_at.is_none()); // Primary never expires

        // Register reference workspace
        let ref_path = temp_dir
            .path()
            .join("reference")
            .to_string_lossy()
            .to_string();
        let reference = service
            .register_workspace(ref_path, WorkspaceType::Reference)
            .await
            .unwrap();

        assert_eq!(reference.workspace_type, WorkspaceType::Reference);
        assert!(reference.expires_at.is_some()); // Reference expires

        // Verify workspaces exist
        let all_workspaces = service.get_all_workspaces().await.unwrap();
        assert_eq!(all_workspaces.len(), 2);
    }

    #[tokio::test]
    async fn test_registry_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().to_path_buf();

        // Create and populate registry
        {
            let service = WorkspaceRegistryService::new(workspace_path.clone());
            service
                .register_workspace(
                    temp_dir.path().to_string_lossy().to_string(),
                    WorkspaceType::Primary,
                )
                .await
                .unwrap();
        }

        // Create new service instance and verify persistence
        {
            let service = WorkspaceRegistryService::new(workspace_path);
            let registry = service.load_registry().await.unwrap();
            assert!(registry.primary_workspace.is_some());
        }
    }

    #[tokio::test]
    async fn test_concurrent_registry_saves_do_not_conflict_on_temp_file() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path().to_path_buf();

        // Prime the registry with a primary workspace using one service instance
        {
            let primary_service = WorkspaceRegistryService::new(workspace_root.clone());
            let primary_dir = workspace_root.join("primary");
            std::fs::create_dir_all(&primary_dir).unwrap();
            primary_service
                .register_workspace(
                    primary_dir.to_string_lossy().to_string(),
                    WorkspaceType::Primary,
                )
                .await
                .unwrap();
        }

        let reference_a_dir = workspace_root.join("reference_a");
        let reference_b_dir = workspace_root.join("reference_b");
        std::fs::create_dir_all(&reference_a_dir).unwrap();
        std::fs::create_dir_all(&reference_b_dir).unwrap();

        let barrier = Arc::new(Barrier::new(2));
        let barrier_a = barrier.clone();
        let barrier_b = barrier.clone();

        let workspace_path_a = workspace_root.clone();
        let workspace_path_b = workspace_root.clone();
        let reference_a_path = reference_a_dir.to_string_lossy().to_string();
        let reference_b_path = reference_b_dir.to_string_lossy().to_string();

        let (result_a, result_b) = tokio::join!(
            async move {
                let service = WorkspaceRegistryService::new(workspace_path_a);
                barrier_a.wait().await;
                service
                    .register_workspace(reference_a_path, WorkspaceType::Reference)
                    .await
            },
            async move {
                let service = WorkspaceRegistryService::new(workspace_path_b);
                barrier_b.wait().await;
                service
                    .register_workspace(reference_b_path, WorkspaceType::Reference)
                    .await
            }
        );

        assert!(
            result_a.is_ok(),
            "First concurrent save failed: {:?}",
            result_a
        );
        assert!(
            result_b.is_ok(),
            "Second concurrent save failed: {:?}",
            result_b
        );
    }
}
