//! Path conversion utilities shared across the Julie workspace.
//!
//! Lives in `julie-core` (the bottom leaf crate) so that `julie-core::database`
//! and other sibling crates can call these helpers without depending on the full
//! `julie` crate. `julie::utils::paths` re-exports the public surface so all
//! existing `crate::utils::paths::*` call sites compile unchanged.

use anyhow::{Context, Result};
use std::path::{MAIN_SEPARATOR, Path, PathBuf};

use crate::workspace_errors::{WorkspaceResolutionFailure, WorkspaceResolutionFailureKind};

// ──────────────────────────────────────────────────────────────────────────────
// strip_unc_prefix
// ──────────────────────────────────────────────────────────────────────────────

/// Strip the Windows `\\?\` extended-length (UNC) prefix for path comparison.
///
/// `std::fs::canonicalize()` returns paths with this prefix on Windows, but
/// non-canonical paths do not have it. Leaving it in place makes `strip_prefix`
/// fail even when one path is genuinely nested under the other. On non-Windows
/// targets this is a no-op clone.
///
/// Exposed as `pub` so `julie::utils::paths::relative_within_workspace` (which
/// stays in the main crate) can re-import it without duplication.
pub fn strip_unc_prefix(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let path_str = path.to_string_lossy();
        if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
        path.to_path_buf()
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// to_relative_unix_style (+ private helpers)
// ──────────────────────────────────────────────────────────────────────────────

/// Convert an absolute path to a relative Unix-style path (with `/` separators).
///
/// Strips the workspace root prefix and normalises all path separators to
/// forward slashes, regardless of platform.
///
/// # Arguments
/// * `absolute`       — The absolute path to convert.
/// * `workspace_root` — The workspace root directory.
///
/// # Returns
/// * `Ok(String)` — Relative Unix-style path (e.g. `"src/tools/search.rs"`).
/// * `Err`        — If the file is not within the workspace.
///
/// # Token savings
/// - Windows UNC: `\\?\C:\Users\murphy\source\julie\src\tools\search.rs` (70 chars)
/// - Relative Unix: `src/tools/search.rs` (21 chars) — ~70% characters, ~60% tokens
pub fn to_relative_unix_style(absolute: &Path, workspace_root: &Path) -> Result<String> {
    // 🔥 CRITICAL: Try to canonicalize both paths to handle symlinks (e.g., /var -> /private/var on macOS)
    // If canonicalization fails (path doesn't exist), fall back to original paths
    let (path_to_use, root_to_use) = match (absolute.canonicalize(), workspace_root.canonicalize())
    {
        (Ok(canonical_abs), Ok(canonical_root)) => {
            // Both paths can be canonicalized - use canonical versions
            (canonical_abs, canonical_root)
        }
        _ => {
            // One or both failed - use original paths for consistency
            (absolute.to_path_buf(), workspace_root.to_path_buf())
        }
    };

    let normalized_path = strip_unc_prefix(&path_to_use);
    let normalized_root = strip_unc_prefix(&root_to_use);

    // Strip workspace prefix
    let relative = match normalized_path.strip_prefix(&normalized_root) {
        Ok(relative) => relative,
        Err(error) => {
            if let Some(relative) =
                relative_by_normalized_string(&normalized_path, &normalized_root)
            {
                return Ok(relative);
            }

            return Err(error).with_context(|| {
                format!(
                    "File path '{}' is not within workspace root '{}'",
                    normalized_path.display(),
                    normalized_root.display()
                )
            });
        }
    };

    // Convert to string and normalize separators to Unix-style
    let path_str = relative.to_str().context("Path contains invalid UTF-8")?;

    // Replace platform-specific separators with Unix-style /
    // On Unix, MAIN_SEPARATOR is already '/', so this is a no-op
    // On Windows, this converts '\' to '/'
    let unix_style = if MAIN_SEPARATOR == '\\' {
        path_str.replace('\\', "/")
    } else {
        path_str.to_string()
    };

    Ok(unix_style)
}

fn relative_by_normalized_string(path: &Path, root: &Path) -> Option<String> {
    let path = path.to_string_lossy().replace('\\', "/");
    let root = root.to_string_lossy().replace('\\', "/");
    let root = root.trim_end_matches('/');

    if root.is_empty() {
        return None;
    }

    strip_normalized_prefix(&path, root).map(ToOwned::to_owned)
}

#[cfg(windows)]
fn strip_normalized_prefix<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    let path_lower = path.to_ascii_lowercase();
    let root_lower = root.to_ascii_lowercase();

    if path_lower == root_lower {
        return Some("");
    }

    let prefix = format!("{root_lower}/");
    path_lower
        .starts_with(&prefix)
        .then(|| &path[root.len() + 1..])
}

