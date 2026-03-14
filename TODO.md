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
- [ ] **Filter test code from search results** — Now that symbols have `metadata["is_test"]`, `fast_search` could support an `exclude_tests: bool` parameter (or `include_tests: false` default for NL queries). This would let agents focus on production code when searching. The existing `is_test_path()` scoring penalty (0.95x) is a soft signal; a hard filter using indexed metadata would be more precise since it operates at the symbol level, not just path level. Key files: `src/tools/search/`, `src/search/scoring.rs`

### Test-to-code linkage (Layer C)
- [ ] **`test_coverage` table** — Many-to-many mapping between test symbols and the production code they cover. Three linkage strategies: call graph (outgoing Calls from tests), import analysis (identifiers in test files referencing production symbols), naming convention (`test_process_payment` → `process_payment`). New table + migration 013.

### Test risk scoring (Layer D)
- [ ] **`test_risk_score` column on symbols** — Combine centrality, visibility, and test coverage quality into a single risk score. High centrality + public + untested = high risk. Surface in `get_context` pivots and `deep_dive` output.

### Structural security risk signals
- [ ] **`risk_score` column on symbols** — Six structural signals: exposure (visibility + kind), input handling (string/Request params), sensitive sinks (calls to query/exec/spawn), blast radius (centrality), untested (test coverage), flow depth (BFS from public entry to sink). Pre-computed at index time. Surface in `deep_dive` and `get_context`.

### Reference workspace considerations
- [ ] **Verify reference workspace coverage** — Test quality metrics run per-workspace during indexing via `process_files_optimized`, which handles both primary and reference workspaces. Verify with an integration test that indexes a reference workspace and confirms `is_test` metadata and `test_quality` metrics are present. Key files: `src/tools/workspace/indexing/processor.rs`, `src/tests/integration/reference_workspace.rs`
