use anyhow::Result;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::database::SymbolDatabase;
use crate::extractors::{AnnotationMarker, Symbol};
use crate::search::language_config::{LanguageConfig, LanguageConfigs};
use crate::tools::search::matches_glob_pattern;

const TANTIVY_PROJECTION: &str = "tantivy";
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
    pub canonical_revision: i64,
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

#[derive(Debug, Clone)]
struct ReportCacheKey {
    workspace_id: String,
    canonical_revision: i64,
    projection_revision: i64,
    config_schema_version: u32,
    file_pattern_key: String,
}

#[derive(Debug, Clone)]
struct AnnotationSets {
    entrypoint: HashSet<String>,
    auth: HashSet<String>,
    review: HashSet<String>,
    scheduler: HashSet<String>,
}

pub fn generate_early_warning_report(
    db: &SymbolDatabase,
    language_configs: &LanguageConfigs,
    options: EarlyWarningReportOptions,
) -> Result<EarlyWarningReport> {
    let file_pattern = normalize_file_pattern(options.file_pattern);
    let cache_key = build_cache_key(db, language_configs, &options.workspace_id, &file_pattern)?;

    if !options.fresh {
        if let Some(mut cached) = read_cached_report(db, &cache_key)? {
            apply_limit_per_section(&mut cached, options.limit_per_section);
            cached.from_cache = true;
            return Ok(cached);
        }
    }

    let mut report = build_report(db, language_configs, cache_key, file_pattern)?;
    write_cached_report(db, &report)?;
    apply_limit_per_section(&mut report, options.limit_per_section);
    report.from_cache = false;
    Ok(report)
}

fn build_report(
    db: &SymbolDatabase,
    language_configs: &LanguageConfigs,
    cache_key: ReportCacheKey,
    file_pattern: Option<String>,
) -> Result<EarlyWarningReport> {
    let symbols = db.get_all_symbols()?;
    let symbol_map: HashMap<String, &Symbol> = symbols
        .iter()
        .map(|symbol| (symbol.id.clone(), symbol))
        .collect();

    let mut sets_cache: HashMap<&str, AnnotationSets> = HashMap::new();

    let mut entry_points = Vec::new();
    let mut auth_coverage_candidates = Vec::new();
    let mut review_markers = Vec::new();
    let mut scheduler_signals = Vec::new();
    let mut auth_candidate_symbol_ids = HashSet::new();

    for symbol in symbols
        .iter()
        .filter(|symbol| matches_file_pattern(&symbol.file_path, file_pattern.as_deref()))
    {
        if symbol.annotations.is_empty() {
            continue;
        }
        let Some(config) = language_configs.get(&symbol.language) else {
            continue;
        };
        let sets = sets_cache
            .entry(&symbol.language)
            .or_insert_with(|| annotation_sets(config));

        for annotation in &symbol.annotations {
            if sets.entrypoint.contains(&annotation.annotation_key) {
                entry_points.push(entry_point_signal(symbol, annotation));
                if auth_candidate_symbol_ids.insert(symbol.id.clone()) {
                    let has_auth = has_auth_marker_in_owner_chain(symbol, &symbol_map, &sets.auth);
                    if !has_auth {
                        auth_coverage_candidates.push(auth_coverage_candidate(symbol, annotation));
                    }
                }
            }
            if sets.review.contains(&annotation.annotation_key) {
                review_markers.push(review_marker_signal(symbol, annotation));
            }
            if sets.scheduler.contains(&annotation.annotation_key) {
                scheduler_signals.push(scheduler_signal(symbol, annotation));
            }
        }
    }

    // Entry point linkage gaps: entry points with no observed test_linkage metadata
    let mut entry_point_linkage_gaps = Vec::new();
    for ep in &entry_points {
        let has_test_linkage = symbol_map
            .get(&ep.symbol_id)
            .and_then(|s| s.metadata.as_ref())
            .and_then(|m| m.get("test_linkage"))
            .is_some();
        if !has_test_linkage {
            entry_point_linkage_gaps.push(EntryPointLinkageGap {
                symbol_id: ep.symbol_id.clone(),
                symbol_name: ep.symbol_name.clone(),
                symbol_kind: ep.symbol_kind.clone(),
                language: ep.language.clone(),
                file_path: ep.file_path.clone(),
                start_line: ep.start_line,
                entry_annotation: ep.annotation.clone(),
            });
        }
    }

    let high_centrality_linkage_gaps =
        collect_high_centrality_linkage_gaps(db, file_pattern.as_deref(), 20)?;

    let summary = ReportSummary {
        entry_points: entry_points.len(),
        auth_coverage_candidates: auth_coverage_candidates.len(),
        review_markers: review_markers.len(),
        scheduler_signals: scheduler_signals.len(),
        entry_point_linkage_gaps: entry_point_linkage_gaps.len(),
        high_centrality_linkage_gaps: high_centrality_linkage_gaps.len(),
    };

    Ok(EarlyWarningReport {
        workspace_id: cache_key.workspace_id,
        file_pattern,
        generated_at: unix_timestamp_millis(),
        from_cache: false,
        canonical_revision: cache_key.canonical_revision,
        projection_revision: cache_key.projection_revision,
        config_schema_version: cache_key.config_schema_version,
        summary,
        entry_points,
        auth_coverage_candidates,
        review_markers,
        scheduler_signals,
        entry_point_linkage_gaps,
        high_centrality_linkage_gaps,
    })
}

