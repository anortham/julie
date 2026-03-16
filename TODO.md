# TODO

## Bugs

- [ ] **F4. Embedding KNN smoke test may be red** — `test_pipeline_knn_works_after_embedding` asserts `authenticate_user` ranks above `DatabaseConnection` for an auth query; needs verification run (`src/tests/integration/embedding_pipeline.rs`)
- [ ] **workspace_init is pre-existing red + pathological** — `tests::core::workspace_init::test_find_workspace_root_rejects_home_julie_dir` fails on both `main` and `feat/test-runner-tiering`, and the `workspace-init` bucket still times out even with a `480s` budget; this currently blocks treating `cargo xtask test system` / `full` as green-by-default (`src/tests/core/workspace_init.rs`)
- [x] ~~**`fast_search` content path ignores `exclude_tests`**~~ — fixed: `line_mode_search` now accepts and applies `exclude_tests` parameter using `is_test_path` for file-level filtering
- [x] ~~**Test coverage fallback can mislink tests to the wrong symbol**~~ — fixed: language filter applied in Rust code (not SQL — adding it to SQL broke the query plan, causing a 3+ min hang). GPT's "collapsed buckets" claim was incorrect — `(test_id, ident_name)` grouping is correct.
- [x] ~~**`get_context` drops risk/security labels in `SignatureOnly` mode**~~ — fixed: always fetch `full_symbols` for metadata even in SignatureOnly mode
- [x] ~~**`deep_dive` prints `Change Risk` twice**~~ — fixed: removed `format_change_risk_info` call from `format_header` (kind-specific formatters already call it)
- [x] ~~**`get_context` no-results output ignores compact/default format**~~ — fixed: routes through `format_context_with_mode` instead of hardcoded readable format
- [x] ~~**Definition search truncates before `exclude_tests` filtering**~~ — fixed: moved `filter_test_symbols` before `truncate(limit)` in both hybrid and keyword paths
- [x] ~~**`deep_dive` caller/blast-radius counts are mislabeled**~~ — fixed: relabeled to "dependents"/"incoming refs" since `incoming_total` includes all relationship types

## Tech Debt

- [ ] **Run embedding benchmark** — baseline vs candidate on `LabHandbookV2` reference workspace, record quality/overhead deltas
- [ ] **Watcher doesn't respect `.gitignore`** — The file walker uses `ignore::WalkBuilder` with `.gitignore` support, but the filewatcher uses hardcoded `glob::Pattern` via `build_ignore_patterns()`. Any `.gitignore` pattern not also in `build_ignore_patterns()` leaks through the watcher. Fix: use `ignore` crate's gitignore parsing at event-filter time, or sync patterns from `.gitignore` at startup. Key files: `src/watcher/filtering.rs`, `src/watcher/events.rs`, `src/utils/walk.rs`
- [ ] **Test quality regexes count comments/strings as evidence** — `analyze_test_body()` scans raw source, so comment text like `// should_err` can incorrectly upgrade a test to `thorough` (`src/analysis/test_quality.rs`, `src/tests/analysis/test_quality_tests.rs`)
- [x] ~~**Deep-dive test-location lookup is noisy and not linkage-based**~~ — partially fixed: added dedup by (file_path, symbol_name) and cap at 10. Full linkage-based approach is future work.
- [x] ~~**Cap and dedupe deep-dive test locations**~~ — fixed: dedup + truncate(10) in `build_test_refs`
- [ ] **Broaden and normalize cross-language test detection** — Java/Kotlin/C# annotation matching is exact-text only, while PHP/Swift `test*` detection is broad enough to risk production false positives (`crates/julie-extractors/src/test_detection.rs`, `crates/julie-extractors/src/java/methods.rs`, `crates/julie-extractors/src/csharp/helpers.rs`)
- [x] ~~**Go test detection misses fuzz/example entry points**~~ — fixed: `detect_go()` now recognizes `FuzzXxx` and `ExampleXxx` in addition to `TestXxx`
- [ ] **Add regression coverage for code-health output plumbing** — missing tests for line-mode `exclude_tests`, `get_context` label rendering in `SignatureOnly`, compact no-results formatting, and `deep_dive` change/security output let the current regressions slip through (`src/tests/tools/search/line_mode.rs`, `src/tests/tools/get_context_formatting_tests.rs`, `src/tests/tools/deep_dive_tests.rs`)
- [x] ~~**Doc contract tests are stale**~~ — fixed: updated AGENTS.md assertion to match current wording, added "green-by-default" caveat to CLAUDE.md/AGENTS.md for blocked tiers

