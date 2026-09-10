//! Tests for Task 3: OR-fallback instrumentation + per-stage drop counters.
//!
//! RED → GREEN coverage for:
//!   * `ContentSearchResults.and_candidate_count` / `or_candidate_count`
//!   * `LineModeSearchResult.stage_counts` (each filter stage)

#[cfg(test)]
mod search_content_candidate_counts {
    use crate::search::index::{SearchDocument, SearchFilter, SearchIndex};
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
                .add_search_doc(&SearchDocument::file_from_parts(*path, *content, "rust"))
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
    use crate::tests::helpers::snapshot::snapshot_context;
    use crate::tools::navigation::resolution::WorkspaceTarget;
    use crate::tools::search::line_mode::line_mode_matches;
    use julie_test_support::FakeToolContext;
    use std::fs;
    use tempfile::TempDir;

    async fn seed_workspace(files: &[(&str, &str)]) -> (TempDir, FakeToolContext) {
        let temp_dir = TempDir::new().expect("tempdir");
        let workspace_path = temp_dir.path().to_path_buf();

        for (rel_path, content) in files {
            let full = workspace_path.join(rel_path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create parent dirs");
            }
            fs::write(full, content).expect("write file");
        }

        let handler = snapshot_context(&workspace_path).expect("snapshot fixture");

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
            // Reversed marker tokens keep the fallback line matcher empty,
            // so this remains a zero-hit stage-count test rather than a
            // scope-rescue test.
            (
                "src/example.rs",
                "fn alpha() { let abc = 1; let marker = 2; }\n",
            ),
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

    /// Task 3: when the first scoped fetch window is saturated by higher-ranked
    /// out-of-scope files but later ranked files are in-scope, the adaptive
    /// fetch loop should widen and return the in-scope hit instead of a
    /// diagnostic-only zero-hit.
    #[tokio::test(flavor = "multi_thread")]
    async fn scoped_candidate_starvation_returns_in_scope_hit() {
        let mut files = Vec::new();
        for idx in 0..120 {
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
            "adaptive scoped fetch should recover the in-scope file; got {:?}; counts={:?}; diagnostic={:?}; scope_relaxed={}",
            result
                .matches
                .iter()
                .map(|m| (&m.file_path, m.line_number, &m.line_content))
                .collect::<Vec<_>>(),
            result.stage_counts,
            result.file_pattern_diagnostic,
            result.scope_relaxed,
        );
        assert_eq!(result.matches[0].file_path, "src/ui/target.rs");
        assert!(!result.scope_relaxed);
        assert_eq!(result.original_file_pattern, None);
    }

    /// Observed behavior: `line_mode_matches` propagates the caller's `language`
    /// into the Tantivy `SearchFilter`, so a language mismatch dies at the
    /// Tantivy stage, not the per-file indexed-language safety check. The
    /// safety check is therefore unreachable in the current
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
