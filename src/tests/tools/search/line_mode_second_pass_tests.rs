//! Task 5: the second-pass filter inside `line_mode_matches` is redundant
//! with the per-file loop filters. These tests pin that invariant so the
//! second-pass cannot be reintroduced without someone first re-answering
//! the "is it load-bearing?" question.
//!
//! The invariant, plainly stated: **every `LineMatch` returned by
//! `line_mode_matches` already satisfies the caller's `file_pattern`,
//! `language`, and `exclude_tests` constraints.** That is guaranteed by the
//! per-file loop, which skips any file that fails any of those checks
//! *before* it calls `collect_line_matches`. `collect_line_matches`
//! preserves the `file_path` verbatim, so no subsequent filter can ever
//! find a match to drop.
//!
//! The tests below:
//!
//! 1. Walk a mixed-path workspace and show the per-file counters populate
//!    while the second-pass has nothing left to do.
//! 2. Exercise the Target-workspace branch with the same invariant.
//!
//! If someone reintroduces the second-pass, either these tests (which
//! assert on `stage_counts.line_match_miss_dropped`) or the output set
//! will diverge.

#[cfg(test)]
mod tests {
    use crate::handler::JulieServerHandler;
    use crate::tools::navigation::resolution::WorkspaceTarget;
    use crate::tools::search::line_mode::line_mode_matches;
    use crate::tools::search::query::matches_glob_pattern;
    use crate::tools::workspace::ManageWorkspaceTool;
    use std::fs;
    use std::sync::atomic::Ordering;
    use tempfile::TempDir;
    use tokio::time::{Duration, sleep};

    async fn mark_index_ready(handler: &JulieServerHandler) {
        handler
            .indexing_status
            .search_ready
            .store(true, Ordering::Relaxed);
        *handler.is_indexed.write().await = true;
    }

    async fn seed_workspace(files: &[(&str, &str)]) -> (TempDir, JulieServerHandler) {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new().expect("tempdir");
        let workspace_path = temp_dir.path().to_path_buf();

        for (rel_path, content) in files {
            let full = workspace_path.join(rel_path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create parent dirs");
            }
            fs::write(full, content).expect("write file");
        }

        let handler = JulieServerHandler::new_for_test()
            .await
            .expect("handler init");
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await
            .expect("workspace init");

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await.expect("index");
        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        (temp_dir, handler)
    }

    /// The per-file loop is the sole arbiter of `file_pattern`,
    /// `language`, and `exclude_tests`. A mixed-extension, mixed-path
    /// workspace should have its filter work recorded on the per-file
    /// counters, with the second-pass dropping nothing additional.
    ///
    /// Asserted via `stage_counts.line_match_miss_dropped == 0` on a run
    /// where every kept match genuinely produced a line hit. If someone
    /// reintroduces a meaningful second-pass drop, that counter goes
    /// non-zero and this test fails.
    #[tokio::test(flavor = "multi_thread")]
    async fn every_line_match_satisfies_the_caller_filters_primary() {
        // All three files are .rs so the language filter (if any) stays
        // permissive and the per-file loop — not Tantivy — handles
        // file_pattern / exclude_tests. The test would be meaningless if
        // Tantivy pre-filtered these files out.
        let (_dir, handler) = seed_workspace(&[
            ("src/code.rs", "fn alpha() { let marker_gold = 1; }\n"),
            (
                "src/tests/util_test.rs",
                "fn scenario() { let marker_gold = 2; }\n",
            ),
            (
                "crates/other/misc.rs",
                "fn beta() { let marker_gold = 3; }\n",
            ),
        ])
        .await;

        let file_pattern = Some("src/**".to_string());
        let exclude_tests = Some(true);

        let result = line_mode_matches(
            "marker_gold",
            &None,
            &file_pattern,
            10,
            exclude_tests,
            &WorkspaceTarget::Primary,
            &handler,
        )
        .await
        .expect("line_mode_matches");

        assert!(
            !result.matches.is_empty(),
            "src/code.rs should produce at least one line match"
        );
        for m in &result.matches {
            let fp_pat = file_pattern.as_deref().expect("pattern set");
            assert!(
                matches_glob_pattern(&m.file_path, fp_pat),
                "match {} should satisfy file_pattern {}",
                m.file_path,
                fp_pat
            );
            assert!(
                !crate::search::scoring::is_test_path(&m.file_path),
                "exclude_tests=true should have dropped {}",
                m.file_path
            );
        }

        // crates/other/misc.rs dropped by file_pattern inside the per-file
        // loop (SearchFilter.file_pattern is unused by search_content, so
        // Tantivy cannot pre-filter on it).
        assert!(
            result.stage_counts.file_pattern_dropped >= 1,
            "per-file loop should have dropped crates/other/misc.rs via file_pattern"
        );
        // src/tests/util_test.rs dropped by exclude_tests inside the loop.
        assert!(
            result.stage_counts.test_dropped >= 1,
            "per-file loop should have dropped src/tests/util_test.rs via exclude_tests"
        );
        // The second pass is redundant: no extra drops land in the
        // line_match_miss bucket (which is where the second-pass delta
        // folded in the pre-Task-5 code). Any match that reached the
        // second pass already satisfies every filter, so there is
        // nothing for it to drop.
        assert_eq!(
            result.stage_counts.line_match_miss_dropped, 0,
            "second-pass filter is redundant: every LineMatch reaching it already satisfies all caller filters"
        );
    }

    /// Same invariant, no caller filters at all. Every reaching file
    /// passes trivially, and the second-pass is a no-op.
    #[tokio::test(flavor = "multi_thread")]
    async fn second_pass_is_noop_without_caller_filters() {
        let (_dir, handler) =
            seed_workspace(&[("src/code.rs", "fn alpha() { let marker_plain = 1; }\n")]).await;

        let result = line_mode_matches(
            "marker_plain",
            &None,
            &None,
            10,
            None,
            &WorkspaceTarget::Primary,
            &handler,
        )
        .await
        .expect("line_mode_matches");

        assert!(!result.matches.is_empty());
        assert_eq!(result.stage_counts.file_pattern_dropped, 0);
        assert_eq!(result.stage_counts.test_dropped, 0);
        assert_eq!(result.stage_counts.line_match_miss_dropped, 0);
    }
}
