//! Cross-language call path matching using naming variants and semantic similarity

use crate::database::SymbolDatabase;
use crate::extractors::{RelationshipKind, Symbol};
use crate::handler::JulieServerHandler;
use crate::utils::cross_language_intelligence::generate_naming_variants;
use anyhow::Result;
use std::sync::{Arc, Mutex};
use tracing::debug;

use super::tracing::semantic_neighbors;
use super::types::SemanticMatch;

/// Find cross-language callers using naming variants
pub async fn find_cross_language_callers(
    db: &Arc<Mutex<SymbolDatabase>>,
    symbol: &Symbol,
) -> Result<Vec<Symbol>> {
    let variants = generate_naming_variants(&symbol.name);
    debug!(
        "Generated {} naming variants for {}",
        variants.len(),
        symbol.name
    );

    let mut cross_lang_symbols = Vec::new();
    let db_lock = db.lock().unwrap();

    for variant in variants {
        if variant == symbol.name {
            continue; // Skip original
        }

        // Find symbols with this variant name
        if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
            for variant_symbol in variant_symbols {
                // Only include if different language - naming variant match is sufficient
                if variant_symbol.language != symbol.language {
                    cross_lang_symbols.push(variant_symbol);
                }
            }
        }
    }

    drop(db_lock);

    debug!(
        "Found {} cross-language callers for {}",
        cross_lang_symbols.len(),
        symbol.name
    );

    Ok(cross_lang_symbols)
}

/// Find cross-language callees using naming variants
pub async fn find_cross_language_callees(
    db: &Arc<Mutex<SymbolDatabase>>,
    symbol: &Symbol,
) -> Result<Vec<Symbol>> {
    let variants = generate_naming_variants(&symbol.name);
    debug!(
        "Generated {} naming variants for {}",
        variants.len(),
        symbol.name
    );

    let mut cross_lang_symbols = Vec::new();
    let db_lock = db.lock().unwrap();

    for variant in variants {
        if variant == symbol.name {
            continue;
        }

        // Find symbols with this variant name in different languages
        if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
            for variant_symbol in variant_symbols {
                // Only include if different language - naming variant match is sufficient
                if variant_symbol.language != symbol.language {
                    cross_lang_symbols.push(variant_symbol);
                }
            }
        }
    }

    drop(db_lock);

    debug!(
        "Found {} cross-language callees for {}",
        cross_lang_symbols.len(),
        symbol.name
    );

    Ok(cross_lang_symbols)
}

/// Find semantic cross-language callers using embedding similarity
pub async fn find_semantic_cross_language_callers(
    handler: &JulieServerHandler,
    db: &Arc<Mutex<SymbolDatabase>>,
    vector_store: &Option<Arc<tokio::sync::RwLock<crate::embeddings::vector_store::VectorStore>>>,
    symbol: &Symbol,
) -> Result<Vec<SemanticMatch>> {
    // Use hardcoded semantic limit for good balance between coverage and performance
    let limit = 8;
    let candidates = semantic_neighbors(handler, db, vector_store, symbol, limit).await?;

    if candidates.is_empty() {
        return Ok(vec![]);
    }

    let mut matches = Vec::new();
    let db_lock = db.lock().unwrap();

    for (candidate, similarity) in candidates {
        // Only match cross-language symbols (that's the whole point!)
        if candidate.language == symbol.language {
            continue;
        }

        // Check if there's an existing relationship (for metadata only)
        // But semantic match is VALID even without a relationship!
        let relationships = db_lock.get_relationships_for_symbol(&candidate.id).ok();
        let relationship_kind = relationships
            .and_then(|rels| {
                rels.into_iter().find(|r| {
                    matches!(
                        r.kind,
                        RelationshipKind::Calls | RelationshipKind::References
                    ) && r.from_symbol_id == candidate.id
                        && r.to_symbol_id == symbol.id
                })
            })
            .map(|r| r.kind)
            .unwrap_or(RelationshipKind::Calls); // Default to Calls for semantic bridges

        // Accept ALL cross-language semantic matches above threshold
        // This is how we bridge language gaps!
        matches.push(SemanticMatch {
            symbol: candidate,
            relationship_kind,
            similarity,
        });
    }

    drop(db_lock);

    Ok(matches)
}

/// Find semantic cross-language callees using embedding similarity
pub async fn find_semantic_cross_language_callees(
    handler: &JulieServerHandler,
    db: &Arc<Mutex<SymbolDatabase>>,
    vector_store: &Option<Arc<tokio::sync::RwLock<crate::embeddings::vector_store::VectorStore>>>,
    symbol: &Symbol,
) -> Result<Vec<SemanticMatch>> {
    // Use hardcoded semantic limit for good balance between coverage and performance
    let limit = 8;
    let candidates = semantic_neighbors(handler, db, vector_store, symbol, limit).await?;

    if candidates.is_empty() {
        return Ok(vec![]);
    }

    let mut matches = Vec::new();
    let db_lock = db.lock().unwrap();

    for (candidate, similarity) in candidates {
        // Only match cross-language symbols (that's the whole point!)
        if candidate.language == symbol.language {
            continue;
        }

        // Check if there's an existing relationship (for metadata only)
        // But semantic match is VALID even without a relationship!
        let relationships = db_lock.get_relationships_to_symbol(&candidate.id).ok();
        let relationship_kind = relationships
            .and_then(|rels| {
                rels.into_iter().find(|r| {
                    matches!(
                        r.kind,
                        RelationshipKind::Calls | RelationshipKind::References
                    ) && r.from_symbol_id == symbol.id
                        && r.to_symbol_id == candidate.id
                })
            })
            .map(|r| r.kind)
            .unwrap_or(RelationshipKind::Calls); // Default to Calls for semantic bridges

        // Accept ALL cross-language semantic matches above threshold
        // This is how we bridge language gaps!
        matches.push(SemanticMatch {
            symbol: candidate,
            relationship_kind,
            similarity,
        });
    }

    drop(db_lock);

    Ok(matches)
}
