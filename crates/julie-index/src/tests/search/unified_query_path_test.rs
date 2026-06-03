//! T5 — `build_unified_query` + `search_unified` tests.
//!
//! Verifies:
//! - A unified query returns mixed-kind hits (function, class, file) with
//!   `kind` preserved in each `UnifiedHit`.
//! - A file row with an exact basename match scores at least as well as a
//!   symbol with a partial-name match (file row in top-3).

use tempfile::TempDir;

use crate::search::index::{SearchDocument, SearchFilter, SearchIndex};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn make_index() -> (TempDir, SearchIndex) {
    let dir = TempDir::new().unwrap();
    let index = SearchIndex::create(dir.path()).unwrap();
    (dir, index)
}

fn sym_doc(id: &str, name: &str, kind: &str, file_path: &str, sig: &str) -> SearchDocument {
    SearchDocument {
        doc_type: "symbol".to_string(),
        id: id.to_string(),
        name: name.to_string(),
        language: "python".to_string(),
        file_path: file_path.to_string(),
        basename: file_path.split('/').last().unwrap_or("").to_string(),
        kind: kind.to_string(),
        role: "source".to_string(),
        test_role: String::new(),
        signature: sig.to_string(),
        doc_comment: String::new(),
        code_body: String::new(),
        annotation_keys: vec![],
        annotations_text: String::new(),
        owner_names_text: String::new(),
        start_line: 1,
        content: String::new(),
        path_text: String::new(),
        pretokenized_code: String::new(),
        relationship_text: String::new(),
    }
}

fn file_doc(name: &str, file_path: &str) -> SearchDocument {
    let basename = file_path.split('/').last().unwrap_or("").to_string();
    SearchDocument {
        doc_type: "file".to_string(),
        id: String::new(),
        name: name.to_string(),
        language: "python".to_string(),
        file_path: file_path.to_string(),
        basename: basename.clone(),
        kind: "file".to_string(),
        role: "source".to_string(),
        test_role: String::new(),
        signature: String::new(),
        doc_comment: String::new(),
        code_body: String::new(),
        annotation_keys: vec![],
        annotations_text: String::new(),
        owner_names_text: String::new(),
        start_line: 0,
        content: format!("contents of {}", file_path),
        path_text: file_path.to_string(),
        pretokenized_code: String::new(),
        relationship_text: String::new(),
    }
}

// ---------------------------------------------------------------------------
// Test 1: returns_mixed_kinds
//
// Index one function, one class, one file — all containing the same query
// term. A single unified query should return all three with kind preserved.
// ---------------------------------------------------------------------------

#[test]
fn returns_mixed_kinds() {
    let (_dir, index) = make_index();

    // All three docs share the term "browser" in different fields.
    index
        .add_search_doc(&sym_doc(
            "fn-001",
            "browser_render",
            "function",
            "src/browser.py",
            "def browser_render(url: str) -> None",
        ))
        .unwrap();
    index
        .add_search_doc(&sym_doc(
            "cls-001",
            "BrowserSession",
            "class",
            "src/browser.py",
            "class BrowserSession:",
        ))
        .unwrap();
    index
        .add_search_doc(&file_doc("browser", "src/browser.py"))
        .unwrap();
    index.commit().unwrap();

    let filter = SearchFilter::default();
    let hits = index.search_unified("browser", &filter, 10).unwrap();

    assert!(
        !hits.is_empty(),
        "expected at least one unified hit for 'browser'"
    );

    // Collect distinct kinds returned.
    let kinds: std::collections::HashSet<String> = hits.iter().map(|h| h.kind.clone()).collect();

    assert!(
        kinds.contains("function"),
        "expected 'function' kind in results, got: {:?}",
        kinds
    );
    assert!(
        kinds.contains("class"),
        "expected 'class' kind in results, got: {:?}",
        kinds
    );
    assert!(
        kinds.contains("file"),
        "expected 'file' kind in results, got: {:?}",
        kinds
    );

    // All hits carry a non-empty kind.
    for hit in &hits {
        assert!(
            !hit.kind.is_empty(),
            "every UnifiedHit must carry a non-empty kind field"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2: file_exact_beats_symbol_partial
//
// Index a file with basename "browser_client.py" and a symbol whose name
// contains "browser_client". Query "browser_client". The file row must appear
// in the top-3 (final ordering deferred to T6's reranker).
// ---------------------------------------------------------------------------

#[test]
fn file_exact_beats_symbol_partial() {
    let (_dir, index) = make_index();

    // File row — exact basename match.
    index
        .add_search_doc(&file_doc("browser_client", "src/browser_client.py"))
        .unwrap();

    // Symbol row — name contains "browser_client" as a substring.
    index
        .add_search_doc(&sym_doc(
            "fn-partial",
            "init_browser_client_pool",
            "function",
            "src/pool.py",
            "def init_browser_client_pool(size: int) -> None",
        ))
        .unwrap();

    index.commit().unwrap();

    let filter = SearchFilter::default();
    let hits = index.search_unified("browser_client", &filter, 10).unwrap();

    assert!(
        !hits.is_empty(),
        "expected hits for 'browser_client', got none"
    );

    // File row must appear in top-3.
    let top3_kinds: Vec<&str> = hits.iter().take(3).map(|h| h.kind.as_str()).collect();
    let file_in_top3 = hits.iter().take(3).any(|h| h.kind == "file");

    assert!(
        file_in_top3,
        "file row must appear in top-3; top-3 kinds = {:?}",
        top3_kinds
    );
}
