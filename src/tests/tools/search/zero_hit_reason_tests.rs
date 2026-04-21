//! Task 4a: `attribute_zero_hit_reason` pins the top-down stage walk used
//! to classify empty `line_mode_matches` runs. These tests fix one
//! variant per stage so any future reshuffle of the pipeline order —
//! or a silent bucket reassignment — fails loudly.
//!
//! Pure-unit coverage below drives every `ZeroHitReason` variant by
//! handing in a synthetic `LineModeStageCounts`. Two integration tests
//! at the bottom exercise the realistic variants against the live
//! pipeline (TantivyNoCandidates and LineMatchMiss) so the plumbing
//! from counters → attribution on the return path can't silently
//! regress.

#[cfg(test)]
mod tests {
    use crate::tools::search::line_mode::{
        LineModeStageCounts, attribute_zero_hit_reason,
    };
    use crate::tools::search::trace::ZeroHitReason;

    /// Tantivy returned nothing — the per-file loop never ran. This is
    /// the only stage that fires when `tantivy_file_candidates == 0` and
    /// dominates every other counter.
    #[test]
    fn tantivy_no_candidates_wins_when_zero_candidates_entered_the_loop() {
        let counts = LineModeStageCounts {
            and_candidates: 0,
            or_candidates: 0,
            tantivy_file_candidates: 0,
            file_pattern_dropped: 0,
            language_dropped: 0,
            test_dropped: 0,
            file_content_unavailable_dropped: 0,
            line_match_miss_dropped: 0,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::TantivyNoCandidates),
        );
    }

    /// file_pattern drains every candidate → FilePatternFiltered wins.
    /// Language/test/content counters deliberately set non-zero to
    /// confirm the walk is top-down and stops at the first drainer.
    #[test]
    fn file_pattern_filtered_wins_when_pattern_drains_the_survivors() {
        let counts = LineModeStageCounts {
            and_candidates: 10,
            or_candidates: 0,
            tantivy_file_candidates: 3,
            file_pattern_dropped: 3,
            // Would-have-been dropped if file_pattern hadn't taken them
            // first. The top-down walk must NOT credit these.
            language_dropped: 2,
            test_dropped: 2,
            file_content_unavailable_dropped: 1,
            line_match_miss_dropped: 1,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::FilePatternFiltered),
        );
    }

    /// Language filter is second in the walk. file_pattern must survive
    /// intact; language then drains to zero.
    #[test]
    fn language_filtered_wins_when_language_drains_after_file_pattern_survives() {
        let counts = LineModeStageCounts {
            and_candidates: 5,
            or_candidates: 0,
            tantivy_file_candidates: 4,
            file_pattern_dropped: 1,
            // 4 - 1 = 3 survive file_pattern; language drops all 3.
            language_dropped: 3,
            test_dropped: 0,
            file_content_unavailable_dropped: 0,
            line_match_miss_dropped: 0,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::LanguageFiltered),
        );
    }

    /// exclude_tests is third. file_pattern + language survive; test
    /// drains the rest.
    #[test]
    fn test_filtered_wins_when_exclude_tests_drains_after_earlier_stages_survive() {
        let counts = LineModeStageCounts {
            and_candidates: 5,
            or_candidates: 0,
            tantivy_file_candidates: 3,
            file_pattern_dropped: 0,
            language_dropped: 0,
            test_dropped: 3,
            file_content_unavailable_dropped: 0,
            line_match_miss_dropped: 0,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::TestFiltered),
        );
    }

    /// File-content-unavailable is fourth. All path/kind filters let
    /// candidates through; blob retrieval then fails for every one.
    #[test]
    fn file_content_unavailable_wins_when_content_lookup_drains_the_survivors() {
        let counts = LineModeStageCounts {
            and_candidates: 5,
            or_candidates: 0,
            tantivy_file_candidates: 2,
            file_pattern_dropped: 0,
            language_dropped: 0,
            test_dropped: 0,
            file_content_unavailable_dropped: 2,
            line_match_miss_dropped: 0,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::FileContentUnavailable),
        );
    }

    /// Every filter passes; line-level matching produces zero hits.
    /// This is the "Tantivy was optimistic, the actual lines don't
    /// contain the term" case.
    #[test]
    fn line_match_miss_wins_when_lines_drain_after_every_earlier_stage_survives() {
        let counts = LineModeStageCounts {
            and_candidates: 4,
            or_candidates: 0,
            tantivy_file_candidates: 2,
            file_pattern_dropped: 0,
            language_dropped: 0,
            test_dropped: 0,
            file_content_unavailable_dropped: 0,
            line_match_miss_dropped: 2,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::LineMatchMiss),
        );
    }

    /// Defensive fallback: candidates existed and nothing we instrument
    /// claims to have dropped them. Attribute to LineMatchMiss so the
    /// reason field is never silently None when there was something to
    /// explain.
    #[test]
    fn fallback_attributes_unexplained_drains_to_line_match_miss() {
        let counts = LineModeStageCounts {
            and_candidates: 5,
            or_candidates: 0,
            tantivy_file_candidates: 3,
            file_pattern_dropped: 0,
            language_dropped: 0,
            test_dropped: 0,
            file_content_unavailable_dropped: 0,
            // No drop counter fired, but `matches` is still empty at the
            // caller. Something took the survivors — attribute to the
            // closest thing we instrument rather than leaving None.
            line_match_miss_dropped: 0,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::LineMatchMiss),
        );
    }

    /// Promoted is NEVER produced by `attribute_zero_hit_reason`.
    /// teammate-a's Task 4b / Task 7 stamps it on the trace when a
    /// content→definitions auto-promotion fires. Pinning this so nobody
    /// "helpfully" wires it in here and breaks the ownership contract.
    #[test]
    fn promoted_is_never_produced_by_attribution_helper() {
        // Every plausible counter combination → verify Promoted never
        // appears. A brute-force sweep isn't needed; the helper has no
        // branch that constructs Promoted, but cover the extremes.
        let zero = LineModeStageCounts::default();
        let saturated = LineModeStageCounts {
            and_candidates: 100,
            or_candidates: 100,
            tantivy_file_candidates: 100,
            file_pattern_dropped: 100,
            language_dropped: 100,
            test_dropped: 100,
            file_content_unavailable_dropped: 100,
            line_match_miss_dropped: 100,
        };
        assert_ne!(
            attribute_zero_hit_reason(&zero),
            Some(ZeroHitReason::Promoted),
        );
        assert_ne!(
            attribute_zero_hit_reason(&saturated),
            Some(ZeroHitReason::Promoted),
        );
    }
}

