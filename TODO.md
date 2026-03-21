# TODO

## Bugs

- [x] **workspace_init is pre-existing red + pathological** — Verified 2026-03-19: all 11 tests pass (including `test_find_workspace_root_rejects_home_julie_dir`), no timeout. Likely fixed during 2026-03-16/17 bugfix sessions.
- [x] **F4. Embedding KNN smoke test may be red** — Verified 2026-03-19: `test_pipeline_knn_works_after_embedding` passes.
- [x] **Watcher: incremental indexing Tantivy content test was missing commit** — Test called handler directly without committing (production path batch-commits via `process_pending_changes`). Added explicit `idx.commit()` after handler call. Fixed 2026-03-19. System tier now fully green (124 pass, 0 fail).

## Tech Debt

- [x] **Run embedding benchmark** — Completed 2026-03-20. Benchmarked 19 workspaces (18 languages, 93k source embeddings). Results in `benchmarks/results/`. Test exclusion saves 47% of embedding budget. CodeRankEmbed shows +10% namespace overlap, +20% cross-language vs BGE-small.
- [x] **Consolidate `find_child_by_type` duplicates** — 8 copies across dart, gdscript, elixir, lua, razor consolidated to free functions in `base/tree_methods.rs`. 4 new tests added. `get_node_text` copies remain (dart uses thread-local cache, vue takes different args; not worth forcing into shared pattern).

## Performance

(No open items)

## Future Ideas

- [ ] **AST-based complexity metrics** — Add cyclomatic complexity calculation during AST extraction. Store as symbol metadata. Enables a `/hotspots` skill (complexity x centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 33 extractors — needs a language-agnostic approach.
- [ ] **Function body hashing for duplication detection** — Hash normalized function bodies during extraction to detect near-duplicate functions across a codebase. Low priority — useful during refactoring but the need arises rarely in practice.
- [ ] **Scoped path extraction for Rust** — Capture `crate::module::func()` qualified paths as implicit import edges. Currently these don't appear in `use` statements, so the call graph misses callers that use qualified paths. Would improve call graph quality for Rust codebases specifically.
- [ ] **Data-driven language config for semantic constants** — Move per-language constant tables (public keywords, method parent kinds, test decorators) from Rust match arms to config files. Would reduce boilerplate across 33 extractors without touching extraction logic. Big refactor with regression risk — future consideration.

## Enhancements

- [ ] **Upgrade to ORT rc.12 and test auto-device on Mac** — `ort` crate 2.0.0-rc.12 adds `SessionBuilder::with_auto_device` (ONNX Runtime 1.22+) which auto-selects NPU when available. On Apple Silicon, the Neural Engine is an NPU. If this routes to CoreML/ANE without the 13GB memory bloat we saw before, it would give us GPU-class acceleration via ORT natively, eliminating the Mac sidecar dependency for ONNX models. Also ships CUDA 13 builds. Caveat: maintainer says "expect little to no macOS support" after losing Hackintosh VM.
- [ ] **Evaluate CodeRankEmbed ONNX export** — Track [fastembed issue #587](https://github.com/qdrant/fastembed/issues/587). Once an ONNX export exists, CodeRankEmbed (currently sidecar-only, best quality) could run via ORT natively on all platforms with DirectML/CoreML/CUDA acceleration. This would make it viable as the default model everywhere without requiring the Python sidecar.
- [ ] **Embedding model selection** — Currently BGE-small-en-v1.5 (384d) is the default everywhere. CodeRankEmbed (768d, nomic-ai) is available as opt-in via `JULIE_EMBEDDING_SIDECAR_MODEL_ID=nomic-ai/CodeRankEmbed`. Benchmark results: +10% namespace overlap, +20% cross-language, 2.5x slower (375 vs 928 sym/s on MPS). Jina-code-v2 is broken on the sidecar (transformers 5.x incompatibility) but works via ORT. Decision deferred until either: (a) CodeRankEmbed gets ONNX export, or (b) ORT rc.12 auto-device works on Mac. See `benchmarks/results/` for full data.
- [x] **Embedding coverage gaps per language** — Addressed 2026-03-20 via per-language TOML configs (`languages/*.toml` `[embeddings]` section). Constructors now included for Java/C#/Kotlin/Swift/Dart/Scala via `extra_kinds = ["constructor"]`. Variable budgets tunable per-language via `variable_ratio`. Benchmark confirms coverage up 30-136% for affected languages with no quality loss. Remaining gap: Zig constants (4,394) not yet addressed. See `benchmarks/results/bge-small-per-lang-configs.md`.
- [ ] **Windows Python launcher versioned probing** — `python_interpreter_candidates()` now lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs:196-213`)
- [ ] **Worktree agent metrics are lost on cleanup** — Worktree agents spawn their own Julie MCP server instance with a separate `.julie/` directory. When the worktree is cleaned up, those metrics are deleted. Even if the worktree persists, metrics don't merge back (`.julie/` is gitignored). Fix: route metrics writes to the primary workspace's database regardless of which worktree Julie is running in, or consolidate metrics post-merge.
- [ ] **Verify reference workspace coverage** — Test quality metrics run per-workspace during indexing via `process_files_optimized`, which handles both primary and reference workspaces. Verify with an integration test that indexes a reference workspace and confirms `is_test` metadata and `test_quality` metrics are present. Key files: `src/tools/workspace/indexing/processor.rs`, `src/tests/integration/reference_workspace.rs`
- [ ] **Claude Code plugin distribution** — Investigated 2026-03-20. Viable via a separate `julie-plugin` repo that bundles pre-built binaries + sidecar + skills. Key findings:
  - **Separate repo required**: Julie's source repo is 33GB; users need only the ~79MB binary, 484KB sidecar, and plugin metadata. The plugin repo is a distribution artifact, not a dev repo.
  - **Binary bundling**: Include all 3 platform binaries (`bin/{platform}/julie-server`) directly in the repo. ~75-100MB total. Use force-push on release to avoid git history bloat.
  - **Launcher script**: `.mcp.json` calls `bash ${CLAUDE_PLUGIN_ROOT}/scripts/launch.sh` which detects platform and `exec`s the right binary. MCP server defined inline in `plugin.json` (like goldfish pattern).
  - **Sidecar bundling**: Include the Python sidecar source (484KB) in the plugin repo. Point `JULIE_EMBEDDING_SIDECAR_SCRIPT` at it via env in the MCP config. Julie's existing `uv` bootstrapping handles venv creation.
  - **Skills bundled**: search-debug, explore-area, call-trace, impact-analysis, type-flow, dependency-graph, logic-flow all ship with the plugin. Manual users would still need to copy skills separately.
  - **Hooks**: SessionStart for auto-recall/indexing, PreCompact for checkpointing, etc.
  - **CI integration**: Extend release.yml to copy binaries + sidecar + skills into `julie-plugin/` and push.
  - **Windows launcher**: Needs `.cmd` or PowerShell equivalent since bash isn't guaranteed. Or rely on Git Bash / WSL.
  - **No PostInstall hooks exist** in Claude Code plugin system (open feature request #9394/#11240). SessionStart can't download binaries because MCP connects before hooks run. Bundling is the only reliable approach.
  - **Manual path unchanged**: Non-Claude-Code users still download binary, add to PATH, register MCP, copy skills. Plugin is additive, not a replacement.
  - Reference: https://code.claude.com/docs/en/plugins, goldfish plugin at ~/source/goldfish as working example

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
