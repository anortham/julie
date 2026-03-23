//! In-memory session metrics with atomic counters.
//!
//! Pre-allocated at handler construction, zero-allocation on the hot path.
//! Indexed by ToolKind ordinal for O(1) per-tool counter access.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Per-tool atomic counters. Default-initialized to zero.
pub struct ToolCounters {
    pub calls: AtomicU64,
    pub duration_us: AtomicU64,
    pub output_bytes: AtomicU64,
}

impl Default for ToolCounters {
    fn default() -> Self {
        Self {
            calls: AtomicU64::new(0),
            duration_us: AtomicU64::new(0),
            output_bytes: AtomicU64::new(0),
        }
    }
}

/// Maps tool names to array indices. Known at compile time.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum ToolKind {
    FastSearch = 0,
    FastRefs = 1,
    GetSymbols = 2,
    DeepDive = 3,
    GetContext = 4,
    RenameSymbol = 5,
    ManageWorkspace = 6,
    QueryMetrics = 7,
}

impl ToolKind {
    pub const COUNT: usize = 8;

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "fast_search" => Some(Self::FastSearch),
            "fast_refs" => Some(Self::FastRefs),
            "get_symbols" => Some(Self::GetSymbols),
            "deep_dive" => Some(Self::DeepDive),
            "get_context" => Some(Self::GetContext),
            "rename_symbol" => Some(Self::RenameSymbol),
            "manage_workspace" => Some(Self::ManageWorkspace),
            "query_metrics" => Some(Self::QueryMetrics),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::FastSearch => "fast_search",
            Self::FastRefs => "fast_refs",
            Self::GetSymbols => "get_symbols",
            Self::DeepDive => "deep_dive",
            Self::GetContext => "get_context",
            Self::RenameSymbol => "rename_symbol",
            Self::ManageWorkspace => "manage_workspace",
            Self::QueryMetrics => "query_metrics",
        }
    }
}

/// Metrics captured from inside a tool's call_tool method.
pub struct ToolCallReport {
    pub result_count: Option<u32>,
    pub source_bytes: Option<u64>,
    pub output_bytes: u64,
    pub metadata: serde_json::Value,
    /// File paths touched by this tool call (for deferred source_bytes lookup).
    pub source_file_paths: Vec<String>,
}

impl ToolCallReport {
    pub fn empty() -> Self {
        Self {
            result_count: None,
            source_bytes: None,
            output_bytes: 0,
            metadata: serde_json::Value::Null,
            source_file_paths: Vec::new(),
        }
    }
}

/// Extract file paths from Julie's formatted tool output.
/// Looks for patterns like `src/foo/bar.rs:42` or `src/foo/bar.rs` in the text.
pub fn extract_source_paths(text: &str) -> Vec<String> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut paths = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        // Skip empty lines and header/decoration lines
        if trimmed.is_empty() || trimmed.starts_with("──") || trimmed.starts_with("NOTE:") {
            continue;
        }
        // Look for tokens that look like file paths: contain '/' and end with a known extension
        for token in trimmed.split_whitespace() {
            // Strip trailing punctuation and line:col suffixes
            let clean = token.trim_end_matches(|c: char| c == ',' || c == ')' || c == ']');
            // Split off :line_number if present
            let path_part = if let Some(idx) = clean.find(':') {
                let after = &clean[idx + 1..];
                if after.chars().all(|c| c.is_ascii_digit() || c == '-') {
                    &clean[..idx]
                } else {
                    clean
                }
            } else {
                clean
            };
            // Must contain '/' and have a file extension
            if path_part.contains('/') && !path_part.starts_with("http") && path_part.contains('.')
            {
                let p = path_part.to_string();
                if seen.insert(p.clone()) {
                    paths.push(p);
                }
            }
        }
    }
    paths
}

/// Session-wide metrics. Wrapped in Arc on the handler.
pub struct SessionMetrics {
    pub session_id: String,
    pub session_start: Instant,
    pub total_calls: AtomicU64,
    pub total_duration_us: AtomicU64,
    pub total_source_bytes: AtomicU64,
    pub total_output_bytes: AtomicU64,
    pub per_tool: [ToolCounters; ToolKind::COUNT],
}

impl SessionMetrics {
    pub fn new() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            session_start: Instant::now(),
            total_calls: AtomicU64::new(0),
            total_duration_us: AtomicU64::new(0),
            total_source_bytes: AtomicU64::new(0),
            total_output_bytes: AtomicU64::new(0),
            per_tool: std::array::from_fn(|_| ToolCounters::default()),
        }
    }

    pub fn record(&self, tool: ToolKind, duration_us: u64, source_bytes: u64, output_bytes: u64) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.total_duration_us
            .fetch_add(duration_us, Ordering::Relaxed);
        self.total_source_bytes
            .fetch_add(source_bytes, Ordering::Relaxed);
        self.total_output_bytes
            .fetch_add(output_bytes, Ordering::Relaxed);

        let counters = &self.per_tool[tool as usize];
        counters.calls.fetch_add(1, Ordering::Relaxed);
        counters
            .duration_us
            .fetch_add(duration_us, Ordering::Relaxed);
        counters
            .output_bytes
            .fetch_add(output_bytes, Ordering::Relaxed);
    }

    pub fn total_calls(&self) -> u64 {
        self.total_calls.load(Ordering::Relaxed)
    }

    pub fn total_source_bytes(&self) -> u64 {
        self.total_source_bytes.load(Ordering::Relaxed)
    }

    pub fn total_output_bytes(&self) -> u64 {
        self.total_output_bytes.load(Ordering::Relaxed)
    }
}
