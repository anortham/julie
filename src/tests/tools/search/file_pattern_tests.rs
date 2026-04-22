//! Tests for multi-pattern `file_pattern` parser and boundary normalization.
//!
//! Covers `matches_glob_pattern`:
//! - Single-pattern (legacy) behaviors preserved
//! - Comma-separated OR semantics
//! - Brace alternation
//! - Exclusions via `!` prefix, including mixed include/exclude
//! - Whitespace inside a glob (literal space path) stays a single pattern
//! - Whitespace between globs is NOT a split separator
//!
//! Also covers Task 2: boundary normalization of empty / whitespace-only
//! `file_pattern` to `None` at the `execute_search` entry point. The
//! integration test exercises FastSearchTool across the four whitespace
//! forms the spec names ("", "   ", "\t", "\n") and asserts each one
//! yields the same result set as omitting `file_pattern` entirely. This
//! covers the dashboard route and any other caller transitively, since
//! they all dispatch through `execute_search`.

use crate::tools::search::matches_glob_pattern;

// ---------------------------------------------------------------------------
// Comma-separated OR semantics
// ---------------------------------------------------------------------------

#[test]
fn comma_splits_into_or_of_inclusions() {
    // Real-world case from the plan: overlapping patterns joined with commas.
    let path = "src/database/workspace.rs";
    let pattern = "src/database/*.rs,src/database/**/*.rs";
    assert!(
        matches_glob_pattern(path, pattern),
        "comma-separated inclusions should OR — {} should match '{}'",
        path,
        pattern,
    );
}

#[test]
fn comma_or_matches_second_alternative() {
    let pattern = "src/**,tests/**";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(matches_glob_pattern("tests/foo/bar.rs", pattern));
    assert!(
        !matches_glob_pattern("docs/README.md", pattern),
        "docs/ is not in either inclusion; should not match",
    );
}

#[test]
fn pipe_splits_into_or_of_inclusions() {
    let pattern = "src/**|tests/**";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(matches_glob_pattern("tests/foo/bar.rs", pattern));
    assert!(
        !matches_glob_pattern("docs/README.md", pattern),
        "docs/ is not in either inclusion; should not match",
    );
}

// ---------------------------------------------------------------------------
// Brace alternation (globset native feature; must survive comma splitting)
// ---------------------------------------------------------------------------

#[test]
fn brace_alternation_is_preserved() {
    // Top-level comma split must skip commas inside `{...}`.
    let pattern = "{src/**,tests/**}";
    assert!(
        matches_glob_pattern("src/lib.rs", pattern),
        "brace alternation should match src/ tree",
    );
    assert!(
        matches_glob_pattern("tests/foo.rs", pattern),
        "brace alternation should match tests/ tree",
    );
    assert!(!matches_glob_pattern("docs/README.md", pattern));
}

#[test]
fn brace_alternation_comma_is_not_a_split() {
    // If brace-awareness is broken, this would split into `{src/**` and
    // `tests/**}` — both are invalid globs and would match nothing. A
    // correctly preserved brace expression matches `src/` OR `tests/`.
    let pattern = "{src/database/*.rs,tests/**/*.rs}";
    assert!(matches_glob_pattern("src/database/workspace.rs", pattern));
    assert!(matches_glob_pattern("tests/integration/foo.rs", pattern));
}

#[test]
fn top_level_pipe_respects_brace_nesting() {
    let pattern = "src/**|{tests/**,docs/**}";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(matches_glob_pattern("tests/foo.rs", pattern));
    assert!(matches_glob_pattern("docs/README.md", pattern));
    assert!(!matches_glob_pattern("xtask/src/main.rs", pattern));
}

// ---------------------------------------------------------------------------
// Exclusions
// ---------------------------------------------------------------------------

#[test]
fn mixed_include_and_exclude() {
    let pattern = "!docs/**,src/**";
    assert!(
        matches_glob_pattern("src/lib.rs", pattern),
        "src/lib.rs is included by src/** and not excluded",
    );
    assert!(
        !matches_glob_pattern("docs/README.md", pattern),
        "docs/README.md is excluded by !docs/**",
    );
}

