use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use julie::tools::search::SearchBackend;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMatrixCaseSet {
    pub cases: Vec<SearchMatrixCase>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMatrixCase {
    pub case_id: String,
    pub family: String,
    pub query: String,
    pub search_target: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub file_pattern: Option<String>,
    #[serde(default)]
    pub exclude_tests: Option<bool>,
    #[serde(default)]
    pub backend: Option<SearchBackend>,
    #[serde(default)]
    pub product_route: bool,
    #[serde(default)]
    pub profile_tags: Vec<String>,
    #[serde(default)]
    pub repo_selector: Option<Vec<String>>,
    pub expected_mode: String,
    #[serde(default)]
    pub expected_value: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMatrixCorpus {
    pub roots: Vec<String>,
    pub profiles: BTreeMap<String, SearchMatrixProfile>,
    pub repos: Vec<SearchMatrixRepo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMatrixProfile {
    pub repos: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMatrixRepo {
    pub name: String,
    pub language: String,
    #[serde(default)]
    pub profile_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatrixBaselineReport {
    pub profile: String,
    pub executions: Vec<SearchMatrixBaselineExecution>,
    pub skipped_repos: Vec<SearchMatrixSkippedRepo>,
    pub summary_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatrixBaselineExecution {
    pub repo_name: String,
    pub workspace_id: String,
    pub case_id: String,
    pub family: String,
    pub search_target: String,
    pub hit_count: usize,
    pub hit_count_is_lower_bound: bool,
    pub relaxed: bool,
    pub zero_hit_reason: Option<String>,
    pub file_pattern_diagnostic: Option<String>,
    pub hint_kind: Option<String>,
    pub latency_ms: u128,
    pub top_hits: Vec<SearchMatrixTopHit>,
    /// Ablation label for this execution. Empty string means no ablation (baseline).
    /// Serde default keeps existing reports parseable when this field is absent.
    #[serde(default)]
    pub ablation_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatrixTopHit {
    pub name: String,
    pub file: String,
    pub line: Option<u32>,
    pub kind: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatrixSkippedRepo {
    pub repo_name: String,
    pub reason: String,
}

impl SearchMatrixCaseSet {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
}

impl SearchMatrixCorpus {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
}
