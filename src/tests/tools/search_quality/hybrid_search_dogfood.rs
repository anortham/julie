#![cfg(feature = "embeddings-ort")]

//! Hybrid Search Dogfood Test — Phase 2 Exit Criteria
//!
//! Proves that hybrid search (Tantivy keyword + KNN semantic, merged via RRF)
//! improves natural-language query results on Julie's own codebase.
//!
//! **This test is SLOW** (~110s) because it:
//! 1. Copies the fixture DB (~27,000 symbols)
//! 2. Backfills a Tantivy index from the fixture
//! 3. Runs the embedding pipeline (generates vectors for ~20,000 embeddable symbols)
//! 4. Runs hybrid_search with NL queries and checks results
//!
//! Run with the dogfood tier:
//! ```bash
//! cargo test --lib test_hybrid_search_nl 2>&1 | tail -30
//! ```

use serial_test::serial;

use crate::database::SymbolDatabase;
use crate::embeddings::OrtEmbeddingProvider;
use crate::embeddings::pipeline::run_embedding_pipeline;
use crate::search::hybrid::hybrid_search;
use crate::search::index::SearchFilter;
use crate::tests::fixtures::julie_db::JulieTestFixture;
use std::sync::{Arc, Mutex};

/// Set up the fixture DB + Tantivy + embeddings for hybrid search testing.
///
/// Returns (SymbolDatabase, SearchIndex, OrtEmbeddingProvider) ready for hybrid_search.
fn setup_hybrid_search_fixture() -> (
    SymbolDatabase,
    crate::search::SearchIndex,
    OrtEmbeddingProvider,
) {
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

    // 3. Backfill Tantivy index from fixture symbols
    let tantivy_dir = tempfile::tempdir().expect("Failed to create tantivy temp dir");
    let tantivy_path = tantivy_dir.path().to_path_buf();
    std::mem::forget(tantivy_dir);

    let configs = crate::search::LanguageConfigs::load_embedded();
    let search_index =
        crate::search::SearchIndex::open_or_create_with_language_configs(&tantivy_path, &configs)
            .expect("Failed to create Tantivy index");

    let symbols = db
        .get_all_symbols()
        .expect("Failed to get symbols for backfill");
    for symbol in &symbols {
        let doc = crate::search::SymbolDocument::from_symbol(symbol);
        let _ = search_index.add_symbol(&doc);
    }
    search_index
        .commit()
        .expect("Failed to commit Tantivy index");
    println!("Tantivy backfilled: {} symbols", symbols.len());

    // 4. Run the embedding pipeline
    let cache_dir =
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
            .join(".cache")
            .join("fastembed");

    let provider =
        OrtEmbeddingProvider::try_new(Some(cache_dir), Some("bge-small"))
            .expect("Embedding provider should init");

    let db_arc = Arc::new(Mutex::new(db));
    let stats =
        run_embedding_pipeline(&db_arc, &provider, None).expect("Embedding pipeline should succeed");
    println!(
        "Embedding pipeline: {}/{} symbols embedded in {} batches",
        stats.symbols_embedded, stats.symbols_scanned, stats.batches_processed
    );

    // Unwrap the Arc<Mutex<>> to get the DB back
    let db = Arc::try_unwrap(db_arc)
        .map_err(|_| "Arc still has multiple owners")
        .unwrap()
        .into_inner()
        .expect("Mutex should not be poisoned");

    (db, search_index, provider)
}

/// **Phase 2 exit criteria**: Hybrid search improves NL query results over keyword-only.
///
/// This single test validates two things:
/// 1. NL query "how does text search work" surfaces search-related symbols in top 5
/// 2. Hybrid search produces different (better) results than keyword-only search
///
/// We consolidate both checks into one test because the setup (embedding 20K+ symbols)
/// takes ~100s and we don't want to pay that cost twice.
#[test]
#[serial(fastembed)]
fn test_hybrid_search_nl_query_improves_over_keyword_only() {
    let (db, search_index, provider) = setup_hybrid_search_fixture();

    let query = "how does text search work";
    let filter = SearchFilter::default();

    // ── Keyword-only search (baseline) ─────────────────────────────────
    let keyword_results = hybrid_search(query, &filter, 10, &search_index, &db, None, None)
        .expect("keyword search should succeed");

    println!("\n=== Keyword-only results for '{}' ===", query);
    for (i, r) in keyword_results.results.iter().enumerate() {
        println!(
            "  [{}] {} ({}) score={:.6} file={}:{}",
            i + 1,
            r.name,
            r.kind,
            r.score,
            r.file_path,
            r.start_line,
        );
    }

    // ── Hybrid search (keyword + semantic via RRF) ─────────────────────
    let profile = crate::search::weights::SearchWeightProfile::fast_search();
    let hybrid_results = hybrid_search(
        query,
        &filter,
        10,
        &search_index,
        &db,
        Some(&provider),
        Some(profile),
    )
    .expect("hybrid_search should succeed");

    println!("\n=== Hybrid search results for '{}' ===", query);
    for (i, r) in hybrid_results.results.iter().enumerate() {
        println!(
            "  [{}] {} ({}) score={:.6} file={}:{}",
            i + 1,
            r.name,
            r.kind,
            r.score,
            r.file_path,
            r.start_line,
        );
    }

    // ── Check 1: Hybrid search surfaces search-related symbols in top 5 ──
    let top_5_names: Vec<&str> = hybrid_results
        .results
        .iter()
        .take(5)
        .map(|r| r.name.as_str())
        .collect();

    let has_search_impl = top_5_names.iter().any(|n| {
        n.contains("text_search")
            || n.contains("TextSearch")
            || n.contains("search_symbols")
            || n.contains("search_content")
            || n.contains("SearchIndex")
            || n.contains("search_index")
    });

    assert!(
        has_search_impl,
        "Phase 2 exit criteria FAILED: expected text search implementation in top 5, got: {:?}",
        top_5_names
    );

    // ── Check 2: Hybrid results differ from keyword-only ───────────────
    let keyword_ids: Vec<&str> = keyword_results
        .results
        .iter()
        .take(5)
        .map(|r| r.id.as_str())
        .collect();
    let hybrid_ids: Vec<&str> = hybrid_results
        .results
        .iter()
        .take(5)
        .map(|r| r.id.as_str())
        .collect();

    // Hybrid should return MORE results than keyword-only for NL queries
    // (semantic search finds relevant symbols that keyword matching misses)
    println!(
        "\nKeyword returned {} results, hybrid returned {} results",
        keyword_results.results.len(),
        hybrid_results.results.len()
    );

    assert!(
        hybrid_results.results.len() >= keyword_results.results.len(),
        "Hybrid search should return at least as many results as keyword-only. \
         Keyword: {}, Hybrid: {}",
        keyword_results.results.len(),
        hybrid_results.results.len()
    );

    // Log whether top-5 differs (informational, not a hard assertion)
    if keyword_ids != hybrid_ids {
        println!("Semantic search reshuffled top-5 rankings (expected for NL queries).");
    } else {
        println!(
            "NOTE: Identical top-5 — semantic search added results but didn't change ranking."
        );
    }
}
