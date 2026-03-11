//! Diagnostic bundle collection.
//!
//! Collects logs, system info, project data, and daemon state into a zip file
//! for easy sharing when reporting issues. Sensitive paths are sanitized
//! (home directory → `~`).

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::daemon_client;

/// Collect a diagnostic bundle and save it to the user's Desktop.
/// Returns the path to the created zip file.
pub async fn export_bundle() -> Result<PathBuf> {
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("julie-diagnostic-{}.zip", timestamp);

    let output_dir = desktop_or_home()?;
    let output_path = output_dir.join(&filename);

    let file = std::fs::File::create(&output_path)
        .with_context(|| format!("Failed to create {}", output_path.display()))?;

    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // System info
    add_system_info(&mut zip, options)?;

    // Daemon health (if running)
    add_daemon_info(&mut zip, options).await;

    // Project list (if daemon running)
    add_project_info(&mut zip, options).await;

    // PID file
    add_file_if_exists(&mut zip, options, "daemon.pid", &julie_home()?.join("daemon.pid"))?;

    // Registry
    add_file_if_exists(
        &mut zip,
        options,
        "registry.toml",
        &julie_home()?.join("registry.toml"),
    )?;

    // Recent logs
    add_recent_logs(&mut zip, options)?;

    zip.finish().context("Failed to finalize zip")?;

    Ok(output_path)
}

/// Add system info as a text file.
fn add_system_info(zip: &mut ZipWriter<std::fs::File>, options: SimpleFileOptions) -> Result<()> {
    let info = format!(
        "Julie Diagnostic Bundle\n\
         =======================\n\
         Date: {}\n\
         Tray Version: {}\n\
         OS: {}\n\
         Arch: {}\n\
         Home: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z"),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        sanitize_home(&julie_home().unwrap_or_default().to_string_lossy()),
    );

    zip.start_file("system-info.txt", options)?;
    zip.write_all(info.as_bytes())?;
    Ok(())
}

/// Add daemon health response as JSON.
async fn add_daemon_info(zip: &mut ZipWriter<std::fs::File>, options: SimpleFileOptions) {
    let port = daemon_client::is_daemon_running()
        .map(|info| info.port)
        .unwrap_or(daemon_client::DEFAULT_PORT);

    if let Some(health) = daemon_client::check_health(port).await {
        if let Ok(json) = serde_json::to_string_pretty(&health) {
            let _ = zip.start_file("health.json", options);
            let _ = zip.write_all(json.as_bytes());
        }
    } else {
        let _ = zip.start_file("health.json", options);
        let _ = zip.write_all(b"{\"error\": \"daemon not reachable\"}");
    }
}

/// Add project list as JSON.
async fn add_project_info(zip: &mut ZipWriter<std::fs::File>, options: SimpleFileOptions) {
    let port = daemon_client::is_daemon_running()
        .map(|info| info.port)
        .unwrap_or(daemon_client::DEFAULT_PORT);

    if let Some(projects) = daemon_client::fetch_projects(port).await {
        if let Ok(json) = serde_json::to_string_pretty(&projects) {
            let _ = zip.start_file("projects.json", options);
            let _ = zip.write_all(sanitize_home(&json).as_bytes());
        }
    }
}

/// Add a file from disk if it exists, sanitizing any paths in its content.
fn add_file_if_exists(
    zip: &mut ZipWriter<std::fs::File>,
    options: SimpleFileOptions,
    zip_name: &str,
    path: &std::path::Path,
) -> Result<()> {
    if path.exists() {
        let content = std::fs::read_to_string(path).unwrap_or_else(|e| format!("(read error: {e})"));
        zip.start_file(zip_name, options)?;
        zip.write_all(sanitize_home(&content).as_bytes())?;
    }
    Ok(())
}

/// Add recent log files (today + yesterday, last 2000 lines each).
fn add_recent_logs(zip: &mut ZipWriter<std::fs::File>, options: SimpleFileOptions) -> Result<()> {
    let logs_dir = julie_home()?.join("logs");
    if !logs_dir.exists() {
        return Ok(());
    }

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();

    for date in [&today, &yesterday] {
        let log_file = logs_dir.join(format!("julie.log.{}", date));
        if log_file.exists() {
            let content = std::fs::read_to_string(&log_file).unwrap_or_default();
            // Take last 2000 lines to keep bundle size reasonable
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(2000);
            let truncated = lines[start..].join("\n");

            let zip_name = format!("logs/julie.log.{}", date);
            zip.start_file(&zip_name, options)?;
            zip.write_all(sanitize_home(&truncated).as_bytes())?;
        }
    }

    // Also include daemon stdout/stderr logs
    for name in ["daemon-stdout.log", "daemon-stderr.log"] {
        let path = logs_dir.join(name);
        if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(500);
            let truncated = lines[start..].join("\n");

            let zip_name = format!("logs/{}", name);
            zip.start_file(&zip_name, options)?;
            zip.write_all(sanitize_home(&truncated).as_bytes())?;
        }
    }

    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

fn julie_home() -> Result<PathBuf> {
    daemon_client::julie_home()
}

/// Find the Desktop directory, falling back to home.
fn desktop_or_home() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("Cannot determine home directory")?;

    let desktop = PathBuf::from(&home).join("Desktop");
    if desktop.exists() {
        Ok(desktop)
    } else {
        Ok(PathBuf::from(home))
    }
}

/// Sanitize paths in content — replace home directory with `~`.
/// On Windows, also handles forward-slash variants of the home path.
fn sanitize_home(content: &str) -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();

    if home.is_empty() {
        return content.to_string();
    }

    let result = content.replace(&home, "~");
    // On Windows, paths may use forward slashes too (e.g. C:/Users/Name)
    let home_fwd = home.replace('\\', "/");
    if home_fwd != home {
        result.replace(&home_fwd, "~")
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_home() {
        let home = std::env::var("HOME").unwrap_or("/home/user".to_string());
        let input = format!("{}/source/julie/logs/test.log", home);
        let result = sanitize_home(&input);
        assert!(result.starts_with("~/"));
        assert!(!result.contains(&home));
    }
}
