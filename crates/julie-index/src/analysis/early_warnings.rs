use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::search::language_config::{LanguageConfig, LanguageConfigs};
use julie_core::glob::matches_glob_pattern;
use julie_extractors::AnnotationMarker;

const DEFAULT_CONFIG_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarlyWarningReportOptions {
    pub workspace_id: String,
    pub file_pattern: Option<String>,
    pub fresh: bool,
    pub limit_per_section: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EarlyWarningReport {
    pub workspace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_pattern: Option<String>,
    pub generated_at: i64,
    pub from_cache: bool,
    pub facts_revision: i64,
    pub projection_revision: i64,
    pub config_schema_version: u32,
    pub summary: ReportSummary,
    pub entry_points: Vec<EntryPointSignal>,
    pub auth_coverage_candidates: Vec<AuthCoverageCandidate>,
    pub review_markers: Vec<ReviewMarkerSignal>,
    pub scheduler_signals: Vec<SchedulerSignal>,
    pub entry_point_linkage_gaps: Vec<EntryPointLinkageGap>,
    pub high_centrality_linkage_gaps: Vec<HighCentralityLinkageGap>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReportSummary {
    pub entry_points: usize,
    pub auth_coverage_candidates: usize,
    pub review_markers: usize,
    pub scheduler_signals: usize,
    pub entry_point_linkage_gaps: usize,
    pub high_centrality_linkage_gaps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntryPointSignal {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub annotation: String,
    pub annotation_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthCoverageCandidate {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub annotation: String,
    pub annotation_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewMarkerSignal {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub annotation: String,
    pub annotation_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchedulerSignal {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub annotation: String,
    pub annotation_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntryPointLinkageGap {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub entry_annotation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HighCentralityLinkageGap {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub reference_score: f64,
}

pub fn generate_early_warning_report(
    snapshot: &crate::snapshot::Snapshot,
    language_configs: &LanguageConfigs,
    options: EarlyWarningReportOptions,
) -> Result<EarlyWarningReport> {
    let file_pattern = normalize_file_pattern(options.file_pattern);
    let graph = snapshot.graph();
    let mut sets_cache: HashMap<&str, AnnotationSets> = HashMap::new();
    let mut entry_points = Vec::new();
    let mut auth_coverage_candidates = Vec::new();
    let mut review_markers = Vec::new();
    let mut scheduler_signals = Vec::new();
    let mut auth_candidate_symbol_ids = HashSet::new();

    for index in 0..graph.len() {
        let row = graph.symbol(crate::graph::SymbolId(index as u32));
        if !matches_file_pattern(&row.path, file_pattern.as_deref()) {
            continue;
        }
        if row.annotations.is_empty() {
            continue;
        }
        let Some(config) = language_configs.get(&row.language) else {
            continue;
        };
        let sets = sets_cache
            .entry(&row.language)
            .or_insert_with(|| annotation_sets(config));
        for annotation in &row.annotations {
            if sets.entrypoint.contains(&annotation.annotation_key) {
                entry_points.push(entry_point_signal(row, annotation));
                if auth_candidate_symbol_ids.insert(row.id.clone()) {
                    let has_auth = row
                        .annotations
                        .iter()
                        .any(|a| sets.auth.contains(&a.annotation_key));
                    if !has_auth {
                        auth_coverage_candidates.push(auth_coverage_candidate(row, annotation));
                    }
                }
            }
            if sets.review.contains(&annotation.annotation_key) {
                review_markers.push(review_marker_signal(row, annotation));
            }
            if sets.scheduler.contains(&annotation.annotation_key) {
                scheduler_signals.push(scheduler_signal(row, annotation));
            }
        }
    }

    let mut report = EarlyWarningReport {
        workspace_id: options.workspace_id,
        file_pattern,
        generated_at: unix_timestamp_millis(),
        from_cache: false,
        facts_revision: 0,
        projection_revision: 0,
        config_schema_version: DEFAULT_CONFIG_SCHEMA_VERSION,
        summary: ReportSummary {
            entry_points: entry_points.len(),
            auth_coverage_candidates: auth_coverage_candidates.len(),
            review_markers: review_markers.len(),
            scheduler_signals: scheduler_signals.len(),
            entry_point_linkage_gaps: 0,
            high_centrality_linkage_gaps: 0,
        },
        entry_points,
        auth_coverage_candidates,
        review_markers,
        scheduler_signals,
        entry_point_linkage_gaps: Vec::new(),
        high_centrality_linkage_gaps: Vec::new(),
    };
    apply_limit_per_section(&mut report, options.limit_per_section);
    Ok(report)
}

struct AnnotationSets {
    entrypoint: HashSet<String>,
    auth: HashSet<String>,
    review: HashSet<String>,
    scheduler: HashSet<String>,
}

fn annotation_sets(config: &LanguageConfig) -> AnnotationSets {
    let entrypoint = config
        .annotation_classes
        .entrypoint
        .iter()
        .cloned()
        .collect();
    let mut auth: HashSet<String> = config.annotation_classes.auth.iter().cloned().collect();
    auth.extend(config.annotation_classes.auth_bypass.iter().cloned());
    let mut review: HashSet<String> = config
        .annotation_classes
        .auth_bypass
        .iter()
        .cloned()
        .collect();
    review.extend(config.early_warnings.review_markers.iter().cloned());
    let scheduler = config
        .annotation_classes
        .scheduler
        .iter()
        .cloned()
        .collect();
    AnnotationSets {
        entrypoint,
        auth,
        review,
        scheduler,
    }
}

fn entry_point_signal(
    symbol: &julie_facts::rows::SymbolRow,
    annotation: &AnnotationMarker,
) -> EntryPointSignal {
    EntryPointSignal {
        symbol_id: symbol.id.clone(),
        symbol_name: symbol.name.clone(),
        symbol_kind: format!("{:?}", symbol.kind).to_lowercase(),
        language: symbol.language.clone(),
        file_path: symbol.path.clone(),
        start_line: symbol.span.start_line,
        annotation: annotation.annotation.clone(),
        annotation_key: annotation.annotation_key.clone(),
        raw_text: annotation.raw_text.clone(),
    }
}

fn auth_coverage_candidate(
    symbol: &julie_facts::rows::SymbolRow,
    annotation: &AnnotationMarker,
) -> AuthCoverageCandidate {
    AuthCoverageCandidate {
        symbol_id: symbol.id.clone(),
        symbol_name: symbol.name.clone(),
        symbol_kind: format!("{:?}", symbol.kind).to_lowercase(),
        language: symbol.language.clone(),
        file_path: symbol.path.clone(),
        start_line: symbol.span.start_line,
        annotation: annotation.annotation.clone(),
        annotation_key: annotation.annotation_key.clone(),
        raw_text: annotation.raw_text.clone(),
    }
}

fn scheduler_signal(
    symbol: &julie_facts::rows::SymbolRow,
    annotation: &AnnotationMarker,
) -> SchedulerSignal {
    SchedulerSignal {
        symbol_id: symbol.id.clone(),
        symbol_name: symbol.name.clone(),
        symbol_kind: format!("{:?}", symbol.kind).to_lowercase(),
        language: symbol.language.clone(),
        file_path: symbol.path.clone(),
        start_line: symbol.span.start_line,
        annotation: annotation.annotation.clone(),
        annotation_key: annotation.annotation_key.clone(),
        raw_text: annotation.raw_text.clone(),
    }
}

fn review_marker_signal(
    symbol: &julie_facts::rows::SymbolRow,
    annotation: &AnnotationMarker,
) -> ReviewMarkerSignal {
    ReviewMarkerSignal {
        symbol_id: symbol.id.clone(),
        symbol_name: symbol.name.clone(),
        symbol_kind: format!("{:?}", symbol.kind).to_lowercase(),
        language: symbol.language.clone(),
        file_path: symbol.path.clone(),
        start_line: symbol.span.start_line,
        annotation: annotation.annotation.clone(),
        annotation_key: annotation.annotation_key.clone(),
        raw_text: annotation.raw_text.clone(),
    }
}

fn matches_file_pattern(file_path: &str, file_pattern: Option<&str>) -> bool {
    file_pattern
        .map(|pattern| matches_glob_pattern(file_path, pattern))
        .unwrap_or(true)
}

fn normalize_file_pattern(file_pattern: Option<String>) -> Option<String> {
    file_pattern.and_then(|pattern| {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn truncate_if_needed<T>(items: &mut Vec<T>, limit: Option<usize>) {
    if let Some(limit) = limit {
        items.truncate(limit);
    }
}

fn apply_limit_per_section(report: &mut EarlyWarningReport, limit: Option<usize>) {
    truncate_if_needed(&mut report.entry_points, limit);
    truncate_if_needed(&mut report.auth_coverage_candidates, limit);
    truncate_if_needed(&mut report.review_markers, limit);
    truncate_if_needed(&mut report.scheduler_signals, limit);
    truncate_if_needed(&mut report.entry_point_linkage_gaps, limit);
    truncate_if_needed(&mut report.high_centrality_linkage_gaps, limit);
}

fn unix_timestamp_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
