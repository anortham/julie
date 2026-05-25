use tempfile::TempDir;

use crate::search::index::{SearchDocument, SearchFilter, SearchIndex};
use crate::search::language_config::LanguageConfigs;

/// Documents the tokenizer mismatch bug: index with language_configs tokenizer,
/// open with default tokenizer → search fails. This is the raw bug.
/// Ignored because it tests the broken path (SearchIndex::open without configs).
/// See test_ref_workspace_search_with_matching_tokenizer for the regression test.
#[test]
#[ignore]
fn test_tokenizer_mismatch_reproduces_ref_workspace_bug() {
    let temp_dir = TempDir::new().unwrap();

    {
        let configs = LanguageConfigs::load_embedded();
        let index = SearchIndex::create_with_language_configs(temp_dir.path(), &configs).unwrap();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "SmartQueryPreprocessor",
                "public class SmartQueryPreprocessor",
                "Preprocesses search queries",
                "public class SmartQueryPreprocessor { }",
                "Services/SmartQueryPreprocessor.cs",
                "class",
                "csharp",
                31,
            ))
            .unwrap();
        index.commit().unwrap();
    }

    // BUG: Opening with default tokenizer can't find symbols indexed with language_configs
    let index = SearchIndex::open(temp_dir.path()).unwrap();
    let results = index
        .search_symbols("SmartQueryPreprocessor", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        results.is_empty(),
        "This test documents the bug: mismatched tokenizer produces no results"
    );
}

/// Regression test: reference workspace search must use language_configs tokenizer.
/// Index created with from_language_configs, reopened with from_language_configs → works.
/// This simulates the fixed production path.
#[test]
fn test_ref_workspace_search_with_matching_tokenizer() {
    let temp_dir = TempDir::new().unwrap();

    // Step 1: Create and populate index (simulates reference workspace indexing)
    {
        let configs = LanguageConfigs::load_embedded();
        let index = SearchIndex::create_with_language_configs(temp_dir.path(), &configs).unwrap();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "SmartQueryPreprocessor",
                "public class SmartQueryPreprocessor",
                "Preprocesses search queries",
                "public class SmartQueryPreprocessor { }",
                "Services/SmartQueryPreprocessor.cs",
                "class",
                "csharp",
                31,
            ))
            .unwrap();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "2",
                "SearchMode",
                "public SearchMode SearchMode { get; set; }",
                "",
                "",
                "Services/SmartQueryPreprocessor.cs",
                "property",
                "csharp",
                395,
            ))
            .unwrap();

        index.commit().unwrap();
    }

    // Step 2: Reopen with language_configs tokenizer (simulates fixed search path)
    let configs = LanguageConfigs::load_embedded();
    let index = SearchIndex::open_with_language_configs(temp_dir.path(), &configs).unwrap();

    // CamelCase multi-part name
    let results = index
        .search_symbols("SmartQueryPreprocessor", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "SmartQueryPreprocessor must be found with matching tokenizer"
    );
    assert_eq!(results[0].name, "SmartQueryPreprocessor");

    // Individual token from CamelCase split
    let results = index
        .search_symbols("preprocessor", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "'preprocessor' must be found with matching tokenizer"
    );

    // 2-part CamelCase
    let results = index
        .search_symbols("SearchMode", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "SearchMode must be found with matching tokenizer"
    );
}

/// Control test: same scenario but open with SAME tokenizer (should always work).
#[test]
fn test_same_tokenizer_search_works() {
    let temp_dir = TempDir::new().unwrap();

    // Create and populate with language_configs tokenizer
    {
        let configs = LanguageConfigs::load_embedded();
        let index = SearchIndex::create_with_language_configs(temp_dir.path(), &configs).unwrap();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "SmartQueryPreprocessor",
                "public class SmartQueryPreprocessor",
                "",
                "",
                "Services/SmartQueryPreprocessor.cs",
                "class",
                "csharp",
                31,
            ))
            .unwrap();
        index.commit().unwrap();
    }

    // Open with SAME tokenizer
    let configs = LanguageConfigs::load_embedded();
    let index = SearchIndex::open_with_language_configs(temp_dir.path(), &configs).unwrap();

    let results = index
        .search_symbols("SmartQueryPreprocessor", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "SmartQueryPreprocessor should be found when using same tokenizer"
    );
    assert_eq!(results[0].name, "SmartQueryPreprocessor");

    let results = index
        .search_symbols("preprocessor", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "'preprocessor' should be found when using same tokenizer"
    );
}
