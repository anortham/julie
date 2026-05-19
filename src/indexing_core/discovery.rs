use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::external_extract::paths::normalize_existing_external_file;
use crate::tools::shared::{BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS};
use crate::tools::workspace::indexing::file_policy::{
    should_index_path_candidate, supported_extensions_for_indexing,
};
use crate::utils::walk::{WalkConfig, build_walker, try_build_single_path_walker};

const EXTERNAL_DISCOVERY_MAX_FILE_SIZE: u64 = 1024 * 1024;

pub fn discover_external_files(root: &Path, ignore_files: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let config = WalkConfig::full_index().with_ignore_files(ignore_files.to_vec());
    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    let ignore_file_paths = ignore_file_paths(root, ignore_files);
    let mut files = Vec::new();

    for entry in build_walker(root, &config).filter_map(Result::ok) {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.into_path();
        if ignore_file_paths.contains(&path) {
            continue;
        }
        if should_discover_external_file(&path, &blacklisted_exts)? {
            files.push(normalize_existing_external_file(root, &path)?.absolute);
        }
    }

    files.sort();
    files.dedup();
    Ok(files)
}

pub fn is_external_file_indexable(
    root: &Path,
    file: &Path,
    ignore_files: &[PathBuf],
) -> Result<bool> {
    let normalized = normalize_existing_external_file(root, file)?;
    if ignore_file_paths(root, ignore_files).contains(&normalized.absolute) {
        return Ok(false);
    }
    if contains_blacklisted_path_component(root, &normalized.absolute) {
        return Ok(false);
    }

    let config = WalkConfig::full_index().with_ignore_files(ignore_files.to_vec());
    if !path_visible_to_scoped_walk(root, &normalized.absolute, &config)? {
        return Ok(false);
    }

    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    should_discover_external_file(&normalized.absolute, &blacklisted_exts)
}

fn should_discover_external_file(
    file_path: &Path,
    blacklisted_exts: &HashSet<&str>,
) -> Result<bool> {
    if file_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, ".gitignore" | ".julieignore"))
    {
        return Ok(false);
    }

    if is_minified_file(file_path) {
        return Ok(false);
    }

    let extension = file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext.to_lowercase()))
        .unwrap_or_default();
    if blacklisted_exts.contains(extension.as_str()) {
        return Ok(false);
    }

    let metadata = match std::fs::metadata(file_path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(false),
    };
    if metadata.len() > EXTERNAL_DISCOVERY_MAX_FILE_SIZE {
        return Ok(false);
    }

    Ok(should_index_path_candidate(
        file_path,
        supported_extensions_for_indexing(),
    ))
}

fn is_minified_file(file_path: &Path) -> bool {
    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    file_name.contains(".min.")
        || file_name.ends_with(".min.js")
        || file_name.ends_with(".min.css")
        || file_name.ends_with(".bundle.js")
        || file_name.ends_with(".bundle.css")
}

fn contains_blacklisted_path_component(root: &Path, file_path: &Path) -> bool {
    let relative = file_path.strip_prefix(root).unwrap_or(file_path);
    relative.components().any(|component| {
        let std::path::Component::Normal(name) = component else {
            return false;
        };
        name.to_str().is_some_and(|name| {
            matches!(name, ".git" | ".julie") || BLACKLISTED_DIRECTORIES.contains(&name)
        })
    })
}

fn path_visible_to_scoped_walk(root: &Path, file_path: &Path, config: &WalkConfig) -> Result<bool> {
    let Ok(relative) = file_path.strip_prefix(root) else {
        return Ok(false);
    };
    let mut candidate = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Ok(false);
        };
        candidate.push(name);
        if !scoped_walk_includes_path(root, &candidate, config)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn scoped_walk_includes_path(root: &Path, path: &Path, config: &WalkConfig) -> Result<bool> {
    Ok(try_build_single_path_walker(root, path, config)?
        .filter_map(Result::ok)
        .any(|entry| entry.path() == path))
}

fn ignore_file_paths(root: &Path, ignore_files: &[PathBuf]) -> HashSet<PathBuf> {
    ignore_files
        .iter()
        .map(|path| {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                root.join(path)
            }
        })
        .collect()
}
