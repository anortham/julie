# Daemon Lifecycle Robustness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate daemon startup/restart race conditions by adding an explicit state file so the adapter knows whether the daemon is ready, starting, or stopping.

**Architecture:** Add `~/.julie/daemon.state` (plain text: `starting`/`ready`/`stopping`). The daemon writes state at each phase transition. The adapter reads state + PID to make informed decisions. The adapter holds `daemon.lock` through readiness confirmation. Also fixes `stop_daemon()` deleting files under a live process.

**Tech Stack:** Rust, std::fs, fs2 (file locking), tempfile (tests)

**Spec:** `docs/superpowers/specs/2026-04-08-daemon-lifecycle-design.md`

---

### Task 1: Add `daemon_state()` path to `DaemonPaths`

**Files:**
- Modify: `src/paths.rs:146` (after `daemon_port()`)
- Test: `src/tests/adapter/launcher.rs` (existing path test)

- [ ] **Step 1: Write the failing test**

Add to `src/tests/adapter/launcher.rs` inside `mod tests`:

```rust
#[test]
fn test_daemon_paths_includes_state_file() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    let state_path = paths.daemon_state();
    assert_eq!(state_path, dir.path().join("daemon.state"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_daemon_paths_includes_state_file 2>&1 | tail -10`
Expected: Compilation error, `daemon_state` method not found.

- [ ] **Step 3: Implement `daemon_state()` method**

Add to `src/paths.rs` after the `daemon_port()` method (line ~148):

```rust
/// Daemon lifecycle state file (starting/ready/stopping)
pub fn daemon_state(&self) -> PathBuf {
    self.julie_home.join("daemon.state")
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_daemon_paths_includes_state_file 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/paths.rs src/tests/adapter/launcher.rs
git commit -m "feat(daemon): add daemon_state() path to DaemonPaths"
```

---

### Task 2: Add `DaemonReadiness` enum and `daemon_readiness()` to launcher

**Files:**
- Modify: `src/adapter/launcher.rs:17-38` (add enum, replace `is_daemon_running`)
- Test: `src/tests/adapter/launcher.rs`

- [ ] **Step 1: Write failing tests for all readiness states**

Add to `src/tests/adapter/launcher.rs` inside `mod tests`:

```rust
use crate::adapter::launcher::DaemonReadiness;

#[test]
fn test_readiness_dead_when_no_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    let launcher = DaemonLauncher::new(paths);
    assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
}

#[test]
fn test_readiness_dead_cleans_stale_state_file() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    // State file exists but no PID = stale
    fs::write(paths.daemon_state(), "ready").unwrap();
    let launcher = DaemonLauncher::new(paths.clone());
    assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
    assert!(!paths.daemon_state().exists(), "stale state file should be cleaned up");
}

#[test]
fn test_readiness_ready_with_live_pid_and_ready_state() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();
    let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
    fs::write(paths.daemon_state(), "ready").unwrap();
    let launcher = DaemonLauncher::new(paths);
    assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Ready);
}

#[test]
fn test_readiness_starting_with_live_pid_and_starting_state() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();
    let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
    fs::write(paths.daemon_state(), "starting").unwrap();
    let launcher = DaemonLauncher::new(paths);
    assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Starting);
}

#[test]
fn test_readiness_starting_with_live_pid_and_no_state_file() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();
    let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
    // No state file at all
    let launcher = DaemonLauncher::new(paths);
    assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Starting);
}

#[test]
fn test_readiness_stopping_with_live_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();
    let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
    fs::write(paths.daemon_state(), "stopping").unwrap();
    let launcher = DaemonLauncher::new(paths);
    assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Stopping);
}

#[test]
fn test_readiness_dead_with_stale_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();
    // Bogus PID that's not alive
    fs::write(paths.daemon_pid(), "99999999").unwrap();
    fs::write(paths.daemon_state(), "ready").unwrap();
    let launcher = DaemonLauncher::new(paths.clone());
    assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
    // Both stale files should be cleaned
    assert!(!paths.daemon_state().exists());
    assert!(!paths.daemon_pid().exists());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_readiness_ 2>&1 | tail -10`
