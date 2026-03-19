# TODO

## Bugs

- [ ] **workspace_init is pre-existing red + pathological** — `tests::core::workspace_init::test_find_workspace_root_rejects_home_julie_dir` fails on both `main` and `feat/test-runner-tiering`, and the `workspace-init` bucket still times out even with a `480s` budget; this currently blocks treating `cargo xtask test system` / `full` as green-by-default (`src/tests/core/workspace_init.rs`)
- [ ] **F4. Embedding KNN smoke test may be red** — `test_pipeline_knn_works_after_embedding` asserts `authenticate_user` ranks above `DatabaseConnection` for an auth query; needs verification run (`src/tests/integration/embedding_pipeline.rs`)

## Tech Debt

- [ ] **Run embedding benchmark** — baseline vs candidate on `LabHandbookV2` reference workspace, record quality/overhead deltas
- [x] **Consolidate `find_child_by_type` duplicates** — 8 copies across dart, gdscript, elixir, lua, razor consolidated to free functions in `base/tree_methods.rs`. 4 new tests added. `get_node_text` copies remain (dart uses thread-local cache, vue takes different args; not worth forcing into shared pattern).

## Performance

(No open items)

## Future Ideas

- [ ] **AST-based complexity metrics** — Add cyclomatic complexity calculation during AST extraction. Store as symbol metadata. Enables a `/hotspots` skill (complexity x centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 33 extractors — needs a language-agnostic approach.
- [ ] **Function body hashing for duplication detection** — Hash normalized function bodies during extraction to detect near-duplicate functions across a codebase. Low priority — useful during refactoring but the need arises rarely in practice.
- [ ] **Scoped path extraction for Rust** — Capture `crate::module::func()` qualified paths as implicit import edges. Currently these don't appear in `use` statements, so the call graph misses callers that use qualified paths. Would improve call graph quality for Rust codebases specifically.
- [ ] **Data-driven language config for semantic constants** — Move per-language constant tables (public keywords, method parent kinds, test decorators) from Rust match arms to config files. Would reduce boilerplate across 33 extractors without touching extraction logic. Big refactor with regression risk — future consideration.

## Enhancements

- [ ] **Windows Python launcher versioned probing** — `python_interpreter_candidates()` now lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs:196-213`)
- [ ] **Verify reference workspace coverage** — Test quality metrics run per-workspace during indexing via `process_files_optimized`, which handles both primary and reference workspaces. Verify with an integration test that indexes a reference workspace and confirms `is_test` metadata and `test_quality` metrics are present. Key files: `src/tools/workspace/indexing/processor.rs`, `src/tests/integration/reference_workspace.rs`

## Review Notes

- 2026-03-15 static review only — findings above come from code/test inspection; runtime verification is still pending.
- Post-indexing analysis order looks sane: reference scores -> test quality -> test coverage -> change risk -> security risk (`src/tools/workspace/indexing/processor.rs`).
- `get_context` batching is a solid improvement and avoids the usual N+1 nonsense (`src/tools/get_context/pipeline.rs`).
- Security sink detection deduplicates evidence across identifiers and relationships before scoring, which is the right shape for this feature (`src/analysis/security_risk.rs`).
- 2026-03-15 bugfix session — validated and fixed 7/7 code bugs, 4 tech debt items from GPT review.
- 2026-03-16 dogfood pass (primary + `LabHandbookV2`) — `deep_dive` test/risk metadata is already useful, but `get_context` still under-serves test-centric workflows.
- 2026-03-16 bugfix session — validated and fixed 4 more bugs from GPT review. All 8 xtask dev buckets green.
- 2026-03-17 dogfood session (Scala/Elixir) — found and fixed language detection sync, vendor detection, Elixir routing, test detection issues. Consolidated language detection to single source of truth.
- 2026-03-18 watcher `.gitignore` support — replaced hardcoded glob patterns with `ignore` crate's `Gitignore` matcher.
- 2026-03-18 added `query_metrics` MCP tool and 3 report skills (`/codehealth`, `/security-audit`, `/architecture`). Skills leverage existing analysis data via the new metadata query tool.
- 2026-03-18 codehealth-driven test coverage — 96 new tests targeting the highest-risk untested code identified by `/codehealth`: extractor critical path (`get_node_text`, `create_symbol`, `create_identifier`, `find_containing_symbol`, `find_doc_comment`), test detection dispatch (`is_test_symbol`), database write paths (`incremental_update_atomic`, `bulk_store_types`), and type conversion (`convert_types_map`).
