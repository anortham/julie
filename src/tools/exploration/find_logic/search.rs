use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::extractors::SymbolKind;
use crate::extractors::base::Symbol;
use crate::handler::JulieServerHandler;
use crate::search::{SearchFilter, SearchIndex, SymbolSearchResult};

use super::FindLogicTool;

// Maximum candidates for graph analysis (prevents pathological cases)
pub const MAX_GRAPH_ANALYSIS_CANDIDATES: usize = 100;

/// Maximum search results per keyword query
const MAX_KEYWORD_RESULTS: usize = 200;

/// Convert a SymbolSearchResult from Tantivy into a Symbol
fn symbol_from_search_result(result: SymbolSearchResult) -> Symbol {
    Symbol {
        id: result.id,
        name: result.name,
        kind: SymbolKind::from_string(&result.kind),
        language: result.language,
        file_path: result.file_path,
        start_line: result.start_line,
        signature: if result.signature.is_empty() {
            None
        } else {
            Some(result.signature)
        },
        doc_comment: if result.doc_comment.is_empty() {
            None
        } else {
            Some(result.doc_comment)
        },
        start_column: 0,
        end_line: 0,
        end_column: 0,
        start_byte: 0,
        end_byte: 0,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: Some(result.score),
        code_context: None,
        content_type: None,
    }
}

/// Search symbols via Tantivy index with the given query
fn search_tantivy(
    search_index: &Arc<std::sync::Mutex<SearchIndex>>,
    query: &str,
    limit: usize,
) -> Result<Vec<Symbol>> {
    let index = match search_index.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("Search index mutex poisoned, recovering: {}", poisoned);
            poisoned.into_inner()
        }
    };
    let filter = SearchFilter {
        language: None,
        kind: None,
        file_pattern: None,
    };
    let results = index.search_symbols(query, &filter, limit)?;
    Ok(results.into_iter().map(symbol_from_search_result).collect())
}

impl FindLogicTool {
    /// Tier 1: Search using Tantivy for fast keyword matching
    pub async fn search_by_keywords(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        let domain_keywords: Vec<&str> = self.domain.split_whitespace().collect();
        let mut keyword_results: Vec<Symbol> = Vec::new();

        debug!("🔍 Using Tantivy keyword search");
        if let Ok(Some(workspace)) = handler.get_workspace().await {
            if let Some(search_index) = workspace.search_index.as_ref() {
                for keyword in &domain_keywords {
                    if let Ok(results) = search_tantivy(search_index, keyword, MAX_KEYWORD_RESULTS)
                    {
                        for mut symbol in results {
                            symbol.confidence = Some(0.5);
                            keyword_results.push(symbol);
                        }
                    }
                }
            }
        }

        debug!(
            "🔍 Tantivy keyword search found {} candidates",
            keyword_results.len()
        );
        Ok(keyword_results)
    }

    /// Tier 2: Find architectural patterns using Tantivy search
    pub async fn find_architectural_patterns(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<Vec<Symbol>> {
        let mut pattern_matches: Vec<Symbol> = Vec::new();
        let domain_keywords: Vec<&str> = self.domain.split_whitespace().collect();

        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available"))?;
        let search_index = workspace
            .search_index
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Search index not available"))?;

        // Pattern 1: Find Service/Controller/Handler classes
        let architectural_patterns = vec![
            "Service",
            "Controller",
            "Handler",
            "Manager",
            "Processor",
            "Repository",
            "Provider",
            "Factory",
            "Builder",
            "Validator",
        ];

        for pattern in &architectural_patterns {
            for keyword in &domain_keywords {
                let query = format!("{}{}", keyword, pattern);
                if let Ok(results) = search_tantivy(search_index, &query, MAX_KEYWORD_RESULTS) {
                    for mut symbol in results {
                        if matches!(symbol.kind, SymbolKind::Class | SymbolKind::Struct) {
                            symbol.confidence = Some(0.8);
                            symbol.semantic_group = Some(pattern.to_lowercase());
                            pattern_matches.push(symbol);
                        }
                    }
                }
            }
        }

        // Pattern 2: Find business logic method names
        let business_method_prefixes = vec![
            "process",
            "validate",
            "calculate",
            "execute",
            "handle",
            "create",
            "update",
            "delete",
            "get",
            "find",
            "fetch",
        ];

        for prefix in &business_method_prefixes {
            for keyword in &domain_keywords {
                let query = format!("{}{}", prefix, keyword);
                if let Ok(results) = search_tantivy(search_index, &query, MAX_KEYWORD_RESULTS) {
                    for mut symbol in results {
                        if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                            symbol.confidence = Some(0.7);
                            pattern_matches.push(symbol);
                        }
                    }
                }
            }
        }

        debug!(
            "🌳 AST pattern recognition found {} architectural matches",
            pattern_matches.len()
        );
        Ok(pattern_matches)
    }

