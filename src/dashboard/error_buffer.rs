//! Ring buffer that captures recent WARN and ERROR tracing events for the dashboard.
//!
//! `ErrorBuffer` is cheaply cloneable (Arc inside) and produces a `tracing_subscriber::Layer`
//! that can be composed with any subscriber.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde::ser::Serializer;
use tracing::Level;
use tracing_subscriber::Layer;

// ---------------------------------------------------------------------------
// LogEntry
// ---------------------------------------------------------------------------

/// A single captured log event.
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    #[serde(serialize_with = "serialize_system_time")]
    pub timestamp: SystemTime,
    pub level: String,
    pub message: String,
    pub target: String,
}

fn serialize_system_time<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let secs = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    serializer.serialize_u64(secs)
}

// ---------------------------------------------------------------------------
// ErrorBuffer
// ---------------------------------------------------------------------------

/// Thread-safe ring buffer of recent WARN/ERROR log entries.
///
/// Clone is cheap — all clones share the same underlying `Arc<Mutex<VecDeque>>`.
#[derive(Debug, Clone)]
pub struct ErrorBuffer {
    inner: Arc<Mutex<VecDeque<LogEntry>>>,
    capacity: usize,
}

impl ErrorBuffer {
    /// Create a new empty buffer with the given maximum capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    /// Returns a snapshot of all entries, oldest first.
    pub fn recent_entries(&self) -> Vec<LogEntry> {
        self.inner
            .lock()
            .expect("ErrorBuffer mutex poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// Returns a `tracing_subscriber::Layer` that feeds this buffer.
    pub fn layer(&self) -> ErrorBufferLayer {
        ErrorBufferLayer {
            buffer: self.clone(),
        }
    }

    /// Push a new entry, evicting the oldest if at capacity.
    fn push(&self, entry: LogEntry) {
        let mut deque = self.inner.lock().expect("ErrorBuffer mutex poisoned");
        if deque.len() == self.capacity {
            deque.pop_front();
        }
        deque.push_back(entry);
    }
}

// ---------------------------------------------------------------------------
// MessageVisitor — extracts the "message" field from a tracing event
// ---------------------------------------------------------------------------

struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_owned();
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
}

// ---------------------------------------------------------------------------
// ErrorBufferLayer
// ---------------------------------------------------------------------------

/// A `tracing_subscriber::Layer` that captures WARN and ERROR events into an `ErrorBuffer`.
pub struct ErrorBufferLayer {
    buffer: ErrorBuffer,
}

impl<S> Layer<S> for ErrorBufferLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let level = *event.metadata().level();
        if level != Level::WARN && level != Level::ERROR {
            return;
        }

        let mut visitor = MessageVisitor {
            message: String::new(),
        };
        event.record(&mut visitor);

        let entry = LogEntry {
            timestamp: SystemTime::now(),
            level: level.to_string(),
            message: visitor.message,
            target: event.metadata().target().to_owned(),
        };
        self.buffer.push(entry);
    }
}
