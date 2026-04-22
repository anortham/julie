# Fast Search File Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Add a first-class `fast_search` file target with canonical `search_target="files"` behavior, `paths` alias support, Tantivy-backed indexed file lookup, and token-lean file-shaped results.

**Architecture:** Keep `fast_search` as one tool with three explicit targets: `content`, `definitions`, and `files`. Implement `files` as a separate retrieval lane, not a content-search hack: new target parsing, new Tantivy file-path fields and search method, projection coverage for all indexed file rows, a file-specific execution/result shape, and telemetry plus matrix updates. Execute this plan in the worktree at `/Users/murphy/source/julie/.worktrees/fast-search-files`; do not touch the unrelated dirty `src/tools/search/line_mode.rs` in the main checkout.

**Tech Stack:** Rust, Tantivy, SQLite, serde/schemars, Julie xtask search-matrix harness

---

**TDD rule for every task:** start with the narrowest failing test, make it pass with the smallest coherent code change, then run `cargo xtask test changed` after each task. Because this feature changes search retrieval and ranking, finish the full batch with `cargo xtask test dogfood` and `cargo xtask test dev`.

### Task 1: Add explicit search-target parsing and route `files` through its own lane

**Files:**
- Create: `src/tools/search/target.rs`
- Modify: `src/tools/search/mod.rs`
- Modify: `src/tools/search/execution.rs`
- Modify: `src/tools/search/text_search.rs`
- Modify: `src/handler/search_telemetry.rs`
- Modify: `src/tests/tools/search/mod.rs`
- Create: `src/tests/tools/search/file_mode_tests.rs`
- Modify: `src/tests/core/handler_telemetry.rs`

**What to build:** Replace the raw two-mode `search_target` split with an explicit `SearchTarget` enum that supports `content`, `definitions`, and canonical `files`, with `paths` accepted as a serde alias. Reject unknown targets instead of falling through, reject `context_lines` for file mode, and ensure telemetry records canonical `files` plus a distinct `file_lookup` intent bucket.

**Approach:** Keep parsing logic near `FastSearchTool`, but move the enum and serde/alias handling into `src/tools/search/target.rs` so `mod.rs` does not swell further. Update `execute_with_trace`, `execution::execute_search`, and `text_search_impl` to branch on the enum, not string comparisons. Do not route file mode through the content verifier. Extend telemetry normalization and intent inference so `paths` never appears in stored metadata and path-shaped queries can be classified separately from `content_grep`.

**Acceptance criteria:**
- [ ] `FastSearchTool` accepts `search_target="files"` and `search_target="paths"` and normalizes both to `SearchTarget::Files`.
- [ ] Unknown `search_target` values fail fast with a validation error.
- [ ] `context_lines` is rejected for file mode.
- [ ] Telemetry writes canonical `search_target="files"` and `intent="file_lookup"` for file-mode calls.
- [ ] Narrow file-mode routing and telemetry tests pass, then `cargo xtask test changed` passes.

### Task 2: Extend the Tantivy file schema and cover all indexed file rows

**Files:**
- Modify: `src/search/schema.rs`
- Modify: `src/search/query.rs`
- Modify: `src/search/index.rs`
- Modify: `src/search/projection.rs`
- Modify: `src/database/files.rs`
- Modify: `src/tests/tools/search/mod.rs`
- Create: `src/tests/tools/search/file_mode_index_tests.rs`
- Modify: `src/tests/integration/projection_repair.rs`

**What to build:** Add `basename` and `path_text` fields to file documents, bump the search compat marker, add `SearchIndex::search_files`, and change Tantivy projection input so file docs exist for every indexed file row, not only rows where `content IS NOT NULL`.

**Approach:** Keep `file_path` as the exact full-path field used for delete-by-path and exact-path hits. Add `basename` as an exact string field and `path_text` as code-tokenized text for fragments and component matches. Build a file query path that supports exact relative path, exact basename, path fragment, and glob-like queries using candidate retrieval plus post-filtering with the existing glob matcher. Do not add a SQLite schema migration, because the `files` table already stores `path` and `language`; instead, add a broader file-row read path in `src/database/files.rs` and use it in `src/search/projection.rs`.

**Acceptance criteria:**
- [ ] Tantivy schema exposes `basename` and `path_text` for file docs and forces stale-index recreation through the compat marker bump.
- [ ] `SearchIndex::search_files` returns file hits for exact basename, exact path, fragment, and glob-like queries.
- [ ] File docs are projected for indexed file rows even when `content` is absent.
- [ ] New index/projection regression tests cover ranking, stale-index recreation, and content-less file coverage.
- [ ] Narrow file-index tests pass, then `cargo xtask test changed` passes.

