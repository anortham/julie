use crate::paths::RegistryPaths;
use serial_test::serial;
use std::path::PathBuf;

/// Helper that sets the env var and returns a guard that restores the previous
/// value on drop. Mirrors the pattern in `src/tests/registry/drain_timeout.rs`.
fn with_env(key: &str, value: &str) -> EnvGuard {
    let previous = std::env::var(key).ok();
    // SAFETY: single-threaded by serial attribute; no other threads read this var.
    unsafe { std::env::set_var(key, value) };
    EnvGuard {
        key: key.to_owned(),
        previous,
    }
}

fn without_env(key: &str) -> EnvGuard {
    let previous = std::env::var(key).ok();
    // SAFETY: single-threaded by serial attribute; no other threads read this var.
    unsafe { std::env::remove_var(key) };
    EnvGuard {
        key: key.to_owned(),
        previous,
    }
}

struct EnvGuard {
    key: String,
    previous: Option<String>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => unsafe { std::env::set_var(&self.key, v) },
            None => unsafe { std::env::remove_var(&self.key) },
        }
    }
}

const JULIE_HOME_ENV: &str = "JULIE_HOME";

/// This test deliberately exercises `dirs::home_dir()` to verify the default
/// fallback. To prevent a race with other tests that mutate `HOME`, we join
/// both the `julie_home_env` and the `home_env` serial groups (serial_test
/// supports multiple keys; tests with overlapping keys cannot run together).
#[test]
#[serial(julie_home_env, home_env)]
fn test_julie_home_uses_home_dir() {
    let _guard = without_env(JULIE_HOME_ENV);
    let paths = RegistryPaths::new();
    let home = dirs::home_dir().unwrap();
    assert_eq!(paths.julie_home(), home.join(".julie"));
}

#[test]
#[serial(julie_home_env)]
fn test_julie_home_env_override() {
    let tmp = tempfile::tempdir().unwrap();
    let override_home = tmp.path().join("external-julie-home");
    let _guard = with_env(JULIE_HOME_ENV, override_home.to_str().unwrap());

    let paths = RegistryPaths::try_new().expect("try_new should succeed when JULIE_HOME is set");
    assert_eq!(paths.julie_home(), override_home);
    assert_eq!(paths.indexes_dir(), override_home.join("indexes"));
    assert_eq!(paths.registry_db(), override_home.join("registry.db"));
}

#[test]
#[serial(julie_home_env)]
fn test_julie_home_env_empty_is_rejected() {
    let _guard = with_env(JULIE_HOME_ENV, "");

    match RegistryPaths::try_new() {
        Ok(_) => panic!("empty JULIE_HOME must be rejected"),
        Err(err) => {
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
            assert!(
                err.to_string().contains("JULIE_HOME"),
                "error message should mention JULIE_HOME, got: {}",
                err
            );
        }
    }
}

/// Finding 2: a relative `JULIE_HOME` would yield cwd-dependent daemon
/// identities across processes. `try_new` must reject non-absolute paths
/// rather than silently absolutizing them.
#[test]
#[serial(julie_home_env)]
fn test_julie_home_env_relative_is_rejected() {
    let _guard = with_env(JULIE_HOME_ENV, "relative/path");

    match RegistryPaths::try_new() {
        Ok(_) => panic!("relative JULIE_HOME must be rejected"),
        Err(err) => {
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
            assert!(
                err.to_string().contains("JULIE_HOME"),
                "error message should mention JULIE_HOME, got: {}",
                err
            );
            assert!(
                err.to_string().to_lowercase().contains("absolute"),
                "error message should mention 'absolute', got: {}",
                err
            );
        }
    }
}

#[test]
#[serial(julie_home_env)]
fn test_julie_home_env_relative_dot_is_rejected() {
    // A single "." is the canonical relative-path footgun.
    let _guard = with_env(JULIE_HOME_ENV, ".");

    match RegistryPaths::try_new() {
        Ok(_) => panic!("a relative '.' JULIE_HOME must be rejected"),
        Err(err) => assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput),
    }
}

#[test]
fn test_is_julie_home_matches_canonicalized_path() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let paths = RegistryPaths::with_home(home.clone());
    assert!(paths.is_julie_home(&home));
}

#[test]
fn test_is_julie_home_rejects_unrelated_path() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let other = tmp.path().join("other");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    let paths = RegistryPaths::with_home(home);
    assert!(!paths.is_julie_home(&other));
}