#[cfg(not(windows))]
fn strip_normalized_prefix<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    if path == root {
        return Some("");
    }

    path.strip_prefix(&format!("{root}/"))
}

// ──────────────────────────────────────────────────────────────────────────────
// resolve_workspace_file_input
// ──────────────────────────────────────────────────────────────────────────────

/// The two path forms that tool handlers need after resolving a file input.
#[derive(Debug)]
pub struct WorkspaceFileInputResolution {
    pub absolute_path: PathBuf,
    pub relative_query_path: String,
    pub canonicalized: bool,
}

/// Resolve a tool file input into the two path forms tool handlers need.
///
/// Tool inputs may be absolute, relative, contain `.` / `..`, or point at a
/// file that does not exist yet. This canonicalizes the input path when
/// possible, otherwise keeps the absolute candidate path, then computes a
/// relative Unix-style path for database queries.
///
/// # Strict contract — no raw-input fallback
///
/// If the resolved absolute path is **outside the workspace root**, this
/// function returns an `Err` wrapping [`WorkspaceResolutionFailure`] with
/// kind [`WorkspaceResolutionFailureKind::FileOutsideWorkspace`]. Callers
/// MUST propagate the error — they must not fall back to raw string
/// normalization of the input, which would let outside-workspace paths
/// silently reach the database as if they were workspace-relative.
///
/// At the MCP boundary, route this error through
/// `classify_tool_failure` in `handler::tools::error`, which downcasts to
/// [`WorkspaceResolutionFailure`] and surfaces the result as
/// `McpError::invalid_params` so the user sees a clear "outside workspace"
/// message instead of an opaque internal error.
pub fn resolve_workspace_file_input(
    input: &str,
    workspace_root: &Path,
) -> Result<WorkspaceFileInputResolution> {
    let input_path = Path::new(input);
    let absolute_candidate = if input_path.is_absolute() {
        input_path.to_path_buf()
    } else {
        workspace_root.join(input_path)
    };

    let (absolute_path, canonicalized) = match absolute_candidate.canonicalize() {
        Ok(canonical) => (canonical, true),
        Err(_) => (absolute_candidate, false),
    };

    let relative_query_path =
        to_relative_unix_style(&absolute_path, workspace_root).map_err(|_| {
            WorkspaceResolutionFailure::new(
                WorkspaceResolutionFailureKind::FileOutsideWorkspace,
                format!("file path is outside the workspace: {}", input),
            )
        })?;

    Ok(WorkspaceFileInputResolution {
        absolute_path,
        relative_query_path,
        canonicalized,
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// VCS_ROOT_MARKERS + RegistryPaths
// (moved from the root `julie` crate so that `julie-runtime` can reference
//  workspace-root discovery and the Julie-home guard without depending upward)
// ──────────────────────────────────────────────────────────────────────────────

/// Marker directories/files that identify a *version-control repository root*.
///
/// In their modern layouts these are outer boundaries: each VCS writes its
/// metadata once, at the checkout root, not in every sub-directory. That
/// non-nesting property is what makes them safe to treat as "stop the upward
/// workspace-root walk here" boundaries — unlike build manifests (`Cargo.toml`,
/// `package.json`, `go.mod`, `pyproject.toml`), which appear at BOTH a monorepo
/// root AND every member package and would falsely halt discovery inside a
/// Cargo-workspace member crate / npm-workspace package.
///
/// Caveat: Subversion BEFORE 1.7 (2011) wrote a `.svn` dir in every versioned
/// sub-directory. On such a (now-obsolete) checkout the discovery walk would stop
/// at the first sub-directory it visits. Modern SVN (1.7+) keeps a single root
/// `.svn`. We accept this rare residual rather than drop SVN-root detection.
pub const VCS_ROOT_MARKERS: &[&str] = &[".git", ".hg", ".svn", ".jj", ".bzr", "_darcs"];

/// Centralized path resolution for Julie registry and runtime infrastructure.
///
/// All Julie home paths derive from `julie_home`. The default is `~/.julie/`,
/// which can be overridden by setting the `JULIE_HOME` environment variable to
/// an absolute path. An empty `JULIE_HOME` is rejected as a misconfiguration
/// (rather than silently falling back to `~/.julie/`).
#[derive(Clone)]
pub struct RegistryPaths {
    julie_home: PathBuf,
}

impl RegistryPaths {
    /// Create using the resolved Julie home directory.
    ///
    /// Resolution order:
    /// 1. `JULIE_HOME` set and non-empty → use verbatim (must be absolute).
    /// 2. `JULIE_HOME` set but empty → `Err(InvalidInput)`.
    /// 3. `JULIE_HOME` set but relative → `Err(InvalidInput)`.
    /// 4. Otherwise → `dirs::home_dir().join(".julie")`.
    pub fn try_new() -> Result<Self, std::io::Error> {
        if let Some(value) = std::env::var_os("JULIE_HOME") {
            if value.is_empty() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "JULIE_HOME is set but empty",
                ));
            }
            let candidate = PathBuf::from(value);
            if !candidate.is_absolute() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "JULIE_HOME must be an absolute path (got '{}')",
                        candidate.display()
                    ),
                ));
            }
            return Ok(Self {
                julie_home: candidate,
            });
        }

        let home = dirs::home_dir().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not determine home directory",
            )
        })?;
        Ok(Self {
            julie_home: home.join(".julie"),
        })
    }

    /// Create with default home (`~/.julie/`). Panics if home cannot be determined.
    pub fn new() -> Self {
        Self::try_new().expect("Could not determine home directory")
    }

    /// Create with an explicit home path (for testing or `JULIE_HOME` override).
    pub fn with_home(julie_home: PathBuf) -> Self {
        Self { julie_home }
    }

    /// Root directory for all Julie daemon state.
    pub fn julie_home(&self) -> PathBuf {
        self.julie_home.clone()
    }

    /// Check whether `candidate` resolves to the same directory as the configured home.
    pub fn is_julie_home(&self, candidate: &Path) -> bool {
        match (candidate.canonicalize(), self.julie_home.canonicalize()) {
            (Ok(c), Ok(h)) => c == h,
            _ => candidate == self.julie_home.as_path(),
        }
    }

    /// Check whether `candidate` lives under (or is equal to) the configured home.
    pub fn is_under_julie_home(&self, candidate: &Path) -> bool {
        let candidate_canon = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.to_path_buf());
        let home_canon = self
            .julie_home
            .canonicalize()
            .unwrap_or_else(|_| self.julie_home.clone());
        candidate_canon.starts_with(&home_canon)
    }

    /// Walker-safety check: returns true if `candidate` matches either the
    /// configured Julie home or the conventional `~/.julie` default.
    ///
    /// Used by workspace-root discovery to guard against walking into daemon
    /// state even when `JULIE_HOME` is misconfigured.
    pub fn is_any_known_julie_home(candidate: &Path) -> bool {
        if let Ok(configured) = Self::try_new() {
            if configured.is_julie_home(candidate) {
                return true;
            }
        }
        if let Some(default_home) = dirs::home_dir() {
            let default = default_home.join(".julie");
            if Self::with_home(default).is_julie_home(candidate) {
                return true;
            }
        }
        false
    }

    /// Directory containing all workspace indexes.
    pub fn indexes_dir(&self) -> PathBuf {
        self.julie_home.join("indexes")
    }

    /// Directory for a specific workspace's index (SQLite + Tantivy).
    pub fn workspace_index_dir(&self, workspace_id: &str) -> PathBuf {
        self.indexes_dir().join(workspace_id)
    }

    /// SQLite database path for a workspace.
    pub fn workspace_db_path(&self, workspace_id: &str) -> PathBuf {
        self.workspace_index_dir(workspace_id)
            .join("db")
            .join("symbols.db")
    }

    /// Tantivy index directory for a workspace.
    pub fn workspace_tantivy_path(&self, workspace_id: &str) -> PathBuf {
        self.workspace_index_dir(workspace_id).join("tantivy")
    }

    /// Per-workspace leader-election lock (Phase 3c).
    ///
    /// Returns `indexes/{workspace_id}/leader.lock` — a direct sibling of the
    /// `db/` and `tantivy/` subdirs inside the workspace index directory.
    ///
    /// Placing the lock here means it survives a Tantivy-dir rebuild (which
    /// atomically swaps the `tantivy/` tree, not the parent dir) and is
    /// uniquely scoped per workspace.  It is intentionally distinct from the
    /// Tantivy rebuild lock (`tantivy.julie-rebuild.lock`) so the non-blocking
    /// leader lock can never alias the blocking rebuild lock.
    pub fn workspace_leader_lock(&self, workspace_id: &str) -> PathBuf {
        self.workspace_index_dir(workspace_id).join("leader.lock")
    }

    /// FNV-1a hash of `julie_home`, used for Windows embedding host pipe names.
    #[cfg(windows)]
    fn julie_home_hash(&self) -> u64 {
        let path_str = self.julie_home.to_string_lossy();
        let mut hash: u64 = 14695981039346656037;
        for byte in path_str.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
        hash
    }

    /// Unix domain socket for the resident embedding-host (Phase 3b).
    ///
    /// One host per `$JULIE_HOME` serves embeddings to every Julie process,
    /// mirroring the process-global lifetime of the daemon's `EmbeddingService`.
    #[cfg(unix)]
    pub fn embedding_host_socket(&self) -> PathBuf {
        self.julie_home.join("embedding-host.sock")
    }

    /// Kernel-held singleton lock for the running embedding-host (Phase 3b).
    ///
    /// Ensures exactly one host process per `$JULIE_HOME`; a second launch
    /// fails to acquire the lock and yields to the incumbent.
    pub fn embedding_host_lock(&self) -> PathBuf {
        self.julie_home.join("embedding-host.lock")
    }

    /// Named pipe for the resident embedding-host (Windows, Phase 3b).
    #[cfg(windows)]
    pub fn embedding_host_pipe_name(&self) -> String {
        format!(
            "\\\\.\\pipe\\julie-embedding-host-{:016x}",
            self.julie_home_hash()
        )
    }

    /// Per-project log directory (written by daemon, scoped to project).
    pub fn project_log_dir(&self, project_root: &Path) -> PathBuf {
        project_root.join(".julie").join("logs")
    }

    /// Persistent daemon state database.
    pub fn registry_db(&self) -> PathBuf {
        self.julie_home.join("registry.db")
    }

    /// Ensure `julie_home` and `indexes` directories exist.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.indexes_dir())
    }
}

