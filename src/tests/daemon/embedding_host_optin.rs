//! Tests for the opt-in embedding-host coexistence wiring (Phase 3b, Task 7).
//!
//! These tests drive `use_embedding_host()` directly — a unit-level check that
//! proves "env unset → existing `create_embedding_provider` path is selected,
//! host path only when truthy." The heavy integration (shared sidecar across
//! sessions) is covered by T8.

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use crate::daemon::app::use_embedding_host;

    // -----------------------------------------------------------------------
    // EnvGuard — sets/restores an env var on drop (SAFETY: serialised by
    // #[serial] so no other thread touches the var concurrently).
    // -----------------------------------------------------------------------

    fn with_env(key: &str, value: &str) -> EnvGuard {
        let previous = std::env::var(key).ok();
        unsafe { std::env::set_var(key, value) };
        EnvGuard { key: key.to_owned(), previous }
    }

    fn without_env(key: &str) -> EnvGuard {
        let previous = std::env::var(key).ok();
        unsafe { std::env::remove_var(key) };
        EnvGuard { key: key.to_owned(), previous }
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

    const KEY: &str = "JULIE_EMBEDDING_USE_HOST";

    // -----------------------------------------------------------------------
    // Test 1: env unset → false (existing create_embedding_provider path)
    // -----------------------------------------------------------------------

    #[test]
    #[serial(embedding_host_optin_env)]
    fn false_when_env_unset() {
        let _guard = without_env(KEY);
        assert!(!use_embedding_host(), "expected false when {KEY} is unset");
    }

    // -----------------------------------------------------------------------
    // Test 2: truthy values → true (host path selected)
    // -----------------------------------------------------------------------

    #[test]
    #[serial(embedding_host_optin_env)]
    fn true_for_all_truthy_values() {
        for value in &["1", "true", "on", "TRUE", "On", "ON"] {
            let _guard = with_env(KEY, value);
            assert!(
                use_embedding_host(),
                "expected true for {KEY}={value:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Test 3: falsy / unrecognised values → false
    // -----------------------------------------------------------------------

    #[test]
    #[serial(embedding_host_optin_env)]
    fn false_for_falsy_and_unrecognised_values() {
        for value in &["0", "false", "off", "no", "", "yes", "maybe"] {
            let _guard = with_env(KEY, value);
            assert!(
                !use_embedding_host(),
                "expected false for {KEY}={value:?}"
            );
        }
    }
}
