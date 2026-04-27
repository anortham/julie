//! Tests for Task 3: OR-fallback instrumentation + per-stage drop counters.
//!
//! RED → GREEN coverage for:
//!   * `ContentSearchResults.and_candidate_count` / `or_candidate_count`
//!   * `LineModeSearchResult.stage_counts` (each filter stage)

#[cfg(test)]
mod search_content_candidate_counts {
    use crate::search::index::{FileDocument, SearchFilter, SearchIndex};
    use crate::search::language_config::LanguageConfigs;
    use tempfile::TempDir;

    /// Build a tiny index with a per-test set of file documents.
    fn build_index(docs: &[(&str, &str)]) -> (TempDir, SearchIndex) {
        let dir = TempDir::new().expect("tempdir");
        let configs = LanguageConfigs::load_embedded();
        let index = SearchIndex::create_with_language_configs(dir.path(), &configs)
            .expect("create search index");
        for (path, content) in docs {
            index
                .add_file_content(&FileDocument {
                    file_path: (*path).to_string(),
                    content: (*content).to_string(),
                    language: "rust".to_string(),
                })
                .expect("add file content");
        }
        index.commit().expect("commit");
        (dir, index)
    }

    /// The signature fixture from the plan: two files with overlapping tokens
    /// but no single file containing all three. AND must drop to zero,
    /// OR must rescue, and `relaxed` must flip to true.
    #[test]
    fn three_tokens_no_single_file_contains_all_falls_back_to_or() {
        let (_dir, index) = build_index(&[
            ("a.rs", "fn alpha() { let token_x = 1; let token_y = 2; }"),
            ("b.rs", "fn beta() { let token_y = 3; let token_z = 4; }"),
        ]);

        let result = index
            .search_content("token_x token_y token_z", &SearchFilter::default(), 10)
            .expect("search_content");

        assert_eq!(
            result.and_candidate_count,
            0,
            "AND should find no file containing all three tokens; got {:?}",
            result
                .results
                .iter()
                .map(|r| &r.file_path)
                .collect::<Vec<_>>()
        );
        assert!(
            result.or_candidate_count > 0,
            "OR fallback should produce at least one candidate; counted {}",
            result.or_candidate_count
        );
        assert!(
            result.relaxed,
            "relaxed flag must be true when OR fallback fires"
        );
        assert!(
            !result.results.is_empty(),
            "OR results should have been copied into the result vector"
        );
    }

    /// When AND already finds results, OR must NOT be invoked; `or_candidate_count`
    /// stays at zero and `relaxed` remains false.
    #[test]
    fn and_path_suppresses_or_fallback_counter() {
        let (_dir, index) = build_index(&[(
            "all_in_one.rs",
            "fn container() { let token_x = 1; let token_y = 2; let token_z = 3; }",
        )]);

        let result = index
            .search_content("token_x token_y token_z", &SearchFilter::default(), 10)
            .expect("search_content");

        assert!(
            result.and_candidate_count >= 1,
            "file contains all three tokens; AND should return >=1 (got {})",
            result.and_candidate_count
        );
        assert_eq!(
            result.or_candidate_count, 0,
            "OR fallback must stay dormant when AND hits"
        );
        assert!(
            !result.relaxed,
            "relaxed flag should remain false on AND-hit path"
        );
    }

    /// Single-word queries never trigger OR fallback even when AND hits zero
    /// (the word-count gate is explicit in `search_content`). Counters should
    /// reflect that: both candidate counts are zero when the term is absent.
    #[test]
    fn single_word_miss_does_not_trigger_or_fallback() {
        let (_dir, index) = build_index(&[("a.rs", "fn alpha() { let apple = 1; }")]);

        let result = index
            .search_content("nonexistent_symbol_xyz", &SearchFilter::default(), 10)
            .expect("search_content");

        assert_eq!(result.and_candidate_count, 0);
        assert_eq!(
            result.or_candidate_count, 0,
            "single-word queries skip OR fallback even on AND-miss"
        );
        assert!(!result.relaxed);
    }
}

#[cfg(test)]
mod line_mode_stage_counts {
    use crate::handler::JulieServerHandler;
    use crate::tools::navigation::resolution::WorkspaceTarget;
    use crate::tools::search::line_mode::line_mode_matches;
    use crate::tools::search::trace::{FilePatternDiagnostic, ZeroHitReason};
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

