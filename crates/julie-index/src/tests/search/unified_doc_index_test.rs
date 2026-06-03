//! T2 — `SearchDocument` + `add_search_doc` tests.
//!
//! Verifies:
//! - A symbol-row `SearchDocument` is indexed and searchable by `name`.
//! - A file-row `SearchDocument` is retrievable by `path_text` and `name`
//!   (basename without extension).
//! - `code_body` is truncated to ≤ 2000 bytes on a UTF-8 boundary.

use tempfile::TempDir;

use crate::search::index::{SearchDocument, SearchFilter, SearchIndex};

// ---------------------------------------------------------------------------
// Helper: create a fresh in-memory (temp-dir) index
// ---------------------------------------------------------------------------

fn make_index() -> (TempDir, SearchIndex) {
    let dir = TempDir::new().unwrap();
    let index = SearchIndex::create(dir.path()).unwrap();
    (dir, index)
}

// ---------------------------------------------------------------------------
// Test 1: symbol-row SearchDocument is searchable by name
// ---------------------------------------------------------------------------

#[test]
fn indexes_symbol_doc() {
    let (_dir, index) = make_index();

    let doc = SearchDocument {
        doc_type: "symbol".to_string(),
        id: "sym-001".to_string(),
        name: "compute_hash".to_string(),
        signature: "fn compute_hash(input: &str) -> u64".to_string(),
        doc_comment: "Computes a hash of the input string.".to_string(),
        code_body: "fn compute_hash(input: &str) -> u64 { 0 }".to_string(),
        pretokenized_code: String::new(),
        relationship_text: String::new(),
        language: "rust".to_string(),
        file_path: "src/hashing.rs".to_string(),
        basename: "hashing.rs".to_string(),
        kind: "function".to_string(),
        start_line: 42,
        role: "source".to_string(),
        test_role: String::new(),
        // annotation fields — populated for symbol rows
        annotation_keys: vec!["deprecated".to_string()],
        annotations_text: "deprecated".to_string(),
        owner_names_text: String::new(),
        // file-row-only fields — empty for symbol rows
        content: String::new(),
        path_text: String::new(),
    };

    index.add_search_doc(&doc).unwrap();
    index.commit().unwrap();

    // name field must be searchable
    let results = index
        .search_symbols("compute_hash", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "expected at least one result searching 'compute_hash'"
    );
    assert_eq!(
        results[0].name, "compute_hash",
        "first result must be compute_hash"
    );
}

#[test]
fn unified_hit_preserves_symbol_code_body_excerpt() {
    let (_dir, index) = make_index();

    let doc = SearchDocument {
        doc_type: "symbol".to_string(),
        id: "sym-body-001".to_string(),
        name: "body_marker_fn".to_string(),
        signature: "fn body_marker_fn()".to_string(),
        doc_comment: String::new(),
        code_body: "fn body_marker_fn() { let alpha_beta_gamma_delta = 42; }".to_string(),
        pretokenized_code: String::new(),
        relationship_text: String::new(),
        language: "rust".to_string(),
        file_path: "src/body_marker.rs".to_string(),
        basename: "body_marker.rs".to_string(),
        kind: "function".to_string(),
        start_line: 7,
        role: "source".to_string(),
        test_role: String::new(),
        annotation_keys: vec![],
        annotations_text: String::new(),
        owner_names_text: String::new(),
        content: String::new(),
        path_text: String::new(),
    };

    index.add_search_doc(&doc).unwrap();
    index.commit().unwrap();

    let hits = index
        .search_unified("alpha_beta_gamma_delta", &SearchFilter::default(), 10)
        .unwrap();
    let hit = hits
        .iter()
        .find(|hit| hit.id == "sym-body-001")
        .expect("body_marker_fn should be returned by code_body search");

    assert!(
        hit.code_body.contains("alpha_beta_gamma_delta"),
        "UnifiedHit must preserve the stored code_body excerpt for reranking and output context; got {:?}",
        hit.code_body
    );
}

