//! PID file lifecycle management for the Julie daemon.
//!
//! Provides atomic PID file creation, stale process detection,
//! and cleanup on graceful shutdown.

use anyhow::Context;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Handle to a PID file. Holds the path so cleanup can remove it on drop/shutdown.
#[derive(Debug)]
pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// Create a PID file at `path` containing the current process ID.
    ///
    /// The write is atomic: writes to a `.tmp` sibling first, then renames.
    /// Returns a `PidFile` handle for later cleanup.
    ///
    /// Prefer `create_exclusive` for production code — it uses O_CREAT|O_EXCL
    /// to eliminate the TOCTOU window. This method is only used by test helpers.
    #[allow(dead_code)]
    pub fn create(path: &Path) -> anyhow::Result<Self> {
        let pid = std::process::id();
        let tmp_path = path.with_extension("pid.tmp");

        // Write to temp file first
        fs::write(&tmp_path, pid.to_string())
            .with_context(|| format!("Failed to write temp PID file: {}", tmp_path.display()))?;

        // Atomic rename (on the same filesystem, rename is atomic on both Unix and Windows)
        fs::rename(&tmp_path, path).with_context(|| {
            // Clean up the tmp file on rename failure
            let _ = fs::remove_file(&tmp_path);
            format!("Failed to rename PID file to: {}", path.display())
        })?;

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Read a PID from the file at `path`.
    ///
    /// Returns `None` if the file is missing, empty, or contains non-numeric data.
    pub fn read_pid(path: &Path) -> Option<u32> {
        let contents = fs::read_to_string(path).ok()?;
        contents.trim().parse::<u32>().ok()
    }

    /// Check whether a process with the given PID is currently alive.
    ///
    /// Uses `kill(pid, 0)` on Unix (signal 0 checks existence without sending a signal).
    /// Returns `false` on Windows for now.
    pub fn is_process_alive(pid: u32) -> bool {
        #[cfg(unix)]
        {
            // kill(pid, 0) returns 0 if the process exists and we have permission to signal it.
            // It returns -1 with ESRCH if the process doesn't exist.
            // It returns -1 with EPERM if the process exists but we lack permission, which
            // still means "alive" for our purposes.
            let ret = unsafe { libc::kill(pid as i32, 0) };
            if ret == 0 {
                return true;
            }
            // EPERM means "exists but we can't signal it" (still alive)
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            errno == libc::EPERM
        }

        #[cfg(windows)]
        {
            // OpenProcess with PROCESS_QUERY_LIMITED_INFORMATION (0x1000) is the
            // Windows equivalent of kill(pid, 0). If the process exists and we
            // have access, the handle is valid. If it exists but we lack access,
            // we get ERROR_ACCESS_DENIED (still alive, just restricted).
            unsafe extern "system" {
                fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> isize;
                fn CloseHandle(handle: isize) -> i32;
            }

            const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
            let handle =
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            if handle != 0 {
                unsafe {
                    CloseHandle(handle);
                }
                return true;
            }
            // ERROR_ACCESS_DENIED (5) means the process exists but we lack permission
            let error = std::io::Error::last_os_error();
            error.raw_os_error() == Some(5)
        }
    }

    /// Check if a daemon is already running by reading the PID file and verifying the process.
    ///
    /// If the PID file exists but the process is dead, the stale file is removed.
    /// Returns `Some(pid)` if a live process owns the PID file, `None` otherwise.
    pub fn check_running(path: &Path) -> Option<u32> {
        let pid = Self::read_pid(path)?;

        if Self::is_process_alive(pid) {
            Some(pid)
        } else {
            // Stale PID file; clean it up
            let _ = fs::remove_file(path);
            None
        }
    }

    /// Atomically create the PID file, checking for an already-running daemon.
    ///
    /// Uses `O_CREAT|O_EXCL` semantics (`create_new(true)`) to collapse the
    /// read-check and write into a single atomic operation, eliminating the
    /// TOCTOU window present in the `check_running` + `create` sequence.
    ///
    /// Behavior:
    /// - No file exists → create it and return `Ok(PidFile)`.
    /// - File exists, process is alive → return `Err("already running …")`.
    /// - File exists, process is dead (stale) → remove it, then create.
    pub fn create_exclusive(path: &Path) -> anyhow::Result<Self> {
        let pid = std::process::id();
        const MAX_RETRIES: u32 = 10;
        let mut retries = 0u32;

        loop {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut f) => {
                    write!(f, "{}", pid)
                        .with_context(|| format!("Failed to write PID to {}", path.display()))?;
                    return Ok(Self {
                        path: path.to_path_buf(),
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if let Some(existing) = Self::read_pid(path) {
                        if Self::is_process_alive(existing) {
                            anyhow::bail!("Daemon already running (PID {})", existing);
                        }
                    }
                    // Stale or unreadable file — remove and retry the exclusive create
                    let _ = fs::remove_file(path);
                    retries += 1;
                    if retries >= MAX_RETRIES {
                        anyhow::bail!(
                            "Failed to create PID file at {} after {} retries",
                            path.display(),
                            MAX_RETRIES
                        );
                    }
                }
                Err(e) => {
                    return Err(anyhow::Error::from(e)
                        .context(format!("Failed to create PID file: {}", path.display())));
                }
            }
        }
    }

    /// Remove the PID file. Called during graceful shutdown.
    pub fn cleanup(self) -> anyhow::Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)
                .with_context(|| format!("Failed to remove PID file: {}", self.path.display()))?;
        }
        Ok(())
    }
}
