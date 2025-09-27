use async_trait::async_trait;
use rust_mcp_sdk::schema::{
    schema_utils::CallToolError, CallToolRequest, CallToolResult, ListToolsRequest,
    ListToolsResult, RpcError,
};
use rust_mcp_sdk::{mcp_server::ServerHandler, McpServer};
use std::sync::Arc;
use tracing::{info, debug, error, warn};
use anyhow::Result;

use crate::tools::JulieTools;
use crate::extractors::{Symbol, Relationship};
use crate::search::SearchEngine;
use crate::embeddings::EmbeddingEngine;
use crate::workspace::JulieWorkspace;
use tokio::sync::RwLock;

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
    /// In-memory storage for indexed symbols (compatibility)
    pub symbols: Arc<RwLock<Vec<Symbol>>>,
    /// In-memory storage for symbol relationships (compatibility)
    pub relationships: Arc<RwLock<Vec<Relationship>>>,
    /// Tantivy-based search engine for fast indexed search
    pub search_engine: Arc<RwLock<SearchEngine>>,
    /// Flag to track if workspace has been indexed
    pub is_indexed: Arc<RwLock<bool>>,
    /// Cached embedding engine for semantic search (expensive to initialize)
    pub embedding_engine: Arc<RwLock<Option<EmbeddingEngine>>>,
}

impl JulieServerHandler {
    /// Create a new Julie server handler with all components initialized
    pub async fn new() -> Result<Self> {
        info!("🔧 Initializing Julie server handler");

        // Initialize SearchEngine with in-memory index for compatibility (workspace will override)
        info!("🔍 Initializing fallback Tantivy search engine");
        let search_engine = SearchEngine::in_memory().map_err(|e| {
            anyhow::anyhow!("Failed to initialize fallback search engine: {}", e)
        })?;

        debug!("✓ Julie handler components initialized");

        Ok(Self {
            workspace: Arc::new(RwLock::new(None)),
            symbols: Arc::new(RwLock::new(Vec::new())),
            relationships: Arc::new(RwLock::new(Vec::new())),
            search_engine: Arc::new(RwLock::new(search_engine)),
            is_indexed: Arc::new(RwLock::new(false)),
            embedding_engine: Arc::new(RwLock::new(None)),
        })
    }

    /// Get the current working directory for workspace operations
    fn get_workspace_path(&self) -> std::path::PathBuf {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    }

    /// Get or initialize the cached embedding engine for semantic operations
    /// This avoids expensive repeated initialization of the ONNX model
    pub async fn ensure_embedding_engine(&self) -> Result<()> {
        let mut embedding_guard = self.embedding_engine.write().await;

        if embedding_guard.is_none() {
            debug!("🧠 Initializing cached embedding engine");
            // Create model cache directory
            let cache_dir = std::env::temp_dir().join("julie_cache").join("embeddings");
            std::fs::create_dir_all(&cache_dir).map_err(|e| {
                anyhow::anyhow!("Failed to create embedding cache directory: {}", e)
            })?;

            let engine = EmbeddingEngine::new("bge-small", cache_dir).map_err(|e| {
                anyhow::anyhow!("Failed to initialize embedding engine: {}", e)
            })?;

            *embedding_guard = Some(engine);
            info!("✅ Cached embedding engine initialized successfully");
        }

        Ok(())
    }

    /// Initialize or load workspace and update components to use persistent storage
    pub async fn initialize_workspace(&self, workspace_path: Option<String>) -> Result<()> {
        let target_path = match workspace_path {
            Some(path) => {
                let expanded_path = shellexpand::tilde(&path).to_string();
                std::path::PathBuf::from(expanded_path)
            },
            None => self.get_workspace_path(),
        };

        info!("Initializing workspace at: {}", target_path.display());

        // Try to load existing workspace first
        let workspace = match JulieWorkspace::detect_and_load(target_path.clone())? {
            Some(existing_workspace) => {
                info!("Loaded existing workspace");
                existing_workspace
            },
            None => {
                info!("Creating new workspace");
                JulieWorkspace::initialize(target_path)?
            }
        };

        // Update search engine to use persistent index from workspace
        if let Some(persistent_search) = &workspace.search {
            let mut search_guard = self.search_engine.write().await;
            let persistent_search_guard = persistent_search.read().await;
            // We'll need to implement a way to replace the search engine
            // For now, store the workspace so indexing can use it
        }

        // Store the initialized workspace
        {
            let mut workspace_guard = self.workspace.write().await;
            *workspace_guard = Some(workspace);
        }

        // Start file watching if workspace was successfully initialized
        if let Some(mut workspace_clone) = self.get_workspace().await? {
            if let Err(e) = workspace_clone.start_file_watching().await {
                warn!("Failed to start file watching: {}", e);
            }
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
        let tool_params: JulieTools = JulieTools::try_from(request.params)
            .map_err(|e| {
                error!("❌ Failed to parse tool parameters: {}", e);
                CallToolError::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid tool parameters: {}", e)
                ))
            })?;

        // Execute the requested tool
        let result = match &tool_params {
            JulieTools::IndexWorkspaceTool(tool) => {
                info!("📚 Indexing workspace at: {:?}", self.get_workspace_path());
                tool.call_tool(self).await
            },
            // Consolidated fast tools with appealing names
            JulieTools::FastSearchTool(tool) => {
                debug!("⚡ Fast search: {:?}", tool);
                tool.call_tool(self).await
            },
            JulieTools::FastGotoTool(tool) => {
                debug!("⚡ Fast goto definition: {:?}", tool);
                tool.call_tool(self).await
            },
            JulieTools::FastRefsTool(tool) => {
                debug!("⚡ Fast find references: {:?}", tool);
                tool.call_tool(self).await
            },
            JulieTools::FastExploreTool(tool) => {
                debug!("⚡ Fast explore codebase: {:?}", tool);
                tool.call_tool(self).await
            },
            JulieTools::FindLogicTool(tool) => {
                debug!("🏢 Find business logic: {:?}", tool);
                tool.call_tool(self).await
            },
            JulieTools::FastEditTool(tool) => {
                debug!("⚡ Fast edit: {:?}", tool);
                tool.call_tool(self).await
            },
            JulieTools::LineEditTool(tool) => {
                debug!("📝 Line edit: {:?}", tool);
                tool.call_tool(self).await
            },
            JulieTools::ManageWorkspaceTool(tool) => {
                debug!("🏗️ Manage workspace: {:?}", tool);
                tool.call_tool(self).await
            },
        };

        match result {
            Ok(call_result) => {
                info!("✅ Tool executed successfully");
                Ok(call_result)
            },
            Err(e) => {
                error!("❌ Tool execution failed: {}", e);
                Err(CallToolError::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Tool execution failed: {}", e)
                )))
            }
        }
    }
}