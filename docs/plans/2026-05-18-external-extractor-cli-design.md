# External Extractor SQLite CLI Design

**Status:** Draft, Claude review revisions applied
**Created:** 2026-05-18
**Owner:** main session

---

## Context

Julie already has high-quality extraction, persistence, and incremental update
paths, but today's caller-facing shape is workspace/MCP oriented:

- CLI workspace indexing stores under Julie-managed paths like
  `.julie/indexes/{workspace_id}/db/symbols.db`.
- Standalone CLI bootstraps a normal `JulieWorkspace`, then runs the workspace
  indexing command.
- Single-file update/delete behavior exists through the watcher, but it is not
  exposed as a stable process interface for another program.

Other projects need a simpler boundary: call the Julie binary from Go, C#, or
another runtime, pass a source root and a caller-owned SQLite path, and let Julie
write the extracted data idempotently. The host program owns file watching and
calls Julie when files change.

This should be a real process-facing extractor interface, not a Rust FFI layer
and not a flag bolted onto `workspace index`.

## Goals

- Provide a CLI that scans a codebase and writes extracted data to a
  caller-owned SQLite database.
- Provide single-file update and delete commands for external file watchers.
- Make all operations idempotent.
- Keep the external DB SQLite-only. No daemon, no MCP session state, no Tantivy
  requirement, no embeddings requirement.
- Reuse Julie's extractor and canonical persistence code instead of creating a
  second extractor implementation.
- Treat the SQLite schema as a versioned integration contract for consuming
  projects.

## Non-Goals

- No cross-language library bindings.
- No long-running watcher mode in this feature. The host project owns watching.
- No multi-root database in the first version. One external DB represents one
  logical codebase.
- No search-serving API. Consumers read SQLite directly or build their own
  indexes.
- No backward compatibility for every historical Julie DB shape. Consumers must
  check `schema_version`.

## Ignore And File Selection

External extract mode should reuse Julie's existing file selection policy by default:

- respect `.gitignore`, nested `.gitignore`, global gitignore, and `.git/info/exclude`
- respect a root `.julieignore` when present
- exclude Julie's hardcoded blacklisted directories, filenames, and extensions
- exclude `.git`, `.julie`, generated/minified bundles, oversized files, and unsupported/binary files
- use the same supported-extension and likely-text-file fallback as workspace indexing

Unlike normal workspace indexing, external extract mode should not create or modify `.julieignore` by default. The caller owns the project tree as well as the DB path. If vendor-pattern generation is useful, expose it explicitly through an opt-in flag such as `extract scan --write-julieignore`.

Additional ignore inputs should be supported without changing the project tree:

```bash
julie-server extract scan --root /repo --db /path/code.sqlite --ignore-file /path/project.ignore
julie-server extract update --root /repo --db /path/code.sqlite --file src/foo.go --ignore-file /path/project.ignore
```

`--ignore-file` uses gitignore-style syntax and layers after `.gitignore` and `.julieignore`. A repeatable `--ignore <pattern>` flag can be added if implementation cost is low, but the file form is the main integration point for host programs.

The blacklist is still enforced even when an ignore file is supplied. Ignore files narrow the indexable set; they do not make binary, oversized, or hard-blacklisted files indexable.

## CLI Surface

Add a standalone-only top-level command group:

```bash
julie-server extract scan --root /repo --db /path/code.sqlite --json
julie-server extract update --root /repo --db /path/code.sqlite --file src/foo.go --json
julie-server extract delete --root /repo --db /path/code.sqlite --file src/foo.go --json
julie-server extract analyze --db /path/code.sqlite --json
julie-server extract info --db /path/code.sqlite --json
```

Shared flags:

- `--strict-schema`: fail instead of migrating an older external DB
- `--ignore-file <path>`: additional gitignore-style excludes for scan/update
- `--workspace-id <id>`: first-writer override for the generated external
  workspace id
- `--analyze`: run DB-derived analysis after a successful scan/update/delete

### `extract scan`