### Task 3: Add file-shaped execution, ranking, and lean output formatting

**Files:**
- Modify: `src/tools/search/trace.rs`
- Modify: `src/tools/search/execution.rs`
- Modify: `src/tools/search/formatting.rs`
- Modify: `src/tools/search/text_search.rs`
- Modify: `src/search/scoring.rs`
- Modify: `src/tests/tools/search/file_mode_tests.rs`
- Modify: `src/tests/tools/search/lean_format_tests.rs`

**What to build:** Add a file-hit backing type and `SearchExecutionKind::Files`, then expose token-lean file results for both `return_format="locations"` and `return_format="full"`, with deterministic ranking that prefers exact path and basename hits and demotes tests/docs/fixtures with language-agnostic heuristics.

**Approach:** Do not reuse the content-mode `Symbol` synthesis path. File hits should serialize with `line=None`, `kind="file"`, and the relative Unix-style path as the primary output. Add a file-mode formatter that keeps output to one header plus one result line per file, with terse `(language, match_kind, test)` annotations in `full` mode only. Use generic heuristics from `src/search/scoring.rs` for test/docs/fixture demotion; do not pull in `src/utils/path_relevance.rs`, because its `src` and `lib` assumptions violate Julie’s language-agnostic rules.

**Acceptance criteria:**
- [ ] File execution returns file-shaped hits, not content-mode `Symbol` stand-ins.
- [ ] `locations` output is one relative path per line, no snippets, no line numbers.
- [ ] `full` output stays lean and includes terse disambiguation annotations only.
- [ ] Ranking prefers exact relative path, then exact basename, then suffix/component matches, with generic demotion for tests/docs/fixtures.
- [ ] Narrow execution and formatting tests pass, then `cargo xtask test changed` passes.

### Task 4: Wire file mode into telemetry consumers and the search-matrix harness

**Files:**
- Modify: `src/handler/search_telemetry.rs`
- Modify: `xtask/src/search_matrix.rs`
- Modify: `fixtures/search-quality/search-matrix-cases.toml`
- Modify: `xtask/tests/search_matrix_contract_tests.rs`
- Modify: `src/tests/core/handler_telemetry.rs`

**What to build:** Make telemetry and matrix runs understand the new canonical `files` target, add starter matrix cases for file lookup, and preserve token-lean top-hit reporting for file hits.

**Approach:** Extend matrix case handling so `search_target="files"` is treated as a normal first-class target. Seed the harness with a small set of file-mode cases that exercise basename, exact path, fragment, and glob behavior across multiple repos. Keep top-hit reporting compact, reusing the existing `file`, `name`, `line`, `kind`, and `score` shape so matrix reports do not need a parallel report format.

**Acceptance criteria:**
- [ ] Telemetry serialization tests cover canonical `files` metadata and file-hit trace output.
- [ ] Search-matrix fixtures include smoke and breadth file-mode cases.
- [ ] Matrix contract tests cover the new fixture shape.
- [ ] Narrow telemetry and matrix tests pass, then `cargo xtask test changed` passes.

### Task 5: Run the batch validation and produce a fresh matrix baseline

**Files:**
- Output: `artifacts/search-matrix/smoke-*.json`
- Output: `artifacts/search-matrix/smoke-*.md`
- Output: `artifacts/search-matrix/breadth-*.json`
- Output: `artifacts/search-matrix/breadth-*.md`

**What to build:** Prove the feature works end to end and does not regress the existing search stack.

**Approach:** After Tasks 1 through 4 are green, run the calibrated repo gates in this order: `cargo xtask test changed`, `cargo xtask test dogfood`, `cargo xtask test dev`. Then run `cargo xtask search-matrix baseline --profile smoke ...` and `cargo xtask search-matrix baseline --profile breadth ...` from the worktree and inspect the file-mode cases for sane hit counts, sane ordering, and no new bogus zero-hit clusters.

**Acceptance criteria:**
- [ ] `cargo xtask test changed` passes after the final integration batch.
- [ ] `cargo xtask test dogfood` passes.
- [ ] `cargo xtask test dev` passes.
- [ ] Smoke and breadth matrix reports are regenerated with file-mode cases and no new unexplained regression cluster.
- [ ] The branch is ready for external review with commits separated by concern where practical.
