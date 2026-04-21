//! Line-mode (grep-style) search
//!
//! Provides grep-style line-by-line searching with file paths and line numbers.
//! Used when output format is set to "lines".

use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use tracing::{debug, warn};

use crate::handler::JulieServerHandler;
use crate::search::SearchFilter;
use crate::tools::navigation::resolution::WorkspaceTarget;

use super::query::{line_match_strategy, line_matches, matches_glob_pattern};
use super::types::{LineMatch, LineMatchStrategy};

pub(crate) struct LineModeSearchResult {
    pub matches: Vec<LineMatch>,
    pub relaxed: bool,
    pub strategy: LineMatchStrategy,
    pub workspace_label: String,
    pub stage_counts: LineModeStageCounts,
}

/// Per-stage drop counters for the `line_mode_matches` pipeline.
///
/// Each field records how many candidates each filter stage dropped.
/// Used for zero-hit diagnosis: when `matches.is_empty()`, the first
/// stage where `count_before > 0 && count_after == 0` is the culprit.
/// Task 4 translates this into a `ZeroHitReason` enum; Task 3 keeps
/// the raw counts so the replay report can attribute every zero-hit
/// to a specific stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LineModeStageCounts {
    /// AND-stage candidate count surfaced by `SearchIndex::search_content`.
    pub and_candidates: usize,
    /// OR-stage candidate count (0 if OR fallback was not invoked).
    pub or_candidates: usize,
    /// Files fed from Tantivy into the per-file filter loop.
    pub tantivy_file_candidates: usize,
    /// Files dropped by the `file_pattern` filter inside the loop.
    pub file_pattern_dropped: usize,
    /// Files dropped by the language filter inside the loop.
    pub language_dropped: usize,
    /// Files dropped by the `exclude_tests` filter inside the loop.
    pub test_dropped: usize,
    /// Files where `get_file_content` returned `None` (content unavailable).
    pub file_content_unavailable_dropped: usize,
    /// Files that passed every filter but produced zero line-level matches.
    pub line_match_miss_dropped: usize,
}

pub(crate) fn query_uses_file_level_header(query: &str) -> bool {
    matches!(
        line_match_strategy(query),
        LineMatchStrategy::FileLevel { .. }
    )
}

/// Line-level search mode (grep-style output with line numbers)
///
/// Returns every line matching the query with file:line_number:line_content format.
/// Supports language and file pattern filtering for targeted searches.
///
/// Accepts a pre-resolved `WorkspaceTarget` to avoid redundant workspace resolution.
/// The caller (`FastSearchTool::call_tool`) resolves the workspace once and passes it here.
#[allow(dead_code)]
pub async fn line_mode_search(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    exclude_tests: Option<bool>,
    workspace_target: &WorkspaceTarget,
    handler: &JulieServerHandler,
) -> Result<CallToolResult> {
    let result = line_mode_matches(
        query,
        language,
        file_pattern,
        limit,
        exclude_tests,
        workspace_target,
        handler,
    )
    .await?;

    if result.matches.is_empty() {
        let message = format!(
            "🔍 No lines found matching: '{}'\n\
            💡 Try search_target=\"definitions\" if looking for a symbol name, or broaden file_pattern/language filters",
            query
        );
        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
    }

    Ok(CallToolResult::text_content(vec![Content::text(
        format_line_mode_output(query, &result),
    )]))
}

