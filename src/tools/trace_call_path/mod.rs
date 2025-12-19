//! Cross-Language Call Path Tracing - Julie's Killer Feature
//!
//! This tool traces execution flow across multiple programming languages using:
//! 1. Direct relationship analysis from the symbol database
//! 2. Naming convention variants for cross-language bridging
//! 3. Semantic embeddings for conceptual similarity
//!
//! This is Julie's unique differentiator - NO other tool can trace calls
//! across language boundaries in polyglot codebases.

pub mod cross_language;
pub mod formatting;
pub mod tracing;
pub mod types;

use anyhow::{Result, anyhow};
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, WithStructuredContent};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::handler::JulieServerHandler;

pub use types::{CallPath, SerializablePathNode, TraceCallPathResult};
use types::{default_depth, default_output_format, default_upstream, default_workspace};

//***************************//
//   Trace Call Path Tool    //
//***************************//

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TraceCallPathTool {
    /// Symbol name (supports qualified names)
    pub symbol: String,
    /// Trace direction: "upstream" (default), "downstream", or "both"
    #[serde(default = "default_upstream")]
    pub direction: String,
    /// Maximum trace depth (default: 3, range: 1-10)
    #[serde(default = "default_depth")]
    pub max_depth: u32,
    /// Context file path (helps disambiguate symbols)
    #[serde(default)]
    pub context_file: Option<String>,
    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Output format: "lean" (default - ASCII tree), "json", "toon", or "auto"
    #[serde(default = "default_output_format")]
    pub output_format: Option<String>,
}

impl TraceCallPathTool {
    /// Find cross-language callers using naming variants (exposed for testing)
    pub async fn find_cross_language_callers(
        &self,
        db: &Arc<Mutex<crate::database::SymbolDatabase>>,
        symbol: &crate::extractors::Symbol,
    ) -> Result<Vec<crate::extractors::Symbol>> {
        cross_language::find_cross_language_callers(db, symbol).await
    }

