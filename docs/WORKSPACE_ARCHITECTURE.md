# Workspace Architecture

**Last Updated:** 2026-06-06
**Status:** Production, in-process stdio runtime

This document describes Julie's workspace storage, routing, and liveness model.

## Runtime Model

The no-args `julie-server` serves MCP in-process over rmcp stdio. Each MCP
session is its own process. Processes coordinate through shared files under
`$JULIE_HOME`, host admission locks, and per-workspace OS locks:

```text
$JULIE_HOME/                     # Default: ~/.julie
+-- registry.db                  # Workspaces, cleanup events, snapshots, tool calls
+-- scheduler/                   # Host admission slots limiting concurrent indexing (1..=8 slots)
|   +-- config.json
|   +-- config.lock
|   +-- index-0.lock
|   +-- index-1.lock
+-- indexes/
    +-- julie_316c0b08/
    |   +-- leader.lock          # Advisory owner lock
    |   +-- publication.lock     # Cross-process reader/writer synchronization
    |   +-- continuations.db     # Private SQLite store for durable continuation snapshots (15m TTL, 64 MiB limit)
    |   +-- db/symbols.db        # Canonical SQLite symbol database
    |   +-- tantivy/             # Projected full-text search index
    +-- coa-mcp-framework_c77f81e4/
        +-- leader.lock
        +-- publication.lock
        +-- continuations.db
        +-- db/symbols.db
        +-- tantivy/

<source_root>/.julie/            # Project-local source coordination (independent of JULIE_HOME)
+-- locks/
|   +-- source-edit.lock         # Serializes multi-process source file writes
+-- edit-journals/               # Durable atomic multi-file journals (<edit_id>.json) with resume/rollback
+-- logs/                        # Project-local diagnostics
```

There is no background daemon, stdio adapter, or HTTP MCP bridge. MCP clients
start `julie-server`; that process handles stdio directly.

`JULIE_HOME` overrides the shared home directory directly. The path is used
as-is; `.julie` is not appended. All Julie processes must see the same value,
or they will use different registries and indexes. `JULIE_HOME` is unrelated to
`JULIE_WORKSPACE`, which only selects the startup workspace root.

Standalone CLI commands that pass `--standalone` use project-local storage under
`<project>/.julie/indexes/` and do not participate in the shared registry.
However, both standalone CLI and shared MCP sessions serialize source modifications
through `<source_root>/.julie/locks/source-edit.lock` and `<source_root>/.julie/edit-journals/`.

## Registry

`registry.db` is the authoritative registry. It replaces the old JSON registry
and the old daemon database name. It tracks:

- Known workspaces: ID, path, status, file counts, and symbol counts.
- Cleanup events for deleted or pruned workspaces.
- Codehealth snapshots for dashboard views.
- Tool call history retained for metrics.
- Lightweight runtime state needed by standalone dashboard reads.

## Global Workspace Targeting

Julie uses four workspace concepts:

- **Current workspace**: the session's primary workspace.
- **Known workspace**: a workspace recorded in `registry.db`.
- **Active workspace**: a known workspace opened for the current MCP session.
- **Target workspace**: the active workspace selected by a tool call.

Cross-workspace work goes through one front door:

1. Call `manage_workspace(operation="open", path=<path>)` or
   `manage_workspace(operation="open", workspace_id=<id>)`.
2. Julie resolves the workspace, indexes or refreshes it as needed, and
   activates it for the session.
3. Search, navigation, and editing tools route by the resulting `workspace_id`.

`manage_workspace(operation="register", ...)` indexes or refreshes a known
workspace without activating it for the current session.

Omitted `workspace` parameters still mean the current primary workspace only.
Secondary roots or opened workspaces do not expand the default search scope.

## Startup Hint And Roots Model

Julie treats startup path resolution and MCP client roots as separate inputs:

- **Startup hint**: path from CLI, `JULIE_WORKSPACE`, or process `cwd`.
- **Primary workspace binding**: the session's current `primary` target.
- **Client roots**: request-time hints from MCP hosts that support `roots/list`.

When Julie starts from a weak hint such as `cwd`, the session can remain
unbound until the first primary-scoped request. At that boundary, Julie asks the
client for roots, binds the first root as the session primary, and keeps any
additional roots active as secondary workspaces for explicit targeting.

`notifications/roots/list_changed` is request-bound, not immediate. Julie marks
the session roots state dirty when the notification arrives, then refreshes
`roots/list` on the next primary-scoped request. Julie does not switch
workspaces in the middle of an in-flight tool call.

For explicit CLI or env startup sessions, a dirty roots notification settles
back to the startup hint on the next primary-scoped request. Julie clears the
dirty state there, but does not re-query roots or rebind away from the explicit
startup root.

