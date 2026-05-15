//! Tests for the configurable drain timeout helper.
//!
//! These tests mutate process-level env vars, so they are serialized via
//! `#[serial]` to prevent cross-test pollution.

use std::time::Duration;

use serial_test::serial;

use crate::daemon::drain_timeout;

/// Helper that removes the env var and returns a guard that restores the
/// previous value on drop. Using explicit save/restore keeps the tests
/// hygienic without requiring unsafe code.
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

const ENV_KEY: &str = "JULIE_DAEMON_DRAIN_TIMEOUT_SECS";

#[test]
#[serial]
fn test_drain_timeout_reads_env_var() {
    let _guard = with_env(ENV_KEY, "7");
    assert_eq!(
        drain_timeout(),
        Duration::from_secs(7),
        "drain_timeout() should read JULIE_DAEMON_DRAIN_TIMEOUT_SECS=7"
    );
}

#[test]
#[serial]
fn test_drain_timeout_default_when_unset() {
    let _guard = without_env(ENV_KEY);
    assert_eq!(
        drain_timeout(),
        Duration::from_secs(60),
        "drain_timeout() should default to 60s when env var is unset"
    );
}

#[test]
#[serial]
fn test_drain_timeout_clamps_out_of_range() {
    // Below minimum (1)
    let _guard = with_env(ENV_KEY, "0");
    assert_eq!(
        drain_timeout(),
        Duration::from_secs(60),
        "value 0 is below range [1,120] and should fall back to default 60s"
    );
    drop(_guard);

    // Above maximum (120)
    let _guard2 = with_env(ENV_KEY, "500");
    assert_eq!(
        drain_timeout(),
        Duration::from_secs(60),
        "value 500 is above range [1,120] and should fall back to default 60s"
    );
    drop(_guard2);

    // Unparseable string
    let _guard3 = with_env(ENV_KEY, "abc");
    assert_eq!(
        drain_timeout(),
        Duration::from_secs(60),
        "unparseable value 'abc' should fall back to default 60s"
    );
}
