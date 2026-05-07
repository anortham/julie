# Daemon Lifecycle & Stale-Binary Replacement: Windows-Lens Review

**Date:** 2026-05-06
**Reviewer:** Claude (Opus 4.7) on macOS, with Windows file-locking semantics in mind
**Scope:** `src/daemon/{mod.rs, lifecycle.rs, pid.rs, mcp_session.rs, http_transport.rs, embedding_service.rs, shutdown_event.rs}`, `src/adapter/launcher.rs`, `src/paths.rs`, `src/search/index.rs`

> Caveat: this review was done from a macOS workstation. Behavior described as "Windows" is based on documented Win32 semantics (`LockFileEx`, NTFS sharing modes, mandatory file locks, PID recycling, ReadDirectoryChangesW lifecycle), not from running the daemon on a Windows host. A Windows CI run that exercises the restart loop would be the only way to fully validate or refute these findings.

---

## How the flow works (recap)

1. **Daemon startup** (`run_daemon` in `src/daemon/mod.rs:234`): captures `binary_mtime()` from `current_exe()`, opens `daemon.db`, builds the `WorkspacePool` / `WatcherPool` / `EmbeddingService`, binds an HTTP MCP transport on `127.0.0.1:0`, writes `daemon.state="ready"`, then `select!`s on three shutdown sources: SIGTERM/SIGINT, `restart_notify`, or the Windows shutdown event.
2. **Stale-binary detection** runs at two points per session:
   - On `initialize` via `admit_initialize` → `stale_binary_accept_action` (`lifecycle.rs:276`).
   - On disconnect via `apply_disconnect_action` → `stale_binary_disconnect_action` (`lifecycle.rs:293`).
   - Comparison is `current_mtime > startup_mtime`, where `startup_mtime` is captured once at daemon launch.
3. **Restart trigger**: when stale, the controller flips `restart_pending=true` and either marks pending (sessions remain) or fires `notify_restart` (last session gone). `run_daemon`'s `select!` wakes on the notify, drains for ≤5 s, tears resources down.
4. **Adapter side** (`src/adapter/launcher.rs`): `daemon_readiness()` → `(Ready | Starting | Stopping | Dead)`; if `Stopping`, `wait_for_pid_exit`; if `Dead`, acquire `daemon.lock`, `spawn_daemon()`, poll `daemon.state` for `"ready"`.

The architecture cleanly *separates the policy* (pure functions in `lifecycle.rs` like `stale_binary_disconnect_action`, easy to unit-test) from *the side-effecting plumbing* (`apply_disconnect_action` in `mcp_session.rs`). That split is good and is why the logic is review-able at all. Most of the issues below live in the plumbing half, not the policy half.

---

## Findings

### 🔴 1. Per-workspace Tantivy/SQLite resources are dropped implicitly — not explicitly shut down

**Where:** `run_daemon`'s shutdown sequence (`src/daemon/mod.rs:663-680`).

The shutdown calls `embedding_service.shutdown()` and `http_transport.shutdown().await` but *never* calls anything analogous for `WorkspacePool` or `WatcherPool`. There is a `SearchIndex::shutdown()` at `src/search/index.rs:1200` that *does* commit the writer and explicitly drop it to release the Tantivy lock — its own doc-comment even calls out *"release the Tantivy file lock"* — but nothing in the shutdown path invokes it.

**Consequences:**
- **All platforms:** Buffered, uncommitted Tantivy writes are lost when `IndexWriter` is dropped without `commit()`. Tantivy's `IndexWriter::drop` releases the lock but does not persist uncommitted segments. So recent indexing work can be lost on every clean restart.
- **Windows specifically:** When the process exits, the OS reclaims handles, but the order is undefined. If the new daemon (spawned by the adapter ~50–500 ms later) tries to open the per-workspace Tantivy index before the kernel has finished tearing down the old process's `.tantivy-writer.lock` handle, the new `IndexWriter` open returns `LockBusy`. There's no retry path that I can see.

**Recommendation:** Add an explicit `WorkspacePool::shutdown()` that walks every per-workspace `SearchIndex` and calls its existing `shutdown()` method, then call it from `run_daemon` *before* dropping the pool. Mirror it for the watcher pool (call notify shutdown explicitly).

---

### 🔴 2. Adapter races OS handle cleanup after `wait_for_pid_exit`

**Where:** `wait_for_pid_exit` (`src/adapter/launcher.rs:240`) returns as soon as `is_daemon_running()` becomes false, then `wait_for_daemon_ready` falls through to `Dead` and immediately `spawn_daemon()`.

