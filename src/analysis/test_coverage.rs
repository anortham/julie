//! Test-to-code linkage: determines which test symbols exercise each
//! production symbol and aggregates their quality tiers.
//!
//! Uses two data sources:
//! 1. Relationships — direct test→production edges (high confidence)
//! 2. Identifiers — test file references to production symbols (medium confidence)

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tracing::{debug, info};

use crate::database::SymbolDatabase;

/// Per-symbol test coverage data, stored in metadata["test_coverage"].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestCoverageInfo {
    pub test_count: usize,
    pub best_tier: String,
    pub worst_tier: String,
    pub covering_tests: Vec<String>,
}

/// Summary stats from running test coverage analysis.
#[derive(Debug, Clone, Default)]
pub struct TestCoverageStats {
    pub symbols_covered: usize,
    pub total_linkages: usize,
}

/// Rank quality tiers for comparison (higher = better).
pub fn tier_rank(tier: &str) -> u8 {
    match tier {
        "thorough" => 4,
        "adequate" => 3,
        "thin" => 2,
        "stub" => 1,
        _ => 0,
    }
}

/// Compute test-to-code linkage for all production symbols.
///
/// Runs after `compute_test_quality_metrics()` in the indexing pipeline.
/// Reads relationships and identifiers to find test→production edges,
/// then aggregates coverage data into each production symbol's metadata.
pub fn compute_test_coverage(_db: &SymbolDatabase) -> Result<TestCoverageStats> {
    todo!("implement in Task 2")
}
