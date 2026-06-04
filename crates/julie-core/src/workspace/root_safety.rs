use std::path::{Path, PathBuf};

use anyhow::Result;

pub fn reject_sensitive_workspace_root(path: &Path) -> Result<()> {
    if is_sensitive_workspace_root(path) {
        anyhow::bail!(
            "Refusing to use sensitive system path {} as Julie workspace root. Choose a project directory or pass a narrower path.",
            path.display()
        );
    }
    Ok(())
}

pub fn reject_sensitive_cwd_workspace_root(path: &Path) -> Result<()> {
    if is_sensitive_workspace_root(path) {
        anyhow::bail!(
            "Refusing to use sensitive system path {} as Julie primary workspace from process cwd. \
Set JULIE_WORKSPACE or launch the MCP server from a project directory.",
            path.display()
        );
    }
    Ok(())
}

pub fn is_sensitive_workspace_root(path: &Path) -> bool {
    if path.parent().is_none() {
        return true;
    }

    let target_norm = normalize_sensitive_root_for_compare(path);
    let target_canon = path
        .canonicalize()
        .ok()
        .map(|p| normalize_sensitive_root_for_compare(&p));

    let forbidden = sensitive_root_candidates();

    forbidden.iter().any(|candidate| {
        let candidate_norm = normalize_sensitive_root_for_compare(candidate);
        if candidate_norm == target_norm {
            return true;
        }
        let candidate_canon = candidate
            .canonicalize()
            .ok()
            .map(|p| normalize_sensitive_root_for_compare(&p));
        match (&target_canon, &candidate_canon) {
            (Some(a), Some(b)) if a == b => true,
            (Some(a), _) if a == &candidate_norm => true,
            (_, Some(b)) if b == &target_norm => true,
            _ => false,
        }
    })
}

/// Build the list of "obviously sensitive" workspace roots to reject. The list
/// combines `dirs::home_dir()` with platform-specific system directories. On
/// macOS we also include `/private/var/root` explicitly so the rejection does
/// not depend on `canonicalize()` succeeding at the comparison site. On Windows
/// we honour env vars (SystemDrive, ProgramFiles, etc.) so installs on drives
/// other than `C:` are still caught.
pub fn sensitive_root_candidates() -> Vec<PathBuf> {
    let mut forbidden: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        forbidden.push(home);
    }
    #[cfg(target_os = "macos")]
    {
        forbidden.push(PathBuf::from("/Users"));
        forbidden.push(PathBuf::from("/var/root"));
        forbidden.push(PathBuf::from("/private/var/root"));
    }
    #[cfg(target_os = "linux")]
    {
        forbidden.push(PathBuf::from("/home"));
        forbidden.push(PathBuf::from("/root"));
    }
    #[cfg(windows)]
    {
        push_windows_sensitive_candidates(&mut forbidden);
    }
    forbidden
}

#[cfg(windows)]
fn push_windows_sensitive_candidates(forbidden: &mut Vec<PathBuf>) {
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let drive_root = format!("{}\\", system_drive.trim_end_matches('\\'));
    forbidden.push(PathBuf::from(format!("{}Users", drive_root)));
    forbidden.push(PathBuf::from(format!("{}Windows", drive_root)));
    forbidden.push(PathBuf::from(format!("{}Windows\\System32", drive_root)));
    forbidden.push(PathBuf::from(format!("{}Program Files", drive_root)));
    forbidden.push(PathBuf::from(format!("{}Program Files (x86)", drive_root)));
    forbidden.push(PathBuf::from(format!("{}ProgramData", drive_root)));
    for key in [
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "ProgramData",
        "PUBLIC",
    ] {
        if let Ok(p) = std::env::var(key) {
            if !p.is_empty() {
                forbidden.push(PathBuf::from(p));
            }
        }
    }
}

fn normalize_sensitive_root_for_compare(path: &Path) -> String {
    let raw = path.to_string_lossy().to_string();
    let normalized = raw.replace('\\', "/");
    let normalized = normalized.trim_end_matches('/');
    if normalized.is_empty() {
        "/".to_string()
    } else if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized.to_string()
    }
}
