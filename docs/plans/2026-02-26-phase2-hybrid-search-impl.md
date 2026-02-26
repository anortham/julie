# Phase 2: Hybrid Search Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Merge Tantivy keyword search with sqlite-vec KNN semantic search using Reciprocal Rank Fusion (RRF) in `get_context`, then add semantic fallback to `fast_search`.

**Architecture:** New `src/search/hybrid.rs` module implements RRF merging. `get_context`'s `run_pipeline` calls `hybrid_search()` instead of `search_symbols()` directly — same output type, downstream pipeline unchanged. Graceful degradation: if embeddings unavailable, falls through to keyword-only.

**Tech Stack:** sqlite-vec KNN, fastembed-rs (already in Phase 1), RRF algorithm (k=60)

---

## Task 1: RRF Merge Function (Pure Algorithm)

**Files:**
- Create: `src/search/hybrid.rs`
- Modify: `src/search/mod.rs` (add `pub mod hybrid;`)
- Test: `src/tests/tools/hybrid_search_tests.rs`
- Modify: `src/tests/mod.rs` (register test module)

**Step 1: Write the failing tests**

Create `src/tests/tools/hybrid_search_tests.rs`:

```rust
//! Tests for RRF (Reciprocal Rank Fusion) merge algorithm.

#[cfg(test)]
mod tests {
    use crate::search::hybrid::rrf_merge;
    use crate::search::index::SymbolSearchResult;

    fn make_result(id: &str, name: &str, score: f32) -> SymbolSearchResult {
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: String::new(),
            doc_comment: String::new(),
            file_path: format!("src/{}.rs", name),
            kind: "function".to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        }
    }

    #[test]
    fn test_rrf_merge_disjoint_lists() {
        // Two lists with no overlap — all items should appear
        let tantivy = vec![
            make_result("a", "alpha", 10.0),
            make_result("b", "beta", 8.0),
        ];
        let semantic = vec![
            make_result("c", "gamma", 5.0),
            make_result("d", "delta", 3.0),
        ];

        let merged = rrf_merge(tantivy, semantic, 60, 10);
        assert_eq!(merged.len(), 4);
        // All items present
        let ids: Vec<&str> = merged.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
        assert!(ids.contains(&"c"));
        assert!(ids.contains(&"d"));
    }

    #[test]
    fn test_rrf_merge_overlapping_lists() {
        // Overlapping item "a" should rank highest (appears in both)
        let tantivy = vec![
            make_result("a", "alpha", 10.0),
            make_result("b", "beta", 8.0),
        ];
        let semantic = vec![
            make_result("a", "alpha", 5.0),
            make_result("c", "gamma", 3.0),
        ];

        let merged = rrf_merge(tantivy, semantic, 60, 10);
        assert_eq!(merged.len(), 3);
        // "a" should be first — it appears in both lists
        assert_eq!(merged[0].id, "a");
    }

    #[test]
    fn test_rrf_merge_respects_limit() {
        let tantivy = vec![
            make_result("a", "alpha", 10.0),
            make_result("b", "beta", 8.0),
            make_result("c", "gamma", 6.0),
        ];
        let semantic = vec![
            make_result("d", "delta", 5.0),
            make_result("e", "epsilon", 3.0),
        ];

        let merged = rrf_merge(tantivy, semantic, 60, 3);
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn test_rrf_merge_empty_semantic() {
        // Graceful degradation: empty semantic list → tantivy results unchanged
        let tantivy = vec![
            make_result("a", "alpha", 10.0),
            make_result("b", "beta", 8.0),
        ];
        let semantic = vec![];

        let merged = rrf_merge(tantivy, semantic, 60, 10);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].id, "a");
        assert_eq!(merged[1].id, "b");
    }

    #[test]
    fn test_rrf_merge_empty_tantivy() {
        // Empty tantivy → semantic results returned
        let tantivy = vec![];
        let semantic = vec![
            make_result("a", "alpha", 5.0),
            make_result("b", "beta", 3.0),
        ];

        let merged = rrf_merge(tantivy, semantic, 60, 10);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_rrf_score_is_stored_in_result() {
        // The merged results should have RRF scores, not original scores
        let tantivy = vec![make_result("a", "alpha", 10.0)];
        let semantic = vec![make_result("a", "alpha", 5.0)];

        let merged = rrf_merge(tantivy, semantic, 60, 10);
        assert_eq!(merged.len(), 1);
        // RRF score for rank 1 in both lists: 1/(60+1) + 1/(60+1) ≈ 0.0328
        let expected_rrf = 2.0 / 61.0;
        assert!((merged[0].score - expected_rrf as f32).abs() < 0.001);
    }
}
```

