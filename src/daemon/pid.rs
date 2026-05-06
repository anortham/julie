//! PID file lifecycle management for the Julie daemon.
//!
//! Format: `"<pid> <creation_time_unix_micros> <binary_mtime_unix_micros>\n"`
//!
//! The creation_time field closes the PID-reuse gap: on Windows, PIDs recycle
//! aggressively. A stale PID file pointing at a recycled PID (Chrome, Slack, …)
//! would make the adapter think the daemon is alive and wait 60 s to time out.
//! A mismatch between the stored creation_time and the running process's actual
//! creation time signals that the PID was recycled, and the file is treated as stale.
//!
//! Legacy single-integer PID files are rejected as stale (parser returns `None`).

use anyhow::Context;
use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

// ── PidFileContents ───────────────────────────────────────────────────────────

/// Parsed contents of a three-field PID file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PidFileContents {
    pub pid: u32,
    /// Microseconds since UNIX epoch when the process was created.
    /// 0 = unknown (platform does not support creation-time lookup).
    pub creation_time_micros: u64,
    /// Binary mtime in microseconds since UNIX epoch. 0 = unavailable.
    pub binary_mtime_micros: u64,
}

impl PidFileContents {
    /// Parse `"<pid> <creation> <binary_mtime>\n"`.
    /// Returns `None` for legacy single-integer files or any parse failure.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.trim().split_whitespace().collect();
        if parts.len() != 3 {
            return None; // legacy or corrupt — treat as stale
        }
        Some(Self {
            pid: parts[0].parse().ok()?,
            creation_time_micros: parts[1].parse().ok()?,
            binary_mtime_micros: parts[2].parse().ok()?,
        })
    }
}

impl fmt::Display for PidFileContents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.pid, self.creation_time_micros, self.binary_mtime_micros)
    }
}

// ── Process creation-time helpers ────────────────────────────────────────────

/// Return the creation time of process `pid` as microseconds since UNIX epoch.
/// Returns `None` when unsupported or when the process doesn't exist.
///
/// Linux: `/proc/<pid>/stat` field 21 (starttime ticks since boot).
/// macOS: `sysctl(KERN_PROC_PID)` — raw kinfo_proc buffer, p_starttime at offset 0.
/// Windows: `OpenProcess` + `GetProcessTimes`.
fn process_creation_time_micros(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    { linux_process_creation_time(pid) }

    #[cfg(target_os = "macos")]
    { macos_process_creation_time(pid) }

    #[cfg(windows)]
    { windows_process_creation_time(pid) }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    { let _ = pid; None }
}

fn current_process_creation_time_micros() -> u64 {
    process_creation_time_micros(std::process::id()).unwrap_or(0)
}

// ── Linux ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn linux_process_creation_time(pid: u32) -> Option<u64> {
    use std::time::SystemTime;

    let stat = fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
    // Command name (field 1) is wrapped in parens and may contain spaces.
    // Find the last ')' to safely skip past it.
    let after_paren = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = after_paren.split_whitespace().collect();
    // After the closing paren, field indices restart at 2; starttime is field 21 → offset 19.
    let ticks: u64 = fields.get(19)?.parse().ok()?;
    let ticks_per_sec = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as u64;
    if ticks_per_sec == 0 { return None; }

    let uptime_str = fs::read_to_string("/proc/uptime").ok()?;
    let uptime_secs: f64 = uptime_str.split_whitespace().next()?.parse().ok()?;
    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs_f64();
    let boot_secs = now_secs - uptime_secs;
    let start_micros = ((boot_secs + ticks as f64 / ticks_per_sec as f64) * 1_000_000.0) as u64;
    Some(start_micros)
}

