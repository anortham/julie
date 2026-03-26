# Daemon Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a web dashboard to the Julie daemon for observability into workspaces, tool metrics, search debugging, and system health.

**Architecture:** Axum HTTP server running alongside the existing IPC listener in `run_daemon`. Tera templates rendered server-side, htmx for interactivity, SSE for live updates. Frontend assets embedded via `rust-embed` for release, loaded from disk in dev mode.

**Tech Stack:** Axum 0.8, Tera, htmx 2.x, Alpine.js, Bulma CSS, Chart.js (CDN), rust-embed, tokio broadcast channels for SSE.

**Spec:** `docs/superpowers/specs/2026-03-26-daemon-dashboard-design.md`

---

## File Structure

### New files to create

```
src/dashboard/
├── mod.rs                  # Router, template engine init, static serving, dev-mode detection
├── state.rs                # DashboardState struct with Arc refs + error ring buffer + broadcast
├── error_buffer.rs         # Tracing layer that captures warn/error into VecDeque
├── routes/
│   ├── mod.rs              # Re-exports route modules
│   ├── status.rs           # GET / -- system status landing page
│   ├── projects.rs         # GET /projects, GET /projects/:id/detail
│   ├── metrics.rs          # GET /metrics, GET /metrics/table
│   ├── search.rs           # GET /search, POST /search
│   └── events.rs           # SSE endpoints (GET /events/status, /events/metrics, /events/activity)

dashboard/
├── templates/
│   ├── base.html           # Shell: nav, theme toggle, CDN imports
│   ├── status.html         # System status page
│   ├── projects.html       # Projects list page
│   ├── metrics.html        # Metrics analytics page
│   ├── search.html         # Search playground page
│   └── partials/
│       ├── status_cards.html
│       ├── services_panel.html
│       ├── error_feed.html
│       ├── project_row.html
│       ├── project_detail.html
│       ├── metrics_summary.html
│       ├── metrics_table.html
│       ├── activity_event.html
│       ├── search_results.html
│       └── search_detail.html
├── static/
│   ├── app.css             # Custom dark theme, branding overrides
│   └── app.js              # Theme toggle, SSE helpers
```

### Files to modify

```
Cargo.toml                     # Add tera, rust-embed dependencies
src/lib.rs                     # Add `pub mod dashboard;`
src/cli.rs                     # Add --no-dashboard flag to Daemon command, add Dashboard command
src/main.rs                    # Pass --no-dashboard to run_daemon, handle Dashboard command
src/daemon/mod.rs              # Start HTTP server alongside IPC, accept DashboardState
src/paths.rs                   # Add daemon_port() method
src/handler.rs                 # Emit tool call events to broadcast channel
```

---

## Task 0: Add Dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add tera and rust-embed to Cargo.toml**

Add after the `axum` line (around line 87):

```toml
# Dashboard (daemon mode)
tera = "1"
rust-embed = "8"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: compiles with no errors (new deps are unused but that's fine)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add tera and rust-embed dependencies for dashboard"
```

---

## Task 1: Error Ring Buffer (Tracing Layer)

The error buffer captures recent warn/error log entries for the dashboard. This has no dependencies on any other dashboard code, so it can be built and tested in isolation.

**Files:**
- Create: `src/dashboard/error_buffer.rs`
- Create: `src/dashboard/mod.rs` (minimal, just declares submodules)

- [ ] **Step 1: Write the test**

Create `src/tests/dashboard/error_buffer.rs`:

```rust
use julie::dashboard::error_buffer::{ErrorBuffer, LogEntry};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[test]
fn test_error_buffer_captures_warn_and_error() {
    let buffer = ErrorBuffer::new(50);
    let buffer_clone = buffer.clone();

    // Set up tracing with our layer
    let subscriber = tracing_subscriber::registry()
        .with(buffer.layer());

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("this should not be captured");
        tracing::warn!("this is a warning");
        tracing::error!("this is an error");
    });

    let entries = buffer_clone.recent_entries();
    assert_eq!(entries.len(), 2, "should capture warn + error, not info");
    assert_eq!(entries[0].level, "WARN");
    assert_eq!(entries[0].message, "this is a warning");
    assert_eq!(entries[1].level, "ERROR");
    assert_eq!(entries[1].message, "this is an error");
}

#[test]
fn test_error_buffer_respects_capacity() {
    let buffer = ErrorBuffer::new(3);
    let buffer_clone = buffer.clone();

    let subscriber = tracing_subscriber::registry()
        .with(buffer.layer());

    tracing::subscriber::with_default(subscriber, || {
        for i in 0..5 {
            tracing::warn!("warning {}", i);
        }
    });

    let entries = buffer_clone.recent_entries();
    assert_eq!(entries.len(), 3, "should keep only last 3");
    assert_eq!(entries[0].message, "warning 2");
    assert_eq!(entries[1].message, "warning 3");
    assert_eq!(entries[2].message, "warning 4");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_error_buffer_captures_warn_and_error 2>&1 | tail -10`