Discovers all indexable files under `--root`, applies Julie's existing ignore
and language detection rules, hashes files, updates changed/new files, and
removes DB rows for files no longer present on disk.

Default behavior is incremental and idempotent:

- unchanged files are skipped
- changed files are atomically replaced
- new files are inserted
- orphaned DB rows are deleted
- no changes means no new canonical revision

`--force` should be supported for a full rebuild of the caller-owned DB. It
must not use the current workspace-indexing sequence of `delete_workspace_data`
followed by `bulk_store_fresh_atomic`, because those are separate
transactions. External `scan --force` must extract the replacement batch first,
then commit delete+insert in one DB transaction.

### `extract update`

Updates exactly one file.

`--file` may be absolute or root-relative. Julie normalizes it to the same
relative Unix-style path used in the database. Paths outside `--root` are
rejected.

Behavior:

- missing file: return an error suggesting `extract delete`
- unchanged hash: no-op, exit 0
- changed supported file: atomically replace that file's rows
- unsupported or ignored file: delete any stale rows for that path and report
  `ignored`, exit 0

The unsupported/ignored behavior matters for convergence. If a host watcher
calls `update` for a file that used to be indexable but no longer should be,
the external DB should not keep stale symbols.

### `extract delete`

Deletes all DB state for exactly one file path.

Behavior:

- existing DB rows: delete symbols, relationships, identifiers, types,
  annotations, vectors if present, file metadata, repairs, and revision data
  needed for consistency
- absent DB rows: no-op, exit 0
- missing file on disk is expected

Renames are represented by `extract delete --file old_path` followed by
`extract update --file new_path`.

### `extract info`

Prints machine-readable DB metadata. This command must open the database
read-only and must not run migrations or initialize missing schema.

- Julie version that last wrote the DB
- SQLite schema version
- external extract schema contract version
- stored external workspace id
- last root path seen
- file/symbol/relationship/identifier/type counts
- latest canonical revision
- whether DB-derived analysis is current or stale

### `extract analyze`

Runs expensive DB-derived analysis over the existing SQLite data:

- reference scores
- test quality metrics
- test linkage

This is separated from `extract update` so watcher-driven hosts can keep
single-file updates fast. `scan`, `update`, and `delete` mark analysis stale
when they commit a new canonical revision. Recompute happens through
`extract analyze` or an explicit `--analyze` flag.

## JSON Output

All commands support `--json`. Successful operations should emit one object:

```json
{
  "operation": "update",
  "db_path": "/path/code.sqlite",
  "root": "/repo",
  "workspace_id": "external:...",
  "schema_version": 25,
  "revision": 42,
  "files_processed": 1,
  "files_skipped": 0,
  "files_deleted": 0,
  "symbols_total": 1234,
  "relationships_total": 5678,
  "identifiers_total": 9012,
  "analysis_state": "stale",
  "status": "changed"
}
```

Status values:

- `changed`
- `unchanged`
- `ignored`
- `deleted`
- `not_found`
- `scanned`
- `rebuilt`
- `analyzed`

Exit code rules:

- `0`: success, including no-op/unchanged/delete-missing
- non-zero: invalid root, file outside root, DB migration failure, extraction
  failure that cannot preserve prior data, malformed args, lock failure timeout

## Database Contract

The caller owns the DB path. Julie owns the schema inside that file.

The external DB uses Julie's canonical SQLite schema and migrations through
`SymbolDatabase::new(db_path)` for write operations. Initial public read tables:

- `schema_version`
- `files`
- `symbols`
- `symbol_annotations`
- `relationships`
- `identifiers`
- `types`

Internal tables may exist and are not part of the external contract unless a
specific design revision promotes them. Examples: search projection state,
embeddings/vector tables, early-warning report caches, canonical revisions,
revision file changes, and indexing repairs.

Revision and repair details should be exposed through `extract info` JSON
instead of making the internal tables public contract surface.

Add a small external metadata table or equivalent metadata rows:

