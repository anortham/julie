use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};

use super::FastSearchExecution;
use super::hint_formatter::build_file_pattern_syntax_hint;
use super::query::looks_like_whitespace_separated_globs;
use super::trace::{FilePatternDiagnostic, HintKind, SearchExecutionKind, SearchExecutionResult};

/// Returns an early-exit diagnostic when `file_pattern` looks like
/// whitespace-separated globs (e.g. `"src/** docs/**"`). After the T8 unified
/// cutover there is no `search_target` parameter — the diagnostic always uses
/// `SearchExecutionKind::Definitions` as the execution kind placeholder.
pub fn build_request_level_file_pattern_diagnostic(
    query: &str,
    file_pattern: Option<&str>,
) -> Option<FastSearchExecution> {
    let file_pattern = file_pattern?;
    if !looks_like_whitespace_separated_globs(file_pattern) {
        return None;
    }

    let execution = SearchExecutionResult::input_diagnostic(
        "fast_search_input_diagnostic",
        SearchExecutionKind::Definitions,
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