## Workspace Isolation

Each workspace has its own physical database and Tantivy index. Workspace
selection happens before opening the database connection:

1. A tool receives a `workspace` parameter.
2. The handler routes to `indexes/{workspace_id}/db/symbols.db`.
3. The connection is scoped to that workspace and cannot query other workspace
   databases.

Tool-level `workspace` parameters are essential. They choose which workspace
database and Tantivy index are opened for that request.

## Leader And Follower Sessions

Each shared workspace index directory contains `leader.lock`. On startup or
workspace open, a Julie process attempts to acquire that advisory OS lock:

- **Leader (Owner)**: owns writes for that workspace, including the watcher,
  catch-up indexer, repairs, force reindex, refresh stats, and Tantivy writes.
  All index mutations require an authentic `WriterPermit` minted from the active
  `OwnerEpoch`.
- **Follower**: serves read requests over SQLite WAL and Tantivy mmap. Followers
  can also preview and safely apply source-code edits without index ownership.
- **Dynamic Leader Election**: Followers run a background probe loop (every 500ms
  + 0–100ms jitter). When the owner exits or drops `leader.lock`, a follower detects
  the release, enters `Recovering` phase to reconcile any projection gap, and
  promotes dynamically to `Owner` without restarting the MCP session.
- **Protocol Independence**: Under MCP protocol `2026-07-28` (and direct first-message
  tool calls), there is no mandatory `initialize` handshake. The lack of an initialize
  requirement does *not* eliminate runtime state: processes bind workspaces on the
  fly, probe the owner lock, and participate in dynamic leader election seamlessly.

The leader lock is a durable file. Seeing `leader.lock` on disk is normal even
when no process currently holds it.

## Runtime Lifetimes: Request, Owner, Source-Edit, and Index-Commit

Julie strictly decouples runtime lifetimes to ensure that transient connection drops
or concurrent clients cannot corrupt shared workspace state:

1. **Request Lifetime**:
   - Each client request or tool call has its own bounded execution lifetime and
     deadline.
   - Client disconnects, cancellations, or read timeouts terminate only the
     requesting connection.
   - Request drops **never** abort an in-flight owner commit, rollback shared
     transactions, or release the workspace `leader.lock`.
2. **Owner Lifetime**:
   - Tied to holding the advisory OS lock at `$JULIE_HOME/indexes/<ws_id>/leader.lock`.
   - Managed by `WorkspaceRuntimeManager` with explicit `RuntimePhase` (`Starting`,
     `Follower`, `Recovering`, `Owner`, `Draining`, `Terminated`) and `OwnerEpoch`.
   - The owner is responsible for background filesystem watchers, catch-up indexing,
     Tantivy projection commits, and maintenance.
   - When the owner terminates, the OS automatically releases `leader.lock`, allowing
     a follower to promote dynamically.
3. **Source-Edit Lifetime**:
   - Completely independent of index ownership: followers can preview and apply
     source edits safely.
   - Dry-run previews perform zero disk writes, acquire zero locks, and create zero
     journals.
   - All source-edit apply operations (from MCP, shared CLI, or standalone CLI)
     coordinate under `<source_root>/.julie/locks/source-edit.lock`.
   - Edits use durable multi-file journals at `<source_root>/.julie/edit-journals/<edit_id>.json`.
   - Prior to modification, file hashes are re-validated against expected source states;
     stale edits are rejected with `EDIT_CONFLICT`.
   - AST-aware operations (`rename_symbol`, `rewrite_symbol`) validate syntax tree
     correctness post-edit before committing file writes.
   - Partial writes are recoverable via the `recover-edit` command (named CLI or
     `manage_workspace(operation="recover_edit", ...)`), supporting idempotent `resume`
     and safe `rollback` under source hash guards.
4. **Index-Commit Lifetime**:
   - All 8 index writer entry points enforce authentic OS owner proofs (`WriterPermit`).
   - Writes commit to canonical SQLite first, then publish to Tantivy under
     `publication.lock`.
   - If an owner crashes mid-commit, the newly promoted owner reconciles Tantivy from
     canonical SQLite without requiring a source file change.

## Index Writer Fencing and Coherent Publication

To prevent torn reads, mixed-generation search results, or dual-writer corruption across
processes:

- **Publication Lock (`publication.lock`)**: Located at `$JULIE_HOME/indexes/<ws_id>/publication.lock`.
  - Readers acquire a shared OS lock during revision queries and snapshot acquisition.
  - Writers acquire an exclusive OS lock during revision publication, schema migrations,
    or index resets.
  - `publication.lock` is never unlinked while processes may hold it.
