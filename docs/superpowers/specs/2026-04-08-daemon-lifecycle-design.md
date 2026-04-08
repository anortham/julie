# Daemon Lifecycle Robustness: State File Design

**Date:** 2026-04-08
**Status:** Approved
**Scope:** Daemon startup, shutdown, restart, and adapter connection logic

## Problem

The daemon lifecycle has race conditions that cause Julie to fail to start after a daemon restart. The root cause: the adapter uses the PID file as a readiness signal when it only indicates process liveness.

### Specific Failure Sequence

When the daemon shuts down for stale-binary restart, cleanup runs in this order:

1. `drain_sessions` (up to 5s)
2. `embedding_service.shutdown()` (variable, kills Python sidecar)
3. `listener.cleanup()` -- removes IPC socket
4. `remove_file(port)` -- removes port file
5. `pid_file.cleanup()` -- removes PID file

Between steps 3 and 5, the adapter sees a live PID but can't connect (socket gone). The adapter's retry budget (3 attempts, 2s apart, ~6s total) can be exhausted if shutdown takes longer.

### Additional Bugs

- **`stop_daemon()` split-brain:** After waiting 5s for shutdown, it unconditionally removes the PID file and socket even if the process is still alive. This can orphan a running daemon and let a second one start.
- **Lock released too early:** `ensure_daemon_running()` drops `daemon.lock` before `wait_for_socket()`, so a second adapter can see the PID (early in startup, before the socket is ready), skip the wait, and fail to connect.
- **Cold-start race:** PID file is created long before the IPC socket is bound. An adapter arriving during that window takes the "already running" fast path and fails to connect.

## Design

### State File

Add `~/.julie/daemon.state` alongside the existing PID and lock files. Contains a single word representing the daemon's current phase.

**Path:** `DaemonPaths::daemon_state()` -> `~/.julie/daemon.state`

**Contents:** One of `starting`, `ready`, or `stopping`. Plain text, no JSON, no newline required.

**Write strategy:** `std::fs::write()`. For a 7-8 byte file, partial reads are not a practical concern. On Windows, both `std::fs::write` and `std::fs::read_to_string` open-write/read-close immediately with no held handles, avoiding sharing violations.

**Crash safety:** A stale state file (process dead) is detected by PID liveness check, same as stale PID files. Dead PID + any state = clean up both and spawn fresh.

### State Transitions (Daemon Side)

All state writes happen in `run_daemon()`:

```
PidFile::create_exclusive()
    |
    v
write "starting"      <-- after PID created, before any initialization
    |
    v
[DB open, normalize, embedding init, IPC bind]
    |
    v
write "ready"          <-- after IpcListener::bind(), before accept_loop starts
    |
    v
[accept_loop runs, serving sessions]
    |
    v
write "stopping"       <-- FIRST thing in shutdown, before drain/cleanup
    |
    v
[drain sessions, shutdown embeddings, cleanup socket, cleanup port]
    |
    v
delete daemon.state    <-- after pid_file.cleanup(), very last step
```

Key: `stopping` is written BEFORE any cleanup, not after. This gives the adapter maximum warning.

### Readiness Assessment (Adapter Side)

New `DaemonReadiness` enum and `daemon_readiness()` function replace `is_daemon_running() -> bool`:

```rust
enum DaemonReadiness {
    Ready,              // PID alive, state = "ready"
    Starting,           // PID alive, state = "starting" or missing/unreadable
    Stopping,           // PID alive, state = "stopping"
    Dead,               // No PID or PID dead (clean up stale files)
}

fn daemon_readiness(&self) -> DaemonReadiness {
    match PidFile::check_running(&self.paths.daemon_pid()) {
        None => {
            // Clean up stale state file if present
            let _ = std::fs::remove_file(self.paths.daemon_state());
            DaemonReadiness::Dead
        }
        Some(_pid) => {
            match std::fs::read_to_string(self.paths.daemon_state()) {
                Ok(s) if s.trim() == "ready" => DaemonReadiness::Ready,
                Ok(s) if s.trim() == "stopping" => DaemonReadiness::Stopping,
                _ => DaemonReadiness::Starting, // "starting", missing, or unreadable
            }
        }
    }
}
```

### Rewritten Adapter Startup

`ensure_daemon_running()` becomes `ensure_daemon_ready()`. The lock is held through the entire readiness confirmation:

```rust
pub fn ensure_daemon_ready(&self) -> io::Result<()> {
    // Fast path (no lock): if daemon is ready, skip the lock.
    // If the daemon transitions to stopping between this check and
    // connect_and_handshake, run_adapter's retry loop catches it.
    if matches!(self.daemon_readiness(), DaemonReadiness::Ready) {
        return Ok(());
    }

    // Slow path: acquire lock and handle state transitions
    let lock_file = acquire_daemon_lock()?;
    let deadline = Instant::now() + Duration::from_secs(60);

    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Timed out waiting for daemon readiness",
            ));
        }

        match self.daemon_readiness() {
            DaemonReadiness::Ready => {
                // Daemon is ready to accept connections
                drop(lock_file);
                return Ok(());
            }
            DaemonReadiness::Starting => {
                // Daemon is initializing, wait for it
                poll_for_ready(&self.paths, deadline)?;
                // Loop back to re-check
            }
            DaemonReadiness::Stopping => {
                // Daemon is shutting down, wait for it to exit
                wait_for_pid_exit(&self.paths, deadline)?;
                // PID gone, fall through to Dead on next iteration
            }
            DaemonReadiness::Dead => {
                // No daemon running, spawn one
                self.spawn_daemon()?;
                poll_for_ready(&self.paths, deadline)?;
                // Loop back to re-check
            }
        }
    }
}
```

