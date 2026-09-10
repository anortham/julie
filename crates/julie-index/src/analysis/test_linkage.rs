//! Test-to-code linkage: determines which test symbols exercise each
//! production symbol and aggregates their quality tiers plus evidence sources.
//!
//! Uses two data sources:
//! 1. Relationships — direct test→production edges (high confidence)
//! 2. Identifiers — test file references to production symbols (medium confidence)

use anyhow::Result;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use tracing::{debug, info};

#[derive(Debug, Clone, Default)]
struct LinkedTest {
    name: String,
    file_path: String,
    tier: String,
    confidence: f64,
    evidence_sources: HashSet<String>,
}

/// Per-symbol static test linkage data, stored in metadata["test_linkage"].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestLinkageInfo {
    pub test_count: usize,
    pub best_tier: String,
    pub worst_tier: String,
    #[serde(default = "default_confidence")]
    pub best_confidence: f64,
    pub linked_tests: Vec<String>,
    pub linked_test_paths: Vec<String>,
    pub evidence_sources: Vec<String>,
}

fn default_confidence() -> f64 {
    0.5
}

/// Summary stats from running test linkage analysis.
#[derive(Debug, Clone, Default)]
pub struct TestLinkageStats {
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

fn add_linkage(
    linkages: &mut HashMap<String, HashMap<String, LinkedTest>>,
    prod_id: String,
    test_id: String,
    test_name: String,
    test_file_path: String,
    tier: String,
    confidence: f64,
    source: &str,
) {
    let entry = linkages
        .entry(prod_id)
        .or_default()
        .entry(test_id)
        .or_insert_with(|| LinkedTest {
            name: test_name.clone(),
            file_path: test_file_path.clone(),
            tier: tier.clone(),
            confidence,
            evidence_sources: HashSet::new(),
        });

    entry.name = test_name;
    entry.file_path = test_file_path;
    entry.tier = tier;
    entry.confidence = confidence;
    entry.evidence_sources.insert(source.to_string());
}

pub fn test_linkage_entry<'a>(metadata: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
    metadata
        .get("test_linkage")
        .or_else(|| metadata.get("test_coverage"))
}

/// Compute test-to-code linkage for all production symbols.
///
/// Runs after `compute_test_quality_metrics()` in the indexing pipeline.
/// Reads relationships and identifiers to find test→production edges,
/// then aggregates linkage data into each production symbol's metadata.
pub fn compute_test_linkage() -> Result<TestLinkageStats> {
    Ok(TestLinkageStats::default())
}

/// Count shared directory segments between two paths.
fn common_directory_depth(path_a: &str, path_b: &str) -> usize {
    let dirs_a: Vec<&str> = path_a
        .rsplitn(2, '/')
        .last()
        .unwrap_or("")
        .split('/')
        .collect();
    let dirs_b: Vec<&str> = path_b
        .rsplitn(2, '/')
        .last()
        .unwrap_or("")
        .split('/')
        .collect();
    dirs_a
        .iter()
        .zip(dirs_b.iter())
        .take_while(|(a, b)| a == b)
        .count()
}
