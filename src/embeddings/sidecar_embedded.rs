//! Compile-time embedded sidecar source and extraction logic.
//!
//! This module embeds only the minimal files needed to bootstrap the Python
//! sidecar from a binary distribution:
//! - `sidecar/` package directory (~4 .py files)
//! - `pyproject.toml` (dependency manifest)
//!
//! Previous versions embedded the entire `python/embeddings_sidecar/` tree
//! which pulled in `uv.lock` (248KB), `tests/`, `__pycache__/`, etc.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use include_dir::{Dir, include_dir};

use super::sidecar_supervisor::INSTALL_MARKER_VERSION;

/// Only the `sidecar/` Python package — no tests, __pycache__, or lock files.
static EMBEDDED_SIDECAR_PACKAGE: Dir =
    include_dir!("$CARGO_MANIFEST_DIR/python/embeddings_sidecar/sidecar");

/// The `pyproject.toml` needed for `pip install --editable`.
static EMBEDDED_PYPROJECT_TOML: &str =
    include_str!("../../python/embeddings_sidecar/pyproject.toml");

const EMBEDDED_VERSION_MARKER: &str = ".embedded-version";

/// Return the cache directory path where embedded sidecar source is extracted.
pub(crate) fn managed_sidecar_source_path() -> PathBuf {
    super::sidecar_supervisor::managed_cache_base_dir()
        .join("embeddings")
        .join("sidecar")
        .join("source")
}

/// Extract the compile-time embedded sidecar source to `target_dir`.
///
/// Writes `pyproject.toml` at the top level and the `sidecar/` package as a
/// subdirectory. Skips extraction if the version marker already matches
/// [`INSTALL_MARKER_VERSION`].
pub(crate) fn extract_embedded_sidecar(target_dir: &Path) -> Result<()> {
    let marker_path = target_dir.join(EMBEDDED_VERSION_MARKER);

    // Check version marker — skip if up-to-date
    if marker_path.is_file() {
        if let Ok(contents) = std::fs::read_to_string(&marker_path) {
            if contents.trim() == INSTALL_MARKER_VERSION {
                return Ok(());
            }
        }
    }

    // Ensure target directory exists
    std::fs::create_dir_all(target_dir)
        .with_context(|| format!("creating target directory {}", target_dir.display()))?;

    // Write pyproject.toml
    std::fs::write(target_dir.join("pyproject.toml"), EMBEDDED_PYPROJECT_TOML)
        .context("writing embedded pyproject.toml")?;

    // Extract sidecar/ package
    let sidecar_target = target_dir.join("sidecar");
    std::fs::create_dir_all(&sidecar_target)
        .with_context(|| format!("creating sidecar directory {}", sidecar_target.display()))?;

    extract_dir_recursive(&EMBEDDED_SIDECAR_PACKAGE, &sidecar_target)
        .context("extracting embedded sidecar package")?;

    // Write version marker
    std::fs::write(&marker_path, INSTALL_MARKER_VERSION)
        .context("writing embedded version marker")?;

    Ok(())
}

/// Recursively extract files from an embedded [`Dir`] into `target`.
///
/// File paths from the embedded dir are joined onto `target`, preserving
/// the relative structure. `__pycache__` directories are skipped since
/// `include_dir!` captures them at compile time if they exist on disk.
fn extract_dir_recursive(dir: &Dir, target: &Path) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(sub_dir) => {
                let dir_name = sub_dir
                    .path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                if dir_name == "__pycache__" {
                    continue;
                }

                let sub_target = target.join(sub_dir.path());
                std::fs::create_dir_all(&sub_target).with_context(|| {
                    format!("creating directory {}", sub_target.display())
                })?;
                extract_dir_recursive(sub_dir, target)?;
            }
            include_dir::DirEntry::File(file) => {
                let file_target = target.join(file.path());
                if let Some(parent) = file_target.parent() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!("creating parent directory {}", parent.display())
                    })?;
                }
                std::fs::write(&file_target, file.contents()).with_context(|| {
                    format!("writing file {}", file_target.display())
                })?;
            }
        }
    }
    Ok(())
}
