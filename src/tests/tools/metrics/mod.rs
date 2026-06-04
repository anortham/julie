// file_size_query_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
// migration_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
// tool_calls_db_tests relocated to crates/julie-tools/src/tests/ (T2b.6)
pub mod query_tests; // STAYS: #[path] binding to src/tools/metrics/query.rs uses crate::analysis, crate::database (top-crate-only)
pub mod session_metrics_tests;
