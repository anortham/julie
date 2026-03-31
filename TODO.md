# TODO

## Bugs

- [ ] **Flaky search line mode tests** -- `test_fast_search_line_mode_combined_filters` and `test_fast_search_line_mode_language_filter` fail nondeterministically in the `tools-search` xtask bucket. Pass reliably in isolation. Likely a shared-state or fixture ordering issue in the line mode test suite. (`src/tests/tools/search/line_mode.rs`)

- [ ] **deep_dive parameter mismatch** -- `symbol_name` param sent by some clients but the tool expects `symbol`. Causes `missing field 'symbol'` deserialization error.

## Architecture Questions

- [ ] **Do reference workspaces need separate indexes?** -- Currently `manage_workspace add /path/to/dep` creates a full separate `indexes/{ref_id}/db/symbols.db` + `tantivy/` for each reference. The centralized daemon already manages per-workspace indexes. Having a separate "reference" concept with its own index path may be unnecessary indirection. Needs analysis: is the `workspace_references` linkage table the only thing that needs to exist?

- [ ] **PreToolUse hook to enforce Julie tool usage in subagents** -- Subagents spawned via the Agent tool default to Grep/Glob/Read even when Julie MCP tools are available. A PreToolUse hook on the `Agent` tool could intercept the prompt and remind the parent to include explicit `mcp__julie__*` tool names. Ship as part of the Julie plugin so all users benefit.

## Enhancements

- [ ] **Linux ROCm (AMD GPU) support in sidecar bootstrap** -- PyTorch supports AMD GPUs on Linux via ROCm (`https://download.pytorch.org/whl/rocm6.2`). When ROCm torch is installed, `torch.cuda.is_available()` returns True (ROCm provides HIP-based CUDA compat), so the runtime `_select_device` works fine. But the Rust bootstrap (`sidecar_bootstrap.rs`) has no `detect_amd_rocm()` equivalent and never installs ROCm torch. Linux users with AMD GPUs silently get CPU-only embeddings. Detection: check for `rocminfo` command or `/opt/rocm`. Intel XPU (`intel-extension-for-pytorch`) is a similar gap but much more niche.
- [ ] **Windows Python launcher versioned probing** -- `python_interpreter_candidates()` lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs`)
- [ ] **Embedding format versioning** -- When embedding enrichment format changes (e.g., adding field accesses), symbols need re-embedding. Currently requires `force=true` on reindex. Add a format version to the pipeline so changes trigger automatic re-embedding.
- [ ] **Worktree agent metrics are lost on cleanup** -- Worktree agents spawn their own Julie MCP server instance with a separate `.julie/` directory. When the worktree is cleaned up, those metrics are deleted. Fix: route metrics writes to the primary workspace's database regardless of which worktree Julie is running in, or consolidate metrics post-merge.
- [ ] **NL query vocabulary gap** -- Code embedding models match tokens, not semantic synonyms ("save" != "record", "persist" != "insert"). Needs NL query expansion or dual-model approach. Related: `fast_search` content mode with NL queries consistently misses relevant code that `get_context` (embedding-powered) finds.
- [ ] **Claude Code plugin distribution** -- Viable via a separate `julie-plugin` repo that bundles pre-built binaries + sidecar + skills. Key constraint: no PostInstall hooks exist in Claude Code plugin system, so bundling binaries is the only reliable approach. See goldfish plugin at `~/source/goldfish` as working example.
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
- **One persistent weakness:** "content management rich text editing" missed the core content subsystem. Embedding vectors for some symbols don't associate strongly with the concepts they implement. Worth investigating whether enriching embedding input (beyond symbol name + signature) helps.
- **Multi-instance VRAM is a concern** with larger models (768d). 2+ Julie processes = 2+ model loads. Daemon mode's shared `EmbeddingService` mitigates this for same-machine sessions.