```sql
CREATE TABLE IF NOT EXISTS external_extract_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

Required keys:

- `julie_version`
- `sqlite_schema_version`
- `extract_contract_version`
- `workspace_id`
- `root_path`
- `created_at`
- `updated_at`
- `analysis_state`
- `analyzed_revision`

The external `workspace_id` must be stored in the DB on first write and reused
for subsequent canonical revisions. It must never be passed as an empty string.

The first writer generates a UUIDv4 workspace id unless the caller supplies
`--workspace-id <id>` on first `scan`. Once stored, the metadata value wins and
later `--workspace-id` values are rejected if they differ. The workspace id
should not be derived from the absolute root path. That keeps the DB usable if
the project directory moves.

One DB represents one logical codebase. If a subsequent command points the same DB
at a different root and the relative file set would be ambiguous, Julie should
fail with a clear root mismatch unless the caller explicitly requests a rebuild
with `scan --force`.

Schema migration policy:

- write operations may migrate an older external DB to the current Julie schema
- `extract info` must not migrate
- if the DB schema is newer than the binary supports, every command fails clearly
- hosts that need strict pinning can use `--strict-schema` to fail instead of
  migrating an older DB

## Path Semantics

For commands that take `--root`, Julie should use one normalized root for all
containment checks and stored paths:

1. expand `~` and make `--root` absolute
2. canonicalize root to `canonical_root`
3. resolve root-relative `--file` values against `canonical_root`
4. make absolute `--file` values absolute as given
5. canonicalize existing file paths before containment checks
6. reject files whose canonical path is outside `canonical_root`
7. store file keys as relative Unix-style paths derived from `canonical_root`

Deleted paths may not exist and cannot always be canonicalized. For `delete`,
normalize the textual absolute path against `canonical_root`, reject obvious
`..` traversal outside root, and then store/delete by the relative Unix-style
key.

Symlink behavior must be explicit: symlinks resolving outside `canonical_root`
are outside the project and are rejected. On Windows, normalize verbatim path
prefixes before deriving stored relative paths.

## Idempotency Contract

`scan` after a successful `scan` with no file changes:

- processes 0 files
- deletes 0 files
- preserves latest canonical revision
- writes no revision rows
- exits 0

`update` after an unchanged file:

- compares the stored Blake3 hash with current content
- writes nothing
- preserves latest canonical revision
- exits 0

`update` after a changed file:

- reads content once
- extracts symbols/relationships/identifiers/types
- atomically deletes old rows for that file and inserts new rows
- stores file metadata, hash, content, symbol count, parse diagnostics
- resolves outbound pending relationships from the touched file and inbound
  references that can now target newly inserted symbols
- marks DB-derived analysis stale
- exits 0

`delete` after a missing or already-deleted file:

- writes nothing if no matching rows exist
- exits 0

`delete` after an indexed file:

- deletes rows owned by the file
- deletes relationships whose source or target symbol belongs to the file
- clears identifier targets in other files when they pointed at deleted symbols
- marks DB-derived analysis stale
- exits 0

Crash behavior:

- failed update leaves old committed data intact
- failed incremental scan leaves old committed data intact for any file whose
  transaction did not commit
- mixed incremental scans that include changed files and orphan deletes must
  commit in one canonical transaction and produce one canonical revision
- failed `scan --force` leaves old committed data intact; extract the full
  replacement batch before starting the DB write, then delete and insert in one
  transaction
- no command should leave half-deleted rows for a file

## Analysis Contract

External extract mode separates canonical extraction from expensive whole-DB
analysis.

Canonical extraction commands (`scan`, `update`, `delete`) maintain files,
symbols, identifiers, relationships, types, parse diagnostics, file hashes, and
canonical revision metadata. They also mark `analysis_state=stale` whenever the
committed revision changes.

`extract analyze` recomputes derived fields such as reference scores, test
quality metrics, and test linkage, then records `analysis_state=current` and
`analyzed_revision=<latest canonical revision>`.

`scan`, `update`, and `delete` do not run whole-DB analysis by default. Hosts
that want eager analysis can pass `--analyze` to a write command or run
`extract analyze` after coalescing watcher events.

## Concurrency Contract

External file watchers often emit bursts or parallel events. SQLite WAL and busy
timeouts are not enough to make scan/update/delete sequencing obvious to callers.

Each write command (`scan`, `update`, `delete`, `analyze`) must take an
exclusive per-DB operation lock for the full duration of the command:

```text
<db_path>.julie-extract.lock
```

Rules:

- only one writer operation per DB at a time
- `scan` blocks concurrent `update`/`delete`
- `update`/`delete` block each other
- `analyze` blocks writes and is blocked by writes
- lock timeout is configurable, default 30s
- timeout exits non-zero with a machine-readable error
- the lock file is durable process coordination state and may remain on disk
  after a command exits

This keeps the public contract simple: callers may invoke commands from a
watcher without building their own write serialization, though queuing events is
still better for throughput.

Lock ordering must be fixed:

1. acquire `<db_path>.julie-extract.lock`
2. open the SQLite DB, which may acquire Julie's existing DB init/migration lock
3. run the operation
4. close DB handles
5. release the extract lock

`extract info` should be read-only. It should either avoid the extract lock and
open SQLite read-only, or take a shared lock if a cross-platform shared-lock
path is available. It must not acquire the exclusive write lock and block behind
long scans unless a platform limitation forces that behavior and the output
documents it.

## Implementation Shape

Do not route this through daemon mode or `ManageWorkspaceTool` as-is.

Add a small SQLite-only extraction module with a deep caller-facing interface:

```rust
pub struct ExternalExtractOptions {
    pub db_path: PathBuf,
    pub root: Option<PathBuf>,
    pub workspace_id: Option<String>,
    pub ignore_files: Vec<PathBuf>,
    pub strict_schema: bool,
    pub analyze_after_write: bool,
}

