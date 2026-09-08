//! Cross-process host slot admission and lock pool for bounding concurrent index jobs.
//! Enforces OS advisory lock pool at `$JULIE_HOME/scheduler/index-{n}.lock` (1..=8 slots),
//! atomic `config.json` protected by `config.lock`, `RESOURCE_CONFIG_CONFLICT` on mismatch,
//! 50-100ms jittered backoff with cancellation, and stable retained lock files.

use std::fmt;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::workspace::leader_lock::{AcquireError, DaemonLockGuard};

static JITTER_STATE: AtomicU64 = AtomicU64::new(0x853c_49e6_748f_ea9b);

fn backoff_duration() -> Duration {
    let prev = JITTER_STATE.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed);
    let mut x = prev ^ (prev >> 30);
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    Duration::from_millis(50 + (x % 51))
}

/// Errors occurring during host slot admission and configuration.
#[derive(Debug)]
pub enum AdmissionError {
    ResourceConfigConflict {
        active: usize,
        requested: usize,
    },
    Busy {
        slots: usize,
    },
    Cancelled,
    Timeout,
    InvalidConfig(String),
    ConfigLockBusy,
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub type HostSlotError = AdmissionError;

impl AdmissionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ResourceConfigConflict { .. } => "RESOURCE_CONFIG_CONFLICT",
            Self::Busy { .. } => "BUSY",
            Self::Cancelled => "CANCELLED",
            Self::Timeout => "DEADLINE_EXCEEDED",
            Self::InvalidConfig(_) => "INVALID_CONFIG",
            Self::ConfigLockBusy => "CONFIG_LOCK_BUSY",
            Self::Io { .. } => "IO_ERROR",
        }
    }
}

impl fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourceConfigConflict { active, requested } => write!(
                f,
                "RESOURCE_CONFIG_CONFLICT: active pool configuration has {active} slots, conflicting with requested {requested} slots (reconfiguration refused while slots may be active)"
            ),
            Self::Busy { slots } => write!(f, "All {slots} host index slots are currently busy"),
            Self::Cancelled => write!(f, "Index job admission cancelled"),
            Self::Timeout => write!(f, "Timeout waiting for index job admission slot"),
            Self::InvalidConfig(msg) => {
                write!(f, "Invalid index job admission configuration: {msg}")
            }
            Self::ConfigLockBusy => write!(f, "Admission configuration lock is busy"),
            Self::Io { path, source } => write!(f, "I/O error on {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for AdmissionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Persistent on-disk pool configuration stored in `config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolConfig {
    pub version: u32,
    pub max_slots: usize,
    #[serde(default)]
    pub created_at_utc: Option<String>,
    #[serde(default)]
    pub creator_pid: Option<u32>,
}

impl PoolConfig {
    pub const CURRENT_VERSION: u32 = 1;
    pub const MIN_SLOTS: usize = 1;
    pub const MAX_SLOTS: usize = 8;

    pub fn new(max_slots: usize) -> Result<Self, AdmissionError> {
        if !(Self::MIN_SLOTS..=Self::MAX_SLOTS).contains(&max_slots) {
            return Err(AdmissionError::InvalidConfig(format!(
                "Slot count must be between {} and {}, got {max_slots}",
                Self::MIN_SLOTS,
                Self::MAX_SLOTS
            )));
        }
        Ok(Self {
            version: Self::CURRENT_VERSION,
            max_slots,
            created_at_utc: Some(chrono::Utc::now().to_rfc3339()),
            creator_pid: Some(std::process::id()),
        })
    }
}

/// RAII permit representing exclusive ownership of one host index slot.
#[derive(Debug)]
pub struct IndexJobPermit {
    slot_index: usize,
    guard: DaemonLockGuard,
}

impl IndexJobPermit {
    pub fn slot_index(&self) -> usize {
        self.slot_index
    }
    pub fn lock_path(&self) -> &Path {
        self.guard.path()
    }
    pub fn path(&self) -> &Path {
        self.guard.path()
    }
}

