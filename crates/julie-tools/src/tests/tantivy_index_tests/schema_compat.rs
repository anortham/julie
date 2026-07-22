use tempfile::TempDir;

use julie_index::search::index::{
    SearchDocument, SearchFilter, SearchIndex, SearchIndexOpenDisposition,
};
use julie_index::search::language_config::LanguageConfigs;

/// Regression test: opening a Tantivy index created with an older schema
/// (different field names / tokenizer) should recreate the index transparently
/// instead of crashing with "Error getting tokenizer for field: symbol_name".
///
/// This reproduces the bug reported when users upgraded from the pre-razorback
/// Julie version (which used `symbol_id`, `symbol_name`, `code_aware` tokenizer)
/// to the current version (which uses `id`, `name`, `code` tokenizer).
#[test]
fn test_schema_migration_recreates_stale_index() {
    use tantivy::schema::{
        IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions,
    };
    use tantivy::tokenizer::TextAnalyzer;

    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path).unwrap();

    // Create an index with the OLD schema (symbol_id, symbol_name, code_aware tokenizer)
    {
        let mut builder = Schema::builder();
        let old_text_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("code_aware")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();

        builder.add_text_field("doc_type", STRING | STORED);
        builder.add_text_field("symbol_id", STRING | STORED); // old name for "id"
        builder.add_text_field("file_path", STRING | STORED);
        builder.add_text_field("language", STRING | STORED);
        builder.add_text_field("symbol_name", old_text_options); // old name for "name"
        let old_schema = builder.build();

        let old_index = tantivy::Index::create_in_dir(&index_path, old_schema).unwrap();
        // Register the old tokenizer name so we can write a doc
        old_index.tokenizers().register(
            "code_aware",
            TextAnalyzer::builder(
                julie_index::search::tokenizer::CodeTokenizer::with_default_patterns(),
            )
            .build(),
        );
        let mut writer: tantivy::IndexWriter<tantivy::TantivyDocument> =
            old_index.writer(15_000_000).unwrap();
        writer.commit().unwrap();
        // Index with old schema now exists on disk
    }

    // open_or_create should detect the mismatch and recreate
    let index = SearchIndex::open_or_create(&index_path).unwrap();
    assert_eq!(index.num_docs(), 0, "recreated index should be empty");

    // Verify we can write and search with the new schema
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "test_sym",
            "MyTestClass",
            "class MyTestClass",
            "",
            "",
            "src/test.rs",
            "class",
            "rust",
            1,
        ))
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_symbols("MyTestClass", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "MyTestClass");
}

/// Same as above, but exercises the `open_with_language_configs` path
/// (used by `handler.rs` when loading existing workspaces at startup).
#[test]
fn test_schema_migration_via_open_path() {
    use tantivy::schema::{
        IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions,
    };
    use tantivy::tokenizer::TextAnalyzer;

    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path).unwrap();

    // Create old-schema index
    {
        let mut builder = Schema::builder();
        builder.add_text_field("doc_type", STRING | STORED);
        builder.add_text_field("symbol_id", STRING | STORED);
        builder.add_text_field("file_path", STRING | STORED);
        builder.add_text_field(
            "symbol_name",
            TextOptions::default()
                .set_indexing_options(
                    TextFieldIndexing::default()
                        .set_tokenizer("code_aware")
                        .set_index_option(IndexRecordOption::WithFreqsAndPositions),
                )
                .set_stored(),
        );
        let old_schema = builder.build();

        let old_index = tantivy::Index::create_in_dir(&index_path, old_schema).unwrap();
        old_index.tokenizers().register(
            "code_aware",
            TextAnalyzer::builder(
                julie_index::search::tokenizer::CodeTokenizer::with_default_patterns(),
            )
            .build(),
        );
        let mut writer: tantivy::IndexWriter<tantivy::TantivyDocument> =
            old_index.writer(15_000_000).unwrap();
        writer.commit().unwrap();
    }

    // open (not open_or_create) should also handle the migration
    let configs = LanguageConfigs::load_embedded();
    let index = SearchIndex::open_with_language_configs(&index_path, &configs).unwrap();
    assert_eq!(index.num_docs(), 0);

    // Verify writes work
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "sym1",
            "ProcessPayment",
            "fn process_payment()",
            "",
            "",
            "src/payments.rs",
            "function",
            "rust",
            10,
        ))
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_symbols("ProcessPayment", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert_eq!(results.len(), 1);
}

#[test]
fn test_open_or_create_recreates_when_compat_marker_missing() {
    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path).unwrap();
    let configs = LanguageConfigs::load_embedded();

    let index = SearchIndex::create_with_language_configs(&index_path, &configs).unwrap();
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "sym1",
            "NeedsRepair",
            "fn needs_repair()",
            "",
            "",
            "src/repair.rs",
            "function",
            "rust",
            1,
        ))
        .unwrap();
    index.commit().unwrap();
    drop(index);

    let compat_marker_path = index_path.join("julie-search-compat.json");
    assert!(compat_marker_path.exists(), "compat marker should exist");
    std::fs::remove_file(&compat_marker_path).unwrap();

    let outcome =
        SearchIndex::open_or_create_with_language_configs_outcome(&index_path, &configs).unwrap();
    assert_eq!(
        outcome.disposition,
        SearchIndexOpenDisposition::RecreatedIncompatible
    );
    assert!(outcome.repair_required());
    assert_eq!(outcome.index.num_docs(), 0);
}
