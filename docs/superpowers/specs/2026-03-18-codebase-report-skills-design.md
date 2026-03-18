# Codebase Report Skills

**Date:** 2026-03-18
**Status:** Approved (v2 — revised after spec review)

## Problem

Julie has rich codebase analysis data (centrality, test quality, change risk, security risk, call graphs, import graphs) but it's only accessible to AI agents via MCP tools. Humans have no way to get high-level reports from this data without manually querying individual tools. Meanwhile, non-technical users are increasingly shipping AI-generated code without understanding its security or quality implications.

### Key Gap Identified During Review

None of Julie's existing MCP tools can query symbols by metadata (security_risk, change_risk, test_quality, centrality). `fast_search` is text-only. `deep_dive` shows metadata for a single known symbol. `get_context` returns pivots by text relevance, not by quality scores. The skills need a way to ask "give me the top N symbols ranked by security risk" — which requires a new tool.

## Solution

Two deliverables:
1. **`query_metrics` MCP tool** — Thin query layer over existing SQLite metadata, enabling skills to find symbols by quality scores
2. **Three report skills** — SKILL.md files that use `query_metrics` + existing tools to produce formatted human-readable reports

## New MCP Tool: `query_metrics`

### Purpose

Query symbols ranked and filtered by analysis metadata. Complements `fast_search` (which finds code by text) by finding code by quality metrics.

### Parameters

```
query_metrics(
    sort_by: "security_risk" | "change_risk" | "test_coverage" | "centrality",
    order: "desc" | "asc",           // default: "desc" (worst first)
    min_risk: "low" | "medium" | "high",  // optional: filter on the sort_by metric's label
                                     // (only applies when sort_by is a risk metric; ignored otherwise)
    has_tests: bool,                  // optional: true = metadata.test_coverage exists,
                                     // false = metadata.test_coverage is absent
    kind: "function" | "class" | ..., // optional: filter by symbol kind
    file_pattern: "src/**",           // optional: scope to files
    language: "rust",                 // optional: scope to language
    exclude_tests: bool,              // optional: exclude test symbols
    limit: 20,                        // default: 20, max: 100
    workspace: "primary"              // optional: workspace selection
)
```

### Output

Returns a ranked list of symbols with their metadata scores:

```
Top 20 symbols by security_risk (descending):

1. process_user_input (src/handlers/input.rs:45)
   Security: HIGH | Change Risk: MEDIUM | Tests: none | Centrality: 0.85
   Evidence: SQL string concatenation, unsanitized input parameter

2. render_template (src/views/render.rs:112)
   Security: HIGH | Change Risk: LOW | Tests: stub | Centrality: 0.72
   Evidence: XSS sink — user content interpolated into HTML
...
```

### Implementation

SQL query against the existing symbols table + metadata JSON blobs. The data is already there — this tool just exposes it.

**Workspace routing:** Each workspace has its own SQLite file at `.julie/indexes/{workspace_id}/db/symbols.db`. The `workspace` parameter routes to the correct database connection (same pattern as all other tools). No `workspace_id` column in the query.

**File pattern filtering:** Use Rust-side `globset::Glob` matching (via `matches_glob_pattern()` in `src/tools/search/query.rs`), not SQLite GLOB — consistent with `fast_search` and supports `**` patterns.

**NULL handling:** Most symbols have no security/change risk metadata (analysis only populates symbols with signals). Use `COALESCE(json_extract(...), 0.0)` for numeric sorts and filter NULLs for label-based `min_risk` filtering.

**Evidence output:** Return raw metadata signal data (sink_calls, input_handling score, exposure score). The skill instructions tell Claude to synthesize human-readable evidence descriptions from these structured signals — that's Claude's strength, not the tool's job.

Key query shape:

```sql
SELECT s.name, s.file_path, s.line_number, s.kind,
       s.reference_score, s.metadata
FROM symbols s
WHERE (s.kind = ? OR ? IS NULL)           -- optional kind filter
ORDER BY COALESCE(json_extract(s.metadata, '$.security_risk.score'), 0.0) DESC
LIMIT ?
-- file_pattern applied post-query via Rust globset matching
```

Metadata keys on production symbols:
- `$.security_risk` — `.score` (float), `.label` (LOW/MEDIUM/HIGH), `.signals` (structured evidence)
- `$.change_risk` — `.score` (float), `.label`, `.factors`
- `$.test_coverage` — `.best_tier`, `.worst_tier`, `.test_count` (present only if symbol has tests)
- `reference_score` — direct column (float), not in metadata JSON

**Performance note:** `json_extract` on ORDER BY parses every row's JSON. Fine for typical codebases (~20K symbols). For 500K+ symbol monorepos, expression indexes on frequently-sorted paths (`CREATE INDEX ... ON symbols(json_extract(metadata, '$.security_risk.score'))`) can be added when needed.

### Dead Code Detection

For the "dead code candidates" use case, `query_metrics` with `sort_by: "centrality"` and `order: "asc"` returns symbols with the lowest reference counts. Centrality = 0.0 means zero incoming references. To exclude false positives:
- `exclude_tests: true` filters out test functions
- `kind: "function"` focuses on callable symbols
- Entry points (like `main`) will have low centrality but are recognizable by name — the skill's instructions tell Claude to filter these when formatting the report

