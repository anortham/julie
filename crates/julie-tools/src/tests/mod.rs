//! Handler-free tool tests relocated into julie-tools (T2b.6).
//!
//! Tests here have no dependency on `JulieServerHandler`, `crate::handler`,
//! `crate::daemon`, `crate::session`, or the `workspace` test helper.
//! They may use `julie_test_support::{db, tempdir, cleanup}`.

// Original 5 (pre-T2b.6)
pub mod blast_radius_formatting_tests;
pub mod filtering_tests;
pub mod hybrid_search_tests;
pub mod phase4_token_savings;
pub mod query_classification_tests;

// Deep dive (T2b.6)
pub mod deep_dive_regression_tests;
pub mod deep_dive_tests;

// Get context (T2b.6)
pub mod get_context_allocation_tests;
pub mod get_context_formatting_tests;
pub mod get_context_graph_expansion_tests;
pub mod get_context_pipeline_relevance_tests;
pub mod get_context_pipeline_tests;
pub mod get_context_quality_tests;
pub mod get_context_relevance_tests;
pub mod get_context_scoring_tests;
pub mod get_context_task_inputs_tests;
pub mod get_context_tests;
pub mod get_context_token_budget_tests;

// Editing (T2b.6)
pub mod editing_markdown_section_tests;
pub mod editing_security_tests;
pub mod editing_transactional_editing_tests;
pub mod editing_validation_tests;

// Refactoring (T2b.6)
pub mod refactoring_ast_aware;
pub mod refactoring_compute_line_changes_tests;
pub mod refactoring_import_update_tests;

// Metrics (T2b.6)
pub mod metrics_file_size_query_tests;
pub mod metrics_migration_tests;
// metrics_query_tests STAYS top-crate: #[path] binding to src/tools/metrics/query.rs uses crate::analysis, crate::database, crate::tools::search (top-crate-only paths)
pub mod metrics_tool_calls_db_tests;

// Search (T2b.6)
pub mod search_annotation_search_tests;
pub mod search_lean_format_tests;
pub mod search_line_match_strategy_tests;
pub mod search_nl_path_prior_pipeline_tests;
pub mod search_nl_symbol_query_latency_tests;
pub mod search_pretokenized_emit_test;
pub mod search_promotion_tests;
pub mod search_title_exact_boost_tests;
pub mod search_zero_hit_reason_tests;
pub mod tantivy_index_tests;
pub mod tantivy_integration_tests;
pub mod tantivy_path_prior_tests;

// Standalone formatting (T2b.6)
pub mod formatting_tests;
