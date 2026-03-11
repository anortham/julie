//! System tray icon and menu construction.
//!
//! Builds the tray icon with a context menu that provides daemon status,
//! dashboard access, project list, embeddings status, and lifecycle controls.
//! The menu is rebuilt periodically by the health poller to reflect live state.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Wry};

use crate::daemon_client::{self, DEFAULT_PORT, ProjectResponse};

// Embed tray icons at compile time — use @2x for better HiDPI display.
// The OS will downscale for standard-DPI screens.
const ICON_GREEN: &[u8] = include_bytes!("../icons/tray-green@2x.png");
const ICON_YELLOW: &[u8] = include_bytes!("../icons/tray-yellow@2x.png");
const ICON_RED: &[u8] = include_bytes!("../icons/tray-red@2x.png");

/// Daemon status as reflected by the tray icon color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonStatus {
    /// Green — daemon running and healthy
    Healthy,
    /// Yellow — daemon starting, indexing, or degraded
    Starting,
    /// Red — daemon stopped or unreachable
    Stopped,
}

/// Returns the icon image for the given status.
pub fn icon_for_status(status: DaemonStatus) -> Image<'static> {
    let bytes = match status {
        DaemonStatus::Healthy => ICON_GREEN,
        DaemonStatus::Starting => ICON_YELLOW,
        DaemonStatus::Stopped => ICON_RED,
    };
    Image::from_bytes(bytes).expect("embedded icon PNG is valid")
}

/// Build the tray context menu for the given daemon state.
pub fn build_menu(
    app: &AppHandle,
    status: DaemonStatus,
    version: Option<&str>,
    port: u16,
    projects: &[ProjectResponse],
) -> tauri::Result<Menu<Wry>> {
    // Status line
    let status_text = match (status, version) {
        (DaemonStatus::Healthy, Some(v)) => format!("Julie {} — Running (port {})", v, port),
        (DaemonStatus::Healthy, None) => format!("Julie — Running (port {})", port),
        (DaemonStatus::Starting, _) => "Julie — Starting...".to_string(),
        (DaemonStatus::Stopped, _) => "Julie — Stopped".to_string(),
    };
    let status_item = MenuItem::with_id(app, "status", &status_text, false, None::<&str>)?;

    let sep1 = PredefinedMenuItem::separator(app)?;

    // Dashboard link
    let dashboard_enabled = status == DaemonStatus::Healthy;
    let dashboard =
        MenuItem::with_id(app, "dashboard", "Open Dashboard", dashboard_enabled, None::<&str>)?;

    let sep2 = PredefinedMenuItem::separator(app)?;

    // Projects submenu
    let projects_submenu = build_projects_submenu(app, projects)?;

    // Embeddings status line
    let embeddings_text = summarize_embeddings(projects);
    let embeddings_item =
        MenuItem::with_id(app, "embeddings", &embeddings_text, false, None::<&str>)?;

    let sep3 = PredefinedMenuItem::separator(app)?;

    // Daemon lifecycle
    let is_running = status != DaemonStatus::Stopped;
    let start = MenuItem::with_id(app, "start", "Start Daemon", !is_running, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Stop Daemon", is_running, None::<&str>)?;
    let restart = MenuItem::with_id(app, "restart", "Restart Daemon", is_running, None::<&str>)?;

    let sep4 = PredefinedMenuItem::separator(app)?;

    // Export diagnostic bundle
    let export = MenuItem::with_id(
        app,
        "export-diagnostics",
        "Export Diagnostic Bundle...",
        true,
        None::<&str>,
    )?;

    // Update available (shown only when detected)
    let update_item = if let Some(update) = crate::updates::available_update() {
        Some(MenuItem::with_id(
            app,
            "update",
            &format!("Update Available: v{}", update.version),
            true,
            None::<&str>,
        )?)
    } else {
        None
    };

    // Autostart toggle
    let autostart_enabled = {
        use tauri_plugin_autostart::ManagerExt;
        app.autolaunch().is_enabled().unwrap_or(false)
    };
    let autostart_label = if autostart_enabled {
        "Launch at Login  ✓"
    } else {
        "Launch at Login"
    };
    let autostart =
        MenuItem::with_id(app, "autostart", autostart_label, true, None::<&str>)?;

    let sep5 = PredefinedMenuItem::separator(app)?;

    // Quit
    let quit = MenuItem::with_id(app, "quit", "Quit Julie", true, None::<&str>)?;

    // Build menu — conditionally include update item
    let mut items: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = vec![
        &status_item,
        &sep1,
        &dashboard,
        &sep2,
        &projects_submenu,
        &embeddings_item,
        &sep3,
        &start,
        &stop,
        &restart,
        &sep4,
        &export,
    ];

    if let Some(ref update) = update_item {
        items.push(update);
    }

    items.push(&autostart);
    items.push(&sep5);
    items.push(&quit);

    Menu::with_items(app, &items)
}

/// Build the Projects submenu with per-project status.
fn build_projects_submenu(
    app: &AppHandle,
    projects: &[ProjectResponse],
) -> tauri::Result<Submenu<Wry>> {
    let submenu = Submenu::with_id(app, "projects-menu", "Projects", true)?;

    if projects.is_empty() {
        let empty =
            MenuItem::with_id(app, "no-projects", "No projects registered", false, None::<&str>)?;
        submenu.append(&empty)?;
    } else {
        for (i, project) in projects.iter().enumerate() {
            let symbols = project
                .symbol_count
                .map(|n| format_number(n))
                .unwrap_or_default();

            let label = if symbols.is_empty() {
                format!("{} ({})", project.name, project.status.to_lowercase())
            } else {
                format!(
                    "{} ({} symbols, {})",
                    project.name,
                    symbols,
                    project.status.to_lowercase()
                )
            };

            let item = MenuItem::with_id(
                app,
                &format!("project-{}", i),
                &label,
                false,
                None::<&str>,
            )?;
            submenu.append(&item)?;
        }
    }

    Ok(submenu)
}

/// Summarize embeddings status across all projects into a single line.
fn summarize_embeddings(projects: &[ProjectResponse]) -> String {
    if projects.is_empty() {
        return "Embeddings: —".to_string();
    }

    // Find the "best" embedding status across projects
    let mut has_sidecar = false;
    let mut has_ort = false;
    let mut has_accelerated = false;
    let mut any_initialized = false;

    for project in projects {
        if let Some(ref es) = project.embedding_status {
            any_initialized = true;
            if es.accelerated {
                has_accelerated = true;
            }
            match es.backend.as_str() {
                "sidecar" => has_sidecar = true,
                "ort" => has_ort = true,
                _ => {}
            }
        }
    }

    if !any_initialized {
        return "Embeddings: not initialized".to_string();
    }

    let backend = if has_sidecar { "sidecar" } else if has_ort { "ort" } else { "unknown" };
    let accel = if has_accelerated { "GPU" } else { "CPU" };

    format!("Embeddings: {} ({})", accel, backend)
}

/// Format a number with thousands separators: 31245 → "31,245".
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

/// Set up the tray icon with initial state and event handlers.
pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let (initial_status, port) = match daemon_client::is_daemon_running() {
        Some(info) => (DaemonStatus::Starting, info.port),
        None => (DaemonStatus::Stopped, DEFAULT_PORT),
    };

    let menu = build_menu(app, initial_status, None, port, &[])?;

    let _tray = TrayIconBuilder::with_id("julie-tray")
        .icon(icon_for_status(initial_status))
        .tooltip("Julie — Code Intelligence")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(handle_menu_event)
        .build(app)?;

    Ok(())
}