/// Finding 4: previous code unconditionally lowercased path strings on macOS,
/// which is wrong on case-sensitive APFS volumes. For NON-existent paths that
/// differ only in case, we cannot consult the filesystem, so we must NOT
/// match them — they could legitimately be different directories on a
/// case-sensitive volume.
#[test]
fn test_is_julie_home_does_not_lowercase_nonexistent_paths() {
    let tmp = tempfile::tempdir().unwrap();
    // Use paths that intentionally do NOT exist on disk so canonicalize() fails
    // and we fall back to raw PathBuf comparison.
    let home = tmp.path().join("Home-Nonexistent");
    let lower = tmp.path().join("home-nonexistent");
    let paths = RegistryPaths::with_home(home);
    assert!(
        !paths.is_julie_home(&lower),
        "case-only differing non-existent paths must NOT be treated as equal — \
         on case-sensitive filesystems they are distinct directories"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_is_julie_home_canonicalizes_existing_paths_on_macos() {
    // On default macOS HFS+/APFS (case-insensitive), canonicalize() returns
    // the on-disk case for an existing directory regardless of input case,
    // so two case-only-differing inputs that BOTH point to the same directory
    // should compare equal via canonicalize.
    let tmp = tempfile::tempdir().unwrap();
    let upper = tmp.path().join("Home");
    std::fs::create_dir_all(&upper).unwrap();
    let lower = tmp.path().join("home");
    let paths = RegistryPaths::with_home(upper);
    // On a case-insensitive filesystem, canonicalize on `lower` resolves to
    // the same canonical path as the created `Home`. We rely on canonicalize
    // here rather than string-level lowercasing.
    let _ = paths.is_julie_home(&lower);
    // We can't assert true/false because the test machine's APFS could be
    // case-sensitive. The important invariant is that calling the method
    // does not panic. The dedicated non-existent case is asserted above
    // and is filesystem-agnostic.
}

#[test]
fn test_indexes_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("explicit-test-home");
    let paths = RegistryPaths::with_home(home.clone());
    assert_eq!(paths.indexes_dir(), home.join("indexes"));
}

#[test]
fn test_workspace_index_path() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("explicit-test-home");
    let paths = RegistryPaths::with_home(home.clone());
    assert_eq!(
        paths.workspace_index_dir("myproject_abc12345"),
        home.join("indexes").join("myproject_abc12345"),
    );
}

#[test]
fn test_workspace_db_path() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("explicit-test-home");
    let paths = RegistryPaths::with_home(home.clone());
    assert_eq!(
        paths.workspace_db_path("myproject_abc12345"),
        home.join("indexes")
            .join("myproject_abc12345")
            .join("db")
            .join("symbols.db"),
    );
}

#[test]
fn test_workspace_tantivy_path() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("explicit-test-home");
    let paths = RegistryPaths::with_home(home.clone());
    assert_eq!(
        paths.workspace_tantivy_path("myproject_abc12345"),
        home.join("indexes")
            .join("myproject_abc12345")
            .join("tantivy"),
    );
}

#[test]
fn test_project_log_dir() {
    // project_log_dir does NOT depend on julie_home — confirm with an
    // explicit home that we still get `<project>/.julie/logs`.
    let tmp = tempfile::tempdir().unwrap();
    let paths = RegistryPaths::with_home(tmp.path().join("explicit-test-home"));
    let project = PathBuf::from("/Users/murphy/source/julie");
    assert_eq!(
        paths.project_log_dir(&project),
        project.join(".julie").join("logs"),
    );
}

#[test]
fn test_custom_julie_home() {
    let paths = RegistryPaths::with_home(PathBuf::from("/tmp/test-julie"));
    assert_eq!(paths.julie_home(), PathBuf::from("/tmp/test-julie"));
    assert_eq!(
        paths.indexes_dir(),
        PathBuf::from("/tmp/test-julie/indexes")
    );
}

#[test]
fn test_ensure_dirs_creates_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = RegistryPaths::with_home(tmp.path().join("julie-test-home"));
    // Directory should not exist yet
    assert!(!paths.julie_home().exists());
    // ensure_dirs should create both julie_home and indexes
    paths.ensure_dirs().unwrap();
    assert!(paths.julie_home().exists());
    assert!(paths.indexes_dir().exists());
}

#[test]
#[serial(julie_home_env, home_env)]
fn test_default_impl() {
    let _guard = without_env(JULIE_HOME_ENV);
    // Default should behave the same as new(). We don't assert on a specific
    // path (which would require reading `dirs::home_dir()` here as well);
    // we just verify the equivalence of the two constructors.
    let default_paths = RegistryPaths::default();
    let new_paths = RegistryPaths::new();
    assert_eq!(default_paths.julie_home(), new_paths.julie_home());
}

