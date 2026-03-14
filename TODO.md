# TODO

## Bugs

- [ ] **F4. Embedding KNN smoke test may be red** — `test_pipeline_knn_works_after_embedding` asserts `authenticate_user` ranks above `DatabaseConnection` for an auth query; needs verification run (`src/tests/integration/embedding_pipeline.rs`)
- [ ] **workspace_init is pre-existing red + pathological** — `tests::core::workspace_init::test_find_workspace_root_rejects_home_julie_dir` fails on both `main` and `feat/test-runner-tiering`, and the `workspace-init` bucket still times out even with a `480s` budget; this currently blocks treating `cargo xtask test system` / `full` as green-by-default (`src/tests/core/workspace_init.rs`)

## Tech Debt

- [ ] **Run embedding benchmark** — baseline vs candidate on `LabHandbookV2` reference workspace, record quality/overhead deltas

## Enhancements

- [ ] **Windows Python launcher `py -3.12` / `py -3.13` probing** — `python_interpreter_candidates()` doesn't try `py -3.12` syntax, which is the standard way to request a specific version on Windows (`src/embeddings/sidecar_bootstrap.rs:197-208`)

## Code Health Intelligence — Phase 2

### Search refinement with test metadata
- [x] **Filter test code from search results** — `fast_search` now supports `exclude_tests: Option<bool>` with smart default. Filters on `metadata["is_test"]` after enrichment. Key files: `src/tools/search/text_search.rs`, `src/tools/search/mod.rs`

### Test-to-code linkage (Layer C)
- [x] **Test coverage linkage** — `compute_test_coverage()` in `src/analysis/test_coverage.rs`. Uses relationships + identifiers (with directory-proximity disambiguation) to find test→production linkages. Stores `metadata["test_coverage"]` with test_count, best/worst tier, covering test names.

### Test risk scoring (Layer D)
- [x] **Change risk scoring** — `compute_change_risk_scores()` in `src/analysis/change_risk.rs`. Combines centrality (log sigmoid P95 normalization), visibility, test weakness, and symbol kind into a 0.0–1.0 score with HIGH/MEDIUM/LOW labels. Surfaced in `deep_dive` (full breakdown) and `get_context` (label on pivots).

### Structural security risk signals
- [ ] **`risk_score` column on symbols** — Six structural signals: exposure (visibility + kind), input handling (string/Request params), sensitive sinks (calls to query/exec/spawn), blast radius (centrality), untested (test coverage), flow depth (BFS from public entry to sink). Pre-computed at index time. Surface in `deep_dive` and `get_context`.

### Reference workspace considerations
- [ ] **Verify reference workspace coverage** — Test quality metrics run per-workspace during indexing via `process_files_optimized`, which handles both primary and reference workspaces. Verify with an integration test that indexes a reference workspace and confirms `is_test` metadata and `test_quality` metrics are present. Key files: `src/tools/workspace/indexing/processor.rs`, `src/tests/integration/reference_workspace.rs`