pub enum ExternalExtractOperation {
    Scan { force: bool },
    Update { file: PathBuf },
    Delete { file: PathBuf },
    Analyze,
    Info,
}

pub async fn run_external_extract(
    operation: ExternalExtractOperation,
    options: ExternalExtractOptions,
) -> Result<ExternalExtractReport>;
```

Expected ownership:

- `src/cli.rs` adds the `extract` command group.
- `src/cli_tools/subcommands.rs` or a new CLI args module defines
  `ExtractArgs`.
- New extraction orchestration lives outside MCP tooling, likely
  `src/external_extract/`.
- Shared extraction helpers should be factored out of
  `src/tools/workspace/indexing/pipeline.rs` and `src/watcher/handlers.rs`
  rather than copied.
- Database writes should reuse `bulk_store_fresh_atomic`,
  `incremental_update_atomic`, and `delete_orphaned_files_atomic` where their
  behavior matches the external contract.
- External mode must not reuse any path that passes `workspace_id = ""` into
  canonical revision writes.
- External `scan --force` needs a new atomic replace primitive, or a refactor of
  existing persistence so delete+insert happen in one transaction.
- External incremental `scan` needs one atomic primitive that can commit changed,
  new, and orphaned files together and record one canonical revision.

The main refactor is to separate three concerns currently coupled in the
workspace pipeline:

1. extraction and batch assembly
2. canonical SQLite persistence
3. Tantivy projection and daemon/session status

External extract mode uses 1 and 2, skips 3, then runs DB-only analysis only
when `analyze` or explicit `--analyze` requests it.

## Error Handling

Hard failures:

- invalid DB path parent
- DB schema newer than this binary supports
- DB migration failure on write commands
- DB schema older than this binary when `--strict-schema` is set
- root path does not exist or is not a directory
- `--file` outside `--root`
- extraction panic or unrecoverable parser failure where replacing old data
  would lose known-good rows
- operation lock timeout

Recoverable states:

- unchanged hash
- delete missing row
- ignored unsupported file
- stale analysis after update/delete
- parser-backed file extracts zero symbols while old symbols exist: preserve old
  rows, record repair state, return non-zero unless the caller opts into
  accepting empty extraction

The last rule matches Julie's current data-loss guard. External mode should not
silently erase useful symbols because a parser failed.

## Tests

Use TDD. First tests should be black-box CLI or near-CLI tests because the CLI
is the integration boundary.

Required tests:

- `extract scan` creates a DB at a caller-owned path and stores files/symbols.
- repeating `extract scan` with no changes is a no-op and preserves revision.
- `extract scan` with mixed changed and orphaned files records one revision.
- `extract scan --force` preserves old data when extraction fails before commit.
- `extract update` on unchanged file is a no-op.
- `extract update` after modifying one file replaces only that file's rows.
- `extract update` marks analysis stale without recomputing whole-DB scores.
- `extract analyze` recomputes derived scores and marks analyzed revision current.
- `extract delete` removes one file's rows.
- repeating `extract delete` exits 0 and does not create a new revision.
- `extract update` rejects files outside root.
- `extract update` rejects symlink paths that resolve outside root.
- concurrent update attempts serialize through the per-DB lock.
- DB root/workspace metadata is persisted and `extract info` reports it.
- `extract info` is read-only and does not migrate an older DB.
- schema-newer-than-binary fails clearly.
- `--strict-schema` rejects older DBs instead of migrating.
- caller-supplied `--ignore-file` excludes matching files.

Suggested first narrow test names:

- `extract_scan_writes_caller_owned_sqlite_db`
- `extract_update_unchanged_file_is_noop`
- `extract_delete_missing_file_is_idempotent`

## Architecture Quality

**Affected modules:** CLI command routing, workspace indexing extraction helpers,
watcher single-file helpers, database persistence, schema metadata.

**Caller-facing interface:** `julie-server extract ...` plus the versioned SQLite
schema at the caller-owned DB path.

**Depth/locality check:** The external caller only knows root path, DB path, and
file path. Julie keeps language detection, ignore policy, extraction,
relationship resolution, schema migration, analysis state, and atomic writes
local.

**Test surface:** Black-box CLI behavior and SQLite rows. Tests should not assert
private helper behavior unless a lower-level unit is needed for error handling.

**Seams/adapters:** A new SQLite-only extraction orchestrator is justified
because it removes daemon/MCP/Tantivy obligations from external callers. Avoid a
pass-through wrapper over `ManageWorkspaceTool`.

**Rejected shortcuts:**

- FFI/cgo/.NET bindings.
- Reusing `.julie/indexes/{workspace_id}` as the external integration surface.
- Making external callers invoke `workspace index` and then locate Julie's DB.
- Copying watcher update logic into a second implementation.
- Trusting SQLite busy timeout alone for watcher burst concurrency.
- Reusing current full-index delete-then-insert persistence for `scan --force`.
- Recomputing whole-DB analysis on every watcher-triggered `update`.

**Architecture risk:** medium. The feature creates a new public integration
surface and makes SQLite schema compatibility more visible.

## Acceptance Criteria

- [ ] External caller can scan a project into an arbitrary SQLite file path.
- [ ] External caller can update one changed file idempotently.
- [ ] External caller can delete one file idempotently.
- [ ] External caller can run DB-derived analysis separately from file updates.
- [ ] External DB does not require `.julie`, daemon, Tantivy, or embeddings.
- [ ] DB schema version, extract contract version, workspace id, and analysis state are queryable.
- [ ] One DB operation lock serializes concurrent extract writes.
- [ ] `extract info` is read-only and does not migrate DBs.
- [ ] `scan --force` and mixed incremental scans preserve atomicity.
- [ ] Existing Julie workspace/MCP indexing behavior is unchanged.
- [ ] Tests cover scan/update/delete/analyze/idempotency/root validation/locking/ignore files.
- [ ] Documentation includes CLI examples and schema contract notes.

## Verification Ledger

Record one row per verification run. Leave empty until commands actually run.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|

## Next Steps

1. Review and approve this design.
2. Convert this design into an implementation plan with task ownership.
3. Start with failing CLI tests for scan/update/delete idempotency.
4. Refactor shared extraction/persistence helpers before adding command code.