Expected: Compilation error, `DaemonReadiness` not found.

- [ ] **Step 3: Implement `DaemonReadiness` enum and `daemon_readiness()` method**

In `src/adapter/launcher.rs`, add the enum before the `DaemonLauncher` struct (around line 16):

```rust
/// The daemon's current lifecycle phase, as seen by the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonReadiness {
    /// PID alive, state file says "ready". Safe to connect.
    Ready,
    /// PID alive, state file says "starting" or is missing/unreadable.
    /// Daemon is initializing; wait, don't spawn a second one.
    Starting,
    /// PID alive, state file says "stopping".
    /// Daemon is shutting down; wait for exit, then spawn fresh.
    Stopping,
    /// No PID file, or PID is dead. Safe to spawn a new daemon.
    Dead,
}
```

Add method to `impl DaemonLauncher` (after `is_daemon_running`):

```rust
/// Assess the daemon's lifecycle phase from PID + state file.
///
/// Cleans up stale files as a side effect when the daemon is dead.
pub fn daemon_readiness(&self) -> DaemonReadiness {
    match PidFile::check_running(&self.paths.daemon_pid()) {
        None => {
            // No live daemon. Clean up stale state file if present.
            let _ = std::fs::remove_file(self.paths.daemon_state());
            DaemonReadiness::Dead
        }
        Some(_pid) => {
            match std::fs::read_to_string(self.paths.daemon_state()) {
                Ok(s) if s.trim() == "ready" => DaemonReadiness::Ready,
                Ok(s) if s.trim() == "stopping" => DaemonReadiness::Stopping,
                _ => DaemonReadiness::Starting,
            }
        }
    }
}
```

