# Blast Radius And Task Context Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use @razorback:subagent-driven-development to implement this plan. Fall back to @razorback:executing-plans if the work collapses into one tightly sequential worker.

**Goal:** Add `blast_radius`, task-shaped `get_context`, `spillover_get`, revision file deltas, and `test_linkage` so Julie can answer impact and likely-test questions with bounded token spend.

**Architecture:** Build the foundation first: persist revision file changes, rename static linkage metadata, and add shared spillover state. Then add `blast_radius` and extend `get_context` on top of that foundation. Keep ranking deterministic, keep outputs text-first, and keep the core path language agnostic.

**Tech Stack:** Rust, rmcp, rusqlite, serde, schemars, Julie graph storage, `cargo nextest`, `cargo xtask`

**Design Spec:** `docs/plans/2026-04-20-blast-radius-and-task-context-design.md`

---

## Execution Notes

- Use @razorback:test-driven-development and @razorback:verification-before-completion.
- This is a light plan for same-session execution. Run narrow tests inside each task, then `cargo xtask test changed`, then `cargo xtask test dev` once after the batch.
- Keep `src/tools/get_context/pipeline.rs` on a leash. If task-input parsing or second-hop logic starts inflating that file again, move the new logic into helpers instead of feeding the blob.
- Keep spillover in a dedicated `src/tools/spillover/` module. Do not refactor `src/tools/shared.rs` into a directory unless the code forces that move.
- No codehealth ranking, no repo-wide static “coverage %”, no `src/` heuristics, and no Rust-only stack-trace parsing.

### Task 1: Persist Revision Deltas And Rename Test Linkage

**Files:**
- Create: `src/database/revision_changes.rs`
- Create: `src/analysis/test_linkage.rs`
- Create: `src/tests/core/revision_changes.rs`
- Create: `src/tests/analysis/test_linkage_tests.rs`
- Delete: `src/analysis/test_coverage.rs`
- Delete: `src/tests/analysis/test_coverage_tests.rs`
- Modify: `src/database/mod.rs`
- Modify: `src/database/schema.rs:9-31`
- Modify: `src/database/migrations.rs:15-147`, `src/database/migrations.rs:812-870`
- Modify: `src/database/revisions.rs:1-207`
- Modify: `src/database/bulk_operations.rs:597-986`, `src/database/bulk_operations.rs:993-1418`
- Modify: `src/database/workspace.rs:1-140`
- Modify: `src/tools/workspace/indexing/pipeline.rs:347-463`, `src/tools/workspace/indexing/pipeline.rs:604-652`
- Modify: `src/tools/deep_dive/data.rs:323-392`
- Modify: `src/tools/metrics/query.rs:1-292`
- Modify: `src/analysis/mod.rs:1-9`
- Modify: `src/analysis/change_risk.rs`
- Modify: `src/analysis/security_risk.rs`
- Modify: `src/tests/analysis/mod.rs`
- Modify: `src/tests/core/incremental_update_atomic.rs`
- Modify: `src/tests/mod.rs`

**What to build:** Add a `revision_file_changes` table plus query helpers, then rename static coverage metadata to `test_linkage` and wire that analysis into the post-index pipeline. The revision side must record added, modified, and deleted file rows in the same transaction as the canonical revision. The linkage side must stop calling static linkage “coverage”, keep old indexed workspaces readable through a fallback read path, and improve `deep_dive` test refs so they prefer real test metadata over path folklore.

**Approach:** Add a forward migration for `revision_file_changes` and create the table during fresh schema init. In `incremental_update_atomic`, snapshot existing file hashes before deletion so the writer can tell `added` from `modified`; in `delete_orphaned_files_atomic`, record delete-only revision rows instead of losing that history. Move `compute_test_coverage` into `test_linkage.rs` as `compute_test_linkage`, store `linked_tests` plus `evidence_sources`, and add one small reader helper that checks `test_linkage` first and `test_coverage` second for rollout compatibility. Wire `compute_test_linkage` into `analyze_batch` after `compute_test_quality_metrics`, and update touched readers such as `tools/metrics/query.rs`, `change_risk.rs`, `security_risk.rs`, and `deep_dive/data.rs` to consume the new shape without reviving codehealth ranking.

**Acceptance criteria:**
- [ ] Fresh databases and migrated databases both create `revision_file_changes`.
- [ ] Canonical revision writes record file deltas in the same transaction as the revision row.
- [ ] Deleted-file revision rows exist for orphan cleanup paths, not only mixed incremental writes.
- [ ] `compute_test_linkage` runs in post-index analysis after test quality.
- [ ] Touched readers prefer `test_linkage` and fall back to `test_coverage` for stale indexes.
- [ ] `deep_dive` test refs stop relying on path-only detection when symbol metadata is present.
- [ ] Narrow database, analysis, and indexing tests pass, committed.

