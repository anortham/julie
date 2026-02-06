# Code Intelligence Audit & Optimization — Progress Tracker

**Master Plan:** [2026-02-05-code-intelligence-audit-design.md](2026-02-05-code-intelligence-audit-design.md)
**Started:** 2026-02-05
**Last Updated:** 2026-02-06 (Phase 3 complete, including Round 2 deep-dives)

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

## Phase 3: Tool Deep-Dives — COMPLETE

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

### Priorities 5-9: Parallel team cleanup — COMPLETE

Executed via 4-agent team ("phase3-cleanup") working on non-overlapping file trees.

Commit: `5433cd8 refactor(tools): parallel cleanup of fast_explore, navigation, editing, and memory tools`

| Agent | Scope | Key Changes | Lines |
|-------|-------|-------------|-------|
| explore-agent | fast_explore | Removed dead Tests/Similar modes, unused params | -70 |
| nav-agent | fast_goto, fast_refs | Removed unused create_result params, shared lock_db(), fixed context_file false positive | -105 |
| editing-agent | fuzzy_replace, edit_lines | Split 754→487+250 lines (under limit), &PathBuf→&Path, consolidated validation | -285+250 |
| memory-agent | checkpoint, plan_tool | Extracted shared capture_git_context() | -179 |

Net: 15 files changed, +436/-956 lines. 762 tests pass.

### Priorities 5-9: Deep-Dives Round 2 — COMPLETE

Executed the substantive work skipped during the parallel cleanup: data utilization, output enrichment, Bucket C decisions, and discoverability fixes.

Plan: [2026-02-06-phase3-deep-dives-round2.md](2026-02-06-phase3-deep-dives-round2.md)

| Task | Description | Status |
|------|-------------|--------|
| 1 | Kill find_logic MCP registration (redirect to fast_explore mode=logic) | Done |
| 2 | Integrate identifiers into fast_explore logic mode (Tier 4 caller analysis) | Done |
| 3 | Add visibility-aware ranking to logic mode (pub +0.1, priv -0.15) | Done |
| 4 | Implement fast_goto qualified name resolution (MyClass::method) | Done |
| 5 | Enrich fast_goto output with parent name and visibility | Done |
| 6 | Rewrite editing tool MCP descriptions for agent discoverability | Done |
| 7 | Expand editing tools section in agent instructions | Done |
| 8 | Fix recall ("semantic search" → "text search") and checkpoint descriptions | Done |
| 9 | Dogfood test all improved tools | Done |

Commits: `c3945e4`, `273f41c`, `3543a26`, `12424f7`, `e35cd27`, `42cff15`, `9eba701`

774 tests pass. Data utilization scorecard: identifiers 40%→60%, visibility 0%→20%.

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
