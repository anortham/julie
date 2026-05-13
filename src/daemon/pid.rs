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
        write!(
            f,
            "{} {} {}",
            self.pid, self.creation_time_micros, self.binary_mtime_micros
        )
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
    {
        linux_process_creation_time(pid)
    }

    #[cfg(target_os = "macos")]
    {
        macos_process_creation_time(pid)
    }

    #[cfg(windows)]
    {
        windows_process_creation_time(pid)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = pid;
        None
    }
}

fn current_process_creation_time_micros() -> u64 {
    process_creation_time_micros(std::process::id()).unwrap_or(0)
}

// ── Linux ─────────────────────────────────────────────────────────────────────

/// Read the kernel's per-boot identifier.
///
/// `/proc/sys/kernel/random/boot_id` is a UUID generated at boot and
/// stable for the lifetime of that boot session. Unlike `/proc/stat`
/// `btime`, it is NOT derived from the real-time clock and is therefore
/// immune to `settimeofday`, NTP large-step adjustments, and VM
/// suspend/resume time-warp.
///
/// Returns the file's content trimmed of trailing whitespace, or `None`
/// if the file is missing or unreadable (containers may sysctl-shadow
/// it).
#[cfg(target_os = "linux")]
fn linux_boot_id() -> Option<String> {
    let raw = fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Deterministic identity for a Linux process, computed from the kernel
/// boot UUID and the process's start-time in ticks since boot.
///
/// Public to the crate so tests can exercise the hash mixing without
/// touching `/proc`. The function is pure (no I/O) and stable: given the
/// same `boot_id` and `start_ticks`, it always returns the same `u64`.
///
/// Hash scheme: FNV-1a 64. We mix `boot_id` bytes (variable length) first,
/// then 8 little-endian bytes of `start_ticks`. Two distinct (boot_id,
/// ticks) inputs would have to collide on a 64-bit hash to be confused;
/// the practical collision rate (~2^-64) is far below any operational
/// concern.
///
/// Why hashing rather than packing: the on-disk PID file format stores a
/// single u64 `creation_time_micros` field. Changing the format would
/// break upgrade compatibility; folding the (boot_id, ticks) pair into a
/// hash preserves the format while still distinguishing across reboots
/// and across processes within a boot.
#[cfg(target_os = "linux")]
pub(crate) fn linux_process_identity_from_parts(boot_id: &str, start_ticks: u64) -> u64 {
    let mut hash: u64 = 14695981039346656037; // FNV-1a 64 offset basis
    const PRIME: u64 = 1099511628211;
    for byte in boot_id.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    // Separator so "abc" + ticks=N is not confusable with "abcN" + ticks=0.
    hash ^= 0xFF;
    hash = hash.wrapping_mul(PRIME);
    for byte in start_ticks.to_le_bytes().iter() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Process creation-time identity on Linux.
///
/// Returns a hash of `(boot_id, /proc/<pid>/stat starttime)`. Stored in
/// the PID file's `creation_time_micros` field (semantic abuse — it is no
/// longer a time-domain value, just an opaque per-boot per-process
/// identity). `check_status` compares stored vs. recomputed: a match
/// means the recorded process is still the same one running today.
///
/// Why not a real timestamp:
///   - Earlier versions used `SystemTime::now() − /proc/uptime + ticks/HZ`
///     in f64; non-atomic syscalls and float math caused µs-ms drift
///     between calls, and `check_status` unlinked live PID files on
///     mismatch (2026-05-12 "577-daemon cascade").
///   - Replacing the wall-clock portion with `/proc/stat btime` was
///     stable for HZ-bound drift but not for `settimeofday`:
///     `getboottime64` computes btime as `offs_real − offs_boot` and any
///     real-time clock step shifts it. (Codex 2026-05-12 review.)
///   - `boot_id` is generated by the kernel at boot and untouched
///     thereafter. It survives `settimeofday`, NTP steps, suspend/resume,
///     and VM live migration of the time domain.
#[cfg(target_os = "linux")]
fn linux_process_creation_time(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
    // Command name (field 1) is wrapped in parens and may contain spaces.
    // Find the last ')' to safely skip past it.
    let after_paren = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = after_paren.split_whitespace().collect();
    // After the closing paren, field indices restart at 2; starttime is field 21 → offset 19.
    let ticks: u64 = fields.get(19)?.parse().ok()?;

    let boot_id = linux_boot_id()?;
    Some(linux_process_identity_from_parts(&boot_id, ticks))
}

// ── macOS ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn macos_process_creation_time(pid: u32) -> Option<u64> {
    // sysctl(KERN_PROC / KERN_PROC_PID) returns a kinfo_proc struct (648 bytes).
    // p_starttime (a timeval) is at offset 0: tv_sec=i64 at bytes 0-7, tv_usec=i32 at bytes 8-11.
    // Verified: sizeof(kinfo_proc)=648, offset(p_starttime)=0 on macOS arm64/x86_64.
    const KINFO_PROC_SIZE: usize = 648;
    let mut mib = [
        libc::CTL_KERN,
        libc::KERN_PROC,
        libc::KERN_PROC_PID,
        pid as libc::c_int,
    ];
    let mut buf = [0u8; KINFO_PROC_SIZE];
    let mut size: libc::size_t = KINFO_PROC_SIZE;

    let ret = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            4,
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret != 0 || size < 12 {
        return None;
    }

    let tv_sec = i64::from_ne_bytes(buf[0..8].try_into().ok()?);
    let tv_usec = i32::from_ne_bytes(buf[8..12].try_into().ok()?);
    if tv_sec <= 0 {
        return None;
    }
    let micros = tv_sec as u64 * 1_000_000 + tv_usec as u64;
    if micros == 0 { None } else { Some(micros) }
}

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(windows)]
fn windows_process_creation_time(pid: u32) -> Option<u64> {
    unsafe extern "system" {
        fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> isize;
        fn CloseHandle(handle: isize) -> i32;
        fn GetProcessTimes(h: isize, ct: *mut u64, et: *mut u64, kt: *mut u64, ut: *mut u64)
        -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle == 0 {
        return None;
    }
    let (mut ct, mut et, mut kt, mut ut) = (0u64, 0u64, 0u64, 0u64);
    let ok = unsafe { GetProcessTimes(handle, &mut ct, &mut et, &mut kt, &mut ut) };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return None;
    }
    // FILETIME: 100-ns intervals since 1601-01-01. Convert to µs since UNIX epoch.
    const EPOCH_DIFF: u64 = 116_444_736_000_000_000; // 100-ns intervals
    if ct < EPOCH_DIFF {
        return None;
    }
    Some((ct - EPOCH_DIFF) / 10)
}