    /// Helper: Create result with lean format as default, JSON/TOON as alternatives
    fn create_result(
        &self,
        _success: bool,
        _paths_found: usize,
        _next_actions: Vec<String>,
        ascii_tree: String,
        _error_message: Option<String>,
        call_paths: Option<Vec<CallPath>>,
        output_format: Option<&str>,
    ) -> Result<CallToolResult> {
        // Return based on output_format - lean (ASCII tree) is default
        match output_format {
            None | Some("lean") | Some("tree") => {
                // Lean mode (DEFAULT): ASCII tree visualization
                ::tracing::debug!(
                    "✅ Returning lean trace results ({} chars)",
                    ascii_tree.len()
                );
                Ok(CallToolResult::text_content(vec![
                    crate::mcp_compat::Content::text(ascii_tree),
                ]))
            }
            Some("toon") | Some("auto") | Some("json") => {
                // Structured formats: Build full result object
                let result = TraceCallPathResult {
                    tool: "trace_call_path".to_string(),
                    symbol: self.symbol.clone(),
                    direction: self.direction.clone(),
                    max_depth: self.max_depth,
                    cross_language: true,
                    success: call_paths.is_some(),
                    paths_found: call_paths.as_ref().map(|p| p.len()).unwrap_or(0),
                    next_actions: vec![
                        "Review call paths to understand execution flow".to_string(),
                        "Use fast_goto to navigate to specific symbols".to_string(),
                    ],
                    error_message: None,
                    call_paths,
                };

                match output_format {
                    Some("toon") => {
                        let toon_flat = result.to_toon_flat();
                        match toon_format::encode_default(&toon_flat) {
                            Ok(toon) => Ok(CallToolResult::text_content(vec![
                                crate::mcp_compat::Content::text(toon),
                            ])),
                            Err(_) => {
                                // Fall back to lean on TOON error
                                Ok(CallToolResult::text_content(vec![
                                    crate::mcp_compat::Content::text(ascii_tree),
                                ]))
                            }
                        }
                    }
                    Some("auto") => {
                        let toon_flat = result.to_toon_flat();
                        if toon_flat.len() >= 10 {
                            if let Ok(toon) = toon_format::encode_default(&toon_flat) {
                                return Ok(CallToolResult::text_content(vec![
                                    crate::mcp_compat::Content::text(toon),
                                ]));
                            }
                        }
                        // Fall back to lean for small results
                        Ok(CallToolResult::text_content(vec![
                            crate::mcp_compat::Content::text(ascii_tree),
                        ]))
                    }
                    _ => {
                        // JSON mode
                        let structured = serde_json::to_value(&result)?;
                        let map = match structured {
                            serde_json::Value::Object(m) => m,
                            _ => return Err(anyhow!("Expected object")),
                        };
                        Ok(CallToolResult::text_content(vec![]).with_structured_content(map))
                    }
                }
            }
            Some(unknown) => {
                // Unknown format - warn and use lean
                ::tracing::warn!(
                    "⚠️ Unknown output_format '{}', using lean format",
                    unknown
                );
                Ok(CallToolResult::text_content(vec![
                    crate::mcp_compat::Content::text(ascii_tree),
                ]))
            }
        }
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        ::tracing::info!(
            "🔍 Tracing call path: {} (direction: {}, depth: {}, cross_lang: enabled)",
            self.symbol,
            self.direction,
            self.max_depth
        );

        // Validate parameters
        if self.max_depth > 10 {
            let message = "Error: max_depth cannot exceed 10 (recommended: 5)".to_string();
            return self.create_result(
                false,
                0,
                vec!["Reduce max_depth to 5 or less".to_string()],
                message.clone(),
                Some(message),
                None,
                self.output_format.as_deref(),
            );
        }

        // Get workspace and database with workspace filtering support
        let primary_workspace = handler.get_workspace().await?.ok_or_else(|| {
            anyhow!("No workspace initialized. Run 'manage_workspace index' first")
        })?;

        // Determine target workspace and load appropriate database + vector store
        let (db, vector_store) = match self.workspace.as_deref() {
            Some("primary") | None => {
                // Use primary workspace database and vector store (default)
                let db = primary_workspace
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow!("No primary database available"))?
                    .clone();

                let vector_store = primary_workspace.vector_store.clone();

                (db, vector_store)
            }
            Some(workspace_id) => {
                // Load reference workspace database
                let ref_db_path = primary_workspace.workspace_db_path(workspace_id);
                if !ref_db_path.exists() {
                    let message = format!(
                        "Reference workspace database not found: {}\nCheck workspace ID with 'manage_workspace list'",
                        workspace_id
                    );
                    return self.create_result(
                        false,
                        0,
                        vec!["Use 'manage_workspace list' to see available workspaces".to_string()],
                        message.clone(),
                        Some(format!("Workspace not found: {}", workspace_id)),
                        None,
                self.output_format.as_deref(),
                    );
                }

                ::tracing::debug!("📂 Opening reference workspace DB: {:?}", ref_db_path);

                // Open reference workspace database in blocking task
                let ref_db = tokio::task::spawn_blocking(move || {
                    crate::database::SymbolDatabase::new(&ref_db_path)
                })
                .await
                .map_err(|e| anyhow!("Failed to spawn database task: {}", e))??;

                let db = Arc::new(Mutex::new(ref_db));

                // Load reference workspace vector store
                let vectors_path = primary_workspace.workspace_vectors_path(workspace_id);
                let vector_store = if vectors_path.exists() {
                    ::tracing::debug!("📂 Loading reference workspace vectors: {:?}", vectors_path);

                    // Load HNSW index from disk
                    let mut store = crate::embeddings::vector_store::VectorStore::new(384)?;
                    let hnsw_path = vectors_path.join("hnsw_index");

                    if hnsw_path.with_extension("hnsw.graph").exists() {
                        match store.load_hnsw_index(&hnsw_path) {
                            Ok(()) => {
                                ::tracing::debug!("✅ Loaded HNSW index for reference workspace");
                                Some(Arc::new(tokio::sync::RwLock::new(store)))
                            }
                            Err(e) => {
                                ::tracing::debug!(
                                    "⚠️  Failed to load HNSW index for reference workspace: {}",
                                    e
                                );
                                None
                            }
                        }
                    } else {
                        ::tracing::debug!(
                            "ℹ️  No HNSW index found for reference workspace (semantic search disabled)"
                        );
                        None
                    }
                } else {
                    ::tracing::debug!(
                        "ℹ️  No vectors directory for reference workspace (semantic search disabled)"
                    );
                    None
                };

                (db, vector_store)
            }
        };

