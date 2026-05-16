//! Per-workspace SQLite connection pool.
//!
//! [`WorkspaceConnectionPool`] maintains a bounded pool of `rusqlite::Connection`s
//! for a single workspace database file. [`acquire()`] returns a RAII [`PooledConn`]
//! that returns the connection to the pool on drop and wakes any waiting acquirer.
//!
//! Internal locking strategy:
//! - `std::sync::Mutex` guards [`Inner`] — held only for microseconds (no I/O inside).
//! - `tokio::sync::Notify` is used as a lightweight wakeup channel; no bounded queue
//!   is needed because the acquire loop re-checks under the mutex each time it wakes.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use rusqlite::Connection;
use tokio::sync::Notify;

// ──────────────────────────────────────────────────────────────────────────────
// Public types
// ──────────────────────────────────────────────────────────────────────────────

/// Snapshot of pool occupancy (for observability and tests).
#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    pub idle: usize,
    pub in_use: usize,
    pub min: usize,
    pub max: usize,
}

/// RAII guard — dereferences to `rusqlite::Connection`.
/// Dropping returns the connection to the pool and notifies one waiting acquirer.
///
/// `Send` because `rusqlite::Connection: Send`.  Not `Sync` (no shared mutation).
pub struct PooledConn {
    conn: Option<Connection>,
    pool: Arc<WorkspaceConnectionPool>,
}

impl std::ops::Deref for PooledConn {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("PooledConn used after drop")
    }
}

impl std::ops::DerefMut for PooledConn {
    fn deref_mut(&mut self) -> &mut Connection {
        self.conn.as_mut().expect("PooledConn used after drop")
    }
}

impl Drop for PooledConn {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            let mut inner = self.pool.inner.lock().expect("pool mutex poisoned");
            inner.in_use = inner.in_use.saturating_sub(1);
            inner.idle.push(IdleEntry {
                conn,
                returned_at: Instant::now(),
            });
            drop(inner);
            self.pool.notify.notify_one();
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal storage
// ──────────────────────────────────────────────────────────────────────────────

struct IdleEntry {
    conn: Connection,
    returned_at: Instant,
}

struct Inner {
    idle: Vec<IdleEntry>,
    in_use: usize,
    min: usize,
    max: usize,
}

// ──────────────────────────────────────────────────────────────────────────────
// WorkspaceConnectionPool
// ──────────────────────────────────────────────────────────────────────────────

pub struct WorkspaceConnectionPool {
    db_path: PathBuf,
    inner: Mutex<Inner>,
    notify: Notify,
}

impl WorkspaceConnectionPool {
    /// Construct the pool with default limits.
    ///
    /// `min` = 2; `max` = `min(available_parallelism * 2, 16)`.
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let min = 2usize;
        let max = {
            let parallelism = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            (parallelism * 2).min(16).max(min)
        };
        Self::with_limits(db_path, min, max)
    }

    /// Construct with explicit min/max (useful in tests).
    ///
    /// Eagerly opens `min` connections at construction. Returns an error if any
    /// pre-warm connection fails.
    pub fn with_limits(db_path: PathBuf, min: usize, max: usize) -> Result<Self> {
        assert!(min <= max, "min ({min}) must be <= max ({max})");

        let mut idle = Vec::with_capacity(min);
        for _ in 0..min {
            let conn = open_connection(&db_path)?;
            idle.push(IdleEntry {
                conn,
                returned_at: Instant::now(),
            });
        }

        Ok(Self {
            db_path,
            inner: Mutex::new(Inner {
                idle,
                in_use: 0,
                min,
                max,
            }),
            notify: Notify::new(),
        })
    }

