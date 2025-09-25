use async_trait::async_trait;
use rust_mcp_sdk::schema::{
    schema_utils::CallToolError, CallToolRequest, CallToolResult, ListToolsRequest,
    ListToolsResult, RpcError,
};
use rust_mcp_sdk::{mcp_server::ServerHandler, McpServer};
use std::sync::Arc;
use tracing::{info, debug, error};
use anyhow::Result;

use crate::tools::JulieTools;
use crate::extractors::{Symbol, Relationship};
use std::sync::RwLock;

/// Julie's custom handler for MCP messages
///
/// This handler manages the core Julie functionality including:
/// - Code intelligence operations (search, navigation, extraction)
/// - Symbol database management
/// - Semantic search and embeddings
/// - Cross-language relationship detection
pub struct JulieServerHandler {
    /// In-memory storage for indexed symbols (basic implementation)
    pub symbols: Arc<RwLock<Vec<Symbol>>>,
    /// In-memory storage for symbol relationships
    pub relationships: Arc<RwLock<Vec<Relationship>>>,
    /// Flag to track if workspace has been indexed
    pub is_indexed: Arc<RwLock<bool>>,
}

impl JulieServerHandler {
    /// Create a new Julie server handler with all components initialized
    pub async fn new() -> Result<Self> {
        info!("🔧 Initializing Julie server handler");

        // TODO: Initialize core components:
        // let symbol_database = SymbolDatabase::new().await?;
        // let search_engine = SearchEngine::new().await?;
        // let embedding_service = EmbeddingService::new().await?;
        // let extractor_manager = ExtractorManager::new().await?;

        debug!("✓ Julie handler components initialized");

        Ok(Self {
            symbols: Arc::new(RwLock::new(Vec::new())),
            relationships: Arc::new(RwLock::new(Vec::new())),
            is_indexed: Arc::new(RwLock::new(false)),
        })
    }

    /// Get the current working directory for workspace operations
    fn get_workspace_path(&self) -> std::path::PathBuf {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
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