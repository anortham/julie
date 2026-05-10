use std::path::{Path, PathBuf};

use anyhow::Result;

pub(crate) fn reject_sensitive_workspace_root(path: &Path) -> Result<()> {
    if is_sensitive_workspace_root(path) {
        anyhow::bail!(
            "Refusing to use sensitive system path {} as Julie workspace root. Choose a project directory or pass a narrower path.",
            path.display()
        );
    }
    Ok(())
}

pub(crate) fn reject_sensitive_cwd_workspace_root(path: &Path) -> Result<()> {
    if is_sensitive_workspace_root(path) {
        anyhow::bail!(
            "Refusing to use sensitive system path {} as Julie primary workspace from process cwd. \
Set JULIE_WORKSPACE or launch the MCP server from a project directory.",
            path.display()
        );
    }
    Ok(())
}

pub(crate) fn is_sensitive_workspace_root(path: &Path) -> bool {
    if path.parent().is_none() {
        return true;
    }

    let target = normalize_sensitive_root_for_compare(path);
    let mut forbidden: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        forbidden.push(home);
    }
    #[cfg(target_os = "macos")]
    {
        forbidden.push(PathBuf::from("/Users"));
        forbidden.push(PathBuf::from("/var/root"));
    }
    #[cfg(target_os = "linux")]
    {
        forbidden.push(PathBuf::from("/home"));
        forbidden.push(PathBuf::from("/root"));
    }
    #[cfg(windows)]
    {
        forbidden.push(PathBuf::from(r"C:\Users"));
        forbidden.push(PathBuf::from(r"C:\Windows"));
        forbidden.push(PathBuf::from(r"C:\Windows\System32"));
        forbidden.push(PathBuf::from(r"C:\Program Files"));
        forbidden.push(PathBuf::from(r"C:\Program Files (x86)"));
        forbidden.push(PathBuf::from(r"C:\ProgramData"));
    }

    forbidden
        .iter()
        .any(|candidate| normalize_sensitive_root_for_compare(candidate) == target)
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
