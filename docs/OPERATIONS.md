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

The machine service uses this layout:

```text
$JULIE_HOME/
+-- registry.db
+-- service.json                   # Runtime record: port, pid, token, version
+-- indexes/
    +-- <workspace_id>/
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

The pinned `julie-extractors` v2.42.0 dependency provides 37 user-facing languages:
Rust, C, C++, Go, Zig, TypeScript, JavaScript, HTML, CSS, Vue, QML, Python,
Java, C#, VB.NET, PHP, Ruby, Swift, Kotlin, Dart, Elixir, Erlang, F#, Scala,
Lua, R, Bash, PowerShell, GDScript, Razor, SQL, Regex, Markdown, JSON, TOML,
YAML, and XML. JSX and TSX are aliases; `qmldir` is a container format, not an
additional language.

Standalone CLI commands that pass `--standalone` use project-local storage under
`<project>/.julie/indexes/` instead of `$JULIE_HOME`.

## One Writer Per Checkout

One machine service process (`julie-server service`) owns every workspace index.
Its handler for a checkout is the only writer for that checkout:

- The handler runs the file watcher, startup catch-up, repair work, and Tantivy
  writes. Writes serialize through the in-process mutation gate
  (`crates/julie-core/src/workspace/mutation_gate.rs`).
- There is no per-workspace lock file and no read-only session. `service.json`,
  `registry.db`, and `indexes/<id>/{db,tantivy}` are the only durable files under
  `$JULIE_HOME`.
- `symbols.db` is never migrated. A schema version other than
  `LATEST_SCHEMA_VERSION` (32) or a `SEMANTIC_INDEX_ENGINE_VERSION` mismatch deletes
  `indexes/<id>/` and reindexes. `registry.db` keeps its own small migrations.
- `manage_workspace(operation="rebuild")` deletes `indexes/<id>/` and reindexes.
  `manage_workspace(operation="status")` reports every checkout: root, watcher,
  file/symbol/vector counts, database size, Tantivy state and age, last write.
  `GET /status` carries the same list under `checkouts`.
- A new checkout of a registered repository seeds from its sibling:
  `manage_workspace(operation="open", path=...)` copies the sibling's `symbols.db`
  and `tantivy/`, rewrites the workspace id, and runs the incremental scan.
- Pass the returned ID as `workspace` to scoped tools. `health`, `refresh`, and
  `remove` use `workspace_id`; global `list` and `status` use no selector.

## Semantic Sidecar

Embeddings are served by the native `julie-semantic-sidecar` (llama.cpp GGUF).
`JULIE_EMBEDDING_PROVIDER` accepts `auto`, `native`, or `none`. If the sidecar is
unavailable, keyword search and structural navigation continue to work and
embedding-backed features stay disabled. See `docs/SEMANTIC_PROVIDERS.md`.

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
3. The per-checkout mutation gate serializes the rebuild with other writers.

Operator impact:

- SQLite data is not deleted.
- Rebuild cost is proportional to symbol count, roughly the same as initial
  indexing.
- No user action or reindex flag is normally required.
- To verify the path fired, check the project log for `recreating empty index`
  or `recreated empty during open; rebuilding projection`.

If a Tantivy directory is corrupt but signatures still match, remove that
workspace's `tantivy/` directory and restart the service, or run
`manage_workspace(operation="rebuild")`.

## Dashboard

`julie-server dashboard` starts a standalone local dashboard reader. It opens
`$JULIE_HOME/registry.db` and per-workspace index files directly. The dashboard
does not host MCP, mutate workspaces, refresh projects, or stream live server
events.

## Moving Julie State

To move Julie shared state:

1. Stop all MCP clients and the service (`julie-server service stop`).
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
`daemon.state`, `discovery.json`, `daemon.port`, `daemon.token`,
`embedding-host.lock`, `scheduler/`, or per-workspace lock and continuation
files (`publication.lock`, `continuations.db`, `.symbols.db.init.lock`,
`tantivy.julie-rebuild.lock`, the old owner lock). The machine service does not
use them. Do not recreate them.
