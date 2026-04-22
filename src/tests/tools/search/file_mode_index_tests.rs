use tempfile::TempDir;

use crate::search::index::{
    FileDocument, SEARCH_COMPAT_MARKER_FILE, SearchFilter, SearchIndex, SearchIndexOpenDisposition,
};
use crate::search::language_config::LanguageConfigs;

fn add_file_doc(index: &SearchIndex, file_path: &str, language: &str) {
    index
        .add_file_content(&FileDocument {
            file_path: file_path.into(),
            content: format!("// placeholder for {file_path}"),
            language: language.into(),
        })
        .unwrap();
}

#[test]
fn test_search_files_prefers_exact_basename_over_fragment_matches() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    add_file_doc(&index, "src/tools/search/line_mode.rs", "rust");
    add_file_doc(&index, "src/tests/tools/search/line_mode.rs.snap", "rust");
    index.commit().unwrap();

    let results = index
        .search_files("line_mode.rs", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert!(
        !results.is_empty(),
        "search_files should return at least one hit for exact basename queries"
    );
    assert_eq!(results[0].file_path, "src/tools/search/line_mode.rs");
    assert!(
        results
            .iter()
            .any(|result| result.file_path == "src/tests/tools/search/line_mode.rs.snap"),
        "fragment matches should still appear behind the exact basename hit"
    );
}

#[test]
fn test_search_files_prefers_exact_relative_path_over_same_basename() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    add_file_doc(&index, "src/tools/search/mod.rs", "rust");
    add_file_doc(&index, "src/tests/tools/search/mod.rs", "rust");
    index.commit().unwrap();

    let results = index
        .search_files("src/tools/search/mod.rs", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert!(
        !results.is_empty(),
        "search_files should return a hit for exact relative path queries"
    );
    assert_eq!(results[0].file_path, "src/tools/search/mod.rs");
}

#[test]
fn test_search_files_matches_glob_like_queries() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    add_file_doc(&index, "src/tools/search/mod.rs", "rust");
    add_file_doc(&index, "src/tools/workspace/mod.rs", "rust");
    add_file_doc(&index, "tests/search/mod.rs", "rust");
    index.commit().unwrap();

    let mut result_paths = index
        .search_files("src/**/mod.rs", &SearchFilter::default(), 10)
        .unwrap()
        .results
        .into_iter()
        .map(|result| result.file_path)
        .collect::<Vec<_>>();
    result_paths.sort();

    assert_eq!(
        result_paths,
        vec![
            "src/tools/search/mod.rs".to_string(),
            "src/tools/workspace/mod.rs".to_string()
        ]
    );
}

#[test]
fn test_open_or_create_recreates_when_compat_marker_version_is_stale() {
    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path).unwrap();
    let configs = LanguageConfigs::load_embedded();

    let index = SearchIndex::create_with_language_configs(&index_path, &configs).unwrap();
    add_file_doc(&index, "src/tools/search/mod.rs", "rust");
    index.commit().unwrap();
    drop(index);

    let marker_path = index_path.join(SEARCH_COMPAT_MARKER_FILE);
    let mut marker: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["marker_version"] = serde_json::Value::from(0);
    std::fs::write(&marker_path, serde_json::to_string_pretty(&marker).unwrap()).unwrap();

    let outcome =
        SearchIndex::open_or_create_with_language_configs_outcome(&index_path, &configs).unwrap();
    assert_eq!(
        outcome.disposition,
        SearchIndexOpenDisposition::RecreatedIncompatible
    );
    assert_eq!(outcome.index.num_docs(), 0);
}