On Windows, "process gone" ≠ "all handles released":
- The kernel guarantees handles are reclaimed *during* `NtTerminateProcess`, before the PID is freed, so `OpenProcess` failing means the process object is gone. So far so good.
- BUT child processes (the Python embedding sidecar, spawned via `uv`) inherit handles unless `STARTUPINFOEX` with explicit `bInheritHandles=FALSE` was used. If the sidecar lingers, it keeps any inherited handles to `~/.julie/` files alive.
- `EmbeddingService::shutdown()` (`src/daemon/embedding_service.rs:285`) calls `provider.shutdown()` and returns immediately — no `wait()` for the sidecar process to exit. So if the sidecar dawdles (Python interpreter shutdown can be 100s of ms on Windows), it survives the daemon parent and the new daemon's startup races against it.

**Recommendation:** Have the embedding service's `shutdown()` await the child process's exit with a bounded timeout (say 2–5 s). On Windows, also confirm sidecar `Command` is built with stdin/stdout/stderr null *and* `CREATE_NEW_PROCESS_GROUP` so the parent's handles don't get inherited.

---

### 🟠 3. PID-reuse defense is missing

**Where:** `PidFile::is_process_alive` (`src/daemon/pid.rs:59`) and `read_pid` + `check_running`.

The PID file stores only `process::id()`. `is_process_alive` checks "does *some* process with this PID exist." Windows recycles PIDs aggressively — typically within seconds on a busy machine.

**Failure mode:**
1. Daemon writes PID 4232, exits abnormally without removing PID file.
2. Within 30 s, Chrome/Slack/anything starts and gets PID 4232.
3. Adapter calls `check_running` → `OpenProcess(4232)` succeeds → "daemon is alive" → adapter waits → 60-s timeout → user-facing failure.

The Unix path has the same theoretical problem but Linux defaults `kernel.pid_max=4194304` (or higher), so collision is rare during a single session. Windows is genuinely vulnerable.

**Recommendation:** Write `(pid, process_creation_time)` (Windows: `GetProcessTimes` → `CreationTime`; Unix: `/proc/<pid>/stat` start time or `getpid()`+`clock_gettime(CLOCK_BOOTTIME)`) and require both to match. Alternatively, store the daemon binary mtime in the PID file too — a process running an unrelated binary can't impersonate.

---

### 🟠 4. `PidFile::create_exclusive` retry loop has no backoff

**Where:** `src/daemon/pid.rs:131-156`.

```rust
loop {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        ...
        Err(e) if e.kind() == AlreadyExists => {
            ...
            let _ = fs::remove_file(path);  // may silently fail on Windows
            retries += 1;
            if retries >= MAX_RETRIES { bail!(...) }
        }
    }
}
```

Two Windows-specific things bite here:
- `fs::remove_file` can fail with `ERROR_SHARING_VIOLATION` (32) if another process has the file open with a non-`FILE_SHARE_DELETE` share mode. The error is swallowed (`let _ =`).
- The next `create_new(true)` then fails again with `AlreadyExists`. Loop. Ten iterations burn through with zero delay (microseconds). Bail.

On Unix this is fine because `unlink` works even when other processes have the file open. On Windows, the file is sticky.

**Recommendation:** Add `std::thread::sleep(Duration::from_millis(50 * (1 << retries)))` before each retry, and propagate the actual `remove_file` error if it isn't `NotFound`.

---

### 🟠 5. `daemon.state` write is non-atomic on Windows

**Where:** `lifecycle.rs:325` (`write_daemon_state`) does `std::fs::write(path, state)`.

On Linux, `std::fs::write` is `open(O_TRUNC) + write` — between truncate and write the file is empty, but the syscalls execute fast enough that the empty window is microseconds. On Windows, `CreateFile(CREATE_ALWAYS) + WriteFile + CloseHandle` has the same shape, but there's an extra wrinkle: another process reading concurrently can observe **partial content** (e.g., `"rea"` instead of `"ready"`).

The adapter's polling code:

```rust
if let Ok(s) = std::fs::read_to_string(self.paths.daemon_state()) {
    let state = s.trim();
    if state == target_state { return Ok(()); }
    if target_state == "ready" && state == "draining" { return Ok(()); }
    if target_state == "ready" && state == "stopping" { return Err(...); }
}
```

A partial read gives a string that matches *none* of the branches, so the loop just iterates. **Functionally safe**, but only because every state name is a distinct prefix-free token. If someone ever adds `"ready_quiesce"` or similar, partial reads of `"ready"` could misclassify mid-truncation. Brittle invariant.

