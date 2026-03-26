# Daemon Dashboard Design Spec

**Date**: 2026-03-26
**Status**: Approved
**Version**: Julie v6.0.8+

## Summary

A web dashboard served by the Julie daemon, providing observability into workspaces, tool usage metrics, search quality debugging, and system health. Opens automatically in the browser when the daemon starts.

## Goals

- At-a-glance daemon health and workspace status
- Tool call analytics with live activity streaming
- Search quality debugging for dogfooding (tokenization, scoring breakdown)
- Recent error/warning visibility without leaving the browser
- Zero JS build step; Rust-only toolchain

## Non-Goals

- Agent dispatch (removed from v4 scope)
- Goldfish/memory integration (separate project if needed)
- Editor/terminal launcher buttons (too much per-user customization)
- Full log viewer (deferred; recent errors + log file path is sufficient)

---

## Stack

| Layer | Technology | Notes |
|-------|-----------|-------|
| HTTP server | Axum 0.8 | Already declared in Cargo.toml, runs alongside IPC listener |
| Templates | Tera | Server-side rendering, embedded or disk-loaded |
| CSS framework | Bulma | CDN initially, can vendor later. Dark theme via overrides |
| Interactivity | htmx 2.x | Server-driven partials, SSE extension for live updates |
| Client state | Alpine.js | Expand/collapse, toggle filters, theme persistence |
| Charts | Chart.js | Metrics trend chart only; loaded via CDN |
| Asset embedding | rust-embed | Bakes `dashboard/` into binary for release builds |
| Dev mode | Disk loading | When `dashboard/templates/` exists on disk, load from there instead of embedded. Save + refresh workflow, no recompile for template/CSS changes |

### CDN Dependencies (all loaded in base.html)

- Bulma CSS (~230KB)
- htmx (~14KB)
- Alpine.js (~15KB)
- Chart.js (~65KB, metrics page only)
- No npm, no node_modules, no build step

---

## File Layout

### Frontend (templates + static assets)

```
dashboard/
├── templates/
│   ├── base.html              # Shell: nav, theme toggle, CDN imports
│   ├── status.html            # System status landing page
│   ├── projects.html          # Workspace list
│   ├── metrics.html           # Tool call analytics
│   ├── search.html            # Search playground
│   └── partials/
│       ├── status_cards.html       # Hero stats (SSE target)
│       ├── services_panel.html     # Service health list
│       ├── error_feed.html         # Recent errors (SSE target)
│       ├── project_row.html        # Single project table row
│       ├── project_detail.html     # Expanded project detail
│       ├── metrics_summary.html    # Summary cards (SSE target)
│       ├── metrics_table.html      # Per-tool breakdown table
│       ├── activity_feed.html      # Live tool call feed (SSE target)
│       ├── search_results.html     # Search results list
│       └── search_detail.html      # Expanded result with scoring
├── static/
│   ├── app.css                # Custom styles, dark mode, branding
│   └── app.js                 # Theme toggle persistence, SSE helpers
```

### Backend (Rust)

```
src/dashboard/
├── mod.rs                  # Router, static/template serving, dev-mode detection
├── state.rs                # DashboardState: refs to Arc types + error ring buffer
├── error_buffer.rs         # Tracing subscriber layer -> VecDeque<LogEntry>
├── routes/
│   ├── status.rs           # GET / -- renders status.html + partials
│   ├── projects.rs         # GET /projects, GET /projects/:id/detail (partial)
│   ├── metrics.rs          # GET /metrics, GET /metrics/table (partial)
│   ├── search.rs           # GET /search, POST /search (renders results partial)
│   └── events.rs           # GET /events/status -- SSE for status page
│                           # GET /events/metrics -- SSE for metrics page
│                           # GET /events/activity -- SSE for live tool call feed
```

---

## Port & Browser Open

### Port Selection

1. Try binding to port 7890
2. If taken (EADDRINUSE), bind to port 0 (OS-assigned)
3. Write actual port to `~/.julie/daemon.port` (or `{julie_home}/daemon.port`)
4. The port file is the source of truth for CLI commands and browser-open

### Auto-Open

