//! Install/uninstall Julie as a system service.
//!
//! `julie-server install` copies the binary to `~/.julie/bin/` (or
//! `%APPDATA%\julie\bin\` on Windows), registers a platform-native
//! auto-start service, and starts the daemon immediately.
//!
//! `julie-server uninstall` reverses the process, leaving user data
//! (indexes, logs, memories) intact.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;

use crate::daemon;

/// Resolved paths for the install target.
struct InstallPaths {
    /// `~/.julie/bin/`
    bin_dir: PathBuf,
    /// `~/.julie/bin/julie-server[.exe]`
    binary: PathBuf,
    /// `~/.julie/logs/`
    logs_dir: PathBuf,
}

impl InstallPaths {
    fn resolve() -> Result<Self> {
        let home = daemon::julie_home()?;
        let bin_dir = home.join("bin");
        let binary = if cfg!(windows) {
            bin_dir.join("julie-server.exe")
        } else {
            bin_dir.join("julie-server")
        };
        let logs_dir = home.join("logs");
        Ok(Self {
            bin_dir,
            binary,
            logs_dir,
        })
    }
}

// ─── Install ────────────────────────────────────────────────────────────────

pub fn install(port: u16) -> Result<()> {
    let paths = InstallPaths::resolve()?;

    // 1. Copy binary
    copy_binary(&paths)?;

    // 2. Create platform service
    create_service(&paths, port)?;

    // 3. Start the service
    start_service()?;

    // 4. Print summary
    println!();
    println!("Julie installed successfully!");
    println!("  Binary:    {}", paths.binary.display());
    println!("  Dashboard: http://localhost:{}/ui/", port);
    println!("  API docs:  http://localhost:{}/api/docs", port);
    println!();
    println!("Julie will auto-start on login from now on.");
    println!();
    println!("Make sure julie-server is in your PATH:");
    println!("  {}", paths.bin_dir.display());
    println!();
    println!("Next steps — connect your AI tool:");
    println!("  Claude Code:");
    println!("    /plugin marketplace add anortham/julie");
    println!("    /plugin install julie@julie");
    println!("  Cursor/other: see https://github.com/anortham/julie#installation");

    Ok(())
}

/// Copy the currently-running binary to `~/.julie/bin/`.
fn copy_binary(paths: &InstallPaths) -> Result<()> {
    let current_exe =
        std::env::current_exe().context("Could not determine path of current executable")?;

    // Create bin dir
    fs::create_dir_all(&paths.bin_dir)
        .with_context(|| format!("Failed to create {}", paths.bin_dir.display()))?;

    // Create logs dir (services will log here)
    fs::create_dir_all(&paths.logs_dir)
        .with_context(|| format!("Failed to create {}", paths.logs_dir.display()))?;

    // If the binary is already at the target path, skip copy to avoid
    // "text file busy" errors on some platforms.
    let current_canonical = current_exe.canonicalize().unwrap_or(current_exe.clone());
    let target_canonical = paths
        .binary
        .canonicalize()
        .unwrap_or(paths.binary.clone());

    if current_canonical == target_canonical {
        println!("Binary already at {}", paths.binary.display());
    } else {
        // On upgrade: stop the daemon first so we can overwrite the binary
        let _ = stop_service();

        fs::copy(&current_exe, &paths.binary).with_context(|| {
            format!(
                "Failed to copy {} to {}",
                current_exe.display(),
                paths.binary.display()
            )
        })?;

        // Ensure the binary is executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o755);
            fs::set_permissions(&paths.binary, perms)
                .context("Failed to set binary permissions")?;
        }

        println!("Installed binary to {}", paths.binary.display());
    }

    Ok(())
}

// ─── Platform-specific service management ───────────────────────────────────

#[cfg(target_os = "macos")]
fn service_config_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join("com.julie.server.plist"))
}

#[cfg(target_os = "linux")]
fn service_config_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    Ok(PathBuf::from(home)
        .join(".config/systemd/user")
        .join("julie.service"))
}

#[cfg(target_os = "windows")]
fn service_config_path() -> Result<PathBuf> {
    // Windows uses a registry entry, not a config file — return a sentinel.
    Ok(PathBuf::from("registry://HKCU/Run/Julie Server"))
}

// ─── Create service ─────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn create_service(paths: &InstallPaths, port: u16) -> Result<()> {
    let plist_path = service_config_path()?;
    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.julie.server</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>daemon</string>
        <string>start</string>
        <string>--foreground</string>
        <string>--port</string>
        <string>{port}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{logs}/launchd-stdout.log</string>
    <key>StandardErrorPath</key>
    <string>{logs}/launchd-stderr.log</string>
</dict>
</plist>
"#,
        binary = paths.binary.display(),
        port = port,
        logs = paths.logs_dir.display(),
    );

    fs::write(&plist_path, &plist)
        .with_context(|| format!("Failed to write {}", plist_path.display()))?;

    println!("Created LaunchAgent: {}", plist_path.display());
    Ok(())
}

#[cfg(target_os = "linux")]
fn create_service(paths: &InstallPaths, port: u16) -> Result<()> {
    let unit_path = service_config_path()?;
    if let Some(parent) = unit_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    let unit = format!(
        r#"[Unit]
Description=Julie Code Intelligence Server
After=network.target

[Service]
Type=simple
ExecStart={binary} daemon start --foreground --port {port}
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
"#,
        binary = paths.binary.display(),
        port = port,
    );

    fs::write(&unit_path, &unit)
        .with_context(|| format!("Failed to write {}", unit_path.display()))?;

    // Reload systemd to pick up the new unit
    let status = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .context("Failed to run systemctl daemon-reload")?;

    if !status.success() {
        bail!("systemctl daemon-reload failed");
    }

    // Enable the service for auto-start
    let status = std::process::Command::new("systemctl")
        .args(["--user", "enable", "julie"])
        .status()
        .context("Failed to enable julie service")?;

    if !status.success() {
        bail!("systemctl enable failed");
    }

    println!("Created systemd user service: {}", unit_path.display());
    Ok(())
}

