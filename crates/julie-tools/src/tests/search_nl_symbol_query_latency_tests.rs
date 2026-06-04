//! Tests proving that NL multi-token symbol-intent queries do not trigger
//! combinatorial expansion or an O(N) embedding-probe latency penalty.
//!
//! ## Invariant
//!
//! "NL multi-token symbol-intent queries do not trigger combinatorial expansion."
//!
//! Specifically:
//! - `expand_query_terms` for a k-word NL query produces O(k) added terms, not O(k^k).
//! - The AND-then-OR fallback in `search_symbols` does not materially inflate latency
//!   beyond the index-open cost.
//! - The definition-search pipeline for a 3-token NL query against a small in-tree
//!   fixture completes within 1.5 seconds in debug builds (the acceptance-criteria bound
//!   for daemon mode per the Phase 1 plan).
//!
//! ## Root-cause note (2026-05-21)
//!
//! Profiling revealed that the bakeoff's 9.5s standalone figure was NOT caused by
//! `expand_query_terms` blowup or AND/OR double-querying.  Both complete in < 2ms.
//! The actual cause was `maybe_initialize_embeddings_for_nl_definitions` probing and
//! launching the Python embedding sidecar (~8-10s) on every NL `definitions` query
//! in standalone mode.  The fix is in `bootstrap_standalone_handler`:
//! `handler.mark_standalone_embedding_skipped()` sets `embedding_runtime_status`
//! to a non-`None` sentinel so the sidecar probe guard fires immediately.
//!
//! Daemon mode is unaffected (it uses a shared `EmbeddingService`).  The daemon
//! latency for the same query is ~100ms.
//!
//! ## Ablation caveat (Task 3)
//!
//! `JULIE_ABLATE_STEMMING` and `JULIE_ABLATE_CAMEL_EMIT` change what tokens are
//! *indexed* at indexing time; query-time expansion is orthogonal.  These env vars
//! affect the bakeoff A/B variants but do not change the latency invariant here.

use std::time::Instant;

use anyhow::Result;
use tempfile::TempDir;

use julie_core::database::{FileInfo, SymbolDatabase};
use julie_extractors::{Symbol, SymbolKind};
use julie_index::search::expansion::{MAX_ADDED_TERMS, expand_query_terms};
use julie_index::search::index::{SearchDocument, SearchFilter, SearchIndex};
use julie_index::search::scoring::is_nl_like_query;
use crate::search::text_search::definition_search_with_index_for_test;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_symbol(id: &str, name: &str, file_path: &str, kind: SymbolKind) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        language: "swift".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 10,
        end_column: 0,
        start_byte: 0,
        end_byte: 200,
        signature: Some(format!("func {name}()")),
        doc_comment: Some(format!("Displays a template for {name}.")),
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(format!("func {}() {{\n  // display template\n}}", name)),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

/// Build a small fixture index + DB with symbols that exercise the
/// `function display template` NL query path.
fn build_fixture() -> Result<(TempDir, TempDir, SearchIndex, SymbolDatabase)> {
    let index_dir = TempDir::new()?;
    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("symbols.db");

    let mut db = SymbolDatabase::new(&db_path)?;
    let index = SearchIndex::create(index_dir.path())?;

    // Representative symbols an Alamofire-like repo might contain.
    let symbols = [
        make_symbol(
            "sym-display-template",
            "displayTemplate",
            "Sources/Alamofire/DisplayTemplate.swift",
            SymbolKind::Function,
        ),
        make_symbol(
            "sym-display-cell",
            "displayCell",
            "Sources/Alamofire/DisplayCell.swift",
            SymbolKind::Function,
        ),
        make_symbol(
            "sym-render-template",
            "renderTemplate",
            "Sources/Alamofire/RenderTemplate.swift",
            SymbolKind::Function,
        ),
        make_symbol(
            "sym-session",
            "session",
            "Sources/Alamofire/Session.swift",
            SymbolKind::Function,
        ),
    ];

    for sym in &symbols {
        db.store_file_info(&FileInfo {
            path: sym.file_path.clone(),
            language: sym.language.clone(),
            hash: sym.id.clone(),
            size: 200,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 10,
            content: sym.code_context.clone(),
        })?;

        index.add_search_doc(&SearchDocument::for_symbol(
            sym,
            vec![],
            String::new(),
            String::new(),
        ))?;
    }
    db.store_symbols(&symbols)?;
    index.commit()?;

    Ok((index_dir, db_dir, index, db))
}