After the HTTP server binds successfully:
1. Read the bound port
2. Unless `--no-dashboard` flag is set (or config `dashboard.auto_open = false`):
   - macOS: `open http://localhost:{port}`
   - Linux: `xdg-open http://localhost:{port}`
   - Windows: `start http://localhost:{port}`
3. Log the URL regardless of auto-open setting

### CLI Integration

`julie dashboard` command: reads `~/.julie/daemon.port`, opens the URL in the default browser. Useful when auto-open is disabled or the user closed the tab.

---

## Views

### 1. System Status (Landing Page)

**Route**: `GET /`

**Layout**:
- Top stats row (4 cards): Health status, Uptime, Active Sessions, Workspace count
- Two-column body:
  - Left: Services panel (Tantivy, Embedding Service, File Watchers, Binary Status, Database)
  - Right: Recent Errors & Warnings (last 50, color-coded by level, log file path link)

**Live updates**: SSE on `GET /events/status`
- Status cards refresh every 5s (uptime, session count)
- Error feed appends new entries as they arrive
- Binary status updates when rebuild detected

**Data sources**:
- `SessionTracker::active_count()` for sessions
- `WorkspacePool::active_count()` for workspaces
- `EmbeddingService` health status
- `restart_pending` AtomicBool for binary staleness
- Error ring buffer (new, see Error Buffer section)
- Daemon start time for uptime calculation

### 2. Projects

**Route**: `GET /projects`

**Layout**:
- Summary bar: workspace counts by status, "Refresh All" button
- Table with columns: expand chevron, project name, path, symbols, files, vectors, status badge
- Expandable detail panel (fetched via htmx partial):
  - Language breakdown with progress bars
  - Index stats: relationships, DB size, Tantivy index size, embedding model, last indexed
  - Code health: security risk, change risk, test coverage, avg/max centrality
  - Reference workspace tags

**Interactions**:
- Click row to expand (Alpine.js toggle + htmx `GET /projects/:id/detail`)
- "Refresh All" triggers re-index via htmx POST
- Status badges: Ready (green), Indexing (amber), Error (red), Registered (gray)

**Data sources**:
- `DaemonDatabase::list_workspaces()` for the table
- Per-workspace `SymbolDatabase` queries for expanded detail (language breakdown, symbol kinds, relationships)
- `DaemonDatabase::get_snapshot_history()` for code health
- `DaemonDatabase::list_references()` for reference workspace tags

### 3. Metrics

**Route**: `GET /metrics`

**Layout**:
- Filter bar: workspace selector, time range toggle (24h/7d/30d/90d)
- Summary cards (4): Total tool calls, Sessions, Median duration, p95 duration. Each with trend indicator vs prior period.
- Two-column body:
  - Left (wider): Tool breakdown table (all 8 tools, columns: calls, median, p95, volume bar)
  - Right: Live Activity feed (SSE-driven, newest on top, duration color-coded)
- Bottom: Call Volume Trend line chart (Chart.js)

**Live updates**:
- `GET /events/metrics` SSE for summary card refresh
- `GET /events/activity` SSE for the live tool call feed (each tool call event pushed as it completes)

**Interactions**:
- Workspace/time range filters trigger htmx partial reload of the page body
- Chart.js trend chart re-renders on filter change

**Data sources**:
- `DaemonDatabase::query_tool_call_history()` for all aggregations
- `SessionMetrics` (in-memory atomics) for current-session live data
- Tool call events emitted when each MCP tool completes (new broadcast channel, see SSE Infrastructure)

### 4. Search Playground

**Route**: `GET /search`

**Layout**:
- Search bar with submit button
- Filter row: workspace selector, search target toggle (definitions/content), language dropdown, file pattern input, debug mode toggle
- When debug mode ON:
  - Tokenization panel: raw query, tokenized forms (including stems), search mode (keyword/NL), result count, query time
  - Results with expandable scoring detail
- When debug mode OFF:
  - Results only, no scoring info

**Result rows** (collapsed): rank number, kind badge (color-coded), symbol name, file:line, score
**Result rows** (expanded, debug mode): 3-column scoring breakdown:
  - Column 1: BM25 base, centrality boost (with raw centrality score), pattern boost (with type), final score
  - Column 2: Field matches (name, qualified_name, content, path) with match/exact indicators
  - Column 3: Symbol metadata (kind, language, visibility, reference count)
  - Code preview with syntax-highlighted lines

