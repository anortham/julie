use anyhow::Result;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Implementation, ServerCapabilities, ServerInfo},
    service::NotificationContext,
    tool, tool_handler, tool_router,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tracing::{debug, info, warn};

use crate::database::SymbolDatabase;
use crate::search::SearchIndex;
use crate::workspace::JulieWorkspace;
use tokio::sync::RwLock;

// Import tool parameter types
use crate::tools::{
    DeepDiveTool, FastRefsTool, FastSearchTool, GetContextTool, GetSymbolsTool,
    ManageWorkspaceTool, QueryMetricsTool, RenameSymbolTool,
};
use crate::tools::metrics::session::{SessionMetrics, ToolCallReport, ToolKind, extract_source_paths};

/// Tracks which indexes are ready for search operations
#[derive(Debug)]
pub struct IndexingStatus {
    /// Search system (Tantivy) is ready
    pub search_ready: AtomicBool,
    /// Semantic embeddings are ready
    pub embeddings_ready: AtomicBool,
}

impl IndexingStatus {
    /// Create new indexing status with all indexes not ready
    pub fn new() -> Self {
        Self {
            search_ready: AtomicBool::new(false),
            embeddings_ready: AtomicBool::new(false),
        }
    }
}

impl Default for IndexingStatus {
    fn default() -> Self {
        Self::new()
    }
}

/// Julie's custom handler for MCP messages
///
/// This handler manages the core Julie functionality including:
/// - Code intelligence operations (search, navigation, extraction)
/// - Symbol database management
/// - Cross-language relationship detection
#[derive(Clone)]
pub struct JulieServerHandler {
    /// Resolved workspace root path (single source of truth).
    /// Set once at construction from CLI args / JULIE_WORKSPACE / cwd.
    pub(crate) workspace_root: PathBuf,
    /// Workspace managing persistent storage
    pub workspace: Arc<RwLock<Option<JulieWorkspace>>>,
    /// Flag to track if workspace has been indexed
    pub is_indexed: Arc<RwLock<bool>>,
    /// Tracks which indexes are ready for search operations
    pub indexing_status: Arc<IndexingStatus>,
    /// Per-session operational metrics (tool call timing, output sizes)
    pub session_metrics: Arc<SessionMetrics>,
    /// rmcp tool router for handling tool calls
    tool_router: ToolRouter<Self>,
}

impl JulieServerHandler {
    /// Create a new Julie server handler with all components initialized.
    ///
    /// `workspace_root` is the resolved root path for this server session,
    /// determined by the caller (main.rs) via CLI args / env var / cwd.
    pub async fn new(workspace_root: PathBuf) -> Result<Self> {
        info!(
            "Initializing Julie server handler (workspace_root: {:?})",
            workspace_root
        );

        Ok(Self {
            workspace_root,
            workspace: Arc::new(RwLock::new(None)),
            is_indexed: Arc::new(RwLock::new(false)),
            indexing_status: Arc::new(IndexingStatus::new()),
            session_metrics: Arc::new(SessionMetrics::new()),
            tool_router: Self::tool_router(),
        })
    }

