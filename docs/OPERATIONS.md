# Julie Runtime State - Operations and Triage

This document covers the files Julie writes outside the project tree and the
operational checks useful when storage, indexing, or dashboard state looks
wrong. The default runtime home is `$JULIE_HOME`, which defaults to `~/.julie`.

Julie now serves MCP in-process over stdio. There is no background daemon,
stdio adapter, daemon HTTP MCP endpoint, PID file, discovery file, port file, or
token file in the current runtime.

## `$JULIE_HOME`

`JULIE_HOME` relocates Julie's shared runtime state. Set it to an absolute path:

```bash
export JULIE_HOME=/mnt/fast-ssd/julie-home
```

Rules:

- If `JULIE_HOME` is unset, Julie uses `~/.julie`.
- The path is used as-is; `.julie` is not appended.
- Empty or relative values are startup errors.
- Every MCP client and shell that starts Julie must see the same value.
- `JULIE_HOME` is separate from `JULIE_WORKSPACE`, which only chooses the
  startup workspace root.
- Project logs stay under `<project>/.julie/logs/` and do not move with
  `JULIE_HOME`.

## Shared State Layout

Current in-process MCP sessions use this layout:

```text
$JULIE_HOME/
+-- registry.db
+-- embedding-host.lock
+-- embedding-host.sock            # Unix only
+-- indexes/
    +-- <workspace_id>/
        +-- leader.lock
        +-- db/
        |   +-- symbols.db
        +-- tantivy/
            +-- julie-search-compat.json
```

`registry.db` is the shared registry. It tracks known workspaces, cleanup
events, codehealth snapshots, tool calls, dashboard history, and lightweight
runtime state.

Each workspace owns its own SQLite database and Tantivy index under
`indexes/<workspace_id>/`. Tool-level `workspace` parameters route by opening
that workspace's database path; Julie does not query multiple workspaces from a
single SQLite connection.

Standalone CLI commands that pass `--standalone` use project-local storage under
`<project>/.julie/indexes/` instead of `$JULIE_HOME`.

## Leader Locks

Each workspace has a durable `leader.lock` file beside its `db/` and `tantivy/`
directories. The first live `julie-server` process that acquires the lock is
the workspace leader:

- The leader runs the file watcher, catch-up indexing, repair work, and Tantivy
  writes.
- Followers are read-only over SQLite WAL and Tantivy mmap.
- If the leader exits, the OS releases the lock. A later session can acquire it
  and reconcile changed files.

The file remains on disk after the process exits. That is normal; the OS lock,
not file deletion, is the source of truth.

## Resident Embedding Host

Embeddings are served by one resident embedding host per `$JULIE_HOME` so
multiple Julie sessions do not each load the model into VRAM. The host uses
`embedding-host.lock` plus a Unix socket or Windows named pipe derived from
`$JULIE_HOME`.

If the host is unavailable, keyword search and structural navigation continue to
work. Embedding-backed features stay disabled until a session can connect to or
spawn the host.

## Tantivy Schema Compatibility And Auto-Rebuild

Alongside each workspace's `tantivy/` directory, Julie writes
`julie-search-compat.json` with:

- `marker_version` - Julie's sidecar format version.
- `schema_signature` - field names and field types expected by the current
  binary.
- `tokenizer_signature` - the code tokenizer configuration.

On `SearchIndex::open`, Julie compares the expected signatures against the
sidecar. On mismatch:

1. The incompatible Tantivy directory is deleted and recreated empty.
2. The workspace open path rebuilds the Tantivy projection from
   `db/symbols.db`, which remains the source of truth.
3. Concurrent rebuilds are guarded by the Tantivy rebuild lock.

Operator impact:

- SQLite data is not deleted.
- Rebuild cost is proportional to symbol count, roughly the same as initial
  indexing.
- No user action or reindex flag is normally required.
- To verify the path fired, check the project log for `recreating empty index`
  or `recreated empty during open; rebuilding projection`.

If a Tantivy directory is corrupt but signatures still match, remove that
workspace's `tantivy/` directory and restart the MCP session, or run
`manage_workspace(operation="index", force=true)`.

## Dashboard

`julie-server dashboard` starts a standalone local dashboard reader. It opens
`$JULIE_HOME/registry.db` and per-workspace index files directly. The dashboard
does not host MCP, mutate workspaces, refresh projects, or stream live server
events.

## Moving Julie State

To move Julie shared state:

1. Stop all MCP clients and any running `julie-server` sessions.
2. Move the old home:
   ```bash
   mv ~/.julie /mnt/fast-ssd/julie-home
   ```
3. Set `JULIE_HOME` everywhere Julie is launched:
   ```bash
   export JULIE_HOME=/mnt/fast-ssd/julie-home
   ```
4. Restart the MCP client or run a Julie CLI command.
5. Verify the new home:
   ```bash
   ls "$JULIE_HOME/registry.db" "$JULIE_HOME/indexes/"
   ```

If Julie starts writing under `~/.julie` after the move, one of the launching
processes did not inherit the new `JULIE_HOME`. Fix that before doing more
indexing, or you will split state across two homes.

## Removed Legacy Files

Old installs may still contain files such as `daemon.pid`, `daemon.lock`,
`daemon.state`, `discovery.json`, `daemon.port`, or `daemon.token`. The current
in-process runtime does not use them for MCP serving. Do not recreate them when
debugging 3d.3-era behavior.
