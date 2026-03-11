//! Julie Tray — system tray app for managing the Julie daemon.
//!
//! A lightweight Tauri 2.0 tray-only application that provides:
//! - Persistent tray icon with daemon health status (green/yellow/red)
//! - Context menu for daemon lifecycle (start/stop/restart)
//! - One-click access to the web dashboard
//! - Auto-launch on login
//! - Update notifications via GitHub releases
//!
//! This app does NOT depend on the `julie` crate. It uses subprocess calls
//! for daemon lifecycle and HTTP for status, keeping the binary small (~5MB).

mod daemon_client;
mod diagnostics;
mod health_poller;
mod tray;
mod updates;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            // Build system tray icon with initial menu
            tray::setup_tray(app.handle())?;

            // Spawn background health poller (updates tray icon every 10s)
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(health_poller::run(handle));

            // Spawn update checker (first check after 60s, then every 6h)
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(updates::check_periodically(handle));

            // Auto-start daemon if not running (uses startup lock to prevent races)
            tauri::async_runtime::spawn(async {
                if daemon_client::is_daemon_running().is_none() {
                    if let Err(e) =
                        daemon_client::start_daemon_locked(daemon_client::DEFAULT_PORT).await
                    {
                        eprintln!("Auto-start failed: {e}");
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Julie tray app");
}