Keep `is_daemon_running()` for now (it's used by existing callers); we'll remove it in a later task.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_readiness_ 2>&1 | tail -10`
Expected: All 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/adapter/launcher.rs src/tests/adapter/launcher.rs
git commit -m "feat(adapter): add DaemonReadiness enum and daemon_readiness() method"
```

---

### Task 3: Write state file in `run_daemon()` at phase transitions

**Files:**
- Modify: `src/daemon/mod.rs:267-542` (`run_daemon` function)
- Test: `src/tests/daemon/` (new state file tests)

- [ ] **Step 1: Write failing test for state file write helper**

Create a test in a new section of an appropriate daemon test file (or add to an existing one in `src/tests/daemon/`). We'll test a helper function `write_daemon_state` and `cleanup_daemon_state`:

```rust
// In src/tests/daemon/ (e.g., src/tests/daemon/lifecycle.rs or a new state.rs)
#[cfg(test)]
mod tests {
    use crate::daemon::write_daemon_state;
    use tempfile::TempDir;
    use std::path::PathBuf;

    #[test]
    fn test_write_daemon_state_creates_file() {
        let dir = TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_state(&state_path, "starting");
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), "starting");
    }

    #[test]
    fn test_write_daemon_state_overwrites() {
        let dir = TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_state(&state_path, "starting");
        write_daemon_state(&state_path, "ready");
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), "ready");
    }

    #[test]
    fn test_write_daemon_state_stopping() {
        let dir = TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_state(&state_path, "stopping");
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), "stopping");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_write_daemon_state 2>&1 | tail -10`
Expected: Compilation error, `write_daemon_state` not found.

- [ ] **Step 3: Implement `write_daemon_state` helper**

Add to `src/daemon/mod.rs` (near the top, after the existing helper functions like `binary_mtime`, around line 110):

```rust
/// Write the daemon lifecycle state to the state file.
/// Best-effort: logs a warning if the write fails but does not propagate the error.
/// The state file is advisory; failure to write should not crash the daemon.
pub(crate) fn write_daemon_state(path: &std::path::Path, state: &str) {
    if let Err(e) = std::fs::write(path, state) {
        warn!("Failed to write daemon state '{}': {}", state, e);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_write_daemon_state 2>&1 | tail -10`
Expected: All 3 tests PASS.

- [ ] **Step 5: Add state writes to `run_daemon()`**

Insert state transitions at the right points in `src/daemon/mod.rs`:

**After PID creation (line ~277, after `info!(pid = ...)`)**:
```rust
    write_daemon_state(&paths.daemon_state(), "starting");
```

**After IPC listener bind (line ~480, after the "Daemon listening" log)**:
```rust
    write_daemon_state(&paths.daemon_state(), "ready");
```

**At the start of shutdown (line ~502, before the drain_sessions block)**:
```rust
    // Signal to adapters that we are shutting down before any cleanup begins.
    write_daemon_state(&paths.daemon_state(), "stopping");
```

**After PID cleanup (line ~538, after the PID cleanup block, before "Daemon stopped" log)**:
```rust
    let _ = std::fs::remove_file(paths.daemon_state());
```

- [ ] **Step 6: Commit**

```bash
git add src/daemon/mod.rs src/tests/daemon/
git commit -m "feat(daemon): write state file at lifecycle phase transitions"
```

---

### Task 4: Rewrite `ensure_daemon_running()` to `ensure_daemon_ready()`

**Files:**
- Modify: `src/adapter/launcher.rs:49-98` (rewrite the method)
- Modify: `src/adapter/mod.rs:48` (update caller)
- Test: `src/tests/adapter/launcher.rs`

- [ ] **Step 1: Write failing tests for the new readiness-based logic**

Add to `src/tests/adapter/launcher.rs` inside `mod tests`:

```rust
#[test]
fn test_poll_for_ready_returns_when_ready() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();

    // Pre-write "ready" state and a live PID
    let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
    fs::write(paths.daemon_state(), "ready").unwrap();

    let launcher = DaemonLauncher::new(paths);
    // ensure_daemon_ready should return immediately (fast path)
    let result = launcher.ensure_daemon_ready();
    assert!(result.is_ok());
}

#[test]
fn test_poll_for_ready_waits_for_starting_to_ready() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();

    let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
    let state_path = paths.daemon_state();
    fs::write(&state_path, "starting").unwrap();

    // Spawn a thread that transitions to "ready" after 200ms
    let state_path_clone = state_path.clone();
    let handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        fs::write(&state_path_clone, "ready").unwrap();
    });

    let launcher = DaemonLauncher::new(paths);
    let result = launcher.ensure_daemon_ready();
    handle.join().unwrap();
    assert!(result.is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_poll_for_ready 2>&1 | tail -10`
Expected: Compilation error, `ensure_daemon_ready` method not found.

- [ ] **Step 3: Implement `ensure_daemon_ready()`**

Replace `ensure_daemon_running()` in `src/adapter/launcher.rs` with:

```rust
/// Ensure the daemon is running and ready to accept connections.
///
/// State-file aware: instead of just checking PID liveness, reads the
/// daemon.state file to distinguish starting/ready/stopping. Holds
/// daemon.lock through the entire readiness check to prevent multi-adapter
/// races.
pub fn ensure_daemon_ready(&self) -> io::Result<()> {
    // Fast path (no lock): if daemon is already ready, skip the lock.
    // If the daemon transitions to stopping between this check and
    // connect_and_handshake, run_adapter's retry loop catches it.
    if matches!(self.daemon_readiness(), DaemonReadiness::Ready) {
        debug!("Daemon already ready (fast path)");
        return Ok(());
    }

    // Ensure the julie home directory exists for the lock file
    self.paths.ensure_dirs().map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to create daemon directories: {}", e),
        )
    })?;

    // Acquire advisory lock to serialize daemon startup across adapters
    let lock_path = self.paths.daemon_lock();
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;

    debug!("Acquiring daemon startup lock: {}", lock_path.display());
    lock_file.lock_exclusive()?;

    let deadline = Instant::now() + Duration::from_secs(60);

    let result = self.wait_for_daemon_ready(deadline);

    // Release lock
    lock_file.unlock()?;
    drop(lock_file);

    result
}

/// Internal: poll until the daemon reaches Ready state or deadline expires.
/// Handles Starting (wait), Stopping (wait for exit, then spawn), and
/// Dead (spawn) states.
fn wait_for_daemon_ready(&self, deadline: Instant) -> io::Result<()> {
    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Timed out waiting for daemon readiness",
            ));
        }

        match self.daemon_readiness() {
            DaemonReadiness::Ready => {
                debug!("Daemon is ready");
                return Ok(());
            }
            DaemonReadiness::Starting => {
                debug!("Daemon is starting, waiting for ready...");
                self.poll_for_state_change("ready", deadline)?;
            }
            DaemonReadiness::Stopping => {
                info!("Daemon is stopping, waiting for exit...");
                self.wait_for_pid_exit(deadline)?;
                // PID gone; fall through to Dead on next iteration
            }
            DaemonReadiness::Dead => {
                info!("Daemon not running, spawning...");
                self.spawn_daemon()?;
                // Wait for the daemon to write "ready" to state file
                self.poll_for_state_change("ready", deadline)?;
            }
        }
    }
}

/// Poll the state file until it contains `target_state`, the daemon dies,
/// or the state becomes "stopping" (when waiting for "ready").
///
/// Returns Ok(()) when the target state is reached.
/// Returns Err if the deadline expires or the daemon dies unexpectedly.
fn poll_for_state_change(&self, target_state: &str, deadline: Instant) -> io::Result<()> {
    let mut delay = Duration::from_millis(50);
    let max_delay = Duration::from_millis(500);

    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "Timed out waiting for daemon state '{}'",
                    target_state
                ),
            ));
        }

        std::thread::sleep(delay);
        delay = (delay * 2).min(max_delay);

        // Check if daemon died while we were waiting
        if !self.is_daemon_running() {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Daemon exited while waiting for readiness",
            ));
        }

        // Check current state
        if let Ok(s) = std::fs::read_to_string(self.paths.daemon_state()) {
            let state = s.trim();
            if state == target_state {
                return Ok(());
            }
            // If we're waiting for "ready" but the daemon transitioned to
            // "stopping", return an error so the caller can re-enter the
            // state machine (will see Stopping on next readiness check).
            if target_state == "ready" && state == "stopping" {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Daemon transitioned to stopping before reaching ready",
                ));
            }
        }
    }
}

/// Poll until the daemon's PID file is gone (process exited).
fn wait_for_pid_exit(&self, deadline: Instant) -> io::Result<()> {
    let mut delay = Duration::from_millis(50);
    let max_delay = Duration::from_millis(500);

    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Timed out waiting for daemon to exit",
            ));
        }

        if !self.is_daemon_running() {
            // Clean up stale state file
            let _ = std::fs::remove_file(self.paths.daemon_state());
            return Ok(());
        }

        std::thread::sleep(delay);
        delay = (delay * 2).min(max_delay);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_poll_for_ready 2>&1 | tail -10`
Expected: Both tests PASS.

- [ ] **Step 5: Update `run_adapter()` to call `ensure_daemon_ready()`**

In `src/adapter/mod.rs`, change line 48:

```rust
// Before:
tokio::task::block_in_place(|| launcher.ensure_daemon_running())
    .context("Failed to ensure daemon is running")?;

// After:
tokio::task::block_in_place(|| launcher.ensure_daemon_ready())
    .context("Failed to ensure daemon is ready")?;
```

- [ ] **Step 6: Run existing adapter tests to check for regressions**

Run: `cargo test --lib tests::adapter 2>&1 | tail -20`
Expected: All existing tests pass. (Some tests still reference `is_daemon_running` which we kept.)

- [ ] **Step 7: Commit**

```bash
git add src/adapter/launcher.rs src/adapter/mod.rs src/tests/adapter/launcher.rs
git commit -m "feat(adapter): rewrite ensure_daemon_running to state-aware ensure_daemon_ready"
```

---

### Task 5: Fix `stop_daemon()` to not delete files under a live process

**Files:**
- Modify: `src/daemon/lifecycle.rs:31-87`
- Test: `src/tests/daemon/` (add lifecycle test)

- [ ] **Step 1: Write failing test**

Add a test file `src/tests/daemon/lifecycle.rs` (or add to existing test module):

```rust
#[cfg(test)]
mod tests {
    use crate::daemon::lifecycle::{stop_daemon, check_status, DaemonStatus};
    use crate::daemon::pid::PidFile;
    use crate::paths::DaemonPaths;
    use std::fs;

    #[test]
    fn test_stop_daemon_does_not_delete_files_while_process_alive() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        // Write current process PID (definitely alive; stop_daemon can't kill us
        // because SIGTERM to self is handled, but the key check is file cleanup)
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        fs::write(paths.daemon_state(), "ready").unwrap();

        // stop_daemon will try to signal, the process won't die (it's us),
        // so after the timeout it should NOT delete the files.
        let result = stop_daemon(&paths);

        // The function should return an error since the process didn't exit
        assert!(result.is_err(), "Should error when daemon doesn't exit");

        // Files should still exist (not deleted under a live process)
        assert!(
            paths.daemon_pid().exists(),
            "PID file should not be deleted while process is alive"
        );
    }

    #[test]
    fn test_stop_daemon_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let result = stop_daemon(&paths);
        assert!(result.is_ok());
    }

    #[test]
    fn test_stop_daemon_cleans_stale_pid() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        // Write a bogus PID
        fs::write(paths.daemon_pid(), "99999999").unwrap();
        fs::write(paths.daemon_state(), "ready").unwrap();

        let result = stop_daemon(&paths);
        assert!(result.is_ok());

        // Stale files should be cleaned
        assert!(!paths.daemon_pid().exists());
        assert!(!paths.daemon_state().exists());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_stop_daemon 2>&1 | tail -15`
Expected: `test_stop_daemon_does_not_delete_files_while_process_alive` fails (current code unconditionally deletes).

- [ ] **Step 3: Rewrite `stop_daemon()`**

Replace the function body in `src/daemon/lifecycle.rs`:

```rust
pub fn stop_daemon(paths: &DaemonPaths) -> anyhow::Result<()> {
    match PidFile::check_running(&paths.daemon_pid()) {
        Some(pid) => {
            info!("Sending shutdown signal to daemon PID {}", pid);

            #[cfg(unix)]
            {
                let ret = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                if ret != 0 {
                    anyhow::bail!("Failed to send SIGTERM to PID {}", pid);
                }
            }

            #[cfg(windows)]
            {
                use super::shutdown_event;

                let event_name = paths.daemon_shutdown_event();
                let signaled = shutdown_event::signal_shutdown(&event_name).unwrap_or(false);
                if signaled {
                    info!("Signaled shutdown event: {}", event_name);
                } else {
                    info!("Shutdown event not found, falling back to taskkill /F");
                    let _ = std::process::Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .output();
                }
            }

            // Wait for the process to actually exit (up to 10s).
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                if !PidFile::is_process_alive(pid) {
                    // Process exited. Clean up any stale files the daemon
                    // didn't get to (e.g., if it crashed mid-shutdown).
                    let _ = std::fs::remove_file(paths.daemon_pid());
                    let _ = std::fs::remove_file(paths.daemon_state());
                    #[cfg(unix)]
                    let _ = std::fs::remove_file(paths.daemon_socket());
                    info!("Daemon stopped");
                    return Ok(());
                }
                if std::time::Instant::now() >= deadline {
                    // Process is still alive. Do NOT delete files under it.
                    anyhow::bail!(
                        "Daemon did not stop within 10s (PID {}). \
                         Use `kill {}` to force.",
                        pid,
                        pid
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        None => {
            // No live daemon. Clean up any stale files.
            let _ = std::fs::remove_file(paths.daemon_pid());
            let _ = std::fs::remove_file(paths.daemon_state());
            #[cfg(unix)]
            let _ = std::fs::remove_file(paths.daemon_socket());
            info!("Daemon is not running (cleaned stale files if any)");
            Ok(())
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_stop_daemon 2>&1 | tail -15`
Expected: All 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/daemon/lifecycle.rs src/tests/daemon/
git commit -m "fix(daemon): stop_daemon no longer deletes files under a live process"
```

---

### Task 6: Remove deprecated `is_daemon_running()` fast path, update all callers

**Files:**
- Modify: `src/adapter/launcher.rs` (remove old method, update `wait_for_socket` -> rename or deprecate)
- Modify: `src/tests/adapter/launcher.rs` (update tests that reference old API)

- [ ] **Step 1: Search for all callers of `is_daemon_running`**

Use Julie: `fast_refs("is_daemon_running")`. The known callers are:
- `ensure_daemon_ready()` (internal, via `daemon_readiness()` which calls `PidFile::check_running` directly)
- Old tests: `test_daemon_not_running_when_no_pid_file`, `test_daemon_detected_as_running_with_valid_pid`, `test_stale_pid_detected_and_cleaned`

- [ ] **Step 2: Update old tests to use `daemon_readiness()` instead**

In `src/tests/adapter/launcher.rs`, update the three legacy tests:

```rust
#[test]
fn test_daemon_not_running_when_no_pid_file() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    let launcher = DaemonLauncher::new(paths);
    assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
}

