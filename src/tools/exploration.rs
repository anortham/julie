use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use rust_mcp_sdk::{macros::mcp_tool};
use rust_mcp_sdk::macros::JsonSchema;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use tracing::debug;
use std::collections::HashMap;

use crate::handler::JulieServerHandler;

//********************//
// Exploration Tools  //
//********************//

#[mcp_tool(
    name = "fast_explore",
    description = "UNDERSTAND FIRST - Multi-mode codebase exploration (overview/dependencies/trace/hotspots)",
    title = "Fast Codebase Architecture Explorer"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastExploreTool {
    pub mode: String,
    #[serde(default = "default_medium")]
    pub depth: String,
    #[serde(default)]
    pub focus: Option<String>,
}

fn default_medium() -> String { "medium".to_string() }

impl FastExploreTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🧭 Exploring codebase: mode={}, focus={:?}", self.mode, self.focus);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().await;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable exploration.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform exploration based on mode
        let message = match self.mode.as_str() {
            "overview" => self.generate_overview(handler).await?,
            "dependencies" => self.analyze_dependencies(handler).await?,
            "hotspots" => self.find_hotspots(handler).await?,
            "trace" => self.trace_relationships(handler).await?,
            _ => format!(
                "❌ Unknown exploration mode: '{}'\n\
                💡 Supported modes: overview, dependencies, hotspots, trace",
                self.mode
            ),
        };

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    async fn generate_overview(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().await;

        // WILDCARD SEARCH FIX: Use in-memory symbol store instead of broken search("*")
        // The search engine's wildcard query fails, but symbols are correctly stored in memory
        let all_symbols = handler.symbols.read().await;

        // Count by symbol type - from in-memory symbols
        let mut counts = HashMap::new();
        let mut file_counts = HashMap::new();
        let mut language_counts = HashMap::new();

        for symbol in all_symbols.iter() {
            *counts.entry(&symbol.kind).or_insert(0) += 1;
            *file_counts.entry(&symbol.file_path).or_insert(0) += 1;
            *language_counts.entry(&symbol.language).or_insert(0) += 1;
        }

        let mut message = format!(
            "🧭 Codebase Overview\n\
            ========================\n\
            📊 Total Symbols: {}\n\
            📁 Total Files: {}\n\
            🔗 Total Relationships: {}\n\n",
            all_symbols.len(),
            file_counts.len(),
            relationships.len()
        );

        // Symbol breakdown
        message.push_str("🏷️ Symbol Types:\n");
        let mut sorted_counts: Vec<_> = counts.iter().collect();
        sorted_counts.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (kind, count) in sorted_counts {
            message.push_str(&format!("  {:?}: {}\n", kind, count));
        }

        // Language breakdown
        message.push_str("\n💻 Languages:\n");
        let mut sorted_languages: Vec<_> = language_counts.iter().collect();
        sorted_languages.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (lang, count) in sorted_languages {
            message.push_str(&format!("  {}: {} symbols\n", lang, count));
        }

        // Top files by symbol count
        if matches!(self.depth.as_str(), "medium" | "deep") {
            message.push_str("\n📁 Top Files by Symbol Count:\n");
            let mut sorted_files: Vec<_> = file_counts.iter().collect();
            sorted_files.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
            for (file, count) in sorted_files.iter().take(10) {
                let file_name = std::path::Path::new(file)
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new(file))
                    .to_string_lossy();
                message.push_str(&format!("  {}: {} symbols\n", file_name, count));
            }
        }

        Ok(message)
    }

    async fn analyze_dependencies(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().await;

        // Create HashMap for O(1) symbol lookups instead of O(n) linear search
        let search_engine = handler.search_engine.read().await;
        let all_symbols = search_engine.search("*").await.map_err(|e| {
            anyhow::anyhow!("Failed to search for symbols: {}", e)
        })?;
        let symbol_map: HashMap<String, &crate::extractors::Symbol> =
            all_symbols.iter().map(|sr| (sr.symbol.id.clone(), &sr.symbol)).collect();

        let mut relationship_counts = HashMap::new();
        let mut symbol_references = HashMap::new();

        for rel in relationships.iter() {
            *relationship_counts.entry(&rel.kind).or_insert(0) += 1;
            *symbol_references.entry(&rel.to_symbol_id).or_insert(0) += 1;
        }

        let mut message = format!(
            "🔗 Dependency Analysis\n\
            =====================\n\
            Total Relationships: {}\n\n",
            relationships.len()
        );

        // Relationship type breakdown
        message.push_str("🏷️ Relationship Types:\n");
        let mut sorted_rels: Vec<_> = relationship_counts.iter().collect();
        sorted_rels.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (kind, count) in sorted_rels {
            message.push_str(&format!("  {}: {}\n", kind, count));
        }

        // Most referenced symbols
        if matches!(self.depth.as_str(), "medium" | "deep") {
            message.push_str("\n🔥 Most Referenced Symbols:\n");
            let mut sorted_refs: Vec<_> = symbol_references.iter().collect();
            sorted_refs.sort_by_key(|(_, count)| std::cmp::Reverse(**count));

            for (symbol_id, count) in sorted_refs.iter().take(10) {
                if let Some(symbol) = symbol_map.get(&***symbol_id) {
                    message.push_str(&format!("  {} [{}]: {} references\n", symbol.name, format!("{:?}", symbol.kind).to_lowercase(), count));
                }
            }
        }

        Ok(message)
    }

    async fn find_hotspots(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().await;

        // Use SearchEngine instead of O(n) iteration through all symbols
        let search_engine = handler.search_engine.read().await;
        let all_symbols = search_engine.search("*").await.map_err(|e| {
            anyhow::anyhow!("Failed to search for symbols: {}", e)
        })?;

        // Find files with most symbols (complexity hotspots)
        let mut file_symbol_counts = HashMap::new();
        let mut file_relationship_counts = HashMap::new();

        for search_result in all_symbols.iter() {
            let symbol = &search_result.symbol;
            *file_symbol_counts.entry(&symbol.file_path).or_insert(0) += 1;
        }

        for rel in relationships.iter() {
            *file_relationship_counts.entry(&rel.file_path).or_insert(0) += 1;
        }

        let mut message = "🔥 Complexity Hotspots\n=====================\n".to_string();

        message.push_str("📁 Files with Most Symbols:\n");
        let mut sorted_files: Vec<_> = file_symbol_counts.iter().collect();
        sorted_files.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (file, count) in sorted_files.iter().take(10) {
            let file_name = std::path::Path::new(file)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(file))
                .to_string_lossy();
            message.push_str(&format!("  {}: {} symbols\n", file_name, count));
        }

        message.push_str("\n🔗 Files with Most Relationships:\n");
        let mut sorted_rel_files: Vec<_> = file_relationship_counts.iter().collect();
        sorted_rel_files.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (file, count) in sorted_rel_files.iter().take(10) {
            let file_name = std::path::Path::new(file)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(file))
                .to_string_lossy();
            message.push_str(&format!("  {}: {} relationships\n", file_name, count));
        }

        Ok(message)
    }

    async fn trace_relationships(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().await;

        let mut message = "🔍 Relationship Tracing\n=====================\n".to_string();

        if let Some(focus) = &self.focus {
            // Use SearchEngine to find the focused symbol instead of O(n) linear search
            let search_engine = handler.search_engine.read().await;
            let focus_results = search_engine.search(focus).await.map_err(|e| {
                anyhow::anyhow!("Failed to search for focus symbol: {}", e)
            })?;

            // Find exact match for the focused symbol
            if let Some(target_result) = focus_results.iter().find(|sr| sr.symbol.name == *focus) {
                let target_symbol = &target_result.symbol;
                message.push_str(&format!("Tracing relationships for: {}\n\n", focus));

                // Create HashMap for O(1) symbol lookups instead of O(n) for each relationship
                let all_symbols = search_engine.search("*").await.map_err(|e| {
                    anyhow::anyhow!("Failed to get all symbols: {}", e)
                })?;
                let symbol_map: HashMap<String, &crate::extractors::Symbol> =
                    all_symbols.iter().map(|sr| (sr.symbol.id.clone(), &sr.symbol)).collect();

                // Find incoming relationships (what references this symbol)
                let incoming: Vec<_> = relationships.iter()
                    .filter(|rel| rel.to_symbol_id == target_symbol.id)
                    .collect();

                // Find outgoing relationships (what this symbol references)
                let outgoing: Vec<_> = relationships.iter()
                    .filter(|rel| rel.from_symbol_id == target_symbol.id)
                    .collect();

                message.push_str(&format!("← Incoming ({} relationships):\n", incoming.len()));
                for rel in incoming.iter().take(10) {
                    if let Some(from_symbol) = symbol_map.get(&rel.from_symbol_id) {
                        message.push_str(&format!("  {} {} this symbol\n", from_symbol.name, rel.kind));
                    }
                }

                message.push_str(&format!("\n→ Outgoing ({} relationships):\n", outgoing.len()));
                for rel in outgoing.iter().take(10) {
                    if let Some(to_symbol) = symbol_map.get(&rel.to_symbol_id) {
                        message.push_str(&format!("  This symbol {} {}\n", rel.kind, to_symbol.name));
                    }
                }
            } else {
                message.push_str(&format!("❌ Symbol '{}' not found\n", focus));
            }
        } else {
            message.push_str("💡 Use focus parameter to trace a specific symbol\n");
            message.push_str("Example: { \"mode\": \"trace\", \"focus\": \"functionName\" }");
        }

        Ok(message)
    }
}

#[mcp_tool(
    name = "find_logic",
    description = "DISCOVER CORE LOGIC - Filter framework noise, focus on domain business logic",
    title = "Find Business Logic"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindLogicTool {
    pub domain: String,
    pub max_results: i32,
    pub group_by_layer: bool,
    pub min_business_score: f32,
}

impl FindLogicTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🏢 Finding business logic for domain: {}", self.domain);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().await;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable business logic detection.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        let message = format!(
            "🏢 **Business Logic Detection**\n\
            ==============================\n\n\
            🎯 Domain: {}\n\
            📊 Max results: {}\n\
            🏛️ Group by layer: {}\n\
            ⚡ Min business score: {:.1}\n\n\
            🚧 Intelligent business logic detection coming soon!\n\
            🎯 Will filter framework noise and focus on:\n\
            • Core domain logic (high business value)\n\
            • Service layer business rules\n\
            • Domain entities and aggregates\n\
            • Business process workflows\n\
            • Validation and business constraints\n\n\
            💡 Perfect for understanding what the code actually does!",
            self.domain,
            self.max_results,
            self.group_by_layer,
            self.min_business_score
        );

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }
}