// ── Binary mtime ──────────────────────────────────────────────────────────────

/// Current binary mtime as µs since UNIX epoch; 0 when unavailable.
fn current_binary_mtime_micros() -> u64 {
    std::env::current_exe()
        .ok()
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

/// Three-way classification of an existing PID file's owner. Used by
/// `create_exclusive` to decide whether to bail (real daemon), retry (stale),
/// or refuse to overwrite (indeterminate).
enum OwnerState {
    /// A live process owns the file and validation succeeded (or no
    /// creation_time was stored, e.g. legacy single-integer file).
    Real(u32),
    /// The owner is dead, the PID was recycled, or the file is corrupt.
    /// Safe to remove and retry.
    Stale,
    /// The owner is alive but creation_time validation failed (e.g. Windows
    /// ACCESS_DENIED for a recycled PID owned by a privileged process). Do
    /// not assume this is the daemon, but do not remove the file either —
    /// a legitimate daemon may own it.
    Indeterminate,
}

/// How long an empty PID file may live before it is treated as a crash
/// leftover instead of a daemon mid-write. A fresh file under this age
/// is `Indeterminate`; older files are `Dead`.
///
/// 5 seconds is well above the millisecond-scale window between
/// `O_CREAT|O_EXCL` open and `write_all` in `create_exclusive`, but far
/// below human-scale "I left this running for a while".
const PID_FILE_FRESH_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);

