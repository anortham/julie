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

use crate::daemon_state::DaemonState;
use crate::workspace::JulieWorkspace;
use tokio::sync::RwLock;

// Import tool parameter types
use crate::tools::{
    CheckpointTool, DeepDiveTool, FastRefsTool, FastSearchTool, GetContextTool, GetSymbolsTool,
    ManageWorkspaceTool, PlanTool, RecallTool, RenameSymbolTool,
};

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
    /// rmcp tool router for handling tool calls
    tool_router: ToolRouter<Self>,
    /// Daemon-wide state for cross-project operations.
    ///
    /// `Some` in daemon mode (HTTP server) — gives tools access to all loaded
    /// workspaces for federated search (`workspace="all"`).
    /// `None` in stdio mode — single-workspace, no federation.
    pub(crate) daemon_state: Option<Arc<RwLock<DaemonState>>>,
}

impl JulieServerHandler {
    /// Create a new Julie server handler with all components initialized.
    ///
    /// `workspace_root` is the resolved root path for this server session,
    /// determined by the caller (main.rs) via CLI args / env var / cwd.
    pub async fn new(workspace_root: PathBuf) -> Result<Self> {
        Self::new_sync(workspace_root)
    }

    /// Synchronous constructor for use in contexts where async is not available
    /// (e.g., the MCP Streamable HTTP service factory closure).
    ///
    /// This is the actual implementation — `new()` delegates to it. The handler
    /// construction is inherently synchronous (just creating Arcs and empty state).
    pub fn new_sync(workspace_root: PathBuf) -> Result<Self> {
        info!("🔧 Initializing Julie server handler (workspace_root: {:?})", workspace_root);
        debug!("✓ Julie handler initialized - workspace initialization will provide storage");

        Ok(Self {
            workspace_root,
            workspace: Arc::new(RwLock::new(None)),
            is_indexed: Arc::new(RwLock::new(false)),
            indexing_status: Arc::new(IndexingStatus::new()),
            tool_router: Self::tool_router(),
            daemon_state: None,
        })
    }

