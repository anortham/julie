use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const KEEP_REQUESTS: usize = 50;
const KEEP_ERRORS: usize = 20;

#[derive(Serialize, Clone)]
pub struct RequestRecord {
    pub tool: String,
    pub workspace_id: Option<String>,
    pub latency_ms: u128,
    pub outcome: &'static str,
    pub at: String,
}

#[derive(Serialize, Clone)]
pub struct ErrorRecord {
    pub tool: String,
    pub code: String,
    pub message: String,
    pub at: String,
}

pub struct StatusLog {
    started: Instant,
    inner: Mutex<Inner>,
}

struct Inner {
    requests: VecDeque<RequestRecord>,
    errors: VecDeque<ErrorRecord>,
    last_activity: Instant,
    in_flight: usize,
}

#[derive(Serialize)]
pub struct StatusDocument {
    pub version: &'static str,
    pub pid: u32,
    pub uptime_seconds: u64,
    pub in_flight: usize,
    pub rss_bytes: Option<u64>,
    pub requests: Vec<RequestRecord>,
    pub errors: Vec<ErrorRecord>,
}

impl StatusLog {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            inner: Mutex::new(Inner {
                requests: VecDeque::new(),
                errors: VecDeque::new(),
                last_activity: now,
                in_flight: 0,
            }),
        }
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap()
    }

    pub fn begin(&self) {
        let mut inner = self.guard();
        inner.in_flight += 1;
        inner.last_activity = Instant::now();
    }

    pub fn end(&self, record: RequestRecord, error: Option<ErrorRecord>) {
        let mut inner = self.guard();
        inner.in_flight = inner.in_flight.saturating_sub(1);
        inner.last_activity = Instant::now();
        inner.requests.push_front(record);
        inner.requests.truncate(KEEP_REQUESTS);
        if let Some(error) = error {
            inner.errors.push_front(error);
            inner.errors.truncate(KEEP_ERRORS);
        }
    }

    pub fn idle_for(&self) -> Option<Duration> {
        let inner = self.guard();
        (inner.in_flight == 0).then(|| inner.last_activity.elapsed())
    }

    pub fn document(&self) -> StatusDocument {
        let inner = self.guard();
        StatusDocument {
            version: env!("CARGO_PKG_VERSION"),
            pid: std::process::id(),
            uptime_seconds: self.started.elapsed().as_secs(),
            in_flight: inner.in_flight,
            rss_bytes: rss_bytes(),
            requests: inner.requests.iter().cloned().collect(),
            errors: inner.errors.iter().cloned().collect(),
        }
    }
}

impl Default for StatusLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "linux")]
fn rss_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    statm
        .split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
        .map(|p| p * 4096)
}

#[cfg(not(target_os = "linux"))]
fn rss_bytes() -> Option<u64> {
    None
}

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
