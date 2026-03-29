# Dashboard: Language Distribution Bar + Per-Workspace Metrics Link

**Date:** 2026-03-29
**Status:** Approved

## Overview

Add two features to the Observatory dashboard:
1. A multicolor stacked language bar on the Projects page (compact in table row, detailed in expand panel)
2. A "View metrics" link from project detail to the pre-filtered metrics page

## Language Distribution Bar

### Data Source

Use existing `SymbolDatabase::count_files_by_language()` which returns `Vec<(String, i64)>` (language name, file count). File-based counting is the right granularity for "what's in this project."

### Compact Bar (Project Table Row)

- GitHub-style horizontal stacked bar rendered under each workspace row
- Top 5 languages get distinct colors; remaining grouped as "Other" (gray)
- Tooltip on hover: "Rust: 142 files (68%)"
- Pure HTML/CSS: stacked `<div>`s with percentage widths inside a flex container
- Populated via the `/projects/statuses` polling endpoint (language data added to the JSON response), so it live-updates alongside status badges

### Detailed Breakdown (Project Detail Panel)

- Same stacked bar but wider
- Below the bar: a legend list with color swatch, language name, file count, and percentage
- Added as a new column in the existing 3-column detail layout (or a new row below the existing columns)

### Color Map

Fixed CSS color map for common languages, following GitHub linguist conventions:

| Language | Color |
|----------|-------|
| Rust | #dea584 |
| TypeScript | #3178c6 |
| JavaScript | #f1e05a |
| Python | #3572A5 |
| Java | #b07219 |
| C# | #178600 |
| Go | #00ADD8 |
| C/C++ | #555555 |
| Ruby | #701516 |
| Swift | #F05138 |
| PHP | #4F5D95 |
| Kotlin | #A97BFF |
| HTML | #e34c26 |
| CSS | #563d7c |
| Other | #8b8b8b |

Colors defined as CSS custom properties in the dashboard stylesheet.

### Data Flow

1. `DashboardState` already holds `workspace_pool: Option<Arc<WorkspacePool>>`
2. For the detail endpoint (`/projects/{id}`): call `workspace_pool.get(workspace_id)` to get `Arc<JulieWorkspace>`, access its `SymbolDatabase`, call `count_files_by_language()`
3. For the table/statuses endpoint: same flow but for all workspaces. Since `count_files_by_language()` is a simple GROUP BY query on an indexed column, it should be fast enough without caching.

### Fallback

If WorkspacePool is unavailable (stdio mode) or the workspace isn't loaded, show no bar (graceful degradation, same as other daemon-only features).

## Per-Workspace Metrics Link

- Add a "View metrics" link/button in the project detail panel
- Links to `/metrics?workspace={workspace_id}`
- The metrics page already supports workspace filtering via query parameter

## No New Dependencies

- Colors are CSS custom properties
- Bar is pure HTML/CSS (no charting library)
- Data queries already exist in SymbolDatabase