Expected: FAIL (module doesn't exist yet)

- [ ] **Step 3: Create the dashboard module skeleton**

Create `src/dashboard/mod.rs`:

```rust
pub mod error_buffer;
```

Add to `src/lib.rs` after `pub mod daemon;` (line 25):

```rust
pub mod dashboard;
```

- [ ] **Step 4: Implement ErrorBuffer**

Create `src/dashboard/error_buffer.rs`:

```rust
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use tracing::Subscriber;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

/// A single captured log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: SystemTime,
    pub level: String,
    pub message: String,
    pub target: String,
}

/// Thread-safe ring buffer that captures warn/error tracing events.
///
/// Clone this to share between the tracing layer and dashboard readers.
/// The inner state is behind Arc<Mutex<..>> so cloning is cheap.
#[derive(Clone)]
pub struct ErrorBuffer {
    inner: Arc<Mutex<VecDeque<LogEntry>>>,
    capacity: usize,
}

impl ErrorBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    /// Return a copy of all buffered entries (oldest first).
    pub fn recent_entries(&self) -> Vec<LogEntry> {
        let guard = self.inner.lock().unwrap();
        guard.iter().cloned().collect()
    }

    /// Create a tracing layer that feeds into this buffer.
    pub fn layer(&self) -> ErrorBufferLayer {
        ErrorBufferLayer {
            buffer: self.clone(),
        }
    }

    fn push(&self, entry: LogEntry) {
        let mut guard = self.inner.lock().unwrap();
        if guard.len() >= self.capacity {
            guard.pop_front();
        }
        guard.push_back(entry);
    }
}

/// Tracing subscriber layer that intercepts WARN and ERROR events.
pub struct ErrorBufferLayer {
    buffer: ErrorBuffer,
}

impl<S: Subscriber> Layer<S> for ErrorBufferLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        use tracing::Level;

        let level = *event.metadata().level();
        if level != Level::WARN && level != Level::ERROR {
            return;
        }

        // Extract the message from the event's fields
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let entry = LogEntry {
            timestamp: SystemTime::now(),
            level: level.to_string(),
            message: visitor.message,
            target: event.metadata().target().to_string(),
        };

        self.buffer.push(entry);
    }
}

/// Visitor that extracts the `message` field from a tracing event.
#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}
```

- [ ] **Step 5: Create the test module registration**

Create `src/tests/dashboard/mod.rs`:

```rust
mod error_buffer;
```

Add to `src/tests/mod.rs`:

```rust
mod dashboard;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib test_error_buffer 2>&1 | tail -10`
Expected: 2 tests pass

- [ ] **Step 7: Commit**

```bash
git add src/dashboard/ src/tests/dashboard/ src/lib.rs
git commit -m "feat(dashboard): add error ring buffer tracing layer"
```

---

## Task 2: DashboardState and SSE Broadcast

Shared state for the dashboard and the broadcast channel for SSE events.

**Files:**
- Create: `src/dashboard/state.rs`
- Modify: `src/dashboard/mod.rs`

- [ ] **Step 1: Write the test**

Create `src/tests/dashboard/state.rs`:

```rust
use julie::dashboard::state::{DashboardEvent, DashboardState};
use julie::daemon::session::SessionTracker;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

#[test]
fn test_dashboard_state_creation() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));
    let state = DashboardState::new(
        sessions,
        None, // daemon_db
        restart_pending,
        Instant::now(),
        50, // error buffer capacity
    );

    assert_eq!(state.sessions().active_count(), 0);
    assert!(!state.is_restart_pending());
    assert!(state.error_entries().is_empty());
}

#[tokio::test]
async fn test_dashboard_broadcast_send_receive() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));
    let state = DashboardState::new(
        sessions,
        None,
        restart_pending,
        Instant::now(),
        50,
    );

    let mut rx = state.subscribe();

    state.send_event(DashboardEvent::ToolCall {
        tool_name: "fast_search".to_string(),
        workspace: "julie".to_string(),
        duration_ms: 42.0,
    });

    let event = rx.recv().await.unwrap();
    match event {
        DashboardEvent::ToolCall { tool_name, duration_ms, .. } => {
            assert_eq!(tool_name, "fast_search");
            assert!((duration_ms - 42.0).abs() < f64::EPSILON);
        }
        _ => panic!("unexpected event type"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_dashboard_state_creation 2>&1 | tail -10`
Expected: FAIL (module doesn't exist)

- [ ] **Step 3: Implement DashboardState**

Create `src/dashboard/state.rs`:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::broadcast;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::session::SessionTracker;
use crate::dashboard::error_buffer::{ErrorBuffer, LogEntry};

/// Events broadcast to SSE subscribers.
#[derive(Debug, Clone)]
pub enum DashboardEvent {
    /// A tool call completed.
    ToolCall {
        tool_name: String,
        workspace: String,
        duration_ms: f64,
    },
    /// A session connected or disconnected.
    SessionChange {
        active_count: usize,
    },
    /// An error/warning was logged.
    LogEntry(LogEntry),
}

/// Shared state for the dashboard HTTP routes.
///
/// Holds Arc refs to existing daemon types plus dashboard-specific
/// state (error buffer, broadcast channel). Cloneable via Arc internally;
/// passed to axum via State extractor.
#[derive(Clone)]
pub struct DashboardState {
    sessions: Arc<SessionTracker>,
    daemon_db: Option<Arc<DaemonDatabase>>,
    restart_pending: Arc<AtomicBool>,
    start_time: Instant,
    error_buffer: ErrorBuffer,
    tx: broadcast::Sender<DashboardEvent>,
}

impl DashboardState {
    pub fn new(
        sessions: Arc<SessionTracker>,
        daemon_db: Option<Arc<DaemonDatabase>>,
        restart_pending: Arc<AtomicBool>,
        start_time: Instant,
        error_buffer_capacity: usize,
    ) -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            sessions,
            daemon_db,
            restart_pending,
            start_time,
            error_buffer: ErrorBuffer::new(error_buffer_capacity),
            tx,
        }
    }

    pub fn sessions(&self) -> &SessionTracker {
        &self.sessions
    }

    pub fn daemon_db(&self) -> Option<&Arc<DaemonDatabase>> {
        self.daemon_db.as_ref()
    }

    pub fn is_restart_pending(&self) -> bool {
        self.restart_pending.load(Ordering::Relaxed)
    }

    pub fn uptime(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    pub fn error_buffer(&self) -> &ErrorBuffer {
        &self.error_buffer
    }

    pub fn error_entries(&self) -> Vec<LogEntry> {
        self.error_buffer.recent_entries()
    }

    /// Subscribe to dashboard events (for SSE endpoints).
    pub fn subscribe(&self) -> broadcast::Receiver<DashboardEvent> {
        self.tx.subscribe()
    }

    /// Broadcast an event to all SSE subscribers.
    pub fn send_event(&self, event: DashboardEvent) {
        // Ignore send errors (no subscribers is fine)
        let _ = self.tx.send(event);
    }
}
```

- [ ] **Step 4: Update dashboard mod.rs**

Update `src/dashboard/mod.rs`:

```rust
pub mod error_buffer;
pub mod state;
```

- [ ] **Step 5: Register the test**

Add to `src/tests/dashboard/mod.rs`:

```rust
mod state;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib test_dashboard_state 2>&1 | tail -10`
Expected: 2 tests pass

- [ ] **Step 7: Commit**

```bash
git add src/dashboard/state.rs src/tests/dashboard/state.rs src/dashboard/mod.rs
git commit -m "feat(dashboard): add DashboardState with broadcast channel"
```

---

## Task 3: Dashboard Router and Template Engine

The axum router, Tera template initialization, static file serving, and dev-mode detection. This is the backbone that all view routes plug into.

**Files:**
- Modify: `src/dashboard/mod.rs` (major expansion)
- Create: `src/dashboard/routes/mod.rs`
- Create: `dashboard/templates/base.html`
- Create: `dashboard/static/app.css`
- Create: `dashboard/static/app.js`

- [ ] **Step 1: Write the test**

Create `src/tests/dashboard/router.rs`:

```rust
use axum::http::StatusCode;
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;
use julie::dashboard::{create_router, DashboardConfig};
use julie::dashboard::state::DashboardState;
use julie::daemon::session::SessionTracker;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn test_state() -> DashboardState {
    DashboardState::new(
        Arc::new(SessionTracker::new()),
        None,
        Arc::new(AtomicBool::new(false)),
        Instant::now(),
        50,
    )
}

#[tokio::test]
async fn test_router_serves_landing_page() {
    let state = test_state();
    let config = DashboardConfig::default();
    let app = create_router(state, config);

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_router_serves_static_css() {
    let state = test_state();
    let config = DashboardConfig::default();
    let app = create_router(state, config);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/app.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_router_serves_landing_page 2>&1 | tail -10`
Expected: FAIL (create_router doesn't exist)

- [ ] **Step 3: Create base.html template**

Create `dashboard/templates/base.html`:

```html
<!DOCTYPE html>
<html lang="en" data-theme="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{% block title %}Julie Dashboard{% endblock %}</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/bulma@1.0.2/css/bulma.min.css">
    <link rel="stylesheet" href="/static/app.css">
    <script src="https://unpkg.com/htmx.org@2.0.4"></script>
    <script src="https://unpkg.com/htmx-ext-sse@2.2.2/sse.js"></script>
    <script defer src="https://cdn.jsdelivr.net/npm/alpinejs@3.14.8/dist/cdn.min.js"></script>
</head>
<body>
    <nav class="navbar julie-nav" role="navigation">
        <div class="navbar-brand">
            <a class="navbar-item julie-brand" href="/">
                <strong>Julie</strong>
                <span class="tag is-dark ml-2">v{{ version }}</span>
            </a>
        </div>
        <div class="navbar-menu is-active">
            <div class="navbar-end">
                <a class="navbar-item {% if active_page == 'status' %}is-active{% endif %}" href="/">Status</a>
                <a class="navbar-item {% if active_page == 'projects' %}is-active{% endif %}" href="/projects">Projects</a>
                <a class="navbar-item {% if active_page == 'metrics' %}is-active{% endif %}" href="/metrics">Metrics</a>
                <a class="navbar-item {% if active_page == 'search' %}is-active{% endif %}" href="/search">Search</a>
                <a class="navbar-item" id="theme-toggle" onclick="toggleTheme()" title="Toggle theme">
                    <span id="theme-icon">🌙</span>
                </a>
            </div>
        </div>
    </nav>

    <section class="section">
        <div class="container is-fluid">
            {% block content %}{% endblock %}
        </div>
    </section>

    <script src="/static/app.js"></script>
</body>
</html>
```

- [ ] **Step 4: Create status.html (minimal placeholder for router test)**

Create `dashboard/templates/status.html`:

```html
{% extends "base.html" %}

{% block title %}Status - Julie Dashboard{% endblock %}

{% block content %}
<h1 class="title">System Status</h1>
<p class="subtitle">Dashboard coming soon</p>
{% endblock %}
```

- [ ] **Step 5: Create app.css**

Create `dashboard/static/app.css`:

```css
/* Julie Dashboard - Dark Theme */
:root {
    --julie-bg: #1a1a2e;
    --julie-bg-card: #222244;
    --julie-bg-inset: #191932;
    --julie-border: #2a2a4a;
    --julie-text: #e0e0e0;
    --julie-text-muted: #888;
    --julie-primary: #818cf8;
    --julie-primary-dark: #6366f1;
    --julie-success: #4ade80;
    --julie-warning: #fbbf24;
    --julie-danger: #ef4444;
    --julie-info: #60a5fa;
}

/* Dark theme overrides for Bulma */
[data-theme="dark"] {
    background-color: var(--julie-bg);
    color: var(--julie-text);
}

[data-theme="dark"] .navbar.julie-nav {
    background-color: var(--julie-bg);
    border-bottom: 1px solid var(--julie-border);
}

[data-theme="dark"] .navbar-item {
    color: var(--julie-text-muted);
}

[data-theme="dark"] .navbar-item.is-active {
    color: var(--julie-primary);
    border-bottom: 2px solid var(--julie-primary);
}

[data-theme="dark"] .navbar-item:hover {
    background-color: transparent;
    color: var(--julie-text);
}

.julie-brand strong {
    color: var(--julie-primary);
    font-size: 1.25rem;
}

/* Card styling */
.julie-card {
    background-color: var(--julie-bg-card);
    border-radius: 8px;
    padding: 1rem;
    border: 1px solid var(--julie-border);
}

.julie-card .label-text {
    font-size: 0.75rem;
    color: var(--julie-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
}

.julie-card .value-text {
    font-size: 1.5rem;
    font-weight: 700;
    color: var(--julie-text);
    margin-top: 0.25rem;
}

/* Status badges */
.badge-ready { background-color: #064e3b; color: var(--julie-success); }
.badge-indexing { background-color: #422006; color: var(--julie-warning); }
.badge-error { background-color: #450a0a; color: var(--julie-danger); }

/* Code font */
.mono { font-family: 'SF Mono', 'Fira Code', 'Cascadia Code', monospace; }

/* Error feed entries */
.error-entry {
    padding: 0.5rem;
    background: var(--julie-bg);
    border-radius: 4px;
    font-size: 0.8125rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
    margin-bottom: 0.375rem;
}

.error-entry.level-warn { border-left: 3px solid var(--julie-warning); }
.error-entry.level-error { border-left: 3px solid var(--julie-danger); }

/* Light theme */
[data-theme="light"] {
    --julie-bg: #f5f5f5;
    --julie-bg-card: #ffffff;
    --julie-bg-inset: #f0f0f0;
    --julie-border: #e0e0e0;
    --julie-text: #1a1a1a;
    --julie-text-muted: #666;
}

[data-theme="light"] .navbar.julie-nav {
    background-color: var(--julie-bg-card);
    border-bottom: 1px solid var(--julie-border);
}
```

- [ ] **Step 6: Create app.js**

Create `dashboard/static/app.js`:

```javascript
// Theme toggle with localStorage persistence
function toggleTheme() {
    const html = document.documentElement;
    const current = html.getAttribute('data-theme');
    const next = current === 'dark' ? 'light' : 'dark';
    html.setAttribute('data-theme', next);
    localStorage.setItem('julie-theme', next);
    document.getElementById('theme-icon').textContent = next === 'dark' ? '🌙' : '☀️';
}

// Restore theme on load
(function() {
    const saved = localStorage.getItem('julie-theme');
    if (saved) {
        document.documentElement.setAttribute('data-theme', saved);
        const icon = document.getElementById('theme-icon');
        if (icon) icon.textContent = saved === 'dark' ? '🌙' : '☀️';
    }
})();

// Format uptime duration
function formatUptime(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    if (h > 0) return h + 'h ' + m + 'm';
    return m + 'm';
}
```

- [ ] **Step 7: Implement the dashboard router**

Replace `src/dashboard/mod.rs` with the full router implementation:

```rust
pub mod error_buffer;
pub mod routes;
pub mod state;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;
use tera::Tera;
use tokio::sync::RwLock;

use self::state::DashboardState;

/// Embedded dashboard assets (templates + static files).
/// Only used in release mode; dev mode loads from disk.
#[derive(Embed)]
#[folder = "dashboard/"]
struct DashboardAssets;

/// Configuration for the dashboard server.
#[derive(Clone)]
pub struct DashboardConfig {
    /// Whether to load templates from disk (dev mode) or from embedded assets.
    pub dev_mode: bool,
    /// Path to dashboard/ directory (for dev mode disk loading).
    pub dashboard_dir: PathBuf,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        let dashboard_dir = PathBuf::from("dashboard");
        let dev_mode = dashboard_dir.join("templates").exists();
        Self {
            dev_mode,
            dashboard_dir,
        }
    }
}

/// Shared Tera instance, wrapped in RwLock for dev-mode reloading.
pub type SharedTera = Arc<RwLock<Tera>>;

/// Initialize Tera templates from disk or embedded assets.
fn init_tera(config: &DashboardConfig) -> Tera {
    if config.dev_mode {
        let glob = config
            .dashboard_dir
            .join("templates/**/*.html")
            .to_string_lossy()
            .to_string();
        match Tera::new(&glob) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to load templates from disk: {}", e);
                Tera::default()
            }
        }
    } else {
        let mut tera = Tera::default();
        // Load templates from embedded assets
        for path in DashboardAssets::iter() {
            if path.starts_with("templates/") {
                if let Some(file) = DashboardAssets::get(&path) {
                    let template_name = path.strip_prefix("templates/").unwrap_or(&path);
                    if let Ok(content) = std::str::from_utf8(file.data.as_ref()) {
                        if let Err(e) = tera.add_raw_template(template_name, content) {
                            tracing::error!("Failed to load embedded template {}: {}", path, e);
                        }
                    }
                }
            }
        }
        tera
    }
}

/// Create the dashboard axum Router.
pub fn create_router(state: DashboardState, config: DashboardConfig) -> Router {
    let tera = Arc::new(RwLock::new(init_tera(&config)));

    let app_state = AppState {
        dashboard: state,
        tera,
        config: config.clone(),
    };

    Router::new()
        .route("/", axum::routing::get(routes::status::index))
        .route("/projects", axum::routing::get(routes::projects::index))
        .route(
            "/projects/{id}/detail",
            axum::routing::get(routes::projects::detail),
        )
        .route("/metrics", axum::routing::get(routes::metrics::index))
        .route("/metrics/table", axum::routing::get(routes::metrics::table))
        .route("/search", axum::routing::get(routes::search::index))
        .route("/search", axum::routing::post(routes::search::search))
        .route("/events/status", axum::routing::get(routes::events::status_stream))
        .route("/events/metrics", axum::routing::get(routes::events::metrics_stream))
        .route("/events/activity", axum::routing::get(routes::events::activity_stream))
        .route("/static/{*path}", axum::routing::get(serve_static))
        .with_state(app_state)
}

/// Application state passed to all route handlers.
#[derive(Clone)]
pub struct AppState {
    pub dashboard: DashboardState,
    pub tera: SharedTera,
    pub config: DashboardConfig,
}

/// Render a template with common context variables injected.
pub async fn render_template(
    state: &AppState,
    template_name: &str,
    mut context: tera::Context,
) -> Result<Html<String>, StatusCode> {
    context.insert("version", env!("CARGO_PKG_VERSION"));

    // In dev mode, reload templates from disk on every request
    if state.config.dev_mode {
        let mut tera = state.tera.write().await;
        let glob = state
            .config
            .dashboard_dir
            .join("templates/**/*.html")
            .to_string_lossy()
            .to_string();
        if let Err(e) = tera.full_reload() {
            tracing::warn!("Template reload failed, re-parsing: {}", e);
            *tera = Tera::new(&glob).unwrap_or_default();
        }
    }

    let tera = state.tera.read().await;
    match tera.render(template_name, &context) {
        Ok(html) => Ok(Html(html)),
        Err(e) => {
            tracing::error!("Template render error for {}: {}", template_name, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Serve static files from embedded assets or disk.
async fn serve_static(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    // Dev mode: serve from disk
    if state.config.dev_mode {
        let file_path = state.config.dashboard_dir.join("static").join(&path);
        if let Ok(content) = tokio::fs::read(&file_path).await {
            let content_type = match file_path.extension().and_then(|e| e.to_str()) {
                Some("css") => "text/css",
                Some("js") => "application/javascript",
                Some("svg") => "image/svg+xml",
                Some("png") => "image/png",
                _ => "application/octet-stream",
            };
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, content_type)],
                content,
            )
                .into_response();
        }
    } else {
        // Embedded mode
        let asset_path = format!("static/{}", path);
        if let Some(file) = DashboardAssets::get(&asset_path) {
            let content_type = match path.rsplit('.').next() {
                Some("css") => "text/css",
                Some("js") => "application/javascript",
                Some("svg") => "image/svg+xml",
                Some("png") => "image/png",
                _ => "application/octet-stream",
            };
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, content_type)],
                file.data.to_vec(),
            )
                .into_response();
        }
    }

    StatusCode::NOT_FOUND.into_response()
}
```

- [ ] **Step 8: Create route module stubs**

Create `src/dashboard/routes/mod.rs`:

```rust
pub mod events;
pub mod metrics;
pub mod projects;
pub mod search;
pub mod status;
```

Create stub files for each route module. Each returns a minimal placeholder so the router compiles.

`src/dashboard/routes/status.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;

use crate::dashboard::AppState;
use crate::dashboard::render_template;

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let mut context = tera::Context::new();
    context.insert("active_page", "status");
    render_template(&state, "status.html", context).await
}
```

`src/dashboard/routes/projects.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;

use crate::dashboard::AppState;
use crate::dashboard::render_template;

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let mut context = tera::Context::new();
    context.insert("active_page", "projects");
    render_template(&state, "status.html", context).await
}