**`poll_for_ready()`** replaces `wait_for_socket()`. Polls the state file with exponential backoff (50ms, 100ms, 200ms, 400ms, 500ms cap). Returns `Ok(())` when state becomes `ready`. Returns `Err` on deadline exceeded. If the state becomes `stopping` (daemon went ready->stopping before we saw ready) or the PID dies, returns a distinguishable error so the caller can loop back through the state machine rather than giving up.

**`wait_for_pid_exit()`** polls `PidFile::check_running()` with exponential backoff. Returns when PID is gone, or errors on deadline exceeded.

**Lock held through readiness:** Other adapters block on the lock. When they acquire it, the daemon is already `ready`. Instant fast-path. This eliminates multi-adapter startup races.

### `run_adapter` Retry Loop

The retry loop in `run_adapter()` remains as a safety net, but the semantics change:

- `ensure_daemon_ready()` now guarantees `ready` state before returning
- Connection failures after `ready` are genuinely unexpected (daemon crash between state write and accept)
- Keep 3 retries with 2s delays as defense-in-depth
- On retry, call `ensure_daemon_ready()` again (which may wait for a `stopping` -> spawn cycle)

### `stop_daemon()` Fix

Current: unconditionally removes PID+socket after 5s wait.

Fixed:
1. Send SIGTERM / shutdown event (unchanged)
2. Poll for PID exit, up to 10s with exponential backoff
3. If PID exited: success, files are already cleaned up by daemon
4. If PID still alive after 10s: return error "Daemon did not stop within 10s (PID {pid}). Use `kill {pid}` to force." Do NOT remove files under a live daemon.

### Shutdown Order (Daemon)

Updated `run_daemon()` shutdown sequence:

```
1. write "stopping"              <-- NEW: first thing
2. drain_sessions (up to 5s)
3. reaper_handle.abort()
4. embedding_service.shutdown()
5. listener.cleanup()            <-- remove IPC socket
6. remove_file(port)             <-- remove port file
7. pid_file.cleanup()            <-- remove PID file
8. remove_file(state)            <-- remove state file (last)
```

The `stopping` write at step 1 ensures any adapter arriving during the slow cleanup (steps 2-6) sees the correct state and waits for exit rather than trying to connect.

## Files Changed

| File | Changes |
|------|---------|
| `src/paths.rs` | Add `daemon_state()` method to `DaemonPaths` |
| `src/daemon/mod.rs` | Write state file at phase transitions in `run_daemon()`, write `stopping` at shutdown entry |
| `src/adapter/launcher.rs` | Replace `is_daemon_running()` with `daemon_readiness()`, rewrite `ensure_daemon_running()` -> `ensure_daemon_ready()`, add `poll_for_ready()`, add `wait_for_pid_exit()` |
| `src/adapter/mod.rs` | Update `run_adapter()` to call `ensure_daemon_ready()` |
| `src/daemon/lifecycle.rs` | Fix `stop_daemon()` to wait for confirmed exit before claiming success |
| `src/tests/adapter/launcher.rs` | Update/add tests for new readiness logic |
| `src/tests/daemon/` | Add tests for state file write/cleanup at each phase |

## Cross-Platform Notes

### macOS/Linux
- IPC: Unix domain sockets. Socket file removed at step 5.
- State file: `~/.julie/daemon.state`, regular file.
- Advisory locking via `flock()` (fs2 crate).

### Windows
- IPC: Named pipes. Kernel-managed, no file cleanup needed.
- State file: `~/.julie/daemon.state`, regular file. `std::fs::write`/`read_to_string` open-close immediately, no sharing conflicts.
- Advisory locking via `LockFileEx` (fs2 crate).
- Binary lock: unchanged. `cargo build --release` still fails against a running exe. The state file helps the adapter cope with restarts, not the build.

## Testing Strategy

1. **Unit tests for `daemon_readiness()`:** Create state/PID files in temp dirs, verify all state combinations return correct readiness.
2. **Unit tests for `poll_for_ready()`:** Spawn a thread that writes `starting` then `ready` after a delay, verify poll returns.
3. **Unit tests for `wait_for_pid_exit()`:** Write a PID file with current PID, verify `Stopping` -> delete PID -> returns.
4. **State file lifecycle test:** Verify state file contents at each phase of `run_daemon()` (mock or integration).
5. **`stop_daemon()` test:** Verify it no longer deletes files under a live process.
6. **Existing adapter tests:** Update `test_wait_for_socket_*` tests to use new readiness API.

## Not In Scope

- PID-reuse detection (pid + start_time): Theoretical concern, not worth the complexity.
- IPC health handshake (PING/PONG): Nice defense-in-depth but unnecessary with state file.
- systemd/launchd integration: Future work if we ever want OS-level process management.
