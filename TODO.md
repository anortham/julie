# TODO

## Bugs

- [ ] **Flaky search line mode tests** -- `test_fast_search_line_mode_combined_filters` and `test_fast_search_line_mode_language_filter` fail nondeterministically in the `tools-search` xtask bucket. Pass reliably in isolation. Likely a shared-state or fixture ordering issue in the line mode test suite. (`src/tests/tools/search/line_mode.rs`)

- [x] **Force-indexing a reference workspace clears primary embeddings** -- `handle_index_command()` keeps the handler bound to the primary workspace for reference indexing, but the `force` path still calls `clear_all_embeddings()` through `handler.get_workspace().db`. A forced re-index of a reference workspace can wipe the primary workspace's vectors before re-embedding the reference DB. Clear embeddings for the indexed workspace ID, not the handler's active workspace. (`src/tools/workspace/commands/index.rs`)

- [x] **Incremental embedding watcher never sees the lazily initialized provider** -- `initialize_file_watcher()` snapshots `self.embedding_provider.clone()` before embeddings are deferred-initialized. Later `initialize_embedding_provider()` updates the workspace, but not the already-created `IncrementalIndexer`, so `dispatch_file_event()` usually runs with `embedding_provider=None` and live edits leave semantic vectors stale. Daemon mode is especially affected because the watcher never receives the shared `EmbeddingService` provider at all. (`src/workspace/mod.rs`, `src/watcher/mod.rs`)

- [x] **Workspace `vector_count` stats under-report after embedding runs** -- `spawn_workspace_embedding()` writes `stats.symbols_embedded` back to `daemon.db`, but that is only "vectors stored in this run", not "vectors currently on disk". Partial reruns, no-op reruns that re-embed enriched symbols, and incremental re-embeds all make `vector_count` drift from reality. Read `embedding_count()` after the pipeline and sync the same metadata from the per-file incremental path. (`src/tools/workspace/indexing/embeddings.rs`, `src/embeddings/pipeline.rs`, `src/watcher/mod.rs`)

- [x] **Hybrid semantic filtering ignores `exclude_tests` during merge** -- `hybrid_search()` filters semantic candidates with `matches_filter()`, but `matches_filter()` never checks `SearchFilter.exclude_tests`. `fast_search(..., search_target="definitions", exclude_tests=true)` removes test symbols only after merge and truncation, so semantic test hits can still consume slots and reduce recall for real results. (`src/search/hybrid.rs`, `src/tools/search/text_search.rs`)

- [x] **deep_dive parameter mismatch** -- Fixed via `#[serde(alias = "symbol_name")]` on the `symbol` field.

## Architecture Questions

- [ ] **Do reference workspaces need separate indexes?** -- Currently `manage_workspace add /path/to/dep` creates a full separate `indexes/{ref_id}/db/symbols.db` + `tantivy/` for each reference. The centralized daemon already manages per-workspace indexes. Having a separate "reference" concept with its own index path may be unnecessary indirection. Needs analysis: is the `workspace_references` linkage table the only thing that needs to exist?

- [ ] **PreToolUse hook to enforce Julie tool usage in subagents** -- Subagents spawned via the Agent tool default to Grep/Glob/Read even when Julie MCP tools are available. A PreToolUse hook on the `Agent` tool could intercept the prompt and remind the parent to include explicit `mcp__julie__*` tool names. Ship as part of the Julie plugin so all users benefit.

## Enhancements

