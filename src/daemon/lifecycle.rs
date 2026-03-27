//! Daemon lifecycle management: status checks, stop, and restart support.

use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;
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
                    // Daemon predates the event mechanism (or event creation
                    // failed at startup). Fall back to force-kill.
                    info!("Shutdown event not found, falling back to taskkill /F");
                    let _ = std::process::Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .output();
                }
            }

            // Poll until the process exits (up to 5s), then clean up stale files.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if !PidFile::is_process_alive(pid) {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            // Clean up stale files if the daemon didn't get to them in time
            let _ = std::fs::remove_file(paths.daemon_pid());
            #[cfg(unix)]
            let _ = std::fs::remove_file(paths.daemon_socket());

            info!("Daemon stopped");
            Ok(())
        }
        None => {
            info!("Daemon is not running");
            Ok(())
        }
    }
}
