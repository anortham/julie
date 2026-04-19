# Dependency Upgrade Audit Findings

**Date:** 2026-04-19
**Implementation plan:** [`2026-04-19-dependency-upgrade-audit-implementation-plan.md`](./2026-04-19-dependency-upgrade-audit-implementation-plan.md)
**Scope:** Measured dependency-upgrade pass findings and pilot shortlist

## Current State

Resolved versions are from `cargo tree -p julie --depth 1`.

| Dependency | Manifest (`Cargo.toml`) | Resolved (current) | Latest realistic target | Measured-pass decision |
|---|---:|---:|---:|---|
| tantivy | `0.22` | `0.22.1` | `0.26.0` | Defer, separate plan |
| sqlite-vec | `0.1` | `0.1.9` | `0.1.9` (stable) | No pilot upgrade |
| rusqlite | `0.37` | `0.37.0` | `0.39.0` | **Pilot now** |
| rmcp | `1.2` | `1.5.0` | `1.5.0` | No pilot upgrade |
| notify | `8.2` | `8.2.0` | `8.2.0` | No pilot upgrade |
| tokio | `1.47.1` | `1.52.1` | `1.52.1` | No pilot upgrade |

## Per-Dependency Findings

### tantivy

- **Current:** manifest `0.22`, resolved `0.22.1`
- **Target:** `0.26.0`
- **Release path:** `0.22.1 -> 0.24.2 -> 0.25.0 -> 0.26.0`
- **Upstream value to Julie:**
  - `0.24.x` can read indices from `0.22` and `0.21`, plus merge-loop and panic fixes
  - `0.25.0` includes union performance fix
  - `0.26.0` includes intersection-seek correctness, scorer ordering and lazy-scoring improvements, union optimizations, and a vint-overflow fix during index creation
- **Julie touch points:** `src/search/query.rs`, `src/search/index.rs`, `src/search/schema.rs`, `src/search/tokenizer.rs`, `src/tests/tools/search/**`
- **Risk:** moderate to high, Julie builds manual `BooleanQuery` trees and relies on a shallow `schema_is_compatible()` check for persisted indexes
- **Recommendation:** next candidate after this pass, but only with a dedicated plan

### sqlite-vec

- **Current:** manifest `0.1`, resolved `0.1.9`
- **Target:** stable target is already current (`0.1.9`)
- **Notes:** `0.1.10-alpha.x` exists, not a measured-pass target
- **Julie relevance:**
  - `0.1.7` added `DELETE` support with space reuse and distance constraints
  - `0.1.9` fixed intermittent `SQLITE_DONE` on `DELETE` for `vec0` with long text metadata columns
- **Julie touch points:** `src/database/mod.rs`, `src/database/vectors.rs`, `src/database/memory_vectors.rs`, `src/database/migrations.rs`, `src/database/workspace.rs`, `src/tests/core/embedding_deps.rs`
- **Recommendation:** no pilot upgrade now; optional future manifest pin to `0.1.9` for determinism

### rusqlite

- **Current:** manifest `0.37`, resolved `0.37.0`
- **Target:** `0.39.0`
- **Relevant upstream changes:**
  - `0.38.0` disables default `usize` and `u64` `ToSql` and `FromSql` conversions unless `fallible_uint` is enabled, bumps bundled SQLite to `3.51.1`, and raises minimum SQLite to `3.34.1`
  - `0.39.0` bumps bundled SQLite to `3.51.3`, plus virtual-table aux-constraint and hook/pointer fixes
- **Julie touch points:** broad impact across `src/database/**`, `src/analysis/change_risk.rs`, `src/daemon/database.rs`
- **Known fallout:** raw `usize` binds in:
  - `src/database/files.rs:get_recent_files()`
  - `src/database/symbols/search.rs:get_most_referenced_symbols()`
- **Recommendation:** pilot now, upgrade to `0.39.0`

### rmcp

- **Current:** manifest `1.2`, resolved `1.5.0`
- **Target:** resolved runtime already at `1.5.0`
- **Relevant upstream changes:**
  - `1.3` stdin EOF response draining
  - `1.4` streamable HTTP initialized-notification gate removal and keep-alive default
  - `1.5` MCP protocol `2025-11-25` support
- **Julie touch points:** `src/handler.rs`, `src/daemon/ipc_session.rs`, `src/mcp_compat.rs`, MCP-facing tests
- **Recommendation:** no pilot upgrade; optional later manifest-alignment cleanup

### notify

- **Current:** resolved `8.2.0`, latest stable `8.2.0`
- **Relevant upstream changes in `8.2.0`:** inotify `max_user_watches` exhaustion surfacing and unknown watch-descriptor fixes
- **Notes:** `9.0` is still RC, not a measured-pass target
- **Julie touch points:** `src/watcher/mod.rs`, `src/watcher/events.rs`, `src/watcher/runtime.rs`, `src/tests/integration/watcher.rs`
- **Recommendation:** defer until `9.0` GA and a concrete Julie bug exists

### tokio

- **Current:** manifest `1.47.1`, resolved `1.52.1`
- **Target:** resolved runtime already at `1.52.1`
- **Relevant changes:** recent `spawn_blocking` and `Notify` fixes are already present through current resolution
- **Julie touch points:** daemon, handler, watcher, adapter, tools
- **Recommendation:** no pilot upgrade; optional later manifest-alignment cleanup only

## Shortlist and Recommendation

1. **Measured pilot now:** `rusqlite` to `0.39.0`
2. **Next candidate:** `tantivy`, with its own dedicated plan and staged path (`0.22.1 -> 0.24.2 -> 0.25.0 -> 0.26.0`)
3. **Defer in this pass:** `sqlite-vec`, `rmcp`, `notify`, `tokio`

Plain call: only `rusqlite` clears the bar for the measured pilot in this pass. `tantivy` is the next candidate, but it needs separate planning and risk control.