- [ ] **Linux ROCm (AMD GPU) support in sidecar bootstrap** -- PyTorch supports AMD GPUs on Linux via ROCm (`https://download.pytorch.org/whl/rocm6.2`). When ROCm torch is installed, `torch.cuda.is_available()` returns True (ROCm provides HIP-based CUDA compat), so the runtime `_select_device` works fine. But the Rust bootstrap (`sidecar_bootstrap.rs`) has no `detect_amd_rocm()` equivalent and never installs ROCm torch. Linux users with AMD GPUs silently get CPU-only embeddings. Detection: check for `rocminfo` command or `/opt/rocm`. Intel XPU (`intel-extension-for-pytorch`) is a similar gap but much more niche.
- [ ] **Windows Python launcher versioned probing** -- `python_interpreter_candidates()` lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs`)
- [x] **Embedding format versioning** -- `EMBEDDING_FORMAT_VERSION` constant in `pipeline.rs` triggers full re-embed when bumped. Migration 014 adds `format_version` column to `embedding_config`. Version 2 = enriched format (file paths, implementor names, field signatures).
- [x] **Skip background embeddings on no-op `manage_workspace index`** -- Unlike `handle_refresh_command()`, the normal `index` path always spawns `spawn_workspace_embedding()` after indexing, even when incremental discovery found zero changed files. That turns repeated no-op index calls into expensive re-embed work because enriched symbols are always re-embedded. Gate this path the same way refresh already does. (`src/tools/workspace/commands/index.rs`, `src/embeddings/pipeline.rs`)
- [x] **Incremental per-file embeddings lag full-pipeline quality** -- `reembed_symbols_for_file()` runs with `lang_configs=None` and no implementor map, so per-file updates can miss language-specific extra kinds and trait/interface enrichment that the full workspace pipeline includes. If the watcher path is fixed, this quality gap will become much more visible. (`src/embeddings/pipeline.rs`, `src/embeddings/metadata.rs`)
- [ ] **Worktree agent metrics are lost on cleanup** -- Worktree agents spawn their own Julie MCP server instance with a separate `.julie/` directory. When the worktree is cleaned up, those metrics are deleted. Fix: route metrics writes to the primary workspace's database regardless of which worktree Julie is running in, or consolidate metrics post-merge.
- [ ] **NL query vocabulary gap** -- Code embedding models match tokens, not semantic synonyms ("save" != "record", "persist" != "insert"). Partially mitigated by embedding enrichment (file paths, implementor names, field signatures) and query classification (dynamic keyword/semantic weighting). Remaining gap: `fast_search` content mode with NL queries still misses code that `get_context` (embedding-powered) finds. Full fix likely needs NL query expansion or dual-model approach.
- [x] **Claude Code plugin distribution** -- Shipped as `julie-plugin` repo with bundled platform binaries. See `~/source/julie-plugin`.
- [ ] **Self-improvement skill** -- Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement.

## Future Ideas

- [ ] **Dashboard: code quality metrics page** -- Add a "Code Quality" page to the Observatory dashboard showing doc coverage (per-language breakdown, coverage %) and dead code candidates. Data layer already exists in `query_metrics` categories `doc_coverage` and `dead_code`. Bundle with other dashboard additions to make a single coherent update.
- [ ] **AST-based complexity metrics** -- Add cyclomatic complexity calculation during AST extraction. Store as symbol metadata. Enables a `/hotspots` skill (complexity x centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 33 extractors.
- [ ] **Function body hashing for duplication detection** -- Hash normalized function bodies during extraction to detect near-duplicate functions across a codebase. Low priority.
- [ ] **Scoped path extraction for Rust** -- Capture `crate::module::func()` qualified paths as implicit import edges. Would improve call graph quality for Rust codebases specifically.

## Historical Dogfood Notes

### 2026-03-21 LabHandbook Embedding Quality (CodeRankEmbed predecessor comparison)

Tested 9 semantic queries against LabHandbook V2 (434 files, 7306 symbols). Key takeaways that still apply to CodeRankEmbed:

- **Cross-language semantic search is the killer feature.** "How does the frontend communicate with the backend API" returned a complete API surface map across C# and TypeScript in one call.
- **NL text search (`fast_search`) is the weak spot.** Definition mode is excellent; content mode with NL queries misses relevant code. The quality gap between `fast_search` and `get_context` for NL queries is significant.
- **One persistent weakness:** "content management rich text editing" missed the core content subsystem. Embedding vectors for some symbols don't associate strongly with the concepts they implement. **Update (2026-04-03):** Embedding enrichment shipped (file paths, implementor names, field signatures, query classification). Similarity threshold lowered from 0.5 to 0.35. This improved conceptual search significantly but the core vocabulary gap remains for true semantic synonyms.
- **Multi-instance VRAM is a concern** with larger models (768d). 2+ Julie processes = 2+ model loads. Daemon mode's shared `EmbeddingService` mitigates this for same-machine sessions.