pub async fn detail(
    State(_state): State<AppState>,
    axum::extract::Path(_id): axum::extract::Path<String>,
) -> Result<Html<String>, StatusCode> {
    Ok(Html("<p>Detail placeholder</p>".to_string()))
}
```

`src/dashboard/routes/metrics.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;

use crate::dashboard::AppState;
use crate::dashboard::render_template;

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let mut context = tera::Context::new();
    context.insert("active_page", "metrics");
    render_template(&state, "status.html", context).await
}

pub async fn table(State(_state): State<AppState>) -> Result<Html<String>, StatusCode> {
    Ok(Html("<p>Metrics table placeholder</p>".to_string()))
}
```

`src/dashboard/routes/search.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;

use crate::dashboard::AppState;
use crate::dashboard::render_template;

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let mut context = tera::Context::new();
    context.insert("active_page", "search");
    render_template(&state, "status.html", context).await
}

pub async fn search(State(_state): State<AppState>) -> Result<Html<String>, StatusCode> {
    Ok(Html("<p>Search results placeholder</p>".to_string()))
}
```

`src/dashboard/routes/events.rs`:

```rust
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use futures::stream::Stream;
use std::convert::Infallible;

use crate::dashboard::AppState;
use crate::dashboard::state::DashboardEvent;

pub async fn status_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.dashboard.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(DashboardEvent::SessionChange { .. }) | Ok(DashboardEvent::LogEntry(_)) => {
                Some(Ok(Event::default().data("update")))
            }
            _ => None,
        }
    });
    Sse::new(stream)
}

pub async fn metrics_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.dashboard.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(DashboardEvent::ToolCall { .. }) => {
                Some(Ok(Event::default().data("update")))
            }
            _ => None,
        }
    });
    Sse::new(stream)
}

pub async fn activity_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.dashboard.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(DashboardEvent::ToolCall { tool_name, workspace, duration_ms }) => {
                let data = format!(
                    r#"{{"tool":"{}","workspace":"{}","duration_ms":{:.1}}}"#,
                    tool_name, workspace, duration_ms
                );
                Some(Ok(Event::default().event("activity").data(data)))
            }
            _ => None,
        }
    });
    Sse::new(stream)
}
```

Note: The SSE routes use `futures::stream::Stream`, `tokio_stream`, and `axum::response::sse`. Add these to `Cargo.toml`:

```toml
futures = "0.3"
tokio-stream = "0.1"
```

Also update the axum features to include `sse`:

```toml
axum = { version = "0.8", default-features = false, features = ["http1", "tokio", "json"] }
```

(axum 0.8's SSE support is in the core crate, no extra feature needed, but `json` is useful for later route params.)

- [ ] **Step 9: Register test and run**

Add to `src/tests/dashboard/mod.rs`:

```rust
mod router;
```

Run: `cargo test --lib test_router_serves 2>&1 | tail -10`
Expected: 2 tests pass

- [ ] **Step 10: Commit**

```bash
git add src/dashboard/ dashboard/ Cargo.toml src/tests/dashboard/router.rs
git commit -m "feat(dashboard): add router, template engine, static serving, and route stubs"
```

---

## Task 4: Wire HTTP Server into Daemon

Connect the dashboard to `run_daemon` so the HTTP server starts alongside the IPC listener.

**Files:**
- Modify: `src/daemon/mod.rs`
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Modify: `src/paths.rs`

- [ ] **Step 1: Add daemon_port() to DaemonPaths**

In `src/paths.rs`, add after the `daemon_db()` method (line ~123):

```rust
    /// Path to the file storing the dashboard HTTP port.
    pub fn daemon_port(&self) -> PathBuf {
        self.julie_home.join("daemon.port")
    }