**Recommendation:** Use the same write-temp + atomic-rename pattern as `PidFile::create` (lines 27-45). On Windows, `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` is the atomic primitive. Cheap fix, removes the fragility.

---

### 🟠 6. Stale-binary detection is largely inert on Windows for in-place rebuilds

**Where:** the project's own `CLAUDE.md` documents that on Windows, `cargo build --release` fails with "Access is denied" while a daemon is running, because the OS holds an image-section lock against the running .exe.

**Implication:**
- The intended trigger for stale-binary detection — a developer running `cargo build --release` against a live daemon — *cannot happen* on Windows.
- The detection still fires for: (a) an installer that uses `MoveFileEx(MOVEFILE_DELAY_UNTIL_REBOOT)`, (b) a `touch` that updates mtime without changing bytes, (c) the binary being deleted and a fresh-named one with `MoveFile()` racing in.

So on Windows the feature is effectively a "next-cold-start" mechanism, not a "live-rebuild" one. That's not a bug per se — but the logging, comments, and design doc all assume the live-rebuild flow. If a Windows user reports "I rebuilt and the daemon never restarted," the answer is "you couldn't have rebuilt; you must have stopped first." Worth documenting plainly in code comments, because the current flow gives misleading log lines on Windows ("Binary mtime captured for stale-binary detection") that suggest something is being protected when nothing actually is.

**Recommendation:** Make the comment at `daemon/mod.rs:302-310` Windows-honest: note that on Windows the in-place rebuild path is blocked by the OS file lock and this detection only triggers when the binary is replaced via `MoveFileEx` or out-of-band means.

---

### 🟡 7. 5-second drain timeout is calibrated for Linux

**Where:** `drain_sessions(&sessions, Duration::from_secs(5))` at `src/daemon/mod.rs:644`.

On Linux, in-flight HTTP requests finish quickly because SQLite/Tantivy commits are fast. On Windows, fsync on NTFS plus per-workspace Tantivy commits during a stale-binary restart can comfortably exceed 5 s on a cold cache, especially with multiple workspaces. The drain times out, sessions are forced shut, in-flight writes are dropped.

Minor because most sessions don't have heavy in-flight work at restart time, but if a `force_full_index` is mid-flight when stale-binary fires, you'd lose hours of work without warning.

**Recommendation:** Bump to 10 s on Windows, or make the timeout configurable. The blocker isn't the wait — it's that we don't surface "drain timed out" loudly enough to catch the data-loss case.

---

### 🟡 8. Shutdown ordering: embeddings down before HTTP

**Where:** `src/daemon/mod.rs:668-673`.

```rust
embedding_service.shutdown();
info!("Embedding service shut down");

if let Err(e) = http_transport.shutdown().await { ... }
```

If `drain_sessions` timed out (line 648-652 logs `"forcing shutdown"`), there are still HTTP requests in-flight when we kill the embedding service. Any tool call that needs an embedding (e.g., `fast_refs` zero-result fallback, `deep_dive` related-symbols) returns a "service unavailable" error to a session that thought it was still healthy.

This is a window of time, not a hang. But the *correct* ordering is: HTTP transport first (stops new requests), drain HTTP server task, then embedding service, then pools.

**Recommendation:** Reorder to `http_transport.shutdown().await` → `embedding_service.shutdown()` → workspace pool shutdown → watcher pool shutdown. Treat it as LIFO of the dependency graph.

---

### 🟡 9. Restart-pending decision races against the version gate

**Where:** `admit_initialize` (`src/daemon/mcp_session.rs:452`) calls *both* `stale_binary_accept_action` *and* `version_gate_action` (via two consecutive `apply_admission_action` calls).

If the binary is stale *and* the adapter version mismatches, `apply_admission_action` for the stale-binary check may return `Ok(())` (with `AcceptWithRestartPending`), then the version-gate call returns `RejectForRestart`. Both paths call `mark_restart_pending`; the second sees `first_request=false` because the first already flipped the bit. Functionally fine — the daemon restarts either way — but the log lines tell a confusing story: "accepted while restart pending" *then* "rejecting while waiting to restart" for the *same* admit attempt. Debug-readability cost, not a correctness cost.

**Recommendation:** Short-circuit: if the first admission action returns an error, don't run the second. Move the second `apply_admission_action` inside an `Ok(_) =>` arm.

---

### 🟢 10. `daemon.lock` advisory locking semantics differ Unix vs Windows

**Where:** `ensure_daemon_ready` uses `fs2::FileExt::lock_exclusive()` on `daemon.lock`.

