# Phase 2: Tool Categorization — Complete

**Date:** 2026-02-06
**Status:** Approved
**Depends on:** Phase 1 (Data Pipeline Audit) — Complete

## Competitive Landscape (Feb 2026)

### Claude Code Native LSP (v2.0.74, Dec 2025)
- Ships `lsp_go_to_definition`, `lsp_find_references`, `lsp_get_diagnostics`
- Directly overlaps with Julie's `fast_goto` and `fast_refs`
- Young, limited language breadth, unclear polyglot support

### LSP Advantages Over Julie
- Type-aware navigation (resolves through generics, traits, virtual dispatch)
- Real-time diagnostics (compiler-grade errors)
- Compiler-backed rename (guaranteed correct)

### Julie Advantages Over LSP
- Cross-language call tracing (no LSP can do this)
- Codebase-wide full-text search with code-aware tokenization
- Business logic discovery by domain keywords
- Symbol-by-name navigation (agents think in names, not file:line:col)
- 30 languages, zero configuration, single binary
- Persistent index (no cold start, no language server to install)
- Multi-workspace / reference workspace querying
- Memory system (checkpoint/recall/plan) — novel category
- Agent-native output (TOON format, token-efficient)

### Honest Gap
Julie has no diagnostics, no type-aware navigation, no compiler integration. We don't compete on type-system features — we compete on breadth, speed, and agent-native capabilities.

---

## 4-Bucket Categorization

### Bucket A — Table-stakes (LSP overlap, differentiated)

| Tool | LSP Equivalent | Julie's Differentiation |
|------|---------------|------------------------|
| `fast_goto` | `lsp_go_to_definition` | Name-based (not position-based), cross-workspace, 30 langs, 3-stage CASCADE, no cold start |
| `fast_refs` | `lsp_find_references` | Reference kind filtering, cross-workspace, identifier-based resolution potential |
| `rename_symbol` | `lsp_rename` | Workspace-wide, graph-aware, dry-run preview |

**Strategy:** Keep sharp, differentiate on agent ergonomics and polyglot support. Don't compete on type-awareness.

### Bucket B — Moat (LSP fundamentally can't do this)

| Tool | Why it's moat |
|------|--------------|
| `fast_search` | Codebase-wide full-text + code-aware tokenization. No LSP provides this. |
| `get_symbols` | File structure with code bodies, multiple verbosity modes, TOON output. LSP document symbols is inferior. |
| `trace_call_path` | Cross-language execution tracing. Unique capability. |
| `fast_explore` | Business logic discovery, dependency analysis, type exploration. No LSP equivalent. |
| `checkpoint` / `recall` / `plan` | Persistent dev memory. Completely novel category. |

**Strategy:** Heavy investment. These ARE Julie's reason to exist.

### Bucket C — Evaluate (keep/kill/consolidate)

| Tool | Question | Preliminary Decision |
|------|----------|---------------------|
| `find_logic` | Already deprecated | **Kill** — redirect to `fast_explore(mode='logic')` |
| `edit_lines` | Agents rarely use it | Investigate during deep-dive: discoverability or value problem? |
| `fuzzy_replace` | Same underuse pattern | Same investigation |

### Bucket D — Enhance (works, sitting on untapped data)

| Tool | Untapped Data Source | Enhancement Opportunity |
|------|---------------------|------------------------|
| `fast_refs` | Identifiers table (~1.2M records/workspace) | Precise "all usages" not just relationship-based refs |
| `fast_explore(types)` | Types table (~10% exposed) | "Find all implementations of trait X", type hierarchy queries |
| `trace_call_path` | Relationship kinds (no filtering) | Filter call sites vs type usages vs imports — reduce noise |
| `edit_symbol` | Symbol metadata (async/abstract/decorator) | Context-aware editing |
| `fast_search` | Tantivy capabilities | Fuzzy matching, phrase search, doc comment search, signature search |

---

## Data Utilization Scorecard

From Phase 1 gap analysis:

| Data Category | Extracted | Stored | Indexed (Tantivy) | Queried by Tools | Utilization |
|--------------|-----------|--------|-------------------|------------------|-------------|
| Symbols | ✅ 30 langs | ✅ SQLite | ✅ Full | ✅ All tools | **95%** |
| Identifiers | ✅ 30 langs | ✅ SQLite | ❌ None | ❌ None | **0%** |
| Types | ✅ 8+ langs | ✅ SQLite | ❌ None | ⚠️ ~10% (fast_explore) | **10%** |
| Relationships | ✅ 30 langs | ✅ SQLite | ❌ None | ⚠️ ~30% (trace, explore) | **30%** |
| Symbol metadata | ✅ Partial | ✅ JSON blob | ❌ None | ❌ None | **0%** |
| Visibility | ✅ Most langs | ✅ SQLite | ❌ None | ❌ None | **0%** |
| Confidence scores | ✅ All | ✅ SQLite | ❌ None | ❌ None | **0%** |

**Overall utilization: ~35%** — We're leaving 65% of our extracted intelligence on the table.

---

## Phase 3 Priority Order (Approved)

| Priority | Tool | Bucket | Key Focus | Est. Sessions |
|----------|------|--------|-----------|---------------|
| 1 | `fast_search` | B+D | Unused Tantivy capabilities (fuzzy, phrase, doc comment). Most-used discovery tool. | 1-2 |
| 2 | `fast_refs` | A+D | **Unlock 1.2M identifiers.** Transform from relationship-based to LSP-quality find-all-usages. | 1-2 |
| 3 | `get_symbols` | B | Output optimization, semantic remnant cleanup. Already strong. | 1 |
| 4 | `trace_call_path` | B+D | Relationship kind filtering. Reduce noise, improve precision. | 1 |
| 5 | `fast_explore` | B+D | Types table utilization (~10% → higher). Metadata for business logic. | 1-2 |
| 6 | `fast_goto` | A | Quick audit. Probably already solid (3-stage CASCADE). | 1 |
| 7 | Editing tools | C | Investigate underuse. Keep/kill/consolidate decision. | 1 |
| 8 | Memory tools | B | Recently overhauled. Quick completeness audit. | 1 |
| 9 | `manage_workspace` | — | Infrastructure audit. Health check accuracy. | 0.5 |
| — | `find_logic` | C | **Kill** during cleanup. Redirect to `fast_explore(mode='logic')`. | 0 |

**Total: ~9-11 sessions**

### Strategic Bet
Priorities #1-2 are both search-related and data-unlock heavy. Nailing `fast_search` + `fast_refs` with the untapped identifiers data takes Julie's search from "good" to "best-in-class for agents."

---

## Bugs Fixed Before Phase 2

| Task | Severity | Fix |
|------|----------|-----|
| #11: File watcher drops identifiers/types/relationships/Tantivy | HIGH | Rewired handler to use `extract_all()` + `incremental_update_atomic()` + Tantivy updates |
| #12: Tantivy search false positives from token splitting | MEDIUM | AND-per-term query logic + `filter_compound_tokens` helper |

---

## Success Criteria for Phase 2
✅ All 15 tools categorized into 4 buckets
✅ Competitive landscape documented (Claude Code native LSP)
✅ Data utilization scorecard created
✅ Phase 3 priority order approved
✅ Kill decision on `find_logic`
✅ Design document committed
