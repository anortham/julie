# Dashboard Intelligence Layer

**Date:** 2026-03-29
**Status:** Approved
**Goal:** Surface Julie's code understanding through a new Intelligence page and enrichments to existing dashboard pages. Optimize for "companion panel" usage: glanceable, information-dense, no deep drill-down workflows.

## Context

The dashboard currently surfaces ~30-40% of available data, showing counts and status but not the intelligence layer (centrality, relationships, kind distribution). New users are onboarding the week of 2026-03-31 and first impressions matter. Three-model consensus (Claude, Gemini, Codex) identified "Top Symbols by Centrality" as the #1 quick win and "surface interpretation, not counts" as the guiding principle.

**Constraints:**
- No JS build step (htmx + Tera + Bulma, CDN script tags OK but not needed)
- Companion panel usage pattern (glanceable, not interactive explorer)
- All data already exists in SQLite; zero schema migrations needed

---

## 1. New Intelligence Page

**Route:** `/intelligence/{workspace_id}`

A single glanceable page that answers "what did Julie understand about my codebase?" Five sections, top to bottom:

### 1.1 Codebase Fingerprint (Hero Section)

A row of 5-6 large stat cards using Bulma `.level` or `.columns.has-text-centered`:

| Stat | Source |
|------|--------|
| Files | `SELECT COUNT(*) FROM files` |
| Symbols | `SELECT COUNT(*) FROM symbols` |
| Lines of Code | `SELECT COALESCE(SUM(line_count), 0) FROM files` |
| Languages | `SELECT COUNT(DISTINCT language) FROM files` |
| References | `SELECT COUNT(*) FROM relationships` |
| Index Time | `last_index_duration_ms` from daemon DB (formatted) |

Big numbers with labels. No charts.

### 1.2 Top Symbols ("Main Characters")

Table of top 10-15 symbols ranked by `reference_score`:

| Column | Source |
|--------|--------|
| Rank | Row number |
| Name | `symbols.name` |
| Kind | `symbols.kind` (badge) |
| Language | `symbols.language` (badge) |
| File | `symbols.file_path` (relative) |
| Signature | `symbols.signature` (truncated to ~60 chars) |
| Score | `symbols.reference_score` (number or relative CSS bar) |

**New query:** `get_top_symbols_by_centrality(limit)`
```sql
SELECT name, kind, language, file_path, signature, reference_score
FROM symbols
WHERE reference_score > 0
ORDER BY reference_score DESC
LIMIT ?
```

**New struct:**
```rust
pub struct CentralitySymbol {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub signature: Option<String>,
    pub reference_score: f64,
}
```

### 1.3 Symbol Kind Distribution

Server-side SVG donut chart with legend table.

Data from existing `get_symbol_statistics()` which returns `HashMap<kind, count>`.

**SVG approach:** Handler computes arc geometry in Rust (cumulative percentages to start/end angles), passes segments to Tera template which renders `<path d="M... A...">` elements. Same technique as GitHub language bars, circular instead of linear.

**New struct for template data:**
```rust
pub struct DonutSegment {
    pub label: String,
    pub count: usize,
    pub percentage: f64,
    pub color_var: String,
    pub start_angle: f64,
    pub end_angle: f64,
}
```

Color mapping: extend the existing `lang_css_var()` pattern to symbol kinds. Function = blue, Struct/Class = green, Method = teal, Trait/Interface = purple, etc. CSS variables in the stylesheet.

Legend table alongside the donut shows kind name, count, and percentage.

### 1.4 Complexity Hotspots

Table of top 10 files ranked by composite complexity score:

| Column | Source |
|--------|--------|
| File | `files.path` |
| Language | `files.language` (badge) |
| Lines | `files.line_count` |
| Symbols | COUNT from symbols table |
| Complexity | CSS bar relative to the hottest file |

**New query:** `get_file_hotspots(limit)`
```sql
SELECT f.path, f.language, f.line_count, f.size,
       COUNT(s.id) as symbol_count
FROM files f
LEFT JOIN symbols s ON s.file_path = f.path
GROUP BY f.path
ORDER BY (f.line_count + COUNT(s.id) * 10) DESC
LIMIT ?
```

The composite score weights symbol count 10x relative to line count. A file with 50 symbols ranks equivalently to one with 500 lines. This is a tuning knob; the initial weighting can be adjusted based on how the results look in practice.

**New struct:**
```rust
pub struct FileHotspot {
    pub path: String,
    pub language: String,
    pub line_count: i32,
    pub size: i64,
    pub symbol_count: i64,
}
```

### 1.5 Story Cards

