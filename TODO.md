# TODO

## Enhancements

- [ ] **Linux ROCm (AMD GPU) support in sidecar bootstrap** -- PyTorch supports AMD GPUs on Linux via ROCm (`https://download.pytorch.org/whl/rocm6.2`). When ROCm torch is installed, `torch.cuda.is_available()` returns True (ROCm provides HIP-based CUDA compat), so the runtime `_select_device` works fine. But the Rust bootstrap (`sidecar_bootstrap.rs`) has no `detect_amd_rocm()` equivalent and never installs ROCm torch. Linux users with AMD GPUs silently get CPU-only embeddings. Detection: check for `rocminfo` command or `/opt/rocm`. Intel XPU (`intel-extension-for-pytorch`) is a similar gap but much more niche.
- [ ] **Windows Python launcher versioned probing** -- `python_interpreter_candidates()` lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs`)
- [ ] **Self-improvement skill** -- Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement.

## Future Ideas

- [ ] **Tree-sitter pattern query tool** -- Expose tree-sitter's structural query language as a Julie tool for finding code by AST shape, not just text. Use case: "find this bug pattern elsewhere in the codebase" -- e.g., htmx attribute on element without paired init call, or function calls missing required follow-up. Semantic search finds similar *intent*; text search finds *literal matches*; neither finds *structural patterns*. Tree-sitter's S-expression queries are the right primitive. Infrastructure already exists (we run tree-sitter for 34 languages during extraction); the gap is a tool that accepts a query string and returns matching nodes with file/line/snippet.
- [ ] **AST-based complexity metrics** -- Add cyclomatic complexity calculation during AST extraction. Store as symbol metadata. Enables a `/hotspots` skill (complexity x centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 34 extractors.
- [ ] **Function body hashing for duplication detection** -- `body_hash` is already stored per symbol but currently used only for change detection. Repurpose for near-duplicate function detection across a codebase (normalize whitespace/identifiers before hashing for fuzzier matching). Low priority.
