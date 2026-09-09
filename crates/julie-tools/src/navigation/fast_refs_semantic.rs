use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use julie_context::ToolContext;
use julie_core::embeddings_contract::{EmbeddingRequestBudget, SemanticMode, TaggedQueryEmbedding};
use julie_index::search::similarity;
use tracing::debug;

use super::formatting::format_semantic_fallback;
use super::resolution::WorkspaceTarget;

/// When zero references are found, try semantic similarity as a fallback.
/// Embeds the symbol name on the fly and finds similar symbols by vector distance.
/// Returns formatted semantic results or empty string.
/// Skips for some explicit workspace queries when embeddings are unavailable.
///
/// Offloads synchronous broker inference and SQLite KNN to `tokio::task::spawn_blocking`
/// to prevent stalling Tokio runtime worker threads.
pub async fn try_semantic_fallback(
    symbol: &str,
    handler: &dyn ToolContext,
    workspace_target: &WorkspaceTarget,
    budget: Option<EmbeddingRequestBudget>,
    semantic_mode: SemanticMode,
) -> Result<String> {
    if semantic_mode == SemanticMode::Off {
        return Ok(String::new());
    }

    let effective_budget =
        budget.unwrap_or_else(|| EmbeddingRequestBudget::with_timeout(Duration::from_secs(5)));
    effective_budget.check_budget()?;

    // Embedding provider: prefer daemon shared service, fall back to workspace
    let provider = match handler.embedding_provider().await {
        Some(p) => p,
        None => {
            if semantic_mode == SemanticMode::Required {
                anyhow::bail!("SEMANTICS_NOT_READY: Embedding provider unavailable");
            }
            return Ok(String::new());
        }
    };

    // Pooled DB: read-only, no mutation gate required.
    let pooled_db = match workspace_target {
        WorkspaceTarget::Target(target_workspace_id) => {
            debug!("Semantic fallback: workspace '{}'", target_workspace_id);
            match handler
                .get_pooled_database_for_workspace(target_workspace_id)
                .await
            {
                Ok(db) => db,
                Err(e) => {
                    if semantic_mode == SemanticMode::Required {
                        return Err(e);
                    }
                    debug!(
                        "Semantic fallback: DB error for '{}': {}",
                        target_workspace_id, e
                    );
                    return Ok(String::new());
                }
            }
        }
        WorkspaceTarget::Primary => match handler.primary_pooled_database().await {
            Ok(db) => db,
            Err(e) => {
                if semantic_mode == SemanticMode::Required {
                    return Err(e);
                }
                return Ok(String::new());
            }
        },
    };

    let symbol_owned = symbol.to_string();
    let provider_clone = Arc::clone(&provider);

    let similar_res =
        tokio::task::spawn_blocking(move || -> Result<Vec<similarity::SimilarEntry>> {
            effective_budget.check_budget()?;

            let encoder_key = match provider_clone
                .encoder_identity()
                .and_then(|id| id.storage_key())
            {
                Ok(k) => k,
                Err(e) => {
                    if semantic_mode == SemanticMode::Required {
                        return Err(e.context("SEMANTICS_NOT_READY: Encoder identity unavailable"));
                    }
                    debug!("FastRefs semantic fallback: failed to get encoder storage key: {e}");
                    return Ok(Vec::new());
                }
            };

            let source_revision = pooled_db
                .get_latest_canonical_revision_number()?
                .unwrap_or(0);

            if !pooled_db.embedding_generation_ready(&encoder_key, source_revision)? {
                if semantic_mode == SemanticMode::Required {
                    anyhow::bail!(
                        "SEMANTICS_NOT_READY: generation for encoder '{}' at rev {} is not ready",
                        encoder_key,
                        source_revision
                    );
                }
                debug!("FastRefs semantic fallback: embedding generation not ready");
                return Ok(Vec::new());
            }

            let query_vector = match provider_clone.embed_query(&symbol_owned, &effective_budget) {
                Ok(vec) => vec,
                Err(e) => {
                    effective_budget.check_budget()?;
                    if semantic_mode == SemanticMode::Required {
                        return Err(e.context("SEMANTICS_NOT_READY: embed_query failed"));
                    }
                    debug!("FastRefs semantic fallback: embed_query error: {e}");
                    return Ok(Vec::new());
                }
            };

            effective_budget.check_budget()?;

            let tagged = TaggedQueryEmbedding {
                vector: query_vector,
                encoder_key,
                source_revision,
            };

            // Use a lower threshold than MIN_SIMILARITY_SCORE (0.5) because we're
            // comparing a raw symbol name against rich metadata embeddings (kind +
            // name + signature + docstring). Different input domains = lower scores.
            const QUERY_SIMILARITY_THRESHOLD: f32 = 0.2;
            similarity::find_similar_by_tagged_query(
                &pooled_db,
                &tagged,
                5,
                QUERY_SIMILARITY_THRESHOLD,
                semantic_mode,
            )
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))??;

    if !similar_res.is_empty() {
        Ok(format_semantic_fallback(symbol, &similar_res))
    } else {
        Ok(String::new())
    }
}
