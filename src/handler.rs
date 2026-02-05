use anyhow::{Context, Result};
use rmcp::{
    ServerHandler, RoleServer,
    model::{CallToolResult, ServerCapabilities, ServerInfo, Implementation},
    service::NotificationContext,
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    tool, tool_router, tool_handler,
    ErrorData as McpError,
};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tracing::{debug, info, warn};

use crate::embeddings::EmbeddingEngine;
use crate::workspace::{JulieWorkspace, WorkspaceConfig};
use tokio::sync::RwLock;

// Import tool parameter types (we'll convert these from the tool modules)
use crate::tools::{
    FastSearchTool, FastGotoTool, FastRefsTool, GetSymbolsTool, TraceCallPathTool,
    FastExploreTool, FindLogicTool, EditLinesTool, FuzzyReplaceTool,
    RenameSymbolTool, EditSymbolTool, CheckpointTool, RecallTool, PlanTool,
    ManageWorkspaceTool,
};

/// Tracks which indexes are ready for search operations
///
/// This enables progressive enhancement and graceful degradation:
/// - SQLite FTS5: Available immediately after indexing (~2s)
/// - Semantic (HNSW): Available after embedding generation (~20-30s)
#[derive(Debug)]
pub struct IndexingStatus {
    /// SQLite FTS5 full-text search is ready
    pub sqlite_fts_ready: AtomicBool,
    /// HNSW semantic search is ready
    pub semantic_ready: AtomicBool,
}

