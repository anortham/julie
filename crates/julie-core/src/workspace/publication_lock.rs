//! crates/julie-core/src/workspace/publication_lock.rs
//! Process-shared OS advisory lock at `<index_root>/publication.lock`
//! synchronizing readers and writers across separate OS processes.

use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct PublicationLock {
    path: PathBuf,
}

#[derive(Debug)]
pub struct PublicationSharedGuard {
    file: File,
    #[allow(dead_code)]
    path: PathBuf,
}

#[derive(Debug)]
pub struct PublicationExclusiveGuard {
    file: File,
    #[allow(dead_code)]
    path: PathBuf,
}

#[derive(Debug)]
pub enum PublicationLockError {
    Timeout(Duration),
    WouldBlock,
    Io { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for PublicationLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout(d) => write!(f, "Publication lock acquisition timed out after {d:?}"),
            Self::WouldBlock => write!(f, "Publication lock would block"),
            Self::Io { path, source } => {
                write!(
                    f,
                    "I/O error on publication lock {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for PublicationLockError {}

impl PublicationLock {
    pub fn new(index_root: &Path) -> Self {
        Self {
            path: index_root.join("publication.lock"),
        }
    }

    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn open_lock_file(path: &Path) -> Result<File, PublicationLockError> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|source| PublicationLockError::Io {
                path: path.to_path_buf(),
                source,
            })
    }

    /// Acquire shared lock for readers with cancellable exponential backoff (5-25ms).
    pub async fn acquire_shared(
        &self,
        deadline: Instant,
    ) -> Result<PublicationSharedGuard, PublicationLockError> {
        let file = Self::open_lock_file(&self.path)?;
        let mut backoff = Duration::from_millis(5);
        loop {
            match FileExt::try_lock_shared(&file) {
                Ok(()) => {
                    return Ok(PublicationSharedGuard {
                        file,
                        path: self.path.clone(),
                    });
                }
                Err(e) if is_would_block(&e) => {
                    if Instant::now() >= deadline {
                        return Err(PublicationLockError::Timeout(deadline.elapsed()));
                    }
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_millis(25));
                }
                Err(source) => {
                    return Err(PublicationLockError::Io {
                        path: self.path.clone(),
                        source,
                    });
                }
            }
        }
    }

    /// Acquire exclusive lock for writers around final publication/stamping with backoff.
    pub async fn acquire_exclusive(
        &self,
        deadline: Instant,
    ) -> Result<PublicationExclusiveGuard, PublicationLockError> {
        let file = Self::open_lock_file(&self.path)?;
        let mut backoff = Duration::from_millis(5);
        loop {
            match FileExt::try_lock_exclusive(&file) {
                Ok(()) => {
                    return Ok(PublicationExclusiveGuard {
                        file,
                        path: self.path.clone(),
                    });
                }
                Err(e) if is_would_block(&e) => {
                    if Instant::now() >= deadline {
                        return Err(PublicationLockError::Timeout(deadline.elapsed()));
                    }
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_millis(25));
                }
                Err(source) => {
                    return Err(PublicationLockError::Io {
                        path: self.path.clone(),
                        source,
                    });
                }
            }
        }
    }

    /// Non-blocking attempt to acquire a shared lock on a specific path.
    pub fn try_acquire_shared(path: &Path) -> Result<PublicationSharedGuard, PublicationLockError> {
        let file = Self::open_lock_file(path)?;
        match FileExt::try_lock_shared(&file) {
            Ok(()) => Ok(PublicationSharedGuard {
                file,
                path: path.to_path_buf(),
            }),
            Err(e) if is_would_block(&e) => Err(PublicationLockError::WouldBlock),
            Err(source) => Err(PublicationLockError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Non-blocking attempt to acquire an exclusive lock on a specific path.
    pub fn try_acquire_exclusive(
        path: &Path,
    ) -> Result<PublicationExclusiveGuard, PublicationLockError> {
        let file = Self::open_lock_file(path)?;
        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(PublicationExclusiveGuard {
                file,
                path: path.to_path_buf(),
            }),
            Err(e) if is_would_block(&e) => Err(PublicationLockError::WouldBlock),
            Err(source) => Err(PublicationLockError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Blocking acquire exclusive lock on path (e.g. for synchronous operations or test helpers).
    pub fn acquire_exclusive_sync(
        path: &Path,
    ) -> Result<PublicationExclusiveGuard, PublicationLockError> {
        let file = Self::open_lock_file(path)?;
        FileExt::lock_exclusive(&file).map_err(|source| PublicationLockError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(PublicationExclusiveGuard {
            file,
            path: path.to_path_buf(),
        })
    }

    /// Associated helper: acquire exclusive lock on path.
    pub fn acquire_exclusive_path(
        path: &Path,
    ) -> Result<PublicationExclusiveGuard, PublicationLockError> {
        Self::acquire_exclusive_sync(path)
    }
}

impl Drop for PublicationSharedGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

impl Drop for PublicationExclusiveGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn is_would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock || (cfg!(windows) && err.raw_os_error() == Some(33))
}