**Interactions**:
- Search form submits via htmx POST, replaces results area
- Filter changes included in form submission
- Result expand/collapse via Alpine.js + htmx partial for scoring detail

**Data sources**:
- Same search pipeline as `fast_search` MCP tool
- Debug info exposed via an internal `search_with_debug()` function in the search pipeline (likely `src/tools/search/`) that returns per-result scoring breakdown (BM25, centrality boost, pattern boost, field matches). This is a new function that wraps the existing search logic and captures intermediate scoring values that are normally discarded after ranking.
- Tokenization info from the query analysis stage (the tokenizer already runs; we just need to capture and return its output alongside results)

---

## Error Ring Buffer

A new tracing subscriber layer that captures warn/error log entries into a bounded ring buffer.

### Implementation

```
src/dashboard/error_buffer.rs
```

- `ErrorBuffer`: wraps a `Mutex<VecDeque<LogEntry>>` with capacity 50
- `LogEntry`: timestamp, level (WARN/ERROR), message, optional target/span context
- Implements `tracing_subscriber::Layer` to intercept warn/error events
- Added to the tracing subscriber stack in daemon startup (composable with existing file appender)
- Exposes `recent_entries()` -> `Vec<LogEntry>` for the status page
- New entries also pushed to the SSE broadcast for live updates

### Why a ring buffer and not file parsing

- Zero file I/O for the dashboard hot path
- Structured data from the start (no regex parsing of text logs)
- Bounded memory (50 entries, fixed)
- Instant access, no seeking/tailing

---

## SSE Infrastructure

### Broadcast Channel

A `tokio::sync::broadcast` channel shared via `DashboardState`. Three event types:

1. **Status events**: session connect/disconnect, binary staleness change, daemon health changes
2. **Metrics events**: summary card refreshes (periodic, every 5s)
3. **Activity events**: individual tool call completions (tool name, workspace, duration, timestamp)

Tool call events are emitted from the MCP handler layer when a tool completes. This requires a small addition: the handler sends a `ToolCallEvent` to the broadcast channel after recording to DaemonDatabase.

### SSE Endpoints

Each endpoint subscribes to the broadcast channel and filters to relevant events:

- `GET /events/status` -- status events + error buffer entries
- `GET /events/metrics` -- metrics summary refreshes
- `GET /events/activity` -- individual tool call events

htmx SSE extension on the frontend subscribes and swaps partials:
```html
<div hx-ext="sse" sse-connect="/events/status" sse-swap="error">
  <!-- error feed partial gets swapped in here -->
</div>
```

---

## Dev Mode

### Detection

In `src/dashboard/mod.rs`, on startup:
1. Check if `dashboard/templates/` exists relative to CWD
2. If yes: load Tera templates from disk, serve static files from `dashboard/static/`
3. If no: serve everything from `rust-embed` compiled assets

### Dev Workflow

Template/CSS change:
1. Edit `dashboard/templates/*.html` or `dashboard/static/app.css`
2. Refresh browser
3. See changes (Tera reloads templates from disk on each request in dev mode)

Rust API change:
1. Edit `src/dashboard/routes/*.rs`
2. `cargo build` + restart daemon
3. Refresh browser

Both changed:
1. `cargo build` + restart daemon
2. Refresh browser

No npm. No `npm run build`. No stale UI trap.

---

## Phasing

Designed for agent team parallelization. Phase 0 is the foundation; Phases 1-4 are independent views that can be built in parallel after Phase 0 completes.

### Phase 0: Foundation

**Must complete first. All other phases depend on this.**

- Axum HTTP server startup alongside IPC listener in `run_daemon`
- Port selection logic (try 7890, fallback to auto, write to file)
- Auto-open browser behavior with `--no-dashboard` opt-out
- `DashboardState` struct with refs to existing Arc types
- Tera template engine with dev-mode disk loading / release-mode rust-embed
- `base.html` shell (nav bar, theme toggle, Bulma + htmx + Alpine CDN imports)
- Static file serving (`app.css`, `app.js`)
- Dark theme CSS (Bulma overrides + custom properties)
- `ErrorBuffer` tracing layer + ring buffer
- SSE broadcast channel setup
- `julie dashboard` CLI command