// ── macOS ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn macos_process_creation_time(pid: u32) -> Option<u64> {
    // sysctl(KERN_PROC / KERN_PROC_PID) returns a kinfo_proc struct (648 bytes).
    // p_starttime (a timeval) is at offset 0: tv_sec=i64 at bytes 0-7, tv_usec=i32 at bytes 8-11.
    // Verified: sizeof(kinfo_proc)=648, offset(p_starttime)=0 on macOS arm64/x86_64.
    const KINFO_PROC_SIZE: usize = 648;
    let mut mib = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, pid as libc::c_int];
    let mut buf = [0u8; KINFO_PROC_SIZE];
    let mut size: libc::size_t = KINFO_PROC_SIZE;

    let ret = unsafe {
        libc::sysctl(mib.as_mut_ptr(), 4, buf.as_mut_ptr() as *mut libc::c_void, &mut size, std::ptr::null_mut(), 0)
    };
    if ret != 0 || size < 12 { return None; }

    let tv_sec = i64::from_ne_bytes(buf[0..8].try_into().ok()?);
    let tv_usec = i32::from_ne_bytes(buf[8..12].try_into().ok()?);
    if tv_sec <= 0 { return None; }
    let micros = tv_sec as u64 * 1_000_000 + tv_usec as u64;
    if micros == 0 { None } else { Some(micros) }
}

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(windows)]
fn windows_process_creation_time(pid: u32) -> Option<u64> {
    unsafe extern "system" {
        fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> isize;
        fn CloseHandle(handle: isize) -> i32;
        fn GetProcessTimes(h: isize, ct: *mut u64, et: *mut u64, kt: *mut u64, ut: *mut u64) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle == 0 { return None; }
    let (mut ct, mut et, mut kt, mut ut) = (0u64, 0u64, 0u64, 0u64);
    let ok = unsafe { GetProcessTimes(handle, &mut ct, &mut et, &mut kt, &mut ut) };
    unsafe { CloseHandle(handle) };
    if ok == 0 { return None; }
    // FILETIME: 100-ns intervals since 1601-01-01. Convert to µs since UNIX epoch.
    const EPOCH_DIFF: u64 = 116_444_736_000_000_000; // 100-ns intervals
    if ct < EPOCH_DIFF { return None; }
    Some((ct - EPOCH_DIFF) / 10)
}

// ── Binary mtime ──────────────────────────────────────────────────────────────

/// Current binary mtime as µs since UNIX epoch; 0 when unavailable.
fn current_binary_mtime_micros() -> u64 {
    std::env::current_exe().ok()
        .and_then(|p| fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

// ── Backoff ───────────────────────────────────────────────────────────────────

/// Exponential backoff for retry `n`: `50ms * 2^min(n, 7)`, capped at 5000 ms.
fn backoff_delay(n: u32) -> Duration {
    Duration::from_millis(50_u64.saturating_mul(1_u64 << n.min(7)).min(5000))
}

// ── PidFile ───────────────────────────────────────────────────────────────────

/// Handle to a PID file. Holds the path so cleanup can remove it on shutdown.
#[derive(Debug)]
pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// Atomically create the PID file (write-tmp, rename). Used by tests only.
    /// Prefer `create_exclusive` for production code.
    #[allow(dead_code)]
    pub fn create(path: &Path) -> anyhow::Result<Self> {
        let contents = PidFileContents {
            pid: std::process::id(),
            creation_time_micros: current_process_creation_time_micros(),
            binary_mtime_micros: current_binary_mtime_micros(),
        };
        let tmp = path.with_extension("pid.tmp");
        fs::write(&tmp, format!("{}\n", contents))
            .with_context(|| format!("Failed to write temp PID file: {}", tmp.display()))?;
        fs::rename(&tmp, path).with_context(|| {
            let _ = fs::remove_file(&tmp);
            format!("Failed to rename PID file to: {}", path.display())
        })?;
        Ok(Self { path: path.to_path_buf() })
    }

    /// Read and parse a PID file. Returns `None` for missing, malformed, or
    /// legacy single-integer files (all treated as stale).
    pub fn read_contents(path: &Path) -> Option<PidFileContents> {
        let s = fs::read_to_string(path).ok()?;
        PidFileContents::parse(&s)
    }

    /// Read only the PID. Legacy single-integer files return `None`.
    pub fn read_pid(path: &Path) -> Option<u32> {
        Self::read_contents(path).map(|c| c.pid)
    }

    /// Return `true` if a process with `pid` is currently alive.
    ///
    /// Unix: `kill(pid, 0)`. Windows: `OpenProcess`.
    pub fn is_process_alive(pid: u32) -> bool {
        #[cfg(unix)]
        {
            let ret = unsafe { libc::kill(pid as i32, 0) };
            if ret == 0 { return true; }
            // EPERM: process exists but we lack permission — still alive.
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            errno == libc::EPERM
        }

        #[cfg(windows)]
        {
            unsafe extern "system" {
                fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> isize;
                fn CloseHandle(handle: isize) -> i32;
            }
            const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
            let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            if h != 0 { unsafe { CloseHandle(h) }; return true; }
            // ERROR_ACCESS_DENIED (5): process exists but we lack permission.
            std::io::Error::last_os_error().raw_os_error() == Some(5)
        }
    }

    /// Check if a daemon is running: verify the process is alive AND its
    /// creation_time matches the stored value (PID-reuse defense).
    ///
    /// Removes the file and returns `None` when: file is missing, unreadable,
    /// in legacy format, process is dead, or creation_time mismatches.
    pub fn check_running(path: &Path) -> Option<u32> {
        let stored = match Self::read_contents(path) {
            Some(s) => s,
            None => {
                if path.exists() { let _ = fs::remove_file(path); }
                return None;
            }
        };

        if !Self::is_process_alive(stored.pid) {
            let _ = fs::remove_file(path);
            return None;
        }

        // PID-reuse defense: if both stored and actual creation_time are non-zero
        // and differ, the PID was recycled — reject the file.
        if stored.creation_time_micros != 0 {
            let actual = process_creation_time_micros(stored.pid).unwrap_or(0);
            if actual != 0 && actual != stored.creation_time_micros {
                let _ = fs::remove_file(path);
                return None;
            }
            // actual == 0: platform can't query creation time — skip check (no false positives).
        }

        Some(stored.pid)
    }

    /// Atomically create the PID file using `O_CREAT|O_EXCL`, eliminating the
    /// TOCTOU window in the `check_running` + `create` sequence.
    ///
    /// Stale files (dead process or PID-reuse detected) are removed and the
    /// create is retried with exponential backoff (`50ms * 2^n`, cap 5000 ms).
    /// Non-`NotFound` errors from `remove_file` are propagated (they indicate
    /// real problems such as Windows `ERROR_SHARING_VIOLATION`).
    pub fn create_exclusive(path: &Path) -> anyhow::Result<Self> {
        let contents_str = format!("{}\n", PidFileContents {
            pid: std::process::id(),
            creation_time_micros: current_process_creation_time_micros(),
            binary_mtime_micros: current_binary_mtime_micros(),
        });

        const MAX_RETRIES: u32 = 10;
        let mut retries = 0u32;

        loop {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut f) => {
                    f.write_all(contents_str.as_bytes())
                        .with_context(|| format!("Failed to write PID to {}", path.display()))?;
                    return Ok(Self { path: path.to_path_buf() });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if let Some(stored) = Self::read_contents(path) {
                        if Self::is_process_alive(stored.pid) {
                            let is_real = if stored.creation_time_micros != 0 {
                                let actual = process_creation_time_micros(stored.pid).unwrap_or(0);
                                actual == 0 || actual == stored.creation_time_micros
                            } else {
                                true // no creation_time — give benefit of doubt
                            };
                            if is_real {
                                anyhow::bail!("Daemon already running (PID {})", stored.pid);
                            }
                            // creation_time mismatch → PID recycled, treat as stale
                        }
                    }

                    // Stale/unreadable/PID-reused — remove and retry.
                    // Propagate non-NotFound errors (e.g. Windows sharing violation).
                    if let Err(re) = fs::remove_file(path) {
                        if re.kind() != std::io::ErrorKind::NotFound {
                            return Err(anyhow::Error::from(re).context(
                                format!("Failed to remove stale PID file: {}", path.display())
                            ));
                        }
                        // NotFound: a racing process already cleaned it up — retry.
                    }

                    retries += 1;
                    if retries >= MAX_RETRIES {
                        anyhow::bail!(
                            "Failed to create PID file at {} after {} retries",
                            path.display(), MAX_RETRIES
                        );
                    }
                    // Backoff AFTER first failure, not before first attempt.
                    std::thread::sleep(backoff_delay(retries - 1));
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
