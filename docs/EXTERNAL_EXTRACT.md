# External Extract

`julie-server extract` is a process-facing extractor interface for programs that
want Julie's parser data in a caller-owned SQLite database. It is intended for
hosts written in Go, C#, or other runtimes that already own process management
and file watching.

The command does not use the Julie daemon, MCP transport, Tantivy, or embeddings.
The caller provides a project root and a SQLite path; Julie writes the canonical
SQLite schema into that file.

## Commands

```bash
julie-server extract scan --root /repo --db /var/lib/code.sqlite --json
julie-server extract scan --root /repo --db /var/lib/code.sqlite --force --json
julie-server extract update --root /repo --db /var/lib/code.sqlite --file src/lib.rs --json
julie-server extract delete --root /repo --db /var/lib/code.sqlite --file src/lib.rs --json
julie-server extract analyze --db /var/lib/code.sqlite --json
julie-server extract info --db /var/lib/code.sqlite --json
```

Shared flags:

- `--db <path>`: caller-owned SQLite database path. Required.
- `--root <path>`: project root for `scan`, `update`, and `delete`.
- `--strict-schema`: fail if the DB needs migration.
- `--ignore-file <path>`: extra gitignore-style ignore file. Repeatable.
- `--workspace-id <id>`: first-writer override for the external workspace id.
- `--analyze`: run DB-derived analysis after a mutating command.
- `--json` or `--format json`: machine-readable output.

`analyze` and `info` do not require `--root`.

## Idempotency

All commands are safe to call repeatedly with the same inputs.

- `scan` hashes discovered files, updates changed/new files, deletes orphaned DB
  rows, and creates no new canonical revision when nothing changed.
- `scan --force` extracts first, then replaces the DB contents in one SQLite
  transaction.
- `update` changes exactly one normalized file path. If the hash is unchanged,
  it returns `unchanged` and commits nothing.
- `update` for an ignored or unsupported file deletes stale rows for that path
  and returns `ignored`.
- `delete` removes rows for one file path. Missing DB rows return `not_found`
  with exit code `0`.
- Renames should be sent as `delete old_path` then `update new_path`.
- `update` and `delete` can initialize a missing DB. `update` indexes the one
  file; `delete` creates metadata and returns `not_found`.

If a parser-backed file previously had symbols and a later extraction returns no
symbols or hits a parser failure, Julie preserves the existing rows and exits
non-zero. This prevents a transient parser/read failure from erasing known-good
data. A later successful extraction that returns symbols can replace the file.

## File Selection

External extract reuses Julie's indexing policy:

- respects `.gitignore`, nested `.gitignore`, global gitignore, and
  `.git/info/exclude`
- respects root `.julieignore` if present
- excludes `.git`, `.julie`, generated/minified bundles, oversized files,
  unsupported files, binary files, and hard-blacklisted paths/extensions
- supports extra ignore files through repeatable `--ignore-file`

External extract does not create or modify `.julieignore`. Extra ignore files
only narrow the indexable set; they cannot override Julie's hard blacklist.
The current external extraction file size cap is 1 MiB per file.

## File Paths

`--file` may be absolute or root-relative. Julie canonicalizes it under
`--root` and stores relative Unix-style paths in SQLite. Paths outside `--root`
are rejected.

One DB represents one logical codebase. If a later `scan`, `update`, or `delete`
points the same DB at a different canonical root, Julie returns a root mismatch
error. `scan --force` is the explicit rebuild path for a moved project root and
updates the stored root metadata after the rebuild commits.

## Output

Successful JSON reports use this shape:

```json
{
  "status": "changed",
  "operation": "update",
  "workspace_id": "external:...",
  "db_path": "/var/lib/code.sqlite",
  "root": "/repo",
  "julie_version": "7.10.1",
  "schema_version": 26,
  "schema_state": "current",
  "extract_contract_version": 1,
  "revision": 42,
  "analyzed_revision": null,
  "analysis_state": "stale",
  "missing_metadata_keys": [],
  "files_scanned": 1,
  "files_updated": 1,
  "files_deleted": 0,
  "symbols_extracted": 12,
  "files_total": 100,
  "symbols_total": 2400,
  "relationships_total": 900,
  "identifiers_total": 12000,
  "types_total": 14,
  "errors": []
}
```

`revision` is the latest canonical revision for the external workspace.
`analyzed_revision` is the revision covered by the last successful
`extract analyze`; it is `null` when analysis is stale or has not run.

`extract info --json` uses the same report shape with `operation: "info"` and
zero per-command counters (`files_scanned`, `files_updated`, `files_deleted`,
`symbols_extracted`). It is the canonical way to read `julie_version`,
`schema_state`, `extract_contract_version`, `missing_metadata_keys`, totals, and
analysis state without mutating the DB.

Status values:

- `changed`
- `unchanged`
- `ignored`
- `deleted`
- `not_found`
- `scanned`
- `rebuilt`
- `analyzed`
- `failed`

Exit code rules:

- `0`: success, including no-op and missing-delete cases.
- non-zero: malformed args, invalid root, root mismatch, file outside root,
  schema failure, lock timeout, parser failure that would drop known-good rows,
  or extraction/persistence failure.

On failure, JSON output still uses `ExternalExtractReport` with
`status: "failed"` and one or more `errors`.

## Analysis

`scan`, `update`, and `delete` mark DB-derived analysis stale when they commit a
new canonical revision. Use `extract analyze` to recompute reference scores,
test linkage, and test quality metrics. Use `--analyze` on a mutating command
when synchronous analysis is acceptable.

## Locking

Mutating commands take an exclusive per-DB lock at:

```text
<db_path>.julie-extract.lock
```

The lock is acquired before opening or migrating the SQLite DB. The default
timeout is 30 seconds. The lock file is left on disk after release. `extract
info` opens the DB read-only and does not take the exclusive write lock.

## SQLite Contract

Julie owns the schema inside the caller-owned SQLite file. Consumers should
check `schema_version` and `extract_contract_version` from `extract info`.
Write commands migrate older DBs unless `--strict-schema` is set. `--strict-schema`
fails when the DB is older than the current binary. Any DB newer than the current
binary fails for both read and write commands.

Initial public read tables:

- `schema_version`
- `files`
- `symbols`
- `symbol_annotations`
- `relationships`
- `identifiers`
- `types`

Internal tables may exist and can change unless promoted in a future contract.
Use `extract info` for metadata, counts, latest revision, and analysis state.

Julie opens external extract databases in WAL mode. Hosts should expect SQLite
to create `<db_path>-wal` and `<db_path>-shm` sidecar files next to the main DB.
For backups or handoff snapshots, either include the sidecars with the DB or run
a SQLite checkpoint first.

## Watcher Integration

Host file watchers should call:

```text
created/changed supported file -> extract update --file <path>
deleted file                  -> extract delete --file <path>
rename                        -> extract delete old; extract update new
periodic reconciliation        -> extract scan
```

This keeps the DB convergent even when events are missed or delivered out of
order.
