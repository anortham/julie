use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use fs2::FileExt;

pub const DEFAULT_EXTERNAL_EXTRACT_OPERATION_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub struct ExternalExtractOperationLock {
    lock_path: PathBuf,
    lock_file: File,
}

#[derive(Debug)]
pub enum ExternalExtractOperationLockError {
    Io {
        lock_path: PathBuf,
        source: io::Error,
    },
    Timeout {
        lock_path: PathBuf,
        timeout: Duration,
    },
}

impl ExternalExtractOperationLock {
    pub fn acquire(db_path: &Path) -> Result<Self, ExternalExtractOperationLockError> {
        Self::acquire_with_timeout(db_path, DEFAULT_EXTERNAL_EXTRACT_OPERATION_LOCK_TIMEOUT)
    }

    pub fn acquire_with_timeout(
        db_path: &Path,
        timeout: Duration,
    ) -> Result<Self, ExternalExtractOperationLockError> {
        let lock_path = external_extract_operation_lock_path(db_path);
        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| ExternalExtractOperationLockError::Io {
                lock_path: lock_path.clone(),
                source,
            })?;

        let start = Instant::now();
        loop {
            match lock_file.try_lock_exclusive() {
                Ok(()) => {
                    return Ok(Self {
                        lock_path,
                        lock_file,
                    });
                }
                Err(source) if source.kind() == io::ErrorKind::WouldBlock => {
                    let elapsed = start.elapsed();
                    if elapsed >= timeout {
                        return Err(ExternalExtractOperationLockError::Timeout {
                            lock_path,
                            timeout,
                        });
                    }
                    let remaining = timeout.saturating_sub(elapsed);
                    thread::sleep(Duration::from_millis(10).min(remaining));
                }
                Err(source) => {
                    return Err(ExternalExtractOperationLockError::Io { lock_path, source });
                }
            }
        }
    }

    pub fn lock_path(&self) -> PathBuf {
        self.lock_path.clone()
    }
}

impl Drop for ExternalExtractOperationLock {
    fn drop(&mut self) {
        let _ = self.lock_file.unlock();
    }
}

impl ExternalExtractOperationLockError {
    pub fn lock_path(&self) -> PathBuf {
        match self {
            Self::Io { lock_path, .. } | Self::Timeout { lock_path, .. } => lock_path.clone(),
        }
    }

    pub fn timeout(&self) -> Duration {
        match self {
            Self::Timeout { timeout, .. } => *timeout,
            Self::Io { .. } => Duration::ZERO,
        }
    }
}

impl std::fmt::Display for ExternalExtractOperationLockError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { lock_path, source } => {
                write!(
                    formatter,
                    "failed to acquire external extract operation lock {}: {}",
                    lock_path.display(),
                    source
                )
            }
            Self::Timeout { lock_path, timeout } => {
                write!(
                    formatter,
                    "timed out after {:?} acquiring external extract operation lock {}",
                    timeout,
                    lock_path.display()
                )
            }
        }
    }
}

impl std::error::Error for ExternalExtractOperationLockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Timeout { .. } => None,
        }
    }
}

pub fn external_extract_operation_lock_path(db_path: &Path) -> PathBuf {
    let mut lock_path = db_path.as_os_str().to_os_string();
    lock_path.push(".julie-extract.lock");
    PathBuf::from(lock_path)
}
