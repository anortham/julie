# TODO

## Architecture Questions

- [x] **PreToolUse hook to enforce Julie tool usage in subagents** -- Subagents spawned via the Agent tool default to Grep/Glob/Read even when Julie MCP tools are available. A PreToolUse hook on the `Agent` tool now enforces the reminder workflow, and the hook is shipped through the plugin.
- [x] **Cap dry-run diff output for very large edits (rewrite_symbol)** -- The rewrite_symbol tool now truncates very large dry-run previews with a concise summary.
- [x] **Cap dry-run diff output for very large edits (edit_file)** -- `edit_file` dry-run previews now truncate large unified diffs with a line-count summary to avoid oversized tokens.

## Enhancements

- [ ] **Linux ROCm (AMD GPU) support in sidecar bootstrap** -- PyTorch supports AMD GPUs on Linux via ROCm (`https://download.pytorch.org/whl/rocm6.2`). When ROCm torch is installed, `torch.cuda.is_available()` returns True (ROCm provides HIP-based CUDA compat), so the runtime `_select_device` works fine. But the Rust bootstrap (`sidecar_bootstrap.rs`) has no `detect_amd_rocm()` equivalent and never installs ROCm torch. Linux users with AMD GPUs silently get CPU-only embeddings. Detection: check for `rocminfo` command or `/opt/rocm`. Intel XPU (`intel-extension-for-pytorch`) is a similar gap but much more niche.
- [ ] **Windows Python launcher versioned probing** -- `python_interpreter_candidates()` lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs`)
- [ ] **NL query vocabulary gap** -- Code embedding models match tokens, not semantic synonyms ("save" != "record", "persist" != "insert"). Partially mitigated by embedding enrichment (file paths, implementor names, field signatures) and query classification (dynamic keyword/semantic weighting). Remaining gap: `fast_search` content mode with NL queries still misses code that `get_context` (embedding-powered) finds. Full fix likely needs NL query expansion or dual-model approach.
- [ ] **Self-improvement skill** -- Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement.

## Future Ideas

- [ ] **Full CLI mode for all Julie tools** -- Add CLI subcommands so every MCP tool can be called from the terminal (e.g., `julie search "query" --definitions`, `julie deep-dive SymbolName`, `julie get-symbols src/foo.rs`). Two modes: daemon client (fast, connects via IPC to running daemon) and standalone (initializes workspace, runs, exits). Benefits: (1) live validation of new builds without restarting Claude Code, (2) automated integration test scripts that call real tools and assert on output, (3) search quality regression suites as shell scripts, (4) dogfooding without needing an MCP client session. Architecture: clap frontend dispatching to the same handler functions the MCP server uses. Needs full brainstorm before implementation.

- [ ] **AST-based complexity metrics** -- Add cyclomatic complexity calculation during AST extraction. Store as symbol metadata. Enables a `/hotspots` skill (complexity x centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 34 extractors.
- [ ] **Function body hashing for duplication detection** -- Hash normalized function bodies during extraction to detect near-duplicate functions across a codebase. Low priority.
- [ ] **Scoped path extraction for Rust** -- Capture `crate::module::func()` qualified paths as implicit import edges. Would improve call graph quality for Rust codebases specifically.
