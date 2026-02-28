#![cfg(feature = "embeddings-ort")]

//! Semantic Similarity Dogfood Test — Phase 3 Exit Criteria
//!
//! Proves that `deep_dive` at "full" depth returns meaningful semantically
//! similar symbols when embeddings are available in the fixture DB.
//!
//! **This test is SLOW** (~30-60s) because it:
//! 1. Copies the fixture DB (~27,000 symbols)
//! 2. Runs the embedding pipeline (generates vectors for ~20,000 embeddable symbols)
//! 3. Calls `build_symbol_context` at full depth and checks the `similar` field
//!
//! Run with the dogfood tier:
//! ```bash
//! cargo test --lib test_deep_dive_full_shows_similar 2>&1 | tail -30
//! ```

use serial_test::serial;

use crate::database::SymbolDatabase;
use crate::embeddings::pipeline::run_embedding_pipeline;
use crate::embeddings::OrtEmbeddingProvider;
use crate::tests::fixtures::julie_db::JulieTestFixture;
use crate::tools::deep_dive::data::{build_symbol_context, find_symbol};
use std::sync::{Arc, Mutex};

/// Set up the fixture DB with embeddings for semantic similarity testing.
///
/// Returns a SymbolDatabase with embeddings populated (no Tantivy needed —
/// semantic similarity only uses SQLite vectors via `get_embedding` + `knn_search`).
fn setup_similarity_fixture() -> SymbolDatabase {
    // 1. Load fixture and copy DB to temp dir (read-write needed for embeddings)
    let fixture = JulieTestFixture::get_instance();
    let temp_dir = fixture
        .copy_to_temp()
        .expect("Failed to copy fixture to temp");
    let db_path = temp_dir.path().join("symbols.db");

    // Keep temp dir alive for the duration of the test
    std::mem::forget(temp_dir);

    // 2. Open database (SymbolDatabase::new handles migrations + sqlite-vec registration)
    let db = SymbolDatabase::new(&db_path).expect("Failed to open fixture database");

    let symbol_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .expect("Failed to count symbols");
    println!("Fixture DB opened: {} symbols", symbol_count);

    // 3. Run the embedding pipeline
    let cache_dir =
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
            .join(".cache")
            .join("fastembed");

    let provider =
        OrtEmbeddingProvider::try_new(Some(cache_dir)).expect("Embedding provider should init");

    let db_arc = Arc::new(Mutex::new(db));
    let stats =
        run_embedding_pipeline(&db_arc, &provider).expect("Embedding pipeline should succeed");
    println!(
        "Embedding pipeline: {}/{} symbols embedded in {} batches",
        stats.symbols_embedded, stats.symbols_scanned, stats.batches_processed
    );

    // Unwrap the Arc<Mutex<>> to get the DB back
    Arc::try_unwrap(db_arc)
        .map_err(|_| "Arc still has multiple owners")
        .unwrap()
        .into_inner()
        .expect("Mutex should not be poisoned")
}

/// **Phase 3 exit criteria**: `deep_dive` at full depth surfaces semantically similar symbols.
///
/// This test validates:
/// 1. `build_symbol_context` at "full" depth populates the `similar` field
/// 2. At least one similar symbol is search-related
/// 3. Similarity scores are in valid range (0.0..=1.0)
/// 4. At most 5 results are returned (the configured SIMILAR_LIMIT)
#[test]
#[serial(fastembed)]
fn test_deep_dive_full_shows_similar_on_real_codebase() {
    let db = setup_similarity_fixture();

    // Look up a well-known search-related symbol that exists in the fixture.
    // Note: hybrid_search was added in Phase 2 and isn't in the fixture snapshot.
    // search_symbols is a core method on SearchIndex — guaranteed to be present.
    let symbols = find_symbol(&db, "search_symbols", Some("src/search/index.rs")).unwrap();
    assert!(
        !symbols.is_empty(),
        "search_symbols should exist in fixture DB (it's a core SearchIndex method)"
    );

    // Pick the first match (the method definition on SearchIndex)
    let target = &symbols[0];
    println!(
        "\nTarget symbol: {} ({:?}) at {}:{}",
        target.name, target.kind, target.file_path, target.start_line
    );

    // Build full context — this is where semantic similarity kicks in
    let ctx = build_symbol_context(&db, target, "full", 10, 10).unwrap();

    // === Check 1: similar field is populated ===
    println!(
        "\n=== Semantically Similar Symbols ({}) ===",
        ctx.similar.len()
    );
    for (i, entry) in ctx.similar.iter().enumerate() {
        println!(
            "  [{}] {} (score={:.4}) {:?} at {}:{}",
            i + 1,
            entry.symbol.name,
            entry.score,
            entry.symbol.kind,
            entry.symbol.file_path,
            entry.symbol.start_line,
        );
    }

    assert!(
        !ctx.similar.is_empty(),
        "Phase 3 exit criteria FAILED: deep_dive at full depth should populate similar symbols \
         when embeddings exist. Got empty similar list for '{}'.",
        target.name,
    );

    // === Check 2: at least one similar symbol is search-related ===
    let search_keywords = [
        "search", "knn", "rrf", "query", "tantivy", "hybrid", "ranking", "scoring",
    ];

    let has_search_related = ctx.similar.iter().any(|entry| {
        let name_lower = entry.symbol.name.to_lowercase();
        search_keywords.iter().any(|kw| name_lower.contains(kw))
    });

    let similar_names: Vec<&str> = ctx.similar.iter().map(|e| e.symbol.name.as_str()).collect();
    assert!(
        has_search_related,
        "Phase 3 exit criteria FAILED: expected at least one search-related symbol \
         among similar results for 'search_symbols'. Got: {:?}",
        similar_names,
    );

    // === Check 3: scores are in valid range (0.0..=1.0) ===
    for entry in &ctx.similar {
        assert!(
            (0.0..=1.0).contains(&entry.score),
            "Similarity score out of range [0.0, 1.0]: {} has score {}",
            entry.symbol.name,
            entry.score,
        );
    }

    // === Check 4: at most 5 results (SIMILAR_LIMIT) ===
    assert!(
        ctx.similar.len() <= 5,
        "Expected at most 5 similar symbols, got {}",
        ctx.similar.len(),
    );

    println!(
        "\nPhase 3 exit criteria PASSED: {} similar symbols found, search-related symbols present.",
        ctx.similar.len(),
    );
}
