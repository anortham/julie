# Workspace Architecture

**Last Updated:** 2026-03-22
**Status:** Production (v6 — stdio + daemon modes)

This document provides detailed information about Julie's workspace architecture, routing, and storage.

## Two Operating Modes

Julie runs in two modes with different storage topologies:

### Stdio Mode (default)
Each MCP session is independent. Indexes live under the project:

```
<project>/.julie/indexes/
├── julie_316c0b08/              ← PRIMARY workspace
│   ├── db/symbols.db            ← SQLite database
│   └── tantivy/                 ← Tantivy search index
│
└── coa-mcp-framework_c77f81e4/  ← REFERENCE workspace (also here)
    ├── db/symbols.db
    └── tantivy/
```

Reference workspaces, `add`, `refresh`, and `stats` operations require daemon mode.

### Daemon Mode (`julie daemon`)
A background process shares indexes across all MCP sessions. Indexes live in the user home:

```
~/.julie/
├── daemon.db                    ← Registry: workspaces, references, snapshots, tool calls
└── indexes/
    ├── julie_316c0b08/          ← PRIMARY workspace (shared across sessions)
    │   ├── db/symbols.db
    │   └── tantivy/
    └── coa-mcp-framework_c77f81e4/  ← REFERENCE workspace
        ├── db/symbols.db
        └── tantivy/
```

`daemon.db` is the authoritative registry (replaces the old `registry.json`). It tracks:
- All workspaces (ID, path, status, file/symbol counts)
- Reference workspace relationships
- Per-session codehealth snapshots
- Tool call history (retained 7 days)

## How Workspace Isolation Works

Each workspace has its own PHYSICAL database and Tantivy index files. Workspace selection happens when opening the DB connection:

1. Tool receives `workspace` parameter
2. Handler routes to `indexes/{workspace_id}/db/symbols.db`
3. Connection is locked to that workspace — cannot query others from same connection

**Tool-level `workspace` parameters are ESSENTIAL** — they route to the correct DB file.

## Primary vs Reference Workspaces

**Primary Workspace:**
- Where you're actively developing
- Has full `JulieWorkspace` object with indexer, searcher, embedding machinery
- In daemon mode: its session is tracked in `daemon.db` with session count
- In daemon mode: embedding provider is shared via `EmbeddingService` (not per-workspace)

**Reference Workspaces:**
- Other codebases you want to search/reference (daemon mode only)
- Do NOT have their own `.julie/` directories
- Indexed into the same `indexes/` directory as primary
- Just indexed data — not independent workspace objects

## Storage Location Summary

| Mode   | Workspace data            | Registry                |
|--------|---------------------------|-------------------------|
| Stdio  | `<project>/.julie/`       | None (ephemeral)        |
| Daemon | `~/.julie/indexes/`       | `~/.julie/daemon.db`    |

## Log Location

Logs are PROJECT-LEVEL (not user-level) in both modes:

```bash
# CORRECT
/Users/murphy/source/julie/.julie/logs/julie.log.2026-03-22

# WRONG
~/.julie/logs/  ← DOES NOT EXIST
```

## Key Benefits

- Complete workspace isolation — each workspace has own db/tantivy index
- Centralized storage — all indexes in one location, trivial deletion
- Daemon mode enables cross-session sharing and persistent metrics
- Stdio mode works fully offline with no daemon dependency