impl Default for RegistryPaths {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// relative_within_workspace
// (moved from `julie::utils::paths` so that `julie-runtime` can reference it
//  without depending upward on the full `julie` crate)
// ──────────────────────────────────────────────────────────────────────────────

/// Strip `workspace_root` from `path`, tolerating symlinked workspace roots
/// (e.g. macOS `/tmp` → `/private/tmp`, `/var` → `/private/var`) and deleted
/// leaf paths.
///
/// The file watcher receives event paths from `notify`, which on macOS reports
/// canonical (symlink-resolved) paths via FSEvents even when the workspace was
/// registered under a symlinked root. A naive `path.strip_prefix(workspace_root)`
/// then fails, and callers that fall back to inspecting the *absolute* path hit
/// false positives from ancestor directory names — e.g. `/private/tmp/proj/…`
/// contains the blacklisted component `tmp`, so delete/modify events for gone
/// files get silently dropped and leave orphaned symbols in the index.
///
/// Resolution order:
/// 1. Direct strip — both paths already share a symlink form (the common case:
///    canonical project root + canonical event paths, or raw root + raw paths).
/// 2. Canonicalize the root (which always exists, even when the leaf was
///    deleted) and retry against the candidate as-is. This recovers the relative
///    path whenever the candidate is already canonical, which is what `notify`
///    emits on macOS — and crucially does not require the leaf to exist.
/// 3. Canonicalize the candidate's current form (existing files only) and retry,
///    covering the reverse case where the candidate is raw but the root is
///    canonical (Windows junctions / symlinked candidates).
///
/// Returns the workspace-relative path, or `None` when `path` is genuinely not
/// inside `workspace_root`.
pub fn relative_within_workspace(path: &Path, workspace_root: &Path) -> Option<PathBuf> {
    if let Ok(rel) = path.strip_prefix(workspace_root) {
        return Some(rel.to_path_buf());
    }

    let canonical_root = match workspace_root.canonicalize() {
        Ok(root) => strip_unc_prefix(&root),
        Err(_) => return None,
    };

    let candidate = strip_unc_prefix(path);
    if let Ok(rel) = candidate.strip_prefix(&canonical_root) {
        return Some(rel.to_path_buf());
    }

    if let Ok(canonical_candidate) = path.canonicalize() {
        let canonical_candidate = strip_unc_prefix(&canonical_candidate);
        if let Ok(rel) = canonical_candidate.strip_prefix(&canonical_root) {
            return Some(rel.to_path_buf());
        }
    }

    None
}
