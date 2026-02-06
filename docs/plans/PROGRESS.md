# Code Intelligence Audit & Optimization — Progress Tracker

**Master Plan:** [2026-02-05-code-intelligence-audit-design.md](2026-02-05-code-intelligence-audit-design.md)
**Started:** 2026-02-05
**Last Updated:** 2026-02-06

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

### Priority 3: get_symbols optimization — IN PROGRESS

| Task | Description | Status |
|------|-------------|--------|
| 1 | Lightweight DB query (skip unused columns) | In progress (teammate) |
| 2 | Index-based filter pipeline (single-clone) | In progress (teammate) |
| 3 | SymbolKind Display consistency in TOON | Done |

### Bug fixes discovered during audit

| Bug | Description | Status |
|-----|-------------|--------|
| Overflow panic | `find_containing_symbol()` u32 underflow on multi-line symbols | Fixed |
| is_primary_workspace | Compared against CWD instead of workspace root | Fixed |
| Flaky workspace_init tests | Missing `#[serial]` on env-var-mutating tests | Fixed |
| Stale dogfood test | Referenced removed FTS5 function | Fixed |

Commit: `7c18e80 fix: overflow panic in find_containing_symbol and incorrect is_primary_workspace detection`

### Priorities 4-9: Not yet started

| Priority | Tool | Status |
|----------|------|--------|
| 4 | trace_call_path | Pending |
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
