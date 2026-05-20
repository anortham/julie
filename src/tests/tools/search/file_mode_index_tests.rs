use tempfile::TempDir;

use crate::search::index::{
    FileDocument, FileMatchKind, SEARCH_COMPAT_MARKER_FILE, SearchFilter, SearchIndex,
    SearchIndexOpenDisposition, classify_file_match,
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
fn test_search_files_prefers_hidden_directory_path_for_hidden_directory_query() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    add_file_doc(&index, "Cargo.toml", "toml");
    add_file_doc(&index, "Cargo.lock", "toml");
    add_file_doc(&index, ".cargo/config.toml", "toml");
    index.commit().unwrap();

    let results = index
        .search_files(".cargo", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert!(
        results.len() >= 3,
        "query '.cargo' should return the hidden directory path and root Cargo files"
    );
    assert_eq!(
        results[0].file_path, ".cargo/config.toml",
        "hidden directory path should rank ahead of root Cargo files"
    );
    assert!(
        results[1..]
            .iter()
            .any(|result| result.file_path == "Cargo.toml"),
        "Cargo.toml should still be returned behind the hidden directory path"
    );
    assert!(
        results[1..]
            .iter()
            .any(|result| result.file_path == "Cargo.lock"),
        "Cargo.lock should still be returned behind the hidden directory path"
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

// ── Finding C: Extension-blind exact basename matching ──

#[test]
fn test_search_files_promotes_extensionless_query_to_exact_basename() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    add_file_doc(&index, "src/foo/bar.rs", "rust");
    add_file_doc(&index, "src/baz/bar_helper.rs", "rust");
    index.commit().unwrap();

    let results = index
        .search_files("bar", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert!(!results.is_empty(), "query 'bar' should return results");
    assert_eq!(
        results[0].file_path, "src/foo/bar.rs",
        "bar.rs should be ranked ahead via ExactBasename"
    );
}

#[test]
fn test_search_files_does_not_match_wrong_extension() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    add_file_doc(&index, "bar.rs", "rust");
    add_file_doc(&index, "bar.py", "python");
    index.commit().unwrap();

    let results = index
        .search_files("bar.py", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert!(!results.is_empty());
    assert_eq!(results[0].file_path, "bar.py");

    // Verify bar.rs is NOT classified as ExactBasename for query "bar.py"
    let kind = classify_file_match("bar.py", "bar.py", "bar.rs");
    assert!(
        !matches!(kind, FileMatchKind::ExactBasename),
        "bar.rs should not be ExactBasename for query 'bar.py'"
    );
}

#[test]
fn test_search_files_extensionless_file_still_matches() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    add_file_doc(&index, "src/Makefile", "makefile");
    index.commit().unwrap();

    let results = index
        .search_files("Makefile", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert!(
        !results.is_empty(),
        "query 'Makefile' should return results"
    );
    assert_eq!(results[0].file_path, "src/Makefile");
    assert!(
        matches!(results[0].match_kind, FileMatchKind::ExactBasename),
        "Makefile should classify as ExactBasename through search_files"
    );

    // Query "Makefile" against file "src/Makefile" — no extension, so basename
    // equality catches it as ExactBasename (no dot to strip).
    let kind = classify_file_match("Makefile", "Makefile", "src/Makefile");
    assert!(
        matches!(kind, FileMatchKind::ExactBasename),
        "Makefile should classify as ExactBasename (regression guard)"
    );
}

#[test]
fn test_search_files_hidden_file_suffix_not_promoted() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    add_file_doc(&index, ".gitignore", "gitignore");
    index.commit().unwrap();

    let results = index
        .search_files("gitignore", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert!(
        !results.is_empty(),
        "query 'gitignore' should return results"
    );
    assert_eq!(results[0].file_path, ".gitignore");
    assert!(
        matches!(results[0].match_kind, FileMatchKind::PathFragment),
        "query 'gitignore' should not promote .gitignore to ExactBasename through search_files"
    );

    let results = index
        .search_files(".gitignore", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert!(
        !results.is_empty(),
        "query '.gitignore' should return results"
    );
    assert_eq!(results[0].file_path, ".gitignore");
    assert!(
        matches!(results[0].match_kind, FileMatchKind::ExactPath),
        "query '.gitignore' should match .gitignore via ExactPath through search_files"
    );

    // Query "gitignore" against file ".gitignore" — stem is empty, so NO ExactBasename
    let kind = classify_file_match("gitignore", "gitignore", ".gitignore");
    assert!(
        !matches!(kind, FileMatchKind::ExactBasename),
        "query 'gitignore' should NOT match .gitignore as ExactBasename (stem is empty)"
    );

    // Query ".gitignore" against file ".gitignore" — equality path catches it as ExactPath
    let kind = classify_file_match(".gitignore", ".gitignore", ".gitignore");
    assert!(
        matches!(kind, FileMatchKind::ExactPath),
        "query '.gitignore' should match .gitignore via the ExactPath equality path"
    );
}

#[test]
fn test_search_files_dotted_extension_file_classifies_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    add_file_doc(&index, ".env.local", "dotenv");
    index.commit().unwrap();

    let results = index
        .search_files(".env", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert!(!results.is_empty(), "query '.env' should return results");
    assert_eq!(results[0].file_path, ".env.local");
    assert!(
        matches!(results[0].match_kind, FileMatchKind::ExactBasename),
        "query '.env' should promote .env.local to ExactBasename through search_files"
    );

    // .env.local → rsplit_once('.') gives (".env", "local")
    // Query ".env" matches stem ".env" → ExactBasename
    let kind = classify_file_match(".env", ".env", ".env.local");
    assert!(
        matches!(kind, FileMatchKind::ExactBasename),
        "query '.env' should promote .env.local to ExactBasename via stem comparison"
    );
}