    /// Seed a tiny workspace with a map of `(relative_path, content)` entries,
    /// index it, and return the handler ready for line_mode_matches calls.
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

    /// Tantivy returns no candidates at all → `and_candidates == 0` and no
    /// OR fallback fires (single-word query), so every downstream counter is 0.
    #[tokio::test(flavor = "multi_thread")]
    async fn stage_tantivy_no_candidates() {
        let (_dir, handler) =
            seed_workspace(&[("src/example.rs", "fn alpha() { let apple = 1; }\n")]).await;

        let result = line_mode_matches(
            "completely_absent_symbol_zzz",
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
        assert_eq!(result.stage_counts.and_candidates, 0);
        assert_eq!(result.stage_counts.or_candidates, 0);
        assert_eq!(result.stage_counts.tantivy_file_candidates, 0);
        assert_eq!(result.stage_counts.file_pattern_dropped, 0);
        assert_eq!(result.stage_counts.language_dropped, 0);
        assert_eq!(result.stage_counts.test_dropped, 0);
        assert_eq!(result.stage_counts.file_content_unavailable_dropped, 0);
        assert_eq!(result.stage_counts.line_match_miss_dropped, 0);
    }

    /// Tantivy finds the file, but the caller's `file_pattern` rejects it.
    #[tokio::test(flavor = "multi_thread")]
    async fn stage_file_pattern_dropped() {
        let (_dir, handler) = seed_workspace(&[
            // Space-separated marker tokens keep the fallback line matcher
            // empty, so this remains a zero-hit stage-count test rather than
            // a scope-rescue test.
            ("src/example.rs", "fn alpha() { let marker abc = 1; }\n"),
            ("docs/notes.md", "# docs\n"),
        ])
        .await;

        let result = line_mode_matches(
            "marker_abc",
            &None,
            &Some("docs/**".to_string()),
            10,
            None,
            &WorkspaceTarget::Primary,
            &handler,
        )
        .await
        .expect("line_mode_matches");

        assert!(
            result.matches.is_empty(),
            "file_pattern should exclude the sole hit"
        );
        assert!(
            result.stage_counts.tantivy_file_candidates >= 1,
            "Tantivy should have returned the src file candidate"
        );
        assert!(
            result.stage_counts.file_pattern_dropped >= 1,
            "file_pattern filter should have dropped the src file"
        );
        assert!(!result.scope_relaxed);
    }

    /// Scoped zero-hit with no matching paths even after a wider probe should
    /// classify as `NoInScopeCandidates` while keeping the coarse stage as
    /// `FilePatternFiltered`.
    #[tokio::test(flavor = "multi_thread")]
    async fn file_pattern_diagnostic_no_in_scope_candidates() {
        let (_dir, handler) = seed_workspace(&[
            // Space-separated marker tokens keep the fallback line matcher
            // empty, so the original no-in-scope diagnostic is observable.
            ("src/core.rs", "fn core() { let marker scope = 1; }\n"),
            (
                "crates/other/misc.rs",
                "fn misc() { let marker scope = 2; }\n",
            ),
        ])
        .await;

        let result = line_mode_matches(
            "marker_scope",
            &None,
            &Some("src/ui/**".to_string()),
            1,
            None,
            &WorkspaceTarget::Primary,
            &handler,
        )
        .await
        .expect("line_mode_matches");

        assert!(result.matches.is_empty(), "scoped miss should stay empty");
        assert_eq!(
            result.zero_hit_reason,
            Some(ZeroHitReason::FilePatternFiltered),
        );
        assert_eq!(
            result.file_pattern_diagnostic,
            Some(FilePatternDiagnostic::NoInScopeCandidates),
        );
        assert!(!result.scope_relaxed);
    }

    /// Task 3: when the first scoped fetch window is saturated by higher-ranked
    /// out-of-scope files but later ranked files are in-scope, the adaptive
    /// fetch loop should widen and return the in-scope hit instead of a
    /// diagnostic-only zero-hit.
    #[tokio::test(flavor = "multi_thread")]
    async fn scoped_candidate_starvation_returns_in_scope_hit() {
        let mut files = Vec::new();
        for idx in 0..800 {
            files.push((
                format!("crates/outscope/file_{idx:03}.rs"),
                format!(
                    "fn out_{idx}() {{ let marker_starvation = 1; let marker_starvation = 2; let marker_starvation = 3; let marker_starvation = 4; let marker_starvation = 5; let marker_starvation = 6; let marker_starvation = 7; let marker_starvation = 8; }}\n"
                ),
            ));
        }
        files.push((
            "src/ui/target.rs".to_string(),
            format!(
                "fn target() {{ {} let marker_starvation = 1; }}\n",
                "let filler = 0; ".repeat(200)
            ),
        ));
        let file_refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, content)| (path.as_str(), content.as_str()))
            .collect();
        let (_dir, handler) = seed_workspace(&file_refs).await;

