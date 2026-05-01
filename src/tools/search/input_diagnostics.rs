use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};

use super::FastSearchExecution;
use super::hint_formatter::build_file_pattern_syntax_hint;
use super::line_mode;
use super::query::looks_like_whitespace_separated_globs;
use super::target::SearchTarget;
use super::trace::{FilePatternDiagnostic, HintKind, SearchExecutionKind, SearchExecutionResult};

pub(crate) fn build_request_level_file_pattern_diagnostic(
    query: &str,
    file_pattern: Option<&str>,
    search_target: SearchTarget,
) -> Option<FastSearchExecution> {
    let file_pattern = file_pattern?;
    if !looks_like_whitespace_separated_globs(file_pattern) {
        return None;
    }

    let execution = SearchExecutionResult::input_diagnostic(
        "fast_search_input_diagnostic",
        search_target_kind(search_target, query),
        FilePatternDiagnostic::WhitespaceSeparatedMultiGlob,
        HintKind::FilePatternSyntaxHint,
    );

    Some(FastSearchExecution {
        result: CallToolResult::text_content(vec![Content::text(build_file_pattern_syntax_hint(
            query,
            file_pattern,
        ))]),
        execution: Some(execution),
    })
}

fn search_target_kind(search_target: SearchTarget, query: &str) -> SearchExecutionKind {
    match search_target {
        SearchTarget::Content => SearchExecutionKind::Content {
            workspace_label: None,
            file_level: line_mode::query_uses_file_level_header(query),
        },
        SearchTarget::Definitions => SearchExecutionKind::Definitions,
        SearchTarget::Files => SearchExecutionKind::Files,
    }
}
