# Blast Radius and Task Context Design

**Date:** 2026-04-20
**Status:** Design, ready for implementation planning

## Summary

Build a V1 impact-aware context system for Julie with four linked changes:

1. add a new `blast_radius` MCP tool for change impact and likely test selection
2. extend `get_context` with task-shaped inputs such as changed files, entry symbols, stack traces, and failing tests
3. rename static `test_coverage` linkage to `test_linkage` and stop presenting it as runtime coverage
4. add spillover handles for graph-heavy outputs so large result sets stay bounded without losing follow-up access

This design steals the useful SDL ideas and leaves the cargo cult pieces on the floor. The goal is better agent decisions per token, not a flashy new graph subsystem.

## Goals

1. Give agents a first-class way to answer, "what else is affected by this change?"
2. Make `get_context` stronger for real task inputs without turning it into a workflow executor.
3. Replace misleading "coverage" language with honest static linkage language.
4. Keep default outputs readable and token-bounded.
5. Stay language agnostic across ranking, test discovery, and stack-trace handling.

## Non-goals

- No SDL-style workflow mode or MCP-inside-MCP orchestration.
- No repo-wide coverage percentage from static linkage.
- No codehealth-derived ranking in V1.
- No symbol-history diff engine in V1.
- No compact wire format as the default output style across Julie.
- No model-based ranking or learned agent loop in V1.

## Why This Project Exists

Julie already has a strong coding loop:

1. `fast_search` for code discovery
2. `get_symbols` for file structure
3. `deep_dive` for symbol understanding
4. `edit_file` and `rewrite_symbol` for mutation

The missing piece is impact awareness.

After an edit, agents still have to infer:

- which callers or dependents matter
- which tests are worth reading or running
- whether a failure trace points at the real code path
- which neighboring symbols deserve the next token spend

SDL's best idea is not its workflow layer. It is task-shaped retrieval plus bounded graph packaging. Julie should copy that value and skip the machinery.

## Research Findings That Drive This Design

### 1. Julie already has the graph Julie needs

- `relationships` and `identifiers` already support dependency walking.
- `reference_score` already exists and is a strong language-agnostic ranking signal.
- `deep_dive` already computes test refs and relationship context.

Relevant code:

- [src/database/relationships.rs](/Users/murphy/source/julie/src/database/relationships.rs:459)
- [src/tools/get_context/pipeline.rs](/Users/murphy/source/julie/src/tools/get_context/pipeline.rs:155)
- [src/tools/deep_dive/data.rs](/Users/murphy/source/julie/src/tools/deep_dive/data.rs:323)

### 2. Julie has revision markers, but not usable revision deltas

Canonical revisions exist today, but they are count-level snapshots, not a stored map of changed files or changed symbols between revisions.

Relevant code:

- [src/database/revisions.rs](/Users/murphy/source/julie/src/database/revisions.rs:37)

### 3. Current symbol IDs are not strong enough for revision diff semantics

Symbol IDs are generated from `file_path:name:start_line:start_column`. That is fine for live indexing, but line churn makes it weak as the sole identity for cross-revision diffing.

Relevant code:

- [crates/julie-extractors/src/base/extractor.rs](/Users/murphy/source/julie/crates/julie-extractors/src/base/extractor.rs:199)
- [crates/julie-extractors/src/base/types.rs](/Users/murphy/source/julie/crates/julie-extractors/src/base/types.rs:95)

This pushes V1 toward file-based revision deltas, not symbol-history deltas.

### 4. Julie already has strong test metadata

Julie has two different test systems:

- extractor-level `is_test` metadata across languages
- static test-to-production linkage in `analysis/test_coverage.rs`

That means V1 does not need path heuristics as the primary test signal.

Relevant code:

- [crates/julie-extractors/src/test_detection.rs](/Users/murphy/source/julie/crates/julie-extractors/src/test_detection.rs:1)
- [src/analysis/test_quality.rs](/Users/murphy/source/julie/src/analysis/test_quality.rs:415)
- [src/analysis/test_coverage.rs](/Users/murphy/source/julie/src/analysis/test_coverage.rs:46)

### 5. The old codehealth framing was wrong

The static linkage pass can answer "which tests seem linked to this code?" It cannot answer "what percent of the repo has runtime coverage?"

So V1 must rename the feature and stop emitting fake coverage semantics.

## Design Rules

1. **Readable first.** Default responses must be scan-friendly without decoding a schema legend.
2. **Deterministic first.** Rank with graph structure, reference score, and direct evidence. Do not rank with noisy codehealth guesses.
3. **Language agnostic by default.** No `src/`, `Cargo.toml`, `mod.rs`, or Rust-only stack assumptions in core logic.
4. **Bounded outputs.** Large graph outputs must truncate through spillover, not by dumping giant blobs.
5. **Hints, not orchestration.** Tools may suggest the next call. They should not absorb half the coding loop into one response.

