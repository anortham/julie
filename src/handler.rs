use anyhow::{Context, Result};
use async_trait::async_trait;
use rust_mcp_sdk::schema::{
    schema_utils::CallToolError, CallToolRequest, CallToolResult, ListToolsRequest,
    ListToolsResult, RpcError,
};
use rust_mcp_sdk::{mcp_server::ServerHandler, McpServer};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::embeddings::EmbeddingEngine;
use crate::tools::JulieTools;
use crate::workspace::{JulieWorkspace, WorkspaceConfig};
use tokio::sync::RwLock;

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
pub struct JulieServerHandler {
    /// Workspace managing persistent storage
    pub workspace: Arc<RwLock<Option<JulieWorkspace>>>,
    /// Flag to track if workspace has been indexed
    pub is_indexed: Arc<RwLock<bool>>,
    /// Cached embedding engine for semantic search (expensive to initialize)
    pub embedding_engine: Arc<RwLock<Option<EmbeddingEngine>>>,
    /// Tracks which indexes are ready for search operations
    pub indexing_status: Arc<IndexingStatus>,
    /// 🔒 CRITICAL FIX: Serializes tool execution to prevent stdout interleaving
    /// The rust-mcp-sdk's StdioTransport doesn't synchronize writes to stdout.
    /// When multiple tool calls complete concurrently, their JSON responses can
    /// interleave on stdout, causing client parsing errors.
    /// This mutex ensures only one tool writes its response at a time.
    tool_execution_lock: Arc<tokio::sync::Mutex<()>>,
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
            indexing_status: Arc::new(IndexingStatus::new()),
            tool_execution_lock: Arc::new(tokio::sync::Mutex::new(())),
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

        // Slow path: acquire write lock only if initialization needed
        let mut workspace_guard = self.workspace.write().await;
        if let Some(ref mut ws) = workspace_guard.as_mut() {
            // Double-check: another thread might have initialized while we waited for write lock
            if ws.vector_store.is_none() {
                info!("🔄 Lazy-initializing vector store for semantic search...");

                // 🚨 CRITICAL FIX: HNSW loading/building is BLOCKING (12MB disk I/O + CPU computation)
                // Must run on blocking thread pool to avoid deadlocking the tokio runtime
                // This operation can take 30-60 seconds and was causing semantic search to hang

                // Clone what we need for the blocking context
                let root = ws.root.clone();
                let julie_dir = ws.julie_dir.clone();
                let db = ws.db.clone();

                // Run initialization on blocking threadpool
                let vector_store = tokio::task::spawn_blocking(move || {
                    // Reconstruct minimal workspace for initialization
                    let mut temp_ws = JulieWorkspace {
                        root,
                        julie_dir,
                        db,
                        embeddings: None,
                        vector_store: None,
                        watcher: None,
                        config: WorkspaceConfig::default(),
                    };

                    temp_ws.initialize_vector_store()?;

                    // Extract the initialized vector store
                    temp_ws.vector_store
                        .ok_or_else(|| anyhow::anyhow!("Vector store initialization failed"))
                })
                .await
                .context("Vector store initialization task panicked")??;

                ws.vector_store = Some(vector_store);
                info!("✅ Vector store initialized on blocking threadpool");
            }
        }
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

            // Create model cache directory
            let cache_dir = std::env::temp_dir().join("julie_cache").join("embeddings");
            std::fs::create_dir_all(&cache_dir).map_err(|e| {
                anyhow::anyhow!("Failed to create embedding cache directory: {}", e)
            })?;

            let engine = EmbeddingEngine::new("bge-small", cache_dir, db)
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
        let target_path = match workspace_path {
            Some(path) => {
                let expanded_path = shellexpand::tilde(&path).to_string();
                std::path::PathBuf::from(expanded_path)
            }
            None => self.get_workspace_path(),
        };

        info!("Initializing workspace at: {}", target_path.display());

