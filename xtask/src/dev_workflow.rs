//! Developer-only workflow commands: `dev-link` and `dev-restart`.
//!
//! These are for the Julie maintainer's local dev loop. Regular users install
//! the plugin and never run these. They assume:
//!
//! - You build the dev binary at `target/release/julie-server`.
//! - You want every installed Julie plugin variant on this machine to point at
//!   that one binary so a single `cargo build --release` rebuilds for every
//!   harness.
//! - You want `dev-restart` to gracefully stop the running daemon so the
//!   adapter respawns it on the new binary without the stale-binary force-kill.
//!
//! Discovery is conservative: only the Claude Code plugin cache contains a
//! bundled `julie-server` binary that benefits from a symlink. Codex CLI and
//! OpenCode register an MCP server that points at a user-chosen path (per the
//! README install instructions), so the user already controls which binary
//! those harnesses run.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use julie::daemon::lifecycle;
use julie::paths::DaemonPaths;

#[derive(Debug, Default)]
pub struct DevLinkReport {
    pub dry_run: bool,
    pub target: PathBuf,
    pub linked: Vec<LinkAction>,
    pub already_linked: Vec<PathBuf>,
    pub skipped: Vec<(PathBuf, String)>,
    pub not_found_dirs: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct LinkAction {
    pub path: PathBuf,
    pub previous_kind: PreviousKind,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PreviousKind {
    RealBinary,
    DifferentSymlink,
}

impl PreviousKind {
    fn label(&self) -> &'static str {
        match self {
            PreviousKind::RealBinary => "was real binary",
            PreviousKind::DifferentSymlink => "was different symlink",
        }
    }
}

/// Replace bundled plugin binaries with symlinks to the dev `target/release` binary.
pub fn run_dev_link(
    workspace_root: &Path,
    dry_run: bool,
    cache_root: &Path,
    out: &mut impl Write,
) -> Result<DevLinkReport> {
    let target_binary = workspace_root
        .join("target")
        .join("release")
        .join(binary_name());

    if !dry_run && !target_binary.exists() {
        bail!(
            "release binary not found at {}; run `cargo build --release` first",
            target_binary.display()
        );
    }

    writeln!(
        out,
        "dev-link: target = {}{}",
        target_binary.display(),
        if dry_run { " (dry run)" } else { "" }
    )?;
    writeln!(out, "          cache  = {}", cache_root.display())?;

    let mut report = DevLinkReport {
        dry_run,
        target: target_binary.clone(),
        ..Default::default()
    };

    let candidates = discover_plugin_binaries(cache_root, &mut report)?;
    if candidates.is_empty() {
        writeln!(
            out,
            "  no candidate binaries found under {}",
            cache_root.display()
        )?;
    }

    for path in candidates {
        match link_or_skip(&path, &target_binary, dry_run)? {
            LinkOutcome::Linked(action) => {
                writeln!(
                    out,
                    "  linked: {} ({})",
                    path.display(),
                    action.previous_kind.label()
                )?;
                report.linked.push(action);
            }
            LinkOutcome::AlreadyLinked => {
                writeln!(out, "  already-linked: {}", path.display())?;
                report.already_linked.push(path);
            }
            LinkOutcome::Skipped(reason) => {
                writeln!(out, "  skipped: {} ({reason})", path.display())?;
                report.skipped.push((path, reason));
            }
        }
    }

    writeln!(
        out,
        "\nsummary: {} linked, {} already-linked, {} skipped, {} cache subtree(s) absent",
        report.linked.len(),
        report.already_linked.len(),
        report.skipped.len(),
        report.not_found_dirs.len(),
    )?;

    Ok(report)
}

/// Gracefully stop the running Julie daemon so the adapter respawns it on the
/// freshly-built binary without going through the stale-binary force-kill path.
pub fn run_dev_restart(out: &mut impl Write) -> Result<DevRestartReport> {
    let paths = DaemonPaths::new();
    let was_running = matches!(
        lifecycle::check_status(&paths),
        lifecycle::DaemonStatus::Running { .. }
    );

    if was_running {
        writeln!(out, "dev-restart: stopping daemon (graceful SIGTERM)...")?;
    } else {
        writeln!(out, "dev-restart: daemon not running")?;
    }

    lifecycle::stop_daemon(&paths)?;

    if was_running {
        writeln!(
            out,
            "dev-restart: daemon stopped; adapter will respawn on next MCP request"
        )?;
    }

    Ok(DevRestartReport { was_running })
}

#[derive(Debug)]
pub struct DevRestartReport {
    pub was_running: bool,
}

pub fn default_cache_root() -> PathBuf {
    home_dir()
        .map(|home| home.join(".claude/plugins/cache/julie-plugin/julie"))
        .unwrap_or_else(|_| PathBuf::from(".claude/plugins/cache/julie-plugin/julie"))
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME env var not set"))
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "julie-server.exe"
    } else {
        "julie-server"
    }
}

/// Walk `<cache_root>/<version>/bin/<arch>/julie-server` and collect any
/// candidates that exist on disk.
fn discover_plugin_binaries(
    cache_root: &Path,
    report: &mut DevLinkReport,
) -> Result<Vec<PathBuf>> {
    let mut results = Vec::new();

    if !cache_root.is_dir() {
        report.not_found_dirs.push(cache_root.to_path_buf());
        return Ok(results);
    }

    for version_entry in fs::read_dir(cache_root)
        .with_context(|| format!("reading {}", cache_root.display()))?
    {
        let version_entry = version_entry?;
        if !version_entry.file_type()?.is_dir() {
            continue;
        }
        let bin_dir = version_entry.path().join("bin");
        if !bin_dir.is_dir() {
            continue;
        }
        for arch_entry in fs::read_dir(&bin_dir)? {
            let arch_entry = arch_entry?;
            if !arch_entry.file_type()?.is_dir() {
                continue;
            }
            let candidate = arch_entry.path().join(binary_name());
            if candidate.exists() || candidate.is_symlink() {
                results.push(candidate);
            }
        }
    }

    Ok(results)
}