        let result = line_mode_matches(
            "marker_starvation",
            &None,
            &Some("src/ui/**".to_string()),
            1,
            None,
            &WorkspaceTarget::Primary,
            &handler,
        )
        .await
        .expect("line_mode_matches");

        assert_eq!(
            result.matches.len(),
            1,
            "adaptive scoped fetch should recover the in-scope file; got {:?}",
            result
                .matches
                .iter()
                .map(|m| (&m.file_path, m.line_number, &m.line_content))
                .collect::<Vec<_>>(),
        );
        assert_eq!(result.matches[0].file_path, "src/ui/target.rs");
        assert_eq!(result.zero_hit_reason, None);
        assert!(!result.scope_relaxed);
        assert_eq!(result.original_file_pattern, None);
        assert_eq!(result.original_zero_hit_reason, None);
        assert_eq!(
            result.file_pattern_diagnostic, None,
            "successful widened fetch should not leave a zero-hit diagnostic behind",
        );
    }

    /// Observed behavior: `line_mode_matches` propagates the caller's `language`
    /// into the Tantivy `SearchFilter`, so a language mismatch dies at the
    /// Tantivy stage, not the per-file `file_matches_language` check. The
    /// per-file language filter is therefore unreachable in the current
    /// pipeline; this test pins that fact for Task 5's investigation and the
    /// diagnosis report.
    #[tokio::test(flavor = "multi_thread")]
    async fn stage_language_filter_is_redundant_with_tantivy_filter() {
        let (_dir, handler) =
            seed_workspace(&[("src/example.rs", "fn alpha() { let marker_lang = 1; }\n")]).await;

        let result = line_mode_matches(
            "marker_lang",
            &Some("python".to_string()),
            &None,
            10,
            None,
            &WorkspaceTarget::Primary,
            &handler,
        )
        .await
        .expect("line_mode_matches");

        assert!(
            result.matches.is_empty(),
            "python filter excludes the .rs file"
        );
        assert_eq!(
            result.stage_counts.tantivy_file_candidates, 0,
            "Tantivy should have filtered out the .rs file via SearchFilter.language",
        );
        assert_eq!(
            result.stage_counts.language_dropped, 0,
            "per-file language filter is unreachable when Tantivy already filters language",
        );
    }

    /// `exclude_tests=true` drops files whose paths look test-y.
    #[tokio::test(flavor = "multi_thread")]
    async fn stage_test_dropped() {
        let (_dir, handler) = seed_workspace(&[(
            "src/tests/example_test.rs",
            "fn scenario() { let marker_tests = 1; }\n",
        )])
        .await;

        let result = line_mode_matches(
            "marker_tests",
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
        assert!(
            result.stage_counts.test_dropped >= 1,
            "exclude_tests should have filtered out the test-path file"
        );
    }

    /// Happy path sanity check: successful hits keep all drop counters at 0.
    #[tokio::test(flavor = "multi_thread")]
    async fn stage_counts_zero_on_happy_path() {
        let (_dir, handler) =
            seed_workspace(&[("src/example.rs", "fn alpha() { let marker_ok = 1; }\n")]).await;

        let result = line_mode_matches(
            "marker_ok",
            &None,
            &None,
            10,
            None,
            &WorkspaceTarget::Primary,
            &handler,
        )
        .await
        .expect("line_mode_matches");

        assert!(!result.matches.is_empty(), "happy-path query should match");
        assert_eq!(result.stage_counts.file_pattern_dropped, 0);
        assert_eq!(result.stage_counts.language_dropped, 0);
        assert_eq!(result.stage_counts.test_dropped, 0);
        assert_eq!(result.stage_counts.file_content_unavailable_dropped, 0);
        assert_eq!(result.stage_counts.line_match_miss_dropped, 0);
    }
}