## V1 Product Decisions

### 1. Add `blast_radius`

`blast_radius` is a new read-only MCP tool that answers:

- what symbols are impacted by this change
- why they are impacted
- which tests are likely relevant
- what remains available beyond the first bounded response

It must support three seed modes:

- explicit `symbol_ids`
- explicit `file_paths`
- revision range via `from_revision` and `to_revision`

Revision-range mode resolves changed files first, then seeds impact from current symbols in those files.

### 2. Extend `get_context`

`get_context` stays in place and keeps its current text-first output. V1 extends it with task inputs:

- `edited_files`
- `entry_symbols`
- `stack_trace`
- `failing_test`
- `max_hops`
- `prefer_tests`

The existing `query` field remains required. Task inputs bias pivot selection and decide whether a bounded second hop is warranted.

### 3. Rename `test_coverage` to `test_linkage`

V1 renames the concept, metadata key, analysis module, comments, formatting, and tests from `test_coverage` to `test_linkage`.

`test_linkage` means:

- linked test names
- best and worst quality tier among linked tests
- count of linked tests
- evidence that these are statically linked tests, not measured runtime coverage

It does **not** mean line coverage, branch coverage, or repo-wide percentages.

### 4. Ship spillover in V1

Large impact results and graph-heavy task context must return:

- a bounded primary result
- a `spillover_handle`
- a follow-up retrieval path for more items

V1 should not make users rerun the same graph walk to fetch the next slice of data.

## Architecture

### A. New Tool Surface

### `blast_radius` request shape

```json
{
  "symbol_ids": ["abc123"],
  "file_paths": ["src/tools/get_context/pipeline.rs"],
  "from_revision": null,
  "to_revision": null,
  "max_depth": 2,
  "limit": 12,
  "include_tests": true,
  "format": "readable",
  "workspace": "primary"
}
```

Rules:

- At least one seed mode must be present.
- `from_revision` and `to_revision` must be used together.
- `max_depth` defaults to `2`.
- `limit` defaults to a small bounded set such as `12`.
- `format` defaults to `"readable"`.

### `blast_radius` response shape

Readable format should be the default:

```text
Blast radius from 3 changed files, 9 seed symbols

High impact
1. run_pipeline  src/tools/get_context/pipeline.rs:155
   why: direct caller, 1 hop, centrality=high

2. GetContextTool  src/tools/get_context/mod.rs:28
   why: public entry point reaches changed code in 1 hop

Likely tests
- src/tests/tools/get_context_pipeline_tests.rs
- src/tests/tools/get_context_graph_expansion_tests.rs

More available: spillover_handle=br_...
```

Structured data still matters for internal plumbing, but the outward default should stay text-first.

### `get_context` extended request shape

```json
{
  "query": "failing auth flow after token refresh",
  "edited_files": ["src/auth/service.rs"],
  "entry_symbols": ["AuthService::refresh"],
  "stack_trace": "src/api/auth.rs:41\nsrc/auth/service.rs:88",
  "failing_test": "tests/auth_refresh_tests.rs",
  "max_hops": 2,
  "prefer_tests": false,
  "workspace": "primary",
  "format": "compact"
}
```

Behavior rules:

- old `get_context` calls keep working with current semantics
- supplemental task inputs act as ranking and expansion hints
- one hop remains the default
- hop 2 is only used when `max_hops >= 2` and the first hop under-covers the task

### `spillover_get` request shape

V1 should add a small shared follow-up tool for paged graph results:

```json
{
  "spillover_handle": "br_01HV...",
  "limit": 10,
  "format": "readable"
}
```

Rules:

- `spillover_handle` is required
- `limit` defaults to a bounded page size such as `10`
- `format` defaults to the format stored with the handle

This avoids cramming paging semantics into `blast_radius` or `get_context` request schemas.

### B. Storage and Data Flow

### 1. Add file-based revision delta storage

V1 should add a new database module and table for changed files per canonical revision.

Planned module:

- `src/database/revision_changes.rs`

Planned table:

```sql
CREATE TABLE revision_file_changes (
    revision INTEGER NOT NULL,
    workspace_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    change_kind TEXT NOT NULL CHECK(change_kind IN ('added', 'modified', 'deleted')),
    old_hash TEXT,
    new_hash TEXT,
    PRIMARY KEY (revision, workspace_id, file_path)
)
```

This table is the V1 backbone for:

- `blast_radius(from_revision, to_revision)`
- future "what changed?" tooling
- dashboard or report views that want revision-aware file deltas

### 2. Record revision file changes in the indexing pipeline

The indexing pipeline already knows which files were added, changed, and cleaned. V1 should persist those file changes when a canonical revision is recorded.

Relevant code:

- [src/tools/workspace/indexing/pipeline.rs](/Users/murphy/source/julie/src/tools/workspace/indexing/pipeline.rs:354)
- [src/database/bulk_operations.rs](/Users/murphy/source/julie/src/database/bulk_operations.rs:936)

### 3. Keep spillover in memory for V1

V1 spillover handles should live in process memory, scoped to the active server process and session lifetime.

Reasons:

- lower implementation cost
- no schema churn for paged graph payload storage
- acceptable failure mode if the process restarts and a handle dies

Planned shape:

- `src/tools/shared/spillover.rs`

Features:

- random handle ID
- TTL eviction
- per-session ownership
- bounded entry count

`blast_radius` and `get_context` should both be able to materialize spillover entries into this shared store, and `spillover_get` should page them back out.

### C. Ranking and Expansion

### `blast_radius` ranking signals

V1 ranking should use only deterministic structural signals:

1. graph distance from the seed
2. relationship kind
3. `reference_score`
4. visibility when present
5. direct linked-test evidence

Relationship priority should be explicit:

- `calls`
- `overrides`
- `implements`
- `instantiates`
- `references`
- `imports`

Do not include:

- change risk
- security risk
- sink heuristics
- churn
- clustering

Those can come back later if they earn trust.

### `blast_radius` likely-test selection

Likely tests should use this order:

1. `metadata.test_linkage.linked_tests`
2. direct graph edges from test symbols where `metadata.is_test = true`
3. identifier linkage from test-containing symbols
4. path heuristics as fallback

This keeps the feature anchored in real Julie metadata instead of file-name folklore.

### `get_context` task-shaped scoring

Task inputs should bias current pivot scoring, not supplant it.

Additional boosts:

- symbols in `edited_files`
- explicit `entry_symbols`
- file and line hits extracted from `stack_trace`
- symbols linked to the named `failing_test`

Second-hop expansion rules:

- only if `max_hops >= 2`
- only if first-hop pivots and neighbors do not produce enough code-bearing context
- second hop should prefer code symbols over docs, fixtures, and low-value structure

`get_context` remains a bounded orientation tool, not a full graph explorer.

### D. Test Linkage Rename

### Rename scope

V1 should rename:

- module name
- comments and docs
- metadata key
- tests
- formatting and output labels

From:

- `test_coverage`

To:

- `test_linkage`

Planned metadata shape:

```json
{
  "test_linkage": {
    "test_count": 4,
    "best_tier": "thorough",
    "worst_tier": "thin",
    "linked_tests": ["test_refresh_happy_path", "test_refresh_expired_token"],
    "evidence_sources": ["relationship", "resolved_identifier"]
  }
}
```

### Temporary compatibility rule

Readers should check `test_linkage` first, then fall back to `test_coverage` for old indexes until a reindex overwrites metadata under the new key.

This avoids stale-index breakage during rollout without treating the old name as the long-term public contract.

### Required pipeline wiring

The linkage pass exists today, but it is not wired into the post-index analysis flow.

V1 must wire it into:

- [src/tools/workspace/indexing/pipeline.rs](/Users/murphy/source/julie/src/tools/workspace/indexing/pipeline.rs:606)

Desired analysis order:

1. `compute_reference_scores`
2. `compute_test_quality_metrics`
3. `compute_test_linkage`

No codehealth ranking pass should join this hot path in V1.

### E. Language-Agnostic Constraints

The implementation must stay language agnostic end to end.

Hard rules:

- no `path.starts_with("src/")`
- no Rust-only stack-trace parser in the core path
- no `Cargo.toml`, `mod.rs`, `lib.rs`, or `main.rs` checks
- no ranking rule that privileges Rust constructs
- no assumption that visibility exists for all languages

Good generic signals:

- graph relationships
- `reference_score`
- `metadata.is_test`
- `metadata.test_linkage`
- extractor-provided visibility when available
- generic file:line extraction from stack traces

## Primary Code Areas

### Existing files to modify