        // Find the starting symbol(s) - wrap in block to ensure mutex guard is dropped
        let mut starting_symbols = {
            let db_lock = match db.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    ::tracing::warn!(
                        "Database mutex poisoned in trace_call_path symbol lookup, recovering: {}",
                        poisoned
                    );
                    poisoned.into_inner()
                }
            };
            db_lock.get_symbols_by_name(&self.symbol)?
        }; // Guard dropped here automatically

        if starting_symbols.is_empty() {
            let message = format!(
                "Symbol not found: '{}'\nTry fast_search to find the symbol, or check spelling",
                self.symbol
            );
            return self.create_result(
                false,
                0,
                vec![
                    "Use fast_search to find the symbol".to_string(),
                    "Check symbol name spelling".to_string(),
                ],
                message.clone(),
                Some(format!("Symbol not found: {}", self.symbol)),
                None,
                self.output_format.as_deref(),
            );
        }

        // If context file provided, filter to symbols in that file
        if let Some(ref context_file) = self.context_file {
            starting_symbols.retain(|s| s.file_path == *context_file);
            if starting_symbols.is_empty() {
                let message = format!(
                    "Symbol '{}' not found in file: {} (try without context_file to search all files)",
                    self.symbol, context_file
                );
                return self.create_result(
                    false,
                    0,
                    vec!["Try without context_file parameter".to_string()],
                    message.clone(),
                    Some(format!("Symbol not found in file: {}", context_file)),
                    None,
                self.output_format.as_deref(),
                );
            }
        }

        // Build call path tree
        let mut visited = HashSet::new();
        let mut all_trees = Vec::new();

        for starting_symbol in &starting_symbols {
            let call_tree = match self.direction.as_str() {
                "upstream" => {
                    tracing::trace_upstream(
                        handler,
                        &db,
                        &vector_store,
                        starting_symbol,
                        0,
                        &mut visited,
                        self.max_depth,
                    )
                    .await?
                }
                "downstream" => {
                    tracing::trace_downstream(
                        handler,
                        &db,
                        &vector_store,
                        starting_symbol,
                        0,
                        &mut visited,
                        self.max_depth,
                    )
                    .await?
                }
                "both" => {
                    // Use single visited set to prevent duplicate processing across both directions
                    let mut upstream = tracing::trace_upstream(
                        handler,
                        &db,
                        &vector_store,
                        starting_symbol,
                        0,
                        &mut visited,
                        self.max_depth,
                    )
                    .await?;
                    let downstream = tracing::trace_downstream(
                        handler,
                        &db,
                        &vector_store,
                        starting_symbol,
                        0,
                        &mut visited,
                        self.max_depth,
                    )
                    .await?;
                    upstream.extend(downstream);
                    upstream
                }
                _ => {
                    let message = format!(
                        "Invalid direction: '{}' (valid options: 'upstream', 'downstream', 'both')",
                        self.direction
                    );
                    return self.create_result(
                        false,
                        0,
                        vec!["Use 'upstream', 'downstream', or 'both'".to_string()],
                        message.clone(),
                        Some(format!("Invalid direction: {}", self.direction)),
                        None,
                self.output_format.as_deref(),
                    );
                }
            };

            if !call_tree.is_empty() {
                all_trees.push((starting_symbol.clone(), call_tree));
            }
        }

        // Format output - "lean" (ASCII tree) is default
        let output = formatting::format_call_trees(
            &all_trees,
            &self.symbol,
            &self.direction,
            self.max_depth,
            self.output_format.as_deref().unwrap_or("lean"),
        )?;

        // Convert trees to serializable format for structured content
        let call_paths = if !all_trees.is_empty() {
            Some(formatting::trees_to_call_paths(&all_trees))
        } else {
            None
        };

        self.create_result(
            true,
            all_trees.len(),
            vec![
                "Review call paths to understand execution flow".to_string(),
                "Use fast_goto to navigate to specific symbols".to_string(),
            ],
            output,
            None,
            call_paths,
            self.output_format.as_deref(),
        )
    }
}
