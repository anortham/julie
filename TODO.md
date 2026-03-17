# TODO

## Bugs

- [ ] **F4. Embedding KNN smoke test may be red** — `test_pipeline_knn_works_after_embedding` asserts `authenticate_user` ranks above `DatabaseConnection` for an auth query; needs verification run (`src/tests/integration/embedding_pipeline.rs`)
- [ ] **workspace_init is pre-existing red + pathological** — `tests::core::workspace_init::test_find_workspace_root_rejects_home_julie_dir` fails on both `main` and `feat/test-runner-tiering`, and the `workspace-init` bucket still times out even with a `480s` budget; this currently blocks treating `cargo xtask test system` / `full` as green-by-default (`src/tests/core/workspace_init.rs`)
- [ ] **Common method names can resolve to the wrong callee symbol** — in the `LabHandbookV2` reference workspace, `HandleAuthenticateAsync` is shown as calling `ApiResponse.Success`, but the code actually calls `AuthenticateResult.Success(ticket)`; the relationship/disambiguation path is too name-based for overloaded/common methods (`src/tools/workspace/indexing/resolver.rs`, `src/database/symbols/queries.rs`, `src/tools/deep_dive/data.rs`)

## Tech Debt

- [ ] **Run embedding benchmark** — baseline vs candidate on `LabHandbookV2` reference workspace, record quality/overhead deltas
- [ ] **Watcher doesn't respect `.gitignore`** — The file walker uses `ignore::WalkBuilder` with `.gitignore` support, but the filewatcher uses hardcoded `glob::Pattern` via `build_ignore_patterns()`. Any `.gitignore` pattern not also in `build_ignore_patterns()` leaks through the watcher. Fix: use `ignore` crate's gitignore parsing at event-filter time, or sync patterns from `.gitignore` at startup. Key files: `src/watcher/filtering.rs`, `src/watcher/events.rs`, `src/utils/walk.rs`
- [ ] **Test quality regexes count comments/strings as evidence** — `analyze_test_body()` scans raw source, so comment text like `// should_err` can incorrectly upgrade a test to `thorough` (`src/analysis/test_quality.rs`, `src/tests/analysis/test_quality_tests.rs`)
- [ ] **Broaden and normalize cross-language test detection** — Java/Kotlin/C# annotation matching is exact-text only, while PHP/Swift `test*` detection is broad enough to risk production false positives (`crates/julie-extractors/src/test_detection.rs`, `crates/julie-extractors/src/java/methods.rs`, `crates/julie-extractors/src/csharp/helpers.rs`)
- [ ] **Elixir extractor doesn't extract ExUnit `test` blocks as symbols** — `test "description" do` is a macro call with target `"test"`, but `dispatch_call` in `calls.rs` sends it to `_ => None`. These are the primary test definitions in Elixir projects. Need to add a `"test"` arm that extracts a Function symbol with `is_test: true` metadata. Currently test detection only works for `def`/`defp` functions inside test directories. (`crates/julie-extractors/src/elixir/calls.rs:37`)
- [ ] **Add regression coverage for code-health output plumbing** — missing tests for line-mode `exclude_tests`, `get_context` label rendering in `SignatureOnly`, compact no-results formatting, and `deep_dive` change/security output let the current regressions slip through (`src/tests/tools/search/line_mode.rs`, `src/tests/tools/get_context_formatting_tests.rs`, `src/tests/tools/deep_dive_tests.rs`)

## Performance

(No open items)

## Enhancements

- [ ] **Windows Python launcher `py -3.12` / `py -3.13` probing** — `python_interpreter_candidates()` doesn't try `py -3.12` syntax, which is the standard way to request a specific version on Windows (`src/embeddings/sidecar_bootstrap.rs:197-208`)
- [ ] **Verify reference workspace coverage** — Test quality metrics run per-workspace during indexing via `process_files_optimized`, which handles both primary and reference workspaces. Verify with an integration test that indexes a reference workspace and confirms `is_test` metadata and `test_quality` metrics are present. Key files: `src/tools/workspace/indexing/processor.rs`, `src/tests/integration/reference_workspace.rs`

## Review Notes

- 2026-03-15 static review only — findings above come from code/test inspection; runtime verification is still pending.
- Post-indexing analysis order looks sane: reference scores -> test quality -> test coverage -> change risk -> security risk (`src/tools/workspace/indexing/processor.rs`).
- `get_context` batching is a solid improvement and avoids the usual N+1 nonsense (`src/tools/get_context/pipeline.rs`).
- Security sink detection deduplicates evidence across identifiers and relationships before scoring, which is the right shape for this feature (`src/analysis/security_risk.rs`).
- 2026-03-15 bugfix session — validated and fixed 7/7 code bugs, 4 tech debt items from GPT review. GPT's review quality was high — only overclaim was "collapsed identifier buckets" in test coverage.
- 2026-03-16 dogfood pass (primary + `LabHandbookV2`) — `deep_dive` test/risk metadata is already useful, but `get_context` still under-serves test-centric workflows and the current security/callee heuristics can produce misleading output.
- 2026-03-16 bugfix session — validated and fixed 4 more bugs from GPT review: `filter` false positive in sinks, `protected`→`public` mislabel, `get_context` test_quality omission, exact test-name queries hidden by TEST_FILE_PENALTY. All 8 xtask dev buckets green.
- 2026-03-17 dogfood session (Scala/Elixir) — found and fixed: language detection lists not synced (5 files), vendor detection false positive on `lib/`, duplicate Elixir routing arms, Elixir test detection not wired up, Scala test detection JUnit-only. Consolidated all language detection to single source of truth in `crates/julie-extractors/src/language.rs`.