#[test]
fn exclusion_only_implies_include_all() {
    // Exclusion-only: everything matches EXCEPT the excluded set.
    let pattern = "!docs/**";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(matches_glob_pattern("tests/integration/foo.rs", pattern));
    assert!(!matches_glob_pattern("docs/README.md", pattern));
    assert!(!matches_glob_pattern("docs/nested/page.md", pattern));
}

#[test]
fn multiple_exclusions_combine() {
    let pattern = "!docs/**,!target/**";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(!matches_glob_pattern("docs/README.md", pattern));
    assert!(!matches_glob_pattern("target/debug/foo", pattern));
}

#[test]
fn inclusion_and_exclusion_order_independent() {
    // Same pattern, swapped order: result must be identical.
    let a = "!docs/**,src/**";
    let b = "src/**,!docs/**";
    for path in ["src/lib.rs", "docs/README.md", "tests/foo.rs"] {
        assert_eq!(
            matches_glob_pattern(path, a),
            matches_glob_pattern(path, b),
            "order of segments must not affect outcome for {}",
            path,
        );
    }
}

// ---------------------------------------------------------------------------
// Whitespace handling — pinned regressions for literal-space globs
// ---------------------------------------------------------------------------

#[test]
fn literal_space_in_glob_is_preserved() {
    // This mirrors the pinned regression test at
    // src/tests/integration/search_regression_tests.rs:253-260.
    // A space inside a glob component must NOT be treated as a split separator.
    let path = "\\\\?\\C:\\source\\My Project\\src\\file name.rs";
    let pattern = "**/file name.rs";
    assert!(
        matches_glob_pattern(path, pattern),
        "literal space in glob should match path with literal space",
    );
}

#[test]
fn whitespace_between_globs_is_not_a_split() {
    // Whitespace is NOT a separator — "a/** b/**" is a single literal pattern
    // (globset likely rejects or never matches it). Must not split into
    // two patterns.
    let pattern = "a/** b/**";
    assert!(!matches_glob_pattern("a/foo.rs", pattern));
    assert!(!matches_glob_pattern("b/foo.rs", pattern));
    assert!(!matches_glob_pattern("src/a/b/foo.rs", pattern));
}

// ---------------------------------------------------------------------------
// Single-pattern legacy behavior preserved
// ---------------------------------------------------------------------------

#[test]
fn single_simple_filename_still_basename_matches() {
    // Existing behavior from matches_glob_pattern: simple filename (no
    // wildcards, no separators) matches against basename, tolerating UNC
    // paths.
    let path = "\\\\?\\C:\\source\\proj\\Program.cs";
    assert!(matches_glob_pattern(path, "Program.cs"));
}

#[test]
fn single_exclusion_still_works() {
    let path = "docs/README.md";
    assert!(!matches_glob_pattern(path, "!docs/**"));
    assert!(matches_glob_pattern("src/lib.rs", "!docs/**"));
}

#[test]
fn trailing_empty_segment_after_comma_is_ignored() {
    // "src/**," trailing empty after comma must not mean "include all"
    // (which would flip to implicit include-all since there'd be no
    // effective inclusion). The non-empty segment stays the only inclusion.
    let pattern = "src/**,";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(!matches_glob_pattern("docs/README.md", pattern));
}

#[test]
fn whitespace_around_comma_segments_is_trimmed() {
    let pattern = " src/** , tests/** ";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(matches_glob_pattern("tests/foo.rs", pattern));
    assert!(!matches_glob_pattern("docs/README.md", pattern));
}

// ---------------------------------------------------------------------------
// Task 2: boundary normalization of empty/whitespace file_pattern
// ---------------------------------------------------------------------------

mod boundary_normalization {
    use std::fs;
    use tempfile::TempDir;

    use crate::handler::JulieServerHandler;
    use crate::tools::search::trace::{FilePatternDiagnostic, HintKind};
    use crate::tools::{FastSearchTool, ManageWorkspaceTool};

