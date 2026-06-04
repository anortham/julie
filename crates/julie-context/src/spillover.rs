use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use blake3::Hasher;

const DEFAULT_MAX_ENTRIES: usize = 256;
const DEFAULT_TTL_SECS: u64 = 15 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpilloverFormat {
    Readable,
    Compact,
}

impl SpilloverFormat {
    pub fn from_option(value: Option<&str>) -> Self {
        match value {
            Some(v) if v.eq_ignore_ascii_case("readable") => Self::Readable,
            _ => Self::Compact,
        }
    }

    /// Strict parse used by tools that want to reject typos instead of
    /// silently coercing to Readable. Case-insensitive; empty strings error.
    pub fn parse_strict(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "readable" => Ok(Self::Readable),
            "compact" => Ok(Self::Compact),
            "" => Err("format must be \"readable\" or \"compact\" (got empty string)".to_string()),
            other => Err(format!(
                "unknown format \"{other}\" (expected \"readable\" or \"compact\")"
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Readable => "readable",
            Self::Compact => "compact",
        }
    }
}

#[derive(Debug, Clone)]
struct SpilloverEntry {
    owner_session_id: String,
    prefix: String,
    title: String,
    rows: Arc<Vec<String>>,
    offset: usize,
    default_limit: usize,
    format: SpilloverFormat,
    created_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpilloverPage {
    pub title: String,
    pub rows: Vec<String>,
    pub next_handle: Option<String>,
    pub format: SpilloverFormat,
}

#[derive(Default)]
struct SpilloverState {
    entries: HashMap<String, SpilloverEntry>,
    order: VecDeque<String>,
}

pub struct SpilloverStore {
    inner: Mutex<SpilloverState>,
    max_entries: usize,
    ttl: Duration,
}

impl Default for SpilloverStore {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_ENTRIES, Duration::from_secs(DEFAULT_TTL_SECS))
    }
}

impl SpilloverStore {
    pub fn new(max_entries: usize, ttl: Duration) -> Self {
        Self {
            inner: Mutex::new(SpilloverState::default()),
            max_entries: max_entries.max(1),
            ttl,
        }
    }

    pub fn store_rows(
        &self,
        owner_session_id: &str,
        prefix: &str,
        title: impl Into<String>,
        rows: Vec<String>,
        offset: usize,
        default_limit: usize,
        format: SpilloverFormat,
    ) -> Option<String> {
        if offset >= rows.len() {
            return None;
        }

        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.purge_expired_locked(&mut inner, Instant::now());

        let handle = self.insert_locked(
            &mut inner,
            SpilloverEntry {
                owner_session_id: owner_session_id.to_string(),
                prefix: prefix.to_string(),
                title: title.into(),
                rows: Arc::new(rows),
                offset,
                default_limit: default_limit.max(1),
                format,
                created_at: Instant::now(),
            },
        );

        Some(handle)
    }

    pub fn page(
        &self,
        owner_session_id: &str,
        handle: &str,
        limit: Option<usize>,
        format: Option<SpilloverFormat>,
    ) -> Result<SpilloverPage> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();

        if let Some(entry) = inner.entries.get(handle) {
            if now.duration_since(entry.created_at) > self.ttl {
                inner.entries.remove(handle);
                inner.order.retain(|queued_handle| queued_handle != handle);
                return Err(anyhow!("Spillover handle expired."));
            }
        }

        self.purge_expired_locked(&mut inner, now);

        let Some(entry) = inner.entries.get(handle).cloned() else {
            return Err(anyhow!("Spillover handle not found."));
        };

        if entry.owner_session_id != owner_session_id {
            return Err(anyhow!("Spillover handle does not belong to this session."));
        }

        let page_limit = limit.unwrap_or(entry.default_limit).max(1);
        let start = entry.offset;
        let end = (start + page_limit).min(entry.rows.len());
        let next_handle = if end < entry.rows.len() {
            Some(self.insert_locked(
                &mut inner,
                SpilloverEntry {
                    offset: end,
                    ..entry.clone()
                },
            ))
        } else {
            None
        };

        Ok(SpilloverPage {
            title: entry.title,
            rows: entry.rows[start..end].to_vec(),
            next_handle,
            format: format.unwrap_or(entry.format),
        })
    }

    fn insert_locked(&self, inner: &mut SpilloverState, entry: SpilloverEntry) -> String {
        let handle = stable_handle(&entry);
        inner.order.retain(|queued_handle| queued_handle != &handle);
        inner.order.push_back(handle.clone());
        inner.entries.insert(handle.clone(), entry);
        self.enforce_limit_locked(inner);
        handle
    }

    fn purge_expired_locked(&self, inner: &mut SpilloverState, now: Instant) {
        let expired: Vec<String> = inner
            .entries
            .iter()
            .filter_map(|(handle, entry)| {
                if now.duration_since(entry.created_at) > self.ttl {
                    Some(handle.clone())
                } else {
                    None
                }
            })
            .collect();

        if expired.is_empty() {
            return;
        }

        for handle in expired {
            inner.entries.remove(&handle);
        }
        inner
            .order
            .retain(|handle| inner.entries.contains_key(handle));
    }

    fn enforce_limit_locked(&self, inner: &mut SpilloverState) {
        while inner.entries.len() > self.max_entries {
            let Some(oldest) = inner.order.pop_front() else {
                break;
            };
            inner.entries.remove(&oldest);
        }
    }
}

fn stable_handle(entry: &SpilloverEntry) -> String {
    let mut hasher = Hasher::new();
    hasher.update(entry.owner_session_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(entry.prefix.as_bytes());
    hasher.update(&[0]);
    hasher.update(entry.title.as_bytes());
    hasher.update(&[0]);
    hasher.update(entry.offset.to_string().as_bytes());
    hasher.update(&[0]);
    hasher.update(entry.default_limit.to_string().as_bytes());
    hasher.update(&[0]);
    hasher.update(entry.format.as_str().as_bytes());
    hasher.update(&[0]);

    for row in entry.rows.iter() {
        hasher.update(row.as_bytes());
        hasher.update(&[0x1e]);
    }

    let digest = hasher.finalize().to_hex().to_string();
    format!("{}_{}", entry.prefix, &digest[..24])
}
