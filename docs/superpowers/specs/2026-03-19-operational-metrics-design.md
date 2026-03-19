# Operational Metrics for Julie

**Date:** 2026-03-19
**Status:** Approved
**Category:** Observability / User Value

## Problem

Julie does significant work behind the scenes (searching 48K+ symbols, distilling multi-file results, graph traversal) but none of this is visible to users. Users have no way to know whether Julie is helping, hurting, or doing nothing. Transparency builds trust.

There are two audiences:

1. **Developer (us)** - operational health: latencies, error rates, tool usage patterns
2. **End users (AI agents and their operators)** - confidence that Julie is delivering value, not a black box

## Design Principles

- **Facts, not claims.** Every metric is backed by measured data. No counterfactual "Julie saved you X" guesses.
- **Bytes in, bytes out.** Source file sizes and output sizes are both in bytes, giving apples-to-apples compression ratios.
- **Collection is nearly free.** Per-call overhead is ~150ns for atomic counter bumps plus one lightweight SQLite INSERT (~0.02ms via `tokio::spawn_blocking`). No tool call response is ever delayed by metrics collection.
- **Disposable with `.julie/`.** Metrics live in the workspace SQLite database. Delete `.julie/`, start fresh. That's the contract.
- **No telemetry home.** All data stays local. Period.

## Approach: Per-Call SQLite Instrumentation

Hybrid architecture: in-memory atomic counters for session stats (zero I/O per call), with per-call records written to SQLite for historical queries.

## Architecture

### Collection Layer

A thin instrumentation wrapper in `handler.rs` around every tool's `call_tool` dispatch. Each tool's `call_tool` method returns a `(CallToolResult, ToolCallReport)` tuple, where `ToolCallReport` contains both input and output metadata:

```rust
/// Metrics captured from inside a tool's call_tool method.
/// Built where both inputs and outputs are available.
pub struct ToolCallReport {
    pub result_count: Option<u32>,
    pub source_bytes: Option<u64>,  // total size of source files examined
    pub output_bytes: u64,          // response string .len() (UTF-8 bytes)
    pub metadata: serde_json::Value, // tool-specific data
}
```

The handler wrapper adds timing on top:

```rust
async fn fast_search(&self, Parameters(params): Parameters<FastSearchTool>) -> Result<CallToolResult, McpError> {
    let start = Instant::now();
    let (result, report) = params.call_tool_with_metrics(self).await
        .map_err(|e| McpError::internal_error(format!("fast_search failed: {}", e), None))?;
    let duration = start.elapsed();

    // Fire-and-forget: bumps atomics synchronously, spawns SQLite write
    self.record_tool_call("fast_search", duration, &report);

    Ok(result)
}
```

Each tool builds its `ToolCallReport` inside `call_tool_with_metrics` where it has access to both the input params and the computed results. Metadata examples:

- **fast_search:** `{"query": "UserService", "target": "definitions", "results": 5, "relaxed": false}`
- **get_context:** `{"query": "payment processing", "pivots": 3, "neighbors": 12, "token_budget": 3000}`
- **deep_dive:** `{"symbol": "process_payment", "depth": "context", "callers": 4, "callees": 7}`
- **fast_refs:** `{"symbol": "Handler", "definitions": 1, "references": 14, "semantic_fallback": false}`
- **get_symbols:** `{"file": "src/handler.rs", "mode": "structure", "target": "fast_search"}`
- **rename_symbol:** `{"old": "foo", "new": "bar", "files_modified": 3, "changes": 12, "dry_run": true}`
- **manage_workspace:** `{"operation": "index", "files_processed": 1247, "symbols_added": 48321}`
- **query_metrics:** `{"category": "session"}`

#### source_bytes collection

`source_bytes` requires knowing the total size of files that contributed to the result. This is NOT free for all tools since file sizes come from the `files` table, not from Tantivy search results. Each tool populates `source_bytes` as follows:

- **get_symbols:** File size is looked up as part of the existing file read. Zero extra cost.
- **fast_search (definitions):** Results include file paths from Tantivy. One batch query: `SELECT SUM(size) FROM files WHERE path IN (...)` for matched files. ~0.1ms.
- **fast_search (content/line mode):** Same batch query approach using matched file paths.
- **deep_dive:** Symbol queries already touch the `symbols` table which has `file_path`. One batch file size query. ~0.1ms.
- **fast_refs:** Same pattern: result file paths feed a batch size query.
- **get_context:** Pivot and neighbor file paths feed a batch size query.
- **rename_symbol:** Modified file paths are known. Batch size query.
- **manage_workspace, query_metrics:** `source_bytes` is `None` (not read operations).