```

- [ ] **Step 2: Add --no-dashboard flag and Dashboard command to CLI**

In `src/cli.rs`, update the `Daemon` variant and add `Dashboard`:

```rust
    /// Run as persistent daemon (HTTP + IPC transport)
    Daemon {
        /// HTTP port for dashboard (default: 7890, fallback to auto if taken)
        #[arg(long, default_value = "7890")]
        port: u16,
        /// Disable auto-opening dashboard in browser
        #[arg(long)]
        no_dashboard: bool,
    },
    /// Open the dashboard in the default browser
    Dashboard,
```

- [ ] **Step 3: Handle Dashboard command in main.rs**

In `src/main.rs`, add the `Dashboard` match arm after `Restart`:

```rust
        Some(Command::Dashboard) => {
            let paths = julie::paths::DaemonPaths::new();
            let port_file = paths.daemon_port();
            match std::fs::read_to_string(&port_file) {
                Ok(port) => {
                    let url = format!("http://localhost:{}", port.trim());
                    println!("Opening {}", url);
                    opener::open(&url)?;
                }
                Err(_) => {
                    eprintln!("Dashboard not available. Is the daemon running?");
                    std::process::exit(1);
                }
            }
        }
```

Add `opener = "0.7"` to `Cargo.toml` for cross-platform browser opening.

Also update the `Daemon` match arm to pass both `port` and `no_dashboard`:

```rust
        Some(Command::Daemon { port, no_dashboard }) => {
            // ... existing logging setup ...
            info!("Starting Julie daemon v{}", env!("CARGO_PKG_VERSION"));
            julie::daemon::run_daemon(paths, port, no_dashboard).await?;
        }
```

- [ ] **Step 4: Add HTTP server to run_daemon**

In `src/daemon/mod.rs`, update the function signature:

```rust
pub async fn run_daemon(paths: DaemonPaths, port: u16, no_dashboard: bool) -> Result<()> {
```

After the `let restart_notify = Arc::new(Notify::new());` line (around line 301), add the HTTP server startup:

```rust
    // Start the dashboard HTTP server
    let dashboard_state = crate::dashboard::state::DashboardState::new(
        Arc::clone(&sessions),
        daemon_db.clone(),
        Arc::clone(&restart_pending),
        std::time::Instant::now(),
        50,
    );

    let dashboard_config = crate::dashboard::DashboardConfig::default();
    let dashboard_router = crate::dashboard::create_router(
        dashboard_state.clone(),
        dashboard_config,
    );

    // Try the requested port, fall back to auto-assign
    let http_listener = match tokio::net::TcpListener::bind(
        format!("127.0.0.1:{}", port),
    ).await {
        Ok(l) => l,
        Err(_) if port != 0 => {
            warn!("Port {} in use, falling back to auto-assign", port);
            tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .context("Failed to bind HTTP server on any port")?
        }
        Err(e) => return Err(anyhow::anyhow!("Failed to bind HTTP server: {}", e)),
    };

    let actual_port = http_listener.local_addr()?.port();

    // Write port file so `julie dashboard` can find it
    let port_file = paths.daemon_port();
    std::fs::write(&port_file, actual_port.to_string())
        .context("Failed to write daemon port file")?;

    let dashboard_url = format!("http://localhost:{}", actual_port);
    info!(port = actual_port, url = %dashboard_url, "Dashboard HTTP server started");

    // Auto-open browser
    if !no_dashboard {
        if let Err(e) = opener::open(&dashboard_url) {
            warn!("Failed to open browser: {}", e);
        }
    }

    // Spawn the HTTP server as a background task
    tokio::spawn(async move {
        if let Err(e) = axum::serve(http_listener, dashboard_router).await {
            tracing::error!("Dashboard HTTP server error: {}", e);
        }
    });
```

- [ ] **Step 5: Clean up port file on shutdown**

In the shutdown section of `run_daemon` (after `listener.cleanup()`), add:

```rust
    // Clean up the port file
    let _ = std::fs::remove_file(paths.daemon_port());
```

- [ ] **Step 6: Update the imports in daemon/mod.rs**

At the top of `src/daemon/mod.rs`, the existing imports don't need changes since we're using fully qualified paths (`crate::dashboard::...`). But add `opener` to `Cargo.toml`:

```toml
# Browser opening (dashboard)
opener = "0.7"
```

- [ ] **Step 7: Build and verify**

Run: `cargo build 2>&1 | tail -10`
Expected: compiles successfully

- [ ] **Step 8: Commit**

```bash
git add src/daemon/mod.rs src/cli.rs src/main.rs src/paths.rs Cargo.toml
git commit -m "feat(dashboard): wire HTTP server into daemon startup"
```

---

## Task 5: System Status View (Full Implementation)

The landing page with health cards, services panel, and error feed.

**Files:**
- Modify: `dashboard/templates/status.html`
- Create: `dashboard/templates/partials/status_cards.html`
- Create: `dashboard/templates/partials/services_panel.html`
- Create: `dashboard/templates/partials/error_feed.html`
- Modify: `src/dashboard/routes/status.rs`

- [ ] **Step 1: Implement the status route handler**

Replace `src/dashboard/routes/status.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;

use crate::dashboard::AppState;
use crate::dashboard::render_template;

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let uptime = state.dashboard.uptime();
    let uptime_secs = uptime.as_secs();
    let hours = uptime_secs / 3600;
    let minutes = (uptime_secs % 3600) / 60;
    let uptime_str = if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    };

    let active_sessions = state.dashboard.sessions().active_count();
    let restart_pending = state.dashboard.is_restart_pending();
    let errors = state.dashboard.error_entries();

    // Workspace count from daemon_db
    let workspace_count = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .map(|ws| ws.len())
        .unwrap_or(0);

    // Embedding service status (check if available)
    let embedding_available = state.dashboard.embedding_available();

    let mut context = tera::Context::new();
    context.insert("active_page", "status");
    context.insert("uptime", &uptime_str);
    context.insert("active_sessions", &active_sessions);
    context.insert("workspace_count", &workspace_count);
    context.insert("restart_pending", &restart_pending);
    context.insert("embedding_available", &embedding_available);
    context.insert("errors", &errors);

    render_template(&state, "status.html", context).await
}
```

Note: This requires adding `embedding_available()` to DashboardState and making `LogEntry` implement `Serialize`. Update `src/dashboard/state.rs` to store an `embedding_available: bool` field (set at construction time from `EmbeddingService::is_available()`). Update `LogEntry` in `error_buffer.rs` to derive `serde::Serialize`.

Add to `DashboardState::new()` parameters: `embedding_available: bool`. Add the method:

```rust
    pub fn embedding_available(&self) -> bool {
        self.embedding_available
    }
```

Update `LogEntry`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogEntry {
    #[serde(serialize_with = "serialize_system_time")]
    pub timestamp: SystemTime,
    pub level: String,
    pub message: String,
    pub target: String,
}

fn serialize_system_time<S: serde::Serializer>(
    time: &SystemTime,
    s: S,
) -> Result<S::Ok, S::Error> {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    s.serialize_u64(duration.as_secs())
}
```

Add `serde = { version = "1", features = ["derive"] }` if not already in Cargo.toml (it should be).

- [ ] **Step 2: Create the status.html template**

Replace `dashboard/templates/status.html`:

```html
{% extends "base.html" %}

{% block title %}Status - Julie Dashboard{% endblock %}

{% block content %}
<div hx-ext="sse" sse-connect="/events/status">
    <!-- Hero stats -->
    <div class="columns is-multiline mb-5">
        <div class="column is-3">
            <div class="julie-card">
                <div class="label-text">Status</div>
                <div class="value-text" style="color: var(--julie-success);">● Healthy</div>
            </div>
        </div>
        <div class="column is-3">
            <div class="julie-card">
                <div class="label-text">Uptime</div>
                <div class="value-text">{{ uptime }}</div>
            </div>
        </div>
        <div class="column is-3">
            <div class="julie-card">
                <div class="label-text">Active Sessions</div>
                <div class="value-text" sse-swap="sessions">{{ active_sessions }}</div>
            </div>
        </div>
        <div class="column is-3">
            <div class="julie-card">
                <div class="label-text">Workspaces</div>
                <div class="value-text">{{ workspace_count }}</div>
            </div>
        </div>
    </div>

    <!-- Services + Errors -->
    <div class="columns">
        <div class="column is-6">
            <div class="julie-card">
                <h3 class="subtitle is-6 mb-3" style="color: var(--julie-text);">Services</h3>
                {% include "partials/services_panel.html" %}
            </div>
        </div>
        <div class="column is-6">
            <div class="julie-card">
                <div class="is-flex is-justify-content-space-between is-align-items-center mb-3">
                    <h3 class="subtitle is-6 mb-0" style="color: var(--julie-text);">Recent Errors & Warnings</h3>
                    <span class="is-size-7" style="color: var(--julie-text-muted);">Last 50 entries</span>
                </div>
                <div id="error-feed" sse-swap="error" hx-swap="innerHTML">
                    {% include "partials/error_feed.html" %}
                </div>
            </div>
        </div>
    </div>
</div>
{% endblock %}
```

- [ ] **Step 3: Create partials**

`dashboard/templates/partials/services_panel.html`:

```html
<table class="table is-fullwidth" style="background: transparent; color: var(--julie-text);">
    <tbody>
        <tr>
            <td>Search Engine (Tantivy)</td>
            <td class="has-text-right"><span style="color: var(--julie-success);">● Running</span></td>
        </tr>
        <tr>
            <td>Embedding Service</td>
            <td class="has-text-right">
                {% if embedding_available %}
                <span style="color: var(--julie-success);">● Available</span>
                {% else %}
                <span style="color: var(--julie-text-muted);">● Not configured</span>
                {% endif %}
            </td>
        </tr>
        <tr>
            <td>Binary Status</td>
            <td class="has-text-right">
                {% if restart_pending %}
                <span style="color: var(--julie-warning);">⚠ Restart Pending</span>
                {% else %}
                <span style="color: var(--julie-success);">● Current</span>
                {% endif %}
            </td>
        </tr>
    </tbody>
</table>
```