// ---------------------------------------------------------------------------
// Invariant 1: expand_query_terms is O(k) not O(k^k)
// ---------------------------------------------------------------------------

/// The core invariant: NL multi-token symbol-intent queries do not trigger
/// combinatorial expansion in `expand_query_terms`.
///
/// For `"function display template"` (3 tokens, no phrase-alias matches):
/// - alias_terms = 0  (no entry in the static phrase_aliases table)
/// - normalized_terms = 0  (none of the 3 words have -ing/-s suffixes meeting length thresholds)
/// - total added terms <= MAX_ADDED_TERMS (8)
#[test]
fn nl_symbol_query_does_not_trigger_combinatorial_expansion() {
    let query = "function display template";

    assert!(
        is_nl_like_query(query),
        "Precondition: '{query}' must be classified as NL-like for this test to exercise the NL path"
    );

    let expanded = expand_query_terms(query);

    // Structural: original_terms == tokenized words (3 for this query)
    assert_eq!(
        expanded.original_terms.len(),
        3,
        "Expected 3 original terms for '{}', got: {:?}",
        query,
        expanded.original_terms
    );

    // No phrase in the query matches any entry in the static phrase_aliases table.
    assert!(
        expanded.alias_terms.is_empty(),
        "Expected no alias expansion for '{}' (no phrase matches the alias table): {:?}",
        query,
        expanded.alias_terms
    );

    // Total added terms are bounded — even a worst-case 100-word NL query caps at MAX_ADDED_TERMS.
    let total_added = expanded.alias_terms.len() + expanded.normalized_terms.len();
    assert!(
        total_added <= MAX_ADDED_TERMS,
        "Total added terms ({total_added}) must not exceed MAX_ADDED_TERMS ({MAX_ADDED_TERMS}): alias={:?}, normalized={:?}",
        expanded.alias_terms,
        expanded.normalized_terms
    );
}

/// A stress query that could theoretically hit the phrase_aliases for multiple bigrams
/// must still cap at MAX_ADDED_TERMS.
#[test]
fn expansion_cap_applies_to_high_alias_nl_queries() {
    // This query contains bigrams that ARE in the alias table.
    let query = "workspace routing symbol extraction dependency graph call trace index refresh semantic search reference lookup";

    let expanded = expand_query_terms(query);
    let total_added = expanded.alias_terms.len() + expanded.normalized_terms.len();

    assert!(
        total_added <= MAX_ADDED_TERMS,
        "Cap must hold even for a query with many alias-matching bigrams: got {total_added} > MAX_ADDED_TERMS ({MAX_ADDED_TERMS})"
    );
}

// ---------------------------------------------------------------------------
// Invariant 2: latency bound for definition search on a small fixture
// ---------------------------------------------------------------------------

/// The definition-search pipeline for a 3-token NL query completes within
/// 1500ms in debug builds against a small in-process fixture.
///
/// This test does NOT use standalone CLI mode (which would add Tantivy index
/// open cost from disk). It exercises the already-open `SearchIndex` path,
/// mirroring what daemon mode sees.
///
/// Acceptance criterion from Phase 1 plan: < 1.5s in debug daemon mode.
#[test]
fn nl_three_token_definition_search_completes_within_latency_bound() -> Result<()> {
    let (_idx_dir, _db_dir, index, db) = build_fixture()?;

    let filter = SearchFilter {
        language: None,
        kind: None,
        file_pattern: None,
        exclude_tests: false,
    };

    // Warm up: one ignored call to open any lazy internal Tantivy readers.
    let _ = definition_search_with_index_for_test("session", &filter, 10, &index, Some(&db))?;

    // Measured call: the NL path.
    let start = Instant::now();
    let (results, _relaxed, _total) = definition_search_with_index_for_test(
        "function display template",
        &filter,
        10,
        &index,
        Some(&db),
    )?;
    let elapsed = start.elapsed();

    // Correctness: the pipeline returns results (not a zero-hit crash).
    assert!(
        !results.is_empty(),
        "Expected at least one result for 'function display template'; got none"
    );

    // Latency: the search itself (not index open) must be well under 1.5s in debug.
    // In practice it runs in ~1ms; this bound is intentionally generous to survive
    // slow CI and debug builds.
    const LATENCY_BOUND_MS: u128 = 1500;
    assert!(
        elapsed.as_millis() < LATENCY_BOUND_MS,
        "NL symbol-intent search took {}ms, exceeding the {}ms debug bound. \
         Root cause is likely the embedding-sidecar probe firing on the hot path — \
         see `mark_standalone_embedding_skipped` in handler.rs.",
        elapsed.as_millis(),
        LATENCY_BOUND_MS
    );

    Ok(())
}