/// How far in the future the mtime is allowed to be while still counting
/// as "fresh". Hosts with NTP step adjustments, VM time-warp, or
/// filesystem-vs-system clock skew can produce mtimes a few seconds
/// ahead of `SystemTime::now()`; that's not a corruption signal. Beyond
/// this tolerance, a future mtime is treated as stale data (corrupt fs
/// metadata or backup restore) so the launcher does not wait forever on
/// such files. Matches `PID_FILE_FRESH_WINDOW` symmetrically.
const PID_FILE_FUTURE_MTIME_SKEW: std::time::Duration = std::time::Duration::from_secs(5);

/// Three-state classification of a PID file on disk.
///
/// `check_running` collapses this to `Option<u32>` (Alive → Some, others → None)
/// for back-compatible callers. New callers that need to distinguish "daemon
/// mid-write" from "no daemon" should call `check_status` directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidFileStatus {
    /// A live process owns the file and validation succeeded.
    Alive(u32),
    /// No daemon: file missing, owner dead, or PID-reuse detected. Any
    /// on-disk file that produced this verdict has been removed as a side
    /// effect.
    Dead,
    /// File exists but cannot be authoritatively classified (e.g., empty
    /// mid-write, transient validation failure). The file is preserved;
    /// callers should wait and re-check rather than spawn a replacement
    /// daemon.
    Indeterminate,
}

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
        Ok(Self {
            path: path.to_path_buf(),
        })
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
            if ret == 0 {
                return true;
            }
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
            if h != 0 {
                unsafe { CloseHandle(h) };
                return true;
            }
            // ERROR_ACCESS_DENIED (5): process exists but we lack permission.
            std::io::Error::last_os_error().raw_os_error() == Some(5)
        }
    }

    /// Check if a daemon is running. Thin back-compat wrapper around
    /// `check_status`: returns `Some(pid)` for `Alive`, `None` for `Dead`
    /// and `Indeterminate`.
    ///
    /// New callers that need to distinguish "daemon mid-write" from "no
    /// daemon" — notably the adapter launcher — should call `check_status`.
    pub fn check_running(path: &Path) -> Option<u32> {
        match Self::check_status(path) {
            PidFileStatus::Alive(pid) => Some(pid),
            PidFileStatus::Dead | PidFileStatus::Indeterminate => None,
        }
    }

    /// Three-state classification of the PID file at `path`.
    ///
    /// - `Alive(pid)`: the file references a live process whose
    ///   creation_time matches the stored value, or a live legacy
    ///   single-integer daemon. The file is preserved.
    /// - `Dead`: the file's owner is provably gone (dead process,
    ///   PID-reuse mismatch, or stale unparseable content). The file is
    ///   removed before returning so subsequent acquirers can proceed.
    /// - `Indeterminate`: the file exists but cannot be authoritatively
    ///   classified — either content is empty (a racing daemon is
    ///   mid-write of `create_exclusive`) or creation_time validation
    ///   returned an indeterminate result (Windows ACCESS_DENIED on a
    ///   recycled privileged PID). The file is preserved and callers
    ///   should wait + retry.
    ///
    /// The empty-file freshness check uses `PID_FILE_FRESH_WINDOW` against
    /// the file's mtime. This is the P2 layer of the 577-daemon cascade
    /// fix: pre-fix, an empty file went straight to the legacy fallback
    /// and was unlinked, which let the adapter respawn into the brief
    /// window between `O_CREAT|O_EXCL` and `write_all`.
    pub fn check_status(path: &Path) -> PidFileStatus {
        let stored = match Self::read_contents(path) {
            Some(s) => s,
            None => {
                // Legacy single-integer format, empty file, or corrupt
                // content. Empty / unparseable files MAY represent a
                // racing daemon mid-write; the freshness window guards
                // against unlinking them.
                return Self::classify_legacy_or_unparseable(path);
            }
        };

        if !Self::is_process_alive(stored.pid) {
            let _ = fs::remove_file(path);
            return PidFileStatus::Dead;
        }

        // PID-reuse defense: if stored creation_time is non-zero, validate
        // against the actual creation_time.
        if stored.creation_time_micros != 0 {
            match process_creation_time_micros(stored.pid) {
                Some(actual) if actual == stored.creation_time_micros => {
                    // Match — fall through to Alive.
                }
                Some(_) => {
                    // Mismatch — PID recycled, file is stale.
                    let _ = fs::remove_file(path);
                    return PidFileStatus::Dead;
                }
                None => {
                    // Creation-time lookup failed (e.g., Windows
                    // ACCESS_DENIED on a recycled PID owned by a
                    // privileged process). Preserve the file so a
                    // follow-up call can retry validation.
                    return PidFileStatus::Indeterminate;
                }
            }
        }

        PidFileStatus::Alive(stored.pid)
    }

    /// Classify a PID file whose content is empty, legacy single-integer,
    /// or otherwise unparseable by `PidFileContents::parse`.
    ///
    /// - Live legacy PID → `Alive(pid)`, file preserved (upgrade path).
    /// - Dead legacy PID → `Dead`, file removed.
    /// - **Empty** content + fresh mtime → `Indeterminate`, file preserved
    ///   (racing daemon mid-write of `create_exclusive`).
    /// - Empty content + stale mtime → `Dead`, file removed (crash leftover).
    /// - **Nonempty unparseable** content → `Dead`, file removed
    ///   (crash artifact: a partial write that left something behind but
    ///   doesn't represent an in-flight daemon). Codex 2026-05-12 review
    ///   finding #3: any path that pinned nonempty-unparseable files as
    ///   Indeterminate could let a corrupt PID file hold the launcher in
    ///   `Starting` indefinitely.
    fn classify_legacy_or_unparseable(path: &Path) -> PidFileStatus {
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                if path.exists() {
                    let _ = fs::remove_file(path);
                }
                return PidFileStatus::Dead;
            }
        };

        // Try the legacy single-integer parse first.
        if let Some(pid) = raw.split_whitespace().next().and_then(|s| s.parse().ok()) {
            if Self::is_process_alive(pid) {
                // Legacy daemon still alive — preserve the file.
                return PidFileStatus::Alive(pid);
            }
            let _ = fs::remove_file(path);
            return PidFileStatus::Dead;
        }

        // Only EMPTY content qualifies for the racing-daemon-mid-write
        // path. Nonempty unparseable content is a crash artifact and
        // must not pin the launcher.
        if !raw.is_empty() {
            let _ = fs::remove_file(path);
            return PidFileStatus::Dead;
        }

        // Empty + fresh mtime → Indeterminate (racing daemon).
        // Empty + stale mtime → Dead (crashed before writing).
        if Self::is_pid_file_fresh(path) {
            PidFileStatus::Indeterminate
        } else {
            let _ = fs::remove_file(path);
            PidFileStatus::Dead
        }
    }

    /// Returns true if the file's mtime is within `PID_FILE_FRESH_WINDOW`
    /// of `now`. Future mtimes are accepted ONLY within a small skew
    /// tolerance (`PID_FILE_FUTURE_MTIME_SKEW`) — beyond that, the file
    /// is treated as stale. Codex 2026-05-12 review finding #3: blanket
    /// "future-mtime = fresh" let a corrupt PID file with a far-future
    /// timestamp pin the launcher forever.
    fn is_pid_file_fresh(path: &Path) -> bool {
        let Ok(metadata) = fs::metadata(path) else {
            return false;
        };
        let Ok(mtime) = metadata.modified() else {
            return false;
        };
        match std::time::SystemTime::now().duration_since(mtime) {
            Ok(age) => age <= PID_FILE_FRESH_WINDOW,
            // mtime is in the future. Accept small NTP-style skew but
            // reject far-future timestamps (corrupted fs metadata or
            // restore from a future-dated backup).
            Err(future_err) => future_err.duration() <= PID_FILE_FUTURE_MTIME_SKEW,
        }
    }

    /// Atomically create the PID file using `O_CREAT|O_EXCL`, eliminating the
    /// TOCTOU window in the `check_running` + `create` sequence.
    ///
    /// Stale files (dead process or PID-reuse detected) are removed and the
    /// create is retried with exponential backoff (`50ms * 2^n`, cap 5000 ms).
    /// Non-`NotFound` errors from `remove_file` are propagated (they indicate
    /// real problems such as Windows `ERROR_SHARING_VIOLATION`).
    pub fn create_exclusive(path: &Path) -> anyhow::Result<Self> {
        let contents_str = format!(
            "{}\n",
            PidFileContents {
                pid: std::process::id(),
                creation_time_micros: current_process_creation_time_micros(),
                binary_mtime_micros: current_binary_mtime_micros(),
            }
        );

        const MAX_RETRIES: u32 = 10;
        let mut retries = 0u32;

        loop {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut f) => {
                    f.write_all(contents_str.as_bytes())
                        .with_context(|| format!("Failed to write PID to {}", path.display()))?;
                    return Ok(Self {
                        path: path.to_path_buf(),
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Decide: real running daemon, stale, or indeterminate.
                    //   - Some(stored): 3-field format. Validate liveness +
                    //     creation_time match.
                    //   - None: legacy single-integer file. If the legacy PID
                    //     is alive, treat as a real running daemon (preserves
                    //     the upgrade-path single-daemon invariant).
                    let owner_state = match Self::read_contents(path) {
                        Some(stored) => {
                            if !Self::is_process_alive(stored.pid) {
                                OwnerState::Stale
                            } else if stored.creation_time_micros == 0 {
                                // No creation_time recorded — benefit of doubt
                                // for legacy files written by older daemons.
                                OwnerState::Real(stored.pid)
                            } else {
                                match process_creation_time_micros(stored.pid) {
                                    Some(actual) if actual == stored.creation_time_micros => {
                                        OwnerState::Real(stored.pid)
                                    }
                                    Some(_) => OwnerState::Stale, // PID recycled
                                    None => OwnerState::Indeterminate, // can't validate
                                }
                            }
                        }
                        None => {
                            // Legacy or corrupt format. Try to extract a PID
                            // from the first whitespace-delimited token.
                            let raw = fs::read_to_string(path).unwrap_or_default();
                            let legacy_pid: Option<u32> =
                                raw.split_whitespace().next().and_then(|s| s.parse().ok());
                            match legacy_pid {
                                Some(pid) if Self::is_process_alive(pid) => OwnerState::Real(pid),
                                _ => OwnerState::Stale,
                            }
                        }
                    };

                    match owner_state {
                        OwnerState::Real(pid) => {
                            anyhow::bail!("Daemon already running (PID {})", pid);
                        }
                        OwnerState::Indeterminate => {
                            // Cannot validate the existing PID. Don't remove
                            // the file — a legitimate daemon may own it. Bail
                            // so the adapter can retry validation on a fresh
                            // call.
                            anyhow::bail!(
                                "PID file at {} exists but creation_time validation is indeterminate; not overwriting",
                                path.display()
                            );
                        }
                        OwnerState::Stale => {
                            // Fall through to the remove+retry path below.
                        }
                    }

                    // Stale/PID-reused — remove and retry.
                    // Propagate non-NotFound errors (e.g. Windows sharing violation).
                    if let Err(re) = fs::remove_file(path) {
                        if re.kind() != std::io::ErrorKind::NotFound {
                            return Err(anyhow::Error::from(re).context(format!(
                                "Failed to remove stale PID file: {}",
                                path.display()
                            )));
                        }
                        // NotFound: a racing process already cleaned it up — retry.
                    }

                    retries += 1;
                    if retries >= MAX_RETRIES {
                        anyhow::bail!(
                            "Failed to create PID file at {} after {} retries",
                            path.display(),
                            MAX_RETRIES
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