### Task 2: Add Spillover Store And Follow-Up Tool

**Files:**
- Create: `src/tools/spillover/mod.rs`
- Create: `src/tools/spillover/store.rs`
- Create: `src/tests/tools/spillover_tests.rs`
- Modify: `src/tools/mod.rs:1-30`
- Modify: `src/handler.rs:342-384`, `src/handler.rs:748-774`, `src/handler.rs:2384-2837`
- Modify: `src/handler/tool_targets.rs:1-90`
- Modify: `src/tools/metrics/session.rs:1-173`
- Modify: `src/tests/core/handler.rs`
- Modify: `src/tests/tools/metrics/session_metrics_tests.rs`
- Modify: `src/tests/mod.rs`

**What to build:** Add an in-memory spillover store owned by the handler and expose a public `spillover_get` tool that pages stored results without rerunning the underlying graph walk. The store must enforce TTL, bounded entry count, and session ownership so handles do not leak across sessions or live forever.

**Approach:** Put spillover storage in `src/tools/spillover/store.rs` and keep the tool surface in `src/tools/spillover/mod.rs`. Add a `spillover_store` field to `JulieServerHandler` and initialize it in all constructor paths. The store payload can stay narrow: opaque handle, default format, next-page cursor, and a typed page payload for graph rows. No generic query replay engine, no SQLite persistence, no cross-session resurrection. Register `spillover_get`, add tool-target metadata plus `ToolKind` coverage, and return clear errors for expired or foreign handles.

**Acceptance criteria:**
- [ ] Handler state owns a shared spillover store in stdio and daemon construction paths.
- [ ] `spillover_get` is public and registered in the tool router.
- [ ] Expired, missing, or foreign-session handles return a clear tool error.
- [ ] Session metrics and tool-target metadata include `spillover_get`.
- [ ] Narrow spillover and handler tests pass, committed.

### Task 3: Build Blast Radius

**Files:**
- Create: `src/tools/impact/mod.rs`
- Create: `src/tools/impact/seed.rs`
- Create: `src/tools/impact/walk.rs`
- Create: `src/tools/impact/ranking.rs`
- Create: `src/tools/impact/formatting.rs`
- Create: `src/tests/tools/blast_radius_tests.rs`
- Create: `src/tests/tools/blast_radius_formatting_tests.rs`
- Modify: `src/tools/mod.rs:6-24`
- Modify: `src/handler.rs:748-774`, `src/handler.rs:2384-2837`
- Modify: `src/handler/tool_targets.rs:1-90`
- Modify: `src/tools/metrics/session.rs:25-70`
- Modify: `src/database/revision_changes.rs`
- Modify: `src/tests/core/handler_telemetry.rs`
- Modify: `src/tests/mod.rs`

**What to build:** Add a public `blast_radius` tool that accepts explicit symbols, explicit files, or a revision range, then returns impacted symbols, why they ranked, likely tests, and a spillover handle when the first page fills up. V1 must seed from current symbols in changed files, report deleted files that have no current symbols, and keep ranking deterministic.

**Approach:** Put request parsing and tool wiring in `impact/mod.rs`, keep seed resolution in `seed.rs`, graph walking in `walk.rs`, and ranking in `ranking.rs`. Use revision-range semantics of `(from_revision, to_revision]` so the lower bound acts as the previous checkpoint and the upper bound is included. Walk inbound edges with explicit relationship priority: `calls`, `overrides`, `implements`, `instantiates`, `references`, `imports`. Rank by distance, relationship priority, `reference_score`, and optional visibility boost only when metadata provides it. Likely tests should read `test_linkage.linked_tests` first, then live test-symbol edges, then identifier linkage, then path heuristics as the last fallback. Use spillover once impacted symbols or likely tests exceed the first page budget.

**Acceptance criteria:**
- [ ] `blast_radius` works from symbol seeds, file seeds, and revision-range seeds.
- [ ] Output explains each impacted symbol with a short `why` path or reason.
- [ ] Deleted files from a revision range are reported instead of disappearing.
- [ ] Ranking contains no codehealth scores and no Rust-only heuristics.
- [ ] Spillover works for long impact lists without rerunning the graph walk.
- [ ] Narrow blast-radius and telemetry tests pass, committed.

