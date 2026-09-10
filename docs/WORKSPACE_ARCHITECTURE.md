# Workspace Architecture

**Last Updated:** 2026-09-09
**Status:** Production, machine service + stdio shim runtime

This document describes Julie's workspace storage, routing, and liveness model.

## Runtime Model

The machine service runs as a single background process per user machine,
serving Streamable HTTP MCP at `/mcp`, JSON API at `/api/<tool>`, status at
`/status`, and the dashboard at `/`. The service records its port, process ID,
and bearer token in `~/.julie/service.json`.

The no-args `julie-server` serves as a lightweight stdio shim, forwarding
newline-delimited JSON-RPC messages between stdin/stdout and the service over
HTTP localhost. If no service is running, the shim auto-spawns a detached service
process. MCP clients that support Streamable HTTP (Claude Code, Codex, Cursor)
can register directly against `http://127.0.0.1:<port>/mcp` with the bearer
token, bypassing process spawning completely.

```text
$JULIE_HOME/                     # Default: ~/.julie
+-- service.json                 # Discovery file: port, pid, token, version
+-- registry.db                  # Workspaces, cleanup events, snapshots, tool calls
+-- indexes/
    +-- julie_316c0b08/
    |   +-- facts.sqlite         # Blob-keyed facts (plus -wal/-shm)
    |   +-- tantivy/             # Projected full-text search index
    +-- coa-mcp-framework_c77f81e4/
        +-- facts.sqlite
        +-- tantivy/

<source_root>/.julie/            # Project-local source coordination (independent of JULIE_HOME)
+-- locks/
|   +-- source-edit.lock         # Serializes multi-process source file writes
+-- edit-journals/               # Durable atomic multi-file journals (<edit_id>.json) with resume/rollback
+-- logs/                        # Project-local diagnostics
```

There is a single long-running service per machine (or user account), with a
lightweight stdio shim for clients that speak stdio MCP. All MCP sessions share
the service's memory, caches, and runtime pipelines. `service.json`, `registry.db`,
and `indexes/<id>/{facts.sqlite,tantivy}` are the only durable files under `$JULIE_HOME`.

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

Julie uses three workspace concepts:

- **Current workspace**: the checkout the handler is bound to.
- **Known workspace**: a workspace recorded in `registry.db`.
- **Target workspace**: the known workspace selected by a tool call's `workspace` parameter.

Cross-workspace work goes through one front door:

1. Call `manage_workspace(operation="open", path=<path>)` or
   `manage_workspace(operation="open", workspace_id=<id>)`.
2. Julie binds the existing index, or indexes the path if it has no index.
3. Search, navigation, and editing tools route by the resulting `workspace_id`.

`open` covers the retired `register` operation. `list` prunes stale rows;
`status` covers the retired `stats`.

Omitted `workspace` parameters mean the current workspace only. Opened
workspaces do not expand the default search scope.

## Workspace Binding

A handler is bound once by `RuntimeFactory` per `(root, index_root)`. The root
comes from the startup hint: an explicit CLI path, `JULIE_WORKSPACE`, or the
process `cwd`. There is no primary-workspace swap, no session attachment, no
deferred auto-index, and no MCP `roots/list` negotiation. Julie does not switch
workspaces in the middle of an in-flight tool call.

## Workspace Isolation

Each workspace has its own physical database and Tantivy index. Workspace
selection happens before opening the database connection:

1. A tool receives a `workspace` parameter.
2. The handler routes to `indexes/{workspace_id}/facts.sqlite`.
3. The checkout store is scoped to that workspace and cannot query other
   checkouts.

Tool-level `workspace` parameters are essential. They choose which workspace
database and Tantivy index are opened for that request.

## One Writer Per Checkout

One machine service process (`julie-server service`) owns every workspace index.
The service's handler for a checkout is the only writer for that checkout. There
is no leader election, no per-workspace lock file, no read-only session, no owner
epoch, no `WriterPermit`, no publication lock, and no host admission slot.

- **Mutation gate**: per-checkout writes serialize through the in-process async
  mutex in `crates/julie-core/src/workspace/mutation_gate.rs`. Gated writers call
  `acquire_gate(workspace_id)` and pass the `MutationGuard<'_>` proof token. The
  gated writers are: watcher event-processor, watcher repair scan, watcher
  repair-replay, watcher Tantivy retry, startup catch-up, force-reindex,
  `refresh`, and `rebuild`.
