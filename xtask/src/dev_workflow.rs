//! Developer-only workflow commands: `dev-link` and `dev-restart`.
//!
//! These are for the Julie maintainer's local dev loop. Regular users install
//! the plugin and never run these. They assume:
//!
//! - You build the dev binaries at `target/release/julie-server`,
//!   `target/release/julie-adapter`, and `target/release/julie-daemon`.
//! - You want every installed Julie plugin variant on this machine to point at
//!   those binaries so a single `cargo build --release --bins` rebuilds for
//!   every harness.
//! - You want `dev-restart` to gracefully stop the running daemon so the
//!   adapter respawns it on the new binary without the stale-binary force-kill.
//!
//! Discovery is conservative: only the Claude Code plugin cache contains
//! bundled binaries that benefit from symlinks. Codex CLI and OpenCode register
//! an MCP server that points at a user-chosen path (per the README install
//! instructions), so the user already controls which binary those harnesses
//! run.

use std::fs;
use std::io::{self, Write};
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
    Missing,
    RealBinary,
    DifferentSymlink,
}

impl PreviousKind {
    fn label(&self) -> &'static str {
        match self {
            PreviousKind::Missing => "created missing link",
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
    let target_dir = workspace_root.join("target").join("release");
    let target_binary = target_dir.join(binary_name());

    if !dry_run {
        for binary in split_binary_names() {
            let path = target_dir.join(binary);
            if !path.exists() {
                bail!(
                    "release binary not found at {}; run `cargo build --release --bins` first",
                    path.display()
                );
            }
        }
    }

    writeln!(
        out,
        "dev-link: target = {}{}",
        target_dir.display(),
        if dry_run { " (dry run)" } else { "" }
    )?;
    writeln!(out, "          cache  = {}", cache_root.display())?;

    let mut report = DevLinkReport {
        dry_run,
        target: target_binary.clone(),
        ..Default::default()
    };

    let bin_dirs = discover_plugin_bin_dirs(cache_root, &mut report)?;
    if bin_dirs.is_empty() {
        writeln!(
            out,
            "  no candidate binary directories found under {}",
            cache_root.display()
        )?;
    }

    for bin_dir in bin_dirs {
        for binary in split_binary_names() {
            let path = bin_dir.join(binary);
            let target = target_dir.join(binary);
            match link_or_skip(&path, &target, dry_run)? {
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

/// Soft-restart the running Julie daemon.
///
/// **Default (`force == false`):** does NOT SIGTERM. The daemon's existing
/// stale-binary detection (in `stale_binary_disconnect_action` and
/// `stale_binary_accept_action`) will swap to the new binary the next time a
/// session disconnects or a new session connects. The calling MCP session
/// (e.g., the Claude Code instance the user is iterating from) stays alive.
///
/// Previously this command always SIGTERMed the daemon, which entered the
/// drain path and force-aborted in-flight requests after the drain timeout.
/// That killed the calling session because the adapter classifies transport
/// errors after `wrote_any_output: true` as `Terminal` and exits.
///
/// **`force == true`:** legacy SIGTERM behavior. Use only when no live session
/// matters (e.g., terminal-only iteration, no Claude Code running).
pub fn run_dev_restart(out: &mut impl Write, force: bool) -> Result<DevRestartReport> {
    let paths = DaemonPaths::new();
    let was_running = matches!(
        lifecycle::check_status(&paths),
        lifecycle::DaemonStatus::Running { .. }
    );

    if !was_running {
        writeln!(
            out,
            "dev-restart: daemon not running; next MCP request will spawn a fresh daemon \
             with the latest binary"
        )?;
        return Ok(DevRestartReport {
            was_running: false,
            forced: force,
            sigterm_sent: false,
        });
    }

    if force {
        writeln!(
            out,
            "dev-restart: --force given; sending SIGTERM (in-flight sessions will be drained \
             then force-aborted on timeout)"
        )?;
        lifecycle::stop_daemon(&paths)?;
        writeln!(
            out,
            "dev-restart: daemon stopped; adapter will respawn on next MCP request"
        )?;
        Ok(DevRestartReport {
            was_running: true,
            forced: true,
            sigterm_sent: true,
        })
    } else {
        writeln!(
            out,
            "dev-restart: daemon running; leaving it alive so the calling MCP session is not \
             interrupted"
        )?;
        writeln!(
            out,
            "  the daemon will auto-pick up the new binary on the next session disconnect \
             or new session connect"
        )?;
        writeln!(
            out,
            "  pass --force to SIGTERM immediately (kills in-flight sessions on drain timeout)"
        )?;
        Ok(DevRestartReport {
            was_running: true,
            forced: false,
            sigterm_sent: false,
        })
    }
}

#[derive(Debug)]
pub struct DevRestartReport {
    pub was_running: bool,
    pub forced: bool,
    pub sigterm_sent: bool,
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

fn split_binary_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["julie-server.exe", "julie-adapter.exe", "julie-daemon.exe"]
    } else {
        &["julie-server", "julie-adapter", "julie-daemon"]
    }
}

/// Walk `<cache_root>/<version>/bin/<arch>` and collect installed plugin binary
/// directories that need to mirror the local split binaries.
fn discover_plugin_bin_dirs(cache_root: &Path, report: &mut DevLinkReport) -> Result<Vec<PathBuf>> {
    let mut results = Vec::new();

    if !cache_root.is_dir() {
        report.not_found_dirs.push(cache_root.to_path_buf());
        return Ok(results);
    }

    for version_entry in
        fs::read_dir(cache_root).with_context(|| format!("reading {}", cache_root.display()))?
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
            if arch_entry.file_name().to_string_lossy() == "archives" {
                continue;
            }

            let arch_dir = arch_entry.path();
            let has_known_binary = split_binary_names().iter().any(|binary| {
                let candidate = arch_dir.join(binary);
                candidate.exists() || candidate.is_symlink()
            });
            if has_known_binary {
                results.push(arch_dir);
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
    let previous_kind = match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
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
            let have = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
            if want == have {
                return Ok(LinkOutcome::AlreadyLinked);
            }
            PreviousKind::DifferentSymlink
        }
        Ok(meta) if meta.file_type().is_file() => PreviousKind::RealBinary,
        Ok(_) => {
            return Ok(LinkOutcome::Skipped(
                "not a regular file or symlink".to_string(),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => PreviousKind::Missing,
        Err(error) => {
            return Err(error).with_context(|| format!("reading metadata of {}", path.display()));
        }
    };

    if dry_run {
        return Ok(LinkOutcome::Linked(LinkAction {
            path: path.to_path_buf(),
            previous_kind,
        }));
    }

    if previous_kind != PreviousKind::Missing {
        fs::remove_file(path).with_context(|| format!("removing {}", path.display()))?;
    }
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

    fn make_fake_release_bins(workspace: &Path) -> io::Result<PathBuf> {
        let target = workspace.join("target").join("release");
        fs::create_dir_all(&target)?;
        for binary in split_binary_names() {
            File::create(target.join(binary))?;
        }
        Ok(target)
    }

    #[test]
    fn dev_link_creates_split_binary_symlinks_for_existing_cache_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        let cache = tmp.path().join("cache").join("julie-plugin").join("julie");
        let target = make_fake_release_bins(&workspace).unwrap();

        let cache_server = make_fake_cache(&cache, "7.9.3", "aarch64-apple-darwin").unwrap();
        let cache_bin_dir = cache_server.parent().unwrap().to_path_buf();

        let mut out = Vec::new();
        let report = run_dev_link(&workspace, false, &cache, &mut out).expect("dev-link succeeds");

        assert_eq!(report.linked.len(), 3, "all split binaries are linked");
        assert_eq!(report.already_linked.len(), 0);
        assert_eq!(report.skipped.len(), 0);

        for binary in split_binary_names() {
            let cache_bin = cache_bin_dir.join(binary);
            let meta = fs::symlink_metadata(&cache_bin).unwrap();
            assert!(meta.file_type().is_symlink(), "{binary} is a symlink");
            assert_eq!(fs::read_link(&cache_bin).unwrap(), target.join(binary));
        }
    }

    #[test]
    fn dev_link_ignores_archives_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        let cache = tmp.path().join("cache").join("julie-plugin").join("julie");
        make_fake_release_bins(&workspace).unwrap();

        let cache_server = make_fake_cache(&cache, "7.9.3", "aarch64-apple-darwin").unwrap();
        let version_bin_dir = cache_server
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let archives_dir = version_bin_dir.join("archives");
        fs::create_dir_all(&archives_dir).unwrap();

        let mut out = Vec::new();
        let report = run_dev_link(&workspace, false, &cache, &mut out).expect("dev-link succeeds");

        assert_eq!(
            report.linked.len(),
            3,
            "only the architecture dir is linked"
        );
        for binary in split_binary_names() {
            assert!(
                !archives_dir.join(binary).exists() && !archives_dir.join(binary).is_symlink(),
                "archives directory must not receive {binary}"
            );
        }
    }

    #[test]
    fn dev_link_replaces_real_binary_with_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        let cache = tmp.path().join("cache").join("julie-plugin").join("julie");

        // Workspace with a release binary stand-in
        let target = make_fake_release_bins(&workspace).unwrap();
        let target_bin = target.join(binary_name());

        let cache_bin = make_fake_cache(&cache, "7.8.1", "aarch64-apple-darwin").unwrap();

        let mut out = Vec::new();
        let report = run_dev_link(&workspace, false, &cache, &mut out).expect("dev-link succeeds");

        assert_eq!(report.linked.len(), 3, "all split binaries linked");
        let server_action = report
            .linked
            .iter()
            .find(|action| action.path == cache_bin)
            .expect("server binary link reported");
        assert_eq!(server_action.previous_kind, PreviousKind::RealBinary);
        assert_eq!(
            report
                .linked
                .iter()
                .filter(|action| action.previous_kind == PreviousKind::Missing)
                .count(),
            2,
            "adapter and daemon links were created from missing cache entries"
        );
        assert_eq!(report.already_linked.len(), 0);
        assert_eq!(report.skipped.len(), 0);

        let meta = fs::symlink_metadata(&cache_bin).unwrap();
        assert!(
            meta.file_type().is_symlink(),
            "cache entry is now a symlink"
        );
        let link_target = fs::read_link(&cache_bin).unwrap();
        assert_eq!(link_target, target_bin, "symlink points at dev binary");
    }

    #[test]
    fn dev_link_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        let cache = tmp.path().join("cache").join("julie-plugin").join("julie");

        make_fake_release_bins(&workspace).unwrap();

        let _cache_bin = make_fake_cache(&cache, "7.8.1", "aarch64-apple-darwin").unwrap();

        let mut out = Vec::new();
        run_dev_link(&workspace, false, &cache, &mut out).unwrap();

        let mut out = Vec::new();
        let report = run_dev_link(&workspace, false, &cache, &mut out).unwrap();
        assert_eq!(report.linked.len(), 0, "second run links nothing");
        assert_eq!(
            report.already_linked.len(),
            3,
            "second run sees existing symlinks"
        );
    }

    #[test]
    fn dev_link_dry_run_does_not_modify_filesystem() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        let cache = tmp.path().join("cache").join("julie-plugin").join("julie");

        let cache_bin = make_fake_cache(&cache, "7.8.1", "aarch64-apple-darwin").unwrap();

        let mut out = Vec::new();
        let report = run_dev_link(&workspace, true, &cache, &mut out)
            .expect("dry-run succeeds even without release binary");

        assert_eq!(report.linked.len(), 3);
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
        make_fake_release_bins(&workspace).unwrap();

        let cache = tmp.path().join("does").join("not").join("exist");

        let mut out = Vec::new();
        let report = run_dev_link(&workspace, false, &cache, &mut out).unwrap();
        assert_eq!(report.linked.len(), 0);
        assert_eq!(report.not_found_dirs.len(), 1);
    }
}
