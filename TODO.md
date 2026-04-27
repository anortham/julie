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

## Search Quality: Post-File-Mode Metrics (2026-04-27)

Data: 1,876 fast_search calls with enriched telemetry (824 before file mode, 1,052 after).

**Headline:** True zero-hit rate dropped 15.2% to 8.0% (47% improvement), but raw rate looks flat (20.0% to 19.2%) because `file_pattern_filtered` failures spiked and mask the gain.

### Findings

- [ ] **F1: file_pattern scope rescue** -- 58% of remaining raw zero-hits (118/202). Agents pass narrow globs or exact file paths as `file_pattern`, Tantivy finds candidates across the codebase, then the filter eliminates everything. Fix: on `FilePatternFiltered + NoInScopeCandidates`, re-run without file_pattern and return labeled out-of-scope rescue results with structured trace fields (`scope_relaxed`, `original_file_pattern`). Sample is skewed (113/118 on Julie workspace during April 22-23 development). See plan for details.

- [ ] **F2: Agent file_pattern guidance** -- Guidance (not prohibition) on file_pattern usage. Single-file file_pattern is valid for grep-within-file; agents should prefer `get_symbols` when they want symbol structure. Update `JULIE_AGENT_INSTRUCTIONS.md` and rescue hint text. Ships alongside F1.

- [x] **F3: Julie workspace content latency** -- ~~Appeared as 511ms avg, turned out to be outlier skew.~~ P50 is 12ms, P95 is 46ms, 97.8% of searches are under 100ms at 15ms avg. Four extreme outliers (60-98s) from April 24 during re-indexing dragged the mean. Not a regression. No action needed.

- [ ] **F4: line_match_miss (two phases)** -- 26% of remaining true zero-hits (52/202). Phase 1: narrow OR-disjunction detection in `line_match_strategy`. Clean `identifier OR identifier OR identifier` patterns route to `FileLevel` instead of Substring. Does NOT blanket-parse OR/AND/NOT (would break SQL `INSERT OR REPLACE`, `IS NOT NULL`, etc.). Phase 2: separator normalization fallback in `line_matches_literal` (hyphen<->underscore, strip escape backslashes).

### Context

- File mode shipped April 22 (102 calls, 5.9% zero-hit rate, 13ms avg latency)
- `file_pattern_filtered` zeros are 96% concentrated on the Julie workspace (113/118), heavily on April 22-23 during file mode development
- Today's (Apr 27) elevated true-zero rate (22.5%) is from hermes-agent searching for symbols that genuinely don't exist in that codebase (loguru, structlog, UtcTime, etc.) -- correct behavior
- Definitions zero-hit rate rose slightly (6.3% to 9.5%) but absolute numbers are small (14 to 16)

## Future Ideas

- [x] **Full CLI mode for all Julie tools** -- Implemented. CLI execution now supports daemon/fallback/standalone modes, named wrappers, generic tool dispatch, and output formats (`src/cli_tools/`, `src/main.rs`, `src/tests/cli/`, `src/tests/cli_execution_tests.rs`).

- [ ] **AST-based complexity metrics** -- Add cyclomatic complexity calculation during AST extraction. Store as symbol metadata. Enables a `/hotspots` skill (complexity x centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 34 extractors.
- [ ] **Function body hashing for duplication detection** -- Hash normalized function bodies during extraction to detect near-duplicate functions across a codebase. Low priority.
- [x] **Scoped path extraction for Rust** -- Implemented as structured scoped-call resolution: Rust `scoped_identifier` calls preserve namespace paths, indexing carries structured pendings, and the resolver uses namespace-aware candidate selection to avoid false edges like `std::collections::HashMap::new()` resolving to local `new`.