impl IndexingStatus {
    /// Create new indexing status with all indexes not ready
    pub fn new() -> Self {
        Self {
            sqlite_fts_ready: AtomicBool::new(false),
            semantic_ready: AtomicBool::new(false),
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
/// - Semantic search and embeddings
/// - Cross-language relationship detection
#[derive(Clone)]
pub struct JulieServerHandler {
    /// Workspace managing persistent storage
    pub workspace: Arc<RwLock<Option<JulieWorkspace>>>,
    /// Flag to track if workspace has been indexed
    pub is_indexed: Arc<RwLock<bool>>,
    /// Cached embedding engine for semantic search (expensive to initialize)
    pub embedding_engine: Arc<RwLock<Option<EmbeddingEngine>>>,
    /// Timestamp of last embedding engine use (for lazy cleanup)
    pub embedding_engine_last_used: Arc<tokio::sync::Mutex<Option<std::time::Instant>>>,
    /// Tracks which indexes are ready for search operations
    pub indexing_status: Arc<IndexingStatus>,
    /// 🔒 CRITICAL FIX: Serializes tool execution to prevent stdout interleaving
    /// The MCP StdioTransport doesn't synchronize writes to stdout.
    /// When multiple tool calls complete concurrently, their JSON responses can
    /// interleave on stdout, causing client parsing errors.
    /// This mutex ensures only one tool writes its response at a time.
    tool_execution_lock: Arc<tokio::sync::Mutex<()>>,
    /// rmcp tool router for handling tool calls
    tool_router: ToolRouter<Self>,
}

impl JulieServerHandler {
    /// Create a new Julie server handler with all components initialized
    pub async fn new() -> Result<Self> {
        info!("🔧 Initializing Julie server handler");
        debug!("✓ Julie handler initialized - workspace initialization will provide storage");

        Ok(Self {
            workspace: Arc::new(RwLock::new(None)),
            is_indexed: Arc::new(RwLock::new(false)),
            embedding_engine: Arc::new(RwLock::new(None)),
            embedding_engine_last_used: Arc::new(tokio::sync::Mutex::new(None)),
            indexing_status: Arc::new(IndexingStatus::new()),
            tool_execution_lock: Arc::new(tokio::sync::Mutex::new(())),
            tool_router: Self::tool_router(),
        })
    }

    /// Get the current working directory for workspace operations
    fn get_workspace_path(&self) -> std::path::PathBuf {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    }

    /// Get or initialize the cached embedding engine for semantic operations
    /// This avoids expensive repeated initialization of the ONNX model
    /// Ensure vector store is initialized (lazy initialization for semantic search)
    /// 🔥 CRITICAL FIX: Wraps blocking HNSW initialization in spawn_blocking to prevent runtime deadlock
    pub async fn ensure_vector_store(&self) -> Result<()> {
        // Fast path: check with read lock first (avoids blocking concurrent searches)
        {
            let workspace_guard = self.workspace.read().await;
            if let Some(ws) = workspace_guard.as_ref() {
                if ws.vector_store.is_some() {
                    return Ok(()); // Already initialized
                }
            }
        } // Drop read lock before acquiring write lock

        // 🚀 CRITICAL FIX: Extract data with minimal write lock, then release before long operation
        // The old code held write lock for 30-60 seconds, blocking ALL workspace access
        let (root, julie_dir, db) = {
            let mut workspace_guard = self.workspace.write().await;
            if let Some(ref mut ws) = workspace_guard.as_mut() {
                // Double-check: another thread might have initialized while we waited for write lock
                if ws.vector_store.is_some() {
                    return Ok(()); // Another thread finished while we waited
                }

                info!("🔄 Lazy-initializing vector store for semantic search...");

                // Clone what we need, then RELEASE the lock
                (ws.root.clone(), ws.julie_dir.clone(), ws.db.clone())
            } else {
                return Err(anyhow::anyhow!("Workspace not initialized"));
            }
        }; // 🔓 Write lock released here - other operations can proceed!

        // 🚨 CRITICAL FIX: HNSW loading/building is BLOCKING (12MB disk I/O + CPU computation)
        // Must run on blocking thread pool to avoid deadlocking the tokio runtime
        // This operation can take 30-60 seconds but now runs WITHOUT holding workspace lock

        // Run initialization on blocking threadpool (NO LOCK HELD)
        let vector_store = tokio::task::spawn_blocking(move || {
            // Reconstruct minimal workspace for initialization
            let mut temp_ws = JulieWorkspace {
                root,
                julie_dir,
                db,
                embeddings: None,
                vector_store: None,
                search_index: None,
                watcher: None,
                config: WorkspaceConfig::default(),
            };

            temp_ws.initialize_vector_store()?;

            // Extract the initialized vector store
            temp_ws
                .vector_store
                .ok_or_else(|| anyhow::anyhow!("Vector store initialization failed"))
        })
        .await
        .context("Vector store initialization task panicked")??;

        // 🔒 Re-acquire write lock ONLY to store the result (fast operation)
        {
            let mut workspace_guard = self.workspace.write().await;
            if let Some(ref mut ws) = workspace_guard.as_mut() {
                // Check one more time in case another thread beat us
                if ws.vector_store.is_none() {
                    ws.vector_store = Some(vector_store);
                    info!("✅ Vector store initialized on blocking threadpool");
                }
            }
        } // 🔓 Write lock released immediately

        Ok(())
    }

    pub async fn ensure_embedding_engine(&self) -> Result<()> {
        // Fast path: check with read lock first (avoids blocking concurrent searches)
        {
            let embedding_guard = self.embedding_engine.read().await;
            if embedding_guard.is_some() {
                return Ok(()); // Already initialized
            }
        } // Drop read lock before acquiring write lock

        // Slow path: acquire write lock only if initialization needed
        let mut embedding_guard = self.embedding_engine.write().await;

        // Double-check: another thread might have initialized while we waited
        if embedding_guard.is_none() {
            debug!("🧠 Initializing cached embedding engine");

            // Get database from workspace
            let workspace_guard = self.workspace.read().await;
            let workspace = workspace_guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Workspace not initialized"))?;

            let db = workspace
                .db
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Database not initialized"))?
                .clone();

            // Use workspace's persistent embedding cache (.julie/cache/embeddings/)
            let cache_dir = workspace.ensure_embedding_cache_dir()?;

            let engine = EmbeddingEngine::new("bge-small", cache_dir, db)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialize embedding engine: {}", e))?;

            *embedding_guard = Some(engine);
            info!("✅ Cached embedding engine initialized successfully");
        }

        Ok(())
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
            // 🔥 CRITICAL FIX: Now awaited due to async ONNX initialization
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

    /// Start the periodic embedding engine cleanup task
    /// This task checks every minute if the engine has been idle for >5 minutes and drops it to free memory
    pub fn start_embedding_cleanup_task(&self) {
        let engine = self.embedding_engine.clone();
        let last_used = self.embedding_engine_last_used.clone();

        tokio::spawn(async move {
            const CHECK_INTERVAL_SECS: u64 = 60; // Check every minute
            const IDLE_TIMEOUT_SECS: u64 = 300; // Drop after 5 minutes of inactivity

            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(CHECK_INTERVAL_SECS)).await;

                // Check if engine exists and is idle
                let should_drop = {
                    let last_used_guard = last_used.lock().await;
                    if let Some(last_use_time) = *last_used_guard {
                        let idle_duration = last_use_time.elapsed().as_secs();
                        idle_duration > IDLE_TIMEOUT_SECS
                    } else {
                        false // Never used, don't drop
                    }
                };

                if should_drop {
                    // Check if engine actually exists before trying to drop
                    let engine_exists = {
                        let engine_guard = engine.read().await;
                        engine_guard.is_some()
                    };

                    if engine_exists {
                        // Drop the engine to release memory
                        let mut engine_guard = engine.write().await;
                        *engine_guard = None;
                        info!(
                            "🧹 Dropped embedding engine after 5 minutes of inactivity - ONNX model memory released"
                        );

                        // Reset last_used timestamp
                        let mut last_used_guard = last_used.lock().await;
                        *last_used_guard = None;
                    }
                }
            }
        });

        info!(
            "🕐 Started periodic embedding engine cleanup task (checks every 60s, drops after 5min idle)"
        );
    }

    /// Check if the tool execution lock is currently free (used in tests)
    #[allow(dead_code)]
    pub(crate) fn tool_lock_is_free(&self) -> bool {
        match self.tool_execution_lock.try_lock() {
            Ok(guard) => {
                drop(guard);
                true
            }
            Err(_) => false,
        }
    }

    /// Run auto-indexing in background (called after MCP handshake)
    async fn run_auto_indexing(&self) {
        use crate::startup::check_if_indexing_needed;

        info!("🔍 Starting background auto-indexing check...");

        // Check if indexing is needed
        match check_if_indexing_needed(self).await {
            Ok(true) => {
                info!("📚 Workspace needs indexing - starting auto-indexing...");

                // Run indexing via manage_workspace tool
                let index_tool = ManageWorkspaceTool {
                    operation: "index".to_string(),
                    path: None, // Use default workspace path
                    name: None,
                    workspace_id: None,
                    force: Some(false),
                    detailed: None,
                };

                if let Err(e) = index_tool.call_tool(self).await {
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
}

// Load agent instructions for server info
fn load_agent_instructions() -> Option<String> {
    match std::fs::read_to_string("JULIE_AGENT_INSTRUCTIONS.md") {
        Ok(content) => Some(content),
        Err(_) => None,
    }
}

/// Tool router implementation - defines all available tools
#[tool_router]
impl JulieServerHandler {
    pub fn new_router() -> Self {
        // This is used by rmcp to create the tool router
        // We need to provide a way to construct with proper state
        panic!("Use JulieServerHandler::new() instead")
    }

    // ========== Search & Navigation Tools ==========

    #[tool(
        name = "fast_search",
        description = "Search code using text or semantic search. Supports multi-word queries with AND/OR logic.",
        annotations(
            title = "Fast Code Search",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn fast_search(&self, Parameters(params): Parameters<FastSearchTool>) -> Result<CallToolResult, McpError> {
        debug!("⚡ Fast search: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("fast_search failed: {}", e), None)
        })
    }

    #[tool(
        name = "fast_goto",
        description = "Navigate to symbol definition. Finds where a symbol is defined in the codebase.",
        annotations(
            title = "Go to Definition",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn fast_goto(&self, Parameters(params): Parameters<FastGotoTool>) -> Result<CallToolResult, McpError> {
        debug!("⚡ Fast goto definition: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("fast_goto failed: {}", e), None)
        })
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
    async fn fast_refs(&self, Parameters(params): Parameters<FastRefsTool>) -> Result<CallToolResult, McpError> {
        debug!("⚡ Fast find references: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("fast_refs failed: {}", e), None)
        })
    }

    #[tool(
        name = "get_symbols",
        description = "Get symbols (functions, classes, etc.) from a file without reading full content.",
        annotations(
            title = "Get File Symbols",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_symbols(&self, Parameters(params): Parameters<GetSymbolsTool>) -> Result<CallToolResult, McpError> {
        debug!("📋 Get symbols for file: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("get_symbols failed: {}", e), None)
        })
    }

    #[tool(
        name = "trace_call_path",
        description = "Trace execution flow between symbols across languages. Unique cross-language analysis.",
        annotations(
            title = "Trace Call Path",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn trace_call_path(&self, Parameters(params): Parameters<TraceCallPathTool>) -> Result<CallToolResult, McpError> {
        debug!("🔍 Trace call path: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("trace_call_path failed: {}", e), None)
        })
    }

    // ========== Exploration Tools ==========

    #[tool(
        name = "fast_explore",
        description = "Multi-mode exploration: find business logic, similar code, tests, or dependencies.",
        annotations(
            title = "Fast Explore",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn fast_explore(&self, Parameters(params): Parameters<FastExploreTool>) -> Result<CallToolResult, McpError> {
        debug!("🔍 Fast explore (mode={:?}): {:?}", params.mode, params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("fast_explore failed: {}", e), None)
        })
    }

    #[tool(
        name = "find_logic",
        description = "Find business logic implementation. Deprecated - use fast_explore with mode='logic'.",
        annotations(
            title = "Find Business Logic",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn find_logic(&self, Parameters(params): Parameters<FindLogicTool>) -> Result<CallToolResult, McpError> {
        debug!("🏢 Find business logic: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("find_logic failed: {}", e), None)
        })
    }

    // ========== Editing Tools ==========

    #[tool(
        name = "edit_lines",
        description = "Surgical line editing: insert, replace, or delete specific lines in a file.",
        annotations(
            title = "Edit Lines",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn edit_lines(&self, Parameters(params): Parameters<EditLinesTool>) -> Result<CallToolResult, McpError> {
        debug!("✂️ Surgical line edit: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("edit_lines failed: {}", e), None)
        })
    }

    #[tool(
        name = "fuzzy_replace",
        description = "Fuzzy search and replace using diff-match-patch algorithm. Tolerant of whitespace changes.",
        annotations(
            title = "Fuzzy Replace",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn fuzzy_replace(&self, Parameters(params): Parameters<FuzzyReplaceTool>) -> Result<CallToolResult, McpError> {
        debug!("🔍 Fuzzy replace: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("fuzzy_replace failed: {}", e), None)
        })
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
    async fn rename_symbol(&self, Parameters(params): Parameters<RenameSymbolTool>) -> Result<CallToolResult, McpError> {
        debug!("✏️ Rename symbol: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("rename_symbol failed: {}", e), None)
        })
    }

    #[tool(
        name = "edit_symbol",
        description = "Edit a symbol's body (function, class, etc.) with fuzzy matching.",
        annotations(
            title = "Edit Symbol",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn edit_symbol(&self, Parameters(params): Parameters<EditSymbolTool>) -> Result<CallToolResult, McpError> {
        debug!("✂️ Edit symbol: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("edit_symbol failed: {}", e), None)
        })
    }

    // ========== Memory Tools ==========

    #[tool(
        name = "checkpoint",
        description = "Save development memory checkpoint to .memories/ directory.",
        annotations(
            title = "Save Memory Checkpoint",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn checkpoint(&self, Parameters(params): Parameters<CheckpointTool>) -> Result<CallToolResult, McpError> {
        debug!("💾 Checkpoint: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("checkpoint failed: {}", e), None)
        })
    }

    #[tool(
        name = "recall",
        description = "Retrieve development memories using semantic search.",
        annotations(
            title = "Recall Memories",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn recall(&self, Parameters(params): Parameters<RecallTool>) -> Result<CallToolResult, McpError> {
        debug!("🔍 Recall: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("recall failed: {}", e), None)
        })
    }

    #[tool(
        name = "plan",
        description = "Manage working plans with atomic updates. One active plan at a time.",
        annotations(
            title = "Manage Plans",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn plan(&self, Parameters(params): Parameters<PlanTool>) -> Result<CallToolResult, McpError> {
        debug!("📋 Plan: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("plan failed: {}", e), None)
        })
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
    async fn manage_workspace(&self, Parameters(params): Parameters<ManageWorkspaceTool>) -> Result<CallToolResult, McpError> {
        info!("🏗️ Managing workspace: {}", params.operation);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("manage_workspace failed: {}", e), None)
        })
    }
}

/// ServerHandler implementation with tool_handler macro
#[tool_handler]
impl ServerHandler for JulieServerHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "Julie".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: Some("Julie - Code Intelligence Server".into()),
                icons: None,
                website_url: None,
            },
            instructions: load_agent_instructions(),
        }
    }

    async fn on_initialized(&self, _context: NotificationContext<RoleServer>) {
        info!("🔗 MCP connection established - client initialized");

        // Run auto-indexing in background task after handshake completes
        let handler = self.clone();
        tokio::spawn(async move {
            handler.run_auto_indexing().await;
        });
    }
}
