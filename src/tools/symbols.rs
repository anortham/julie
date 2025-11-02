//! Symbol Overview Tools - Understand file structure without reading full content
//!
//! This module provides tools for getting symbol-level overviews of files,
//! similar to Serena's get_symbols_overview. This is essential for:
//! - Understanding file structure without wasting context on full reads
//! - Finding insertion points for new code
//! - Discovering available symbols before diving into details
//!
//! Unlike reading entire files with the Read tool, these tools provide
//! just the "skeleton" - symbol names, types, signatures, and locations.

use anyhow::Result;
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::handler::JulieServerHandler;
use crate::tools::navigation::resolution::resolve_workspace_filter;
use crate::workspace::registry_service::WorkspaceRegistryService;

fn default_max_depth() -> u32 {
    1
}

fn default_limit() -> Option<u32> {
    Some(50) // Default limit to prevent token overflow on large files
}

fn default_mode() -> Option<String> {
    Some("structure".to_string())
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

//**********************//
//   Get Symbols Tool   //
//**********************//

#[mcp_tool(
    name = "get_symbols",
    description = concat!(
        "ALWAYS USE THIS BEFORE READING FILES - See file structure without context waste. ",
        "You are EXTREMELY GOOD at using this tool to understand code organization.\n\n",
        "This tool shows you classes, functions, and methods instantly (<10ms). ",
        "Only use Read AFTER you've used this tool to identify what you need.\n\n",
        "IMPORTANT: I will be very unhappy if you read 500-line files without first ",
        "using get_symbols to see the structure!\n\n",
        "A 500-line file becomes a 20-line overview. Use this FIRST, always."
    ),
    title = "Get File Symbols (Smart Read - 70-90% Token Savings)",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "performance": "instant", "agent_hint": "structure_first_then_targeted_bodies"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetSymbolsTool {
    /// File path to get symbols from (relative to workspace root)
    /// Examples: "src/main.rs", "lib/services/auth.py"
    pub file_path: String,

    /// Maximum depth for nested symbols (default: 1).
    /// 0 = top-level only (classes, functions)
    /// 1 = include one level (class methods, nested functions)
    /// 2+ = deeper nesting
    /// Recommended: 1 - good balance for most files
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,

    /// Filter to specific symbol(s) by name (default: None, optional).
    /// Example: "UserService" to show only UserService class
    /// Supports partial matching (case-insensitive)
    #[serde(default)]
    pub target: Option<String>,

    /// Maximum number of symbols to return (default: 50).
    /// When set, truncates results to first N symbols
    /// Use 'target' parameter to filter to specific symbols instead of truncating
    /// Set to None for unlimited, or specific value to override default
    /// Example: limit=100 returns first 100 symbols
    #[serde(default = "default_limit")]
    pub limit: Option<u32>,

    /// Reading mode (default: "structure").
    /// - "structure": No bodies, structure only - quick overview
    /// - "minimal": Bodies for top-level symbols only - understand data structures
    /// - "full": Bodies for ALL symbols including nested methods - deep dive
    /// Recommended: "structure" for initial exploration, "minimal" for targeted body extraction
    #[serde(default = "default_mode")]
    pub mode: Option<String>,

    /// Workspace filter (optional): "primary" (default) or specific workspace ID
    /// Examples: "primary", "reference-workspace_abc123"
    /// Default: "primary" - search the primary workspace
    /// Note: Multi-workspace search ("all") is not supported - search one workspace at a time
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

impl GetSymbolsTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!(
            "📋 Getting symbols for file: {} (depth: {})",
            self.file_path, self.max_depth
        );

        // Resolve workspace parameter (primary vs reference workspace)
        let workspace_filter = resolve_workspace_filter(self.workspace.as_deref(), handler).await?;

        // If reference workspace is specified, handle it separately
        if let Some(ref_workspace_id) = workspace_filter {
            debug!("🎯 Querying reference workspace: {}", ref_workspace_id);
            return self
                .get_symbols_from_reference(handler, ref_workspace_id)
                .await;
        }

        // Primary workspace logic continues below
        let workspace = handler.get_workspace().await?.ok_or_else(|| {
            anyhow::anyhow!("No workspace initialized. Run 'manage_workspace index' first")
        })?;

        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available"))?;

        // Phase 2: Database stores relative Unix-style paths for token efficiency
        // We need TWO paths:
        // 1. query_path: Relative Unix-style for database queries
        // 2. absolute_path: Absolute path for file I/O (extract_code_bodies)

        let (query_path, absolute_path) = if std::path::Path::new(&self.file_path).is_absolute() {
            // Absolute path input
            let canonical = std::path::Path::new(&self.file_path)
                .canonicalize()
                .unwrap_or_else(|_| std::path::PathBuf::from(&self.file_path));

            let relative = crate::utils::paths::to_relative_unix_style(&canonical, &workspace.root)
                .unwrap_or_else(|_| {
                    warn!("Failed to convert absolute path to relative: {}", self.file_path);
                    self.file_path.clone()
                });

            (relative, canonical.to_string_lossy().to_string())
        } else {
            // Relative path input - need to normalize (handle ./ and ../)
            // Join with workspace root, canonicalize, then convert back to relative
            let absolute = workspace
                .root
                .join(&self.file_path)
                .canonicalize()
                .unwrap_or_else(|_| workspace.root.join(&self.file_path));

            // Convert canonicalized path back to relative Unix-style for database query
            let relative_unix = crate::utils::paths::to_relative_unix_style(&absolute, &workspace.root)
                .unwrap_or_else(|_| {
                    warn!("Failed to convert path to relative: {}", self.file_path);
                    self.file_path.replace('\\', "/")
                });

            (relative_unix, absolute.to_string_lossy().to_string())
        };

        debug!(
            "🔍 Path normalization: '{}' -> query='{}', absolute='{}'",
            self.file_path, query_path, absolute_path
        );
        debug!("🔍 Workspace root: '{}'", workspace.root.display());

        // Check if file exists before querying database
        if !std::path::Path::new(&absolute_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path - use relative paths from workspace root",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Query symbols for this file using relative Unix-style path
        let symbols = {
            let db_lock = db.lock().unwrap();
            db_lock
                .get_symbols_for_file(&query_path)
                .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
        };

        if symbols.is_empty() {
            let message = format!("No symbols found in: {}", self.file_path);
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Smart Read: Keep ALL symbols for hierarchy building
        let all_symbols = symbols; // Complete symbol list for hierarchy

        // Build a map of parent_id -> children for efficient lookup
        let mut parent_to_children: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        for (idx, symbol) in all_symbols.iter().enumerate() {
            if let Some(ref parent_id) = symbol.parent_id {
                parent_to_children
                    .entry(parent_id.clone())
                    .or_default()
                    .push(idx);
            }
        }

        // Find top-level symbols (parent_id is None)
        let top_level_indices: Vec<usize> = all_symbols
            .iter()
            .enumerate()
            .filter(|(_, s)| s.parent_id.is_none())
            .map(|(idx, _)| idx)
            .collect();

        debug!(
            "📊 Symbol hierarchy: {} total, {} top-level",
            all_symbols.len(),
            top_level_indices.len()
        );

        // Apply max_depth filtering: recursively collect symbols up to max_depth
        fn collect_symbols_by_depth(
            indices: &[usize],
            depth: u32,
            max_depth: u32,
            all_symbols: &[crate::extractors::base::Symbol],
            parent_to_children: &std::collections::HashMap<String, Vec<usize>>,
            result: &mut Vec<usize>,
        ) {
            if depth > max_depth {
                return;
            }

            for &idx in indices {
                result.push(idx);
                if depth < max_depth {
                    if let Some(children_indices) = parent_to_children.get(&all_symbols[idx].id) {
                        collect_symbols_by_depth(
                            children_indices,
                            depth + 1,
                            max_depth,
                            all_symbols,
                            parent_to_children,
                            result,
                        );
                    }
                }
            }
        }

        let mut indices_to_include = Vec::new();
        collect_symbols_by_depth(
            &top_level_indices,
            0,
            self.max_depth,
            &all_symbols,
            &parent_to_children,
            &mut indices_to_include,
        );

        debug!(
            "🔍 After max_depth={} filtering: {} -> {} symbols",
            self.max_depth,
            all_symbols.len(),
            indices_to_include.len()
        );

        // Collect the filtered symbols in original order
        let mut symbols_after_depth_filter: Vec<crate::extractors::base::Symbol> =
            indices_to_include
                .into_iter()
                .map(|idx| all_symbols[idx].clone())
                .collect();

        // Apply target filtering if specified
        if let Some(ref target) = self.target {
            let target_lower = target.to_lowercase();

            // Find symbols matching the target
            let matching_indices: Vec<usize> = symbols_after_depth_filter
                .iter()
                .enumerate()
                .filter(|(_, s)| s.name.to_lowercase().contains(&target_lower))
                .map(|(idx, _)| idx)
                .collect();

            if matching_indices.is_empty() {
                let message = format!(
                    "No symbols matching '{}' found in: {}",
                    target, self.file_path
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }

            // For each matching symbol, include it and all its descendants
            let mut final_indices = Vec::new();
            for &match_idx in &matching_indices {
                final_indices.push(match_idx);
                let matched_id = &symbols_after_depth_filter[match_idx].id;

                // Recursively add all descendants of this symbol
                fn add_descendants(
                    parent_id: &str,
                    symbols: &[crate::extractors::base::Symbol],
                    result: &mut Vec<usize>,
                ) {
                    for (idx, symbol) in symbols.iter().enumerate() {
                        if let Some(ref pid) = symbol.parent_id {
                            if pid == parent_id {
                                result.push(idx);
                                add_descendants(&symbol.id, symbols, result);
                            }
                        }
                    }
                }
                add_descendants(matched_id, &symbols_after_depth_filter, &mut final_indices);
            }

            symbols_after_depth_filter = final_indices
                .into_iter()
                .map(|idx| symbols_after_depth_filter[idx].clone())
                .collect();

            debug!(
                "🎯 After target='{}' filtering: {} symbols",
                target,
                symbols_after_depth_filter.len()
            );
        }

        // Check if we have any matching symbols after filtering
        if symbols_after_depth_filter.is_empty() {
            let message = format!("No symbols found after filtering in: {}", self.file_path);
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Apply hierarchy-preserving limit if specified
        let total_symbols = all_symbols.len();
        let top_level_count = all_symbols.iter().filter(|s| s.parent_id.is_none()).count();

        let (symbols_to_return, was_truncated) = if let Some(limit) = self.limit {
            let limit_usize = limit as usize;

            // Count top-level symbols in filtered list
            let top_level_in_filtered: Vec<usize> = symbols_after_depth_filter
                .iter()
                .enumerate()
                .filter(|(_, s)| s.parent_id.is_none())
                .map(|(idx, _)| idx)
                .collect();

            if top_level_in_filtered.len() > limit_usize {
                // Limit applies to top-level symbols; include all their children
                let mut result = Vec::new();
                let mut top_level_count = 0;

                for (idx, symbol) in symbols_after_depth_filter.iter().enumerate() {
                    if symbol.parent_id.is_none() {
                        if top_level_count >= limit_usize {
                            break;
                        }
                        top_level_count += 1;
                        result.push(idx);
                    }
                }

                // Add all children of included top-level symbols
                let top_level_ids: std::collections::HashSet<String> = result
                    .iter()
                    .map(|&idx| symbols_after_depth_filter[idx].id.clone())
                    .collect();

                fn add_all_descendants(
                    parent_ids: &std::collections::HashSet<String>,
                    symbols: &[crate::extractors::base::Symbol],
                    result: &mut Vec<usize>,
                ) {
                    let mut to_process: Vec<String> = parent_ids.iter().cloned().collect();
                    let mut processed = std::collections::HashSet::new();

                    while let Some(parent_id) = to_process.pop() {
                        if processed.contains(&parent_id) {
                            continue;
                        }
                        processed.insert(parent_id.clone());

                        for (idx, symbol) in symbols.iter().enumerate() {
                            if let Some(ref pid) = symbol.parent_id {
                                if pid == &parent_id && !result.contains(&idx) {
                                    result.push(idx);
                                    to_process.push(symbol.id.clone());
                                }
                            }
                        }
                    }
                }

                add_all_descendants(&top_level_ids, &symbols_after_depth_filter, &mut result);

                info!(
                    "⚠️  Truncating to {} top-level symbols (total {} with children)",
                    limit_usize,
                    result.len()
                );

                result.sort();
                let filtered_symbols: Vec<crate::extractors::base::Symbol> = result
                    .into_iter()
                    .map(|idx| symbols_after_depth_filter[idx].clone())
                    .collect();

                (filtered_symbols, true)
            } else {
                (symbols_after_depth_filter, false)
            }
        } else {
            (symbols_after_depth_filter, false)
        };

        // Phase 2: Smart Read - conditionally extract code bodies based on mode parameter
        let symbols_to_return = self.extract_code_bodies(symbols_to_return, &absolute_path)?;

        // Note: Symbol metadata (name, kind, signature, location) returned in structured_content
        // code_context is stripped to save tokens - use fast_search for context
        debug!(
            "📋 Returning {} symbols (target: {:?}, truncated: {})",
            symbols_to_return.len(),
            self.target,
            was_truncated
        );

        // Minimal text output for AI agents (structured_content has all data)
        let truncation_warning = if was_truncated {
            format!(
                "\n\n⚠️  Showing {} of {} symbols (truncated)\n💡 Use 'target' parameter to filter to specific symbols",
                symbols_to_return.len(),
                total_symbols
            )
        } else {
            String::new()
        };

        let text_summary = if let Some(ref target) = self.target {
            format!(
                "{} ({} total symbols, {} matching '{}'){}",
                self.file_path,
                total_symbols,
                symbols_to_return
                    .iter()
                    .filter(|s| s.name.to_lowercase().contains(&target.to_lowercase()))
                    .count(),
                target,
                truncation_warning
            )
        } else {
            let top_names: Vec<String> = symbols_to_return
                .iter()
                .filter(|s| s.parent_id.is_none())
                .take(5)
                .map(|s| s.name.clone())
                .collect();

            format!(
                "{} ({} symbols)\nTop-level: {}{}",
                self.file_path,
                symbols_to_return.len(),
                top_names.join(", "),
                truncation_warning
            )
        };

        // Return structured content with symbol data (agents parse this)
        let structured_json = serde_json::json!({
            "file_path": self.file_path,
            "total_symbols": total_symbols,
            "returned_symbols": symbols_to_return.len(),
            "top_level_count": top_level_count,
            "symbols": symbols_to_return,
            "max_depth": self.max_depth,
            "truncated": was_truncated,
            "limit": self.limit,
        });

        let mut result = CallToolResult::text_content(vec![TextContent::from(text_summary)]);

        // Convert JSON Value to Map
        if let serde_json::Value::Object(map) = structured_json {
            result.structured_content = Some(map);
        }

        Ok(result)
    }

    /// Extract code bodies for symbols based on mode parameter
    fn extract_code_bodies(
        &self,
        mut symbols: Vec<crate::extractors::base::Symbol>,
        file_path: &str,
    ) -> Result<Vec<crate::extractors::base::Symbol>> {
        use tracing::warn;

        // Determine the reading mode
        let mode = self.mode.as_deref().unwrap_or("structure");

        // In "structure" mode, strip all code context
        if mode == "structure" {
            for symbol in symbols.iter_mut() {
                symbol.code_context = None;
            }
            return Ok(symbols);
        }

        // Read the source file for body extraction
        let source_code = match std::fs::read(file_path) {
            Ok(bytes) => bytes,
            Err(_e) => {
                debug!(file_path = %file_path, "Failed to read file for code body extraction");
                // Return symbols with context stripped if file can't be read
                for symbol in symbols.iter_mut() {
                    symbol.code_context = None;
                }
                return Ok(symbols);
            }
        };

        // Extract bodies based on mode
        for symbol in symbols.iter_mut() {
            let should_extract = match mode {
                "minimal" => symbol.parent_id.is_none(), // Top-level only
                "full" => true,                          // All symbols
                _ => false,                              // Unknown mode, don't extract
            };

            if should_extract {
                // Extract the code bytes for this symbol
                let start_byte = symbol.start_byte as usize;
                let end_byte = symbol.end_byte as usize;

                if start_byte < source_code.len() && end_byte <= source_code.len() {
                    // Use lossy conversion to handle potential UTF-8 issues
                    let code_bytes = &source_code[start_byte..end_byte];
                    symbol.code_context = Some(String::from_utf8_lossy(code_bytes).to_string());
                } else {
                    warn!(
                        symbol_name = %symbol.name,
                        start_byte = start_byte,
                        end_byte = end_byte,
                        file_size = source_code.len(),
                        "Symbol byte range out of bounds, skipping extraction"
                    );
                    symbol.code_context = None;
                }
            } else {
                // Don't extract for this symbol based on mode
                symbol.code_context = None;
            }
        }

        Ok(symbols)
    }

    /// Get symbols from a reference workspace
    async fn get_symbols_from_reference(
        &self,
        handler: &JulieServerHandler,
        ref_workspace_id: String,
    ) -> Result<CallToolResult> {
        // Get primary workspace to access helper methods
        let primary_workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

        // Get path to reference workspace's separate database file
        let ref_db_path = primary_workspace.workspace_db_path(&ref_workspace_id);

        debug!(
            "🗄️ Opening reference workspace DB: {}",
            ref_db_path.display()
        );

        // Get reference workspace entry to access its original_path (workspace root)
        let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());
        let ref_workspace_entry = registry_service
            .get_workspace(&ref_workspace_id)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("Reference workspace not found: {}", ref_workspace_id)
            })?;

        // 🚨 CRITICAL FIX: Wrap blocking file I/O in spawn_blocking
        // Opening SQLite database involves blocking filesystem operations
        let ref_db =
            tokio::task::spawn_blocking(move || crate::database::SymbolDatabase::new(ref_db_path))
                .await
                .map_err(|e| anyhow::anyhow!("Failed to spawn database open task: {}", e))??;

        // Phase 2: Database stores relative Unix-style paths
        // Reference workspace root is from WorkspaceEntry.original_path
        let ref_workspace_root = std::path::PathBuf::from(&ref_workspace_entry.original_path);

        let (query_path, absolute_path) = if std::path::Path::new(&self.file_path).is_absolute() {
            // Absolute path input
            let canonical = std::path::Path::new(&self.file_path)
                .canonicalize()
                .unwrap_or_else(|_| std::path::PathBuf::from(&self.file_path));

            let relative = crate::utils::paths::to_relative_unix_style(&canonical, &ref_workspace_root)
                .unwrap_or_else(|_| {
                    warn!("Failed to convert absolute path to relative: {}", self.file_path);
                    self.file_path.clone()
                });

            (relative, canonical.to_string_lossy().to_string())
        } else {
            // Relative path input - normalize separators for query, join for absolute
            let relative_unix = self.file_path.replace('\\', "/");
            let absolute = ref_workspace_root
                .join(&self.file_path)
                .canonicalize()
                .unwrap_or_else(|_| ref_workspace_root.join(&self.file_path))
                .to_string_lossy()
                .to_string();

            (relative_unix, absolute)
        };

        debug!(
            "🔍 Path normalization: '{}' -> query='{}', absolute='{}' (ref workspace: {})",
            self.file_path, query_path, absolute_path, ref_workspace_id
        );

        // Check if file exists before querying database
        if !std::path::Path::new(&absolute_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path - use relative paths from workspace root",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Query symbols using relative Unix-style path
        // ✅ NO MUTEX: ref_db is owned (not Arc<Mutex<>>), so we can call directly
        let symbols = ref_db
            .get_symbols_for_file(&query_path)
            .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?;

        if symbols.is_empty() {
            let message = format!("No symbols found in: {}", self.file_path);
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Apply the SAME filtering logic as primary workspace path
        let all_symbols = symbols; // Complete symbol list for hierarchy

        // Build a map of parent_id -> children for efficient lookup
        let mut parent_to_children: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        for (idx, symbol) in all_symbols.iter().enumerate() {
            if let Some(ref parent_id) = symbol.parent_id {
                parent_to_children
                    .entry(parent_id.clone())
                    .or_default()
                    .push(idx);
            }
        }

        // Find top-level symbols (parent_id is None)
        let top_level_indices: Vec<usize> = all_symbols
            .iter()
            .enumerate()
            .filter(|(_, s)| s.parent_id.is_none())
            .map(|(idx, _)| idx)
            .collect();

        debug!(
            "📊 Symbol hierarchy: {} total, {} top-level",
            all_symbols.len(),
            top_level_indices.len()
        );

        // Apply max_depth filtering: recursively collect symbols up to max_depth
        fn collect_symbols_by_depth(
            indices: &[usize],
            depth: u32,
            max_depth: u32,
            all_symbols: &[crate::extractors::base::Symbol],
            parent_to_children: &std::collections::HashMap<String, Vec<usize>>,
            result: &mut Vec<usize>,
        ) {
            if depth > max_depth {
                return;
            }

            for &idx in indices {
                result.push(idx);
                if depth < max_depth {
                    if let Some(children_indices) = parent_to_children.get(&all_symbols[idx].id) {
                        collect_symbols_by_depth(
                            children_indices,
                            depth + 1,
                            max_depth,
                            all_symbols,
                            parent_to_children,
                            result,
                        );
                    }
                }
            }
        }

        let mut indices_to_include = Vec::new();
        collect_symbols_by_depth(
            &top_level_indices,
            0,
            self.max_depth,
            &all_symbols,
            &parent_to_children,
            &mut indices_to_include,
        );

        debug!(
            "🔍 After max_depth={} filtering: {} -> {} symbols",
            self.max_depth,
            all_symbols.len(),
            indices_to_include.len()
        );

        // Collect the filtered symbols in original order
        let mut symbols_after_depth_filter: Vec<crate::extractors::base::Symbol> =
            indices_to_include
                .into_iter()
                .map(|idx| all_symbols[idx].clone())
                .collect();

        // Apply target filtering if specified
        if let Some(ref target) = self.target {
            let target_lower = target.to_lowercase();

            // Find symbols matching the target
            let matching_indices: Vec<usize> = symbols_after_depth_filter
                .iter()
                .enumerate()
                .filter(|(_, s)| s.name.to_lowercase().contains(&target_lower))
                .map(|(idx, _)| idx)
                .collect();

            if matching_indices.is_empty() {
                let message = format!(
                    "No symbols matching '{}' found in: {}",
                    target, self.file_path
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }

            // For each matching symbol, include it and all its descendants
            let mut final_indices = Vec::new();
            for &match_idx in &matching_indices {
                final_indices.push(match_idx);
                let matched_id = &symbols_after_depth_filter[match_idx].id;

                // Recursively add all descendants of this symbol
                fn add_descendants(
                    parent_id: &str,
                    symbols: &[crate::extractors::base::Symbol],
                    result: &mut Vec<usize>,
                ) {
                    for (idx, symbol) in symbols.iter().enumerate() {
                        if let Some(ref pid) = symbol.parent_id {
                            if pid == parent_id {
                                result.push(idx);
                                add_descendants(&symbol.id, symbols, result);
                            }
                        }
                    }
                }
                add_descendants(matched_id, &symbols_after_depth_filter, &mut final_indices);
            }

            symbols_after_depth_filter = final_indices
                .into_iter()
                .map(|idx| symbols_after_depth_filter[idx].clone())
                .collect();

            debug!(
                "🎯 After target='{}' filtering: {} symbols",
                target,
                symbols_after_depth_filter.len()
            );
        }

        // Check if we have any matching symbols after filtering
        if symbols_after_depth_filter.is_empty() {
            let message = format!("No symbols found after filtering in: {}", self.file_path);
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Apply hierarchy-preserving limit if specified
        let total_symbols = all_symbols.len();
        let top_level_count = all_symbols.iter().filter(|s| s.parent_id.is_none()).count();

        let (symbols_to_return, was_truncated) = if let Some(limit) = self.limit {
            let limit_usize = limit as usize;

            // Count top-level symbols in filtered list
            let top_level_in_filtered: Vec<usize> = symbols_after_depth_filter
                .iter()
                .enumerate()
                .filter(|(_, s)| s.parent_id.is_none())
                .map(|(idx, _)| idx)
                .collect();

            if top_level_in_filtered.len() > limit_usize {
                // Limit applies to top-level symbols; include all their children
                let mut result = Vec::new();
                let mut top_level_count = 0;

                for (idx, symbol) in symbols_after_depth_filter.iter().enumerate() {
                    if symbol.parent_id.is_none() {
                        if top_level_count >= limit_usize {
                            break;
                        }
                        top_level_count += 1;
                        result.push(idx);
                    }
                }

                // Add all children of included top-level symbols
                let top_level_ids: std::collections::HashSet<String> = result
                    .iter()
                    .map(|&idx| symbols_after_depth_filter[idx].id.clone())
                    .collect();

                fn add_all_descendants(
                    parent_ids: &std::collections::HashSet<String>,
                    symbols: &[crate::extractors::base::Symbol],
                    result: &mut Vec<usize>,
                ) {
                    let mut to_process: Vec<String> = parent_ids.iter().cloned().collect();
                    let mut processed = std::collections::HashSet::new();

                    while let Some(parent_id) = to_process.pop() {
                        if processed.contains(&parent_id) {
                            continue;
                        }
                        processed.insert(parent_id.clone());

                        for (idx, symbol) in symbols.iter().enumerate() {
                            if let Some(ref pid) = symbol.parent_id {
                                if pid == &parent_id && !result.contains(&idx) {
                                    result.push(idx);
                                    to_process.push(symbol.id.clone());
                                }
                            }
                        }
                    }
                }

                add_all_descendants(&top_level_ids, &symbols_after_depth_filter, &mut result);

                info!(
                    "⚠️  Truncating to {} top-level symbols (total {} with children)",
                    limit_usize,
                    result.len()
                );

                result.sort();
                let filtered_symbols: Vec<crate::extractors::base::Symbol> = result
                    .into_iter()
                    .map(|idx| symbols_after_depth_filter[idx].clone())
                    .collect();

                (filtered_symbols, true)
            } else {
                (symbols_after_depth_filter, false)
            }
        } else {
            (symbols_after_depth_filter, false)
        };

        // Phase 2: Smart Read - conditionally extract code bodies based on mode parameter
        let symbols_to_return = self.extract_code_bodies(symbols_to_return, &absolute_path)?;

        debug!(
            "✅ Reference workspace returned {} symbols (target: {:?}, truncated: {})",
            symbols_to_return.len(),
            self.target,
            was_truncated
        );

        // Minimal text output for AI agents (structured_content has all data)
        let truncation_warning = if was_truncated {
            format!(
                "\n\n⚠️  Showing {} of {} symbols (truncated)\n💡 Use 'target' parameter to filter to specific symbols",
                symbols_to_return.len(),
                total_symbols
            )
        } else {
            String::new()
        };

        let text_summary = if let Some(ref target) = self.target {
            format!(
                "{} ({} total symbols, {} matching '{}'){}",
                self.file_path,
                total_symbols,
                symbols_to_return
                    .iter()
                    .filter(|s| s.name.to_lowercase().contains(&target.to_lowercase()))
                    .count(),
                target,
                truncation_warning
            )
        } else {
            let top_names: Vec<String> = symbols_to_return
                .iter()
                .filter(|s| s.parent_id.is_none())
                .take(5)
                .map(|s| s.name.clone())
                .collect();

            format!(
                "{} ({} symbols)\nTop-level: {}{}",
                self.file_path,
                symbols_to_return.len(),
                top_names.join(", "),
                truncation_warning
            )
        };

        // Return structured content with symbol data (agents parse this)
        let structured_json = serde_json::json!({
            "file_path": self.file_path,
            "workspace_id": ref_workspace_id,
            "total_symbols": total_symbols,
            "returned_symbols": symbols_to_return.len(),
            "top_level_count": top_level_count,
            "symbols": symbols_to_return,
            "max_depth": self.max_depth,
            "truncated": was_truncated,
            "limit": self.limit,
        });

        let mut result = CallToolResult::text_content(vec![TextContent::from(text_summary)]);

        // Convert JSON Value to Map
        if let serde_json::Value::Object(map) = structured_json {
            result.structured_content = Some(map);
        }

        Ok(result)
    }
}