fn build_cache_key(
    db: &SymbolDatabase,
    language_configs: &LanguageConfigs,
    workspace_id: &str,
    file_pattern: &Option<String>,
) -> Result<ReportCacheKey> {
    let canonical_revision = db
        .get_latest_canonical_revision(workspace_id)?
        .map(|revision| revision.revision)
        .unwrap_or(0);
    let projection_revision = db
        .get_projection_state(TANTIVY_PROJECTION, workspace_id)?
        .and_then(|state| state.projected_revision)
        .unwrap_or(0);
    let config_schema_version = workspace_config_schema_version(db, language_configs)?;

    Ok(ReportCacheKey {
        workspace_id: workspace_id.to_string(),
        canonical_revision,
        projection_revision,
        config_schema_version,
        file_pattern_key: file_pattern.clone().unwrap_or_default(),
    })
}

fn workspace_config_schema_version(
    db: &SymbolDatabase,
    language_configs: &LanguageConfigs,
) -> Result<u32> {
    let mut stmt = db
        .conn
        .prepare("SELECT DISTINCT language FROM symbols ORDER BY language")?;
    let languages = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut version = DEFAULT_CONFIG_SCHEMA_VERSION;
    for language in languages {
        if let Some(config) = language_configs.get(&language) {
            version = version.max(config.early_warnings.schema_version);
        }
    }

    Ok(version)
}

fn read_cached_report(
    db: &SymbolDatabase,
    cache_key: &ReportCacheKey,
) -> Result<Option<EarlyWarningReport>> {
    let serialized_json: Option<String> = db
        .conn
        .query_row(
            "SELECT serialized_json
             FROM early_warning_reports
             WHERE workspace_id = ?1
               AND canonical_revision = ?2
               AND projection_revision = ?3
               AND config_schema_version = ?4
               AND file_pattern = ?5",
            params![
                &cache_key.workspace_id,
                cache_key.canonical_revision,
                cache_key.projection_revision,
                cache_key.config_schema_version,
                &cache_key.file_pattern_key
            ],
            |row| row.get(0),
        )
        .optional()?;

    serialized_json
        .map(|json| serde_json::from_str(&json).map_err(Into::into))
        .transpose()
}