enum LinkOutcome {
    Linked(LinkAction),
    AlreadyLinked,
    Skipped(String),
}

fn link_or_skip(path: &Path, target: &Path, dry_run: bool) -> Result<LinkOutcome> {
    let meta = fs::symlink_metadata(path)
        .with_context(|| format!("reading metadata of {}", path.display()))?;

    let previous_kind = if meta.file_type().is_symlink() {
        let existing = fs::read_link(path)?;
        let resolved = if existing.is_absolute() {
            existing.clone()
        } else {
            path.parent()
                .map(|p| p.join(&existing))
                .unwrap_or_else(|| existing.clone())
        };
        let want = target
            .canonicalize()
            .unwrap_or_else(|_| target.to_path_buf());
        let have = resolved
            .canonicalize()
            .unwrap_or_else(|_| resolved.clone());
        if want == have {
            return Ok(LinkOutcome::AlreadyLinked);
        }
        PreviousKind::DifferentSymlink
    } else if meta.file_type().is_file() {
        PreviousKind::RealBinary
    } else {
        return Ok(LinkOutcome::Skipped(
            "not a regular file or symlink".to_string(),
        ));
    };

    if dry_run {
        return Ok(LinkOutcome::Linked(LinkAction {
            path: path.to_path_buf(),
            previous_kind,
        }));
    }

    fs::remove_file(path).with_context(|| format!("removing {}", path.display()))?;
    create_symlink(target, path)?;

    Ok(LinkOutcome::Linked(LinkAction {
        path: path.to_path_buf(),
        previous_kind,
    }))
}

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).with_context(|| {
        format!(
            "creating symlink {} -> {}",
            link.display(),
            target.display()
        )
    })
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::windows::fs::symlink_file(target, link).with_context(|| {
        format!(
            "creating symlink {} -> {}; on Windows this may require Developer Mode or admin",
            link.display(),
            target.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io;

    fn make_fake_cache(root: &Path, version: &str, arch: &str) -> io::Result<PathBuf> {
        let bin_dir = root.join(version).join("bin").join(arch);
        fs::create_dir_all(&bin_dir)?;
        let path = bin_dir.join(binary_name());
        File::create(&path)?;
        Ok(path)
    }

    #[test]
    fn dev_link_replaces_real_binary_with_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        let cache = tmp.path().join("cache").join("julie-plugin").join("julie");

        // Workspace with a release binary stand-in
        let target = workspace.join("target").join("release");
        fs::create_dir_all(&target).unwrap();
        let target_bin = target.join(binary_name());
        File::create(&target_bin).unwrap();

        let cache_bin =
            make_fake_cache(&cache, "7.8.1", "aarch64-apple-darwin").unwrap();

        let mut out = Vec::new();
        let report =
            run_dev_link(&workspace, false, &cache, &mut out).expect("dev-link succeeds");

        assert_eq!(report.linked.len(), 1, "exactly one binary linked");
        assert_eq!(report.linked[0].previous_kind, PreviousKind::RealBinary);
        assert_eq!(report.already_linked.len(), 0);
        assert_eq!(report.skipped.len(), 0);

        let meta = fs::symlink_metadata(&cache_bin).unwrap();
        assert!(meta.file_type().is_symlink(), "cache entry is now a symlink");
        let link_target = fs::read_link(&cache_bin).unwrap();
        assert_eq!(link_target, target_bin, "symlink points at dev binary");
    }

    #[test]
    fn dev_link_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        let cache = tmp.path().join("cache").join("julie-plugin").join("julie");

        let target = workspace.join("target").join("release");
        fs::create_dir_all(&target).unwrap();
        let target_bin = target.join(binary_name());
        File::create(&target_bin).unwrap();

        let _cache_bin =
            make_fake_cache(&cache, "7.8.1", "aarch64-apple-darwin").unwrap();

        let mut out = Vec::new();
        run_dev_link(&workspace, false, &cache, &mut out).unwrap();

        let mut out = Vec::new();
        let report = run_dev_link(&workspace, false, &cache, &mut out).unwrap();
        assert_eq!(report.linked.len(), 0, "second run links nothing");
        assert_eq!(report.already_linked.len(), 1, "second run sees existing symlink");
    }

    #[test]
    fn dev_link_dry_run_does_not_modify_filesystem() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        let cache = tmp.path().join("cache").join("julie-plugin").join("julie");

        let cache_bin =
            make_fake_cache(&cache, "7.8.1", "aarch64-apple-darwin").unwrap();

        let mut out = Vec::new();
        let report = run_dev_link(&workspace, true, &cache, &mut out)
            .expect("dry-run succeeds even without release binary");

        assert_eq!(report.linked.len(), 1);
        assert!(report.dry_run);

        let meta = fs::symlink_metadata(&cache_bin).unwrap();
        assert!(
            meta.file_type().is_file() && !meta.file_type().is_symlink(),
            "cache binary still a real file after dry-run"
        );
    }

    #[test]
    fn dev_link_reports_missing_cache_root_without_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        let target = workspace.join("target").join("release");
        fs::create_dir_all(&target).unwrap();
        File::create(target.join(binary_name())).unwrap();

        let cache = tmp.path().join("does").join("not").join("exist");

        let mut out = Vec::new();
        let report = run_dev_link(&workspace, false, &cache, &mut out).unwrap();
        assert_eq!(report.linked.len(), 0);
        assert_eq!(report.not_found_dirs.len(), 1);
    }
}