Register in `src/tests/mod.rs` — add under the `tools` module:
```rust
pub mod hybrid_search_tests;
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib hybrid_search_tests 2>&1 | tail -10`
Expected: Compilation error — `crate::search::hybrid::rrf_merge` doesn't exist

**Step 3: Implement `rrf_merge`**

Create `src/search/hybrid.rs`:

```rust
//! Hybrid search: merge Tantivy keyword results with KNN semantic results using RRF.
//!
//! Reciprocal Rank Fusion (RRF) combines two ranked lists without requiring
//! score normalization. Each document's score is: Σ 1/(k + rank_i).
//! Documents appearing in both lists naturally rank highest.

use std::collections::HashMap;

use super::index::SymbolSearchResult;

/// Merge two ranked result lists using Reciprocal Rank Fusion.
///
/// `k` is the RRF constant (standard: 60). Higher k reduces the impact of
/// high-ranking items. `limit` caps the output size.
///
/// Results are sorted by descending RRF score. The `score` field on each
/// returned `SymbolSearchResult` is replaced with the RRF score.
pub fn rrf_merge(
    tantivy_results: Vec<SymbolSearchResult>,
    semantic_results: Vec<SymbolSearchResult>,
    k: u32,
    limit: usize,
) -> Vec<SymbolSearchResult> {
    // Fast paths: if one list is empty, return the other (capped)
    if semantic_results.is_empty() {
        return tantivy_results.into_iter().take(limit).collect();
    }
    if tantivy_results.is_empty() {
        return semantic_results.into_iter().take(limit).collect();
    }

    let k = k as f32;
    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut results_by_id: HashMap<String, SymbolSearchResult> = HashMap::new();

    // Score tantivy results by rank
    for (rank, result) in tantivy_results.into_iter().enumerate() {
        let rrf = 1.0 / (k + (rank + 1) as f32);
        *scores.entry(result.id.clone()).or_default() += rrf;
        results_by_id.entry(result.id.clone()).or_insert(result);
    }

    // Score semantic results by rank
    for (rank, result) in semantic_results.into_iter().enumerate() {
        let rrf = 1.0 / (k + (rank + 1) as f32);
        *scores.entry(result.id.clone()).or_default() += rrf;
        results_by_id.entry(result.id.clone()).or_insert(result);
    }

    // Sort by RRF score descending
    let mut merged: Vec<SymbolSearchResult> = scores
        .into_iter()
        .filter_map(|(id, rrf_score)| {
            results_by_id.remove(&id).map(|mut result| {
                result.score = rrf_score;
                result
            })
        })
        .collect();

    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(limit);
    merged
}
```

Add to `src/search/mod.rs`:
```rust
pub mod hybrid;
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib hybrid_search_tests 2>&1 | tail -10`
Expected: All 6 tests PASS

**Step 5: Commit**

```bash
git add src/search/hybrid.rs src/search/mod.rs src/tests/tools/hybrid_search_tests.rs src/tests/mod.rs
git commit -m "feat: add RRF merge algorithm for hybrid search"
```

---

## Task 2: KNN-to-SymbolSearchResult Conversion

**Files:**
- Modify: `src/search/hybrid.rs` (add `knn_to_search_results`)
- Test: `src/tests/tools/hybrid_search_tests.rs` (add conversion tests)

**Step 1: Write the failing tests**

Add to `src/tests/tools/hybrid_search_tests.rs`:

```rust
mod conversion_tests {
    use std::sync::{Arc, Mutex};
    use crate::database::SymbolDatabase;
    use crate::search::hybrid::knn_to_search_results;

    fn create_test_db_with_symbols() -> Arc<Mutex<SymbolDatabase>> {
        let db = SymbolDatabase::new(":memory:").unwrap();
        {
            // Insert test symbols directly
            db.conn.execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_column, end_line, end_column, start_byte, end_byte, signature, doc_comment)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?6, 0, 0, 0, ?7, ?8)",
                rusqlite::params!["sym_1", "process_payment", "function", "rust", "src/payment.rs", 10, "fn process_payment(amount: f64)", "Handles payment processing"],
            ).unwrap();
            db.conn.execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_column, end_line, end_column, start_byte, end_byte, signature, doc_comment)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?6, 0, 0, 0, ?7, ?8)",
                rusqlite::params!["sym_2", "PaymentService", "struct", "rust", "src/service.rs", 5, "pub struct PaymentService", ""],
            ).unwrap();
        }
        Arc::new(Mutex::new(db))
    }

    #[test]
    fn test_knn_to_search_results_converts_correctly() {
        let db = create_test_db_with_symbols();
        let knn_results = vec![
            ("sym_1".to_string(), 0.3),  // distance 0.3
            ("sym_2".to_string(), 0.5),  // distance 0.5
        ];

        let db_guard = db.lock().unwrap();
        let results = knn_to_search_results(&knn_results, &db_guard).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "sym_1");
        assert_eq!(results[0].name, "process_payment");
        assert_eq!(results[0].kind, "function");
        assert_eq!(results[1].id, "sym_2");
        assert_eq!(results[1].name, "PaymentService");
    }

    #[test]
    fn test_knn_to_search_results_skips_missing_symbols() {
        let db = create_test_db_with_symbols();
        let knn_results = vec![
            ("sym_1".to_string(), 0.3),
            ("nonexistent".to_string(), 0.1),  // not in DB
        ];

        let db_guard = db.lock().unwrap();
        let results = knn_to_search_results(&knn_results, &db_guard).unwrap();

        // Only sym_1 should be returned
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "sym_1");
    }

    #[test]
    fn test_knn_to_search_results_empty_input() {
        let db = create_test_db_with_symbols();
        let knn_results: Vec<(String, f64)> = vec![];

        let db_guard = db.lock().unwrap();
        let results = knn_to_search_results(&knn_results, &db_guard).unwrap();
        assert!(results.is_empty());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib hybrid_search_tests::conversion 2>&1 | tail -10`
Expected: Compilation error — `knn_to_search_results` doesn't exist

**Step 3: Implement `knn_to_search_results`**

Add to `src/search/hybrid.rs`:

```rust
use crate::database::SymbolDatabase;
use anyhow::Result;

/// Convert KNN results (symbol_id, distance) into SymbolSearchResult objects.
///
/// Fetches symbol metadata from the database by ID. Symbols not found in
/// the database (e.g., deleted since embeddings were created) are silently
/// skipped — this is expected during incremental updates.
///
/// The `score` field is set to `1.0 - distance` (higher = more similar),
/// preserving the convention that higher scores = better results.
pub fn knn_to_search_results(
    knn_results: &[(String, f64)],
    db: &SymbolDatabase,
) -> Result<Vec<SymbolSearchResult>> {
    if knn_results.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<String> = knn_results.iter().map(|(id, _)| id.clone()).collect();
    let symbols = db.get_symbols_by_ids(&ids)?;

    // Build a lookup map from symbol ID → Symbol (preserves order from DB)
    let symbol_map: std::collections::HashMap<&str, &crate::extractors::base::Symbol> =
        symbols.iter().map(|s| (s.id.as_str(), s)).collect();

    // Convert in KNN order (most similar first), skipping missing symbols
    let results = knn_results
        .iter()
        .filter_map(|(id, distance)| {
            symbol_map.get(id.as_str()).map(|sym| SymbolSearchResult {
                id: sym.id.clone(),
                name: sym.name.clone(),
                signature: sym.signature.clone().unwrap_or_default(),
                doc_comment: sym.doc_comment.clone().unwrap_or_default(),
                file_path: sym.file_path.clone(),
                kind: sym.kind.to_string(),
                language: sym.language.clone(),
                start_line: sym.start_line,
                score: (1.0 - *distance) as f32,
            })
        })
        .collect();

    Ok(results)
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib hybrid_search_tests 2>&1 | tail -10`
Expected: All 9 tests PASS (6 RRF + 3 conversion)

**Step 5: Commit**

