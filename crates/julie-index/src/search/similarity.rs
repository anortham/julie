//! Shared semantic similarity search via KNN on stored embeddings.
//!
//! Used by `deep_dive` (similar symbols section) and `fast_refs` (zero-ref fallback).

use anyhow::Result;
use std::collections::HashMap;

use julie_core::Symbol;
use julie_core::database::SymbolDatabase;
use julie_core::embeddings_contract::{SemanticMode, TaggedQueryEmbedding};

/// Minimum similarity score (1.0 - cosine_distance) to include in results.
/// Below this threshold, matches are likely noise.
/// Lowered from 0.5 to 0.35 after embedding enrichment (file paths, callees,
/// field signatures, implementors) improved vector quality. CodeRankEmbed's
/// cosine distances for genuinely related symbols cluster around 0.3-0.5.
pub const MIN_SIMILARITY_SCORE: f32 = 0.35;

/// Entry for a semantically similar symbol.
#[derive(Debug)]
pub struct SimilarEntry {
    pub symbol: Symbol,
    /// Similarity score: 0.0..1.0, higher = more similar (1.0 - cosine_distance)
    pub score: f32,
}

/// Find symbols semantically similar to a tagged query vector.
///
/// Verifies that the tagged query embedding's encoder key and source revision
/// match an active ready generation and match current canonical revision inside
/// the read transaction before issuing KNN.
pub fn find_similar_by_tagged_query(
    db: &SymbolDatabase,
    tagged: &TaggedQueryEmbedding,
    limit: usize,
    min_score: f32,
    semantic_mode: SemanticMode,
) -> Result<Vec<SimilarEntry>> {
    db.with_read_transaction(|tx_db| {
        let current_rev = tx_db.get_latest_canonical_revision_number()?.unwrap_or(0);
        if tagged.source_revision != current_rev {
            if semantic_mode == SemanticMode::Required {
                anyhow::bail!(
                    "SEMANTICS_NOT_READY: query revision {} differs from current canonical revision {}",
                    tagged.source_revision,
                    current_rev
                );
            }
            return Ok(vec![]);
        }

        if !tx_db.embedding_generation_ready(&tagged.encoder_key, tagged.source_revision)? {
            if semantic_mode == SemanticMode::Required {
                anyhow::bail!(
                    "SEMANTICS_NOT_READY: generation for encoder '{}' at rev {} is not ready",
                    tagged.encoder_key,
                    tagged.source_revision
                );
            }
            return Ok(vec![]);
        }

        let knn_results = tx_db.knn_search(&tagged.vector, limit)?;

        let filtered: Vec<(String, f64)> = knn_results
            .into_iter()
            .filter(|(_, distance)| (1.0 - distance) as f32 >= min_score)
            .take(limit)
            .collect();

        if filtered.is_empty() {
            return Ok(vec![]);
        }

        let distances: HashMap<String, f64> = filtered.iter().cloned().collect();
        let symbol_ids: Vec<String> = filtered.iter().map(|(id, _)| id.clone()).collect();

        let symbols = tx_db.get_symbols_by_ids(&symbol_ids)?;
        let symbol_map: HashMap<&str, &Symbol> =
            symbols.iter().map(|s| (s.id.as_str(), s)).collect();

        let mut entries = Vec::new();
        for id in &symbol_ids {
            if let Some(sym) = symbol_map.get(id.as_str()) {
                let distance = distances.get(id).copied().unwrap_or(1.0);
                entries.push(SimilarEntry {
                    symbol: (*sym).clone(),
                    score: (1.0 - distance) as f32,
                });
            }
        }

        Ok(entries)
    })
}

