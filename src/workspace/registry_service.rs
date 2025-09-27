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

/// High-performance workspace registry service with caching and atomic operations
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
    pub async fn save_registry(&self, mut registry: WorkspaceRegistry) -> Result<()> {
        let _lock = self.registry_lock.lock().await;

        // Update metadata
        registry.last_updated = current_timestamp();
        self.update_registry_statistics(&mut registry).await?;

        // Ensure .julie directory exists
        let julie_dir = self.workspace_path.join(".julie");
        fs::create_dir_all(&julie_dir).await?;

        // Atomic write with temp file
        let registry_path = self.registry_path();
        let temp_path = registry_path.with_extension("tmp");

        let json = serde_json::to_string_pretty(&registry)
            .map_err(|e| anyhow!("Failed to serialize registry: {}", e))?;

        fs::write(&temp_path, json)
            .await
            .map_err(|e| anyhow!("Failed to write temp registry file: {}", e))?;

        // Atomic rename
        fs::rename(&temp_path, &registry_path)
            .await
            .map_err(|e| anyhow!("Failed to rename temp registry file: {}", e))?;

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
                            // Save restored registry as main file
                            self.save_registry(registry.clone()).await?;
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
    pub async fn register_workspace(
        &self,
        workspace_path: String,
        workspace_type: WorkspaceType,
    ) -> Result<WorkspaceEntry> {
        let mut registry = self.load_registry().await?;

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

        self.save_registry(registry).await?;

        info!(
            "Registered new workspace: {} (type: {:?}, id: {})",
            workspace.original_path, workspace_type, workspace.id
        );

        Ok(workspace)
    }

    /// Unregister a workspace
    pub async fn unregister_workspace(&self, workspace_id: &str) -> Result<bool> {
        let mut registry = self.load_registry().await?;

        // Check if it's the primary workspace
        if let Some(ref primary) = registry.primary_workspace {
            if primary.id == workspace_id {
                registry.primary_workspace = None;
                self.save_registry(registry).await?;
                info!("Unregistered primary workspace: {}", workspace_id);
                return Ok(true);
            }
        }

        // Check reference workspaces
        if registry.reference_workspaces.remove(workspace_id).is_some() {
            self.save_registry(registry).await?;
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

    /// Update workspace last accessed time
    pub async fn update_last_accessed(&self, workspace_id: &str) -> Result<()> {
        let mut registry = self.load_registry().await?;
        let mut updated = false;

        // Update primary workspace
        if let Some(ref mut primary) = registry.primary_workspace {
            if primary.id == workspace_id {
                primary.update_last_accessed();
                updated = true;
            }
        }

        // Update reference workspace
        if let Some(workspace) = registry.reference_workspaces.get_mut(workspace_id) {
            workspace.update_last_accessed();
            updated = true;
        }

        if updated {
            self.save_registry(registry).await?;
        }

        Ok(())
    }

    /// Update workspace statistics
    pub async fn update_workspace_statistics(
        &self,
        workspace_id: &str,
        document_count: usize,
        index_size_bytes: u64,
    ) -> Result<()> {
        let mut registry = self.load_registry().await?;
        let mut updated = false;

        // Update primary workspace
        if let Some(ref mut primary) = registry.primary_workspace {
            if primary.id == workspace_id {
                primary.document_count = document_count;
                primary.index_size_bytes = index_size_bytes;
                updated = true;
            }
        }

        // Update reference workspace
        if let Some(workspace) = registry.reference_workspaces.get_mut(workspace_id) {
            workspace.document_count = document_count;
            workspace.index_size_bytes = index_size_bytes;
            updated = true;
        }

        if updated {
            self.save_registry(registry).await?;
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
        let mut total_documents = 0;
        let mut total_size = 0;

        if registry.primary_workspace.is_some() {
            total_workspaces += 1;
        }

        total_workspaces += registry.reference_workspaces.len();

        for workspace in registry.reference_workspaces.values() {
            total_documents += workspace.document_count;
            total_size += workspace.index_size_bytes;
        }

        if let Some(ref primary) = registry.primary_workspace {
            total_documents += primary.document_count;
            total_size += primary.index_size_bytes;
        }

        registry.statistics.total_workspaces = total_workspaces;
        registry.statistics.total_orphans = registry.orphaned_indexes.len();
        registry.statistics.total_documents = total_documents;
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
        database: Option<&std::sync::Arc<tokio::sync::Mutex<crate::database::SymbolDatabase>>>,
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
            // Remove from database if available
            if let Some(db) = database {
                let db_lock = db.lock().await;
                match db_lock.delete_workspace_data(&workspace.id) {
                    Ok(stats) => {
                        report.total_symbols_deleted += stats.symbols_deleted;
                        report.total_files_deleted += stats.files_deleted;
                        report.total_relationships_deleted += stats.relationships_deleted;
                        report.database_stats.push((workspace.id.clone(), stats));
                    }
                    Err(e) => {
                        warn!(
                            "Failed to clean database data for workspace {}: {}",
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
        database: Option<&std::sync::Arc<tokio::sync::Mutex<crate::database::SymbolDatabase>>>,
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

            // Remove database data if available
            if let Some(db) = database {
                let db_lock = db.lock().await;
                match db_lock.delete_workspace_data(&workspace.id) {
                    Ok(stats) => {
                        report.total_symbols_deleted += stats.symbols_deleted;
                        report.total_files_deleted += stats.files_deleted;
                        report.total_relationships_deleted += stats.relationships_deleted;
                        report.database_stats.push((workspace.id.clone(), stats));
                    }
                    Err(e) => {
                        warn!(
                            "Failed to clean database data for workspace {}: {}",
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
        let mut orphans = Vec::new();

        // Get all index directories
        let indexes_dir = self
            .workspace_path
            .join(".julie")
            .join("index")
            .join("tantivy")
            .join("references");
        if !indexes_dir.exists() {
            return Ok(orphans);
        }

        let entries = std::fs::read_dir(&indexes_dir)?;
        for entry in entries {
            let entry = entry?;
            let dir_name = entry.file_name().to_string_lossy().to_string();

            // Skip if this directory has a registry entry
            if registry.reference_workspaces.contains_key(&dir_name) {
                continue;
            }

            // Check if it's already marked as orphaned
            if registry.orphaned_indexes.contains_key(&dir_name) {
                continue;
            }

            // Calculate directory size
            let size = calculate_directory_size(entry.path())?;
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
            // Remove the physical directory
            let orphan_path = self
                .workspace_path
                .join(".julie")
                .join("index")
                .join("tantivy")
                .join("references")
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

/// Calculate the total size of a directory recursively
fn calculate_directory_size<P: AsRef<std::path::Path>>(path: P) -> Result<u64> {
    let mut total_size = 0;
    let entries = std::fs::read_dir(path)?;

    for entry in entries {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            total_size += calculate_directory_size(entry.path())?;
        } else {
            total_size += metadata.len();
        }
    }

    Ok(total_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
}
