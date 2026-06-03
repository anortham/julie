use tempfile::TempDir;

use crate::search::index::{SearchDocument, SearchFilter, SearchIndex};

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
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "UserService",
            "pub struct UserService",
            "Manages users",
            "pub struct UserService { db: Database }",
            "src/user.rs",
            "class",
            "rust",
            10,
        ))
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_symbols("user", &SearchFilter::default(), 10)
        .unwrap()
        .results;
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
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/main.rs",
            "fn main() { println!(\"hello world\"); }",
            "rust",
        ))
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_content("println", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(!results.is_empty(), "Should find file containing 'println'");
    assert_eq!(results[0].file_path, "src/main.rs");
}

#[test]
fn test_language_filter() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "process",
            "fn process()",
            "",
            "",
            "src/lib.rs",
            "function",
            "rust",
            1,
        ))
        .unwrap();
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "2",
            "process",
            "function process()",
            "",
            "",
            "src/lib.ts",
            "function",
            "typescript",
            1,
        ))
        .unwrap();
    index.commit().unwrap();

    let filter = SearchFilter {
        language: Some("rust".to_string()),
        ..Default::default()
    };
    let results = index
        .search_symbols("process", &filter, 10)
        .unwrap()
        .results;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].language, "rust");
}

#[test]
fn test_delete_by_file_path() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1", "foo", "fn foo()", "", "", "src/a.rs", "function", "rust", 1,
        ))
        .unwrap();
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "2", "bar", "fn bar()", "", "", "src/b.rs", "function", "rust", 1,
        ))
        .unwrap();
    index.commit().unwrap();
    assert_eq!(index.num_docs(), 2);

    index.remove_by_file_path("src/a.rs").unwrap();
    index.commit().unwrap();
    assert_eq!(index.num_docs(), 1);
}
