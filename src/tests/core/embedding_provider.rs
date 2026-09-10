//! Tests for EmbeddingProvider trait and factory resolution.

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use tempfile::TempDir;

    use crate::embeddings::create_embedding_provider;
    use crate::embeddings::{
        BackendResolverCapabilities, DeviceInfo, EmbeddingBackend, EmbeddingConfig,
        EmbeddingProviderFactory, EmbeddingRuntimeStatus, parse_provider_preference,
        resolve_backend_preference, should_disable_for_strict_acceleration,
        strict_acceleration_enabled_from_env_value,
    };
    use crate::tests::helpers::env::EnvVarGuard;
    use crate::workspace::{JulieWorkspace, build_embedding_runtime_log_fields};

    fn capabilities(native_available: bool) -> BackendResolverCapabilities {
        BackendResolverCapabilities {
            native_available,
            target_os: "linux",
            target_arch: "x86_64",
        }
    }

    #[test]
    fn test_embedding_config_default_provider_is_auto() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.provider, "auto");
    }

    #[test]
    fn test_parse_provider_preference_accepts_known_values() {
        assert_eq!(
            parse_provider_preference("auto").unwrap(),
            EmbeddingBackend::Auto
        );
        assert_eq!(
            parse_provider_preference("native").unwrap(),
            EmbeddingBackend::Native
        );
    }

    #[test]
    fn test_parse_provider_preference_rejects_removed_backends_with_helpful_error() {
        for removed in ["ort", "sidecar"] {
            let message = parse_provider_preference(removed).unwrap_err().to_string();
            assert!(
                message.contains("has been removed") && message.contains("native"),
                "expected removal message with native hint for {removed}, got: {message}"
            );
        }
    }

    #[test]
    fn test_parse_provider_preference_rejects_unknown_values() {
        let err = parse_provider_preference("not-a-real-provider").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("auto|native|none"),
            "expected valid provider set in error, got: {message}"
        );
    }

    #[test]
    fn test_strict_acceleration_enabled_from_env_value_truthy_values() {
        assert!(strict_acceleration_enabled_from_env_value("1"));
        assert!(strict_acceleration_enabled_from_env_value("true"));
        assert!(strict_acceleration_enabled_from_env_value("on"));
        assert!(strict_acceleration_enabled_from_env_value("TrUe"));
    }

    #[test]
    fn test_strict_acceleration_enabled_from_env_value_non_truthy_values() {
        assert!(!strict_acceleration_enabled_from_env_value("0"));
        assert!(!strict_acceleration_enabled_from_env_value("false"));
        assert!(!strict_acceleration_enabled_from_env_value("off"));
        assert!(!strict_acceleration_enabled_from_env_value(""));
    }

    #[test]
    fn test_should_disable_for_strict_acceleration_when_degraded() {
        assert!(should_disable_for_strict_acceleration(
            true,
            &EmbeddingBackend::Native,
            false,
            Some("CPU only; no GPU detected")
        ));
        assert!(!should_disable_for_strict_acceleration(
            false,
            &EmbeddingBackend::Native,
            false,
            Some("CPU only; no GPU detected")
        ));
    }

    #[test]
    fn test_should_disable_for_strict_acceleration_when_unresolved() {
        assert!(should_disable_for_strict_acceleration(
            true,
            &EmbeddingBackend::Unresolved,
            false,
            None
        ));
        assert!(!should_disable_for_strict_acceleration(
            false,
            &EmbeddingBackend::Unresolved,
            false,
            None
        ));
    }

    #[test]
    fn test_should_disable_for_strict_acceleration_when_not_accelerated() {
        assert!(should_disable_for_strict_acceleration(
            true,
            &EmbeddingBackend::Native,
            false,
            None
        ));
        assert!(!should_disable_for_strict_acceleration(
            true,
            &EmbeddingBackend::Native,
            true,
            None
        ));
    }

    #[test]
    fn test_resolver_auto_resolves_to_native_when_sidecar_binary_found() {
        let resolved =
            resolve_backend_preference(EmbeddingBackend::Auto, &capabilities(true)).unwrap();
        assert_eq!(resolved, EmbeddingBackend::Native);
    }

    #[test]
    fn test_resolver_auto_errors_when_sidecar_binary_missing() {
        let err =
            resolve_backend_preference(EmbeddingBackend::Auto, &capabilities(false)).unwrap_err();
        assert!(
            err.to_string().contains("No embedding backend available"),
            "expected no-backend error, got: {err}"
        );
    }

    #[test]
    fn test_resolver_explicit_native_resolves_even_without_binary() {
        let resolved =
            resolve_backend_preference(EmbeddingBackend::Native, &capabilities(false)).unwrap();
        assert_eq!(resolved, EmbeddingBackend::Native);
    }

    #[test]
    fn test_embedding_runtime_status_captures_init_state() {
        let status = EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Native,
            accelerated: true,
            degraded_reason: None,
        };

        assert_eq!(status.requested_backend, EmbeddingBackend::Auto);
        assert_eq!(status.resolved_backend, EmbeddingBackend::Native);
        assert!(status.accelerated);
        assert!(status.degraded_reason.is_none());
    }

    #[test]
    fn test_embedding_runtime_status_supports_unresolved_backend() {
        let status = EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Invalid("bad-provider".to_string()),
            resolved_backend: EmbeddingBackend::Unresolved,
            accelerated: false,
            degraded_reason: Some("unknown provider".to_string()),
        };

        assert_eq!(status.resolved_backend, EmbeddingBackend::Unresolved);
        assert_eq!(status.resolved_backend.as_str(), "unresolved");
    }

    #[test]
    fn test_build_embedding_runtime_log_fields_includes_provider_runtime_context() {
        let status = EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Native,
            accelerated: true,
            degraded_reason: None,
        };
        let provider_info = DeviceInfo {
            runtime: "llama.cpp".to_string(),
            device: "Metal (MPS)".to_string(),
            model_name: "bge-small-en-v1.5".to_string(),
            dimensions: 384,
        };

        let fields =
            build_embedding_runtime_log_fields(&status, Some(&provider_info), false, false);
        assert_eq!(fields.requested_backend, "auto");
        assert_eq!(fields.resolved_backend, "native");
        assert_eq!(fields.runtime, "llama.cpp");
        assert_eq!(fields.device, "Metal (MPS)");
        assert!(fields.accelerated);
        assert_eq!(fields.degraded_reason, "none");
        assert_eq!(fields.telemetry_confidence, "high");
        assert!(!fields.strict_mode);
        assert!(!fields.fallback_used);
    }

    #[test]
    fn test_build_embedding_runtime_log_fields_handles_missing_provider() {
        let status = EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Native,
            accelerated: false,
            degraded_reason: Some("fallback to CPU".to_string()),
        };

        let fields = build_embedding_runtime_log_fields(&status, None, true, true);
        assert_eq!(fields.requested_backend, "auto");
        assert_eq!(fields.resolved_backend, "native");
        assert_eq!(fields.runtime, "unavailable");
        assert_eq!(fields.device, "unavailable");
        assert!(!fields.accelerated);
        assert_eq!(fields.degraded_reason, "fallback to CPU");
        assert_eq!(fields.telemetry_confidence, "low");
        assert!(fields.strict_mode);
        assert!(fields.fallback_used);
    }

    #[test]
    fn test_build_embedding_runtime_log_fields_marks_unknown_device_low_confidence() {
        let status = EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Native,
            accelerated: false,
            degraded_reason: None,
        };
        let provider_info = DeviceInfo {
            runtime: "llama.cpp".to_string(),
            device: "Unknown".to_string(),
            model_name: "BGE-small-en-v1.5".to_string(),
            dimensions: 384,
        };

        let fields =
            build_embedding_runtime_log_fields(&status, Some(&provider_info), false, false);
        assert_eq!(fields.telemetry_confidence, "low");
    }

    #[test]
    fn test_device_info_acceleration_heuristic_distinguishes_cpu_and_gpu() {
        let cpu_fallback = DeviceInfo {
            runtime: "llama.cpp".to_string(),
            device: "CPU".to_string(),
            model_name: "BGE-small-en-v1.5".to_string(),
            dimensions: 384,
        };
        assert!(!cpu_fallback.is_accelerated());

        let metal_gpu = DeviceInfo {
            runtime: "llama.cpp".to_string(),
            device: "Metal (MPS)".to_string(),
            model_name: "BGE-small-en-v1.5".to_string(),
            dimensions: 384,
        };
        assert!(metal_gpu.is_accelerated());

        let directml_gpu = DeviceInfo {
            runtime: "llama.cpp".to_string(),
            device: "DirectML".to_string(),
            model_name: "BGE-small-en-v1.5".to_string(),
            dimensions: 384,
        };
        assert!(directml_gpu.is_accelerated());
    }

    #[tokio::test]
    #[serial(embedding_env)]
    async fn test_invalid_provider_sets_unresolved_runtime_status() {
        let mut env = EnvVarGuard::new();
        env.set("JULIE_EMBEDDING_PROVIDER", "definitely-not-valid");
        env.set("JULIE_SKIP_SEARCH_INDEX", "1");

        let (_provider, status) = create_embedding_provider();
        let status = status.expect("runtime status should be captured");

        assert!(matches!(
            status.requested_backend,
            EmbeddingBackend::Invalid(ref provider) if provider == "definitely-not-valid"
        ));
        assert_eq!(status.resolved_backend, EmbeddingBackend::Unresolved);
        assert!(!status.accelerated);
    }

    #[tokio::test]
    #[serial(embedding_env)]
    async fn test_workspace_embeddings_are_disabled_by_default_under_cargo() {
        let mut env = EnvVarGuard::new();
        env.set("JULIE_SKIP_SEARCH_INDEX", "1");

        let (provider, status) = create_embedding_provider();

        assert!(
            provider.is_none(),
            "Embedding provider should be None when disabled"
        );
        assert!(
            status.is_none(),
            "Runtime status should be None when explicitly disabled"
        );
    }

    #[test]
    fn test_provider_factory_rejects_unknown_provider() {
        let config = EmbeddingConfig {
            provider: "not-a-real-provider".to_string(),
            cache_dir: None,
            ..Default::default()
        };

        let err = match EmbeddingProviderFactory::create(&config) {
            Ok(_) => panic!("Factory should reject unknown provider"),
            Err(err) => err,
        };
        assert!(
            err.to_string().contains("auto|native|none"),
            "Expected unknown provider error, got: {err}"
        );
    }

    #[test]
    fn test_provider_factory_rejects_removed_sidecar_provider() {
        let config = EmbeddingConfig {
            provider: "sidecar".to_string(),
            cache_dir: None,
            ..Default::default()
        };

        let err = match EmbeddingProviderFactory::create(&config) {
            Ok(_) => panic!("Factory should reject removed sidecar provider"),
            Err(err) => err,
        };
        assert!(
            err.to_string().contains("has been removed"),
            "Expected removal message, got: {err}"
        );
    }

    /// The test default comes from `.cargo/config.toml` `[env]`, not from the
    /// test itself: tests must never start an embedding process by accident.
    #[test]
    #[serial(embedding_env)]
    fn test_create_embedding_provider_defaults_to_no_provider_without_test_override() {
        assert_eq!(
            std::env::var("JULIE_EMBEDDING_PROVIDER").as_deref(),
            Ok("none"),
            "cargo [env] must set the test default"
        );

        let (provider, status) = create_embedding_provider();

        assert!(provider.is_none(), "provider should be None by default");
        assert!(
            status.is_none(),
            "runtime status should be None when embeddings are disabled by default"
        );
    }
}