## Performance

- [x] ~~**Pending relationship resolution is O(N) per relationship — bottleneck for large repos**~~ — fixed: `resolve_batch` groups pending relationships by callee_name and uses `find_symbols_by_names_batch` (SQL `IN` clause, chunked at 500) to query once per unique name instead of once per relationship. On Guava this should reduce ~434K individual queries to ~50K batched lookups. Key files: `src/tools/workspace/indexing/resolver.rs`, `src/database/symbols/queries.rs`

## Enhancements

- [ ] **Windows Python launcher `py -3.12` / `py -3.13` probing** — `python_interpreter_candidates()` doesn't try `py -3.12` syntax, which is the standard way to request a specific version on Windows (`src/embeddings/sidecar_bootstrap.rs:197-208`)

## Code Health Intelligence — Phase 2

### Search refinement with test metadata
- [x] **Filter test code from search results** — `fast_search` now supports `exclude_tests: Option<bool>` with smart default (definitions → include, NL → exclude). Filters on `metadata["is_test"]` after enrichment. Key files: `src/tools/search/text_search.rs`, `src/tools/search/mod.rs`

### Test-to-code linkage (Layer C)
- [x] **Test coverage linkage** — `compute_test_coverage()` in `src/analysis/test_coverage.rs`. Uses relationships + identifiers (with directory-proximity disambiguation) to find test→production linkages. Stores `metadata["test_coverage"]` with test_count, best/worst tier, covering test names.

### Test risk scoring (Layer D)
- [x] **Change risk scoring** — `compute_change_risk_scores()` in `src/analysis/change_risk.rs`. Combines centrality (log sigmoid P95 normalization), visibility, test weakness, and symbol kind into a 0.0–1.0 score with HIGH/MEDIUM/LOW labels. Surfaced in `deep_dive` (full breakdown) and `get_context` (label on pivots).

### Structural security risk signals
- [x] **`risk_score` column on symbols** — Six structural signals: exposure (visibility + kind), input handling (string/Request params), sensitive sinks (calls to query/exec/spawn), blast radius (centrality), untested (test coverage), flow depth (BFS from public entry to sink). Pre-computed at index time. Surface in `deep_dive` and `get_context`.

### Reference workspace considerations
- [ ] **Verify reference workspace coverage** — Test quality metrics run per-workspace during indexing via `process_files_optimized`, which handles both primary and reference workspaces. Verify with an integration test that indexes a reference workspace and confirms `is_test` metadata and `test_quality` metrics are present. Key files: `src/tools/workspace/indexing/processor.rs`, `src/tests/integration/reference_workspace.rs`

## Review Notes

- 2026-03-15 static review only — findings above come from code/test inspection; runtime verification is still pending.
- Post-indexing analysis order looks sane: reference scores -> test quality -> test coverage -> change risk -> security risk (`src/tools/workspace/indexing/processor.rs`).
- `get_context` batching is a solid improvement and avoids the usual N+1 nonsense (`src/tools/get_context/pipeline.rs`).
- Security sink detection deduplicates evidence across identifiers and relationships before scoring, which is the right shape for this feature (`src/analysis/security_risk.rs`).
- 2026-03-15 bugfix session — validated and fixed 7/7 code bugs, 4 tech debt items from GPT review. GPT's review quality was high — only overclaim was "collapsed identifier buckets" in test coverage.


Spotted this in the logs:
2026-03-14T18:15:16.584659Z  WARN julie::tools::workspace::indexing::processor: src/tools/workspace/indexing/processor.rs:629: ⏭️  Skipping symbol extraction for large file (762567 bytes > 488KB limit): /home/murphy/source/julie/fixtures/databases/julie-snapshot/metadata.json - indexing for text search only