**Estimated scope**: ~500-700 lines of Rust, ~200 lines of HTML/CSS

### Phase 1: System Status View

**Independent after Phase 0.**

- `GET /` route rendering `status.html`
- Status cards partial (health, uptime, sessions, workspaces)
- Services panel partial (Tantivy, embeddings, watchers, binary status, DB)
- Error feed partial (reads from ErrorBuffer)
- SSE endpoint `GET /events/status` wired to broadcast channel
- htmx SSE integration on the page

**Data sources used**: SessionTracker, WorkspacePool, EmbeddingService, restart_pending, ErrorBuffer, daemon start time

### Phase 2: Projects View

**Independent after Phase 0.**

- `GET /projects` route rendering `projects.html`
- Project table with summary bar
- `GET /projects/:id/detail` partial endpoint for expandable rows
- Language breakdown, index stats, code health, reference tags in detail
- Refresh All button (POST to trigger re-index)

**Data sources used**: DaemonDatabase (list_workspaces, get_snapshot_history, list_references), per-workspace SymbolDatabase

### Phase 3: Metrics View

**Independent after Phase 0.**

- `GET /metrics` route rendering `metrics.html`
- Summary cards with trend calculation
- Tool breakdown table
- Live activity feed with SSE
- Chart.js trend chart
- Workspace/time range filters (htmx partial reload)
- `GET /events/activity` SSE endpoint
- Tool call event emission from MCP handler (broadcast channel send)

**Data sources used**: DaemonDatabase (query_tool_call_history), SessionMetrics, broadcast channel

### Phase 4: Search Playground

**Independent after Phase 0.**

- `GET /search` route rendering `search.html`
- `POST /search` route returning results partial
- Debug mode toggle (tokenization panel + scoring breakdown)
- `search_with_debug()` internal API exposing scoring details
- Expandable result rows with scoring detail partial
- Kind badges, code preview, field match display

**Data sources used**: Search pipeline (same as fast_search tool), query tokenizer

---

## Theme & Visual Direction

- **Dark by default**, light mode available via toggle (persisted to localStorage)
- **Color palette**: Indigo/slate family. Primary: `#818cf8`, darker shades for depth. Not a copy of v4, but same aesthetic neighborhood.
- **Status colors**: Green (#4ade80) for healthy/success, Amber (#fbbf24) for warning/in-progress, Red (#ef4444) for error
- **Typography**: System font stack (`-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif`). Monospace for code, paths, and technical values (`'SF Mono', 'Fira Code', monospace`).
- **Cards**: Rounded corners (8px), subtle background differentiation from page background
- **Table rows**: Alternating subtle background shading, hover highlight
- Bulma provides the structural components (navbar, table, columns, tags, progress bars); custom CSS handles the dark theme and branding colors

---

## Dependencies (Rust crates)

| Crate | Purpose | Status |
|-------|---------|--------|
| axum 0.8 | HTTP server | Already in Cargo.toml |
| tera | Template engine | New dependency |
| rust-embed | Embed dashboard/ into binary | New dependency (was in v4) |
| tokio::sync::broadcast | SSE event distribution | Already available (tokio feature) |

No new heavyweight dependencies. Tera and rust-embed are small, well-maintained crates.

---

## Open Questions (Resolved)

These came up during brainstorming and were resolved:

1. **Vue vs htmx?** -- htmx. No JS build step, single toolchain, dev loop is just save+refresh. Vue's SPA interactivity isn't needed for a dashboard.
2. **Full log viewer?** -- No. Recent errors in ring buffer + log file path. Can graduate to a full viewer later.
3. **PrimeVue replacement?** -- Bulma CSS. Clean components out of the box, no JS dependency.
4. **Port strategy?** -- Default 7890 with auto-fallback. Port written to file.
5. **SSE vs WebSocket?** -- SSE. Dashboard is mostly one-way data push. Filter changes use regular REST.
6. **Future visualizations (graphs, charts)?** -- Not blocked by htmx. D3/Cytoscape/Chart.js are all standalone libs that render into a DOM element regardless of what put it on the page.
