//! Post-indexing analysis: test quality metrics and static test linkage.
//!
//! Runs after symbols are indexed and reference scores computed.
//! These analyses enrich symbol metadata with derived quality signals
//! that tools can surface to users.

pub mod early_warnings;
pub mod test_linkage;
pub mod test_quality;

pub use early_warnings::{
    AuthCoverageCandidate, EarlyWarningReport, EarlyWarningReportOptions, EntryPointSignal,
    ReportSummary, ReviewMarkerSignal, generate_early_warning_report,
};
pub use test_linkage::compute_test_linkage;
pub use test_quality::compute_test_quality_metrics;