    /// Acquire a connection. Blocks (asynchronously) until one is available.
    ///
    /// Never holds the mutex across `.await` or file I/O.
    pub async fn acquire(self: &Arc<Self>) -> Result<PooledConn> {
        loop {
            // ── Try to get a connection under the mutex ──────────────────────
            let outcome = {
                let mut inner = self.inner.lock().expect("pool mutex poisoned");
                let total = inner.in_use + inner.idle.len();

                if let Some(entry) = inner.idle.pop() {
                    inner.in_use += 1;
                    AcquireOutcome::Reuse(entry.conn)
                } else if total < inner.max {
                    inner.in_use += 1;
                    AcquireOutcome::OpenNew
                } else {
                    AcquireOutcome::Wait
                }
            };
            // Mutex released here, before any I/O or await ───────────────────

            match outcome {
                AcquireOutcome::Reuse(conn) => {
                    return Ok(PooledConn {
                        conn: Some(conn),
                        pool: Arc::clone(self),
                    });
                }
                AcquireOutcome::OpenNew => {
                    // File I/O outside the mutex
                    match open_connection(&self.db_path) {
                        Ok(conn) => {
                            return Ok(PooledConn {
                                conn: Some(conn),
                                pool: Arc::clone(self),
                            });
                        }
                        Err(e) => {
                            // Roll back the in_use increment we optimistically took
                            let mut inner = self.inner.lock().expect("pool mutex poisoned");
                            inner.in_use = inner.in_use.saturating_sub(1);
                            drop(inner);
                            self.notify.notify_one();
                            return Err(e);
                        }
                    }
                }
                AcquireOutcome::Wait => {
                    // Async wait — no mutex held
                    self.notify.notified().await;
                    // Loop back and try again
                }
            }
        }
    }

    /// Evict idle connections older than `idle_threshold`, never dropping below `min`.
    ///
    /// `now` is injectable so tests can advance time without sleeping.
    /// Returns the number of connections evicted.
    pub fn evict_idle(&self, idle_threshold: Duration, now: Instant) -> usize {
        let mut inner = self.inner.lock().expect("pool mutex poisoned");
        let in_use = inner.in_use;
        let min = inner.min;

        // Partition into stale and fresh buckets.
        let mut stale: Vec<IdleEntry> = Vec::new();
        let mut fresh: Vec<IdleEntry> = Vec::new();
        for entry in inner.idle.drain(..) {
            let age = now.saturating_duration_since(entry.returned_at);
            if age >= idle_threshold {
                stale.push(entry);
            } else {
                fresh.push(entry);
            }
        }

        // We must keep (idle + in_use) >= min.
        // fresh entries are always kept; stale entries can be evicted as long as
        // the floor is respected.
        let must_keep_idle = min.saturating_sub(in_use);
        let currently_idle = stale.len() + fresh.len();
        let evictable = currently_idle.saturating_sub(must_keep_idle);
        // Prefer evicting stale entries first.
        let evict_count = evictable.min(stale.len());
        let keep_count = stale.len() - evict_count;

        // Collect surviving stale entries (keep the most-recently-returned ones,
        // which are at the end because Drop appends with `push`).
        let mut survivors: Vec<IdleEntry> = stale.into_iter().take(keep_count).collect();
        survivors.extend(fresh);
        inner.idle = survivors;

        evict_count
    }

    /// Snapshot of current pool statistics.
    pub fn stats(&self) -> PoolStats {
        let inner = self.inner.lock().expect("pool mutex poisoned");
        PoolStats {
            idle: inner.idle.len(),
            in_use: inner.in_use,
            min: inner.min,
            max: inner.max,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Outcome of a single acquire attempt (before any I/O or await).
enum AcquireOutcome {
    Reuse(Connection),
    OpenNew,
    Wait,
}

/// Open a single `rusqlite::Connection` with Julie's standard PRAGMA set.
///
/// Mirrors the settings applied in `SymbolDatabase::new` (`src/database/mod.rs`).
/// Factored here so every connection — eagerly pre-warmed or opened under load —
/// gets identical configuration.
fn open_connection(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .map_err(|e| anyhow!("Failed to open connection to {}: {}", db_path.display(), e))?;

    // WAL mode — must be set before any other operations.
    conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
        .map_err(|e| anyhow!("Failed to set WAL mode: {e}"))?;

    // Verify WAL was actually applied (some filesystems silently downgrade).
    let actual: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .map_err(|e| anyhow!("Failed to query journal_mode: {e}"))?;
    if !actual.eq_ignore_ascii_case("wal") {
        return Err(anyhow!(
            "WAL mode not applied (got '{actual}'). \
             This filesystem may not support WAL."
        ));
    }

    conn.busy_timeout(Duration::from_millis(5000))
        .map_err(|e| anyhow!("Failed to set busy_timeout: {e}"))?;

    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| anyhow!("Failed to set synchronous=NORMAL: {e}"))?;

    conn.pragma_update(None, "wal_autocheckpoint", 2000)
        .map_err(|e| anyhow!("Failed to set wal_autocheckpoint: {e}"))?;

    Ok(conn)
}