### Task 4: Extend Get Context With Task Inputs

**Files:**
- Create: `src/tools/get_context/task_signals.rs`
- Create: `src/tests/tools/get_context_task_inputs_tests.rs`
- Modify: `src/tools/get_context/mod.rs:1-45`
- Modify: `src/tools/get_context/scoring.rs:1-188`
- Modify: `src/tools/get_context/pipeline.rs:53-608`
- Modify: `src/tools/get_context/formatting.rs:1-236`
- Modify: `src/handler/tool_targets.rs:56-65`
- Modify: `src/tests/tools/get_context_scoring_tests.rs`
- Modify: `src/tests/tools/get_context_pipeline_tests.rs`
- Modify: `src/tests/tools/get_context_graph_expansion_tests.rs`
- Modify: `src/tests/tools/get_context_formatting_tests.rs`
- Modify: `src/tests/mod.rs`

**What to build:** Extend `GetContextTool` with optional `edited_files`, `entry_symbols`, `stack_trace`, `failing_test`, `max_hops`, and `prefer_tests` fields while keeping old `query`-only calls intact. The tool should bias pivot selection with those signals, allow a bounded second hop when the first hop is thin, and page overflow through spillover instead of dumping a giant graph blob.

**Approach:** Add a `TaskSignals` helper so `pipeline.rs` does not absorb another wad of parsing logic. Keep `query` required and central. Task inputs bias scoring; they do not replace search. Use generic file:line extraction plus light symbol token scraping for stack traces. Boost symbols in `edited_files`, explicit `entry_symbols`, and production symbols linked to the `failing_test`. Only take a second hop when `max_hops >= 2` and first-hop context is still under-covered. If `prefer_tests` is true, let linked tests compete for neighbor slots; if false, keep tests in a secondary lane. Use spillover for extra neighbors or second-hop additions that do not fit in the first response.

**Acceptance criteria:**
- [ ] Existing `get_context(query=...)` behavior stays green.
- [ ] Task inputs influence pivot selection and neighbor expansion in focused tests.
- [ ] Second-hop expansion stays bounded and only triggers when requested plus useful.
- [ ] Stack-trace handling works from generic file:line inputs, not language-specific parsers.
- [ ] Overflow neighbors or second-hop rows page through spillover instead of bloating the first response.
- [ ] Narrow get_context tests pass, committed.

### Task 5: Finish Tool Surface, Docs, And Batch Verification

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md`
- Modify: `.claude/settings.local.json`
- Modify: `src/tests/core/handler.rs`
- Modify: `src/tests/core/handler_telemetry.rs`
- Modify: `src/tests/integration/indexing_pipeline.rs`
- Modify: `src/tests/tools/metrics/session_metrics_tests.rs`

**What to build:** Finish the public tool surface so the new tools can be discovered and called by harnesses, then run the calibrated verification path for the full batch. This task closes the loop on agent instructions, allowlists, handler coverage, and the user-facing language around static linkage.

**Approach:** Update `JULIE_AGENT_INSTRUCTIONS.md` with `blast_radius`, `spillover_get`, and the new `get_context` task inputs. Update `.claude/settings.local.json` so the harness can call the new tool surface. Tighten handler tests so public tool registration, hidden internal tools, and telemetry metadata still behave. Use the integration test pass to catch analysis ordering, reindex fallback, and tool-router drift. Confirm no touched user-facing output still claims that static linkage is runtime coverage.

**Acceptance criteria:**
- [ ] Agent instructions and harness allowlist cover `blast_radius`, `spillover_get`, and the extended `get_context`.
- [ ] Handler tests cover the new public tool surface and telemetry shapes.
- [ ] No touched user-facing output still claims static linkage is runtime coverage.
- [ ] `cargo xtask test changed` passes.
- [ ] `cargo xtask test dev` passes.

## Final Verification

- Run the narrowest test-name command for each RED and GREEN loop inside the tasks above.
- Run `cargo xtask test changed`.
- Run `cargo xtask test dev`.
- Dogfood the new flow in the repo workspace:
- `blast_radius(file_paths=[...])`
- `blast_radius(from_revision=..., to_revision=...)`
- `get_context(query=..., edited_files=[...], stack_trace=...)`
- `spillover_get(spillover_handle=...)` for both impact and context results
- verify stale `test_coverage` metadata still reads through fallback before reindex, then confirm reindex writes `test_linkage`

## Review Gate

- Use @razorback:subagent-driven-development for execution unless the work unexpectedly collapses into one narrow sequential slice.
- Capture the external reviewer choice after implementation-plan approval.
