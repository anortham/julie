# Agent Tool Surface Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Remove `query_metrics` from Julie's MCP surface, cover the lost admin visibility in the dashboard, replace `edit_symbol` with AST-backed `rewrite_symbol`, and add compact shortest-path `call_path`.

**Architecture:** Ship this as one coherent tool-surface pass. The metrics removal, dashboard follow-through, symbolic editing rewrite, path-finding tool, and guidance updates all change the agent-facing story at once, so they should land on one branch with one review pass. Keep the design narrow: each tool has one job, default output stays compact, and no task should reintroduce broad tracing or kitchen-sink editing.

**Tech Stack:** Rust, rmcp tool routing, existing symbol database and relationship graph, tree-sitter-backed file parsing, Axum + Tera dashboard routes, Julie test harness, `cargo nextest`, `cargo xtask`

---

## Plan Type

Light plan for same-session execution. Each task follows TDD with the narrowest test first, then `cargo xtask test changed` after localized batches, then one `cargo xtask test dev` after the full implementation batch.

## Scope Check

This spec touches three subsystems, metrics/dashboard, symbolic editing, and path tracing. They stay in one plan because they converge on one public outcome: a tighter agent tool surface with aligned docs, handler routing, and site examples. The lower-use output-shape tweaks for `fast_refs`, `get_context`, and `rename_symbol` are deferred until the core tool changes land.

## File Map

- `src/handler.rs`
  Tool registration and handler methods for removing `query_metrics`, replacing `edit_symbol`, and adding `call_path`.
- `src/tools/mod.rs`
  Top-level tool exports.
- `src/tools/metrics/mod.rs`
  Current `QueryMetricsTool` definition and tool entrypoint, to be removed.
- `src/dashboard/routes/metrics.rs`
  Dashboard metrics aggregation, filters, and summary/table route behavior.
- `dashboard/templates/metrics.html`
  Metrics page shell.
- `dashboard/templates/partials/metrics_table.html`
  Metrics table output.
- `dashboard/templates/partials/metrics_summary.html`
  Metrics summary output.
- `src/tools/editing/mod.rs`
  Editing module registration.
- `src/tools/editing/validation.rs`
  Shared edit validation helpers reused by `rewrite_symbol`.
- `src/tools/editing/rewrite_symbol.rs`
  New symbol rewrite tool implementation.
- `src/tools/navigation/mod.rs`
  Navigation tool registration.
- `src/tools/navigation/call_path.rs`
  New shortest-path navigation tool.
- `JULIE_AGENT_INSTRUCTIONS.md`
  Agent-facing tool descriptions and default workflows.
- `docs/site/index.html`
  Site examples and tool list.
- `docs/site/script.js`
  Site animation and example command strings.
- `src/tests/tools/metrics/`
  Remove or replace `query_metrics`-specific tool tests.
- `src/tests/dashboard/`
  Dashboard route and rendering coverage for the metrics page.
- `src/tests/tools/editing/`
  `rewrite_symbol` tool tests.
- `src/tests/tools/`
  New `call_path` tool tests.
- `src/tests/mod.rs`
  Test module registration and stale top-level comments.

## Task 1: Remove `query_metrics` and make the dashboard carry the metrics story

**Files:**
- Modify: `src/handler.rs:717-730`
- Modify: `src/handler.rs:2647-2673`
- Modify: `src/tools/mod.rs:18-25`
- Modify: `src/tools/metrics/mod.rs:37-166`
- Modify: `src/dashboard/routes/metrics.rs:23-287`
- Modify: `dashboard/templates/metrics.html`
- Modify: `dashboard/templates/partials/metrics_table.html`
- Modify: `dashboard/templates/partials/metrics_summary.html`
- Modify: `src/tests/tools/metrics/mod.rs:1-6`
- Modify or delete: `src/tests/tools/metrics/primary_rebind_metrics_tests.rs`
- Test: `src/tests/dashboard/integration.rs:185-552`
- Test: `src/tests/dashboard/router.rs:29-63`

**What to build:** Remove the public `query_metrics` MCP tool and its handler wiring, while keeping the metrics writer and daemon DB history intact. Expand the dashboard metrics routes and templates so users still have workspace filtering, all-workspace aggregation, latency, success rate, and context-efficiency visibility without needing a tool call.

**Approach:** Use the dashboard's direct daemon DB access as the source of truth. Delete the tool schema and handler entrypoints instead of leaving a dormant MCP shim behind. Replace tool-focused tests with dashboard route coverage that exercises no-data, per-workspace, and all-workspace views.

**Acceptance criteria:**
- [ ] `query_metrics` no longer appears in the MCP tool list or handler routing.
- [ ] `src/tools/mod.rs` no longer re-exports `QueryMetricsTool`.
- [ ] Dashboard metrics pages still render with no daemon DB, one workspace, and aggregated all-workspace data.
- [ ] Metrics summary and table views expose success rate, latency, and context-efficiency data without tool indirection.
- [ ] No `query_metrics`-specific tool tests remain.

## Task 2: Replace `edit_symbol` with AST-backed `rewrite_symbol`