fn write_cached_report(db: &SymbolDatabase, report: &EarlyWarningReport) -> Result<()> {
    let serialized_json = serde_json::to_string(report)?;
    db.conn.execute(
        "INSERT INTO early_warning_reports
         (workspace_id, canonical_revision, projection_revision, config_schema_version,
          file_pattern, generated_at, serialized_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(workspace_id, canonical_revision, projection_revision, config_schema_version, file_pattern)
         DO UPDATE SET
            generated_at = excluded.generated_at,
            serialized_json = excluded.serialized_json",
        params![
            &report.workspace_id,
            report.canonical_revision,
            report.projection_revision,
            report.config_schema_version,
            report.file_pattern.clone().unwrap_or_default(),
            report.generated_at,
            serialized_json
        ],
    )?;
    db.conn.execute(
        "DELETE FROM early_warning_reports
         WHERE workspace_id = ?1
           AND file_pattern = ?2
           AND NOT (
                canonical_revision = ?3
            AND projection_revision = ?4
            AND config_schema_version = ?5
           )",
        params![
            &report.workspace_id,
            report.file_pattern.clone().unwrap_or_default(),
            report.canonical_revision,
            report.projection_revision,
            report.config_schema_version
        ],
    )?;
    Ok(())
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

fn has_auth_marker_in_owner_chain(
    symbol: &Symbol,
    symbol_map: &HashMap<String, &Symbol>,
    auth_keys: &HashSet<String>,
) -> bool {
    let mut visited = HashSet::new();
    let mut current = Some(symbol);

    while let Some(candidate) = current {
        if !visited.insert(candidate.id.as_str()) {
            return false;
        }

        if candidate
            .annotations
            .iter()
            .any(|a| auth_keys.contains(&a.annotation_key))
        {
            return true;
        }

        current = candidate
            .parent_id
            .as_ref()
            .and_then(|parent_id| symbol_map.get(parent_id).copied());
    }

    false
}

fn entry_point_signal(symbol: &Symbol, annotation: &AnnotationMarker) -> EntryPointSignal {
    EntryPointSignal {
        symbol_id: symbol.id.clone(),
        symbol_name: symbol.name.clone(),
        symbol_kind: symbol.kind.to_string(),
        language: symbol.language.clone(),
        file_path: symbol.file_path.clone(),
        start_line: symbol.start_line,
        annotation: annotation.annotation.clone(),
        annotation_key: annotation.annotation_key.clone(),
        raw_text: annotation.raw_text.clone(),
    }
}

fn auth_coverage_candidate(
    symbol: &Symbol,
    annotation: &AnnotationMarker,
) -> AuthCoverageCandidate {
    AuthCoverageCandidate {
        symbol_id: symbol.id.clone(),
        symbol_name: symbol.name.clone(),
        symbol_kind: symbol.kind.to_string(),
        language: symbol.language.clone(),
        file_path: symbol.file_path.clone(),
        start_line: symbol.start_line,
        annotation: annotation.annotation.clone(),
        annotation_key: annotation.annotation_key.clone(),
        raw_text: annotation.raw_text.clone(),
    }
}

fn scheduler_signal(symbol: &Symbol, annotation: &AnnotationMarker) -> SchedulerSignal {
    SchedulerSignal {
        symbol_id: symbol.id.clone(),
        symbol_name: symbol.name.clone(),
        symbol_kind: symbol.kind.to_string(),
        language: symbol.language.clone(),
        file_path: symbol.file_path.clone(),
        start_line: symbol.start_line,
        annotation: annotation.annotation.clone(),
        annotation_key: annotation.annotation_key.clone(),
        raw_text: annotation.raw_text.clone(),
    }
}

fn review_marker_signal(symbol: &Symbol, annotation: &AnnotationMarker) -> ReviewMarkerSignal {
    ReviewMarkerSignal {
        symbol_id: symbol.id.clone(),
        symbol_name: symbol.name.clone(),
        symbol_kind: symbol.kind.to_string(),
        language: symbol.language.clone(),
        file_path: symbol.file_path.clone(),
        start_line: symbol.start_line,
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

fn collect_high_centrality_linkage_gaps(
    db: &SymbolDatabase,
    file_pattern: Option<&str>,
    limit: usize,
) -> Result<Vec<HighCentralityLinkageGap>> {
    // Query symbols with reference_score > 0, not tests, and no test_linkage metadata.
    // Over-fetch by 4x to allow for file_pattern filtering after the query.
    let fetch_limit = if file_pattern.is_some() {
        limit * 4
    } else {
        limit
    };
    let mut stmt = db.conn.prepare(
        "SELECT id, name, kind, language, file_path, start_line, reference_score
         FROM symbols
         WHERE reference_score > 0
           AND (json_extract(metadata, '$.is_test') IS NULL
                OR json_extract(metadata, '$.is_test') != 1)
           AND json_extract(metadata, '$.test_linkage') IS NULL
         ORDER BY reference_score DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![fetch_limit as i64], |row| {
        Ok(HighCentralityLinkageGap {
            symbol_id: row.get(0)?,
            symbol_name: row.get(1)?,
            symbol_kind: row.get(2)?,
            language: row.get(3)?,
            file_path: row.get(4)?,
            start_line: row.get(5)?,
            reference_score: row.get(6)?,
        })
    })?;

    let mut gaps = Vec::new();
    for row_result in rows {
        let gap = row_result?;
        if matches_file_pattern(&gap.file_path, file_pattern) {
            gaps.push(gap);
            if gaps.len() >= limit {
                break;
            }
        }
    }
    Ok(gaps)
}

fn unix_timestamp_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
