# TODO

## Code Review Findings (2026-04-05)

Validated against source. All findings confirmed. **All fixed 2026-04-05.**

### High Priority

- [x] **edit_symbol stale index guard** -- Added blake3 hash freshness check in `call_tool` before applying indexed line ranges. Compares current file hash against stored hash; refuses with clear error if stale. (`src/tools/editing/edit_symbol.rs`)

- [x] **Session-connect catch-up misses deletions** -- Added `indexed_files.difference(&workspace_files)` check to `check_if_indexing_needed()`. Returns true to trigger cleanup when files were deleted while daemon was down. (`src/startup.rs`)

- [x] **Watcher 1s dedup drops real second saves** -- Changed from `continue` (drop) to `push_back` (re-queue) so the latest state is always eventually processed. (`src/watcher/mod.rs`)

### Medium Priority

- [x] **Embedding cancellation is global, not per-workspace** -- Changed `embedding_task` from single `Option` slot to `HashMap<String, ...>` keyed by workspace_id. Also updated cancel sites in `index.rs` and `refresh_stats.rs`. (`src/handler.rs`, `src/tools/workspace/indexing/embeddings.rs`)

- [x] **web-research skill leaks full pages into context** -- Combined fetch+save into single step that pipes directly to file, never printing to stdout. (`.claude/skills/web-research/SKILL.md`)

- [x] **edit_symbol is line-based at apply time** -- Documented line-granularity limitation in tool description. Long-term: byte offsets from tree-sitter for sub-line precision. (`src/handler.rs`)

### Low/Medium Priority

- [x] **Bracket-balance validation ignores strings and comments** -- Downgraded from `Err` (hard reject) to `Option<String>` (advisory warning appended to output). Updated call sites in both `edit_symbol.rs` and `edit_file.rs`. (`src/tools/editing/validation.rs`)

### Opportunities (from review)

- [ ] **Cap dry-run diff output for very large edits** -- The edit tools are token savers until the preview diff itself becomes the expensive part. Consider truncating diffs beyond a threshold (e.g., 200 lines) with a "diff truncated, N more lines" summary.

- [x] **Add end-to-end edit_symbol tests** -- Added 4 integration tests: replace via index, stale index rejection, insert_after dry-run, symbol not found. (`src/tests/tools/editing/edit_symbol_tests.rs`)

- [x] ~~**Document web-research fallbacks**~~ -- Removed. The filewatcher must index saved files reliably; if it doesn't, that's a bug to fix, not a workflow to document around.

## Architecture Questions

- [ ] **Do reference workspaces need separate indexes?** -- Currently `manage_workspace add /path/to/dep` creates a full separate `indexes/{ref_id}/db/symbols.db` + `tantivy/` for each reference. The centralized daemon already manages per-workspace indexes. Having a separate "reference" concept with its own index path may be unnecessary indirection. Needs analysis: is the `workspace_references` linkage table the only thing that needs to exist?

- [ ] **PreToolUse hook to enforce Julie tool usage in subagents** -- Subagents spawned via the Agent tool default to Grep/Glob/Read even when Julie MCP tools are available. A PreToolUse hook on the `Agent` tool could intercept the prompt and remind the parent to include explicit `mcp__julie__*` tool names. Ship as part of the Julie plugin so all users benefit.

## Enhancements

- [ ] **Linux ROCm (AMD GPU) support in sidecar bootstrap** -- PyTorch supports AMD GPUs on Linux via ROCm (`https://download.pytorch.org/whl/rocm6.2`). When ROCm torch is installed, `torch.cuda.is_available()` returns True (ROCm provides HIP-based CUDA compat), so the runtime `_select_device` works fine. But the Rust bootstrap (`sidecar_bootstrap.rs`) has no `detect_amd_rocm()` equivalent and never installs ROCm torch. Linux users with AMD GPUs silently get CPU-only embeddings. Detection: check for `rocminfo` command or `/opt/rocm`. Intel XPU (`intel-extension-for-pytorch`) is a similar gap but much more niche.
- [ ] **Windows Python launcher versioned probing** -- `python_interpreter_candidates()` lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs`)
- [ ] **NL query vocabulary gap** -- Code embedding models match tokens, not semantic synonyms ("save" != "record", "persist" != "insert"). Partially mitigated by embedding enrichment (file paths, implementor names, field signatures) and query classification (dynamic keyword/semantic weighting). Remaining gap: `fast_search` content mode with NL queries still misses code that `get_context` (embedding-powered) finds. Full fix likely needs NL query expansion or dual-model approach.
- [ ] **Self-improvement skill** -- Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement.

## Future Ideas

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
