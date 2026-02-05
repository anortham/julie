//! Tests for Tantivy search index.

use tempfile::TempDir;

use crate::search::index::{FileDocument, SearchFilter, SearchIndex, SymbolDocument};

#[test]
fn test_create_index() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();
    assert_eq!(index.num_docs(), 0);
    assert!(temp_dir.path().join("meta.json").exists());
}

#[test]
fn test_open_existing_index() {
    let temp_dir = TempDir::new().unwrap();
    {
        let _index = SearchIndex::create(temp_dir.path()).unwrap();
    }
    let index = SearchIndex::open(temp_dir.path()).unwrap();
    assert_eq!(index.num_docs(), 0);
}

#[test]
fn test_open_or_create() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::open_or_create(temp_dir.path()).unwrap();
    assert_eq!(index.num_docs(), 0);
}

#[test]
fn test_add_symbol_and_search() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "UserService".into(),
            signature: "pub struct UserService".into(),
            doc_comment: "Manages users".into(),
            code_body: "pub struct UserService { db: Database }".into(),
            file_path: "src/user.rs".into(),
            kind: "class".into(),
            language: "rust".into(),
            start_line: 10,
        })
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_symbols("user", &SearchFilter::default(), 10)
        .unwrap();
    assert!(
        !results.is_empty(),
        "Should find UserService when searching 'user'"
    );
    assert_eq!(results[0].name, "UserService");
}

#[test]
fn test_add_file_content_and_search() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_file_content(&FileDocument {
            file_path: "src/main.rs".into(),
            content: "fn main() { println!(\"hello world\"); }".into(),
            language: "rust".into(),
        })
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_content("println", &SearchFilter::default(), 10)
        .unwrap();
    assert!(
        !results.is_empty(),
        "Should find file containing 'println'"
    );
    assert_eq!(results[0].file_path, "src/main.rs");
}

#[test]
fn test_name_match_ranks_higher_than_body() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "process_data".into(),
            signature: "fn process_data()".into(),
            doc_comment: "".into(),
            code_body: "fn process_data() {}".into(),
            file_path: "src/a.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "handle_request".into(),
            signature: "fn handle_request()".into(),
            doc_comment: "This will process the data".into(),
            code_body: "fn handle_request() {}".into(),
            file_path: "src/b.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_symbols("process", &SearchFilter::default(), 10)
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(
        results[0].name, "process_data",
        "Name match should rank first"
    );
}

#[test]
fn test_language_filter() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "process".into(),
            signature: "fn process()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/lib.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();
    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "process".into(),
            signature: "function process()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/lib.ts".into(),
            kind: "function".into(),
            language: "typescript".into(),
            start_line: 1,
        })
        .unwrap();
    index.commit().unwrap();

    let filter = SearchFilter {
        language: Some("rust".into()),
        ..Default::default()
    };
    let results = index.search_symbols("process", &filter, 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].language, "rust");
}

#[test]
fn test_delete_by_file_path() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "foo".into(),
            signature: "fn foo()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/a.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();
    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "bar".into(),
            signature: "fn bar()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/b.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();
    index.commit().unwrap();
    assert_eq!(index.num_docs(), 2);

    index.remove_by_file_path("src/a.rs").unwrap();
    index.commit().unwrap();
    assert_eq!(index.num_docs(), 1);
}

#[test]
fn test_camel_case_cross_convention_search() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "getUserData".into(),
            signature: "fn getUserData()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/api.ts".into(),
            kind: "function".into(),
            language: "typescript".into(),
            start_line: 1,
        })
        .unwrap();
    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "get_user_data".into(),
            signature: "fn get_user_data()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/api.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_symbols("user", &SearchFilter::default(), 10)
        .unwrap();
    assert_eq!(
        results.len(),
        2,
        "Should find both getUserData and get_user_data when searching 'user'"
    );
}