`dashboard/templates/partials/error_feed.html`:

```html
{% if errors | length == 0 %}
<p class="has-text-centered" style="color: var(--julie-text-muted); padding: 2rem 0;">
    No recent errors or warnings
</p>
{% else %}
{% for error in errors | reverse %}
<div class="error-entry level-{{ error.level | lower }}">
    <div class="is-flex is-justify-content-space-between">
        <span style="color: {% if error.level == 'ERROR' %}var(--julie-danger){% else %}var(--julie-warning){% endif %};">
            {{ error.level }}
        </span>
        <span style="color: var(--julie-text-muted);">{{ error.target }}</span>
    </div>
    <div style="color: var(--julie-text); margin-top: 0.25rem;">{{ error.message }}</div>
</div>
{% endfor %}
{% endif %}
```

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | tail -10`
Expected: compiles successfully

- [ ] **Step 5: Commit**

```bash
git add dashboard/templates/ src/dashboard/
git commit -m "feat(dashboard): implement system status view with error feed"
```

---

## Task 6: Projects View (Full Implementation)

**Files:**
- Create: `dashboard/templates/projects.html`
- Create: `dashboard/templates/partials/project_row.html`
- Create: `dashboard/templates/partials/project_detail.html`
- Modify: `src/dashboard/routes/projects.rs`

- [ ] **Step 1: Implement the projects route handler**

Replace `src/dashboard/routes/projects.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;

use crate::dashboard::AppState;
use crate::dashboard::render_template;

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let workspaces = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .unwrap_or_default();

    let ready_count = workspaces.iter().filter(|w| w.status == "ready").count();
    let indexing_count = workspaces.iter().filter(|w| w.status == "indexing").count();
    let error_count = workspaces.iter().filter(|w| w.status == "error").count();

    let mut context = tera::Context::new();
    context.insert("active_page", "projects");
    context.insert("workspaces", &workspaces);
    context.insert("total_count", &workspaces.len());
    context.insert("ready_count", &ready_count);
    context.insert("indexing_count", &indexing_count);
    context.insert("error_count", &error_count);

    render_template(&state, "projects.html", context).await
}

