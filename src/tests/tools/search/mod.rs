// Tests extracted from src/tools/search.rs
// These were previously inline tests that have been moved to follow project standards

mod content_scoring_tests;
mod definition_overfetch_tests;
mod definition_promotion_tests;
mod file_pattern_tests;
mod lean_format_tests;
mod line_match_strategy_tests;
mod line_mode;
mod line_mode_or_fallback_tests;
mod line_mode_second_pass_tests;
mod primary_workspace_bug;
mod promotion_tests;
mod quality;
mod race_condition;
mod tantivy_affix_tests;
mod tantivy_index_tests;
mod tantivy_integration_tests;
mod tantivy_language_config_tests;
mod tantivy_path_prior_tests;
mod tantivy_query_expansion_tests;
mod tantivy_query_weighting_tests;
mod tantivy_scoring_tests;
mod tantivy_stemming_tests;
mod tantivy_tokenizer_tests;
mod tantivy_variants_tests;