- **Durable roots**: `$JULIE_HOME/indexes/<id>/facts.sqlite` (plus `-wal`/`-shm`)
  and `$JULIE_HOME/indexes/<id>/tantivy/` per checkout; `$JULIE_HOME/registry.db`
  and `service.json` per machine. Project logs stay under `<project>/.julie/logs/`.
- **Schema drift rebuilds**: `facts.sqlite` is never migrated. A schema or
  `SEMANTIC_INDEX_ENGINE_VERSION` mismatch deletes `indexes/<id>/` and reindexes.
  `registry.db` keeps its own small migrations. `manage_workspace(operation="rebuild")`
  forces the same delete and reindex.
- **Sibling seeding**: `manage_workspace(operation="open", path=<new checkout>)`
  on a path whose `git rev-parse --git-common-dir` matches a registered workspace
  copies blobs and fact rows by hash, extracts missing blobs, rebuilds `tantivy/`,
  and reports
  `Seeded from <sibling>: <copied> blobs copied, <extracted> extracted, <removed> removed in <ms> ms`.
- **Status**: `manage_workspace(operation="status")` returns a per-checkout
  `CheckoutStatus` (workspace_id, root, root_exists, watcher, last_file_event_at,
  file_count, symbol_count, facts_bytes, tantivy, tantivy_age_seconds, vector_count,
  last_write_at). `GET /status` carries the same list under `checkouts`.

Source edits are independent of index writes:

- Dry-run previews perform zero disk writes, acquire zero locks, and create zero
  journals.
- Apply operations (MCP, shared CLI, or standalone CLI) coordinate under
  `<source_root>/.julie/locks/source-edit.lock` and use durable multi-file
  journals at `<source_root>/.julie/edit-journals/<edit_id>.json`.
- File hashes are re-validated before modification; stale edits are rejected
  with `EDIT_CONFLICT`. AST-aware operations (`rename_symbol`, `rewrite_symbol`)
  validate syntax after the edit before committing file writes.
- Partial writes are recoverable via `recover-edit` (named CLI or
  `manage_workspace(operation="recover_edit", ...)`) with idempotent `resume` and
  safe `rollback` under source hash guards.

## Watchers And Cleanup

Watcher coverage follows active workspaces, not every known workspace in
`registry.db`.

- A watcher is attached when the service binds a handler for the workspace.
- Known but inactive workspaces do not keep background watcher coverage.

Cleanup follows the same liveness model:

- **Present** workspace: path exists and the workspace is usable.
- **Stale** workspace: path is gone and no live session or indexing work blocks
  cleanup.
- **Blocked** workspace: path is gone, but a bound handler still holds the
  workspace open.

Opening a stale inactive workspace prunes it and records a cleanup event.
Opening a missing but blocked workspace reports the blocking reason. Manual
delete uses the same liveness checks and refuses to remove an active workspace.

## Storage Location Summary

| Runtime path | Workspace data | Registry | Source coordination |
| --- | --- | --- | --- |
| Machine service (HTTP / stdio shim) | `$JULIE_HOME/indexes/<workspace_id>/` (`db/`, `tantivy/`) | `$JULIE_HOME/registry.db` | `<source_root>/.julie/locks/source-edit.lock`<br>`<source_root>/.julie/edit-journals/` |
| Standalone CLI | `<project>/.julie/indexes/<workspace_id>/` | None | `<source_root>/.julie/locks/source-edit.lock`<br>`<source_root>/.julie/edit-journals/` |

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

Workspace indexing and tool diagnostics belong in project logs.

## Key Benefits

- Complete workspace isolation through separate db/tantivy files.
- Explicit activation flow for cross-workspace work via
  `manage_workspace(operation="open", ...)`.
- Shared MCP-session storage under `$JULIE_HOME/indexes/`.
- One writer per checkout inside the machine service; no cross-process locks.
- Disposable indexes: schema or engine drift deletes and reindexes instead of migrating.
- Sibling seeding for new worktrees of a registered repository.
- Atomic multi-file source edits under `<source_root>/.julie/locks/source-edit.lock` with
  hash guards and journal-based recovery (`recover-edit`).
- Standalone CLI remains available without shared registry state.