        // Handle force reinitialization vs normal initialization
        let mut workspace = if force {
            info!("🔄 Force reinitialization requested - clearing derived data only");

            // For force reindex, we only clear derived data, NOT the database (source of truth)
            let julie_dir = target_path.join(".julie");
            if julie_dir.exists() {
                info!("🗑️ Clearing search index and cache for force reindex (preserving database)");

                // CRITICAL: Only clear derived data, NEVER the database!
                // Database is source of truth and should be preserved for incremental updates
                // With per-workspace structure, clear the entire indexes/ directory (all workspaces)
                let paths_to_clear = [
                    julie_dir.join("indexes"), // All per-workspace indexes (can be rebuilt)
                    julie_dir.join("cache"),   // Parse cache (can be rebuilt)
                ];

                for path in &paths_to_clear {
                    if path.exists() {
                        if let Err(e) = std::fs::remove_dir_all(path) {
                            warn!("Failed to clear derived data {}: {}", path.display(), e);
                        } else {
                            info!("Cleared derived data: {}", path.display());
                        }
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
}

#[async_trait]
impl ServerHandler for JulieServerHandler {
    /// Handle ListToolsRequest - return all available Julie tools
    async fn handle_list_tools_request(
        &self,
        _request: ListToolsRequest,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        debug!("📋 Listing available Julie tools");

        let tools = JulieTools::tools();
        info!("📊 Returning {} available tools", tools.len());

        Ok(ListToolsResult {
            meta: None,
            next_cursor: None,
            tools,
        })
    }

    /// Handle CallToolRequest - execute the requested Julie tool
    async fn handle_call_tool_request(
        &self,
        request: CallToolRequest,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        debug!("🛠️  Executing tool: {}", request.params.name);

        // Convert request parameters to JulieTools enum
        let tool_params: JulieTools = JulieTools::try_from(request.params).map_err(|e| {
            error!("❌ Failed to parse tool parameters: {}", e);
            CallToolError::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid tool parameters: {}", e),
            ))
        })?;

        // 🔒 CRITICAL FIX: Acquire lock to serialize tool execution and prevent stdout interleaving
        // This ensures only one tool writes its JSON response at a time, preventing corruption
        // when the rust-mcp-sdk's StdioTransport writes multiple responses concurrently.
        let _execution_guard = self.tool_execution_lock.lock().await;

        // Execute the requested tool
        let result = match &tool_params {
            JulieTools::ManageWorkspaceTool(tool) => {
                info!("🏗️ Managing workspace: {}", tool.operation);
                tool.call_tool(self).await
            }
            // Consolidated fast tools with appealing names
            JulieTools::FastSearchTool(tool) => {
                debug!("⚡ Fast search: {:?}", tool);
                tool.call_tool(self).await
            }
            JulieTools::FastGotoTool(tool) => {
                debug!("⚡ Fast goto definition: {:?}", tool);
                tool.call_tool(self).await
            }
            JulieTools::FastRefsTool(tool) => {
                debug!("⚡ Fast find references: {:?}", tool);
                tool.call_tool(self).await
            }
            JulieTools::GetSymbolsTool(tool) => {
                debug!("📋 Get symbols for file: {:?}", tool);
                tool.call_tool(self).await
            }
            JulieTools::TraceCallPathTool(tool) => {
                debug!("🔍 Trace call path: {:?}", tool);
                tool.call_tool(self).await
            }
            JulieTools::EditLinesTool(tool) => {
                debug!("✂️  Surgical line edit: {:?}", tool);
                tool.call_tool(self).await
            }
            JulieTools::FuzzyReplaceTool(tool) => {
                debug!("🔍 Fuzzy replace: {:?}", tool);
                tool.call_tool(self).await
            }
            JulieTools::FastExploreTool(tool) => {
                debug!("⚡ Fast explore codebase: {:?}", tool);
                tool.call_tool(self).await
            }
            JulieTools::FindLogicTool(tool) => {
                debug!("🏢 Find business logic: {:?}", tool);
                tool.call_tool(self).await
            }
            JulieTools::SmartRefactorTool(tool) => {
                debug!("🔄 Smart refactor: {:?}", tool);
                tool.call_tool(self).await
            }
        };

        match result {
            Ok(call_result) => {
                info!("✅ Tool executed successfully");
                Ok(call_result)
            }
            Err(e) => {
                error!("❌ Tool execution failed: {}", e);
                Err(CallToolError::new(std::io::Error::other(format!(
                    "Tool execution failed: {}",
                    e
                ))))
            }
        }
    }
}
