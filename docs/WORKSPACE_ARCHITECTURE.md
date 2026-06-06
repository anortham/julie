# Workspace Architecture

**Last Updated:** 2026-06-06
**Status:** Production, in-process stdio runtime

This document describes Julie's workspace storage, routing, and liveness model.

## Runtime Model

The no-args `julie-server` serves MCP in-process over rmcp stdio. Each MCP
session is its own process. Processes coordinate through shared files under
`$JULIE_HOME` and a per-workspace OS lock:

```text
$JULIE_HOME/                     # Default: ~/.julie
+-- registry.db                  # Workspaces, cleanup events, snapshots, tool calls
+-- indexes/
    +-- julie_316c0b08/
    |   +-- leader.lock
    |   +-- db/symbols.db
    |   +-- tantivy/
    +-- coa-mcp-framework_c77f81e4/
        +-- leader.lock
        +-- db/symbols.db
        +-- tantivy/
```

There is no background daemon, stdio adapter, or HTTP MCP bridge. MCP clients
start `julie-server`; that process handles stdio directly.

`JULIE_HOME` overrides the shared home directory directly. The path is used
as-is; `.julie` is not appended. All Julie processes must see the same value,
or they will use different registries and indexes. `JULIE_HOME` is unrelated to
`JULIE_WORKSPACE`, which only selects the startup workspace root.

Standalone CLI commands that pass `--standalone` use project-local storage under
`<project>/.julie/indexes/` and do not participate in the shared registry.

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
workspace open, a Julie process attempts to acquire that lock:

- **Leader**: owns writes for that workspace, including the watcher, catch-up
  indexer, repairs, force reindex, refresh stats, and Tantivy writes.
- **Follower**: serves read-only requests over SQLite WAL and Tantivy mmap.
- **Leader death**: the OS releases the lock. A later session can acquire it and
  reconcile changed files through the existing catch-up and repair paths.

The leader lock is a durable file. Seeing `leader.lock` on disk is normal even
when no process currently holds it.

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

| Runtime path | Workspace data | Registry |
| --- | --- | --- |
| In-process MCP | `$JULIE_HOME/indexes/<workspace_id>/` | `$JULIE_HOME/registry.db` |
| Standalone CLI | `<project>/.julie/indexes/<workspace_id>/` | None |

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
- Single-writer safety through per-workspace `leader.lock`.
- Read-only followers for concurrent sessions.
- Standalone CLI remains available without shared registry state.
