//! Token file IO primitives for the daemon bearer token.
//!
//! The daemon writes a short-lived bearer token to `~/.julie/daemon.token` at
//! startup; the adapter reads it to authenticate HTTP requests to the daemon's
//! localhost endpoint.  The file is sensitive and must be readable only by the
//! owning user.
//!
//! # Security posture
//! * **POSIX**: mode `0600` is set both at `open()` time (via
//!   [`OpenOptionsExt::mode`]) and enforced with a post-write
//!   [`set_permissions`] call to defend against umask stripping.
//! * **Windows**: The stdlib offers no clean ACL-restriction path for regular
//!   files without a third-party crate.  The token is written without
//!   additional ACL hardening.  A `tracing::warn!` is emitted so operators
//!   are aware.  TODO A1.4 follow-up: add `windows-acl` or `windows-sys`
//!   `SECURITY_DESCRIPTOR` hardening once the plan is approved.

use std::io::Write as _;
use std::path::Path;

use anyhow::Context as _;

/// Write `token` to `path`, creating or truncating the file.
///
/// On POSIX the file is created with mode `0600` and permissions are
/// explicitly re-applied after the write to resist umask stripping.
///
/// On Windows the file is written without ACL restriction (see module-level
/// doc for the rationale and follow-up reference).
///
/// Parent directories are created automatically if absent.
pub fn write_token(path: &Path, token: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent dirs for {}", path.display()))?;
    }

    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // Set mode at open time; this is the primary gate.
        options.mode(0o600);
    }

    #[cfg(windows)]
    {
        // No clean stdlib path to restrict file ACLs on Windows without an
        // external crate.  We emit a warning so operators are aware.
        // TODO A1.4 follow-up: add windows-sys SECURITY_DESCRIPTOR hardening.
        tracing::warn!(
            path = %path.display(),
            "daemon token file ACL hardening not implemented on Windows; \
             token is readable by any process running as the same user — \
             TODO A1.4 follow-up"
        );
    }

    let mut file = options
        .open(path)
        .with_context(|| format!("open token file {}", path.display()))?;

    // Post-write explicit permission set: defends against platforms / filesystems
    // where the open-time mode hint is silently ignored (e.g. some FUSE mounts,
    // exotic umask configurations).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("set_permissions 0600 on {}", path.display()))?;
    }

    writeln!(file, "{token}")
        .with_context(|| format!("write token to {}", path.display()))?;

    Ok(())
}

/// Read and return the token stored at `path`.
///
/// Trailing whitespace (including the newline written by [`write_token`]) is
/// stripped before returning.
pub fn read_token(path: &Path) -> anyhow::Result<String> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read token file {}", path.display()))?;
    Ok(raw.trim_end().to_owned())
}