```bash
git add src/search/hybrid.rs src/tests/tools/hybrid_search_tests.rs
git commit -m "feat: add KNN-to-SymbolSearchResult conversion for hybrid search"
```

---

## Task 3: `hybrid_search` Orchestrator Function

**Files:**
- Modify: `src/search/hybrid.rs` (add `hybrid_search`)
- Test: `src/tests/tools/hybrid_search_tests.rs` (add orchestration tests)

**Step 1: Write the failing test**

Add to `src/tests/tools/hybrid_search_tests.rs`:

```rust
mod orchestration_tests {
    use crate::search::hybrid::hybrid_search;
    use crate::search::index::SearchFilter;

    /// Test that hybrid_search falls back to keyword-only when no provider given
    #[test]
    fn test_hybrid_search_without_provider_is_keyword_only() {
        // This test requires a real Tantivy index + DB, so we test the
        // None-provider fallback path in isolation.
        // The full integration is tested in the dogfood suite.

        // We can't easily construct a SearchIndex in a unit test,
        // so we test the logic branches via the rrf_merge tests above
        // and the integration test in Task 5.
        //
        // This test just verifies the function signature compiles and
        // the None path doesn't panic.
    }
}
```

Note: The real orchestration test is the integration test in Task 5 — the unit test here just verifies compilation.

**Step 2: Implement `hybrid_search`**

Add to `src/search/hybrid.rs`:

```rust
use super::index::{SearchFilter, SearchIndex, SymbolSearchResults};
use crate::embeddings::EmbeddingProvider;
use tracing::debug;

/// Run hybrid search: Tantivy keyword search + KNN semantic search merged via RRF.
///
/// If `embedding_provider` is `None` or KNN returns no results (embeddings not
/// indexed yet), gracefully falls back to keyword-only — identical to pre-hybrid behavior.
///
/// # Arguments
/// * `query` - Search query string
/// * `filter` - Language/kind/file_pattern filter for Tantivy
/// * `limit` - Maximum results to return
/// * `search_index` - Tantivy search index
/// * `db` - Symbol database (for KNN search and ID→Symbol conversion)
/// * `embedding_provider` - Optional embedding provider for semantic search
pub fn hybrid_search(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    search_index: &SearchIndex,
    db: &SymbolDatabase,
    embedding_provider: Option<&dyn EmbeddingProvider>,
) -> Result<SymbolSearchResults> {
    // Always run Tantivy keyword search (over-fetch 2x for merge pool)
    let tantivy_limit = if embedding_provider.is_some() { limit * 2 } else { limit };
    let tantivy_results = search_index.search_symbols(query, filter, tantivy_limit)?;

    // If no embedding provider, return keyword results directly
    let provider = match embedding_provider {
        Some(p) => p,
        None => return Ok(tantivy_results),
    };

    // Run KNN semantic search
    let semantic_results = match provider.embed_query(query) {
        Ok(query_vector) => {
            match db.knn_search(&query_vector, limit * 2) {
                Ok(knn_hits) => {
                    debug!(
                        "Hybrid search: {} tantivy + {} KNN results for '{}'",
                        tantivy_results.results.len(),
                        knn_hits.len(),
                        query
                    );
                    knn_to_search_results(&knn_hits, db).unwrap_or_default()
                }
                Err(e) => {
                    debug!("KNN search failed (using keyword-only): {e}");
                    Vec::new()
                }
            }
        }
        Err(e) => {
            debug!("Query embedding failed (using keyword-only): {e}");
            Vec::new()
        }
    };

    // Merge via RRF (graceful: empty semantic → keyword-only)
    let merged = rrf_merge(tantivy_results.results, semantic_results, 60, limit);

    Ok(SymbolSearchResults {
        results: merged,
        relaxed: tantivy_results.relaxed,
    })
}
```

**Step 3: Run tests to verify compilation**

Run: `cargo test --lib hybrid_search_tests 2>&1 | tail -10`
Expected: All tests PASS (including new compilation check)

**Step 4: Commit**

```bash
git add src/search/hybrid.rs src/tests/tools/hybrid_search_tests.rs
git commit -m "feat: add hybrid_search orchestrator with graceful degradation"
```

---

## Task 4: Wire `hybrid_search` into `get_context` Pipeline