#[cfg(target_os = "windows")]
fn create_service(paths: &InstallPaths, port: u16) -> Result<()> {
    let binary = paths.binary.display().to_string();
    let run_command = format!("\"{}\" daemon start --foreground --port {}", binary, port);

    // Use HKCU\...\Run — no admin rights required (same as Discord, Spotify, etc.)
    let status = std::process::Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Julie Server",
            "/t",
            "REG_SZ",
            "/d",
            &run_command,
            "/f",
        ])
        .status()
        .context("Failed to add registry autostart entry")?;

    if !status.success() {
        bail!("Failed to register Julie for autostart");
    }

    println!("Registered autostart via HKCU\\...\\Run");
    Ok(())
}

// ─── Start service ──────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn start_service() -> Result<()> {
    let plist_path = service_config_path()?;

    // Unload first (ignore errors — may not be loaded yet)
    let _ = std::process::Command::new("launchctl")
        .args(["unload", &plist_path.to_string_lossy()])
        .status();

    let status = std::process::Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .status()
        .context("Failed to run launchctl load")?;

    if !status.success() {
        bail!("launchctl load failed");
    }

    println!("Daemon started via launchctl");
    Ok(())
}

#[cfg(target_os = "linux")]
fn start_service() -> Result<()> {
    let status = std::process::Command::new("systemctl")
        .args(["--user", "restart", "julie"])
        .status()
        .context("Failed to start julie service")?;

    if !status.success() {
        bail!("systemctl start failed");
    }

    println!("Daemon started via systemd");
    Ok(())
}

#[cfg(target_os = "windows")]
fn start_service() -> Result<()> {
    let paths = InstallPaths::resolve()?;
    let status = std::process::Command::new(&paths.binary)
        .args(["daemon", "start"])
        .status()
        .context("Failed to start daemon")?;

    if !status.success() {
        bail!("daemon start failed");
    }

    println!("Daemon started");
    Ok(())
}

// ─── Stop service ───────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn stop_service() -> Result<()> {
    let plist_path = service_config_path()?;
    let _ = std::process::Command::new("launchctl")
        .args(["unload", &plist_path.to_string_lossy()])
        .status();
    Ok(())
}

#[cfg(target_os = "linux")]
fn stop_service() -> Result<()> {
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "stop", "julie"])
        .status();
    Ok(())
}

#[cfg(target_os = "windows")]
fn stop_service() -> Result<()> {
    // Kill by image name — covers both service and manual starts
    let _ = std::process::Command::new("taskkill")
        .args(["/IM", "julie-server.exe"])
        .status();
    Ok(())
}

// ─── Uninstall ──────────────────────────────────────────────────────────────

pub fn uninstall() -> Result<()> {
    // 1. Stop the service
    println!("Stopping Julie daemon...");
    let _ = stop_service();

    // Also try the PID-based stop as a fallback
    let _ = daemon::daemon_stop();

    // 2. Remove service config
    remove_service()?;

    // 3. Remove binary
    let paths = InstallPaths::resolve()?;
    if paths.binary.exists() {
        fs::remove_file(&paths.binary)
            .with_context(|| format!("Failed to remove {}", paths.binary.display()))?;
        println!("Removed {}", paths.binary.display());
    }

    // Remove bin dir if empty
    if paths.bin_dir.exists() {
        let _ = fs::remove_dir(&paths.bin_dir); // Only succeeds if empty
    }

    println!();
    println!("Julie uninstalled. Your data in {} is preserved.", {
        daemon::julie_home()
            .map(|h| h.display().to_string())
            .unwrap_or_else(|_| "~/.julie".to_string())
    });
    println!("To remove all data: rm -rf {}", {
        daemon::julie_home()
            .map(|h| h.display().to_string())
            .unwrap_or_else(|_| "~/.julie".to_string())
    });

    Ok(())
}

#[cfg(target_os = "macos")]
fn remove_service() -> Result<()> {
    let plist_path = service_config_path()?;

    // Unload first
    let _ = std::process::Command::new("launchctl")
        .args(["unload", &plist_path.to_string_lossy()])
        .status();

    if plist_path.exists() {
        fs::remove_file(&plist_path)
            .with_context(|| format!("Failed to remove {}", plist_path.display()))?;
        println!("Removed LaunchAgent: {}", plist_path.display());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn remove_service() -> Result<()> {
    // Stop and disable
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "stop", "julie"])
        .status();
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "disable", "julie"])
        .status();

    let unit_path = service_config_path()?;
    if unit_path.exists() {
        fs::remove_file(&unit_path)
            .with_context(|| format!("Failed to remove {}", unit_path.display()))?;
        println!("Removed systemd unit: {}", unit_path.display());
    }

    // Reload systemd
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    Ok(())
}

#[cfg(target_os = "windows")]
fn remove_service() -> Result<()> {
    let _ = stop_service();

    let status = std::process::Command::new("reg")
        .args([
            "delete",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Julie Server",
            "/f",
        ])
        .status()
        .context("Failed to remove registry autostart entry")?;

    if status.success() {
        println!("Removed autostart: HKCU\\...\\Run\\Julie Server");
    }
    Ok(())
}