/// Handle menu item clicks.
fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let current_port = daemon_client::is_daemon_running()
        .map(|info| info.port)
        .unwrap_or(DEFAULT_PORT);

    match event.id().as_ref() {
        "dashboard" => {
            let url = format!("http://localhost:{}/ui/", current_port);
            use tauri_plugin_opener::OpenerExt;
            let _ = app.opener().open_url(&url, None::<&str>);
        }
        "start" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = daemon_client::start_daemon_locked(DEFAULT_PORT).await {
                    eprintln!("Failed to start daemon: {e}");
                    if let Some(tray) = app.tray_by_id("julie-tray") {
                        let _ = tray.set_tooltip(Some(&format!("Julie — Error: {e}")));
                    }
                }
            });
        }
        "stop" => {
            std::thread::spawn(|| {
                if let Err(e) = daemon_client::stop_daemon() {
                    eprintln!("Failed to stop daemon: {e}");
                }
            });
        }
        "restart" => {
            let port = current_port;
            std::thread::spawn(move || {
                if let Err(e) = daemon_client::restart_daemon(port) {
                    eprintln!("Failed to restart daemon: {e}");
                }
            });
        }
        "export-diagnostics" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                match crate::diagnostics::export_bundle().await {
                    Ok(path) => {
                        use tauri_plugin_notification::NotificationExt;
                        let _ = app
                            .notification()
                            .builder()
                            .title("Diagnostic Bundle Exported")
                            .body(format!("Saved to {}", path.display()))
                            .show();
                    }
                    Err(e) => {
                        eprintln!("Failed to export diagnostics: {e}");
                        use tauri_plugin_notification::NotificationExt;
                        let _ = app
                            .notification()
                            .builder()
                            .title("Export Failed")
                            .body(format!("{e}"))
                            .show();
                    }
                }
            });
        }
        "update" => {
            // Open the GitHub release page
            if let Some(url) = crate::updates::update_url() {
                use tauri_plugin_opener::OpenerExt;
                let _ = app.opener().open_url(&url, None::<&str>);
            }
        }
        "autostart" => {
            use tauri_plugin_autostart::ManagerExt;
            let manager = app.autolaunch();
            let currently_enabled = manager.is_enabled().unwrap_or(false);
            if currently_enabled {
                let _ = manager.disable();
            } else {
                let _ = manager.enable();
            }
            // Menu will rebuild on next health poll cycle with updated state
        }
        "quit" => {
            // Don't stop the daemon — it serves other clients (CLI, MCP).
            // The tray is an optional management overlay, not the lifecycle owner.
            app.exit(0);
        }
        _ => {}
    }
}