/// Verify that AND/OR fallback does not blow up latency when AND returns no results.
///
/// Uses a query whose individual terms exist in the index, but NO single symbol
/// contains ALL of them (so the AND pass returns zero, triggering the OR fallback).
/// Both passes must complete within the latency bound.
#[test]
fn and_or_fallback_does_not_blow_up_latency() -> Result<()> {
    // Build a fixture where the queried terms are split across distinct symbols
    // so no single doc matches all of them (AND → zero, OR → some).
    let index_dir = TempDir::new()?;
    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("symbols.db");

    let mut db = SymbolDatabase::new(&db_path)?;
    let index = SearchIndex::create(index_dir.path())?;

    // Symbol A: contains "alpha" only in its body/name, nothing from "bravo charlie"
    let sym_a = Symbol {
        id: "sym-alpha".to_string(),
        name: "alphaProcessor".to_string(),
        kind: SymbolKind::Function,
        language: "swift".to_string(),
        file_path: "Sources/Alpha.swift".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 5,
        end_column: 0,
        start_byte: 0,
        end_byte: 100,
        signature: Some("func alphaProcessor()".to_string()),
        doc_comment: Some("Handles alpha events.".to_string()),
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some("func alphaProcessor() {}".to_string()),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    };

    // Symbol B: contains "bravo" only
    let sym_b = Symbol {
        id: "sym-bravo".to_string(),
        name: "bravoHandler".to_string(),
        kind: SymbolKind::Function,
        language: "swift".to_string(),
        file_path: "Sources/Bravo.swift".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 5,
        end_column: 0,
        start_byte: 0,
        end_byte: 100,
        signature: Some("func bravoHandler()".to_string()),
        doc_comment: Some("Handles bravo events.".to_string()),
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some("func bravoHandler() {}".to_string()),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    };

    for sym in &[sym_a.clone(), sym_b.clone()] {
        db.store_file_info(&FileInfo {
            path: sym.file_path.clone(),
            language: sym.language.clone(),
            hash: sym.id.clone(),
            size: 100,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 5,
            content: sym.code_context.clone(),
        })?;
        index.add_search_doc(&SearchDocument::for_symbol(
            sym,
            vec![],
            String::new(),
            String::new(),
        ))?;
    }
    db.store_symbols(&[sym_a, sym_b])?;
    index.commit()?;

    let filter = SearchFilter {
        language: None,
        kind: None,
        file_pattern: None,
        exclude_tests: false,
    };

    // "alpha bravo" — each term exists in separate symbols, so AND returns zero,
    // OR fallback returns both.
    let start = Instant::now();
    let (results, relaxed, _) =
        definition_search_with_index_for_test("alpha bravo", &filter, 10, &index, Some(&db))?;
    let elapsed = start.elapsed();

    assert!(
        relaxed,
        "Expected relaxed=true (OR fallback fired) for 'alpha bravo'; got relaxed=false. \
         Likely the AND pass found a match, so choose terms that truly do not co-occur."
    );
    assert!(
        !results.is_empty(),
        "Expected OR fallback to return at least one result for 'alpha bravo'; got none"
    );

    const LATENCY_BOUND_MS: u128 = 1500;
    assert!(
        elapsed.as_millis() < LATENCY_BOUND_MS,
        "AND/OR fallback search took {}ms; both passes together must fit within {}ms",
        elapsed.as_millis(),
        LATENCY_BOUND_MS
    );

    Ok(())
}
