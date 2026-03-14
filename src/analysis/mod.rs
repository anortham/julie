//! Post-indexing analysis: test quality metrics, risk scoring.
//!
//! Runs after symbols are indexed and reference scores computed.
//! These analyses enrich symbol metadata with derived quality signals
//! that tools can surface to users.

pub mod test_quality;
pub mod test_coverage;

pub use test_quality::compute_test_quality_metrics;
pub use test_coverage::compute_test_coverage;