**Files:**
- Modify: `src/handler.rs:2736-2771`
- Modify: `src/tools/editing/mod.rs:9-11`
- Modify: `src/tools/editing/validation.rs`
- Create: `src/tools/editing/rewrite_symbol.rs`
- Modify: `src/tests/tools/editing/mod.rs:5-10`
- Create: `src/tests/tools/editing/rewrite_symbol_tests.rs`
- Remove: `src/tests/tools/editing/edit_symbol_tests.rs`
- Modify: `src/tests/mod.rs:1-7`

**What to build:** Introduce `rewrite_symbol` as the public symbol-edit tool and retire `edit_symbol`. Preserve the safety properties that matter, symbol resolution, ambiguity errors, dry-run-by-default previews, freshness checks, and atomic writes, while replacing the line-range edit engine with live-file syntax-node rewriting.

**Approach:** Start from the current `edit_symbol` flow, not from scratch. Keep symbol lookup, freshness validation, and diff preview behavior. Reparse the current file, locate the current syntax node or node-derived span for the resolved symbol, and apply one narrow operation at a time: `replace_full`, `replace_body`, `replace_signature`, `insert_before`, `insert_after`, `add_doc`. Use diff-match-patch only as a bounded fallback anchor when node recovery drifts, not as the primary rewrite engine.

**Acceptance criteria:**
- [ ] `rewrite_symbol` is the registered public tool and defaults to `dry_run=true`.
- [ ] `rewrite_symbol` supports `replace_full`, `replace_body`, `replace_signature`, `insert_before`, `insert_after`, and `add_doc`.
- [ ] Live-file parsing, not stored line spans, is the primary edit anchor.
- [ ] Ambiguous symbol matches, stale files, and unsupported operations fail with narrow, specific errors.
- [ ] Tests cover dry run, apply, ambiguity, stale index rejection, and at least one case for each supported operation.
- [ ] Stale comments in `src/tests/mod.rs` about removed editing tools are corrected.

## Task 3: Add shortest-path `call_path` under navigation

**Files:**
- Modify: `src/tools/navigation/mod.rs:11-17`
- Create: `src/tools/navigation/call_path.rs`
- Modify: `src/tools/mod.rs:18-25`
- Modify: `src/handler.rs:2386-2414`
- Create: `src/tests/tools/call_path_tests.rs`
- Modify: `src/tests/mod.rs:65-124`

**What to build:** Add `call_path(from, to, max_hops=6)` as a bounded navigation tool that returns one shortest relationship path or a short no-path result. The tool should answer "how does A reach B" without reviving the abandoned broad tracing surface.

**Approach:** Implement this as a new navigation tool beside `fast_refs`. Use the indexed relationship graph and a bounded shortest-path search, breadth-first is fine if the branching caps stay strict. Reuse ideas and fixtures from `src/tracing/mod.rs` and `src/tests/integration/tracing.rs` where useful, but do not resurrect `CrossLanguageTracer` as the tool implementation. Output must stay compact: ordered hop list, terse edge labels, short file anchors, one path only.

**Acceptance criteria:**
- [ ] `call_path` is registered in navigation and exposed through the MCP tool router.
- [ ] The tool returns one shortest path with compact hop entries when a path exists.
- [ ] The tool returns `found: false` with a short diagnostic when no path exists.
- [ ] `max_hops` bounds the search and the output.
- [ ] Tool tests cover found-path, no-path, and hop-cap behavior.

## Task 4: Align agent guidance, site examples, and tool-surface docs

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md:21-22`
- Modify: `JULIE_AGENT_INSTRUCTIONS.md:26-27`
- Modify: `JULIE_AGENT_INSTRUCTIONS.md:50`
- Modify: `docs/site/index.html:287`
- Modify: `docs/site/index.html:442-445`
- Modify: `docs/site/index.html:479-482`
- Modify: `docs/site/script.js:98`
- Modify: `docs/site/script.js:166`

**What to build:** Update the live tool descriptions and examples so they describe the roster that now exists: no `query_metrics`, no `edit_symbol`, and a new `rewrite_symbol` plus `call_path` story. This task is part of the same batch because stale guidance will keep agents reaching for dead tools.

**Approach:** Limit edits to live guidance surfaces, not historical plan docs. Replace `edit_symbol` examples with `rewrite_symbol`, remove `query_metrics` references, and add one compact `call_path` example where it helps explain the navigation story. Keep `manage_workspace` present but out of the default coding loop examples.

**Acceptance criteria:**
- [ ] `JULIE_AGENT_INSTRUCTIONS.md` no longer references `query_metrics` or `edit_symbol`.
- [ ] Site examples and animated command strings match the live tool roster.
- [ ] Guidance still emphasizes the compact core loop: `fast_search`, `get_symbols`, `deep_dive`, `edit_file`, with `rewrite_symbol` and `call_path` presented for their narrow jobs.

## Deferred Follow-up

These are not part of the first implementation batch:

- `fast_refs` summary-first formatting improvements
- `get_context` next-step footer and tighter default neighbor budget
- `rename_symbol` dry-run safety summary improvements

After the core tool-surface changes land, revisit these as a smaller follow-up plan with their own acceptance criteria.
