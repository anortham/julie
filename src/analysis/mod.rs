//! Post-indexing analysis: test quality metrics, risk scoring.
//!
//! Runs after symbols are indexed and reference scores computed.
//! These analyses enrich symbol metadata with derived quality signals
//! that tools can surface to users.

pub mod test_quality;

pub use test_quality::compute_test_quality_metrics;