#[test]
fn test_daemon_detected_as_running_with_valid_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();
    let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
    // No state file = Starting (PID alive but state unknown)
    let launcher = DaemonLauncher::new(paths);
    assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Starting);
}

#[test]
fn test_stale_pid_detected_and_cleaned() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();
    fs::write(paths.daemon_pid(), "99999999\n").unwrap();
    let launcher = DaemonLauncher::new(paths.clone());
    assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
    assert!(!paths.daemon_pid().exists());
}
```

- [ ] **Step 3: Keep `is_daemon_running()` but make it a thin wrapper**

`is_daemon_running()` is still used internally by `wait_for_pid_exit` and `poll_for_state_change`. Keep it as a private helper:

```rust
/// Check if the daemon process is alive (PID file exists and process is running).
/// Used internally by polling loops. External callers should use `daemon_readiness()`.
fn is_daemon_running(&self) -> bool {
    PidFile::check_running(&self.paths.daemon_pid()).is_some()
}
```

Change visibility from `pub` to `fn` (private).

- [ ] **Step 4: Run all adapter tests**

Run: `cargo test --lib tests::adapter 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/adapter/launcher.rs src/tests/adapter/launcher.rs
git commit -m "refactor(adapter): replace public is_daemon_running with daemon_readiness"
```

---

### Task 7: Run full dev tier and verify

**Files:** None (verification only)

- [ ] **Step 1: Run `cargo xtask test dev`**

Run: `cargo xtask test dev`
Expected: All buckets pass with no new failures.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | tail -20`
Expected: No new warnings.

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt --check`
Expected: No formatting issues.

- [ ] **Step 4: If all green, final commit with any fixups**

If clippy or fmt found issues, fix and commit:
```bash
git add -A
git commit -m "chore: clippy and fmt fixups for daemon lifecycle"
```

---

### Task 8: Manual smoke test

- [ ] **Step 1: Build release**

Run: `cargo build --release`

- [ ] **Step 2: Verify the daemon state file lifecycle**

1. Start Claude Code (or any MCP client that spawns `julie-server`)
2. Check: `cat ~/.julie/daemon.state` should show `ready`
3. Check: `cat ~/.julie/daemon.pid` should show the daemon PID
4. Exit Claude Code
5. Rebuild: `cargo build --release`
6. Start Claude Code again
7. Julie should start successfully (the adapter should detect the stale binary, wait for the old daemon to exit, then spawn a fresh one)
8. Check logs: `tail -20 ~/.julie/daemon.log.$(date +%Y-%m-%d)` should show the state transitions

- [ ] **Step 3: Verify `julie stop` works cleanly**

Run: `julie-server stop`
Check: `cat ~/.julie/daemon.state` should be gone (or show error: file not found)
Check: `cat ~/.julie/daemon.pid` should be gone