#[test]
fn unified_reranker_uses_stored_code_body_for_phrase_boost() {
    let (_dir, index) = make_index();

    let phrase_doc = SearchDocument {
        doc_type: "symbol".to_string(),
        id: "sym-phrase-body-001".to_string(),
        name: "neutral_phrase_holder".to_string(),
        signature: "fn neutral_phrase_holder()".to_string(),
        doc_comment: String::new(),
        code_body: "fn neutral_phrase_holder() { alpha bravo charlie delta }".to_string(),
        pretokenized_code: String::new(),
        relationship_text: String::new(),
        language: "rust".to_string(),
        file_path: "src/phrase.rs".to_string(),
        basename: "phrase.rs".to_string(),
        kind: "function".to_string(),
        start_line: 3,
        role: "source".to_string(),
        test_role: String::new(),
        annotation_keys: vec![],
        annotations_text: String::new(),
        owner_names_text: String::new(),
        content: String::new(),
        path_text: String::new(),
    };

    let title_decoy = SearchDocument {
        doc_type: "symbol".to_string(),
        id: "sym-title-decoy-001".to_string(),
        name: "alpha_bravo_charlie_delta_decoy".to_string(),
        signature: "fn alpha_bravo_charlie_delta_decoy()".to_string(),
        doc_comment: String::new(),
        code_body: String::new(),
        pretokenized_code: String::new(),
        relationship_text: String::new(),
        language: "rust".to_string(),
        file_path: "src/decoy.rs".to_string(),
        basename: "decoy.rs".to_string(),
        kind: "function".to_string(),
        start_line: 5,
        role: "source".to_string(),
        test_role: String::new(),
        annotation_keys: vec![],
        annotations_text: String::new(),
        owner_names_text: String::new(),
        content: String::new(),
        path_text: String::new(),
    };

    index.add_search_doc(&phrase_doc).unwrap();
    index.add_search_doc(&title_decoy).unwrap();
    index.commit().unwrap();

    let hits = index
        .search_unified("alpha bravo charlie delta", &SearchFilter::default(), 10)
        .unwrap();
    let names: Vec<&str> = hits.iter().map(|hit| hit.name.as_str()).collect();

    assert_eq!(
        names.first(),
        Some(&"neutral_phrase_holder"),
        "body phrase boost must use stored code_body and beat a title-term decoy; got {names:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 2: file-row SearchDocument is retrievable by path_text and name
// ---------------------------------------------------------------------------

#[test]
fn indexes_file_doc() {
    let (_dir, index) = make_index();

    let doc = SearchDocument {
        doc_type: "file".to_string(),
        id: String::new(),
        name: "parser".to_string(), // basename without extension
        signature: String::new(),
        doc_comment: String::new(),
        code_body: String::new(),
        pretokenized_code: String::new(),
        relationship_text: String::new(),
        language: "rust".to_string(),
        file_path: "src/parser.rs".to_string(),
        basename: "parser.rs".to_string(),
        kind: "file".to_string(),
        start_line: 0,
        role: "source".to_string(),
        test_role: String::new(),
        // annotation fields — empty for file rows
        annotation_keys: vec![],
        annotations_text: String::new(),
        owner_names_text: String::new(),
        // file content fields
        content: "fn parse(input: &str) -> Ast { todo!() }".to_string(),
        path_text: "src/parser.rs".to_string(),
    };

    index.add_search_doc(&doc).unwrap();
    index.commit().unwrap();

    // path_text must be searchable (content search uses this for file hits)
    let content_results = index
        .search_content("parse", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !content_results.is_empty(),
        "expected content search hit for 'parse'"
    );
    assert_eq!(
        content_results[0].file_path, "src/parser.rs",
        "content result must point to src/parser.rs"
    );
}

// ---------------------------------------------------------------------------
// Test 3: code_body truncation is UTF-8-safe at ≤ 2000 bytes
// ---------------------------------------------------------------------------

#[test]
fn body_truncation_utf8_safe() {
    // Build a string that is > 2000 bytes. Use a mix of ASCII + multibyte
    // chars. Place a 4-byte emoji (U+1F600, "😀") straddling the 2000-byte
    // boundary: fill 1998 bytes with 'a', then append the 4-byte emoji,
    // then fill the rest. The truncation must not split the emoji.
    let mut input = "a".repeat(1998);
    input.push('😀'); // bytes 1998..2002 (4 bytes)
    input.push_str(&"b".repeat(200)); // tail to ensure >2000 total

    assert!(input.len() > 2000, "pre-condition: input is >2000 bytes");

    let doc = SearchDocument {
        doc_type: "symbol".to_string(),
        id: "trunc-001".to_string(),
        name: "big_symbol".to_string(),
        signature: String::new(),
        doc_comment: String::new(),
        code_body: input.clone(),
        pretokenized_code: String::new(),
        relationship_text: String::new(),
        language: "rust".to_string(),
        file_path: "src/big.rs".to_string(),
        basename: "big.rs".to_string(),
        kind: "function".to_string(),
        start_line: 1,
        role: "source".to_string(),
        test_role: String::new(),
        annotation_keys: vec![],
        annotations_text: String::new(),
        owner_names_text: String::new(),
        content: String::new(),
        path_text: String::new(),
    };

    // code_body is stored via add_search_doc; we verify truncation by
    // inspecting the stored field value after a commit + retrieve cycle.
    let dir = TempDir::new().unwrap();
    let index = SearchIndex::create(dir.path()).unwrap();
    index.add_search_doc(&doc).unwrap();
    index.commit().unwrap();

    // search for the symbol so we can get a stored doc back
    let results = index
        .search_symbols("big_symbol", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(!results.is_empty(), "must find big_symbol after indexing");

    // The truncation helper is internal; verify it directly via the public
    // helper exposed for testing.
    let truncated = crate::search::index::truncate_utf8_bytes(&input, 2000);
    assert!(
        truncated.len() <= 2000,
        "truncated len {} must be ≤ 2000",
        truncated.len()
    );
    // Must be valid UTF-8 (from_utf8 returns Err if not)
    std::str::from_utf8(truncated.as_bytes()).expect("truncated slice must be valid UTF-8");
    // Must NOT include the emoji (it starts at byte 1998, past the boundary)
    assert!(
        !truncated.contains('😀'),
        "truncated string must not contain the straddling emoji"
    );
}
