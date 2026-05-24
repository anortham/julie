use crate::tools::search::line_mode::{
    LineModeSearchResult, LineModeStageCounts, format_line_mode_output,
};
use crate::tools::search::{LineMatch, LineMatchStrategy};

#[test]
fn test_fast_search_content_full_groups_same_file_hits() {
    let result = LineModeSearchResult {
        matches: vec![
            LineMatch {
                file_path: "src/tests/core/handler_telemetry.rs".to_string(),
                line_number: 48,
                line_content: "fn sample_file_hit() -> SearchHit {".to_string(),
            },
            LineMatch {
                file_path: "src/tests/core/handler_telemetry.rs".to_string(),
                line_number: 62,
                line_content: "tool: \"edit_file\".to_string(),".to_string(),
            },
            LineMatch {
                file_path: "src/tests/core/handler_telemetry.rs".to_string(),
                line_number: 79,
                line_content: "workspace_is_primary: true,".to_string(),
            },
        ],
        strategy: LineMatchStrategy::FileLevel {
            terms: vec!["workspace_is_primary".to_string(), "edit_file".to_string()],
        },
        workspace_label: "julie_528d4264".to_string(),
        stage_counts: LineModeStageCounts::default(),
        zero_hit_reason: None,
        file_pattern_diagnostic: None,
        scope_relaxed: false,
        original_file_pattern: None,
    };

    let output = format_line_mode_output("workspace_is_primary edit_file", &result);

    assert_eq!(
        output
            .matches("src/tests/core/handler_telemetry.rs")
            .count(),
        1,
        "same-file full output should print the path once: {output}",
    );
    assert!(output.contains("src/tests/core/handler_telemetry.rs (3 lines)"));
    assert!(output.contains("  48: fn sample_file_hit() -> SearchHit {"));
    assert!(output.contains("  62: tool: \"edit_file\".to_string(),"));
    assert!(output.contains("  79: workspace_is_primary: true,"));
    assert!(output.contains("found 3 lines across 1 files"));
}
