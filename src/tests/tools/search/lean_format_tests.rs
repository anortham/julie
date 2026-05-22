//! Tests for live formatters used by the unified search path.

#[cfg(test)]
mod tests {
    use crate::tools::search::LineMatch;
    use crate::tools::search::formatting::format_content_locations_only;
    use crate::tools::search::trace::SearchHit;
    use crate::tools::shared::OptimizedResponse;

    fn make_line_hit(file_path: &str, line_number: usize, line_content: &str) -> SearchHit {
        SearchHit::from_line_match(
            LineMatch {
                file_path: file_path.to_string(),
                line_number,
                line_content: line_content.to_string(),
            },
            "primary".to_string(),
            "rust".to_string(),
            1.0,
        )
    }

    #[test]
    fn test_fast_search_content_locations_groups_same_file_hits() {
        let response = OptimizedResponse {
            results: vec![
                make_line_hit(
                    "src/tests/core/handler_telemetry.rs",
                    48,
                    "fn sample_file_hit() -> SearchHit {",
                ),
                make_line_hit(
                    "src/tests/core/handler_telemetry.rs",
                    62,
                    "tool: \"edit_file\".to_string(),",
                ),
                make_line_hit("src/tools/search/query.rs", 301, "pub fn line_matches(...)"),
            ],
            total_found: 3,
        };

        let output = format_content_locations_only("workspace_is_primary edit_file", &response);

        assert_eq!(
            output
                .matches("src/tests/core/handler_telemetry.rs")
                .count(),
            1,
            "same-file locations output should print the path once: {output}",
        );
        assert!(output.contains("src/tests/core/handler_telemetry.rs: 48, 62"));
        assert!(output.contains("src/tools/search/query.rs:301"));
        assert!(!output.contains("fn sample_file_hit"));
        assert!(!output.contains("tool: \"edit_file\""));
    }
}