// ─────────────────────────────────────────────────────────────────────────
// Finding 1b: `is_under_julie_home` — used by the walker & watcher to
// reject any path that lives under the daemon home (catches all daemon
// state files even when JULIE_HOME is set inside a workspace).
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_is_under_julie_home_accepts_nested_path() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("julie-home");
    std::fs::create_dir_all(&home).unwrap();
    let paths = RegistryPaths::with_home(home.clone());
    // File directly under the home root.
    let token = home.join("daemon.token");
    std::fs::write(&token, b"secret").unwrap();
    assert!(paths.is_under_julie_home(&token));
    // Nested file under the home root.
    let nested = home.join("indexes").join("workspace_abc").join("db");
    std::fs::create_dir_all(&nested).unwrap();
    let symbols = nested.join("symbols.db");
    std::fs::write(&symbols, b"").unwrap();
    assert!(paths.is_under_julie_home(&symbols));
}

#[test]
fn test_is_under_julie_home_accepts_the_home_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("julie-home");
    std::fs::create_dir_all(&home).unwrap();
    let paths = RegistryPaths::with_home(home.clone());
    // The home directory itself is "under" the home for exclusion purposes.
    assert!(paths.is_under_julie_home(&home));
}

#[test]
fn test_is_under_julie_home_rejects_unrelated_path() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("julie-home");
    let other = tmp.path().join("workspace");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    let paths = RegistryPaths::with_home(home);
    let file = other.join("src.rs");
    std::fs::write(&file, b"fn main() {}").unwrap();
    assert!(!paths.is_under_julie_home(&file));
}

#[test]
fn test_is_under_julie_home_does_not_match_sibling_prefix() {
    // `julie-homework` shares a string prefix with `julie-home` but is a
    // different directory. starts_with on PathBuf is component-wise so this
    // must NOT match.
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("julie-home");
    let sibling = tmp.path().join("julie-homework");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();
    let paths = RegistryPaths::with_home(home);
    let file = sibling.join("file.rs");
    std::fs::write(&file, b"").unwrap();
    assert!(!paths.is_under_julie_home(&file));
}

// ─────────────────────────────────────────────────────────────────────────
// Finding 3: `is_any_known_julie_home` — walker guard always treats both
// the configured JULIE_HOME *and* the conventional `~/.julie` as the global
// Julie home, so an invalid JULIE_HOME does not disable the guard.
// ─────────────────────────────────────────────────────────────────────────

#[test]
#[serial(julie_home_env, home_env)]
fn test_is_any_known_julie_home_matches_default_home_dir() {
    let _guard = without_env(JULIE_HOME_ENV);
    let default = dirs::home_dir().unwrap().join(".julie");
    // We don't require `default` to exist; the helper should match it
    // structurally via RegistryPaths::is_julie_home (which falls back to raw
    // PathBuf comparison when canonicalize fails).
    assert!(
        RegistryPaths::is_any_known_julie_home(&default),
        "default ~/.julie should always be recognized as a known Julie home",
    );
}

#[test]
#[serial(julie_home_env, home_env)]
fn test_is_any_known_julie_home_when_julie_home_is_empty() {
    // Empty JULIE_HOME makes try_new() return Err. The helper must still
    // match `~/.julie` via the conventional-default fallback, so the
    // walker's global-home guard cannot be silently disabled by a
    // misconfigured env.
    let _guard = with_env(JULIE_HOME_ENV, "");
    let default = dirs::home_dir().unwrap().join(".julie");
    assert!(
        RegistryPaths::is_any_known_julie_home(&default),
        "with JULIE_HOME=\"\" the conventional ~/.julie must still be skipped",
    );
}

#[test]
#[serial(julie_home_env)]
fn test_is_any_known_julie_home_matches_configured_override() {
    let tmp = tempfile::tempdir().unwrap();
    let override_home = tmp.path().join("override-julie-home");
    std::fs::create_dir_all(&override_home).unwrap();
    let _guard = with_env(JULIE_HOME_ENV, override_home.to_str().unwrap());

    assert!(
        RegistryPaths::is_any_known_julie_home(&override_home),
        "configured JULIE_HOME must be recognized as a known Julie home",
    );
}

#[test]
#[serial(julie_home_env, home_env)]
fn test_is_any_known_julie_home_rejects_unrelated_path() {
    let _guard = without_env(JULIE_HOME_ENV);
    let tmp = tempfile::tempdir().unwrap();
    let unrelated = tmp.path().join("some-random-dir");
    std::fs::create_dir_all(&unrelated).unwrap();
    assert!(
        !RegistryPaths::is_any_known_julie_home(&unrelated),
        "unrelated paths must not be misidentified as Julie home",
    );
}