    /// Test-only convenience: create handler using current_dir() as workspace root.
    ///
    /// Tests that explicitly call `initialize_workspace_with_force(Some(path), ...)`
    /// override the workspace root anyway, so this is safe for existing test patterns.
    #[cfg(test)]
    pub async fn new_for_test() -> Result<Self> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new(cwd).await
    }

    /// Get the workspace root path for workspace operations.
    ///
    /// Returns the resolved workspace root that was passed to `new()`.
    /// This replaces the old `current_dir()` fallback, ensuring the handler
    /// always uses the path determined by main.rs (CLI > env var > cwd).
    fn get_workspace_path(&self) -> PathBuf {
        self.workspace_root.clone()
    }

    /// Initialize or load workspace and update components to use persistent storage
    pub async fn initialize_workspace(&self, workspace_path: Option<String>) -> Result<()> {
        self.initialize_workspace_with_force(workspace_path, false)
            .await
    }

    /// Initialize or load workspace with optional force reinitialization
    pub async fn initialize_workspace_with_force(
        &self,
        workspace_path: Option<String>,
        force: bool,
    ) -> Result<()> {
        debug!(
            "🔍 DEBUG: initialize_workspace_with_force called with workspace_path: {:?}, force: {}",
            workspace_path, force
        );
        let target_path = match workspace_path {
            Some(path) => {
                let expanded_path = shellexpand::tilde(&path).to_string();
                std::path::PathBuf::from(expanded_path)
            }
            None => self.get_workspace_path(),
        };

        info!("Initializing workspace at: {}", target_path.display());
        debug!(
            "🔍 DEBUG: target_path resolved to: {}",
            target_path.display()
        );

        // Handle force reinitialization vs normal initialization
        let mut workspace = if force {
            info!("🔄 Force reinitialization requested - clearing derived data only");

            // Teardown old workspace: stop watcher tasks, shut down search index to
            // release the Tantivy file lock. Without this, the new SearchIndex will
            // get LockBusy errors because the old writer is still held by background tasks.
            {
                let mut workspace_guard = self.workspace.write().await;
                if let Some(ref mut old_workspace) = *workspace_guard {
                    info!("Tearing down old workspace before force re-index");

                    // 1. Stop file watcher (signals spawned tasks to exit)
                    if let Err(e) = old_workspace.stop_file_watching().await {
                        warn!("Failed to stop file watching during teardown: {}", e);
                    }

                    // 2. Shut down search index (commits + releases Tantivy file lock)
                    if let Some(ref search_index) = old_workspace.search_index {
                        match search_index.lock() {
                            Ok(idx) => {
                                if let Err(e) = idx.shutdown() {
                                    warn!("Failed to shut down search index: {}", e);
                                } else {
                                    info!("Old search index shut down, file lock released");
                                }
                            }
                            Err(poisoned) => {
                                // Recover from poisoned mutex — we still need to release the lock
                                let idx = poisoned.into_inner();
                                let _ = idx.shutdown();
                                warn!("Recovered from poisoned search index mutex during teardown");
                            }
                        }
                    }
                }
                // Drop the old workspace reference
                *workspace_guard = None;
            }

            // For force reindex, we only clear derived data, NOT the database (source of truth)
            let julie_dir = target_path.join(".julie");
            if julie_dir.exists() {
                info!("🗑️ Clearing search index and cache for force reindex (preserving database)");

                // 🔴 CRITICAL FIX: Only clear the PRIMARY workspace's index, NOT all workspaces!
                // Reference workspaces must be preserved during force reindex

                // Determine the primary workspace ID so we only clear its directory
                use crate::workspace::registry::generate_workspace_id;
                let workspace_path_str = target_path.to_string_lossy().to_string();

                let primary_workspace_index_dir = match generate_workspace_id(&workspace_path_str) {
                    Ok(workspace_id) => {
                        // Successfully got workspace ID - construct path to primary workspace's index
                        let workspace_name = target_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("workspace");
                        let full_workspace_id =
                            format!("{}_{}", workspace_name, &workspace_id[..8]);
                        Some(julie_dir.join("indexes").join(full_workspace_id))
                    }
                    Err(e) => {
                        warn!(
                            "Failed to generate workspace ID: {} - will skip index clearing",
                            e
                        );
                        None
                    }
                };

                // Clear primary workspace's index directory (NOT the entire indexes/ directory)
                if let Some(primary_index_dir) = primary_workspace_index_dir {
                    if primary_index_dir.exists() {
                        if let Err(e) = std::fs::remove_dir_all(&primary_index_dir) {
                            warn!(
                                "Failed to clear primary workspace index {}: {}",
                                primary_index_dir.display(),
                                e
                            );
                        } else {
                            info!(
                                "✅ Cleared primary workspace index: {}",
                                primary_index_dir.display()
                            );
                            info!(
                                "✅ Reference workspaces preserved (workspace isolation maintained)"
                            );
                        }
                    }
                }

                // Clear shared cache (applies to all workspaces, can be rebuilt)
                let cache_path = julie_dir.join("cache");
                if cache_path.exists() {
                    if let Err(e) = std::fs::remove_dir_all(&cache_path) {
                        warn!("Failed to clear cache {}: {}", cache_path.display(), e);
                    } else {
                        info!("Cleared shared cache: {}", cache_path.display());
                    }
                }

                // Database directory is explicitly preserved for incremental updates
                let db_path = julie_dir.join("db");
                if db_path.exists() {
                    info!(
                        "✅ Database preserved at: {} (contains source of truth)",
                        db_path.display()
                    );
                }
            }

            // Initialize workspace (will reuse existing database if present)
            JulieWorkspace::initialize(target_path).await?
        } else {
            // Try to load existing workspace first
            match JulieWorkspace::detect_and_load(target_path.clone()).await? {
                Some(existing_workspace) => {
                    info!("Loaded existing workspace");
                    existing_workspace
                }
                None => {
                    info!("Creating new workspace");
                    JulieWorkspace::initialize(target_path).await?
                }
            }
        };

        // Start file watching BEFORE storing workspace (to avoid clone issue)
        if let Err(e) = workspace.start_file_watching().await {
            warn!("Failed to start file watching: {}", e);
        }

        // Store the initialized workspace
        {
            let mut workspace_guard = self.workspace.write().await;
            *workspace_guard = Some(workspace);
        }

        info!("Workspace initialization complete");
        Ok(())
    }

    /// Get workspace if initialized
    pub async fn get_workspace(&self) -> Result<Option<JulieWorkspace>> {
        let workspace_guard = self.workspace.read().await;
        Ok(workspace_guard.clone())
    }

    /// Ensure workspace is initialized for operations that require it
    pub async fn ensure_workspace(&self) -> Result<()> {
        let workspace_guard = self.workspace.read().await;
        if workspace_guard.is_none() {
            drop(workspace_guard);
            self.initialize_workspace(None).await?;
        }
        Ok(())
    }

    /// Record a completed tool call. Bumps in-memory atomics synchronously,
    /// then spawns async task for source_bytes lookup + SQLite write.
    pub(crate) fn record_tool_call(
        &self,
        tool_name: &str,
        duration: std::time::Duration,
        report: &ToolCallReport,
    ) {
        let duration_us = duration.as_micros() as u64;
        let output_bytes = report.output_bytes;

        // Bump in-memory atomics synchronously (source_bytes=0 for now, updated async)
        if let Some(kind) = ToolKind::from_name(tool_name) {
            self.session_metrics
                .record(kind, duration_us, 0, output_bytes);
        }

        // Async: look up source file sizes + write to SQLite (fire-and-forget)
        let workspace = self.workspace.clone();
        let session_metrics = self.session_metrics.clone();
        let session_id = self.session_metrics.session_id.clone();
        let tool_name = tool_name.to_string();
        let duration_ms = duration.as_secs_f64() * 1000.0;
        let result_count = report.result_count;
        let source_file_paths = report.source_file_paths.clone();
        let metadata = report.metadata.to_string();
        let metadata_str = if metadata == "null" {
            None
        } else {
            Some(metadata)
        };

        tokio::spawn(async move {
            let guard = workspace.read().await;
            if let Some(ws) = guard.as_ref() {
                if let Some(db_arc) = &ws.db {
                    if let Ok(db) = db_arc.lock() {
                        // Look up source file sizes from the index
                        let source_bytes = if !source_file_paths.is_empty() {
                            let path_refs: Vec<&str> =
                                source_file_paths.iter().map(|s| s.as_str()).collect();
                            db.get_total_file_sizes(&path_refs).ok()
                        } else {
                            None
                        };

                        // Bump source_bytes atomics (deferred from synchronous path)
                        if let Some(sb) = source_bytes {
                            session_metrics.total_source_bytes.fetch_add(
                                sb,
                                std::sync::atomic::Ordering::Relaxed,
                            );
                        }

                        let _ = db.insert_tool_call(
                            &session_id,
                            &tool_name,
                            duration_ms,
                            result_count,
                            source_bytes,
                            Some(output_bytes),
                            true,
                            metadata_str.as_deref(),
                        );
                    }
                }
            }
        });
    }

    /// Extract output byte count from a CallToolResult.
    fn output_bytes_from_result(result: &CallToolResult) -> u64 {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .map(|t| t.text.len() as u64)
            .sum()
    }

    /// Extract file paths from a CallToolResult's text content.
    fn extract_paths_from_result(result: &CallToolResult) -> Vec<String> {
        let text: String = result
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        extract_source_paths(&text)
    }

    /// Run auto-indexing in background (called after MCP handshake)
    async fn run_auto_indexing(&self) {
        use crate::startup::check_if_indexing_needed;

        info!("🔍 Starting background auto-indexing check...");

        // Check if indexing is needed
        match check_if_indexing_needed(self).await {
            Ok(true) => {
                info!("📚 Workspace needs indexing - starting auto-indexing...");

                let index_tool = ManageWorkspaceTool {
                    operation: "index".to_string(),
                    path: None, // Use default workspace path
                    name: None,
                    workspace_id: None,
                    force: Some(false),
                    detailed: None,
                };

                if let Err(e) = index_tool.call_tool_with_options(self, false).await {
                    warn!(
                        "⚠️ Background auto-indexing failed: {} (use manage_workspace tool to retry)",
                        e
                    );
                } else {
                    info!("✅ Background auto-indexing completed successfully");
                }
            }
            Ok(false) => {
                info!("✅ Workspace already indexed - skipping auto-indexing");
            }
            Err(e) => {
                warn!("⚠️ Failed to check indexing status: {}", e);
            }
        }
    }

    // ========== Workspace Access Helpers ==========

    /// Get the database for a specific workspace by ID.
    ///
    /// Opens the reference workspace's SQLite database from
    /// the primary workspace's `.julie/indexes/{workspace_id}/db/symbols.db`.
    pub async fn get_database_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Arc<std::sync::Mutex<SymbolDatabase>>> {
        let primary = self
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("Primary workspace not initialized"))?;

        let db_path = primary.workspace_db_path(workspace_id);
        if !db_path.exists() {
            return Err(anyhow::anyhow!(
                "Database not found for workspace '{}' at {}",
                workspace_id,
                db_path.display()
            ));
        }

        tokio::task::spawn_blocking(move || {
            let db = SymbolDatabase::new(&db_path)?;
            Ok(Arc::new(std::sync::Mutex::new(db)))
        })
        .await?
    }

    /// Get the search index for a specific workspace by ID.
    ///
    /// Opens the reference workspace's Tantivy index from
    /// the primary workspace's `.julie/indexes/{workspace_id}/tantivy/`.
    /// Returns `Ok(None)` if the index directory doesn't exist yet.
    pub async fn get_search_index_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Option<Arc<std::sync::Mutex<SearchIndex>>>> {
        let primary = self
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("Primary workspace not initialized"))?;

        let tantivy_path = primary.workspace_tantivy_path(workspace_id);
        if !tantivy_path.join("meta.json").exists() {
            return Ok(None);
        }

        tokio::task::spawn_blocking(move || {
            let configs = crate::search::LanguageConfigs::load_embedded();
            let index = SearchIndex::open_with_language_configs(&tantivy_path, &configs)?;
            Ok(Some(Arc::new(std::sync::Mutex::new(index))))
        })
        .await?
    }

    /// Get the root path on disk for a specific workspace by ID.
    ///
    /// Looks up the workspace entry in the primary workspace's
    /// registry and returns `WorkspaceEntry.original_path`.
    pub async fn get_workspace_root_for_target(&self, workspace_id: &str) -> Result<PathBuf> {
        let primary = self
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("Primary workspace not initialized"))?;

        let registry_service =
            crate::workspace::registry_service::WorkspaceRegistryService::new(primary.root.clone());
        let entry = registry_service
            .get_workspace(workspace_id)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Workspace '{}' not found in workspace registry",
                    workspace_id
                )
            })?;
        Ok(PathBuf::from(entry.original_path))
    }

    /// Returns the agent instructions embedded at compile time.
    ///
    /// `JULIE_AGENT_INSTRUCTIONS.md` is product metadata that ships with Julie,
    /// not something found in user workspaces. Embedding via `include_str!`
    /// guarantees instructions are always available regardless of deployment.
    fn load_agent_instructions(&self) -> Option<String> {
        Some(include_str!("../JULIE_AGENT_INSTRUCTIONS.md").to_string())
    }
}