3-5 auto-generated one-liner observations, lazy-loaded via htmx (`hx-get="/intelligence/{id}/stories" hx-trigger="load"`):

- "Most referenced symbol: `Workspace::index` (score: 47.2)"
- "Largest file: `src/database/mod.rs` (847 lines, 23 symbols)"
- "Dominant language: Rust (78% of files)"
- "Most common symbol kind: Function (42%)"
- "Total references tracked: 12,847"

Pure Rust function. Takes query results, formats observations. No new DB work beyond what's already queried for sections above.

---

## 2. Enrichments to Existing Pages

### 2.1 Projects Table

**Add "Top Symbol" column:** Each workspace row shows the name of its highest-centrality symbol in monospace font. One query per workspace (reuses `get_top_symbols_by_centrality(1)`).

**Add Intelligence link:** Alongside the existing metrics link in the detail modal, add a link to `/intelligence/{workspace_id}`.

### 2.2 Projects Detail Modal

**Add Symbol Kind Breakdown:** Below the existing language distribution table, add a compact horizontal stacked bar (same CSS approach as the language bar) showing proportions of Functions, Classes/Structs, Methods, etc. with counts. Reuses `get_symbol_statistics()`.

### 2.3 Metrics Page

**Add Success Rate card:** New summary card alongside Total Tool Calls, Sessions, Tools Active, Context Saved:
- Shows percentage of tool calls where `success = true`
- Color-coded: green (>99%), yellow (95-99%), red (<95%)

**Query (on DaemonDatabase):**
```sql
SELECT COUNT(*) as total,
       SUM(CASE WHEN success THEN 1 ELSE 0 END) as succeeded
FROM tool_calls
WHERE timestamp > ?
```

### 2.4 Search Results

**Add Centrality Badge:** When a search result symbol is in the workspace's top 20 by reference_score, show a badge like "Top 5" or "Top 20". Requires fetching top-20 symbols once per search and comparing against result names.

---

## 3. Data Layer Summary

### New Queries (all on per-workspace SymbolDatabase)

| Query | Returns | Location |
|-------|---------|----------|
| `get_top_symbols_by_centrality(limit)` | `Vec<CentralitySymbol>` | `src/database/symbols/search.rs` |
| `get_file_hotspots(limit)` | `Vec<FileHotspot>` | `src/database/files.rs` or new `src/database/analytics.rs` |
| `get_aggregate_stats()` | `AggregateStats` | `src/database/helpers.rs` |

### New Query (on DaemonDatabase)

| Query | Returns | Location |
|-------|---------|----------|
| `get_tool_success_rate(since)` | `(total, succeeded)` | `src/daemon/database.rs` |

### New Structs

- `CentralitySymbol` (name, kind, language, file_path, signature, reference_score)
- `FileHotspot` (path, language, line_count, size, symbol_count)
- `AggregateStats` (total_files, total_symbols, total_lines, total_relationships, language_count)
- `DonutSegment` (label, count, percentage, color_var, start_angle, end_angle)

### No Schema Changes

All data already exists in the tables. Zero migrations.

---

## 4. Templates & Routing

### New Files

| File | Purpose |
|------|---------|
| `src/dashboard/routes/intelligence.rs` | Route handlers (index + story_cards) |
| `src/dashboard/templates/intelligence.html` | Main page template |
| `src/dashboard/templates/partials/intelligence_stories.html` | Lazy-loaded story cards partial |
| `src/dashboard/templates/partials/intelligence_donut.html` | SVG donut chart partial |

### Modified Files

| File | Change |
|------|--------|
| `src/dashboard/routes/mod.rs` | Add `pub mod intelligence` |
| `src/dashboard/mod.rs` | Register new routes |
| `src/dashboard/templates/partials/project_row.html` | Add Top Symbol column |
| `src/dashboard/templates/partials/project_detail.html` | Add kind bar + intelligence link |
| `src/dashboard/templates/metrics.html` | Add success rate card |
| `src/dashboard/templates/partials/search_results.html` | Add centrality badge |
| `src/dashboard/templates/base.html` | No top-level nav change (intelligence linked from projects) |
| CSS/static | Add symbol kind color variables |

### Router Registration

```rust
.route("/intelligence/{workspace_id}", get(routes::intelligence::index))
.route("/intelligence/{workspace_id}/stories", get(routes::intelligence::story_cards))
```

---

## 5. Out of Scope

- Call graph explorer (too interactive for companion panel)
- Architecture map / semantic clusters (requires embedding clustering)
- Historical trends / sparklines (insufficient snapshot history)
- Chart.js or any CDN charting dependency
- Real-time updates on intelligence page (snapshot view, not live-polled)
- New top-level navigation items (intelligence linked from projects page)