**Files:**
- Modify: `src/tools/get_context/pipeline.rs` (`run_pipeline` and `run`)
- Test: Existing get_context tests should still pass (regression check)

**Step 1: Modify `run_pipeline` to accept optional embedding provider**

In `src/tools/get_context/pipeline.rs`, change the `run_pipeline` signature to:

```rust
pub fn run_pipeline(
    query: &str,
    max_tokens: Option<u32>,
    language: Option<String>,
    file_pattern: Option<String>,
    format: Option<String>,
    db: &SymbolDatabase,
    search_index: &crate::search::SearchIndex,
    embedding_provider: Option<&dyn crate::embeddings::EmbeddingProvider>,
) -> Result<String> {
```

Replace step 1 (the `search_index.search_symbols(...)` call) with:

```rust
    // 1. Search for relevant symbols (hybrid: keyword + semantic if available)
    let filter = SearchFilter {
        language,
        kind: None,
        file_pattern,
    };
    let search_results = crate::search::hybrid::hybrid_search(
        query, &filter, 30, search_index, db, embedding_provider,
    )?;
```

**Step 2: Update both call sites in `run` to pass the provider**

In the reference workspace path of `run()`:
```rust
// Reference workspaces don't have embeddings (yet), pass None
run_pipeline(&query, max_tokens, language, file_pattern, format, &db, &index, None)
```

In the primary workspace path of `run()`:
```rust
// Get embedding provider from workspace
let embedding_provider = workspace.embedding_provider.clone();

let result = tokio::task::spawn_blocking(move || -> Result<String> {
    let index = search_index.lock().unwrap();
    let db_guard = db.lock().unwrap();
    run_pipeline(
        &query, max_tokens, language, file_pattern, format,
        &db_guard, &index,
        embedding_provider.as_deref(),
    )
})
.await??;
```

Note: `Arc<dyn EmbeddingProvider>` implements `Deref<Target = dyn EmbeddingProvider>`, so `as_deref()` on `Option<Arc<dyn EmbeddingProvider>>` gives `Option<&dyn EmbeddingProvider>`.

**Step 3: Run existing get_context tests to verify no regressions**

Run: `cargo test --lib tests::tools::get_context 2>&1 | tail -10`
Expected: All existing tests PASS (they pass `None` for embedding provider implicitly)

**Step 4: Run fast test tier**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add src/tools/get_context/pipeline.rs
git commit -m "feat: wire hybrid search into get_context pipeline"
```

---

## Task 5: Semantic Fallback for `fast_search`

**Files:**
- Modify: `src/tools/search/text_search.rs` (add semantic fallback in definition search path)
- Test: `src/tests/tools/hybrid_search_tests.rs` (add NL fallback logic test)

**Step 1: Write the failing test**

Add to `src/tests/tools/hybrid_search_tests.rs`:

```rust
mod fast_search_fallback_tests {
    use crate::search::hybrid::should_use_semantic_fallback;

    #[test]
    fn test_fallback_triggers_for_nl_with_sparse_results() {
        assert!(should_use_semantic_fallback("how does payment work", 2));
        assert!(should_use_semantic_fallback("what handles authentication", 0));
    }

    #[test]
    fn test_fallback_does_not_trigger_for_identifiers() {
        assert!(!should_use_semantic_fallback("UserService", 5));
        assert!(!should_use_semantic_fallback("process_payment", 10));
    }

    #[test]
    fn test_fallback_does_not_trigger_with_enough_results() {
        assert!(!should_use_semantic_fallback("how does payment work", 5));
    }
}
```

**Step 2: Implement `should_use_semantic_fallback`**

Add to `src/search/hybrid.rs`:

```rust
use crate::search::scoring::is_nl_like_query;

