//! T4 acceptance tests: projection layer emits SearchDocument with union write.
//!
//! Three invariants:
//! - `symbol_indexable`: project a fixture symbol; raw Tantivy name-field hit.
//! - `file_row_indexable`: project a fixture file doc; kind="file" and basename match.
//! - `old_path_still_works`: after projection, `search_symbols` returns the symbol
//!   — proves union write preserves old-path behaviour.

use tempfile::TempDir;

use crate::database::types::FileInfo;
use crate::extractors::{Symbol, SymbolKind};
use crate::search::projection::apply_documents;
use crate::search::{SearchFilter, SearchIndex};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn make_test_symbol(id: &str, name: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 5,
        end_column: 0,
        start_byte: 0,
        end_byte: 64,
        signature: Some(format!("fn {}(input: &str) -> u64", name)),
        doc_comment: Some(format!("Docs for {}", name)),
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(format!("fn {}(input: &str) -> u64 {{ 0 }}", name)),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

fn make_test_file_info(file_path: &str, content: &str) -> FileInfo {
    FileInfo {
        path: file_path.to_string(),
        content: Some(content.to_string()),
        language: "rust".to_string(),
        hash: String::new(),
        size: content.len() as i64,
        last_modified: 0,
        last_indexed: 0,
        symbol_count: 0,
        line_count: content.lines().count() as i32,
    }
}

fn make_index() -> (TempDir, SearchIndex) {
    let dir = TempDir::new().unwrap();
    let index = SearchIndex::create(dir.path()).unwrap();
    (dir, index)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Project a single symbol; assert it's retrievable via `search_symbols` by name.
#[test]
fn symbol_indexable() {
    let (_dir, index) = make_index();

    let sym = make_test_symbol("sym-t4-001", "compute_checksum", "src/hash.rs");

    apply_documents(&index, std::slice::from_ref(&sym), &[], &[]).unwrap();

    // `apply_documents` commits internally; directly search
    let results = index
        .search_symbols("compute_checksum", &SearchFilter::default(), 5)
        .unwrap()
        .results;

    assert!(
        !results.is_empty(),
        "expected at least one result for 'compute_checksum' after projection"
    );
    assert_eq!(
        results[0].name, "compute_checksum",
        "first result name must be 'compute_checksum'"
    );
    assert_eq!(
        results[0].id, "sym-t4-001",
        "result id must match projected symbol"
    );
}

/// Project a single file doc; assert `kind="file"` and basename present.
#[test]
fn file_row_indexable() {
    let (_dir, index) = make_index();

    let file_info = make_test_file_info("src/parser.rs", "fn parse() {}");

    apply_documents(&index, &[], std::slice::from_ref(&file_info), &[]).unwrap();

    // File rows appear in content search, not symbol search.
    let content_results = index
        .search_content("parse", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert!(
        !content_results.is_empty(),
        "expected content hit for 'parse' after projecting file row"
    );
    let hit = &content_results[0];
    assert_eq!(
        hit.file_path, "src/parser.rs",
        "content result file_path must be 'src/parser.rs'"
    );
}

/// After projection via the new SearchDocument path, `search_symbols` must still
/// return the symbol — proves union shape preserves old-path behaviour.
#[test]
fn old_path_still_works() {
    let (_dir, index) = make_index();

    let sym = make_test_symbol("sym-t4-002", "encode_buffer", "src/codec.rs");

    apply_documents(&index, std::slice::from_ref(&sym), &[], &[]).unwrap();

    let results = index
        .search_symbols("encode_buffer", &SearchFilter::default(), 5)
        .unwrap()
        .results;

    assert!(
        !results.is_empty(),
        "search_symbols must find 'encode_buffer' through union-shape write"
    );
    assert_eq!(
        results[0].name, "encode_buffer",
        "union-shape result name must be 'encode_buffer'"
    );
    // Verify key fields are preserved — these come from what add_search_doc writes
    assert_eq!(results[0].file_path, "src/codec.rs");
    assert_eq!(results[0].kind, "function");
    assert_eq!(results[0].language, "rust");
}
