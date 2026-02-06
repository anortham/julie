# Code Intelligence Audit & Optimization — Progress Tracker

**Master Plan:** [2026-02-05-code-intelligence-audit-design.md](2026-02-05-code-intelligence-audit-design.md)
**Started:** 2026-02-05
**Last Updated:** 2026-02-06 (Priority 4 complete)

---

## Phase 1: Data Pipeline Audit — COMPLETE

Automated census of all 30 extractors confirmed comprehensive data extraction.
Pipeline trace verified: extract → SQLite → Tantivy → query for all data types.

---

## Phase 2: Tool Categorization — COMPLETE

Plan: [2026-02-06-phase2-tool-categorization.md](2026-02-06-phase2-tool-categorization.md)

Categorized 15 tools into Buckets A (table-stakes), B (moat), C (evaluate).
Produced prioritized Phase 3 deep-dive order.

---

## Phase 3: Tool Deep-Dives — IN PROGRESS

### Priority 1: fast_search — COMPLETE

Plan: [2026-02-06-phase3-fast-search-improvements.md](2026-02-06-phase3-fast-search-improvements.md)

| Task | Description | Status |
|------|-------------|--------|
| 1 | Fix content search false positives | Done |
| 2 | Remove dead search_method values | Done |
| 3 | Clean up unused context_lines parameter | Done |
| 4 | Populate code_context from Tantivy | Done |
| 5 | Make LEAN default in AUTO mode | Done |
| 6 | Refactor call_tool() god method | Done |

Commit: `8576d0b fix(search): content search false positives, code_context enrichment, refactor`

### Priority 2: fast_refs + identifiers unlock — COMPLETE

| Task | Description | Status |
|------|-------------|--------|
| 1 | Create src/database/identifiers.rs query layer | Done |
| 2 | Integrate identifiers into fast_refs (Strategy 3) | Done |
| 3 | Fix reference_kind filtering (broken JOIN) | Done |
| 4 | Rebuild dogfood fixture (63K identifiers) | Done |
| 5 | Add unit + dogfood tests | Done |

Commit: `40ac5d3 feat(fast_refs): unlock identifiers table for reference discovery`

### Priority 3: get_symbols optimization — COMPLETE

| Task | Description | Status |
|------|-------------|--------|
| 1 | Lightweight DB query (skip unused columns) | Done |
| 2 | Index-based filter pipeline (single-clone) | Done |
| 3 | SymbolKind Display consistency in TOON | Done |

Commit: `a3f64bc perf(get_symbols): lightweight DB query, index-based filtering, Display consistency`

### Bug fixes discovered during audit

| Bug | Description | Status |
|-----|-------------|--------|
| Overflow panic | `find_containing_symbol()` u32 underflow on multi-line symbols | Fixed |
| is_primary_workspace | Compared against CWD instead of workspace root | Fixed |
| Flaky workspace_init tests | Missing `#[serial]` on env-var-mutating tests | Fixed |
| Stale dogfood test | Referenced removed FTS5 function | Fixed |

Commit: `7c18e80 fix: overflow panic in find_containing_symbol and incorrect is_primary_workspace detection`

### Priority 4: trace_call_path optimization — COMPLETE

Plan: [distributed-crunching-mist.md](/Users/murphy/.claude/plans/distributed-crunching-mist.md)

**Tier 1 — Dead code cleanup (no behavior change):**

| Task | Description | Status |
|------|-------------|--------|
| 1 | Remove 4 unused create_result parameters (6 call sites) | Done |
| 2 | Remove unused handler param from tracing functions (4 recursive sites) | Done |
| 3 | Merge duplicate cross_language functions into single `find_cross_language_symbols` | Done |
| 4 | Remove dead similarity field + fix incorrect #[allow(dead_code)] annotations | Done |
| 5 | Extract lock_db mutex recovery helper (replaced 5 copy-paste blocks) | Done |
| 6 | Fix calculate_max_depth math (was using absolute level, now counts tree height) | Done |

**Tier 2 — Bug fix:**

| Task | Description | Status |
|------|-------------|--------|
| 7 | Fix direction=both shared visited set (separate sets per direction) | Done |

**Tier 3 — Identifiers integration:**

| Task | Description | Status |
|------|-------------|--------|
| 8 | Supplement upstream tracing with identifiers table (naming variants + call kind) | Done |

Results: cross_language.rs 110→53 lines, total ~1260→1189 lines despite new code. 763 tests pass (+2 new).

### Priorities 5-9: Not yet started

| Priority | Tool | Status |
|----------|------|--------|
| 5 | fast_explore | Pending |
| 6 | fast_goto | Pending |
| 7 | Editing tools (edit_symbol, edit_lines, fuzzy_replace) | Pending |
| 8 | Memory tools (checkpoint, recall, plan) | Pending |
| 9 | manage_workspace | Pending |

---

## Phase 4: Documentation Cleanup — NOT STARTED

- Remove all references to embeddings, HNSW, CASCADE, ONNX, semantic search
- Update architecture docs for Tantivy + SQLite
- Update CLAUDE.md
- Update tool descriptions in handler.rs

---

## Pre-Tantivy Work (for reference)

| Phase | Description | Status |
|-------|-------------|--------|
| Tantivy Migration | Replaced FTS5 + ORT/HNSW with Tantivy | Complete (16 tasks) |
| Post-Migration Cleanup | CodeTokenizer fixes, stale comments, test fixes | Complete |

Plans: [2026-02-04-tantivy-search-engine-design.md](2026-02-04-tantivy-search-engine-design.md), [2026-02-04-tantivy-implementation-plan.md](2026-02-04-tantivy-implementation-plan.md)