    /// Fingerprint of a search result set. Ignores score (which may shift due
    /// to ties) and keeps a stable, order-sensitive identity for each hit.
    type HitFingerprint = Vec<(String, Option<u32>, String)>;

    async fn run_search(
        handler: &JulieServerHandler,
        file_pattern: Option<String>,
    ) -> HitFingerprint {
        let tool = FastSearchTool {
            query: "calculate_total".to_string(),
            language: None,
            file_pattern,
            limit: 20,
            search_target: "content".to_string(),
            context_lines: Some(0),
            exclude_tests: None,
            workspace: Some("primary".to_string()),
            return_format: "full".to_string(),
        };

        let execution = tool
            .execute_with_trace(handler)
            .await
            .expect("search should not error")
            .execution
            .expect("execute_with_trace must populate execution for content search");

        execution
            .hits
            .iter()
            .map(|hit| (hit.file.clone(), hit.line, hit.name.clone()))
            .collect()
    }

    fn extract_text_from_result(result: &crate::mcp_compat::CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|value| value.as_str())
                        .map(|text| text.to_string())
                })
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn seed_workspace() -> (TempDir, JulieServerHandler) {
        let temp_dir = TempDir::new().expect("tempdir");
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        let tests_dir = workspace_path.join("tests");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&tests_dir).unwrap();
        fs::write(
            src_dir.join("math.rs"),
            "pub fn calculate_total(items: &[i32]) -> i32 {\n    items.iter().sum()\n}\n",
        )
        .unwrap();
        fs::write(
            src_dir.join("util.rs"),
            "pub fn calculate_total_ex(v: i32) -> i32 {\n    v * 2\n}\n",
        )
        .unwrap();
        fs::write(
            tests_dir.join("math_test.rs"),
            "fn calculate_total_smoke() { assert_eq!(2, 2); }\n",
        )
        .unwrap();

        let handler = JulieServerHandler::new_for_test()
            .await
            .expect("handler for test");
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await
            .expect("initialize workspace");

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool
            .call_tool(&handler)
            .await
            .expect("index workspace");

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        (temp_dir, handler)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn empty_and_whitespace_file_pattern_match_none() {
        let (_temp_dir, handler) = seed_workspace().await;

        let baseline = run_search(&handler, None).await;
        assert!(
            !baseline.is_empty(),
            "Baseline (file_pattern=None) must return hits; \
             workspace may not be indexed correctly"
        );

        for probe in ["", "   ", "\t", "\n"] {
            let probed = run_search(&handler, Some(probe.to_string())).await;
            assert_eq!(
                probed,
                baseline,
                "file_pattern={:?} must produce the same result set as None, \
                 got {} hits vs baseline {} hits",
                probe,
                probed.len(),
                baseline.len(),
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn whitespace_separated_globs_emit_syntax_hint_and_trace_diagnostic() {
        let (_temp_dir, handler) = seed_workspace().await;
        let tool = FastSearchTool {
            query: "calculate_total".to_string(),
            language: None,
            file_pattern: Some("src/** tests/**".to_string()),
            limit: 20,
            search_target: "content".to_string(),
            context_lines: Some(0),
            exclude_tests: None,
            workspace: Some("primary".to_string()),
            return_format: "full".to_string(),
        };

        let run = tool
            .execute_with_trace(&handler)
            .await
            .expect("search should not error");
        let execution = run
            .execution
            .expect("execute_with_trace must populate execution for content search");
        let text = extract_text_from_result(&run.result);

        assert!(
            execution.hits.is_empty(),
            "invalid pattern should stay zero-hit"
        );
        assert_eq!(
            execution.trace.file_pattern_diagnostic,
            Some(FilePatternDiagnostic::WhitespaceSeparatedMultiGlob),
        );
        assert_eq!(
            execution.trace.hint_kind,
            Some(HintKind::FilePatternSyntaxHint),
        );
        assert!(
            text.contains("multiple globs separated by whitespace"),
            "expected syntax hint text, got: {}",
            text,
        );
        assert!(
            text.contains("Use ',' or '|'"),
            "expected separator guidance, got: {}",
            text,
        );
    }
}
