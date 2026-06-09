//! Connect-or-spawn glue for the resident embedding-host (Phase 3b, Task 6).
//!
//! Only the top `julie` crate knows where `julie-embedding-host` lives, so
//! the launch logic lives here rather than in `julie-pipeline`.
//!
//! # Behaviour
//!
//! 1. **Host already live** — `is_host_live` attempts a test connection and
//!    succeeds.  Returns an [`RpcEmbeddingProvider`] immediately.
//! 2. **Host not live** — locates the `julie-embedding-host` sibling binary
//!    (same strategy as `src/adapter/launcher.rs` for `julie-daemon`), spawns
//!    it as a detached background process, then polls until the socket/pipe
//!    accepts connections or the spawn timeout expires (default 180 s, override
//!    with `JULIE_EMBEDDING_HOST_SPAWN_TIMEOUT_SECS`).  180 s covers a cold
//!    sidecar initialisation (model download + venv bootstrap).
//!
//! `JULIE_HOME` is pinned from the `paths` argument when spawning, so the child
//! binds the same socket this function polls; the `JULIE_EMBEDDING_*` vars are
//! inherited from the parent environment.

use std::io;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use tracing::{debug, info};

use crate::paths::RegistryPaths;
use julie_pipeline::embeddings::{
    host_transport::{HostAddress, HostClientConn},
    rpc_client::RpcEmbeddingProvider,
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Connect to the per-`$JULIE_HOME` embedding-host, spawning it if necessary.
///
/// Returns an [`RpcEmbeddingProvider`] whose connection is lazy: the first
/// real embedding call triggers the health handshake.
pub fn connect_or_spawn_host(paths: &RegistryPaths) -> Result<RpcEmbeddingProvider> {
    let addr = HostAddress::from_paths(paths);

    if is_host_live(&addr) {
        debug!("embedding-host already live; returning client directly");
        return Ok(RpcEmbeddingProvider::new(addr));
    }

    info!("embedding-host not live; spawning it now");
    spawn_host_process(paths)?;

    poll_for_liveness(&addr, host_spawn_timeout())
        .context("embedding-host did not become live after spawn")?;

    Ok(RpcEmbeddingProvider::new(addr))
}

// ---------------------------------------------------------------------------
// Spawn timeout
// ---------------------------------------------------------------------------

/// Default wait for the host process to become live after spawning.
///
/// 180 s covers a cold sidecar init: model download + venv bootstrap can
/// take up to ~3 minutes on a slow machine.
const DEFAULT_HOST_SPAWN_TIMEOUT: Duration = Duration::from_secs(180);

/// Parse a raw env-var string into a spawn timeout duration.
///
/// - `Some("0")` → `DEFAULT_HOST_SPAWN_TIMEOUT` (0 means "use the default").
/// - `Some("<positive integer>")` → `Duration::from_secs(n)`.
/// - `Some("<invalid>")` | `None` → `DEFAULT_HOST_SPAWN_TIMEOUT`.
///
/// Exposed as module-private so the inline `#[cfg(test)]` block can unit-test
/// it without touching the process environment.
fn parse_spawn_timeout(raw: Option<String>) -> Duration {
    raw.and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_HOST_SPAWN_TIMEOUT)
}

fn host_spawn_timeout() -> Duration {
    parse_spawn_timeout(std::env::var("JULIE_EMBEDDING_HOST_SPAWN_TIMEOUT_SECS").ok())
}

// ---------------------------------------------------------------------------
// Liveness check
// ---------------------------------------------------------------------------

/// Returns `true` if the host is currently accepting connections.
///
/// Opens a test connection and drops it immediately — the returned
/// `RpcEmbeddingProvider` makes its own connection on first use.
fn is_host_live(addr: &HostAddress) -> bool {
    HostClientConn::connect(addr).is_ok()
}

/// Poll `is_host_live` with exponential back-off until the host is up or
/// `timeout` elapses.
fn poll_for_liveness(addr: &HostAddress, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut delay = Duration::from_millis(10);
    let max_delay = Duration::from_millis(200);

    loop {
        if is_host_live(addr) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(anyhow::anyhow!(
                "embedding-host did not become live within {:?}",
                timeout
            ));
        }
        std::thread::sleep(delay);
        delay = (delay * 2).min(max_delay);
    }
}