On Unix, fs2 → `flock()` (advisory). On Windows, fs2 → `LockFileEx` (mandatory). If an adapter process panics while holding the lock, Unix releases it on file-close. Windows *also* releases on close, but if the process is force-terminated mid-syscall, there's a brief window where the lock can persist until the kernel completes cleanup. Empirically rare.

More notable: the lock file is *never deleted*. It accumulates. Not a leak (single file), but a stale `daemon.lock` on disk is sometimes confusing in a `ls -la ~/.julie/` triage.

No action required, but worth noting in any "operations" doc.

---

### 🟢 11. PID file path traversal under non-ASCII Windows paths

**Where:** `paths.rs:20` (`home.join(".julie")`) and `julie_home_hash` (`paths.rs:78`).

`to_string_lossy()` in `julie_home_hash` will mangle non-UTF-8 segments on Windows (very rare but possible with legacy filesystems). The hash differs across runs if any non-UTF-8 byte gets replaced with U+FFFD inconsistently. Practically: only matters for the `daemon_shutdown_event` name, which is per-user-per-home anyway.

No realistic action required. Flagged for completeness.

---

## Summary table (Windows-only severity)

| # | Issue | Severity | One-line fix |
|---|---|---|---|
| 1 | No explicit per-workspace `SearchIndex` shutdown | 🔴 high | Add `WorkspacePool::shutdown()` calling `SearchIndex::shutdown()` for each workspace |
| 2 | Adapter spawns new daemon before sidecar fully exits | 🔴 high | `EmbeddingService::shutdown()` should wait for child exit |
| 3 | No PID-reuse defense | 🟠 medium | Store `(pid, creation_time)` or binary mtime in PID file |
| 4 | `create_exclusive` retry loop has no backoff | 🟠 medium | Add exponential sleep + propagate `remove_file` non-`NotFound` errors |
| 5 | `daemon.state` non-atomic write | 🟠 medium | Switch to write-temp + rename |
| 6 | Stale-binary detection inert under Windows in-place rebuild | 🟠 docs | Update comment to reflect platform constraint |
| 7 | 5-s drain timeout calibrated for Linux | 🟡 low | Make timeout configurable, surface drain-timeout louder |
| 8 | Embedding shutdown before HTTP shutdown | 🟡 low | Reorder LIFO |
| 9 | Two admission actions in sequence both fire `mark_restart_pending` | 🟡 low | Short-circuit on first error |
| 10 | fs2 lock semantics differ + lock file never cleaned | 🟢 cosmetic | Document; no fix |
| 11 | Non-UTF-8 home path edge case in `julie_home_hash` | 🟢 cosmetic | Document; no fix |

---

## What I'd do first

If I were tackling this, I'd land #1 and #2 together as a single PR ("Daemon shutdown: explicit per-workspace and sidecar teardown") because they share the same theme — *make resource release ordered and synchronous*, instead of relying on Drop-during-stack-unwind. Both also have the property that they help on Linux too (data loss on uncommitted Tantivy is platform-agnostic).

#3 (PID-reuse) is independent and can ride alongside. #4 and #5 are quality-of-life and small enough to bundle into a "Windows file-handling polish" PR.

The harder thing — and what would actually validate this analysis — is a Windows CI job that exercises the restart loop. I don't see one in the test buckets, and you can't really fix-and-forget Windows lifecycle issues without a machine that runs them. Worth flagging if there isn't one in CI.

---

## Files referenced

| Path | What it does |
|---|---|
| `src/daemon/mod.rs` | `run_daemon` main loop, `binary_mtime`, shutdown sequence |
| `src/daemon/lifecycle.rs` | Phase transitions, stale-binary policy functions, `stop_daemon` |
| `src/daemon/pid.rs` | PID file lifecycle, `create_exclusive`, `is_process_alive` |
| `src/daemon/mcp_session.rs` | `apply_admission_action`, `apply_disconnect_action` (where stale fires) |
| `src/daemon/http_transport.rs` | HTTP MCP transport bind/shutdown |
| `src/daemon/embedding_service.rs` | `shutdown()` (calls `provider.shutdown()` non-blocking) |
| `src/daemon/shutdown_event.rs` | Windows named-event graceful-shutdown mechanism |
| `src/adapter/launcher.rs` | `DaemonReadiness`, `wait_for_pid_exit`, `spawn_daemon` |
| `src/paths.rs` | `daemon_pid`, `daemon_lock`, `daemon_state`, `daemon_shutdown_event` |
| `src/search/index.rs` | `SearchIndex::shutdown` (commits writer, releases Tantivy lock — currently uncalled at daemon shutdown) |