pub async fn detail(
    State(state): State<AppState>,
    axum::extract::Path(workspace_id): axum::extract::Path<String>,
) -> Result<Html<String>, StatusCode> {
    let db = state.dashboard.daemon_db().ok_or(StatusCode::NOT_FOUND)?;

    let workspace = db
        .get_workspace(&workspace_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let references = db
        .list_references(&workspace_id)
        .unwrap_or_default();

    let health = db
        .get_latest_snapshot(&workspace_id)
        .ok()
        .flatten();

    let mut context = tera::Context::new();
    context.insert("workspace", &workspace);
    context.insert("references", &references);
    context.insert("health", &health);

    render_template(&state, "partials/project_detail.html", context).await
}
```

Note: This requires `WorkspaceRow` and `CodehealthSnapshotRow` to derive `serde::Serialize`. Add `#[derive(serde::Serialize)]` to both structs in `src/daemon/database.rs`.

- [ ] **Step 2: Create projects.html template**

Create `dashboard/templates/projects.html`:

```html
{% extends "base.html" %}

{% block title %}Projects - Julie Dashboard{% endblock %}

{% block content %}
<div class="is-flex is-justify-content-space-between is-align-items-center mb-4">
    <div class="is-flex" style="gap: 1rem; font-size: 0.875rem; color: var(--julie-text-muted);">
        <span>{{ total_count }} workspaces</span>
        <span>·</span>
        <span style="color: var(--julie-success);">{{ ready_count }} ready</span>
        {% if indexing_count > 0 %}
        <span>·</span>
        <span style="color: var(--julie-warning);">{{ indexing_count }} indexing</span>
        {% endif %}
        {% if error_count > 0 %}
        <span>·</span>
        <span style="color: var(--julie-danger);">{{ error_count }} error</span>
        {% endif %}
    </div>
</div>

<div class="table-container">
    <table class="table is-fullwidth is-hoverable" style="background: transparent; color: var(--julie-text);">
        <thead>
            <tr style="color: var(--julie-text-muted); font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.5px;">
                <th style="width: 28px;"></th>
                <th>Project</th>
                <th>Path</th>
                <th class="has-text-right">Symbols</th>
                <th class="has-text-right">Files</th>
                <th class="has-text-right">Vectors</th>
                <th class="has-text-centered">Status</th>
            </tr>
        </thead>
        <tbody>
            {% for ws in workspaces %}
            {% include "partials/project_row.html" %}
            {% endfor %}
        </tbody>
    </table>
</div>
{% endblock %}
```

- [ ] **Step 3: Create project_row.html partial**

Create `dashboard/templates/partials/project_row.html`:

```html
<tr x-data="{ expanded: false }" style="cursor: pointer;">
    <td @click="expanded = !expanded">
        <span x-text="expanded ? '▼' : '▶'" style="color: var(--julie-primary);"></span>
    </td>
    <td @click="expanded = !expanded"><strong>{{ ws.workspace_id }}</strong></td>
    <td @click="expanded = !expanded"><span class="mono is-size-7" style="color: var(--julie-text-muted);">{{ ws.path }}</span></td>
    <td class="has-text-right" @click="expanded = !expanded">{{ ws.symbol_count | default(value="—") }}</td>
    <td class="has-text-right" @click="expanded = !expanded">{{ ws.file_count | default(value="—") }}</td>
    <td class="has-text-right" @click="expanded = !expanded">{{ ws.vector_count | default(value="—") }}</td>
    <td class="has-text-centered" @click="expanded = !expanded">
        {% if ws.status == "ready" %}
        <span class="tag badge-ready">Ready</span>
        {% elif ws.status == "indexing" %}
        <span class="tag badge-indexing">Indexing</span>
        {% elif ws.status == "error" %}
        <span class="tag badge-error">Error</span>
        {% else %}
        <span class="tag">{{ ws.status }}</span>
        {% endif %}
    </td>
</tr>
<tr x-show="expanded" x-cloak>
    <td colspan="7" style="padding: 0; border: none;">
        <div
            hx-get="/projects/{{ ws.workspace_id }}/detail"
            hx-trigger="intersect once"
            hx-swap="innerHTML"
            style="background: var(--julie-bg-inset); padding: 1rem 1rem 1rem 2.75rem;"
        >
            <p style="color: var(--julie-text-muted);">Loading details...</p>
        </div>
    </td>
</tr>
```

- [ ] **Step 4: Create project_detail.html partial**

Create `dashboard/templates/partials/project_detail.html`:

```html
<div class="columns">
    <div class="column is-4">
        <h4 class="is-size-7 has-text-weight-semibold mb-2" style="color: var(--julie-text-muted); text-transform: uppercase;">Index Stats</h4>
        <table class="table is-narrow is-fullwidth" style="background: transparent; color: var(--julie-text); font-size: 0.8125rem;">
            <tr>
                <td style="color: var(--julie-text-muted);">DB Size</td>
                <td class="has-text-right">—</td>
            </tr>
            <tr>
                <td style="color: var(--julie-text-muted);">Embedding Model</td>
                <td class="has-text-right">{{ workspace.embedding_model | default(value="none") }}</td>
            </tr>
            <tr>
                <td style="color: var(--julie-text-muted);">Last Indexed</td>
                <td class="has-text-right">{{ workspace.last_indexed | default(value="never") }}</td>
            </tr>
        </table>
    </div>

    {% if health %}
    <div class="column is-4">
        <h4 class="is-size-7 has-text-weight-semibold mb-2" style="color: var(--julie-text-muted); text-transform: uppercase;">Code Health</h4>
        <table class="table is-narrow is-fullwidth" style="background: transparent; color: var(--julie-text); font-size: 0.8125rem;">
            <tr>
                <td style="color: var(--julie-text-muted);">Security Risk</td>
                <td class="has-text-right">
                    {% if health.security_high > 0 %}
                    <span style="color: var(--julie-danger);">High ({{ health.security_high }})</span>
                    {% elif health.security_medium > 0 %}
                    <span style="color: var(--julie-warning);">Medium ({{ health.security_medium }})</span>
                    {% else %}
                    <span style="color: var(--julie-success);">Low</span>
                    {% endif %}
                </td>
            </tr>
            <tr>
                <td style="color: var(--julie-text-muted);">Change Risk</td>
                <td class="has-text-right">
                    {% if health.change_high > 0 %}
                    <span style="color: var(--julie-danger);">High ({{ health.change_high }})</span>
                    {% elif health.change_medium > 0 %}
                    <span style="color: var(--julie-warning);">Medium ({{ health.change_medium }})</span>
                    {% else %}
                    <span style="color: var(--julie-success);">Low</span>
                    {% endif %}
                </td>
            </tr>
            <tr>
                <td style="color: var(--julie-text-muted);">Test Coverage</td>
                <td class="has-text-right">
                    {% if health.symbols_tested + health.symbols_untested > 0 %}
                    {{ (health.symbols_tested * 100 / (health.symbols_tested + health.symbols_untested)) }}%
                    {% else %}
                    —
                    {% endif %}
                </td>
            </tr>
        </table>
    </div>
    {% endif %}

    <div class="column is-4">
        <h4 class="is-size-7 has-text-weight-semibold mb-2" style="color: var(--julie-text-muted); text-transform: uppercase;">References</h4>
        {% if references | length > 0 %}
        <div class="tags">
            {% for ref in references %}
            <span class="tag" style="background: var(--julie-border); color: var(--julie-primary);">{{ ref.workspace_id }}</span>
            {% endfor %}
        </div>
        {% else %}
        <p class="is-size-7" style="color: var(--julie-text-muted);">No reference workspaces</p>
        {% endif %}
    </div>
</div>
```

- [ ] **Step 5: Build and verify**

Run: `cargo build 2>&1 | tail -10`
Expected: compiles successfully

- [ ] **Step 6: Commit**

```bash
git add dashboard/templates/ src/dashboard/routes/projects.rs src/daemon/database.rs
git commit -m "feat(dashboard): implement projects view with expandable details"
```

---

## Task 7: Metrics View (Full Implementation)

**Files:**
- Create: `dashboard/templates/metrics.html`
- Create: `dashboard/templates/partials/metrics_summary.html`
- Create: `dashboard/templates/partials/metrics_table.html`
- Create: `dashboard/templates/partials/activity_event.html`
- Modify: `src/dashboard/routes/metrics.rs`

- [ ] **Step 1: Implement the metrics route handler**

Replace `src/dashboard/routes/metrics.rs`:

```rust
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;

use crate::dashboard::AppState;
use crate::dashboard::render_template;

#[derive(Deserialize)]
pub struct MetricsParams {
    #[serde(default = "default_days")]
    pub days: u32,
    pub workspace: Option<String>,
}

fn default_days() -> u32 {
    7
}

pub async fn index(
    State(state): State<AppState>,
    Query(params): Query<MetricsParams>,
) -> Result<Html<String>, StatusCode> {
    let db = match state.dashboard.daemon_db() {
        Some(db) => db,
        None => {
            let mut context = tera::Context::new();
            context.insert("active_page", "metrics");
            context.insert("no_data", &true);
            return render_template(&state, "metrics.html", context).await;
        }
    };

    let workspaces = db.list_workspaces().unwrap_or_default();

    // Get history for selected workspace or aggregate across all
    let workspace_id = params.workspace.as_deref().unwrap_or("");
    let history = if workspace_id.is_empty() {
        // Aggregate across all workspaces
        let mut total = crate::database::HistorySummary::default();
        for ws in &workspaces {
            if let Ok(h) = db.query_tool_call_history(&ws.workspace_id, params.days) {
                total.session_count += h.session_count;
                total.total_calls += h.total_calls;
                total.total_source_bytes += h.total_source_bytes;
                total.total_output_bytes += h.total_output_bytes;
                for tool_summary in h.per_tool {
                    if let Some(existing) = total.per_tool.iter_mut().find(|t| t.tool_name == tool_summary.tool_name) {
                        existing.call_count += tool_summary.call_count;
                        // Weighted average for duration
                        existing.avg_duration_ms = (existing.avg_duration_ms * (existing.call_count - tool_summary.call_count) as f64
                            + tool_summary.avg_duration_ms * tool_summary.call_count as f64)
                            / existing.call_count as f64;
                    } else {
                        total.per_tool.push(tool_summary);
                    }
                }
                for (tool, durations) in h.durations_by_tool {
                    total.durations_by_tool.entry(tool).or_default().extend(durations);
                }
            }
        }
        total
    } else {
        db.query_tool_call_history(workspace_id, params.days)
            .unwrap_or_default()
    };

    // Sort tools by call count descending
    let mut tools = history.per_tool.clone();
    tools.sort_by(|a, b| b.call_count.cmp(&a.call_count));
    let max_calls = tools.first().map(|t| t.call_count).unwrap_or(1);

    // Compute p95 per tool
    let mut p95_by_tool: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for (tool_name, durations) in &history.durations_by_tool {
        let mut sorted = durations.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if !sorted.is_empty() {
            let idx = (sorted.len() as f64 * 0.95) as usize;
            let idx = idx.min(sorted.len() - 1);
            p95_by_tool.insert(tool_name.clone(), sorted[idx]);
        }
    }

    let mut context = tera::Context::new();
    context.insert("active_page", "metrics");
    context.insert("no_data", &false);
    context.insert("days", &params.days);
    context.insert("selected_workspace", &workspace_id);
    context.insert("workspaces", &workspaces);
    context.insert("total_calls", &history.total_calls);
    context.insert("session_count", &history.session_count);
    context.insert("tools", &tools);
    context.insert("max_calls", &max_calls);
    context.insert("p95_by_tool", &p95_by_tool);

    render_template(&state, "metrics.html", context).await
}

pub async fn table(
    State(state): State<AppState>,
    Query(params): Query<MetricsParams>,
) -> Result<Html<String>, StatusCode> {
    // Same data fetching as index, but render only the table partial
    index(State(state), Query(params)).await
}
```

Note: `HistorySummary` needs `Default` derive and `Serialize`. Add both to the struct in `src/database/tool_calls.rs`. Also add `Serialize` to `ToolCallSummary`.

- [ ] **Step 2: Create metrics.html template**

Create `dashboard/templates/metrics.html`:

```html
{% extends "base.html" %}

{% block title %}Metrics - Julie Dashboard{% endblock %}

{% block content %}
{% if no_data %}
<div class="has-text-centered" style="padding: 4rem;">
    <p class="subtitle" style="color: var(--julie-text-muted);">No metrics data available. Daemon database not connected.</p>
</div>
{% else %}

<!-- Filter bar -->
<div class="is-flex mb-4" style="gap: 0.75rem; align-items: center;">
    <div class="select is-small">
        <select hx-get="/metrics" hx-target="#metrics-body" hx-swap="innerHTML" name="workspace"
                hx-include="[name='days']">
            <option value="" {% if selected_workspace == "" %}selected{% endif %}>All Workspaces</option>
            {% for ws in workspaces %}
            <option value="{{ ws.workspace_id }}" {% if selected_workspace == ws.workspace_id %}selected{% endif %}>
                {{ ws.workspace_id }}
            </option>
            {% endfor %}
        </select>
    </div>
    <div class="buttons has-addons are-small">
        {% for d in [1, 7, 30, 90] %}
        <button class="button {% if days == d %}is-primary{% endif %}"
                hx-get="/metrics?days={{ d }}&workspace={{ selected_workspace }}"
                hx-target="body" hx-swap="outerHTML">
            {{ d }}d
        </button>
        {% endfor %}
    </div>
    <span class="is-flex-grow-1"></span>
    <span class="is-size-7" style="color: var(--julie-text-muted);" hx-ext="sse" sse-connect="/events/activity">
        Live
        <span style="display: inline-block; width: 8px; height: 8px; background: var(--julie-success); border-radius: 50%;"></span>
    </span>
</div>

<div id="metrics-body">
    <!-- Summary cards -->
    <div class="columns mb-5">
        <div class="column is-3">
            <div class="julie-card">
                <div class="label-text">Total Tool Calls</div>
                <div class="value-text">{{ total_calls }}</div>
            </div>
        </div>
        <div class="column is-3">
            <div class="julie-card">
                <div class="label-text">Sessions</div>
                <div class="value-text">{{ session_count }}</div>
            </div>
        </div>
        <div class="column is-3">
            <div class="julie-card">
                <div class="label-text">Tools Active</div>
                <div class="value-text">{{ tools | length }}</div>
            </div>
        </div>
        <div class="column is-3">
            <div class="julie-card">
                <div class="label-text">Period</div>
                <div class="value-text">{{ days }}d</div>
            </div>
        </div>
    </div>

    <!-- Tool breakdown + Live activity -->
    <div class="columns">
        <div class="column is-7">
            <div class="julie-card">
                <h3 class="subtitle is-6 mb-3" style="color: var(--julie-text);">Tool Breakdown</h3>
                {% include "partials/metrics_table.html" %}
            </div>
        </div>
        <div class="column is-5">
            <div class="julie-card" hx-ext="sse" sse-connect="/events/activity">
                <h3 class="subtitle is-6 mb-3" style="color: var(--julie-text);">Live Activity</h3>
                <div id="activity-feed" sse-swap="activity" hx-swap="afterbegin"
                     style="max-height: 400px; overflow-y: auto; font-family: 'SF Mono', monospace; font-size: 0.8125rem;">
                    <p style="color: var(--julie-text-muted); text-align: center; padding: 2rem;">
                        Waiting for tool calls...
                    </p>
                </div>
            </div>
        </div>
    </div>
</div>
{% endif %}
{% endblock %}
```

- [ ] **Step 3: Create metrics_table.html partial**

Create `dashboard/templates/partials/metrics_table.html`:

```html
<table class="table is-fullwidth is-narrow" style="background: transparent; color: var(--julie-text); font-size: 0.8125rem;">
    <thead>
        <tr style="color: var(--julie-text-muted); font-size: 0.6875rem; text-transform: uppercase; letter-spacing: 0.5px;">
            <th>Tool</th>
            <th class="has-text-right">Calls</th>
            <th class="has-text-right">Avg</th>
            <th class="has-text-right">p95</th>
            <th style="width: 100px;"></th>
        </tr>
    </thead>
    <tbody>
        {% for tool in tools %}
        <tr>
            <td>{{ tool.tool_name }}</td>
            <td class="has-text-right">{{ tool.call_count }}</td>
            <td class="has-text-right">{{ tool.avg_duration_ms | round(precision=0) }}ms</td>
            <td class="has-text-right">
                {% if p95_by_tool[tool.tool_name] %}
                {{ p95_by_tool[tool.tool_name] | round(precision=0) }}ms
                {% else %}
                —
                {% endif %}
            </td>
            <td>
                <div style="width: 100%; height: 6px; background: var(--julie-border); border-radius: 3px; overflow: hidden;">
                    <div style="width: {{ tool.call_count * 100 / max_calls }}%; height: 100%; background: var(--julie-primary); border-radius: 3px;"></div>
                </div>
            </td>
        </tr>
        {% endfor %}
    </tbody>
</table>
```

- [ ] **Step 4: Create activity_event.html partial**

Create `dashboard/templates/partials/activity_event.html`. This is served as the SSE event data:

```html
<div class="is-flex" style="gap: 0.75rem; padding: 0.375rem 0.5rem; background: var(--julie-bg); border-radius: 4px; margin-bottom: 0.25rem;">
    <span style="color: var(--julie-text-muted); min-width: 55px;">{{ time }}</span>
    <span style="color: var(--julie-primary); min-width: 85px;">{{ tool }}</span>
    <span style="color: var(--julie-text-muted);">{{ workspace }}</span>
    <span style="margin-left: auto; color: {% if duration_ms < 100 %}var(--julie-success){% elif duration_ms < 500 %}var(--julie-warning){% else %}var(--julie-danger){% endif %};">
        {{ duration_ms | round(precision=0) }}ms
    </span>
</div>
```

- [ ] **Step 5: Build and verify**

Run: `cargo build 2>&1 | tail -10`
Expected: compiles successfully

- [ ] **Step 6: Commit**

```bash
git add dashboard/templates/ src/dashboard/routes/metrics.rs src/database/tool_calls.rs
git commit -m "feat(dashboard): implement metrics view with tool breakdown and live feed"
```

---

## Task 8: Search Playground View (Full Implementation)

**Files:**
- Create: `dashboard/templates/search.html`
- Create: `dashboard/templates/partials/search_results.html`
- Create: `dashboard/templates/partials/search_detail.html`
- Modify: `src/dashboard/routes/search.rs`

- [ ] **Step 1: Implement the search route handler**

Replace `src/dashboard/routes/search.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use axum::Form;
use serde::Deserialize;

use crate::dashboard::AppState;
use crate::dashboard::render_template;

#[derive(Deserialize)]
pub struct SearchParams {
    pub query: Option<String>,
    pub workspace: Option<String>,
    pub search_target: Option<String>,
    pub language: Option<String>,
    pub file_pattern: Option<String>,
    pub debug: Option<bool>,
    pub limit: Option<u32>,
}

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let workspaces = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .unwrap_or_default();

    let mut context = tera::Context::new();
    context.insert("active_page", "search");
    context.insert("workspaces", &workspaces);
    context.insert("results", &Vec::<()>::new());
    context.insert("searched", &false);

    render_template(&state, "search.html", context).await
}

pub async fn search(
    State(state): State<AppState>,
    Form(params): Form<SearchParams>,
) -> Result<Html<String>, StatusCode> {
    let query = params.query.unwrap_or_default();
    if query.is_empty() {
        return index(State(state)).await;
    }

    let workspaces = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .unwrap_or_default();

    let workspace_id = params.workspace.unwrap_or_default();
    let search_target = params.search_target.unwrap_or_else(|| "definitions".to_string());
    let language = params.language.unwrap_or_default();
    let file_pattern = params.file_pattern.unwrap_or_default();
    let debug = params.debug.unwrap_or(false);
    let limit = params.limit.unwrap_or(20);

    // The actual search integration will call the search pipeline
    // For now, pass the parameters to the template
    let mut context = tera::Context::new();
    context.insert("active_page", "search");
    context.insert("workspaces", &workspaces);
    context.insert("searched", &true);
    context.insert("query", &query);
    context.insert("selected_workspace", &workspace_id);
    context.insert("search_target", &search_target);
    context.insert("language", &language);
    context.insert("file_pattern", &file_pattern);
    context.insert("debug", &debug);
    context.insert("limit", &limit);

    // TODO: Wire up actual search pipeline call here.
    // This requires access to the workspace's JulieWorkspace (via WorkspacePool)
    // to call the search index. The route handler will need WorkspacePool
    // added to DashboardState. For now, render with empty results and
    // integrate the search pipeline in a follow-up step.
    context.insert("results", &Vec::<()>::new());

    render_template(&state, "search.html", context).await
}
```

Note: The search pipeline integration requires `WorkspacePool` in `DashboardState`. Add it:

In `src/dashboard/state.rs`, add to the struct:

```rust
    workspace_pool: Option<Arc<crate::daemon::workspace_pool::WorkspacePool>>,
```

Add to `new()` parameters and initialization. Add accessor:

```rust
    pub fn workspace_pool(&self) -> Option<&Arc<crate::daemon::workspace_pool::WorkspacePool>> {
        self.workspace_pool.as_ref()
    }
```

Update the `DashboardState::new()` call in `src/daemon/mod.rs` to pass `Some(Arc::clone(&pool))`.

The actual search pipeline integration (calling `JulieWorkspace::search()`) is a follow-up step within this task, not deferred to another task.

- [ ] **Step 2: Create search.html template**

Create `dashboard/templates/search.html`:

```html
{% extends "base.html" %}

{% block title %}Search - Julie Dashboard{% endblock %}

{% block content %}
<form hx-post="/search" hx-target="#search-results" hx-swap="innerHTML">
    <!-- Search bar -->
    <div class="field has-addons mb-3">
        <div class="control is-expanded">
            <input class="input" type="text" name="query" placeholder="Search symbols..."
                   value="{% if query %}{{ query }}{% endif %}"
                   style="background: var(--julie-bg-card); color: var(--julie-text); border-color: var(--julie-border);">
        </div>
        <div class="control">
            <button class="button is-primary" type="submit">Search</button>
        </div>
    </div>

    <!-- Filters -->
    <div class="is-flex mb-4" style="gap: 0.75rem; align-items: center; flex-wrap: wrap;">
        <div class="select is-small">
            <select name="workspace" style="background: var(--julie-bg-card); color: var(--julie-text); border-color: var(--julie-border);">
                <option value="">All Workspaces</option>
                {% for ws in workspaces %}
                <option value="{{ ws.workspace_id }}" {% if selected_workspace == ws.workspace_id %}selected{% endif %}>
                    {{ ws.workspace_id }}
                </option>
                {% endfor %}
            </select>
        </div>
        <div class="buttons has-addons are-small">
            <button type="button" class="button {% if search_target == 'definitions' or not searched %}is-primary{% endif %}"
                    onclick="document.querySelector('[name=search_target]').value='definitions'">definitions</button>
            <button type="button" class="button {% if search_target == 'content' %}is-primary{% endif %}"
                    onclick="document.querySelector('[name=search_target]').value='content'">content</button>
        </div>
        <input type="hidden" name="search_target" value="{{ search_target | default(value='definitions') }}">
        <div class="select is-small">
            <select name="language" style="background: var(--julie-bg-card); color: var(--julie-text); border-color: var(--julie-border);">
                <option value="">All Languages</option>
                {% for lang in ["rust", "typescript", "javascript", "python", "java", "csharp", "go", "cpp", "c"] %}
                <option value="{{ lang }}" {% if language == lang %}selected{% endif %}>{{ lang }}</option>
                {% endfor %}
            </select>
        </div>
        <input class="input is-small" type="text" name="file_pattern" placeholder="file pattern"
               value="{{ file_pattern | default(value='') }}"
               style="width: 200px; background: var(--julie-bg-card); color: var(--julie-text); border-color: var(--julie-border);">
        <label class="checkbox is-size-7" style="color: var(--julie-text-muted);">
            <input type="checkbox" name="debug" value="true" {% if debug %}checked{% endif %}> Debug Mode
        </label>
    </div>
</form>

<div id="search-results">
    {% if searched %}
    {% include "partials/search_results.html" %}
    {% else %}
    <div class="has-text-centered" style="padding: 4rem; color: var(--julie-text-muted);">
        Enter a query to search the index
    </div>
    {% endif %}
</div>
{% endblock %}
```

- [ ] **Step 3: Create search_results.html partial**

Create `dashboard/templates/partials/search_results.html`:

```html
{% if results | length == 0 %}
<div class="has-text-centered" style="padding: 2rem; color: var(--julie-text-muted);">
    {% if searched %}
    No results found for "{{ query }}"
    {% endif %}
</div>
{% else %}
{% for result in results %}
<div x-data="{ expanded: false }" class="mb-1">
    <div @click="expanded = !expanded" class="is-flex is-align-items-center"
         style="padding: 0.75rem 1rem; background: var(--julie-bg-card); border-radius: {% if loop.first %}8px 8px{% else %}0{% endif %} {% if loop.last and not expanded %}0 0 8px 8px{% else %}0 0{% endif %}; cursor: pointer; gap: 0.75rem;">
        <span x-text="expanded ? '▼' : '▶'" style="color: var(--julie-primary);"></span>
        <span style="color: var(--julie-warning); font-size: 0.75rem; font-weight: 600; min-width: 24px;">{{ loop.index }}</span>
        <span class="tag is-small" style="font-size: 0.6875rem;">{{ result.kind }}</span>
        <span class="mono" style="font-weight: 600;">{{ result.name }}</span>
        <span class="mono is-size-7" style="color: var(--julie-text-muted);">{{ result.file }}:{{ result.line }}</span>
        <span class="is-flex-grow-1"></span>
        {% if debug %}
        <span class="is-size-7" style="color: var(--julie-text-muted);">score: {{ result.score }}</span>
        {% endif %}
    </div>
    <div x-show="expanded" x-cloak style="background: var(--julie-bg-inset); padding: 0.75rem 1rem 0.75rem 3.25rem;">
        {% if debug %}
        {% include "partials/search_detail.html" %}
        {% else %}
        <pre class="mono is-size-7" style="background: var(--julie-bg); padding: 0.5rem; border-radius: 4px; color: var(--julie-text);">{{ result.context }}</pre>
        {% endif %}
    </div>
</div>
{% endfor %}
{% endif %}
```

- [ ] **Step 4: Create search_detail.html partial**

Create `dashboard/templates/partials/search_detail.html`:

```html
<div class="columns is-size-7">
    <div class="column is-4">
        <h4 style="color: var(--julie-text-muted); margin-bottom: 0.5rem;">Scoring</h4>
        <table class="table is-narrow is-fullwidth" style="background: transparent; color: var(--julie-text); font-size: 0.75rem;">
            <tr><td style="color: var(--julie-text-muted);">BM25</td><td class="has-text-right">{{ result.bm25 | default(value="—") }}</td></tr>
            <tr><td style="color: var(--julie-text-muted);">Centrality</td><td class="has-text-right" style="color: var(--julie-success);">{{ result.centrality_boost | default(value="—") }}</td></tr>
            <tr><td style="color: var(--julie-text-muted);">Pattern</td><td class="has-text-right" style="color: var(--julie-success);">{{ result.pattern_boost | default(value="—") }}</td></tr>
            <tr style="border-top: 1px solid var(--julie-border);"><td style="font-weight: 600;">Final</td><td class="has-text-right" style="font-weight: 600;">{{ result.score }}</td></tr>
        </table>
    </div>
    <div class="column is-4">
        <h4 style="color: var(--julie-text-muted); margin-bottom: 0.5rem;">Field Matches</h4>
        <table class="table is-narrow is-fullwidth" style="background: transparent; color: var(--julie-text); font-size: 0.75rem;">
            {% for field in ["name", "qualified_name", "content", "path"] %}
            <tr>
                <td style="color: var(--julie-text-muted);">{{ field }}</td>
                <td class="has-text-right">
                    {% if result.field_matches and field in result.field_matches %}
                    <span style="color: var(--julie-success);">&#10003;</span>
                    {% else %}
                    <span style="color: var(--julie-text-muted);">—</span>
                    {% endif %}
                </td>
            </tr>
            {% endfor %}
        </table>
    </div>
    <div class="column is-4">
        <h4 style="color: var(--julie-text-muted); margin-bottom: 0.5rem;">Symbol Info</h4>
        <table class="table is-narrow is-fullwidth" style="background: transparent; color: var(--julie-text); font-size: 0.75rem;">
            <tr><td style="color: var(--julie-text-muted);">Kind</td><td class="has-text-right">{{ result.kind }}</td></tr>
            <tr><td style="color: var(--julie-text-muted);">Language</td><td class="has-text-right">{{ result.language | default(value="—") }}</td></tr>
            <tr><td style="color: var(--julie-text-muted);">Visibility</td><td class="has-text-right">{{ result.visibility | default(value="—") }}</td></tr>
        </table>
    </div>
</div>
{% if result.context %}
<pre class="mono is-size-7 mt-2" style="background: var(--julie-bg); padding: 0.625rem; border-radius: 6px; border: 1px solid var(--julie-border); color: var(--julie-text); line-height: 1.6;">{{ result.context }}</pre>
{% endif %}
```

- [ ] **Step 5: Wire up the search pipeline**

In `src/dashboard/routes/search.rs`, after the TODO comment in the `search` function, add the actual search call. This requires importing the search infrastructure:

```rust
    // Execute search if we have a workspace pool
    let results: Vec<serde_json::Value> = if let Some(pool) = state.dashboard.workspace_pool() {
        if !workspace_id.is_empty() {
            if let Some(ws) = pool.get(&workspace_id).await {
                // Call the search pipeline through the workspace
                match ws.search(&query, &search_target, language.as_deref(), file_pattern.as_deref(), limit) {
                    Ok(search_results) => {
                        search_results.into_iter().map(|r| {
                            serde_json::json!({
                                "name": r.name,
                                "kind": r.kind,
                                "file": r.file_path,
                                "line": r.line,
                                "score": format!("{:.2}", r.score),
                                "context": r.context.unwrap_or_default(),
                            })
                        }).collect()
                    }
                    Err(e) => {
                        tracing::warn!("Search failed: {}", e);
                        vec![]
                    }
                }
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    context.insert("results", &results);
```

Note: The exact method signature for `JulieWorkspace::search()` and result type will need to be verified by the implementing agent using `deep_dive` on the workspace's search method. The implementing agent MUST use Julie's tools (`deep_dive`, `fast_search`, `get_symbols`) to verify the actual API before writing this integration code. The code above is a sketch of the pattern; field names and method signatures may differ.

- [ ] **Step 6: Build and verify**

Run: `cargo build 2>&1 | tail -10`
Expected: compiles successfully

- [ ] **Step 7: Commit**

```bash
git add dashboard/templates/ src/dashboard/
git commit -m "feat(dashboard): implement search playground with debug mode"
```

---

## Task 9: Emit Tool Call Events for Live Feed

Wire up the MCP handler to emit `DashboardEvent::ToolCall` events so the metrics live feed actually works.

**Files:**
- Modify: `src/dashboard/state.rs` (add global sender accessor)
- Modify: `src/handler.rs` (emit events after tool calls)
- Modify: `src/daemon/mod.rs` (pass broadcast sender to handler)

- [ ] **Step 1: Examine the handler's tool call recording**

The implementing agent MUST use `deep_dive` on `MetricsTask` or `handle_ipc_session` to understand where tool calls are recorded today, then add a broadcast send at that point.

The pattern is: after `daemon_db.insert_tool_call(...)` or `session_metrics.record(...)`, also call:

```rust
if let Some(tx) = dashboard_tx.as_ref() {
    let _ = tx.send(DashboardEvent::ToolCall {
        tool_name: tool_name.to_string(),
        workspace: workspace_id.to_string(),
        duration_ms,
    });
}
```

Where `dashboard_tx` is an `Option<broadcast::Sender<DashboardEvent>>` passed into the session handler.

- [ ] **Step 2: Thread the broadcast sender through**

In `handle_ipc_session` in `src/daemon/mod.rs`, add a `dashboard_tx: Option<broadcast::Sender<DashboardEvent>>` parameter. Pass it from `accept_loop`, which gets it from `DashboardState::sender()`.

Add to `DashboardState`:

```rust
    pub fn sender(&self) -> broadcast::Sender<DashboardEvent> {
        self.tx.clone()
    }
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | tail -10`
Expected: compiles successfully

- [ ] **Step 4: Commit**

```bash
git add src/handler.rs src/daemon/mod.rs src/dashboard/state.rs
git commit -m "feat(dashboard): emit tool call events for live activity feed"
```

---

## Task 10: Integration Test and Polish

End-to-end verification that the dashboard starts and serves pages.

**Files:**
- Create: `src/tests/dashboard/integration.rs`

- [ ] **Step 1: Write integration test**

Create `src/tests/dashboard/integration.rs`:

```rust
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;
use julie::dashboard::{create_router, DashboardConfig};
use julie::dashboard::state::DashboardState;
use julie::daemon::session::SessionTracker;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn test_state() -> DashboardState {
    DashboardState::new(
        Arc::new(SessionTracker::new()),
        None,
        Arc::new(AtomicBool::new(false)),
        Instant::now(),
        false, // embedding_available
        None,  // workspace_pool
        50,
    )
}

#[tokio::test]
async fn test_all_pages_return_200() {
    let state = test_state();
    let config = DashboardConfig::default();

    for path in ["/", "/projects", "/metrics", "/search"] {
        let app = create_router(state.clone(), config.clone());
        let response = app
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.status().as_u16(),
            200,
            "GET {} returned {}",
            path,
            response.status()
        );
    }
}

#[tokio::test]
async fn test_static_files_served() {
    let state = test_state();
    let config = DashboardConfig::default();
    let app = create_router(state, config);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/app.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let content_type = response.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("text/css"));
}

#[tokio::test]
async fn test_404_for_missing_static() {
    let state = test_state();
    let config = DashboardConfig::default();
    let app = create_router(state, config);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/nonexistent.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 404);
}
```

- [ ] **Step 2: Register test and run**

Add to `src/tests/dashboard/mod.rs`:

```rust
mod integration;
```

Run: `cargo test --lib test_all_pages_return_200 2>&1 | tail -10`
Expected: PASS

Run: `cargo test --lib test_static_files 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 3: Run xtask dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All tests pass, no regressions

- [ ] **Step 4: Commit**

```bash
git add src/tests/dashboard/
git commit -m "test(dashboard): add integration tests for all pages and static serving"
```

---

## Task 11: Version Bump and Final Commit

- [ ] **Step 1: Bump version to v6.1.0**

This is a feature release (new dashboard), not a patch. Update `Cargo.toml`, `CLAUDE.md`, and `AGENTS.md` version references from `6.0.8` to `6.1.0`.

- [ ] **Step 2: Run full test tier**

Run: `cargo xtask test full 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 3: Commit and tag**

```bash
git add Cargo.toml CLAUDE.md AGENTS.md
git commit -m "chore: bump version to v6.1.0 for dashboard release"
git tag v6.1.0
```

---

## Agent Team Parallelization Guide

After **Task 4** (wire HTTP server into daemon) completes, the following tasks can run in parallel with separate agents:

| Agent | Task | Independence |
|-------|------|-------------|
| Agent 1 | Task 5: System Status View | Fully independent (reads SessionTracker, ErrorBuffer) |
| Agent 2 | Task 6: Projects View | Fully independent (reads DaemonDatabase) |
| Agent 3 | Task 7: Metrics View | Fully independent (reads DaemonDatabase, needs ToolCallSummary serialization) |
| Agent 4 | Task 8: Search Playground | Mostly independent (needs WorkspacePool in state, which Task 4 sets up) |

**Task 9** (tool call event emission) should run after Task 7 since it makes the live feed actually work.

**Task 10** (integration test) runs after all views are complete.

**Task 11** (version bump) runs last.

**Recommended sequence:**
1. Tasks 0-4 sequentially (foundation, must be first)
2. Tasks 5-8 in parallel (4 agents, one per view)
3. Task 9 after Task 7
4. Task 10 after all views
5. Task 11 last