/// Host slot admission manager coordinating cross-process indexing limits.
pub struct IndexJobAdmission {
    scheduler_dir: PathBuf,
    max_slots: usize,
    slot_paths: Vec<PathBuf>,
    next_scan_offset: AtomicUsize,
}

pub type HostSlotPool = IndexJobAdmission;

impl IndexJobAdmission {
    pub const ENV_MAX_INDEX_JOBS: &'static str = "JULIE_MAX_INDEX_JOBS";
    pub const CONFIG_FILE_NAME: &'static str = "config.json";
    pub const CONFIG_LOCK_NAME: &'static str = "config.lock";

    pub fn default_slot_count() -> usize {
        let parallelism = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        std::cmp::min(2, parallelism).clamp(PoolConfig::MIN_SLOTS, PoolConfig::MAX_SLOTS)
    }

    pub fn requested_slots_from_env() -> Result<Option<usize>, AdmissionError> {
        let Ok(val) = std::env::var(Self::ENV_MAX_INDEX_JOBS) else {
            return Ok(None);
        };
        let parsed = val.trim().parse::<usize>().map_err(|e| {
            AdmissionError::InvalidConfig(format!(
                "Failed to parse {}: '{val}' ({e})",
                Self::ENV_MAX_INDEX_JOBS
            ))
        })?;
        if !(PoolConfig::MIN_SLOTS..=PoolConfig::MAX_SLOTS).contains(&parsed) {
            return Err(AdmissionError::InvalidConfig(format!(
                "{} must be 1..=8, got {parsed}",
                Self::ENV_MAX_INDEX_JOBS
            )));
        }
        Ok(Some(parsed))
    }

    pub fn open(scheduler_dir: PathBuf) -> Result<Self, AdmissionError> {
        Self::open_or_init(&scheduler_dir, None)
    }

    pub fn open_with_slots(
        scheduler_dir: PathBuf,
        requested_slots: usize,
    ) -> Result<Self, AdmissionError> {
        Self::open_or_init(&scheduler_dir, Some(requested_slots))
    }

    pub fn open_or_init(
        scheduler_dir: &Path,
        requested_slots: Option<usize>,
    ) -> Result<Self, AdmissionError> {
        std::fs::create_dir_all(scheduler_dir).map_err(|e| AdmissionError::Io {
            path: scheduler_dir.to_path_buf(),
            source: e,
        })?;

        let config_path = scheduler_dir.join(Self::CONFIG_FILE_NAME);
        let config_lock_path = scheduler_dir.join(Self::CONFIG_LOCK_NAME);

        let env_slots = Self::requested_slots_from_env()?;
        let is_explicit = requested_slots.is_some() || env_slots.is_some();
        let desired = requested_slots
            .or(env_slots)
            .unwrap_or_else(Self::default_slot_count);
        let pool_config = PoolConfig::new(desired)?;

        let effective_slots = if config_path.exists() {
            let active = read_pool_config(&config_path)?;
            if active.max_slots == pool_config.max_slots || !is_explicit {
                active.max_slots
            } else {
                try_reconfigure(
                    scheduler_dir,
                    &config_path,
                    &config_lock_path,
                    active.max_slots,
                    pool_config.max_slots,
                )?
            }
        } else {
            init_pool_config(
                scheduler_dir,
                &config_path,
                &config_lock_path,
                &pool_config,
                is_explicit,
            )?
        };

        let slot_paths = ensure_slot_files(scheduler_dir, effective_slots)?;

        Ok(Self {
            scheduler_dir: scheduler_dir.to_path_buf(),
            max_slots: effective_slots,
            slot_paths,
            next_scan_offset: AtomicUsize::new(0),
        })
    }

