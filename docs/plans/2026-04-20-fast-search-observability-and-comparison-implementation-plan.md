# Fast Search Observability And Comparison Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:team-driven-development (on Claude Code) or razorback:subagent-driven-development (elsewhere) to implement this plan. Fall back to razorback:executing-plans for single-task or tightly-sequential plans.

**Goal:** Build a dashboard-backed observability and comparison loop for `fast_search` so Julie can measure first-hit quality, inspect search episodes, and compare ranking changes against real query corpora.

**Architecture:** This work has a sequential foundation and a parallel middle. First, extract a shared structured search execution layer so the dashboard playground and `fast_search` use the same core path. Next, extend tool-call telemetry for `fast_search` and downstream navigation or edit tools. After that foundation lands, split the work: one track builds episode analysis and dashboard trace views, the other builds replay persistence and comparison UI. Finish by wiring the dashboard to the new routes, running search-heavy verification, and reviewing whether the traces answer the product questions that started this project.

**Tech Stack:** Rust, Axum, Tera, SQLite, existing `tool_calls` metrics pipeline, Julie search stack, `cargo nextest`, `cargo xtask test changed`, `cargo xtask test dogfood`, `cargo xtask test dev`

---

## Execution notes

- The dashboard playground must route through the same shared execution core as `fast_search`. If the playground keeps calling raw index APIs, the observability story is fake.
- Keep new logic out of `src/handler.rs`, `src/daemon/database.rs`, `src/dashboard/state.rs`, and `src/tools/search/text_search.rs` where possible. Those files are already over the project size target and do not need more passengers.
- Keep request-path work narrow. Capture data once, persist it through the existing metrics writer, then analyze it later in the dashboard or replay path.
- Treat `tool_calls` as the raw event log. Derive episodes, convergence, stall signals, and compare summaries from stored calls instead of inventing a second live metrics pipeline.
- Follow TDD for behavior changes and regression fixes. The first failing test for this project should prove the dashboard playground and `fast_search` can disagree today.

## Task 1: Extract a shared structured search execution core

**Files:**
- Create: `src/tools/search/execution.rs`
- Create: `src/tools/search/trace.rs`
- Modify: `src/tools/search/mod.rs`
- Modify: `src/tools/search/text_search.rs`
- Modify: `src/dashboard/routes/search.rs`
- Modify: `src/tests/dashboard/integration.rs`
- Modify: `src/tests/tools/search_quality/dogfood_tests.rs`

**What to build:** Pull the common execution path for `fast_search` into a structured search service that returns ranked hits, trace diagnostics, result counts, and top-hit summaries without forcing callers to parse formatted tool output.

**Approach:**
- Move the shared search work out of `text_search.rs` into `execution.rs` so the file stops growing.
- Define trace-friendly result types in `trace.rs`, including top-hit summaries that can feed telemetry and dashboard rendering.
- Keep tool-specific response formatting in `text_search.rs`; the structured execution layer should stay presentation-neutral.
- Change the dashboard search route to call the shared execution core instead of direct `SearchIndex` entrypoints.
- Add one regression that proves the dashboard playground and `fast_search` now use the same ranked result source for the same query and filters.

**Acceptance criteria:**
- [ ] `fast_search` and the dashboard playground call the same structured search execution entrypoint.
- [ ] The shared result type includes ranked hits, result count, strategy id, and compact top-hit summaries.
- [ ] The dashboard no longer needs to reconstruct search behavior from raw Tantivy responses.
- [ ] Focused tests cover shared execution parity for at least one definition search and one content search.

## Task 2: Extend telemetry for search traces and downstream targets

**Files:**
- Create: `src/handler/search_telemetry.rs`
- Create: `src/handler/tool_targets.rs`
- Modify: `src/handler.rs`
- Modify: `src/dashboard/state.rs`
- Modify: `src/tests/daemon/database.rs`
- Modify: `src/tests/integration/daemon_lifecycle.rs`
- Modify: `src/tests/dashboard/integration.rs`

**What to build:** Capture richer `fast_search` metadata and enough downstream-tool target metadata to tell where search bursts landed.

**Approach:**
- Move metadata-building helpers into `src/handler/search_telemetry.rs` and target-resolution helpers into `src/handler/tool_targets.rs` so `handler.rs` does not absorb another block of JSON assembly logic.
- Extend `fast_search` metadata with query, normalized query, filters, intent label, strategy id, result count, trace version, and top 3 hit summaries.
- Extend useful downstream tools with target metadata that prefers symbol id and falls back to file path and line.
- Keep dashboard live events lean. Broadcast summary data that helps the UI update, not full trace payloads that belong in SQLite.
- Reuse the existing bounded metrics writer path. No new synchronous database writes belong in tool handlers.

**Acceptance criteria:**
- [ ] `fast_search` writes structured trace metadata into `tool_calls.metadata`.
- [ ] Useful downstream tools write target metadata that can support symbol-first and file-fallback convergence.
- [ ] The existing metrics writer still owns persistence; tool handlers do not gain direct SQLite writes.
- [ ] Focused tests cover metadata shape for `fast_search` and at least one downstream action.

## Task 3: Add episode analysis services and dashboard trace views

**Files:**
- Create: `src/dashboard/search_analysis.rs`
- Create: `src/dashboard/routes/search_analysis.rs`
- Create: `dashboard/templates/search_analysis.html`
- Create: `dashboard/templates/partials/search_episode_table.html`
- Modify: `src/dashboard/mod.rs`
- Modify: `src/dashboard/routes/mod.rs`
- Modify: `src/daemon/database.rs`
- Modify: `src/tests/dashboard/integration.rs`
- Modify: `src/tests/daemon/database.rs`

