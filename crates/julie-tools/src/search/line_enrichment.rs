use anyhow::Result;

use julie_context::ToolContext;
use julie_core::shared::OptimizedResponse;

use super::formatting;
use super::hint_formatter;
use super::line_mode;
use super::params::FastSearchTool;
use super::query;
use super::trace::{self, SearchExecutionResult, SearchHit, ZeroHitReason};
use super::types::LineMatchStrategy;
use crate::navigation::resolution::WorkspaceTarget;

pub(crate) async fn try_line_mode_locations(
    tool: &FastSearchTool,
    handler: &dyn ToolContext,
    workspace_target: &WorkspaceTarget,
    execution: &mut SearchExecutionResult,
) -> Result<Option<String>> {
    let effective_limit = tool.effective_limit();
    let line_result = line_mode::line_mode_matches(
        &tool.query,
        &tool.language,
        &tool.file_pattern,
        effective_limit,
        tool.exclude_tests,
        workspace_target,
        handler,
    )
    .await?;

    let line_match_strategy = line_match_strategy_label(&line_result.strategy).to_string();
    if line_result.matches.is_empty() {
        execution.trace.record_line_enrichment_no_matches(
            line_match_strategy,
            line_result.zero_hit_reason,
            line_result.file_pattern_diagnostic,
        );
        return Ok(None);
    }

    let workspace_label = match workspace_target {
        WorkspaceTarget::Primary => handler
            .require_primary_workspace_identity()
            .unwrap_or_else(|_| "primary".to_string()),
        WorkspaceTarget::Target(id) => id.clone(),
    };

    let scope_rescue_header = line_result
        .scope_relaxed
        .then(|| {
            line_result.original_file_pattern.as_deref().map(|pattern| {
                let distinct_files = line_result
                    .matches
                    .iter()
                    .map(|line_match| line_match.file_path.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                hint_formatter::build_scope_rescue_header(pattern, distinct_files)
            })
        })
        .flatten();
    let language_by_file = execution
        .hits
        .iter()
        .map(|hit| (hit.file.clone(), hit.language.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let requested_language = tool.language.clone();

    let hits: Vec<SearchHit> = line_result
        .matches
        .into_iter()
        .map(|line_match| {
            let language = language_by_file
                .get(&line_match.file_path)
                .cloned()
                .or_else(|| requested_language.clone())
                .or_else(|| {
                    julie_core::language::detect_language(std::path::Path::new(
                        &line_match.file_path,
                    ))
                    .map(str::to_string)
                })
                .unwrap_or_else(|| "text".to_string());
            SearchHit::from_line_match(line_match, workspace_label.clone(), language, 0.0_f32)
        })
        .collect();

    let total_results = hits.len();
    let optimized = OptimizedResponse::with_total(hits.clone(), total_results);
    let output = formatting::format_content_locations_only(&tool.query, &optimized);

    execution.hits = hits;
    execution.total_results = total_results;
    execution.trace.refresh_hits(&execution.hits);
    execution
        .trace
        .record_line_enrichment_applied(line_match_strategy, total_results);
    if line_result.scope_relaxed {
        execution.trace.scope_relaxed = true;
        execution.trace.original_file_pattern = line_result.original_file_pattern.clone();
        execution.trace.original_zero_hit_reason = Some(ZeroHitReason::FilePatternFiltered);
        execution.trace.scope_rescue_count = execution.trace.scope_rescue_count.saturating_add(1);
    }

    Ok(Some(match scope_rescue_header {
        Some(header) => format!("{header}\n\n{output}"),
        None => output,
    }))
}

pub(crate) fn should_try_line_mode_locations(
    tool: &FastSearchTool,
    execution: &SearchExecutionResult,
    has_exact_name_match: bool,
    symbol_backend_active: bool,
) -> bool {
    if has_exact_name_match || symbol_backend_active || execution.trace.scope_relaxed {
        return false;
    }
    if query::looks_like_file_or_path_query(&tool.query)
        || looks_like_structured_lookup(&tool.query)
    {
        return false;
    }
    true
}

pub(crate) async fn try_enrich_with_line_mode_snippets(
    tool: &FastSearchTool,
    handler: &dyn ToolContext,
    workspace_target: &WorkspaceTarget,
    execution: &mut SearchExecutionResult,
) -> Result<()> {
    let line_result = line_mode::line_mode_matches(
        &tool.query,
        &tool.language,
        &tool.file_pattern,
        tool.effective_limit(),
        tool.exclude_tests,
        workspace_target,
        handler,
    )
    .await?;

    let line_match_strategy = line_match_strategy_label(&line_result.strategy).to_string();
    if line_result.matches.is_empty() {
        execution.trace.record_line_enrichment_no_matches(
            line_match_strategy,
            line_result.zero_hit_reason,
            line_result.file_pattern_diagnostic,
        );
        return Ok(());
    }

    let match_count = line_result.matches.len();
    let mut snippets_by_file: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for line_match in line_result.matches {
        snippets_by_file
            .entry(line_match.file_path)
            .or_default()
            .push(format!(
                "{}: {}",
                line_match.line_number,
                line_match.line_content.trim()
            ));
    }

    for hit in &mut execution.hits {
        let Some(lines) = snippets_by_file.get(&hit.file) else {
            continue;
        };
        let line_snippet = lines.iter().take(3).cloned().collect::<Vec<_>>().join("\n");
        if let trace::SearchHitBacking::Symbol(symbol) = &mut hit.backing {
            let snippet = match hit
                .snippet
                .as_deref()
                .filter(|existing| !existing.trim().is_empty())
            {
                Some(existing) if existing.contains(&line_snippet) => existing.to_string(),
                Some(existing) => format!("{existing}\n{line_snippet}"),
                None => line_snippet.clone(),
            };
            hit.snippet = Some(snippet.clone());
            symbol.code_context = Some(snippet);
        } else {
            hit.snippet = Some(line_snippet);
        }
    }

    execution
        .trace
        .record_line_enrichment_applied(line_match_strategy, match_count);
    Ok(())
}

pub(crate) fn line_match_strategy_label(strategy: &LineMatchStrategy) -> &'static str {
    match strategy {
        LineMatchStrategy::Substring(_) => "substring",
        LineMatchStrategy::Tokens { .. } => "tokens",
        LineMatchStrategy::FileLevel { .. } => "file_level",
    }
}

pub(crate) fn with_scope_rescue_header(text: String, execution: &SearchExecutionResult) -> String {
    if execution.trace.scope_relaxed
        && let Some(original_pattern) = execution.trace.original_file_pattern.as_deref()
    {
        let distinct_files = execution
            .hits
            .iter()
            .map(|hit| hit.file.as_str())
            .collect::<std::collections::HashSet<_>>();
        format!(
            "{}\n\n{}",
            hint_formatter::build_scope_rescue_header(original_pattern, distinct_files.len()),
            text,
        )
    } else {
        text
    }
}

fn looks_like_structured_lookup(query: &str) -> bool {
    let mut token_count = 0;
    let mut saw_strict_structured_shape = false;
    let mut saw_loose_structured_shape = false;
    for token in query.split_whitespace() {
        let token = token.trim_matches(|ch: char| matches!(ch, ',' | ';' | ':' | '(' | ')'));
        if token.is_empty() {
            continue;
        }
        token_count += 1;
        if !token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':'))
        {
            return false;
        }
        let strict_shape = token.contains("::")
            || token.contains('.')
            || (token.chars().any(|ch| ch.is_ascii_uppercase())
                && token.chars().any(|ch| ch.is_ascii_lowercase()));
        saw_strict_structured_shape |= strict_shape;
        saw_loose_structured_shape |= strict_shape || token.contains('_');
    }

    match token_count {
        0 => false,
        1 => saw_strict_structured_shape,
        _ => saw_loose_structured_shape,
    }
}