    pub fn max_slots(&self) -> usize {
        self.max_slots
    }
    pub fn slot_count(&self) -> usize {
        self.max_slots
    }
    pub fn scheduler_dir(&self) -> &Path {
        &self.scheduler_dir
    }
    pub fn slot_path(&self, index: usize) -> &Path {
        &self.slot_paths[index]
    }

    /// Try to acquire an available slot without blocking.
    pub fn try_acquire(&self) -> Result<IndexJobPermit, AdmissionError> {
        let offset = self.next_scan_offset.fetch_add(1, Ordering::Relaxed);
        for i in 0..self.max_slots {
            let slot_idx = (offset + i) % self.max_slots;
            let slot_path = &self.slot_paths[slot_idx];
            match DaemonLockGuard::try_acquire(slot_path) {
                Ok(guard) => {
                    return Ok(IndexJobPermit {
                        slot_index: slot_idx,
                        guard,
                    });
                }
                Err(AcquireError::AlreadyHeld(_)) => continue,
                Err(AcquireError::Io { path, source }) => {
                    return Err(AdmissionError::Io { path, source });
                }
            }
        }
        Err(AdmissionError::Busy {
            slots: self.max_slots,
        })
    }

    pub fn try_acquire_opt(&self) -> Result<Option<IndexJobPermit>, AdmissionError> {
        match self.try_acquire() {
            Ok(permit) => Ok(Some(permit)),
            Err(AdmissionError::Busy { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Acquire an index job permit waiting with 50-100ms backoff until deadline or cancellation.
    pub async fn acquire(
        &self,
        deadline: Option<Instant>,
        cancel: Option<&CancellationToken>,
    ) -> Result<IndexJobPermit, AdmissionError> {
        loop {
            if let Some(c) = cancel {
                if c.is_cancelled() {
                    return Err(AdmissionError::Cancelled);
                }
            }
            if let Some(d) = deadline {
                if Instant::now() >= d {
                    return Err(AdmissionError::Timeout);
                }
            }

            match self.try_acquire() {
                Ok(permit) => return Ok(permit),
                Err(AdmissionError::Busy { .. }) => {}
                Err(e) => return Err(e),
            }

            let backoff = backoff_duration();
            let sleep_dur = match deadline {
                Some(d) => backoff.min(d.saturating_duration_since(Instant::now())),
                None => backoff,
            };

            if let Some(c) = cancel {
                tokio::select! {
                    _ = c.cancelled() => return Err(AdmissionError::Cancelled),
                    _ = tokio::time::sleep(sleep_dur) => {}
                }
            } else {
                tokio::time::sleep(sleep_dur).await;
            }
        }
    }

    pub async fn acquire_timeout(
        &self,
        timeout: Duration,
        cancel: Option<&CancellationToken>,
    ) -> Result<IndexJobPermit, AdmissionError> {
        self.acquire(Some(Instant::now() + timeout), cancel).await
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Private Configuration & Atomic File Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn read_pool_config(config_path: &Path) -> Result<PoolConfig, AdmissionError> {
    let content = std::fs::read_to_string(config_path).map_err(|e| AdmissionError::Io {
        path: config_path.to_path_buf(),
        source: e,
    })?;
    serde_json::from_str::<PoolConfig>(&content).map_err(|e| {
        AdmissionError::InvalidConfig(format!(
            "Corrupt pool config {}: {e}",
            config_path.display()
        ))
    })
}

fn ensure_slot_files(scheduler_dir: &Path, count: usize) -> Result<Vec<PathBuf>, AdmissionError> {
    let mut paths = Vec::with_capacity(count);
    for i in 0..count {
        let path = scheduler_dir.join(format!("index-{i}.lock"));
        if !path.exists() {
            OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(&path)
                .map_err(|e| AdmissionError::Io {
                    path: path.clone(),
                    source: e,
                })?;
        }
        paths.push(path);
    }
    Ok(paths)
}

fn atomic_write_config(
    scheduler_dir: &Path,
    config_path: &Path,
    config: &PoolConfig,
) -> Result<(), AdmissionError> {
    let json = serde_json::to_string_pretty(config).map_err(|e| {
        AdmissionError::InvalidConfig(format!("Failed to serialize pool config: {e}"))
    })?;
    let temp_name = format!(
        ".config.tmp.{}.{}",
        std::process::id(),
        JITTER_STATE.fetch_add(1, Ordering::Relaxed)
    );
    let temp_path = scheduler_dir.join(temp_name);
    std::fs::write(&temp_path, json.as_bytes()).map_err(|e| AdmissionError::Io {
        path: temp_path.clone(),
        source: e,
    })?;
    std::fs::rename(&temp_path, config_path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        AdmissionError::Io {
            path: config_path.to_path_buf(),
            source: e,
        }
    })?;
    Ok(())
}

fn check_or_conflict(
    config_path: &Path,
    desired: &PoolConfig,
    is_explicit: bool,
) -> Result<Option<usize>, AdmissionError> {
    if config_path.exists() {
        let existing = read_pool_config(config_path)?;
        if existing.max_slots == desired.max_slots || !is_explicit {
            return Ok(Some(existing.max_slots));
        }
        return Err(AdmissionError::ResourceConfigConflict {
            active: existing.max_slots,
            requested: desired.max_slots,
        });
    }
    Ok(None)
}

fn init_pool_config(
    scheduler_dir: &Path,
    config_path: &Path,
    config_lock_path: &Path,
    desired: &PoolConfig,
    is_explicit: bool,
) -> Result<usize, AdmissionError> {
    if let Some(slots) = check_or_conflict(config_path, desired, is_explicit)? {
        return Ok(slots);
    }
    let guard = match DaemonLockGuard::try_acquire(config_lock_path) {
        Ok(g) => g,
        Err(AcquireError::AlreadyHeld(_)) => {
            std::thread::sleep(Duration::from_millis(50));
            return check_or_conflict(config_path, desired, is_explicit)?
                .ok_or(AdmissionError::ConfigLockBusy);
        }
        Err(AcquireError::Io { path, source }) => return Err(AdmissionError::Io { path, source }),
    };
    if let Some(slots) = check_or_conflict(config_path, desired, is_explicit)? {
        return Ok(slots);
    }
    atomic_write_config(scheduler_dir, config_path, desired)?;
    drop(guard);
    Ok(desired.max_slots)
}

fn try_reconfigure(
    scheduler_dir: &Path,
    config_path: &Path,
    config_lock_path: &Path,
    active_slots: usize,
    requested_slots: usize,
) -> Result<usize, AdmissionError> {
    let _config_guard = match DaemonLockGuard::try_acquire(config_lock_path) {
        Ok(g) => g,
        Err(AcquireError::AlreadyHeld(_)) => {
            return Err(AdmissionError::ResourceConfigConflict {
                active: active_slots,
                requested: requested_slots,
            });
        }
        Err(AcquireError::Io { path, source }) => return Err(AdmissionError::Io { path, source }),
    };

    if let Ok(current) = read_pool_config(config_path) {
        if current.max_slots == requested_slots {
            return Ok(requested_slots);
        }
    }

    let mut acquired_slots = Vec::with_capacity(active_slots);
    for i in 0..active_slots {
        let slot_path = scheduler_dir.join(format!("index-{i}.lock"));
        match DaemonLockGuard::try_acquire(&slot_path) {
            Ok(g) => acquired_slots.push(g),
            Err(AcquireError::AlreadyHeld(_)) => {
                return Err(AdmissionError::ResourceConfigConflict {
                    active: active_slots,
                    requested: requested_slots,
                });
            }
            Err(AcquireError::Io { path, source }) => {
                return Err(AdmissionError::Io { path, source });
            }
        }
    }

    let new_config = PoolConfig::new(requested_slots)?;
    atomic_write_config(scheduler_dir, config_path, &new_config)?;

    ensure_slot_files(scheduler_dir, requested_slots)?;
    Ok(requested_slots)
}