    /// Tier 3: Apply path-based intelligence to boost business layer symbols
    pub fn apply_path_intelligence(&self, symbols: &mut [Symbol]) {
        for symbol in symbols.iter_mut() {
            let path_lower = symbol.file_path.to_lowercase();
            let mut path_boost: f32 = 0.0;

            // Business logic layers (HIGH priority)
            if path_lower.contains("/services/") || path_lower.contains("/service/") {
                path_boost += 0.25;
                symbol.semantic_group = Some("service".to_string());
            } else if path_lower.contains("/domain/")
                || path_lower.contains("/models/")
                || path_lower.contains("/entities/")
            {
                path_boost += 0.2;
                symbol.semantic_group = Some("domain".to_string());
            } else if path_lower.contains("/controllers/")
                || path_lower.contains("/handlers/")
                || path_lower.contains("/api/")
            {
                path_boost += 0.15;
                symbol.semantic_group = Some("controller".to_string());
            } else if path_lower.contains("/repositories/") || path_lower.contains("/dao/") {
                path_boost += 0.1;
                symbol.semantic_group = Some("repository".to_string());
            }

            // Infrastructure/utilities (PENALTY - not business logic)
            if path_lower.contains("/utils/")
                || path_lower.contains("/helpers/")
                || path_lower.contains("/lib/")
                || path_lower.contains("/vendor/")
            {
                path_boost -= 0.3;
                symbol.semantic_group = Some("utility".to_string());
            }

            // Tests (PENALTY - not production business logic)
            if path_lower.contains("/test")
                || path_lower.contains("_test")
                || path_lower.contains(".test.")
                || path_lower.contains(".spec.")
            {
                path_boost -= 0.5;
                symbol.semantic_group = Some("test".to_string());
            }

            // Apply boost to confidence score
            let current_score = symbol.confidence.unwrap_or(0.5);
            symbol.confidence = Some((current_score + path_boost).clamp(0.0, 1.0));
        }

        debug!(
            "🗂️ Applied path-based intelligence to {} symbols",
            symbols.len()
        );
    }

    /// Tier 4: Analyze relationship graph to boost important business entities
    pub async fn analyze_business_importance(
        &self,
        symbols: &mut [Symbol],
        handler: &JulieServerHandler,
    ) -> Result<()> {
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available"))?;
        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available"))?;
        let db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("Database mutex poisoned, recovering: {}", poisoned);
                poisoned.into_inner()
            }
        };

        // Build a reference count map for all symbols
        let mut reference_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        // 🚀 CRITICAL FIX: Use batched query instead of N+1 individual queries
        // Collect all symbol IDs for batch query (same fix as FastRefsTool)
        let symbol_ids: Vec<String> = symbols.iter().map(|s| s.id.clone()).collect();

        // Single batched query - O(1) database call instead of O(N)
        if let Ok(all_relationships) = db_lock.get_relationships_to_symbols(&symbol_ids) {
            // Count incoming references for each symbol from batched results
            for relationship in all_relationships {
                *reference_counts
                    .entry(relationship.to_symbol_id.clone())
                    .or_insert(0) += 1;
            }
        }

        // Apply centrality boost based on reference counts
        for symbol in symbols.iter_mut() {
            if let Some(&ref_count) = reference_counts.get(&symbol.id) {
                if ref_count > 0 {
                    // Logarithmic boost for reference count (popular symbols get higher scores)
                    let centrality_boost = (ref_count as f32).ln() * 0.05;
                    let current_score = symbol.confidence.unwrap_or(0.5);
                    symbol.confidence = Some((current_score + centrality_boost).clamp(0.0, 1.0));

                    debug!(
                        "📊 Symbol {} has {} references, boost: {:.2}",
                        symbol.name, ref_count, centrality_boost
                    );
                }
            }
        }

        debug!("📊 Applied relationship graph centrality analysis");
        Ok(())
    }

    /// Deduplicate symbols and rank by business score
    pub fn deduplicate_and_rank(&self, mut symbols: Vec<Symbol>) -> Vec<Symbol> {
        // Sort by ID first for deduplication
        symbols.sort_by(|a, b| a.id.cmp(&b.id));
        symbols.dedup_by(|a, b| a.id == b.id);

        // Calculate final business scores with domain keyword matching
        let domain_keywords: Vec<&str> = self.domain.split_whitespace().collect();
        for symbol in symbols.iter_mut() {
            let keyword_score = self.calculate_domain_keyword_score(symbol, &domain_keywords);
            let current_score = symbol.confidence.unwrap_or(0.0);

            // Combine existing intelligence scores with keyword matching
            symbol.confidence = Some((current_score * 0.7 + keyword_score * 0.3).clamp(0.0, 1.0));
        }

        // Sort by final business score (descending)
        symbols.sort_by(|a, b| {
            let score_a = a.confidence.unwrap_or(0.0);
            let score_b = b.confidence.unwrap_or(0.0);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        debug!(
            "✨ Deduplicated and ranked {} final candidates",
            symbols.len()
        );
        symbols
    }

    /// Calculate score based on domain keyword matching
    pub fn calculate_domain_keyword_score(&self, symbol: &Symbol, domain_keywords: &[&str]) -> f32 {
        let mut score: f32 = 0.0;

        // Check symbol name (highest weight)
        let name_lower = symbol.name.to_lowercase();
        for keyword in domain_keywords {
            if name_lower.contains(&keyword.to_lowercase()) {
                score += 0.5;
            }
        }

        // Check file path
        let path_lower = symbol.file_path.to_lowercase();
        for keyword in domain_keywords {
            if path_lower.contains(&keyword.to_lowercase()) {
                score += 0.2;
            }
        }

        // Check documentation
        if let Some(doc) = &symbol.doc_comment {
            let doc_lower = doc.to_lowercase();
            for keyword in domain_keywords {
                if doc_lower.contains(&keyword.to_lowercase()) {
                    score += 0.2;
                }
            }
        }

        // Check signature
        if let Some(sig) = &symbol.signature {
            let sig_lower = sig.to_lowercase();
            for keyword in domain_keywords {
                if sig_lower.contains(&keyword.to_lowercase()) {
                    score += 0.1;
                }
            }
        }

        score.min(1.0)
    }
}
