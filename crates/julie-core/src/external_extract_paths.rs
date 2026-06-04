use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalFilePath {
    pub absolute: PathBuf,
    pub relative: String,
}

pub fn normalize_external_root(root: &Path) -> Result<PathBuf> {
    let expanded = expand_tilde(root);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .context("failed to resolve current directory for external extract root")?
            .join(expanded)
    };

    absolute.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize external extract root {}",
            absolute.display()
        )
    })
}

pub fn normalize_existing_external_file(root: &Path, file: &Path) -> Result<ExternalFilePath> {
    let root = normalize_canonical_root(root)?;
    let candidate = resolve_external_candidate(&root, file);
    let canonical = candidate.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize external extract file {}",
            candidate.display()
        )
    })?;
    external_file_from_absolute(&root, canonical)
}

pub fn normalize_deleted_external_file(root: &Path, file: &Path) -> Result<ExternalFilePath> {
    let root = normalize_canonical_root(root)?;
    let candidate = resolve_external_candidate(&root, file);
    let normalized = normalize_lexical(&candidate)?;
    external_file_from_absolute(&root, normalized)
}

fn normalize_canonical_root(root: &Path) -> Result<PathBuf> {
    if root.is_absolute() && root.exists() {
        return root.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize external extract root {}",
                root.display()
            )
        });
    }
    normalize_external_root(root)
}

fn resolve_external_candidate(root: &Path, file: &Path) -> PathBuf {
    let expanded = expand_tilde(file);
    if expanded.is_absolute() {
        expanded
    } else {
        root.join(expanded)
    }
}

fn external_file_from_absolute(root: &Path, absolute: PathBuf) -> Result<ExternalFilePath> {
    let relative_path = absolute.strip_prefix(root).map_err(|_| {
        anyhow!(
            "external extract file {} is outside external extract root {}",
            absolute.display(),
            root.display()
        )
    })?;

    let relative = relative_path
        .to_str()
        .context("external extract file path contains invalid UTF-8")?
        .replace('\\', "/");

    if relative.is_empty() || relative == ".." || relative.starts_with("../") {
        return Err(anyhow!(
            "external extract file {} is outside external extract root {}",
            absolute.display(),
            root.display()
        ));
    }

    Ok(ExternalFilePath { absolute, relative })
}

fn normalize_lexical(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(anyhow!(
                        "external extract path {} escapes its root",
                        path.display()
                    ));
                }
            }
        }
    }
    Ok(normalized)
}

fn expand_tilde(path: &Path) -> PathBuf {
    PathBuf::from(shellexpand::tilde(&path.to_string_lossy()).into_owned())
}