pub(crate) async fn line_mode_matches(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    exclude_tests: Option<bool>,
    workspace_target: &WorkspaceTarget,
    handler: &JulieServerHandler,
) -> Result<LineModeSearchResult> {
    debug!("📄 Line-level search for: '{}'", query);

    let exclude_test_files = exclude_tests.unwrap_or(false);
    let workspace_label = match workspace_target {
        WorkspaceTarget::Primary => "primary".to_string(),
        WorkspaceTarget::Target(id) => id.clone(),
    };
    let match_strategy = line_match_strategy(query);
    let base_limit = limit.max(1) as usize;
    let has_file_filter = file_pattern.is_some();
    let fetch_limit = if has_file_filter {
        base_limit.saturating_mul(100).max(500).min(1000)
    } else {
        base_limit.saturating_mul(20)
    };
    let filter = SearchFilter {
        language: language.clone(),
        kind: None,
        file_pattern: file_pattern.clone(),
        exclude_tests: false,
    };

    let (all_line_matches, relaxed, mut stage_counts): (Vec<LineMatch>, bool, LineModeStageCounts) =
        match workspace_target {
            WorkspaceTarget::Primary => {
                let primary_snapshot = handler.primary_workspace_snapshot().await?;
                let db = primary_snapshot.database;
                let search_index = primary_snapshot.search_index.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Line-level content search requires a Tantivy index for the current primary workspace. Run manage_workspace(operation=\"refresh\") first."
                    )
                })?;

                let query = query.to_string();
                let match_strategy = match_strategy.clone();
                let file_pattern_clone = file_pattern.clone();
                let language_clone = language.clone();

                tokio::task::spawn_blocking(
                    move || -> Result<(Vec<LineMatch>, bool, LineModeStageCounts)> {
                        let index = match search_index.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                warn!("Search index mutex poisoned, recovering: {}", poisoned);
                                poisoned.into_inner()
                            }
                        };
                        let content_results = index.search_content(&query, &filter, fetch_limit)?;
                        let relaxed = content_results.relaxed;
                        let file_results = content_results.results;
                        let mut counts = LineModeStageCounts {
                            and_candidates: content_results.and_candidate_count,
                            or_candidates: content_results.or_candidate_count,
                            tantivy_file_candidates: file_results.len(),
                            ..Default::default()
                        };
                        drop(index);

                        let db_lock = match db.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                warn!("Database mutex poisoned, recovering: {}", poisoned);
                                poisoned.into_inner()
                            }
                        };

                        let mut matches = Vec::new();
                        for file_result in file_results {
                            if matches.len() >= base_limit {
                                break;
                            }

                            if let Some(ref pattern) = file_pattern_clone
                                && !matches_glob_pattern(&file_result.file_path, pattern)
                            {
                                counts.file_pattern_dropped += 1;
                                continue;
                            }

                            if let Some(ref lang) = language_clone
                                && !file_matches_language(&file_result.file_path, lang)
                            {
                                counts.language_dropped += 1;
                                continue;
                            }

                            if exclude_test_files
                                && crate::search::scoring::is_test_path(&file_result.file_path)
                            {
                                counts.test_dropped += 1;
                                continue;
                            }

                            match db_lock.get_file_content(&file_result.file_path)? {
                                Some(content) => {
                                    let before = matches.len();
                                    collect_line_matches(
                                        &mut matches,
                                        &content,
                                        &file_result.file_path,
                                        &match_strategy,
                                        base_limit,
                                    );
                                    if matches.len() == before {
                                        counts.line_match_miss_dropped += 1;
                                    }
                                }
                                None => {
                                    counts.file_content_unavailable_dropped += 1;
                                }
                            }
                        }

                        Ok((matches, relaxed, counts))
                    },
                )
                .await??
            }
            WorkspaceTarget::Target(workspace_id) => {
                let db_arc = handler.get_database_for_workspace(workspace_id).await?;
                let si_arc = handler.get_search_index_for_workspace(workspace_id).await?;
                let target_workspace_id = workspace_id.clone();

                let query_clone = query.to_string();
                let strategy = match_strategy.clone();
                let ref_file_pattern = file_pattern.clone();
                let ref_language = language.clone();

                tokio::task::spawn_blocking(
                    move || -> Result<(Vec<LineMatch>, bool, LineModeStageCounts)> {
                        let si_arc = match si_arc {
                            Some(si) => si,
                            None => {
                                return Err(anyhow::anyhow!(
                                    "Line-level content search requires a Tantivy index for workspace '{}'. Run manage_workspace(operation=\"refresh\", workspace_id=\"{}\") first.",
                                    target_workspace_id,
                                    target_workspace_id
                                ));
                            }
                        };
                        let ref_index = si_arc
                            .lock()
                            .map_err(|e| anyhow::anyhow!("Search index lock error: {}", e))?;
                        let ref_filter = SearchFilter {
                            language: ref_language.clone(),
                            kind: None,
                            file_pattern: ref_file_pattern.clone(),
                            exclude_tests: false,
                        };
                        let content_results =
                            ref_index.search_content(&query_clone, &ref_filter, fetch_limit)?;
                        let relaxed = content_results.relaxed;
                        let file_results = content_results.results;
                        let mut counts = LineModeStageCounts {
                            and_candidates: content_results.and_candidate_count,
                            or_candidates: content_results.or_candidate_count,
                            tantivy_file_candidates: file_results.len(),
                            ..Default::default()
                        };
                        drop(ref_index);

                        if file_results.is_empty() {
                            return Ok((Vec::new(), relaxed, counts));
                        }

                        let ref_db = db_arc
                            .lock()
                            .map_err(|e| anyhow::anyhow!("Database lock error: {}", e))?;
                        let mut matches = Vec::new();
                        for file_result in file_results {
                            if matches.len() >= base_limit {
                                break;
                            }

                            if let Some(ref pattern) = ref_file_pattern
                                && !matches_glob_pattern(&file_result.file_path, pattern)
                            {
                                counts.file_pattern_dropped += 1;
                                continue;
                            }

                            if let Some(ref lang) = ref_language
                                && !file_matches_language(&file_result.file_path, lang)
                            {
                                counts.language_dropped += 1;
                                continue;
                            }

                            if exclude_test_files
                                && crate::search::scoring::is_test_path(&file_result.file_path)
                            {
                                counts.test_dropped += 1;
                                continue;
                            }

                            match ref_db.get_file_content(&file_result.file_path)? {
                                Some(content) => {
                                    let before = matches.len();
                                    collect_line_matches(
                                        &mut matches,
                                        &content,
                                        &file_result.file_path,
                                        &strategy,
                                        base_limit,
                                    );
                                    if matches.len() == before {
                                        counts.line_match_miss_dropped += 1;
                                    }
                                }
                                None => {
                                    counts.file_content_unavailable_dropped += 1;
                                }
                            }
                        }

                        Ok((matches, relaxed, counts))
                    },
                )
                .await
                .map_err(|e| anyhow::anyhow!("Failed to spawn target workspace search: {}", e))??
            }
        };

    let total_before_second_pass = all_line_matches.len();
    let filtered_matches: Vec<LineMatch> = all_line_matches
        .into_iter()
        .filter(|line_match| {
            let language_match = language
                .as_ref()
                .map(|lang| file_matches_language(&line_match.file_path, lang))
                .unwrap_or(true);
            let file_match = file_pattern
                .as_ref()
                .map(|pattern| matches_glob_pattern(&line_match.file_path, pattern))
                .unwrap_or(true);
            let test_match = if exclude_test_files {
                !crate::search::scoring::is_test_path(&line_match.file_path)
            } else {
                true
            };
            language_match && file_match && test_match
        })
        .collect();

    // The second-pass filter above can drop additional matches that slipped
    // past the per-file loop (e.g., via the target-workspace branch, or when
    // `collect_line_matches` produced partial matches before a filter later
    // rejected the file). Task 5 investigates whether this pass is ever
    // load-bearing; for Task 3's instrumentation we fold any extra drops
    // into the existing counters by delta.
    let second_pass_dropped = total_before_second_pass.saturating_sub(filtered_matches.len());
    if second_pass_dropped > 0 {
        // Attribute the drop to a non-specific bucket: reuse line_match_miss
        // (the catch-all for matches that existed but were removed by
        // downstream filters). A dedicated variant can be added in Task 4.
        stage_counts.line_match_miss_dropped =
            stage_counts.line_match_miss_dropped.saturating_add(second_pass_dropped);
    }

    Ok(LineModeSearchResult {
        matches: filtered_matches,
        relaxed,
        strategy: match_strategy,
        workspace_label,
        stage_counts,
    })
}

