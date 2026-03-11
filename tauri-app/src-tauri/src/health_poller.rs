//! Background health poller that updates tray icon and menu.
//!
//! Polls `/api/health` every 10 seconds and `/api/projects` every 30 seconds.
//! Updates the tray icon color on state transitions. Tolerates transient
//! failures — requires N consecutive failed checks before declaring stopped.

use std::time::Duration;

use tauri::AppHandle;

use crate::daemon_client::{self, DEFAULT_PORT, ProjectResponse};
use crate::tray::{self, DaemonStatus};

/// How many consecutive health check failures before status changes.
/// At 10s intervals, 3 failures = 30 seconds of grace.
const FAILURE_THRESHOLD: u32 = 3;

/// After this many consecutive failures (even with process alive), declare Stopped.
/// At 10s intervals, 9 failures = 90 seconds — if HTTP is dead that long, daemon is stuck.
const STUCK_THRESHOLD: u32 = 9;

/// Interval between health polls.
const POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Fetch projects every Nth health poll (3 × 10s = 30s).
const PROJECTS_EVERY_N_TICKS: u32 = 3;

/// Cached state from the last poll cycle.
struct PollState {
    status: DaemonStatus,
    version: Option<String>,
    port: u16,
    consecutive_failures: u32,
    projects: Vec<ProjectResponse>,
    tick_count: u32,
}

impl PollState {
    fn new() -> Self {
        // Bootstrap from PID file if available
        let (status, port) = match daemon_client::is_daemon_running() {
            Some(info) => (DaemonStatus::Starting, info.port),
            None => (DaemonStatus::Stopped, DEFAULT_PORT),
        };

        Self {
            status,
            version: None,
            port,
            consecutive_failures: 0,
            projects: Vec::new(),
            tick_count: 0,
        }
    }
}

/// Run the health poll loop. Call this from a spawned async task.
pub async fn run(app: AppHandle) {
    let mut state = PollState::new();

    // Do an immediate health check + project fetch before entering the loop
    poll_once(&app, &mut state, true).await;

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        state.tick_count += 1;
        let fetch_projects = state.tick_count % PROJECTS_EVERY_N_TICKS == 0;
        poll_once(&app, &mut state, fetch_projects).await;
    }
}

/// Execute one poll cycle: check health, optionally fetch projects, update tray.
async fn poll_once(app: &AppHandle, state: &mut PollState, fetch_projects: bool) {
    // Refresh port from PID file (daemon may have restarted on a different port)
    if let Some(info) = daemon_client::is_daemon_running() {
        state.port = info.port;
    }

    let old_status = state.status;

    // Check if daemon process exists at all
    if daemon_client::is_daemon_running().is_none() {
        state.consecutive_failures = 0;
        state.version = None;
        state.projects.clear();
        state.status = DaemonStatus::Stopped;
    } else {
        // Process exists — try HTTP health check
        match daemon_client::check_health(state.port).await {
            Some(health) => {
                state.consecutive_failures = 0;
                state.version = health.version;
                state.status = DaemonStatus::Healthy;
            }
            None => {
                state.consecutive_failures += 1;
                if state.consecutive_failures >= STUCK_THRESHOLD {
                    // Process alive but HTTP dead for 90s+ — daemon is stuck
                    state.status = DaemonStatus::Stopped;
                } else if state.consecutive_failures >= FAILURE_THRESHOLD {
                    state.status = if old_status == DaemonStatus::Healthy {
                        DaemonStatus::Starting // Was healthy → probably restarting
                    } else {
                        DaemonStatus::Stopped
                    };
                } else if state.status == DaemonStatus::Stopped {
                    state.status = DaemonStatus::Starting;
                }
            }
        }
    }

    // Fetch projects list when healthy and it's time
    if state.status == DaemonStatus::Healthy && fetch_projects {
        if let Some(projects) = daemon_client::fetch_projects(state.port).await {
            state.projects = projects;
        }
    }

    // Update tray if status changed OR we got fresh project data
    if state.status != old_status || fetch_projects {
        update_tray(app, state);
    }
}

/// Update the tray icon and menu to reflect current state.
fn update_tray(app: &AppHandle, state: &PollState) {
    let Some(tray) = app.tray_by_id("julie-tray") else {
        return;
    };

    // Update icon
    let _ = tray.set_icon(Some(tray::icon_for_status(state.status)));

    // Update tooltip
    let tooltip = match state.status {
        DaemonStatus::Healthy => {
            let v = state.version.as_deref().unwrap_or("");
            let n = state.projects.len();
            if n > 0 {
                format!("Julie {} — {} project{}", v, n, if n == 1 { "" } else { "s" })
            } else {
                format!("Julie {} — Running", v)
            }
        }
        DaemonStatus::Starting => "Julie — Starting...".to_string(),
        DaemonStatus::Stopped => "Julie — Stopped".to_string(),
    };
    let _ = tray.set_tooltip(Some(&tooltip));

    // Rebuild menu with current projects
    if let Ok(menu) = tray::build_menu(
        app,
        state.status,
        state.version.as_deref(),
        state.port,
        &state.projects,
    ) {
        let _ = tray.set_menu(Some(menu));
    }
}
