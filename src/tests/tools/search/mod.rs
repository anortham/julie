// Tests extracted from src/tools/search.rs
// These were previously inline tests that have been moved to follow project standards
//
// Handler-free / decoupled tests have been relocated to crates/julie-index/src/tests/
// (see T3 in docs/plans/2026-06-03-julie-rescue-design.md).
// The tests below require top-crate handler or tools-layer access.

mod annotation_search_tests;
mod backend_param_tests;
mod content_scoring_tests;
mod fast_search_regression_tests;
mod fast_search_unified_cutover_test;
mod file_mode_tests;
mod file_pattern_tests;
mod lean_format_tests;
mod line_match_strategy_tests;
mod line_mode;
mod line_mode_or_fallback_tests;
mod line_mode_second_pass_tests;
mod nl_path_prior_pipeline_tests;
mod nl_symbol_query_latency_tests;
mod pretokenized_emit_test;
mod primary_workspace_bug;
mod promotion_tests;
mod quality;
mod race_condition;
mod relationship_text_test;
mod tantivy_index_tests;
mod tantivy_integration_tests;
mod tantivy_path_prior_tests;
mod title_exact_boost_tests;
mod unified_pass_filter_test;
mod zero_hit_reason_propagation_tests;
mod zero_hit_reason_tests;