pub(crate) fn format_line_mode_output(query: &str, result: &LineModeSearchResult) -> String {
    let header = match &result.strategy {
        LineMatchStrategy::FileLevel { .. } => {
            let file_count = result
                .matches
                .iter()
                .map(|m| &m.file_path)
                .collect::<std::collections::HashSet<_>>()
                .len();
            format!(
                "📄 File-level search in [{}]: '{}' (found {} lines across {} files)",
                result.workspace_label,
                query,
                result.matches.len(),
                file_count
            )
        }
        _ => format!(
            "📄 Line-level search in [{}]: '{}' (found {} lines)",
            result.workspace_label,
            query,
            result.matches.len()
        ),
    };
    let mut lines = vec![header];
    lines.push(String::new());

    for entry in &result.matches {
        lines.push(format!(
            "{}:{}:{}",
            entry.file_path, entry.line_number, entry.line_content
        ));
    }

    lines.join("\n")
}

/// Check if a file path matches the given language filter by extension.
fn file_matches_language(file_path: &str, lang: &str) -> bool {
    let path = std::path::Path::new(file_path);
    let Some(ext) = path.extension() else {
        return false;
    };
    let ext_str = ext.to_string_lossy().to_lowercase();
    match lang.to_lowercase().as_str() {
        "rust" => ext_str == "rs",
        "typescript" => ext_str == "ts" || ext_str == "tsx",
        "javascript" => ext_str == "js" || ext_str == "jsx" || ext_str == "mjs",
        "python" => ext_str == "py",
        "java" => ext_str == "java",
        "csharp" | "c#" => ext_str == "cs",
        "cpp" | "c++" => ext_str == "cpp" || ext_str == "cc" || ext_str == "cxx",
        "c" => ext_str == "c" || ext_str == "h",
        _ => ext_str == lang.to_lowercase(),
    }
}

/// Collect line matches from file content using the given strategy
fn collect_line_matches(
    destination: &mut Vec<LineMatch>,
    content: &str,
    file_path: &str,
    strategy: &LineMatchStrategy,
    max_results: usize,
) {
    if destination.len() >= max_results {
        return;
    }
    for (line_idx, line) in content.lines().enumerate() {
        if line_matches(strategy, line) {
            destination.push(LineMatch {
                file_path: file_path.to_string(),
                line_number: line_idx + 1,
                line_content: line.trim_end_matches('\r').to_string(),
            });

            if destination.len() >= max_results {
                break;
            }
        }
    }
}
