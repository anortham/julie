use std::path::PathBuf;

use crate::workspace::root_safety::{is_sensitive_workspace_root, sensitive_root_candidates};

/// On macOS `/var/root` is a symlink target: canonicalizing it produces
/// `/private/var/root`. CLI workspace resolution canonicalizes inputs before
/// the guard runs, so the guard must reject the canonical form as well.
/// Without this, the sensitive-path bail-out is bypassed and the only safety
/// net is `O_CREAT` permission denial inside `.julie/`.
#[cfg(target_os = "macos")]
#[test]
fn test_rejects_canonicalized_var_root_on_macos() {
    let raw = PathBuf::from("/var/root");
    assert!(
        raw.exists(),
        "test prerequisite missing: /var/root must exist on macOS"
    );
    let canonical = raw
        .canonicalize()
        .expect("canonicalize /var/root must succeed on macOS");
    assert!(
        is_sensitive_workspace_root(&raw),
        "raw /var/root must be rejected"
    );
    assert!(
        is_sensitive_workspace_root(&canonical),
        "canonicalized form {} of /var/root must also be rejected",
        canonical.display()
    );
}

/// Even if canonicalization is unavailable (e.g. constrained CI sandbox), the
/// forbidden list must catch the literal `/private/var/root` form so the macOS
/// guard does not depend on a runtime syscall succeeding. This pins the
/// belt-and-suspenders behavior so future refactors of the canonicalize fallback
/// cannot silently reintroduce the alias bypass.
#[cfg(target_os = "macos")]
#[test]
fn test_rejects_private_var_root_literal_on_macos() {
    assert!(
        is_sensitive_workspace_root(&PathBuf::from("/private/var/root")),
        "/private/var/root must be in the forbidden list literally, not via canonicalize"
    );
}

/// `dirs::home_dir()` returns `$HOME` verbatim. If a workspace path canonicalizes
/// to the same directory through a symlink (common when HOME lives under
/// `/var/folders` on macOS), the guard must still fire. Uses serial_test to
/// avoid clobbering HOME for parallel tests.
#[test]
#[serial_test::serial(home_env)]
fn test_rejects_canonicalized_home_when_home_is_symlinked() {
    let temp = tempfile::tempdir().expect("tempdir");
    let raw = temp.path().to_path_buf();
    let canonical = raw.canonicalize().unwrap_or_else(|_| raw.clone());

    let old_home = std::env::var_os("HOME");
    unsafe {
        std::env::set_var("HOME", &raw);
    }

    let rejected_raw = is_sensitive_workspace_root(&raw);
    let rejected_canonical = is_sensitive_workspace_root(&canonical);

    if let Some(v) = old_home {
        unsafe {
            std::env::set_var("HOME", v);
        }
    } else {
        unsafe {
            std::env::remove_var("HOME");
        }
    }

    assert!(rejected_raw, "guard must reject the raw HOME path");
    if raw != canonical {
        assert!(
            rejected_canonical,
            "guard must reject canonicalized HOME ({} resolves to {})",
            raw.display(),
            canonical.display()
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_sensitive_root_candidates_includes_macos_private_alias() {
    let candidates = sensitive_root_candidates();
    assert!(
        candidates.contains(&PathBuf::from("/private/var/root")),
        "macOS candidate list must include /private/var/root literally so canonicalize is not required, got {:?}",
        candidates
    );
    assert!(
        candidates.contains(&PathBuf::from("/var/root")),
        "macOS candidate list must include /var/root"
    );
    assert!(
        candidates.contains(&PathBuf::from("/Users")),
        "macOS candidate list must include /Users"
    );
}

#[test]
fn test_sensitive_root_candidates_includes_home_dir() {
    let candidates = sensitive_root_candidates();
    if let Some(home) = dirs::home_dir() {
        assert!(
            candidates.contains(&home),
            "candidate list must include dirs::home_dir() = {:?}",
            home
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn test_sensitive_root_candidates_includes_linux_admin_dirs() {
    let candidates = sensitive_root_candidates();
    assert!(
        candidates.contains(&PathBuf::from("/root")),
        "Linux candidate list must include /root"
    );
    assert!(
        candidates.contains(&PathBuf::from("/home")),
        "Linux candidate list must include /home"
    );
}

#[cfg(windows)]
#[test]
#[serial_test::serial(systemdrive_env)]
fn test_sensitive_root_candidates_honours_systemdrive_env_on_windows() {
    let old_drive = std::env::var_os("SystemDrive");
    unsafe {
        std::env::set_var("SystemDrive", "D:");
    }
    let candidates = sensitive_root_candidates();
    if let Some(v) = old_drive {
        unsafe {
            std::env::set_var("SystemDrive", v);
        }
    } else {
        unsafe {
            std::env::remove_var("SystemDrive");
        }
    }
    assert!(
        candidates.contains(&PathBuf::from(r"D:\Users")),
        "Windows candidate list must honour SystemDrive=D: for the Users subdir, got {:?}",
        candidates
    );
    assert!(
        candidates.contains(&PathBuf::from(r"D:\Windows")),
        "Windows candidate list must honour SystemDrive=D: for the Windows subdir"
    );
}
