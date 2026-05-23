//! Daemon file-descriptor limit management.
//!
//! Multi-workspace daemon sessions can keep many SQLite, log, and Tantivy
//! descriptors open. macOS often starts login shells with a soft limit of 256,
//! which is too low for dogfood runs across many registered workspaces.

#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;

#[cfg(unix)]
pub(crate) const MIN_DAEMON_NOFILE_LIMIT: u64 = 4096;

#[cfg(unix)]
pub(crate) fn desired_nofile_soft_limit(
    current_soft: u64,
    current_hard: u64,
    target: u64,
    infinity: u64,
) -> Option<u64> {
    if current_soft >= target {
        return None;
    }

    let desired = if current_hard == infinity {
        target
    } else {
        target.min(current_hard)
    };

    (desired > current_soft).then_some(desired)
}

#[cfg(unix)]
pub(crate) fn raise_nofile_limit_for_daemon() -> Result<Option<(u64, u64)>> {
    let mut limit = std::mem::MaybeUninit::<libc::rlimit>::uninit();
    let get_result = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, limit.as_mut_ptr()) };
    if get_result != 0 {
        return Err(std::io::Error::last_os_error()).context("getrlimit(RLIMIT_NOFILE)");
    }

    let mut limit = unsafe { limit.assume_init() };
    let current_soft = limit.rlim_cur as u64;
    let current_hard = limit.rlim_max as u64;
    let Some(new_soft) = desired_nofile_soft_limit(
        current_soft,
        current_hard,
        MIN_DAEMON_NOFILE_LIMIT,
        libc::RLIM_INFINITY as u64,
    ) else {
        return Ok(None);
    };

    limit.rlim_cur = new_soft as libc::rlim_t;
    let set_result = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) };
    if set_result != 0 {
        return Err(std::io::Error::last_os_error()).context("setrlimit(RLIMIT_NOFILE)");
    }

    Ok(Some((current_soft, new_soft)))
}

#[cfg(not(unix))]
pub(crate) fn raise_nofile_limit_for_daemon() -> Result<Option<(u64, u64)>> {
    Ok(None)
}
