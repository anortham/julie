# Julie Daemon — Operations and Triage

This document covers operational details for the julie daemon that aren't part of the user-facing feature surface. It's a triage reference for "what is this file in `$JULIE_HOME/`?" (default `~/.julie/`) questions, not an architecture overview.

`JULIE_HOME` overrides the daemon home directly — the path is used as-is, with `.julie` **not** appended. If `JULIE_HOME` is unset, the daemon home is `~/.julie`. See `Moving Julie state with JULIE_HOME` below for the migration workflow.

## Lock files in `$JULIE_HOME/`

The daemon coordinates exclusive single-instance access through `$JULIE_HOME/daemon.lock`, managed via the `fs2` crate. Two semantics worth knowing:

- **Unix:** `flock(LOCK_EX)` is **advisory**. Other processes that don't call `flock` can still open the file. Cooperative locking only — the lock keeps two well-behaved daemons from racing, but doesn't stop a malicious or naive process from reading the file.
- **Windows:** `LockFileEx` is **mandatory**. Other processes that try to read the locked region get `ERROR_LOCK_VIOLATION`. Stronger isolation, but force-termination of an adapter mid-syscall can briefly leak a held lock that the OS reaps when the process handle is finally closed (typically within seconds).

The daemon process never deletes `daemon.lock` on exit. The file persists across runs and is reused on the next startup. **Seeing a single `daemon.lock` in `$JULIE_HOME/` is normal — it's not a leak, it's the durable lock anchor.** Removing it manually while no daemon is running is also safe; the next daemon will recreate it.

## PID file (`daemon.pid`)

`$JULIE_HOME/daemon.pid` stores `<pid> <creation_time_unix_micros> <binary_mtime_unix_micros>` in a single line. The creation_time is the daemon's kernel-reported start time, which prevents PID-reuse impersonation (a different process inheriting a recycled PID cannot fool the adapter into thinking the original daemon is alive).

If you see a stale `daemon.pid` after a hard crash, `julie-server` cleans it up automatically on next start by checking whether the stored PID is still alive AND the creation_time matches.

## State file (`daemon.state`)

`$JULIE_HOME/daemon.state` is an advisory string (`ready` / `draining` / `stopping`) updated atomically via temp+rename. Concurrent readers never observe a partial write. The file is purely informational — the daemon does not depend on it for correctness.

## Tantivy schema compatibility and auto-rebuild

Per-workspace Tantivy indexes live at `$JULIE_HOME/indexes/{workspace_id}/tantivy/` (daemon mode, default `~/.julie/indexes/...`) or `<project>/.julie/indexes/{workspace_id}/tantivy/` (stdio mode, unaffected by `JULIE_HOME`). Alongside the Tantivy directory, Julie writes a small JSON sidecar (`julie-search-compat.json`) that records:

- `marker_version` — a Julie-owned format version, bumped when the sidecar layout changes.
- `schema_signature` — a structured fingerprint of every field name + field-type in the persisted schema. Built from `compatibility_signature()` in `src/search/schema.rs`.
- `tokenizer_signature` — the code-tokenizer signature (token patterns, language configs).

On every `SearchIndex::open`, Julie compares the expected signatures against what's on disk via `index_is_compatible()` (`src/search/index.rs`). On mismatch:

1. The mismatched Tantivy directory is **deleted and recreated empty** with the new schema. One `WARN` log line: `Tantivy index at <path> is incompatible with Julie expectations, recreating empty index`.
2. The workspace open path notices `SearchIndexOpenDisposition::RecreatedIncompatible` and calls `projection.repair_recreated_open_if_needed`, which **rebuilds the Tantivy projection from SQLite** (canonical source-of-truth). One `WARN` log line: `Tantivy search index at <path> was recreated empty during open; rebuilding projection from canonical SQLite state`.
3. Concurrent re-creation is guarded by a `.recreating` lock file inside the index directory; concurrent openers either reuse the freshly created index or back off and rebuild locally.

