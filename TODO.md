# TODO

## Architecture Questions

- [x] **PreToolUse hook to enforce Julie tool usage in subagents** -- Subagents spawned via the Agent tool default to Grep/Glob/Read even when Julie MCP tools are available. A PreToolUse hook on the `Agent` tool now enforces the reminder workflow, and the hook is shipped through the plugin.
- [x] **Cap dry-run diff output for very large edits (rewrite_symbol)** -- The rewrite_symbol tool now truncates very large dry-run previews with a concise summary.
- [x] **Cap dry-run diff output for very large edits (edit_file)** -- `edit_file` dry-run previews now truncate large unified diffs with a line-count summary to avoid oversized tokens.

## Enhancements

- [ ] **Linux ROCm (AMD GPU) support in sidecar bootstrap** -- PyTorch supports AMD GPUs on Linux via ROCm (`https://download.pytorch.org/whl/rocm6.2`). When ROCm torch is installed, `torch.cuda.is_available()` returns True (ROCm provides HIP-based CUDA compat), so the runtime `_select_device` works fine. But the Rust bootstrap (`sidecar_bootstrap.rs`) has no `detect_amd_rocm()` equivalent and never installs ROCm torch. Linux users with AMD GPUs silently get CPU-only embeddings. Detection: check for `rocminfo` command or `/opt/rocm`. Intel XPU (`intel-extension-for-pytorch`) is a similar gap but much more niche.
- [ ] **Windows Python launcher versioned probing** -- `python_interpreter_candidates()` lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs`)
- [x] **NL query vocabulary gap** -- Stale. Query classification, hybrid weighting, NL embedding fallback, and deterministic query expansion have since landed (`src/search/weights.rs`, `src/tools/search/nl_embeddings.rs`, `src/search/expansion.rs`). Re-open with fresh examples if current searches still miss obvious synonyms.
- [ ] **Self-improvement skill** -- Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement.

## Future Ideas

- [x] **Full CLI mode for all Julie tools** -- Implemented. CLI execution now supports daemon/fallback/standalone modes, named wrappers, generic tool dispatch, and output formats (`src/cli_tools/`, `src/main.rs`, `src/tests/cli/`, `src/tests/cli_execution_tests.rs`).

- [ ] **AST-based complexity metrics** -- Add cyclomatic complexity calculation during AST extraction. Store as symbol metadata. Enables a `/hotspots` skill (complexity x centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 34 extractors.
- [ ] **Function body hashing for duplication detection** -- Hash normalized function bodies during extraction to detect near-duplicate functions across a codebase. Low priority.
- [x] **Scoped path extraction for Rust** -- Implemented as structured scoped-call resolution: Rust `scoped_identifier` calls preserve namespace paths, indexing carries structured pendings, and the resolver uses namespace-aware candidate selection to avoid false edges like `std::collections::HashMap::new()` resolving to local `new`.