- [src/tools/get_context/mod.rs](/Users/murphy/source/julie/src/tools/get_context/mod.rs:28)
- [src/tools/get_context/pipeline.rs](/Users/murphy/source/julie/src/tools/get_context/pipeline.rs:155)
- [src/tools/get_context/scoring.rs](/Users/murphy/source/julie/src/tools/get_context/scoring.rs:1)
- [src/handler.rs](/Users/murphy/source/julie/src/handler.rs:2586)
- [src/handler/tool_targets.rs](/Users/murphy/source/julie/src/handler/tool_targets.rs:1)
- [src/tools/mod.rs](/Users/murphy/source/julie/src/tools/mod.rs:1)
- [src/tools/metrics/session.rs](/Users/murphy/source/julie/src/tools/metrics/session.rs:23)
- [src/database/revisions.rs](/Users/murphy/source/julie/src/database/revisions.rs:37)
- [src/tools/workspace/indexing/pipeline.rs](/Users/murphy/source/julie/src/tools/workspace/indexing/pipeline.rs:606)
- [src/tools/deep_dive/data.rs](/Users/murphy/source/julie/src/tools/deep_dive/data.rs:323)
- [src/analysis/test_coverage.rs](/Users/murphy/source/julie/src/analysis/test_coverage.rs:46)
- [src/analysis/mod.rs](/Users/murphy/source/julie/src/analysis/mod.rs:1)

### New files and modules

- `src/tools/impact/mod.rs`
- `src/tools/impact/seed.rs`
- `src/tools/impact/walk.rs`
- `src/tools/impact/ranking.rs`
- `src/tools/impact/formatting.rs`
- `src/tools/shared/spillover.rs`
- `src/database/revision_changes.rs`
- `src/tests/tools/blast_radius_tests.rs`
- `src/tests/tools/blast_radius_formatting_tests.rs`
- `src/tests/tools/get_context_task_inputs_tests.rs`
- `src/tests/core/revision_changes.rs`

This split keeps new implementation files under the project size limit.

## Risks and Mitigations

### 1. Revision deltas are file-level, not symbol-level

Risk:

- `blast_radius(from_revision, to_revision)` may seed more symbols than changed in a file

Mitigation:

- make that behavior explicit in docs and output
- keep V1 ranking tight so high-value impacted symbols rise to the top
- revisit symbol-history identity in a later phase only if V1 proves useful

### 2. Stack-trace parsing varies by language and harness

Risk:

- narrow parser support causes missed boosts

Mitigation:

- V1 only requires generic file:line extraction plus simple symbol-name scraping
- language-specific parsers can be small, optional helpers later

### 3. Spillover handles can leak memory

Risk:

- graph-heavy sessions hold too many result sets

Mitigation:

- TTL eviction
- bounded count
- per-session scoping

### 4. Old `test_coverage` readers may break on partially reindexed workspaces

Risk:

- mixed metadata during rollout

Mitigation:

- reader fallback from `test_linkage` to `test_coverage`
- migration note in release docs

## Acceptance Criteria

- [ ] `blast_radius` is registered as a new read-only MCP tool.
- [ ] `blast_radius` accepts seed symbols, seed files, and revision ranges.
- [ ] revision file changes are persisted per canonical revision.
- [ ] `blast_radius` ranks with deterministic graph signals only.
- [ ] `blast_radius` returns likely tests from Julie test metadata before path heuristics.
- [ ] `blast_radius` supports spillover handles for truncated result sets.
- [ ] `get_context` accepts task-shaped inputs and preserves old behavior when they are absent.
- [ ] `get_context` can take a bounded second hop when task inputs and weak first-hop coverage warrant it.
- [ ] `spillover_get` can page stored graph-heavy results without rerunning the underlying graph walk.
- [ ] static `test_coverage` naming is replaced with `test_linkage` in user-facing and internal design surfaces for this feature.
- [ ] no repo-wide percentage is emitted from static test linkage.
- [ ] core ranking and test selection stay language agnostic.
- [ ] new tests cover revision storage, blast radius ranking, spillover, task-shaped context, and test-linkage fallback behavior.

## Recommended Implementation Order

1. add revision file-change storage and tests
2. rename `test_coverage` to `test_linkage`, wire the linkage pass into indexing, and update readers
3. build shared spillover store
4. implement `blast_radius`
5. extend `get_context` with task-shaped inputs and bounded second-hop rules
6. update `deep_dive` test-ref selection to prefer test metadata over path-only filtering

## Open Questions Resolved In This Design

### Should V1 include both `blast_radius` and `get_context`?

Yes. They solve adjacent agent problems and share enough graph and formatting concerns that shipping them together is coherent.

### Should V1 rename `test_coverage`?

Yes. The old name is misleading and encourages fake percentage thinking.

### Should V1 ship spillover now or later?

Now. The first graph-heavy tools should not land with truncation cliffs.

### Should V1 depend on codehealth scoring?

No. The prior codehealth outputs were noisy, and the impact tool should earn trust through deterministic structure first.