- **Separate Revision Tracking**: SQLite canonical revisions and Tantivy projected revisions
  are tracked independently (`PublicationStamp { epoch, canonical_revision, projected_revision, generation }`).
- **Truthful Freshness**: Readers verify generation stamps. If an index commit is pending or
  interrupted, followers report `PROJECTION_LAG` or set `index_refresh_pending: true` honestly
  rather than returning inconsistent hybrid results.

## Durable Continuation Snapshots

Paginated query results (e.g. `fast_search`, `get_symbols`, `spillover_get`) are backed by a
private SQLite database:

- **Storage Path**: `$JULIE_HOME/indexes/<ws_id>/continuations.db` (mode 0o600, private to user).
- **Binding Guards**: Tokens are random 256-bit identifiers bound to `workspace_id`, tool
  name, argument hash, and source generation hashes.
- **Budgeting & Retention**: Default TTL is 15 minutes; individual page snapshots are capped at
  8 MiB; total continuation storage is capped at 64 MiB per workspace.
- **Safety**: Continuation tokens grant no file-system authority, SQL access, or edit rights.
  Cross-process reads succeed if the workspace and token match; attempts to use a token
  across different worktrees or with stale source revisions fail with `CONTINUATION_INVALID`
  or `CONTINUATION_STALE`.

## Host Admission Scheduling

Concurrent indexing across multiple repositories or worktrees is governed by a global
advisory host admission pool:

- **Storage Path**: `$JULIE_HOME/scheduler/index-{0,1}.lock` (configurable 1..=8 slots,
  default `min(2, available_parallelism)`).
- **Concurrency Cap**: Prevents CPU and I/O starvation by bounding concurrent indexing jobs
  across all running Julie processes.
- **Quantum & Backpressure**: Enforces a 10-second scheduling quantum between completed file
  commits and bounds dirty path coalescing to 4,096 entries before triggering full rescan.
  Slots are never held while waiting for semantic inference or while idle.

## Watchers And Cleanup

Watcher coverage follows active workspaces, not every known workspace in
`registry.db`.

- A watcher is attached when a workspace becomes active in a leader session.
- Followers do not run watchers or write to Tantivy.
- Known but inactive workspaces do not keep background watcher coverage.

Cleanup follows the same liveness model:

- **Present** workspace: path exists and the workspace is usable.
- **Stale** workspace: path is gone and no live session or indexing work blocks
  cleanup.
- **Blocked** workspace: path is gone, but a live session still holds the
  workspace open.

Opening a stale inactive workspace prunes it and records a cleanup event.
Opening a missing but blocked workspace reports the blocking reason. Manual
delete uses the same liveness checks and refuses to remove an active workspace.

## Storage Location Summary

| Runtime path | Workspace data | Registry | Source coordination |
| --- | --- | --- | --- |
| In-process MCP | `$JULIE_HOME/indexes/<workspace_id>/` (`leader.lock`, `publication.lock`, `continuations.db`, `db/`, `tantivy/`) | `$JULIE_HOME/registry.db` | `<source_root>/.julie/locks/source-edit.lock`<br>`<source_root>/.julie/edit-journals/` |
| Standalone CLI | `<project>/.julie/indexes/<workspace_id>/` | None | `<source_root>/.julie/locks/source-edit.lock`<br>`<source_root>/.julie/edit-journals/` |
| Host Scheduler | `$JULIE_HOME/scheduler/index-{0..7}.lock` | None | N/A |

Default `$JULIE_HOME` is `~/.julie`. Set `JULIE_HOME` to relocate shared state
and indexes; see `docs/OPERATIONS.md` for the migration workflow.

## Logs

Per-workspace logs are project-local and are not affected by `JULIE_HOME`:

```bash
# Correct
/Users/murphy/source/julie/.julie/logs/julie.log.2026-06-06

# Wrong
~/.julie/logs/
```

The resident embedding host writes its own host log under `$JULIE_HOME`; normal
workspace indexing and tool diagnostics belong in project logs.

## Key Benefits

- Complete workspace isolation through separate db/tantivy files.
- Explicit activation flow for cross-workspace work via
  `manage_workspace(operation="open", ...)`.
- Shared MCP-session storage under `$JULIE_HOME/indexes/`.
- Single-writer safety through per-workspace `leader.lock` with dynamic follower failover.
- Read-only followers can safely preview and apply source edits without index ownership.
- Atomic multi-file source edits under `<source_root>/.julie/locks/source-edit.lock` with
  hash guards and journal-based recovery (`recover-edit`).
- Cross-process publication coherency via `publication.lock` without torn reads or
  mixed generations.
- Durable cross-process query continuations via private SQLite store (`continuations.db`).
- Bounded indexing concurrency across processes via host admission slots.
- Standalone CLI remains available without shared registry state.