The ~0.1ms batch query is the real cost, not the 150ns atomic bumps. Still negligible against 2-50ms tool call durations.

`record_tool_call` does two things:
1. Bumps in-memory atomic session counters (synchronous, ~50ns)
2. Spawns a `tokio::spawn_blocking` task to INSERT into the `tool_calls` table (~0.02ms)

### Storage Layer

#### New table: `tool_calls` (migration v13)

```sql
CREATE TABLE tool_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    tool_name TEXT NOT NULL,
    duration_ms REAL NOT NULL,
    result_count INTEGER,
    source_bytes INTEGER,
    output_bytes INTEGER,
    success INTEGER NOT NULL DEFAULT 1,
    metadata TEXT
);

CREATE INDEX idx_tool_calls_timestamp ON tool_calls(timestamp);
CREATE INDEX idx_tool_calls_tool_name ON tool_calls(tool_name);
CREATE INDEX idx_tool_calls_session ON tool_calls(session_id);
```

Key columns:
- `session_id`: UUID generated at server startup. Enables session counting in history queries.
- `source_bytes`: total size of source files examined (via batch file size query, see Collection Layer).
- `output_bytes`: byte length of the response string (`.len()` on the UTF-8 String, which gives bytes).
- `metadata`: JSON blob for tool-specific data (avoids per-metric schema columns).

#### Altered table: `files` (migration v13)

```sql
ALTER TABLE files ADD COLUMN line_count INTEGER DEFAULT 0;
```

Populated during indexing via `content.lines().count()`. Backfilled on next re-index. This is not used by metrics aggregation, but enriches future tool output (e.g., get_symbols could show "45 of 1,200 lines").

#### Data lifecycle

- Deleted when `.julie/` is deleted. Same contract as all other Julie data.
- No retention policy needed. At ~200 bytes/row, 300 calls/day = ~60KB/day.
- If ever needed: `DELETE FROM tool_calls WHERE timestamp < ?`

### Query Layer

`query_metrics` gains a `category` parameter (defaults to `"code_health"` for backward compatibility):

**Parameter scoping:** The existing parameters (`sort_by`, `order`, `min_risk`, `has_tests`, `kind`, `file_pattern`, `language`, `exclude_tests`, `limit`) apply ONLY to `category: "code_health"`. When `category` is `"session"` or `"history"`, these parameters are silently ignored. Session and history categories return fixed-format summaries.

#### `category: "code_health"` (existing, unchanged)

All current functionality: security risk, change risk, centrality, test coverage queries.

#### `category: "session"`

Current session stats from in-memory counters. No SQLite read required.

Output:
```
Session Metrics (uptime: 47m)

Tool Usage:
  fast_search    22 calls   avg 4.1ms   output: 18.2KB
  deep_dive      10 calls   avg 8.3ms   output: 9.0KB
  get_symbols     8 calls   avg 2.1ms   output: 3.0KB
  fast_refs       6 calls   avg 5.7ms   output: 4.8KB
  get_context     2 calls   avg 23.4ms  output: 6.2KB

Totals: 48 calls | avg 5.8ms

Context Efficiency:
  Source files examined: 214KB across 67 files
  Output returned: 8.2KB
  NOT injected into context: 205.8KB

Workspace: 1,247 files | 48,321 symbols
```

The headline: **"X MB of source code was NOT injected into your context this session because of Julie."** This is `SUM(source_bytes) - SUM(output_bytes)`, pure arithmetic on measured data.

#### `category: "history"`

Aggregated from the `tool_calls` SQLite table. Defaults to last 7 days. Supports time filtering via a `since` parameter (e.g., `"3d"`, `"24h"`).

Output:
```
Historical Metrics (last 7 days, 12 sessions)

Tool Performance:
  fast_search    347 calls   avg 3.8ms   p95 12.1ms
  deep_dive      142 calls   avg 7.9ms   p95 18.3ms
  get_context     38 calls   avg 21.2ms  p95 45.0ms

Context Efficiency (cumulative):
  Source examined: 4.7MB
  Output returned: 187KB
  NOT injected into context: 4.5MB
```