/// Tool router implementation - defines all available tools
#[tool_router]
impl JulieServerHandler {
    pub fn new_router() -> Self {
        // This is used by rmcp to create the tool router
        // We need to provide a way to construct with proper state
        panic!("Use JulieServerHandler::new(workspace_root) instead")
    }

    // ========== Search & Navigation Tools ==========

    #[tool(
        name = "fast_search",
        description = "Search code using text search with code-aware tokenization. Supports multi-word queries with AND/OR logic.",
        annotations(
            title = "Fast Code Search",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn fast_search(
        &self,
        Parameters(params): Parameters<FastSearchTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("⚡ Fast search: {:?}", params);
        let start = std::time::Instant::now();
        let metadata = serde_json::json!({
            "query": params.query,
            "target": params.search_target,
        });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("fast_search failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call("fast_search", start.elapsed(), &report);
        Ok(result)
    }

    #[tool(
        name = "fast_refs",
        description = "Find all references to a symbol across the codebase.",
        annotations(
            title = "Find References",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn fast_refs(
        &self,
        Parameters(params): Parameters<FastRefsTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("⚡ Fast find references: {:?}", params);
        let start = std::time::Instant::now();
        let metadata = serde_json::json!({ "symbol": params.symbol });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("fast_refs failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call("fast_refs", start.elapsed(), &report);
        Ok(result)
    }

    #[tool(
        name = "get_symbols",
        description = "Get symbols (functions, classes, etc.) from a file without reading full content. Requires exact file path — use deep_dive(symbol=...) if you don't know the path.",
        annotations(
            title = "Get File Symbols",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_symbols(
        &self,
        Parameters(params): Parameters<GetSymbolsTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("📋 Get symbols for file: {:?}", params);
        let start = std::time::Instant::now();
        let metadata = serde_json::json!({
            "file": params.file_path,
            "mode": params.mode,
            "target": params.target,
        });
        let source_file_paths = vec![params.file_path.clone()];
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("get_symbols failed: {}", e), None))?;
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes: Self::output_bytes_from_result(&result),
            metadata,
            source_file_paths,
        };
        self.record_tool_call("get_symbols", start.elapsed(), &report);
        Ok(result)
    }

    #[tool(
        name = "deep_dive",
        description = "Investigate a symbol with progressive depth. Returns definition, references, children, and type info in a single call — tailored to the symbol's kind.\n\n**Always use BEFORE modifying or extending a symbol.** Replaces the common chain of fast_search → get_symbols → fast_refs → Read with a single call.",
        annotations(
            title = "Deep Dive Symbol Investigation",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn deep_dive(
        &self,
        Parameters(params): Parameters<DeepDiveTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("🔍 Deep dive: {:?}", params);
        let start = std::time::Instant::now();
        let metadata = serde_json::json!({
            "symbol": params.symbol,
            "depth": params.depth,
        });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("deep_dive failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call("deep_dive", start.elapsed(), &report);
        Ok(result)
    }

    // ========== Context Tools ==========

    #[tool(
        name = "get_context",
        description = "Get token-budgeted context for a concept or task. Returns relevant code subgraph with pivots (full code) and neighbors (signatures). Use at the start of a task for orientation.",
        annotations(
            title = "Get Context",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_context(
        &self,
        Parameters(params): Parameters<GetContextTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("📦 Get context: {:?}", params);
        let start = std::time::Instant::now();
        let metadata = serde_json::json!({ "query": params.query });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("get_context failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call("get_context", start.elapsed(), &report);
        Ok(result)
    }

    // ========== Refactoring Tools ==========

    #[tool(
        name = "rename_symbol",
        description = "Rename a symbol across the entire codebase with workspace-wide updates.",
        annotations(
            title = "Rename Symbol",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn rename_symbol(
        &self,
        Parameters(params): Parameters<RenameSymbolTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("✏️ Rename symbol: {:?}", params);
        let start = std::time::Instant::now();
        let metadata = serde_json::json!({
            "old": params.old_name,
            "new": params.new_name,
            "dry_run": params.dry_run,
        });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("rename_symbol failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call("rename_symbol", start.elapsed(), &report);
        Ok(result)
    }

    // ========== Workspace Management ==========

    #[tool(
        name = "manage_workspace",
        description = "Manage workspace: index, add/remove reference workspaces, view status.",
        annotations(
            title = "Manage Workspace",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn manage_workspace(
        &self,
        Parameters(params): Parameters<ManageWorkspaceTool>,
    ) -> Result<CallToolResult, McpError> {
        info!("🏗️ Managing workspace: {}", params.operation);
        let start = std::time::Instant::now();
        let metadata = serde_json::json!({ "operation": params.operation });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| {
                McpError::internal_error(format!("manage_workspace failed: {}", e), None)
            })?;
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes: Self::output_bytes_from_result(&result),
            metadata,
            source_file_paths: Vec::new(),
        };
        self.record_tool_call("manage_workspace", start.elapsed(), &report);
        Ok(result)
    }

    // ========== Metrics & Reporting Tools ==========

    #[tool(
        name = "query_metrics",
        description = "Query metrics: code health (security risk, change risk, test coverage, centrality) or operational metrics (session stats, historical performance). Use category parameter: \"code_health\" (default), \"session\", or \"history\".",
        annotations(
            title = "Query Code Metrics",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn query_metrics(
        &self,
        Parameters(params): Parameters<QueryMetricsTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("📊 Query metrics: {:?}", params);
        let start = std::time::Instant::now();
        let metadata = serde_json::json!({ "sort_by": params.sort_by });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("query_metrics failed: {}", e), None))?;
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes: Self::output_bytes_from_result(&result),
            metadata,
            source_file_paths: Vec::new(),
        };
        self.record_tool_call("query_metrics", start.elapsed(), &report);
        Ok(result)
    }
}

/// ServerHandler implementation with tool_handler macro
#[tool_handler]
impl ServerHandler for JulieServerHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "Julie".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: Some("Julie - Code Intelligence Server".into()),
                icons: None,
                website_url: None,
            },
            instructions: self.load_agent_instructions(),
        }
    }

    async fn on_initialized(&self, _context: NotificationContext<RoleServer>) {
        info!("MCP connection established - client initialized");

        // Run auto-indexing in background task
        let handler = self.clone();
        tokio::spawn(async move {
            handler.run_auto_indexing().await;
        });
    }
}
