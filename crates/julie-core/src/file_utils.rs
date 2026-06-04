use anyhow::Result;
use std::path::Path;

/// Check if a file has a supported language extension.
///
/// Delegates to `julie_extractors::language::detect_language_from_extension()`.
pub fn is_supported_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(julie_extractors::language::detect_language_from_extension)
        .is_some()
}

/// Read file content safely
pub fn read_file_content(path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(path)?)
}

/// Heuristic: does this file look like a text file by content?
///
/// Reads the first 512 bytes. Rejects files with null bytes or fewer than
/// 80% printable ASCII/UTF-8 bytes. Returns false for unreadable or empty
/// files.
pub fn is_likely_text_file(file_path: &Path) -> bool {
    use std::io::Read;

    let mut file = match std::fs::File::open(file_path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut buffer = [0u8; 512];
    let bytes_read = match file.read(&mut buffer) {
        Ok(n) => n,
        Err(_) => return false,
    };
    if bytes_read == 0 {
        return false;
    }
    if buffer[..bytes_read].contains(&0) {
        return false;
    }
    let printable = buffer[..bytes_read]
        .iter()
        .filter(|&&b| (32..=126).contains(&b) || b == 9 || b == 10 || b == 13 || b >= 128)
        .count();
    (printable as f64 / bytes_read as f64) > 0.8
}

/// Secure path resolution that prevents directory traversal attacks.
///
/// Resolves a file path relative to a workspace root and ensures the final
/// resolved path is within the workspace boundaries.
pub fn secure_path_resolution(
    file_path: &str,
    workspace_root: &Path,
) -> Result<std::path::PathBuf> {
    use std::path::{Component, PathBuf};

    let candidate = Path::new(file_path);

    let canonical_workspace_root = workspace_root
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("Workspace root does not exist: {}", e))?;

    let resolved = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        canonical_workspace_root.join(candidate)
    };

    let mut normalized = PathBuf::new();
    for component in resolved.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(anyhow::anyhow!(
                        "Security: Path traversal attempt blocked. Path must be within workspace."
                    ));
                }
            }
            Component::Normal(name) => normalized.push(name),
        }
    }

    let final_path = if normalized.exists() {
        normalized
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("Failed to canonicalize existing path: {}", e))?
    } else {
        if let Some(parent) = normalized.parent() {
            if parent.exists() {
                let canonical_parent = parent
                    .canonicalize()
                    .map_err(|e| anyhow::anyhow!("Parent directory does not exist: {}", e))?;
                if !canonical_parent.starts_with(&canonical_workspace_root) {
                    return Err(anyhow::anyhow!(
                        "Security: Path traversal attempt blocked. Path must be within workspace."
                    ));
                }
            }
        }
        normalized
    };

    if !final_path.starts_with(&canonical_workspace_root) {
        return Err(anyhow::anyhow!(
            "Security: Path traversal attempt blocked. Path must be within workspace."
        ));
    }

    Ok(final_path)
}