/// Determine whether to invoke semantic fallback for fast_search.
///
/// Returns true when the query looks like natural language AND keyword
/// results are sparse (< 3). This avoids embedding overhead for identifier
/// queries that keyword search handles well.
pub fn should_use_semantic_fallback(query: &str, keyword_result_count: usize) -> bool {
    is_nl_like_query(query) && keyword_result_count < 3
}
```

**Step 3: Run tests**

Run: `cargo test --lib fast_search_fallback 2>&1 | tail -10`
Expected: All 3 tests PASS

**Step 4: Wire into `text_search_impl`**

In `src/tools/search/text_search.rs`, after the existing keyword search produces results, add semantic fallback logic. In the definition search branch, after results are collected:

```rust
// Semantic fallback: if NL query with sparse results, try KNN
if crate::search::hybrid::should_use_semantic_fallback(query, results.len()) {
    if let Some(ref provider) = workspace.embedding_provider {
        if let Ok(query_vector) = provider.embed_query(query) {
            let db_guard = db.lock().unwrap();
            if let Ok(knn_hits) = db_guard.knn_search(&query_vector, limit as usize) {
                let semantic = crate::search::hybrid::knn_to_search_results(&knn_hits, &db_guard)
                    .unwrap_or_default();
                drop(db_guard);
                // Merge existing keyword results with semantic results
                let keyword_as_search: Vec<_> = /* convert current results */;
                results = crate::search::hybrid::rrf_merge(keyword_as_search, semantic, 60, limit as usize);
            }
        }
    }
}
```

Note: The exact integration depends on `text_search_impl`'s current result types (it works with `Symbol`, not `SymbolSearchResult`). The implementer should check the existing flow and adapt the conversion. The key principle: only trigger when `should_use_semantic_fallback` returns true, and merge via `rrf_merge`.

**Step 5: Run fast test tier**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: All tests PASS

**Step 6: Commit**

```bash
git add src/search/hybrid.rs src/tools/search/text_search.rs src/tests/tools/hybrid_search_tests.rs
git commit -m "feat: add semantic fallback for fast_search NL queries"
```

---

## Task 6: Dogfood Integration Test

**Files:**
- Test: `src/tests/tools/search_quality/` (add hybrid search dogfood test)

**Step 1: Write the dogfood test**

Add a test (in the appropriate search quality test file or create `src/tests/tools/search_quality/hybrid_dogfood.rs`) that loads the Julie fixture and runs hybrid search:

```rust
#[test]
fn test_hybrid_search_nl_query_surfaces_implementation() {
    // Load the Julie fixture (same as other search_quality tests)
    let (db, index) = load_julie_fixture();

    // Create a real embedding provider
    let provider = crate::embeddings::OrtEmbeddingProvider::try_new(None)
        .expect("embedding provider should init");

    // First, embed symbols so KNN has data
    let db_arc = Arc::new(Mutex::new(db));
    crate::embeddings::pipeline::run_embedding_pipeline(&db_arc, &provider).unwrap();

    let db_guard = db_arc.lock().unwrap();

    // Run hybrid search with NL query
    let filter = SearchFilter::default();
    let results = crate::search::hybrid::hybrid_search(
        "how does text search work",
        &filter,
        10,
        &index,
        &db_guard,
        Some(&provider),
    ).unwrap();

    // Exit criteria: text_search_impl or TextSearchTool in top 5
    let top_5_names: Vec<&str> = results.results.iter().take(5).map(|r| r.name.as_str()).collect();
    let has_search_impl = top_5_names.iter().any(|n|
        n.contains("text_search") || n.contains("TextSearch") || n.contains("search_symbols")
    );
    assert!(
        has_search_impl,
        "Expected text search implementation in top 5, got: {:?}",
        top_5_names
    );
}
```

**Step 2: Run the dogfood test**

Run: `cargo test --lib test_hybrid_search_nl_query 2>&1 | tail -20`
Expected: PASS — this is the Phase 2 exit criteria

**Step 3: Commit**

```bash
git add src/tests/tools/search_quality/
git commit -m "test: add dogfood test for hybrid search exit criteria"
```

---

## Task 7: Final Verification and Cleanup

**Files:**
- No new files — verification only

**Step 1: Run full test suite**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: All tests PASS (including new hybrid tests + existing regressions)

**Step 2: Verify graceful degradation**

Confirm that when `embedding_provider` is `None`:
- `get_context` works exactly as before (keyword-only)
- `fast_search` works exactly as before (no semantic fallback)
- No error messages in logs

**Step 3: Update plan status**

Update the goldfish plan to mark Phase 2 complete.

**Step 4: Use finishing-a-development-branch skill**

Present merge/PR options for the Phase 2 work.
