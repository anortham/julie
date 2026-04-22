# Search Matrix Investigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Build `cargo xtask search-matrix {mine,baseline}` so Julie can mine live `fast_search` telemetry into seed reports and run a curated cross-repo search matrix from committed fixtures.

**Architecture:** Extend xtask with a top-level `search-matrix` command family instead of overloading the existing `test` parser. The `mine` path reads Julie daemon telemetry from `DaemonDatabase`, clusters high-signal `fast_search` rows into seed candidates, and writes JSON plus Markdown artifacts. The `baseline` path loads committed TOML case and corpus manifests, resolves repos from local source roots, reuses Julie's in-process daemon workspace stack (`WorkspacePool`, `JulieServerHandler::new_with_shared_workspace`, `execute_search`) to run searches, and emits grouped result reports without claiming cross-tool recourse.

**Tech Stack:** Rust, xtask, julie library crate, `DaemonDatabase`, `WorkspacePool`, `JulieServerHandler`, `execute_search`, `serde`, `serde_json`, `toml`, `tokio`

---

### Task 1: Xtask command surface and fixture contracts

**Files:**
- Modify: `xtask/src/cli.rs:7-145`
- Modify: `xtask/src/main.rs:16-120`
- Modify: `xtask/src/lib.rs:1-30`
- Modify: `xtask/Cargo.toml:9-12`
- Create: `fixtures/search-quality/search-matrix-cases.toml`
- Create: `fixtures/search-quality/search-matrix-corpus.toml`
- Test: `xtask/tests/search_matrix_contract_tests.rs`

**What to build:** Introduce a top-level xtask command enum so `cargo xtask test ...` keeps its current behavior while `cargo xtask search-matrix mine ...` and `cargo xtask search-matrix baseline ...` parse cleanly. Add the committed starter case and corpus fixtures with the curated query families, profile tags, and repo selections from the approved design.

**Approach:** Keep the existing test runner flow intact by wrapping `TestCommand` inside a broader command parser, not by shoving search-matrix flags into the current `TestCommand`. Define small deserializable structs for case and corpus fixtures first, then pin them with contract tests before any execution code lands.

**Acceptance criteria:**
- [ ] `cargo xtask search-matrix mine --days 7 --out artifacts/search-matrix/seeds.json` parses into a dedicated command shape.
- [ ] `cargo xtask search-matrix baseline --profile smoke --out artifacts/search-matrix/baseline.json` parses into a dedicated command shape.
- [ ] Bad subcommands and bad flag combinations fail with crisp parser errors.
- [ ] The committed case and corpus TOML fixtures deserialize and expose the expected profiles, query families, and repo selectors.

### Task 2: Mining stage and report generation

**Files:**
- Create: `xtask/src/search_matrix.rs`
- Create: `xtask/src/search_matrix_mine.rs`
- Create: `xtask/src/search_matrix_report.rs`
- Modify: `xtask/src/main.rs:16-120`
- Modify: `xtask/src/lib.rs:1-30`
- Test: `xtask/tests/search_matrix_contract_tests.rs`

**What to build:** Implement the `mine` command so it opens the daemon registry through `DaemonPaths`, reads recent tool-call history with `DaemonDatabase::list_tool_calls_for_search_analysis`, filters to `fast_search`, normalizes each row into a seed candidate, clusters candidates by query family and failure bucket, and writes both JSON and Markdown reports.

**Approach:** Reuse Julie's existing metadata and trace fields instead of inventing a parallel telemetry schema. Mining output should record only what Julie knows: normalized query shape, target, filters, hit counts, trace diagnostics, and example rows. It must not infer downstream recovery from missing non-Julie data.

**Acceptance criteria:**
- [ ] Mining ignores non-`fast_search` tool rows.
- [ ] Seed candidates preserve `zero_hit_reason`, `file_pattern_diagnostic`, `hint_kind`, and `relaxed` when present.
- [ ] Clusters group candidates by query family plus named failure bucket, not raw query string alone.
- [ ] The command writes JSON and Markdown artifacts to the requested output path.
- [ ] Contract tests cover temp daemon-db input and grouped report output shape.

### Task 3: Baseline runner over committed matrix and corpus

**Files:**
- Modify: `xtask/src/search_matrix.rs`
- Modify: `xtask/src/search_matrix_report.rs`
- Modify: `xtask/src/main.rs:16-120`
- Modify: `xtask/Cargo.toml:9-12`
- Test: `xtask/tests/search_matrix_contract_tests.rs`

**What to build:** Implement the `baseline` command so it loads committed matrix and corpus fixtures, resolves repo names against local roots such as `~/source`, matches those repos to ready daemon workspaces, reuses Julie's in-process daemon search stack to execute cases, and records per-case plus per-repo outcomes with grouped summaries and promotion signals.

**Approach:** Keep version 1 strict about pre-indexed repos. Use `DaemonDatabase::list_workspaces` plus the repo manifest to resolve ready workspaces, then build an in-process execution path around `WorkspacePool::new`, `WorkspacePool::get_or_init`, `JulieServerHandler::new_with_shared_workspace`, and `execute_search`. Separate raw execution records from summary rendering so future dogfood promotion can consume the same baseline output.

**Acceptance criteria:**
- [ ] `baseline --profile smoke` resolves committed repos from local source roots and reports missing or unindexed repos without auto-indexing them.
- [ ] The runner filters repos by profile, repo selector, language family, and case tags.
- [ ] Each execution record stores hit count, top hits, latency, `zero_hit_reason`, `file_pattern_diagnostic`, `hint_kind`, and `relaxed`.
- [ ] Summary flags include `cross_repo_zero_hit`, `unattributed_zero_hit`, `line_match_miss_cluster`, `scoped_no_in_scope_cluster`, and `unexpected_hint`.
- [ ] Async contract tests cover one ready indexed temp workspace and one skipped unindexed workspace.

### Task 4: Docs and operator workflow

**Files:**
- Modify: `docs/TESTING_GUIDE.md:40-72`

**What to build:** Document the new search-matrix commands, the pre-indexed corpus requirement, output locations under `artifacts/search-matrix/`, and how the matrix harness relates to the existing dogfood and regression workflow.

**Approach:** Keep the operator story blunt and short. This harness is investigative infrastructure, not a new mandatory xtask tier and not a proxy for cross-tool recourse.

**Acceptance criteria:**
- [ ] `docs/TESTING_GUIDE.md` documents the `mine` and `baseline` command shapes.
- [ ] The guide calls out the pre-indexed corpus requirement and the `artifacts/search-matrix/` output directory.
- [ ] The guide states that the matrix harness complements, not replaces, `cargo xtask test dogfood`.

## Verification

- Narrow RED/GREEN loops:
  - `cargo nextest run -p xtask search_matrix_contract_tests`
  - targeted `cargo nextest run -p xtask cli_tests_*` or exact xtask contract test names as added
- Batch gate after implementation:
  - `cargo xtask test changed`
- Search-quality regression gate after baseline runner lands:
  - `cargo xtask test dogfood`
