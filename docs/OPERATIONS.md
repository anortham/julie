# Julie Daemon — Operations and Triage

This document covers operational details for the julie daemon that aren't part of the user-facing feature surface. It's a triage reference for "what is this file in `~/.julie/`?" questions, not an architecture overview.

## Lock files in `~/.julie/`

The daemon coordinates exclusive single-instance access through `~/.julie/daemon.lock`, managed via the `fs2` crate. Two semantics worth knowing:

- **Unix:** `flock(LOCK_EX)` is **advisory**. Other processes that don't call `flock` can still open the file. Cooperative locking only — the lock keeps two well-behaved daemons from racing, but doesn't stop a malicious or naive process from reading the file.
- **Windows:** `LockFileEx` is **mandatory**. Other processes that try to read the locked region get `ERROR_LOCK_VIOLATION`. Stronger isolation, but force-termination of an adapter mid-syscall can briefly leak a held lock that the OS reaps when the process handle is finally closed (typically within seconds).

The daemon process never deletes `daemon.lock` on exit. The file persists across runs and is reused on the next startup. **Seeing a single `daemon.lock` in `~/.julie/` is normal — it's not a leak, it's the durable lock anchor.** Removing it manually while no daemon is running is also safe; the next daemon will recreate it.

## PID file (`daemon.pid`)

`~/.julie/daemon.pid` stores `<pid> <creation_time_unix_micros> <binary_mtime_unix_micros>` in a single line. The creation_time is the daemon's kernel-reported start time, which prevents PID-reuse impersonation (a different process inheriting a recycled PID cannot fool the adapter into thinking the original daemon is alive).

If you see a stale `daemon.pid` after a hard crash, `julie-server` cleans it up automatically on next start by checking whether the stored PID is still alive AND the creation_time matches.

## State file (`daemon.state`)

`~/.julie/daemon.state` is an advisory string (`ready` / `draining` / `stopping`) updated atomically via temp+rename. Concurrent readers never observe a partial write. The file is purely informational — the daemon does not depend on it for correctness.

## Tantivy schema compatibility and auto-rebuild

Per-workspace Tantivy indexes live at `~/.julie/indexes/{workspace_id}/tantivy/` (daemon mode) or `<project>/.julie/indexes/{workspace_id}/tantivy/` (stdio mode). Alongside the Tantivy directory, Julie writes a small JSON sidecar (`julie-search-compat.json`) that records:

- `marker_version` — a Julie-owned format version, bumped when the sidecar layout changes.
- `schema_signature` — a structured fingerprint of every field name + field-type in the persisted schema. Built from `compatibility_signature()` in `src/search/schema.rs`.
- `tokenizer_signature` — the code-tokenizer signature (token patterns, language configs).

On every `SearchIndex::open`, Julie compares the expected signatures against what's on disk via `index_is_compatible()` (`src/search/index.rs`). On mismatch:

1. The mismatched Tantivy directory is **deleted and recreated empty** with the new schema. One `WARN` log line: `Tantivy index at <path> is incompatible with Julie expectations, recreating empty index`.
2. The workspace open path notices `SearchIndexOpenDisposition::RecreatedIncompatible` and calls `projection.repair_recreated_open_if_needed`, which **rebuilds the Tantivy projection from SQLite** (canonical source-of-truth). One `WARN` log line: `Tantivy search index at <path> was recreated empty during open; rebuilding projection from canonical SQLite state`.
3. Concurrent re-creation is guarded by a `.recreating` lock file inside the index directory; concurrent openers either reuse the freshly created index or back off and rebuild locally.

**Operator impact when upgrading across a schema-changing release** (e.g., the C.3 reranker upgrade that added `role`, `test_role`, `capability_flags` fields):

- First daemon start after upgrade triggers the auto-rebuild for every workspace whose Tantivy index pre-dates the new schema. SQLite is untouched; only the Tantivy directory is recreated.
- Rebuild cost is proportional to symbol count (the rebuild iterates `symbols` rows in SQLite and writes Tantivy documents). On a workspace with N symbols, expect roughly the same wall-clock as an initial `manage_workspace operation="index"` — typically seconds, not minutes.
- No user action required. No reindex flag needed. The watcher and search both stay functional during the rebuild (reads are gated to `false` for the duration; reads resume when the rebuild commits).
- If you want to verify the auto-rebuild fired, grep daemon log for `recreating empty index` or `recreated empty during open; rebuilding projection`.

**Forced rebuild:** If you suspect a corrupt Tantivy directory but the signatures match, you can manually remove the workspace's `tantivy/` directory (including its `julie-search-compat.json` sidecar) — the next session will recreate them via the same path. Or run `manage_workspace operation="index"` with `force=true`.