// ---------------------------------------------------------------------------
// Process spawn
// ---------------------------------------------------------------------------

/// Spawn `julie-embedding-host` as a detached background process.
///
/// stdin/stdout/stderr are redirected to null.  `JULIE_HOME` is pinned from the
/// `paths` argument so the spawned host binds the exact socket this function
/// polls (even when `paths` was built via `with_home(...)` and differs from the
/// parent's own `$JULIE_HOME`, e.g. in tests).  The `JULIE_EMBEDDING_*` vars are
/// inherited from the parent environment.  Process-group detachment mirrors
/// `spawn_daemon` in `src/adapter/launcher.rs`.
fn spawn_host_process(paths: &RegistryPaths) -> io::Result<()> {
    let host_exe = locate_embedding_host()?;
    info!("Spawning embedding-host: {}", host_exe.display());

    // Ensure the socket/lock parent dirs exist before the child tries to bind.
    paths.ensure_dirs().map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("ensure_dirs failed before spawning embedding-host: {e}"),
        )
    })?;

    let mut cmd = std::process::Command::new(&host_exe);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    // Pin the child's $JULIE_HOME to the caller's `paths` so the host binds the
    // exact socket this function polls — `connect_or_spawn_host` derives the
    // address from `paths`, not from the parent's env, and the two must agree.
    cmd.env("JULIE_HOME", paths.julie_home());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        // Detach the child via setsid() so it survives the caller's exit and
        // is not killed by SIGHUP to the controlling terminal. EPERM means
        // the calling process is already a session leader (test harness) —
        // ignore it; the child is still adequately detached.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    let err = io::Error::last_os_error();
                    if err.raw_os_error() != Some(libc::EPERM) {
                        return Err(err);
                    }
                }
                Ok(())
            });
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // DETACHED_PROCESS         (0x0000_0008): no console
        // CREATE_NEW_PROCESS_GROUP (0x0000_0200): immune to Ctrl+C
        // CREATE_NO_WINDOW         (0x0800_0000): no console window flash
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }

    cmd.spawn()?;
    Ok(())
}

/// Locate `julie-embedding-host` using the same two-step strategy as
/// `locate_julie_daemon` in `src/adapter/launcher.rs`:
///
/// 1. Sibling of the current executable (normal plugin / installed layout).
/// 2. `PATH` lookup as a fallback.
fn locate_embedding_host() -> io::Result<std::path::PathBuf> {
    let bin_name = if cfg!(windows) {
        "julie-embedding-host.exe"
    } else {
        "julie-embedding-host"
    };

    // (1) Sibling of the running process's own executable.
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            let sibling = parent.join(bin_name);
            if sibling.is_file() {
                debug!(
                    "Resolved {} as sibling of current exe: {}",
                    bin_name,
                    sibling.display()
                );
                return Ok(sibling);
            }
        }
    }

    // (2) PATH lookup — verify the file exists before returning so errors
    //     land here with a useful message rather than in a confusing spawn().
    if let Ok(path_env) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for entry in path_env.split(sep) {
            let candidate = std::path::Path::new(entry).join(bin_name);
            if candidate.is_file() {
                debug!("Resolved {} via PATH: {}", bin_name, candidate.display());
                return Ok(candidate);
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "{bin_name} not found next to current executable or on PATH; \
             check installation"
        ),
    ))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_spawn_timeout_values() {
        assert_eq!(
            parse_spawn_timeout(Some("5".into())),
            Duration::from_secs(5)
        );
        assert_eq!(parse_spawn_timeout(None), DEFAULT_HOST_SPAWN_TIMEOUT);
        // "0" is treated as "use the default" (not a valid timeout).
        assert_eq!(
            parse_spawn_timeout(Some("0".into())),
            DEFAULT_HOST_SPAWN_TIMEOUT
        );
    }
}