P95 is computed in Rust after fetching duration values. With a 7-day default window (~2,100 rows at 300/day), this is trivial. The `since` parameter caps the query range.

### Presentation Layer (Skill)

A `/metrics` skill that:
1. Calls `query_metrics(category: "session")` for current session data
2. Optionally calls `query_metrics(category: "history")` for trends
3. Formats into a narrative with the "NOT injected" headline

The skill presents facts. No value claims, no "time saved" estimates. The compression ratio speaks for itself.

Example skill output:
```
Julie has been running for 47 minutes this session.

205.8KB of source code was NOT injected into your context this session.
Julie examined 214KB of source across 67 files and returned 8.2KB of
targeted results (3.8% of source).

48 tool calls, average response time 5.8ms. Most-used: fast_search (22 calls).
```

Skill details (file location, parameters, error handling) are deferred to the implementation plan.

### In-Memory Session State

Added to `JulieServerHandler` as `Arc<SessionMetrics>` (handler derives Clone; Arc ensures clones share the same counters):

```rust
pub struct SessionMetrics {
    pub session_id: String,             // UUID, generated once at startup
    pub session_start: Instant,
    pub total_calls: AtomicU64,
    pub total_duration_us: AtomicU64,   // microseconds for sub-ms precision
    pub total_source_bytes: AtomicU64,
    pub total_output_bytes: AtomicU64,
    /// Per-tool counters. Fixed-size array since tool set is known at compile time.
    /// Index by ToolKind enum ordinal.
    pub per_tool: [ToolCounters; 8],
}

#[derive(Default)]
pub struct ToolCounters {
    pub calls: AtomicU64,
    pub duration_us: AtomicU64,
    pub output_bytes: AtomicU64,
}

#[repr(u8)]
pub enum ToolKind {
    FastSearch = 0,
    FastRefs = 1,
    GetSymbols = 2,
    DeepDive = 3,
    GetContext = 4,
    RenameSymbol = 5,
    ManageWorkspace = 6,
    QueryMetrics = 7,
}
```

Pre-allocated at construction, no HashMap, no Mutex. Updated synchronously via `fetch_add`. Read by `query_metrics(category: "session")` without touching SQLite.

## What's Collected Per Tool

| Tool | result_count | source_bytes | output_bytes | Tool-specific metadata |
|------|-------------|-------------|-------------|----------------------|
| fast_search | match count | sum of matched file sizes (batch query) | response `.len()` bytes | query, target, relaxed flag |
| deep_dive | 1 (single symbol) | sum of contributing file sizes (batch query) | response `.len()` bytes | symbol, depth, caller/callee counts |
| fast_refs | definition + reference count | sum of referenced file sizes (batch query) | response `.len()` bytes | symbol, semantic fallback flag |
| get_symbols | symbol count returned | file size from index (already loaded) | response `.len()` bytes | file, mode, target filter |
| get_context | pivot + neighbor count | sum of pivot + neighbor file sizes (batch query) | response `.len()` bytes | query, pivots, neighbors, token budget |
| rename_symbol | changes count | sum of modified file sizes (batch query) | response `.len()` bytes | old/new name, files modified, dry_run |
| manage_workspace | files processed | None (not a read operation) | response `.len()` bytes | operation, symbols added/updated/deleted |
| query_metrics | result count | None (meta-tool) | response `.len()` bytes | category |

## Scope

### In scope (v1)
- `tool_calls` table with `session_id` + migration v13
- `line_count` column on files table (for future tool output enrichment)
- Instrumentation wrapper in `handler.rs` for all 8 tools
- `ToolCallReport` struct and `call_tool_with_metrics` on each tool
- `Arc<SessionMetrics>` with fixed-size per-tool atomic counters on `JulieServerHandler`
- `query_metrics` gains `category` param: `"code_health"` | `"session"` | `"history"`
- `source_bytes` + `output_bytes` as first-class columns
- `/metrics` skill for formatted narrative output
- The "X NOT injected into context" headline number

### Out of scope (defer)
- Per-query search quality analysis
- Cross-workspace comparison
- Workspace growth tracking (requires snapshot mechanism)
- Export/sharing of metrics
- "Time saved" estimation
- Retention policies or auto-pruning
- Tracking non-tool operations (file watcher, background indexing)

### Not doing, ever
- Phoning home / telemetry
- Value claims not backed by measured data
