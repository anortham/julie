//! Unified cross-content search: code symbols + memories in one result set.
//!
//! Queries both the code search index (Tantivy) and memory system (Tantivy BM25 +
//! optional KNN), tags results with `ContentType`, and merges via weighted RRF.
//!
//! The `content_type` filter controls which sources are queried:
//! - `None` (all): queries both code and memories
//! - `Some(Code)`: queries code only
//! - `Some(Memory)`: queries memories only

use std::collections::HashMap;

use anyhow::Result;
use tracing::{debug, warn};

use super::content_type::{ContentType, TaggedResult};
use super::index::{SearchFilter, SearchIndex, SymbolSearchResult};
use crate::database::SymbolDatabase;
use crate::embeddings::EmbeddingProvider;
use crate::memory::index::MemoryIndex;
use crate::memory::MemorySearchResult;

/// A search result that can be either code or memory.
#[derive(Debug, Clone)]
pub enum SearchResultItem {
    Code(SymbolSearchResult),
    Memory(MemorySearchResult),
}

impl SearchResultItem {
    /// Get the unique ID for deduplication during RRF merge.
    pub fn id(&self) -> &str {
        match self {
            SearchResultItem::Code(r) => &r.id,
            SearchResultItem::Memory(r) => &r.id,
        }
    }
}

/// A unified search result: tagged with content type and carrying a merged score.
pub type UnifiedSearchResult = TaggedResult<SearchResultItem>;

/// Options controlling unified search behavior.
pub struct UnifiedSearchOptions {
    /// Filter to a specific content type. `None` = search all.
    pub content_type: Option<ContentType>,
    /// Maximum number of results to return.
    pub limit: usize,
}

/// Run unified cross-content search across code symbols and memories.
///
/// Queries both sources (filtered by `content_type`), tags results, and merges
/// via weighted RRF. Graceful degradation: if memory search fails, code results
/// are still returned (and vice versa).
pub fn unified_search(
    query: &str,
    options: &UnifiedSearchOptions,
    search_index: &SearchIndex,
    memory_index: &MemoryIndex,
    db: &SymbolDatabase,
    embedding_provider: Option<&dyn EmbeddingProvider>,
) -> Result<Vec<UnifiedSearchResult>> {
    let limit = options.limit;
    let search_code = options
        .content_type
        .map_or(true, |ct| ct == ContentType::Code);
    let search_memory = options
        .content_type
        .map_or(true, |ct| ct == ContentType::Memory);

    let over_fetch = if search_code && search_memory {
        limit * 2
    } else {
        limit
    };

    // Step 1: Gather code results (if requested)
    let code_tagged: Vec<UnifiedSearchResult> = if search_code {
        match search_index.search_symbols(query, &SearchFilter::default(), over_fetch) {
            Ok(results) => results
                .results
                .into_iter()
                .map(|r| {
                    let score = r.score;
                    TaggedResult::new(ContentType::Code, SearchResultItem::Code(r), score)
                })
                .collect(),
            Err(e) => {
                warn!("Code search failed in unified search: {e}");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // Step 2: Gather memory results (if requested)
    let memory_tagged: Vec<UnifiedSearchResult> = if search_memory {
        match crate::memory::embedding::hybrid_memory_search(
            query,
            memory_index,
            db,
            embedding_provider,
            over_fetch,
        ) {
            Ok(results) => results
                .into_iter()
                .map(|r| {
                    let score = r.score;
                    TaggedResult::new(ContentType::Memory, SearchResultItem::Memory(r), score)
                })
                .collect(),
            Err(e) => {
                warn!("Memory search failed in unified search: {e}");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // Step 3: If only one source, return directly (no merge needed)
    if !search_code {
        let mut results = memory_tagged;
        results.truncate(limit);
        return Ok(results);
    }
    if !search_memory {
        let mut results = code_tagged;
        results.truncate(limit);
        return Ok(results);
    }

    // Step 4: Merge both sources via RRF
    debug!(
        "Unified search merge: {} code + {} memory → RRF (limit {})",
        code_tagged.len(),
        memory_tagged.len(),
        limit
    );

    let merged = cross_content_rrf_merge(code_tagged, memory_tagged, 60, limit);
    Ok(merged)
}

/// RRF merge for `UnifiedSearchResult` — merges code and memory ranked lists.
///
/// Uses the result's unique ID (prefixed by content type to avoid collisions)
/// for deduplication. Results are ranked by combined RRF score.
fn cross_content_rrf_merge(
    code_results: Vec<UnifiedSearchResult>,
    memory_results: Vec<UnifiedSearchResult>,
    k: u32,
    limit: usize,
) -> Vec<UnifiedSearchResult> {
    if memory_results.is_empty() {
        let mut results = code_results;
        results.truncate(limit);
        return results;
    }
    if code_results.is_empty() {
        let mut results = memory_results;
        results.truncate(limit);
        return results;
    }

    let k_f32 = k as f32;

    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut results_by_key: HashMap<String, UnifiedSearchResult> = HashMap::new();

    // Score code results
    for (i, result) in code_results.into_iter().enumerate() {
        let rank = (i + 1) as f32;
        let rrf_score = 1.0 / (k_f32 + rank);
        let key = format!("code:{}", result.result.id());
        *scores.entry(key.clone()).or_insert(0.0) += rrf_score;
        results_by_key.entry(key).or_insert(result);
    }

    // Score memory results
    for (i, result) in memory_results.into_iter().enumerate() {
        let rank = (i + 1) as f32;
        let rrf_score = 1.0 / (k_f32 + rank);
        let key = format!("memory:{}", result.result.id());
        *scores.entry(key.clone()).or_insert(0.0) += rrf_score;
        results_by_key.entry(key).or_insert(result);
    }

    let mut merged: Vec<UnifiedSearchResult> = results_by_key
        .into_values()
        .map(|mut result| {
            let key = match result.content_type {
                ContentType::Code => format!("code:{}", result.result.id()),
                ContentType::Memory => format!("memory:{}", result.result.id()),
                ContentType::Doc => format!("doc:{}", result.result.id()),
            };
            result.score = scores[&key];
            result
        })
        .collect();

    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    merged.truncate(limit);

    merged
}
