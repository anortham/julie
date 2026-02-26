---
id: improve-get-context-relevance-ux-and-performance
title: Improve get_context Relevance, UX, and Performance
status: completed
created: 2026-02-26T01:06:30.987Z
updated: 2026-02-26T01:47:24.150Z
tags:
  - get-context
  - agent-ux
  - performance
  - search-quality
---

# Improve `get_context` Relevance, UX, and Performance

## Goal
Raise `get_context` from "good first pass" to "default orientation tool" for coding agents by improving:
1. Retrieval relevance for natural-language task queries
2. Output quality/token efficiency
3. Runtime performance and maintainability
4. Regression safety with query-quality benchmarks

## Current Issues (Validated)
- Natural-language queries can surface non-code pivots (docs/memories/tests) instead of actionable code.
- Relationship names can duplicate in output (`Callers: run, run`).
- Pipeline has avoidable N+1 database calls for pivot content and per-pivot relationships.
- Formatting is readable but token-heavy for LLMs.
- `src/tests/tools/get_context_tests.rs` exceeds project test file size guideline.

## Scope
### In scope
- `src/tools/get_context/{scoring,pipeline,formatting,allocation}.rs`
- `src/tests/tools/get_context_*.rs`
- Lightweight docs updates for tool behavior and flags

### Out of scope (for this plan)
- Major re-architecture of search index
- New embedding-based retrieval systems
- Breaking changes to existing tool schema

## Implementation Plan

### Phase 1 — Relevance Guardrails (Highest ROI)
**Objective:** Ensure pivots are mostly actionable code for ambiguous concept queries.

#### Tasks
1. Add a **code-first fallback pass** in `run_pipeline`:
   - If top-ranked pivots are dominated by non-code/test/structural items, retry selection with stronger code constraints.
   - Keep this internal (no API break) and deterministic.
2. Introduce explicit pivot-quality checks:
   - Minimum fraction of pivots from code paths (not docs/memories/tests).
   - Minimum number of non-structural symbol kinds.
3. Tighten scoring penalties/tie-breakers in `select_pivots`:
   - Preserve current penalties but improve deterministic ordering on ties.
4. Add tests for failure modes:
   - Natural-language query returning docs first should recover to code pivots.
   - Mixed docs+code corpus should prioritize executable symbols.

#### Deliverables
- Improved pivot selection behavior in `scoring.rs` + `pipeline.rs`
- New tests proving fallback and quality-gate behavior

#### Exit Criteria
- Query-quality fixture suite (Phase 5) shows improved top-3 pivot code relevance.

---

### Phase 2 — Output Quality and Token Efficiency
**Objective:** Make output easier for agents to consume with less token overhead.

#### Tasks
1. Deduplicate relationship names:
   - Dedup + stable sort `incoming_names` and `outgoing_names` before formatting.
2. Add optional `format` mode to tool params:
   - `format = "readable"` (default, current style)
   - `format = "compact"` (token-lean, predictable sections, minimal separators)
3. Reduce noisy rendering in compact mode:
   - Avoid heavy box drawing; use concise section markers.
   - Keep required semantic info (pivot location, kind, centrality, neighbors, files).
4. Add formatting tests for both modes and dedup behavior.

#### Deliverables
- Updated `GetContextTool` schema with optional `format`
- Formatting support in `formatting.rs`
- Tests validating compact output structure + dedup

#### Exit Criteria
- `compact` output uses materially fewer tokens (target: 20%+ reduction on fixture queries).

---

### Phase 3 — Performance Cleanup (N+1 Removal)
**Objective:** Lower latency for medium/large codebases.

#### Tasks
1. Batch-load pivot content instead of per-pivot `get_symbol_by_id` calls.
2. Batch-load relationships for pivot set where practical:
   - Pre-fetch incoming/outgoing rels for all pivot IDs.
   - Build per-pivot caller/callee name maps in-memory.
3. Minimize repeated symbol-resolution lookups in relationship rendering.
4. Add perf-focused tests/bench harness (small synthetic graph) for regression detection.

#### Deliverables
- Refactored `build_pivot_entries` path with batched DB access
- Reduced blocking time in `run_pipeline`

#### Exit Criteria
- Measurable latency reduction on representative queries (target: 25%+ median reduction in local fixture benchmark).

---

### Phase 4 — Test Structure and Maintainability
**Objective:** Align tests with project standards and keep future changes safe.

#### Tasks
1. Split oversized `get_context_tests.rs` into focused files:
   - `get_context_selection_tests.rs`
   - `get_context_graph_tests.rs`
   - `get_context_pipeline_tests.rs`
   - `get_context_token_budget_tests.rs`
2. Preserve existing coverage while improving readability.
3. Ensure no test file exceeds configured size guidance.

#### Deliverables
- Modular test layout under `src/tests/tools/`
- Updated `src/tests/mod.rs` module wiring

#### Exit Criteria
- All get_context tests pass with equal or higher coverage confidence.

---

### Phase 5 — Query Quality Evaluation Suite
**Objective:** Add a durable quality signal beyond unit tests.

#### Tasks
1. Build a fixed query set (20-50 prompts) representing real agent intents:
   - "where auth token is validated"
   - "payment retry behavior"
   - "workspace routing reference db"
   - etc.
2. Add expected quality assertions (heuristic-based):
   - Top pivots should include at least one code symbol from expected area.
   - Penalize docs/tests-only top pivots.
3. Add lightweight scoring report output for local comparison pre/post changes.

#### Deliverables
- Fixture-based quality tests + report helper
- Baseline metrics committed for future regression tracking

#### Exit Criteria
- Improved quality score vs baseline and stable pass/fail thresholds.

---

## TDD Strategy (Non-negotiable)
For each phase:
1. Write failing tests for the exact behavior gap.
2. Implement minimal code to pass.
3. Refactor while keeping tests green.
4. Run targeted get_context tests after each slice.

## Test Execution Strategy
- Primary loop: targeted tests (`cargo test --lib tests::tools::get_context` and specific new test modules)
- Fast tier after each non-trivial slice: `cargo test --lib -- --skip search_quality`
- Avoid full dogfood suite until merge gate.

## Risks and Mitigations
- **Risk:** Over-penalizing docs may hide useful architectural docs.
  - **Mitigation:** Keep docs as secondary candidates; only fallback when pivots fail quality gates.
- **Risk:** Compact format breaks downstream assumptions.
  - **Mitigation:** Keep current format as default; additive optional parameter only.
- **Risk:** Batched relationship fetch complexity introduces bugs.
  - **Mitigation:** Add graph parity tests comparing old/new relationship name sets.

## Success Metrics
1. **Relevance:** Higher top-3 pivot code hit rate on evaluation queries.
2. **Efficiency:** Compact mode token count reduced by >=20% vs readable mode.
3. **Latency:** Median get_context runtime reduced by >=25% on local benchmark set.
4. **Quality:** No regression in existing get_context test coverage.

## Suggested Delivery Order
1. Phase 1 (relevance guardrails)
2. Phase 2 (dedup + compact format)
3. Phase 3 (performance)
4. Phase 4 (test modularization)
5. Phase 5 (evaluation suite)

## First Concrete Next Step
Implement Phase 1 slice A:
- Add failing test for natural-language query that currently ranks docs/test content above code.
- Implement internal fallback pass in `run_pipeline` to recover code pivots.
- Re-run targeted get_context tests and verify no regressions.

