# TODO

## Bugs

No known bugs. All previous issues resolved as of v5.5.4.

## Tech Debt

- [ ] **Multiple Instances** - Discuss the impact of having multiple instances of Julie running at the same time: multiple projects, git worktrees, shared VRAM, etc.

## Performance

- [ ] **ORT VRAM management for larger models** — Jina-code-v2 (768d, ~270MB) is significantly larger than BGE-small (384d, ~33MB). Current state:
  - `EMBEDDING_BATCH_SIZE = 250` in `pipeline.rs` is model-agnostic.
  - ORT provider has zero VRAM awareness; relies on DirectML crash fallback (`run_with_cpu_fallback`).
  - **Multiple instances are the real risk**: 2+ Julie processes = 2+ full model loads in VRAM.
  - **Proposed fixes**: (a) Model-aware batch size. (b) VRAM query via DirectML/DXGI before loading. (c) Document the multi-instance VRAM risk.
  - Key files: `src/embeddings/pipeline.rs`, `src/embeddings/ort_provider.rs`, Miller project at `c:\source\miller` has GPU memory detection patterns via WMI.

## Enhancements

- [ ] **Upgrade to ORT rc.12 and test auto-device on Mac** — `ort` crate 2.0.0-rc.12 adds `SessionBuilder::with_auto_device` (ONNX Runtime 1.22+) which auto-selects NPU when available. On Apple Silicon, the Neural Engine is an NPU. Caveat: maintainer says "expect little to no macOS support" after losing Hackintosh VM.
- [ ] **Evaluate CodeRankEmbed ONNX export** — Track [fastembed issue #587](https://github.com/qdrant/fastembed/issues/587). Once an ONNX export exists, CodeRankEmbed (currently sidecar-only, best quality) could run via ORT natively.
- [ ] **Embedding model selection** — A/B tested 2026-03-21. Jina-code-v2 beats BGE-small on cross-language queries, BGE wins on English-concept-to-code bridging. Jina-code-v2 is the default for multi-language codebases. BGE-small fallback for single-language or resource-constrained.
- [ ] **Windows Python launcher versioned probing** — `python_interpreter_candidates()` doesn't try `py -3.12` syntax. Needs rework from `Vec<OsString>` to support args. (`src/embeddings/sidecar_bootstrap.rs:196-213`)
- [ ] **Worktree agent metrics lost on cleanup** — Worktree agents spawn their own Julie instance with a separate `.julie/`. When the worktree is cleaned up, metrics are deleted. Fix: route metrics writes to the primary workspace's database.
- [ ] **Verify reference workspace coverage** — Add integration test that indexes a reference workspace and confirms `is_test` metadata and `test_quality` metrics are present.
- [ ] **Claude Code plugin distribution** — Investigated 2026-03-20. Viable via a separate `julie-plugin` repo bundling pre-built binaries + sidecar + skills. See extended notes in git history.
- [ ] **Investigate `fast_search` NL fallback** — When text search returns low-confidence results for NL queries, consider blending with embedding similarity. The quality gap between `fast_search` and `get_context` for NL queries is significant.
- [ ] **Embedding format versioning** — When embedding enrichment format changes (e.g., adding field accesses), symbols need re-embedding. Currently requires `force=true` on reindex. Add a format version to the pipeline so changes trigger automatic re-embedding.
- [ ] **Self-improvement skill** — Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement. Would help developers improve code discoverability.

## Future Ideas

- [ ] **AST-based complexity metrics** — Cyclomatic complexity during AST extraction. Enables `/hotspots` skill (complexity x centrality = refactoring targets). Needs language-agnostic approach across 33 extractors.
- [ ] **Function body hashing for duplication detection** — Hash normalized function bodies to detect near-duplicates. Low priority.
- [ ] **Scoped path extraction for Rust** — Capture `crate::module::func()` qualified paths as implicit import edges. Would improve Rust call graph quality.
- [ ] **Data-driven language config for semantic constants** — Move per-language constant tables from Rust match arms to config files. Big refactor with regression risk.

## Review Notes

Detailed review notes (dogfood sessions, model comparisons, performance benchmarks) are in git history. Key sessions:
- 2026-03-21: LabHandbook embedding model A/B test (Jina-Code-v2 vs BGE-small)
- 2026-03-21: Embedding enrichment dogfood (field access, callee names, doc excerpts)
- 2026-03-15-18: GPT review fixes, codehealth-driven test coverage (96 new tests)