/// Find symbols semantically similar to `symbol` via KNN on stored embeddings.
///
/// Returns empty Vec if the symbol has no embedding (graceful degradation).
/// Filters out self-matches and entries below `min_score`.
pub fn find_similar_symbols(
    db: &SymbolDatabase,
    symbol: &Symbol,
    limit: usize,
    min_score: f32,
) -> Result<Vec<SimilarEntry>> {
    db.with_read_transaction(|tx_db| {
        let Some(ready_gen) = tx_db.get_latest_ready_generation()? else {
            return Ok(vec![]);
        };

        let rev = tx_db.get_latest_canonical_revision_number()?.unwrap_or(0);
        if ready_gen.source_revision != rev {
            return Ok(vec![]);
        }

        if ready_gen.eligible_symbols > 0 && ready_gen.embedded_symbols < ready_gen.eligible_symbols
        {
            return Ok(vec![]);
        }

        // Step 1: Get the symbol's own embedding
        let embedding = match tx_db.get_embedding(&symbol.id)? {
            Some(vec) => vec,
            None => return Ok(vec![]),
        };

        // Step 2: KNN search (fetch extra to account for self + threshold filtering)
        let knn_results = tx_db.knn_search(&embedding, limit + 1)?;

        // Step 3: Filter out self, apply threshold, collect IDs
        let filtered: Vec<(String, f64)> = knn_results
            .into_iter()
            .filter(|(id, _)| id != &symbol.id)
            .filter(|(_, distance)| (1.0 - distance) as f32 >= min_score)
            .take(limit)
            .collect();

        if filtered.is_empty() {
            return Ok(vec![]);
        }

        let distances: HashMap<String, f64> = filtered.iter().cloned().collect();
        let symbol_ids: Vec<String> = filtered.iter().map(|(id, _)| id.clone()).collect();

        // Step 4: Fetch full symbols
        let symbols = tx_db.get_symbols_by_ids(&symbol_ids)?;
        let symbol_map: HashMap<&str, &Symbol> =
            symbols.iter().map(|s| (s.id.as_str(), s)).collect();

        // Step 5: Build entries in KNN order
        let mut entries = Vec::new();
        for id in &symbol_ids {
            if let Some(sym) = symbol_map.get(id.as_str()) {
                let distance = distances.get(id).copied().unwrap_or(1.0);
                entries.push(SimilarEntry {
                    symbol: (*sym).clone(),
                    score: (1.0 - distance) as f32,
                });
            }
        }

        Ok(entries)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use julie_core::database::{FileInfo, SymbolDatabase};
    use julie_extractors::{SymbolKind, Visibility};
    use tempfile::TempDir;

    fn setup_db() -> (TempDir, SymbolDatabase) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();
        db.publish_test_generation("test-encoder", 0, 384).unwrap();

        // FK constraint requires file records before symbols
        for file in &["src/a.rs", "src/b.rs"] {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 500,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 2,
                line_count: 0,
                content: None,
            })
            .unwrap();
        }

        (tmp, db)
    }

    fn make_symbol(id: &str, name: &str, kind: SymbolKind, file: &str, line: u32) -> Symbol {
        Symbol {
            extracted: julie_extractors::Symbol {
                id: id.to_string(),
                name: name.to_string(),
                kind,
                language: "rust".to_string(),
                file_path: file.to_string(),
                start_line: line,
                end_line: line + 10,
                start_column: 0,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                parent_id: None,
                signature: Some(format!("fn {}()", name)),
                visibility: Some(Visibility::Public),
                doc_comment: None,
                content_type: None,
                confidence: None,
                semantic_group: None,
                metadata: None,
                body_span: None,
                body_hash: None,
                annotations: Vec::new(),
            },
            code_context: None,
        }
    }

    #[test]
    fn test_find_similar_returns_results_above_threshold() {
        let (_tmp, mut db) = setup_db();

        let sym_a = make_symbol(
            "sym-a",
            "process_data",
            SymbolKind::Function,
            "src/a.rs",
            10,
        );
        let sym_b = make_symbol("sym-b", "handle_data", SymbolKind::Function, "src/b.rs", 20);
        db.store_symbols(&[sym_a.clone(), sym_b.clone()]).unwrap();

        // Close embeddings -> high similarity score
        let emb_a: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
        let mut emb_b = emb_a.clone();
        emb_b[0] += 0.001;
        db.store_embeddings(&[("sym-a".to_string(), emb_a), ("sym-b".to_string(), emb_b)])
            .unwrap();

        let results = find_similar_symbols(&db, &sym_a, 5, MIN_SIMILARITY_SCORE).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "handle_data");
        assert!(results[0].score >= MIN_SIMILARITY_SCORE);
    }

    #[test]
    fn test_find_similar_filters_below_threshold() {
        let (_tmp, mut db) = setup_db();

        let sym_a = make_symbol(
            "sym-a",
            "process_data",
            SymbolKind::Function,
            "src/a.rs",
            10,
        );
        let sym_b = make_symbol(
            "sym-b",
            "totally_unrelated",
            SymbolKind::Function,
            "src/b.rs",
            20,
        );
        db.store_symbols(&[sym_a.clone(), sym_b.clone()]).unwrap();

        // Distant embeddings -> low similarity score -> should be filtered out
        let emb_a: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
        let emb_b: Vec<f32> = (0..384).map(|i| ((383 - i) as f32) * 0.01).collect();
        db.store_embeddings(&[("sym-a".to_string(), emb_a), ("sym-b".to_string(), emb_b)])
            .unwrap();

        let results = find_similar_symbols(&db, &sym_a, 5, MIN_SIMILARITY_SCORE).unwrap();
        assert!(
            results.is_empty(),
            "Distant embeddings should be filtered out by threshold, got {} results",
            results.len()
        );
    }

    #[test]
    fn test_find_similar_no_embedding_returns_empty() {
        let (_tmp, mut db) = setup_db();

        let sym = make_symbol("sym-a", "lonely", SymbolKind::Function, "src/a.rs", 10);
        db.store_symbols(&[sym.clone()]).unwrap();
        // No embeddings stored

        let results = find_similar_symbols(&db, &sym, 5, MIN_SIMILARITY_SCORE).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_find_similar_excludes_self() {
        let (_tmp, mut db) = setup_db();

        let sym = make_symbol("sym-a", "only_one", SymbolKind::Function, "src/a.rs", 10);
        db.store_symbols(&[sym.clone()]).unwrap();

        let emb: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
        db.store_embeddings(&[("sym-a".to_string(), emb)]).unwrap();

        let results = find_similar_symbols(&db, &sym, 5, MIN_SIMILARITY_SCORE).unwrap();
        assert!(results.is_empty(), "Should not include self");
    }
}
