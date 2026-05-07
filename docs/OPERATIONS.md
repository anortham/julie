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
