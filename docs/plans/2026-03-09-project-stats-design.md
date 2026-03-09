# Project Stats/Insights — Design

**Date:** 2026-03-09
**Scope:** Add per-project stats with expandable row in Projects table

## Problem

The Projects page shows basic info (name, path, status, symbol/file counts) but no language breakdown, symbol kind distribution, or index health details. Users can't get a quick read on what's in each project.

## Design

### Backend

**New endpoint:** `GET /api/projects/:id/stats`

**Response type** `ProjectStatsResponse`:
```rust
pub struct ProjectStatsResponse {
    pub total_symbols: i64,
    pub total_files: i64,
    pub total_relationships: i64,
    pub db_size_mb: f64,
    pub embedding_count: i64,
    pub languages: Vec<LanguageCount>,    // sorted by file_count desc
    pub symbol_kinds: Vec<SymbolKindCount>, // sorted by count desc
}

pub struct LanguageCount {
    pub language: String,
    pub file_count: i64,
}

pub struct SymbolKindCount {
    pub kind: String,
    pub count: i64,
}
```

**New database methods** on `Database` (in `src/database/helpers.rs`):
- `count_files_by_language() -> Result<Vec<(String, i64)>>` — `SELECT language, COUNT(*) FROM files GROUP BY language ORDER BY COUNT(*) DESC`
- `count_symbols_by_kind() -> Result<Vec<(String, i64)>>` — `SELECT kind, COUNT(*) FROM symbols GROUP BY kind ORDER BY COUNT(*) DESC`

The existing `get_stats()` provides `total_symbols`, `total_files`, `total_relationships`, `db_size_mb`, `embedding_count`.

### Frontend

**Expandable row** in Projects table:
- Click a project row to toggle expand/collapse
- Stats fetched on-demand from `/api/projects/:id/stats` when expanded (not pre-loaded)
- Loading spinner while fetching

**Expanded panel contents:**
1. **Language breakdown** — horizontal stacked bar with color legend, showing file count per language
2. **Symbol kinds** — compact chips/badges showing kind → count (e.g. "function: 1,234", "struct: 89")
3. **Index stats line** — relationships count, db size, embedding count, last indexed timestamp

### Files to modify
- `src/database/helpers.rs` — add `count_files_by_language()` and `count_symbols_by_kind()`
- `src/api/projects.rs` — add `get_project_stats` endpoint + `ProjectStatsResponse`, `LanguageCount`, `SymbolKindCount` types
- `src/api/mod.rs` — register route `.route("/projects/{id}/stats", get(projects::get_project_stats))`
- `ui/src/views/Projects.vue` — expandable row with stats panel, on-demand fetch, language bar, kind chips

## Acceptance Criteria

- [ ] `count_files_by_language()` returns language → file count pairs sorted by count desc
- [ ] `count_symbols_by_kind()` returns kind → count pairs sorted by count desc
- [ ] `GET /api/projects/:id/stats` returns full stats for a ready project
- [ ] `GET /api/projects/:id/stats` returns 404 for unknown project, appropriate error for non-ready
- [ ] Projects table rows are clickable to expand/collapse
- [ ] Expanded row shows language breakdown as a horizontal stacked bar with legend
- [ ] Expanded row shows symbol kinds as chips with counts
- [ ] Expanded row shows index stats (relationships, db size, embeddings)
- [ ] Stats are fetched on-demand (not pre-loaded for all projects)
- [ ] All existing tests pass (no regressions)
