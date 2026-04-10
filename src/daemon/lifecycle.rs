//! Daemon lifecycle management: status checks, stop, and restart support.

use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;
#[cfg(unix)]
use libc;
use tracing::info;

/// Current state of the Julie daemon process.
#[derive(Debug, PartialEq)]
pub enum DaemonStatus {
    Running { pid: u32 },
    NotRunning,
}

/// Check whether the daemon is currently running by inspecting the PID file.
///
/// Returns `Running { pid }` if a live process owns the PID file,
/// `NotRunning` otherwise (including stale PID file cleanup).
pub fn check_status(paths: &DaemonPaths) -> DaemonStatus {
    match PidFile::check_running(&paths.daemon_pid()) {
        Some(pid) => DaemonStatus::Running { pid },
        None => DaemonStatus::NotRunning,
    }
}

/// Stop the daemon process if it is running.
///
/// On Unix, sends SIGTERM for graceful shutdown. On Windows, signals a named
/// event that the daemon waits on, falling back to `taskkill /F` for older
/// daemons that predate the event mechanism.
/// Returns `Ok(())` even if the daemon is not running (idempotent).
pub fn stop_daemon(paths: &DaemonPaths) -> anyhow::Result<()> {
    match PidFile::check_running(&paths.daemon_pid()) {
        Some(pid) => {
            info!("Sending shutdown signal to daemon PID {}", pid);

            #[cfg(unix)]
            {
                let ret = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                if ret != 0 {
                    anyhow::bail!("Failed to send SIGTERM to PID {}", pid);
                }
            }

            #[cfg(windows)]
            {
                use super::shutdown_event;

                let event_name = paths.daemon_shutdown_event();
                let signaled = shutdown_event::signal_shutdown(&event_name).unwrap_or(false);
                if signaled {
                    info!("Signaled shutdown event: {}", event_name);
                } else {
                    info!("Shutdown event not found, falling back to taskkill /F");
                    let _ = std::process::Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .output();
                }
            }

            // Wait for the process to actually exit (up to 10s).
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                if !PidFile::is_process_alive(pid) {
                    // Process exited. Clean up any stale files the daemon
                    // didn't get to (e.g., if it crashed mid-shutdown).
                    let _ = std::fs::remove_file(paths.daemon_pid());
                    let _ = std::fs::remove_file(paths.daemon_state());
                    #[cfg(unix)]
                    let _ = std::fs::remove_file(paths.daemon_socket());
                    info!("Daemon stopped");
                    return Ok(());
                }
                if std::time::Instant::now() >= deadline {
                    // Process is still alive. Do NOT delete files under it.
                    anyhow::bail!(
                        "Daemon did not stop within 10s (PID {}). \
                         Use `kill {}` to force.",
                        pid,
                        pid
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        None => {
            // No live daemon. Clean up any stale files.
            let _ = std::fs::remove_file(paths.daemon_pid());
            let _ = std::fs::remove_file(paths.daemon_state());
            #[cfg(unix)]
            let _ = std::fs::remove_file(paths.daemon_socket());
            info!("Daemon is not running (cleaned stale files if any)");
            Ok(())
        }
    }
}