**What to build:** Add dashboard analysis views that turn raw tool calls into search episodes and explain whether a burst of searches converged, dispersed, or stalled.

**Approach:**
- Keep episode derivation in `search_analysis.rs`, not in route handlers and not in the request path.
- Use the agreed rule set:
  - episode starts with `fast_search`
  - another `fast_search` joins when it lands within 10 seconds and no non-search tool fired in between
  - the first non-search tool closes the episode
- Treat these tools as useful downstream actions: `deep_dive`, `get_symbols`, `fast_refs`, `call_path`, `get_context`, `edit_file`, `rewrite_symbol`, `rename_symbol`.
- Define convergence as same symbol when available, otherwise same file.
- Add queries in the daemon database layer that fetch raw tool-call windows for one session, then let the dashboard analysis layer derive episode outcomes.

**Acceptance criteria:**
- [ ] The dashboard can render episode lists with query bursts, timing, top hits, downstream action, and final target.
- [ ] Episode analysis flags `reformulation_converged` and `stalled` using stored tool-call data, not transcript guesses.
- [ ] Intent labels and outcome labels are visible in the analysis view.
- [ ] Focused tests cover the 10-second burst boundary, useful-action closure, and same-symbol or same-file convergence rules.

## Task 4: Build replay persistence and side-by-side comparison

**Files:**
- Create: `src/dashboard/search_compare.rs`
- Create: `src/dashboard/routes/search_compare.rs`
- Create: `dashboard/templates/partials/search_compare_results.html`
- Modify: `src/dashboard/mod.rs`
- Modify: `src/dashboard/routes/mod.rs`
- Modify: `src/daemon/database.rs`
- Modify: `src/dashboard/routes/search.rs`
- Modify: `src/tests/dashboard/integration.rs`
- Modify: `src/tests/daemon/database.rs`

**What to build:** Add a comparison bench that reruns a captured query corpus against baseline and candidate search strategies and stores the run plus per-case results for review in the dashboard.

**Approach:**
- Persist replay artifacts in new daemon-database tables for compare runs and compare cases, while keeping raw session traces in `tool_calls`.
- Reuse the shared execution core from Task 1 so compare runs exercise the same ranking path as live search.
- Store enough case-level detail to inspect top-1 and top-3 changes, duplicate-heavy outputs, source-versus-doc ordering, and whether the later chosen target appeared higher or lower.
- Keep compare execution out of page-render code. Routes should trigger or load persisted runs, then render summaries and drill-down rows.
- Start with deterministic heuristics and captured traces. Leave model judging out of v1.

**Acceptance criteria:**
- [ ] The dashboard can run a compare pass between two search strategies over a stored corpus.
- [ ] Compare results persist run-level summaries and case-level rankings.
- [ ] The comparison UI can show top-1 win rate, top-3 containment, source-over-doc win rate, convergence rate, and stall rate.
- [ ] Focused tests cover compare-run persistence and one rendered compare result page.

## Task 5: Wire the dashboard navigation and tighten search UX

**Files:**
- Modify: `dashboard/templates/search.html`
- Modify: `dashboard/templates/partials/search_results.html`
- Modify: `dashboard/templates/partials/search_detail.html`
- Modify: `src/dashboard/routes/search.rs`
- Modify: `src/dashboard/mod.rs`
- Modify: `src/tests/dashboard/integration.rs`

**What to build:** Turn the search area into a three-part surface: playground, analysis, and compare, with shared filters and visible trace details.

**Approach:**
- Keep the current playground for single-query inspection, but add links or tabs into analysis and compare.
- Show strategy id, intent label, and top-hit trace details in the playground so one search can be inspected without hopping to the metrics page.
- Keep the metrics page light. Search-heavy drill-down belongs in the search section.
- Make page flows clear enough that the dashboard becomes the default debugging surface when harness transcripts hide tool output.

**Acceptance criteria:**
- [ ] The search page links cleanly to analysis and compare views.
- [ ] Playground results expose trace details from the shared execution core.
- [ ] Search observability lives under the search area, not bolted onto the metrics page.
- [ ] Dashboard integration tests cover the new navigation surfaces.

## Task 6: Verify search behavior and guard against dashboard fiction

**Files:**
- Modify: `src/tests/dashboard/integration.rs`
- Modify: `src/tests/daemon/database.rs`
- Modify: `src/tests/tools/search_quality/dogfood_tests.rs`
- Modify: `src/tests/integration/daemon_lifecycle.rs`
- No new production files required

**What to build:** Run the narrow tests added in earlier tasks, then the calibrated tiers that match search-risk changes.

**Approach:**
- During implementation, use exact test-name runs for RED and GREEN loops.
- Before handoff, run `cargo xtask test changed`, `cargo xtask test dogfood`, and `cargo xtask test dev`.
- If dogfood catches search-ranking drift, inspect it with the new comparison bench before calling the project done.
- End with a brief review note that answers the product question: can Julie now explain what `fast_search` did, whether it helped, and whether a candidate ranking is better?

**Acceptance criteria:**
- [ ] New narrow tests for shared execution, telemetry, episode analysis, and compare persistence pass.
- [ ] `cargo xtask test changed` passes.
- [ ] `cargo xtask test dogfood` passes.
- [ ] `cargo xtask test dev` passes.
- [ ] The final implementation summary includes remaining blind spots, if any, instead of pretending the dashboard sees everything.