## Report Skills

All skills follow the same architectural pattern:
1. Skill is a `SKILL.md` file with frontmatter and instructions
2. Instructions tell Claude to use `query_metrics` for ranked data and existing tools (`deep_dive`, `get_context`, `get_symbols`) for detail
3. Claude gathers the data, applies the formatting rules, and presents the report
4. Skills are user-invocable (`/skill-name`) and disabled from model auto-invocation

### Skill 1: `/codehealth [area]`

**Purpose:** Overall codebase health report — the "executive summary" of code quality.

**Query pattern:**
1. `query_metrics(sort_by="change_risk", limit=10)` — riskiest code
2. `query_metrics(sort_by="test_coverage", order="asc", limit=10)` — worst test coverage
3. `query_metrics(sort_by="centrality", order="asc", exclude_tests=true, limit=10)` — dead code candidates
4. `deep_dive` on the top 3-5 worst offenders for full context

**Expected tool calls:** ~6-10 per invocation (3 query_metrics + 3-5 deep_dives + optional get_context if area is specified).

**Report sections:**
1. **Summary** — Overall health snapshot: total symbols queried, risk distribution, test coverage overview
2. **Risk Hotspots** — Top 10 symbols by change_risk, showing centrality and test quality for each
3. **Test Gaps** — High-centrality symbols with poor or no test coverage
4. **Dead Code Candidates** — Zero-centrality symbols (excluding tests and recognizable entry points like `main`, `run`, `setup`)
5. **Recommendations** — Prioritized list of what to address first, based on risk × centrality

**Invocation:**
- `/codehealth` — whole codebase
- `/codehealth authentication` — uses `get_context` first to scope, then `query_metrics` within that scope
- `/codehealth src/tools/` — `query_metrics` with `file_pattern`

### Skill 2: `/security-audit [area]`

**Purpose:** Security-focused analysis targeting the risks of AI-generated and vibe-coded projects. Output should be understandable by someone who isn't a security expert — explain *why* something is risky, not just *that* it was flagged.

**Query pattern:**
1. `query_metrics(sort_by="security_risk", min_risk="medium", limit=30)` — all medium+ security risks
2. `query_metrics(sort_by="security_risk", has_tests=false, limit=20)` — untested security-sensitive code
3. `deep_dive` on HIGH-risk symbols for evidence details and caller context

**Expected tool calls:** ~8-15 per invocation (2 query_metrics + 5-10 deep_dives on flagged symbols).

**Report sections:**
1. **Executive Summary** — Total security signals found, severity distribution (HIGH/MEDIUM/LOW counts), overall risk level
2. **Critical Findings** — HIGH security_risk symbols grouped by category:
   - **Injection** (SQL, command, XSS) — sink detection evidence
   - **Broken Authentication** — auth-related code with risk signals
   - **Sensitive Data** — crypto usage, hardcoded secrets patterns
   - **Other** — remaining security signals
3. **Untested Security Code** — Security-sensitive symbols with no test coverage (the scariest combination: known risk + no verification)
4. **High-Exposure Risks** — Security issues in high-centrality code (widely-used vulnerable functions are worse than isolated ones)
5. **Actionable Recommendations** — For each finding: what's risky, why it matters, what to do about it. Written for non-security-experts.

### Skill 3: `/architecture [area]`

**Purpose:** Structural overview for onboarding, documentation, or understanding unfamiliar code.

**Query pattern:**
1. `get_context(query=area)` for token-budgeted pivots and neighbors
2. `query_metrics(sort_by="centrality", limit=15)` — key entry points (highest centrality = most connected)
3. `get_symbols` on key files for structure overview
4. `deep_dive` on top 3-5 entry points for caller/callee context

**Expected tool calls:** ~8-12 per invocation.

**Report sections:**
1. **Overview** — What this area/codebase does (inferred from symbol names, file structure, doc comments)
2. **Key Entry Points** — Highest-centrality public symbols with one-line descriptions
3. **Module Map** — Files grouped by responsibility, with symbol counts and key exports
4. **Dependency Flow** — How modules connect: who imports what, key call chains between entry points
5. **Suggested Reading Order** — For someone new to this code, which files to read first and why

## Skill Location

```
.claude/skills/
├── codehealth/SKILL.md
├── security-audit/SKILL.md
└── architecture/SKILL.md
```

Project-level skills — they ship with Julie and are available to anyone using Julie as their MCP server.

## Implementation Order

1. **`query_metrics` tool** — The foundation. Must exist before skills can work.
2. **`/codehealth`** — Broadest value, proves the skill pattern
3. **`/security-audit`** — Highest real-world impact
4. **`/architecture`** — Onboarding/documentation value

Each skill is independent and can be built and shipped separately after `query_metrics` is in place.

## Deferred to Phase 2

- **Complexity metrics (cyclomatic complexity)** — Add CC computation during AST extraction. Enables a `/hotspots` skill (complexity × centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 33 extractors.
- **`/hotspots` skill** — Depends on complexity metrics.
- **Function body hashing / duplication detection** — Low priority per evaluation.
- **Persistent report storage** — Reports are ephemeral (printed to terminal).
- **HTML/visual output** — Text/markdown reports only (visual reports could be a follow-up).
- **Cross-session trend tracking** — Would need persistent storage.
