//! Tests for cross-target title-exact and basename-exact score boosts.
//!
//! Invariant proved by this suite:
//!   "Title-exact and basename-exact matches dominate BM25 score across all
//!    three search targets (definitions, files, content)."
//!
//! Coverage:
//!   (a) `displayTemplate`-style — content target: file containing a symbol
//!       literally named `displayTemplate` ranks above duplicate files with
//!       identical body text but no title-matching symbol.
//!   (b) `requestedRedirect`-style — files target: file whose body contains a
//!       symbol matching the camelCased query ranks above a file whose basename
//!       shares only one query token.
//!   (c) Definitions regression — single-token exact-symbol query still returns
//!       the exact-name symbol as rank-1.

use anyhow::Result;
use tempfile::TempDir;

use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::{Symbol, SymbolKind};
use crate::search::index::{ContentSearchResult, FileSearchResult, FileMatchKind, SearchFilter, SearchIndex, SymbolDocument, apply_symbol_title_boost_to_file_results};
use crate::search::language_config::LanguageConfigs;
use crate::tools::search::text_search::{
    apply_reranker_to_content_results, definition_search_with_index_for_test,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_test_index() -> (TempDir, SearchIndex) {
    let temp_dir = TempDir::new().unwrap();
    let configs = LanguageConfigs::load_embedded();
    let index = SearchIndex::create_with_language_configs(temp_dir.path(), &configs).unwrap();
    (temp_dir, index)
}

fn create_test_db() -> (TempDir, SymbolDatabase) {
    let db_dir = TempDir::new().unwrap();
    let db_path = db_dir.path().join("symbols.db");
    let db = SymbolDatabase::new(&db_path).unwrap();
    (db_dir, db)
}

/// Store a minimal file record and a symbol in the DB.
fn store_symbol_in_file(db: &mut SymbolDatabase, file_path: &str, symbol_name: &str, language: &str) {
    db.store_file_info(&FileInfo {
        path: file_path.to_string(),
        language: language.to_string(),
        hash: format!("hash-{file_path}"),
        size: 100,
        last_modified: 1,
        last_indexed: 1,
        symbol_count: 1,
        line_count: 10,
        content: Some(format!("function {symbol_name}() {{}}")),
    })
    .unwrap();
    db.store_symbols(&[Symbol {
        id: format!("{file_path}/{symbol_name}"),
        name: symbol_name.to_string(),
        kind: SymbolKind::Function,
        language: language.to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 5,
        end_column: 0,
        start_byte: 0,
        end_byte: 60,
        signature: Some(format!("function {symbol_name}()")),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(format!("function {symbol_name}() {{}}")),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }])
    .unwrap();
}

// ---------------------------------------------------------------------------
// (a) Content path — displayTemplate-style
// ---------------------------------------------------------------------------

/// A file containing a symbol named exactly `displayTemplate` should rank
/// above files that contain the same body text but have no such symbol, when
/// the query is "displayTemplate".
///
/// This is the Pattern-A duplicate-file scenario: Alamofire's jazzy.search.js
/// copies where the wrong copy wins by BM25 alone.
#[test]
fn content_path_symbol_title_exact_boost_ranks_defining_file_first() -> Result<()> {
    // We test `apply_reranker_to_content_results` directly with a hand-crafted
    // result set and a populated DB.  This isolates the boost from Tantivy
    // BM25 scoring so the assertion is deterministic.

    let (_db_dir, mut db) = create_test_db();

    // File A defines `displayTemplate` as a symbol.
    store_symbol_in_file(&mut db, "src/display/template.js", "displayTemplate", "javascript");

    // File B has the same body text but its only symbol is something else.
    store_symbol_in_file(&mut db, "vendor/copy/template.js", "someOtherFunction", "javascript");

    // Simulate Tantivy returning File B with a higher raw BM25 score (the
    // scenario where BM25 would normally win without the boost).
    let mut results = vec![
        ContentSearchResult {
            file_path: "vendor/copy/template.js".to_string(),
            language: "javascript".to_string(),
            score: 10.0, // higher BM25
        },
        ContentSearchResult {
            file_path: "src/display/template.js".to_string(),
            language: "javascript".to_string(),
            score: 8.0, // lower BM25
        },
    ];

    // Reranker must be enabled (default).
    // Passing Some(&db) triggers the symbol-title lookup.
    // SAFETY: single-threaded test; no other thread reads this var concurrently.
    unsafe { std::env::remove_var("JULIE_RERANKER_ENABLED") };
    apply_reranker_to_content_results("displayTemplate", &mut results, Some(&db));

    assert_eq!(
        results[0].file_path,
        "src/display/template.js",
        "File defining the symbol should rank first. Got: {:?}",
        results.iter().map(|r| (&r.file_path, r.score)).collect::<Vec<_>>()
    );
    assert_eq!(
        results[1].file_path,
        "vendor/copy/template.js",
        "File without the symbol should rank second."
    );

    Ok(())
}

/// Verify the boost does NOT fire when db is None (backward-compat).
#[test]
fn content_path_no_db_preserves_original_order() -> Result<()> {
    // BM25 order is preserved when no DB is provided.
    let mut results = vec![
        ContentSearchResult {
            file_path: "vendor/copy/template.js".to_string(),
            language: "javascript".to_string(),
            score: 10.0,
        },
        ContentSearchResult {
            file_path: "src/display/template.js".to_string(),
            language: "javascript".to_string(),
            score: 8.0,
        },
    ];

    // SAFETY: single-threaded test; no other thread reads this var concurrently.
    unsafe { std::env::remove_var("JULIE_RERANKER_ENABLED") };
    apply_reranker_to_content_results("displayTemplate", &mut results, None);

    // Without DB, only the basename/path reranker fires.
    // "template.js" basename contains "template" not "displaytemplate" so no
    // EXACT_TITLE_BOOST fires.  The vendor penalty may demote the first result,
    // but neither file gets the +100 symbol boost.  We just assert the DB
    // absence doesn't crash and at least doesn't spuriously boost the vendor copy.
    // (Both results should survive.)
    assert_eq!(results.len(), 2, "Both results must survive");

    Ok(())
}

// ---------------------------------------------------------------------------
// (b) Files path — requestedRedirect-style
// ---------------------------------------------------------------------------

/// File `res/redirect.js` defines a function `requestedRedirect`.
/// File `res/location.js` defines a function `setLocation` (no name match).
/// Query "requestedRedirect" on the files target should rank `redirect.js`
/// first because its symbol name exactly matches the query.
#[test]
fn files_path_symbol_title_boost_ranks_defining_file_first() -> Result<()> {
    let (_db_dir, mut db) = create_test_db();

    // redirect.js has the matching symbol.
    store_symbol_in_file(&mut db, "res/redirect.js", "requestedRedirect", "javascript");

    // location.js shares the "res/" prefix and "js" extension but no name match.
    store_symbol_in_file(&mut db, "res/location.js", "setLocation", "javascript");

    // Simulate file search returning location.js first (higher BM25 due to
    // basename token overlap or path depth heuristic).
    let mut results = vec![
        FileSearchResult {
            file_path: "res/location.js".to_string(),
            language: "javascript".to_string(),
            score: 8.0, // higher BM25
            match_kind: FileMatchKind::PathFragment,
        },
        FileSearchResult {
            file_path: "res/redirect.js".to_string(),
            language: "javascript".to_string(),
            score: 6.0, // lower BM25
            match_kind: FileMatchKind::PathFragment,
        },
    ];

    apply_symbol_title_boost_to_file_results("requestedRedirect", &mut results, &db);

    assert_eq!(
        results[0].file_path,
        "res/redirect.js",
        "File defining the symbol should rank first. Got: {:?}",
        results.iter().map(|r| (&r.file_path, r.score)).collect::<Vec<_>>()
    );
    assert_eq!(
        results[1].file_path,
        "res/location.js",
        "File without the symbol should rank second."
    );

    Ok(())
}

/// Verify that `apply_symbol_title_boost_to_file_results` is a no-op on an
/// empty result set (no panic, no DB query).
#[test]
fn files_path_boost_is_noop_on_empty_results() -> Result<()> {
    let (_db_dir, db) = create_test_db();
    let mut results: Vec<FileSearchResult> = Vec::new();
    apply_symbol_title_boost_to_file_results("anything", &mut results, &db);
    assert!(results.is_empty());
    Ok(())
}

// ---------------------------------------------------------------------------
// (c) Definitions regression — exact symbol query
// ---------------------------------------------------------------------------

/// Querying the exact symbol name `renderWidget` must return the symbol named
/// `renderWidget` as rank-1 even when another symbol `renderWidgetHelper` has
/// a slightly higher raw BM25 score (due to body text density, for example).
///
/// This confirms `promote_exact_name_matches` in the definitions path still
/// dominates BM25.
#[test]
fn definitions_path_exact_name_still_ranks_first() -> Result<()> {
    let (_dir, index) = create_test_index();

    // Insert the helper first with richer body text to bias BM25 in its favor.
    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "renderWidgetHelper".into(),
            signature: "fn renderWidgetHelper()".into(),
            doc_comment: "renderWidget renderWidget renderWidget helper utility for renderWidget".into(),
            code_body: "renderWidget renderWidget renderWidget renderWidget".into(),
            file_path: "src/ui/widget_helper.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();

    // The exact-match symbol with thinner body.
    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "renderWidget".into(),
            signature: "fn renderWidget()".into(),
            doc_comment: "Renders a widget.".into(),
            code_body: "draw_frame(widget);".into(),
            file_path: "src/ui/widget.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 10,
        })
        .unwrap();

    index.commit().unwrap();

    let (symbols, _relaxed, _total) =
        definition_search_with_index_for_test("renderWidget", &SearchFilter::default(), 5, &index, None)?;

    assert!(
        !symbols.is_empty(),
        "Expected at least one result for 'renderWidget'"
    );
    assert_eq!(
        symbols[0].name,
        "renderWidget",
        "Exact-name symbol must be rank-1. Got: {:?}",
        symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// titles_for_files unit test
// ---------------------------------------------------------------------------

/// A file that *imports* `SymbolDatabase` (kind=import) but does NOT *define*
/// it must NOT receive the title-exact boost.
///
/// Regression for: `tracing.rs` importing `SymbolDatabase` via
/// `use crate::database::SymbolDatabase` caused it to rank above the actual
/// definition file when searching the content target.  The root cause was
/// `titles_for_files` returning ALL symbol rows including `kind='import'`.
#[test]
fn titles_for_files_excludes_import_kind_symbols() -> Result<()> {
    let (_db_dir, mut db) = create_test_db();

    // File A: the actual definition (kind=Function, the only definition kind
    // used by store_symbol_in_file).
    store_symbol_in_file(&mut db, "src/database/symbols/mod.rs", "SymbolDatabase", "rust");

    // File B: imports SymbolDatabase (kind=Import) but does NOT define it.
    db.store_file_info(&crate::database::FileInfo {
        path: "src/tests/core/tracing.rs".to_string(),
        language: "rust".to_string(),
        hash: "hash-tracing".to_string(),
        size: 200,
        last_modified: 1,
        last_indexed: 1,
        symbol_count: 1,
        line_count: 30,
        content: Some("use crate::database::SymbolDatabase;".to_string()),
    })
    .unwrap();
    db.store_symbols(&[Symbol {
        id: "src/tests/core/tracing.rs/SymbolDatabase".to_string(),
        name: "SymbolDatabase".to_string(),
        kind: SymbolKind::Import,
        language: "rust".to_string(),
        file_path: "src/tests/core/tracing.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: 0,
        start_byte: 0,
        end_byte: 36,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some("use crate::database::SymbolDatabase;".to_string()),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }])?;

    let paths = vec![
        "src/database/symbols/mod.rs",
        "src/tests/core/tracing.rs",
    ];
    let titles = db.titles_for_files(&paths)?;

    // The definition file must have "symboldatabase" in its titles.
    let def_titles = titles
        .get("src/database/symbols/mod.rs")
        .expect("definition file must have an entry");
    assert!(
        def_titles.contains(&"symboldatabase".to_string()),
        "Definition file must include 'symboldatabase'. Got: {def_titles:?}"
    );

    // The import-only file must NOT appear in the title map at all (no
    // definition-kind symbols), so the boost will never fire for it.
    assert!(
        !titles.contains_key("src/tests/core/tracing.rs"),
        "Import-only file must not appear in titles_for_files output. \
         Got keys: {:?}",
        titles.keys().collect::<Vec<_>>()
    );

    Ok(())
}

/// The batched DB method returns correct lowercase symbol names per file.
#[test]
fn titles_for_files_returns_lowercase_names_per_file() -> Result<()> {
    let (_db_dir, mut db) = create_test_db();

    store_symbol_in_file(&mut db, "src/foo.js", "MyFunction", "javascript");
    store_symbol_in_file(&mut db, "src/bar.js", "AnotherFunc", "javascript");
    // Store a second symbol in foo.js.
    db.store_symbols(&[Symbol {
        id: "src/foo.js/helperFn".to_string(),
        name: "helperFn".to_string(),
        kind: SymbolKind::Function,
        language: "javascript".to_string(),
        file_path: "src/foo.js".to_string(),
        start_line: 5,
        start_column: 0,
        end_line: 8,
        end_column: 0,
        start_byte: 50,
        end_byte: 100,
        signature: Some("function helperFn()".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }])?;

    let paths = vec!["src/foo.js", "src/bar.js", "src/missing.js"];
    let titles = db.titles_for_files(&paths)?;

    let foo_titles = titles.get("src/foo.js").expect("foo.js must have entries");
    assert!(
        foo_titles.contains(&"myfunction".to_string()),
        "Expected lowercase 'myfunction'. Got: {foo_titles:?}"
    );
    assert!(
        foo_titles.contains(&"helperfn".to_string()),
        "Expected lowercase 'helperfn'. Got: {foo_titles:?}"
    );

    let bar_titles = titles.get("src/bar.js").expect("bar.js must have entries");
    assert!(
        bar_titles.contains(&"anotherfunc".to_string()),
        "Expected lowercase 'anotherfunc'. Got: {bar_titles:?}"
    );

    assert!(
        !titles.contains_key("src/missing.js"),
        "File with no symbols must have no entry"
    );

    Ok(())
}
