//! Search layer tests — indexing, scoring, tokenization, reranking, projection.

pub mod c3_enriched_schema_tests;
pub mod compat_marker_v4_test;
pub mod file_mode_index_tests;
pub mod language_config_embedded_tests;
pub mod projection_search_doc_test;
pub mod reranker_ordering_tests;
pub mod reranker_tests;
pub mod schema_phase2_fields_test;
pub mod search_index_concurrency_test;
pub mod tantivy_affix_tests;
pub mod tantivy_cross_process_reload_test;
pub mod tantivy_index_tests;
pub mod tantivy_language_config_tests;
pub mod tantivy_path_prior_tests;
pub mod tantivy_query_expansion_tests;
pub mod tantivy_query_weighting_tests;
pub mod tantivy_scoring_tests;
pub mod tantivy_tokenizer_tests;
pub mod tantivy_variants_tests;
pub mod tokenizer_simple_test;
pub mod unified_doc_index_test;
pub mod unified_query_path_test;
pub mod unified_reranker_test;
