//! Integration tests for get_context pipeline behavior.

#[cfg(test)]
mod pipeline_integration_tests {
    use std::sync::Arc;

    use julie_index::snapshot::Snapshot;
    use julie_test_support::SnapshotFixture;
    use tempfile::TempDir;

    use crate::get_context::pipeline::run_pipeline;
    use crate::tests::get_context_tests::snapshot_fixture;

    const HANDLER: &str = "/// Process an incoming request\nfn process_request(req: &Request) -> Response {\n    let valid = validate_input(req);\n    if !valid { return Response::bad_request(); }\n    build_response(req)\n}\n";
    const VALIDATION: &str = "/// Validate request parameters\nfn validate_input(req: &Request) -> bool {\n    !req.body.is_empty()\n}\n";
    const RESPONSE: &str =
        "fn build_response(req: &Request) -> Response {\n    Response::ok(req.body)\n}\n";
    const ERROR: &str =
        "fn handle_error(err: Error) -> Response {\n    process_request(&err.request)\n}\n";
    const MAIN: &str = "fn main() {\n    process_request(&Request::default());\n}\n";

    fn setup_test_env(extra: &[(&str, &str)]) -> (TempDir, SnapshotFixture, Arc<Snapshot>) {
        let mut files = vec![
            ("src/handler.rs", HANDLER),
            ("src/validation.rs", VALIDATION),
            ("src/response.rs", RESPONSE),
            ("src/error.rs", ERROR),
            ("src/main.rs", MAIN),
        ];
        files.extend_from_slice(extra);
        let (dir, fixture) = snapshot_fixture(&files);
        let snapshot = fixture.snapshot();
        (dir, fixture, snapshot)
    }

    #[test]
    fn test_full_pipeline_end_to_end() {
        let (_dir, _fixture, snapshot) = setup_test_env(&[]);

        let result = run_pipeline("process_request", None, None, None, None, &snapshot).unwrap();
        assert!(result.contains("process_request"));
        assert!(result.contains("PIVOT"));
        assert!(result.contains("validate_input"));
        assert!(result.contains("src/handler.rs"));
        assert!(result.contains("Context"));
    }

    #[test]
    fn test_pipeline_no_results() {
        let (_dir, _fixture, snapshot) = setup_test_env(&[]);
        let result = run_pipeline(
            "zzz_nonexistent_symbol_xyz",
            None,
            None,
            None,
            None,
            &snapshot,
        )
        .unwrap();
        assert!(
            result.contains("no relevant symbols"),
            "Expected no-results message, got:\n{}",
            result
        );
    }

    #[test]
    fn test_pipeline_with_explicit_budget() {
        let (_dir, _fixture, snapshot) = setup_test_env(&[]);
        let result =
            run_pipeline("process_request", Some(1000), None, None, None, &snapshot).unwrap();
        assert!(result.contains("process_request"));
    }

    #[test]
    fn test_pipeline_with_compact_format() {
        let (_dir, _fixture, snapshot) = setup_test_env(&[]);
        let result = run_pipeline(
            "process_request",
            None,
            None,
            None,
            Some("compact".to_string()),
            &snapshot,
        )
        .unwrap();

        assert!(result.contains("PIVOT process_request"));
        assert!(result.contains("Context \"process_request\" | pivots="));
    }

    #[test]
    fn test_pipeline_with_language_filter() {
        let (_dir, _fixture, snapshot) = setup_test_env(&[]);
        let result = run_pipeline(
            "process_request",
            None,
            Some("python".to_string()),
            None,
            None,
            &snapshot,
        )
        .unwrap();

        assert!(
            result.contains("no relevant symbols"),
            "Expected no-results message, got:\n{}",
            result
        );
    }

    #[test]
    fn test_pipeline_includes_neighbors() {
        let (_dir, _fixture, snapshot) = setup_test_env(&[]);
        let result = run_pipeline("process_request", None, None, None, None, &snapshot).unwrap();

        let has_neighbor_section = result.contains("Neighbors");
        let has_any_neighbor = result.contains("validate_input")
            || result.contains("build_response")
            || result.contains("handle_error")
            || result.contains("main");
        assert!(has_neighbor_section || has_any_neighbor);
    }

    #[test]
    fn test_pipeline_filters_noise_neighbors() {
        let noisy_handler = "/// Process an incoming request\nfn process_request(req: &Request) -> Response {\n    let valid = validate_input(req);\n    let w = Wrapper;\n    w.clone();\n    w.to_string();\n    w.fmt();\n    build_response(req)\n}\n\nstruct Wrapper;\n\nimpl Wrapper {\n    fn clone(&self) {}\n    fn to_string(&self) {}\n    fn fmt(&self) {}\n}\n";
        let (_dir, _fixture, snapshot) = setup_test_env(&[("src/handler.rs", noisy_handler)]);

        let result = run_pipeline("process_request", None, None, None, None, &snapshot).unwrap();

        assert!(!result.contains("clone_impl") && !result.contains("  clone "));
        assert!(!result.contains("to_string_impl") && !result.contains("  to_string "));
        assert!(!result.contains("fmt_impl") && !result.contains("  fmt "));
        assert!(result.contains("validate_input") || result.contains("build_response"));
    }

    #[test]
    fn test_pipeline_filters_test_file_neighbors() {
        let (_dir, _fixture, snapshot) = setup_test_env(&[(
            "src/tests/handler_tests.rs",
            "fn test_process_request_works() {\n    process_request(&req);\n}\n",
        )]);

        let result = run_pipeline("process_request", None, None, None, None, &snapshot).unwrap();

        assert!(!result.contains("test_process_request_works"));
        assert!(
            result.contains("validate_input")
                || result.contains("build_response")
                || result.contains("handle_error")
        );
    }

    #[test]
    fn test_pipeline_respects_token_budget() {
        let (_dir, _fixture, snapshot) = setup_test_env(&[]);
        let result = run_pipeline("process", Some(100), None, None, None, &snapshot).unwrap();

        assert!(!result.is_empty());
        let token_est = julie_core::token_estimation::TokenEstimator::new();
        let estimated = token_est.estimate_string(&result);
        assert!(estimated < 400);
    }
}