**Operator impact when upgrading across a schema-changing release** (e.g., the C.3 reranker upgrade that added `role` and `test_role` fields, or the v7.9.x cleanup that dropped the unused `capability_flags` field):

- First daemon start after upgrade triggers the auto-rebuild for every workspace whose Tantivy index pre-dates the new schema. SQLite is untouched; only the Tantivy directory is recreated.
- Rebuild cost is proportional to symbol count (the rebuild iterates `symbols` rows in SQLite and writes Tantivy documents). On a workspace with N symbols, expect roughly the same wall-clock as an initial `manage_workspace operation="index"` — typically seconds, not minutes.
- No user action required. No reindex flag needed. The watcher and search both stay functional during the rebuild (reads are gated to `false` for the duration; reads resume when the rebuild commits).
- If you want to verify the auto-rebuild fired, grep daemon log for `recreating empty index` or `recreated empty during open; rebuilding projection`.

**Forced rebuild:** If you suspect a corrupt Tantivy directory but the signatures match, you can manually remove the workspace's `tantivy/` directory (including its `julie-search-compat.json` sidecar) — the next session will recreate them via the same path. Or run `manage_workspace operation="index"` with `force=true`.

## Moving Julie state with JULIE_HOME

`JULIE_HOME` relocates the daemon home directory. Default is `~/.julie`. Set `JULIE_HOME=/some/path` and the daemon uses `/some/path` directly — `.julie` is **not** appended. `daemon.db`, lock/pid/port/token/discovery files, adapter and daemon logs, migration state, and `indexes/<workspace_id>/...` all move with it.

**Important caveats:**

- All Julie processes (adapter, daemon, CLI tools, MCP clients) must see the same `JULIE_HOME` value. If your MCP client launches `julie-server` with one value and a shell sees a different value, you get two independent daemon identities and registries.
- `JULIE_HOME` is unrelated to `JULIE_WORKSPACE`. `JULIE_WORKSPACE` only chooses the current workspace root; it does not move daemon state or indexes.
- Setting `JULIE_HOME` does not auto-migrate existing data. The new home starts empty and gets its own daemon identity and workspace registry. To preserve your existing indexes, move the contents of the old home yourself.
- Per-workspace project logs at `<project>/.julie/logs` are project-local and are not affected by `JULIE_HOME`.
- An empty `JULIE_HOME` value is a hard error at startup. Either set a non-empty path or unset the variable to fall back to `~/.julie`.

**Migration checklist:**

1. **Stop the daemon.** Run `julie-server stop` and confirm no stray `julie-daemon` / `julie-server` processes remain.
2. **Move the existing home.** For example:
   ```bash
   mv ~/.julie /mnt/fast-ssd/julie-home
   ```
3. **Set `JULIE_HOME` everywhere Julie runs.** This includes:
   - The MCP client environment (Claude Code, Codex CLI, OpenCode, VS Code, etc.) — set it where the client launches Julie, not just in your interactive shell.
   - Any shell you use to invoke `julie-daemon`, `julie-server`, or Julie CLI tools.
   - Any service manager unit (launchd, systemd, etc.) that supervises the daemon.
   ```bash
   export JULIE_HOME=/mnt/fast-ssd/julie-home
   ```
4. **Start or restart Julie.** Bring your MCP client up; the adapter will start a fresh daemon under the new home.
5. **Verify the new home is in use.** Either:
   ```bash
   julie-server status
   ```
   and check the reported paths, or list `$JULIE_HOME/`:
   ```bash
   ls "$JULIE_HOME/daemon.db" "$JULIE_HOME/indexes/"
   ```
   You should see `daemon.db` and the moved per-workspace index directories.

If `julie-server status` reports paths under `~/.julie` after the move, one of the launching processes is not seeing your new `JULIE_HOME`. Fix that before doing more work — otherwise indexing will silently write into two homes.
