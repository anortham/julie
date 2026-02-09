use anyhow::Result;
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

use crate::workspace::JulieWorkspace;
use tokio::sync::RwLock;

// Import tool parameter types
use crate::tools::{
    FastSearchTool, FastRefsTool, GetSymbolsTool, DeepDiveTool,
    RenameSymbolTool, CheckpointTool, RecallTool, PlanTool,
    ManageWorkspaceTool,
};

/// Tracks which indexes are ready for search operations
#[derive(Debug)]
pub struct IndexingStatus {
    /// Search system (Tantivy) is ready
    pub search_ready: AtomicBool,
}

impl IndexingStatus {
    /// Create new indexing status with all indexes not ready
    pub fn new() -> Self {
        Self {
            search_ready: AtomicBool::new(false),
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
    /// Workspace managing persistent storage
    pub workspace: Arc<RwLock<Option<JulieWorkspace>>>,
    /// Flag to track if workspace has been indexed
    pub is_indexed: Arc<RwLock<bool>>,
    /// Tracks which indexes are ready for search operations
    pub indexing_status: Arc<IndexingStatus>,
    /// Serializes tool execution to prevent stdout interleaving.
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
            indexing_status: Arc::new(IndexingStatus::new()),
            tool_execution_lock: Arc::new(tokio::sync::Mutex::new(())),
            tool_router: Self::tool_router(),
        })
    }

    /// Get the current working directory for workspace operations
    fn get_workspace_path(&self) -> std::path::PathBuf {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
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
        description = "Search code using text search with code-aware tokenization. Supports multi-word queries with AND/OR logic.",
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
        name = "deep_dive",
        description = "Understand a symbol before modifying it. Returns definition, callers, callees, children, and type info in one call — replaces chaining fast_search → get_symbols → fast_refs → Read.",
        annotations(
            title = "Deep Dive Symbol Investigation",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn deep_dive(&self, Parameters(params): Parameters<DeepDiveTool>) -> Result<CallToolResult, McpError> {
        debug!("🔍 Deep dive: {:?}", params);
        let _guard = self.tool_execution_lock.lock().await;
        params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("deep_dive failed: {}", e), None)
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

    // ========== Memory Tools ==========

    #[tool(
        name = "checkpoint",
        description = "Save development memory checkpoint to .memories/ directory. Captures git context (branch, commit, changed files) automatically. Supports types: checkpoint, decision, learning, observation.",
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
        description = "Retrieve development memories using text search with code-aware tokenization.",
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
