# Workspace Architecture

**Last Updated:** 2026-04-12
**Status:** Production (v6, stdio + daemon modes)

This document provides detailed information about Julie's workspace architecture, routing, and storage.

## Two Operating Modes

Julie runs in two modes with different storage topologies:

### Stdio Mode
Each MCP session is independent. Indexes live under the project:

```
<project>/.julie/indexes/
├── julie_316c0b08/
│   ├── db/symbols.db            ← SQLite database
│   └── tantivy/                 ← Tantivy search index
│
└── coa-mcp-framework_c77f81e4/
    ├── db/symbols.db
    └── tantivy/
```

Stdio mode is centered on the current workspace and has no persistent global registry. It can still index another path and accepts non-`primary` workspace IDs permissively, but the supported global registration and activation flow lives in daemon mode.

### Daemon Mode (`julie daemon`)
A background process shares indexes across all MCP sessions. Workspace indexes live under `~/.julie/indexes/<workspace_id>`:

```
~/.julie/
├── daemon.db                    ← Registry: workspaces, references, snapshots, tool calls
└── indexes/
    ├── julie_316c0b08/
    │   ├── db/symbols.db
    │   └── tantivy/
    └── coa-mcp-framework_c77f81e4/
        ├── db/symbols.db
        └── tantivy/
```

`daemon.db` is the authoritative registry (replaces the old `registry.json`). It tracks:
- All known workspaces (ID, path, status, file/symbol counts)
- Pairing metadata and other convenience metadata
- Per-session codehealth snapshots
- Tool call history (retained 7 days)

## Global Workspace Targeting

Daemon mode is the supported global-workspace path and uses four workspace concepts:

- **Current workspace**: the workspace rooted at the session's project directory.
- **Known workspace**: any workspace recorded in daemon metadata.
- **Active workspace**: a known workspace opened for the current daemon session.
- **Target workspace**: the active workspace selected for a tool call.

In daemon mode, cross-workspace work goes through one front door:

1. Call `manage_workspace(operation="open", path=<path>)` or `manage_workspace(operation="open", workspace_id=<id>)`.
2. Julie resolves the workspace, indexes or refreshes it as needed, then activates it for the current session.
3. Search, navigation, and editing tools route by the resulting `workspace_id`.

`manage_workspace(operation="add", ...)` still records a pairing or registers a workspace, but that metadata does not activate the workspace and does not grant routing authority.

Outside daemon mode, Julie does not have the same registry-backed activation model. Stdio still accepts explicit non-`primary` workspace IDs and can index another path, but that behavior is permissive compatibility behavior, not the supported global-workspace flow.

## Startup Hint And Roots Model

Julie now treats startup path resolution and client roots as separate inputs.

- **Startup hint** is the path Julie gets from CLI, `JULIE_WORKSPACE`, or process `cwd` at session startup.
- **Primary workspace binding** is the session's current `primary` target.
- **Client roots** are request-time hints from MCP hosts that support `roots/list`.

The startup hint is not always authoritative. When Julie starts from a weak hint such as `cwd`, daemon sessions can remain unbound until the first primary-scoped request. On that request boundary, Julie asks the client for roots, binds the first root as the session primary, and keeps any additional roots active as secondary workspaces for explicit targeting.

This keeps default tool behavior stable:

- Omitted `workspace` parameters still mean the current primary workspace only.
- Secondary roots do not expand default search scope.
- Secondary roots stay active in the session so explicit `workspace=<id>` calls keep working after primary rebinds.

`notifications/roots/list_changed` is request-bound, not immediate. Julie marks the session roots state dirty when the notification arrives, then refreshes `roots/list` on the next primary-scoped request. That follow-up request reconciles the primary binding and any newly reported secondary roots. Julie does not switch workspaces in the middle of an in-flight tool call.

## How Workspace Isolation Works

Each workspace has its own PHYSICAL database and Tantivy index files. Workspace selection happens when opening the DB connection:

1. Tool receives `workspace` parameter
2. Handler routes to `indexes/{workspace_id}/db/symbols.db`
3. Connection is locked to that workspace — cannot query others from same connection

**Tool-level `workspace` parameters are essential**. They route to the correct workspace database. In daemon mode the target workspace must already be active for the current session. In stdio mode non-`primary` IDs are still accepted without daemon registry validation.

## Pairings And Watchers

Persistent pairings are convenience metadata.

- They can help Julie suggest or recall related workspaces.
- They do not activate a workspace.
- They do not decide routing.
- They do not bypass freshness checks.

Watcher coverage follows active workspaces, not every known workspace in `daemon.db`. If a workspace is known but not active in the current session, Julie does not keep a live watcher attached to it for that session.

## Storage Location Summary

| Mode   | Workspace data                 | Registry             |
|--------|--------------------------------|----------------------|
| Stdio  | `<project>/.julie/indexes/`    | None (ephemeral)     |
| Daemon | `~/.julie/indexes/<workspace_id>/` | `~/.julie/daemon.db` |

## Log Location

Logs are PROJECT-LEVEL (not user-level) in both modes:

```bash
# CORRECT
/Users/murphy/source/julie/.julie/logs/julie.log.2026-03-22

# WRONG
~/.julie/logs/  ← DOES NOT EXIST
```

## Key Benefits

- Complete workspace isolation, each workspace has its own db/tantivy index
- Explicit activation flow for cross-workspace work via `manage_workspace(operation="open", ...)`
- Centralized daemon storage under `~/.julie/indexes/`
- Daemon mode enables cross-session sharing and persistent metrics
- Stdio mode works fully offline with no daemon dependency

## Startup Hint And Roots Policy

Julie keeps startup intent and MCP roots separate.

- `WorkspaceStartupHint` records where the session started and why: CLI flag, `JULIE_WORKSPACE`, or process `cwd`.
- Client roots are only authoritative for weak `cwd` startup sessions.
- Explicit CLI and env startup remain authoritative even if the client advertises roots support.

On weak `cwd` startup, Julie can leave the session unbound, then resolve the primary workspace from `roots/list` at the next primary-scoped request. Extra roots from that response stay active as secondary workspaces for explicit targeting.

`notifications/roots/list_changed` is request-bound. The notification only marks session roots state dirty. Julie refreshes `roots/list` and may rebind the primary workspace on the next primary-scoped request, but only for weak `cwd` startup sessions.

For explicit CLI or env startup sessions, a dirty roots notification is settled back to the startup hint on the next primary-scoped request. Julie clears the dirty state there, but does not re-query roots or rebind away from the explicit startup root.

Julie does not switch workspaces in the middle of an in-flight tool call.