/// Live-pipeline coverage: make sure `line_mode_matches` actually
/// populates `zero_hit_reason` on empty runs using the same attribution
/// helper. Only the realistic variants are exercised — LanguageFiltered
/// is effectively unreachable here because `SearchFilter.language` is
/// propagated into Tantivy (Task 3 finding), and FileContentUnavailable
/// would require a storage-failure injection that's out of scope for
/// this test.
#[cfg(test)]
mod integration_tests {
    use crate::handler::JulieServerHandler;
    use crate::tools::navigation::resolution::WorkspaceTarget;
    use crate::tools::search::line_mode::line_mode_matches;
    use crate::tools::search::trace::ZeroHitReason;
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

    /// Query a term that doesn't exist anywhere → Tantivy surfaces zero
    /// candidates → `zero_hit_reason` should be `TantivyNoCandidates`.
    #[tokio::test(flavor = "multi_thread")]
    async fn live_zero_hit_attributes_tantivy_no_candidates_for_missing_term() {
        let (_dir, handler) = seed_workspace(&[(
            "src/code.rs",
            "fn alpha() { let present = 1; }\n",
        )])
        .await;

        let result = line_mode_matches(
            "definitely_not_in_the_index_xyz",
            &None,
            &None,
            10,
            None,
            &WorkspaceTarget::Primary,
            &handler,
        )
        .await
        .expect("line_mode_matches");

        assert!(result.matches.is_empty());
        assert_eq!(
            result.zero_hit_reason,
            Some(ZeroHitReason::TantivyNoCandidates),
        );
    }

    /// Term exists but every file carrying it is outside the requested
    /// `file_pattern`. The per-file loop drops them all, and attribution
    /// lands on `FilePatternFiltered`.
    #[tokio::test(flavor = "multi_thread")]
    async fn live_zero_hit_attributes_file_pattern_when_pattern_drops_every_candidate() {
        // Two .rs files, both containing `marker_pattern`, but neither
        // under `src/ui/`. With `file_pattern=src/ui/**` the per-file
        // loop drops every candidate on file_pattern.
        let (_dir, handler) = seed_workspace(&[
            ("src/core.rs", "fn core() { let marker_pattern = 1; }\n"),
            (
                "crates/other/misc.rs",
                "fn misc() { let marker_pattern = 2; }\n",
            ),
        ])
        .await;

        let result = line_mode_matches(
            "marker_pattern",
            &None,
            &Some("src/ui/**".to_string()),
            10,
            None,
            &WorkspaceTarget::Primary,
            &handler,
        )
        .await
        .expect("line_mode_matches");

        assert!(result.matches.is_empty());
        assert_eq!(
            result.zero_hit_reason,
            Some(ZeroHitReason::FilePatternFiltered),
        );
    }

    /// Term exists only in a test file; `exclude_tests=true` drops it
    /// inside the per-file loop. Attribution lands on `TestFiltered`.
    #[tokio::test(flavor = "multi_thread")]
    async fn live_zero_hit_attributes_test_filtered_when_exclude_tests_drops_the_only_match() {
        let (_dir, handler) = seed_workspace(&[(
            "src/tests/util_test.rs",
            "fn scenario() { let marker_only_in_tests = 1; }\n",
        )])
        .await;

        let result = line_mode_matches(
            "marker_only_in_tests",
            &None,
            &None,
            10,
            Some(true),
            &WorkspaceTarget::Primary,
            &handler,
        )
        .await
        .expect("line_mode_matches");

        assert!(result.matches.is_empty());
        assert_eq!(
            result.zero_hit_reason,
            Some(ZeroHitReason::TestFiltered),
        );
    }

    /// Non-empty runs do NOT populate `zero_hit_reason`; the field is
    /// `None` so downstream consumers can treat `Some(_)` as proof of
    /// an empty result.
    #[tokio::test(flavor = "multi_thread")]
    async fn live_non_empty_run_leaves_zero_hit_reason_none() {
        let (_dir, handler) = seed_workspace(&[(
            "src/code.rs",
            "fn alpha() { let marker_found = 1; }\n",
        )])
        .await;

        let result = line_mode_matches(
            "marker_found",
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
        assert!(
            result.zero_hit_reason.is_none(),
            "non-empty matches should leave zero_hit_reason None, got {:?}",
            result.zero_hit_reason
        );
    }
}