    /// Synchronous constructor for daemon mode — injects shared `DaemonState`.
    ///
    /// Used by `create_workspace_mcp_service` so that tool handlers can access
    /// all loaded workspaces for federated search (`workspace="all"`).
    pub fn new_with_daemon_state(
        workspace_root: PathBuf,
        daemon_state: Arc<RwLock<DaemonState>>,
    ) -> Result<Self> {
        info!("🔧 Initializing Julie server handler with daemon state (workspace_root: {:?})", workspace_root);
        debug!("✓ Julie handler initialized with daemon state — federation enabled");

        Ok(Self {
            workspace_root,
            workspace: Arc::new(RwLock::new(None)),
            is_indexed: Arc::new(RwLock::new(false)),
            indexing_status: Arc::new(IndexingStatus::new()),
            tool_router: Self::tool_router(),
            daemon_state: Some(daemon_state),
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

    /// Run auto-indexing in background (called after MCP handshake)
    async fn run_auto_indexing(&self) {
        use crate::startup::check_if_indexing_needed;

        info!("🔍 Starting background auto-indexing check...");

        // Check if indexing is needed
        match check_if_indexing_needed(self).await {
            Ok(true) => {
                info!("📚 Workspace needs indexing - starting auto-indexing...");

                // Run indexing via manage_workspace tool, skipping embeddings.
                // Embeddings are expensive and network-dependent — they can be
                // triggered explicitly via `manage_workspace index` or lazily on
                // first NL-definition search.
                let index_tool = ManageWorkspaceTool {
                    operation: "index".to_string(),
                    path: None, // Use default workspace path
                    name: None,
                    workspace_id: None,
                    force: Some(false),
                    detailed: None,
                };

                if let Err(e) = index_tool.call_tool_with_options(self, true).await {
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
        params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("fast_search failed: {}", e), None))
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
        params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("fast_refs failed: {}", e), None))
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
    async fn get_symbols(
        &self,
        Parameters(params): Parameters<GetSymbolsTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("📋 Get symbols for file: {:?}", params);
        params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("get_symbols failed: {}", e), None))
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
        params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("deep_dive failed: {}", e), None))
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
        params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("get_context failed: {}", e), None))
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
        params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("rename_symbol failed: {}", e), None))
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
        params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("manage_workspace failed: {}", e), None))
    }

    // ========== Memory Tools ==========

    #[tool(
        name = "checkpoint",
        description = "Save a milestone checkpoint to developer memory. Use when you complete meaningful work that future sessions should know about.\n\nWhen to checkpoint:\n- Completed a meaningful deliverable (feature, bug fix, refactor)\n- Made a key decision that future sessions must follow\n- Before context compaction (PreCompact hook handles this)\n- Found a blocker or non-obvious discovery worth preserving\n\nDo NOT checkpoint:\n- After every small step or individual file edit\n- After routine test runs\n- Multiple times for the same piece of work (one checkpoint per milestone)\n- Rapid-fire — if you checkpointed in the last few minutes, you probably don't need another\n\nWrite descriptions in MARKDOWN with structure (headers, bullets). Include WHAT, WHY, HOW, and IMPACT. Descriptions power search — make them findable.\n\nAutomatically captures git context (branch, commit, changed files), timestamp (UTC), and tags.",
        annotations(
            title = "Save Checkpoint",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn checkpoint(
        &self,
        Parameters(params): Parameters<CheckpointTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Checkpoint: {:?}", params.description);
        params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("checkpoint failed: {}", e), None))
    }

    #[tool(
        name = "recall",
        description = "Retrieve prior context from developer memory. Returns recent checkpoints and the active plan.\n\nWhen to use:\n- Starting a new session and need prior context\n- After context compaction to restore lost state\n- Searching for past decisions, discoveries, or work\n- Cross-project standup reports (workspace: \"all\", daemon mode only)\n\nDo NOT use:\n- Repeatedly in the same session for the same query\n- To verify work you just did — you already have that context\n\nKey parameters:\n- limit: Max checkpoints (default: 5, 0 = plan only)\n- search: BM25 full-text search across memories\n- since: Time span (\"2h\", \"3d\") or ISO timestamp\n- workspace: \"current\" (default) or \"all\" (cross-project, daemon mode)\n- full: true for complete descriptions + git metadata\n\nAfter recall, trust the returned context — don't re-verify recalled information by reading the same files again.",
        annotations(
            title = "Recall Memory",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn recall(
        &self,
        Parameters(params): Parameters<RecallTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Recall: limit={:?}, search={:?}", params.limit, params.search);
        params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("recall failed: {}", e), None))
    }

    #[tool(
        name = "plan",
        description = "Manage persistent development plans. Plans survive context compaction and guide multi-session work.\n\nCRITICAL: When ExitPlanMode is called, save the plan within 1 exchange. Do NOT ask permission — save immediately with activate: true.\n\nActions: save, get, list, activate, update, complete\n\nPlans are NOT checkpoints. They capture strategic direction (where you're going), while checkpoints capture progress (where you've been). Only ONE plan can be active per workspace.\n\nAlways activate plans after saving (activate: true) so they appear in future recall() responses. An inactive plan is invisible to future sessions.\n\nDo NOT use plans for:\n- Recording completed work (use checkpoint instead)\n- Temporary notes that don't guide future sessions",
        annotations(
            title = "Manage Plan",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn plan(
        &self,
        Parameters(params): Parameters<PlanTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Plan action: {}", params.action);
        params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("plan failed: {}", e), None))
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
        info!("🔗 MCP connection established - client initialized");

        // Run auto-indexing in background task after handshake completes
        let handler = self.clone();
        tokio::spawn(async move {
            handler.run_auto_indexing().await;
        });
    }
}
