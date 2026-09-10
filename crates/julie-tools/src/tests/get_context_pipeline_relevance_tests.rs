//! Integration tests for get_context relevance behavior through run_pipeline.

#[cfg(test)]
mod tests {
    use crate::get_context::pipeline::run_pipeline;
    use crate::tests::get_context_tests::snapshot_fixture;

    const DOCS_BLOB: &str =
        "docsdominanttoken docsdominanttoken docsdominanttoken docsdominanttoken";

    #[test]
    fn test_run_pipeline_prefers_code_pivots_under_low_actionable_coverage() {
        let overview = format!("# workspace_routing_overview_docsdominanttoken\n\n{DOCS_BLOB}\n");
        let checkpoint =
            format!("# workspace_routing_checkpoint_docsdominanttoken\n\n{DOCS_BLOB}\n");
        let (_dir, fixture) = snapshot_fixture(&[
            ("docs/workspace-routing.md", overview.as_str()),
            ("docs/memories/checkpoint.md", checkpoint.as_str()),
            (
                "src/workspace/resolver.rs",
                "/// Resolve workspace routing\nfn resolve_workspace_routing(docsdominanttoken: &str) -> bool {\n    docsdominanttoken.len() > 0\n}\n",
            ),
            (
                "src/workspace/validator.rs",
                "/// Validate workspace routing\nfn validate_workspace_routing(docsdominanttoken: &str) -> bool {\n    docsdominanttoken.len() > 0\n}\n",
            ),
        ]);
        let snapshot = fixture.snapshot();

        let output = run_pipeline(
            "docsdominanttoken workspace routing",
            None,
            None,
            None,
            None,
            &snapshot,
        )
        .unwrap();

        assert!(
            output.contains("PIVOT resolve_workspace_routing"),
            "expected resolver code symbol as pivot, got:\n{}",
            output
        );
        assert!(
            output.contains("PIVOT validate_workspace_routing"),
            "expected validator code symbol as pivot, got:\n{}",
            output
        );

        assert!(
            !output.contains("PIVOT workspace_routing_overview_docsdominanttoken"),
            "docs pivot should be dropped after code-first fallback, got:\n{}",
            output
        );
        assert!(
            !output.contains("PIVOT workspace_routing_checkpoint_docsdominanttoken"),
            "memory pivot should be dropped after code-first fallback, got:\n{}",
            output
        );
    }
}
