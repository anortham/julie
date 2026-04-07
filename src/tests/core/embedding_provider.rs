//! Tests for EmbeddingProvider trait and factory resolution.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    use serial_test::serial;
    use tempfile::TempDir;

    use crate::embeddings::create_embedding_provider;
    use crate::embeddings::{
        BackendResolverCapabilities, DeviceInfo, EmbeddingBackend, EmbeddingConfig,
        EmbeddingProviderFactory, EmbeddingRuntimeStatus,
        parse_provider_preference, resolve_backend_preference,
        should_disable_for_strict_acceleration, strict_acceleration_enabled_from_env_value,
    };
    use crate::workspace::{JulieWorkspace, build_embedding_runtime_log_fields};

    fn test_python_interpreter() -> String {
        if let Ok(override_value) = std::env::var("JULIE_TEST_PYTHON") {
            let trimmed = override_value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        let candidates = if cfg!(target_os = "windows") {
            vec!["python", "py", "python3"]
        } else {
            vec!["python3", "python"]
        };

        for candidate in candidates {
            let available = Command::new(candidate)
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if available {
                return candidate.to_string();
            }
        }

        panic!("No Python interpreter found for tests; set JULIE_TEST_PYTHON");
    }

    fn write_fake_sidecar_script(temp_dir: &TempDir) -> PathBuf {
        let sidecar_script = temp_dir.path().join("fake_sidecar.py");
        std::fs::write(
            &sidecar_script,
            r#"import json
import sys

while True:
    line = sys.stdin.readline()
    if not line:
        break
    req = json.loads(line)
    req_id = req.get("request_id", "")
    method = req.get("method")

    if method == "health":
        resp = {
            "schema": "julie.embedding.sidecar",
            "version": 1,
            "request_id": req_id,
            "result": {
                "ready": True,
                "runtime": "fake-sidecar",
                "device": "cpu",
                "dims": 384,
            },
        }
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
        continue

    if method == "shutdown":
        resp = {
            "schema": "julie.embedding.sidecar",
            "version": 1,
            "request_id": req_id,
            "result": {"stopping": True},
        }
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
        break

    if method == "embed_query":
        result = {"dims": 384, "vector": [0.0] * 384}
    elif method == "embed_batch":
        texts = req.get("params", {}).get("texts", [])
        result = {"dims": 384, "vectors": [[0.0] * 384 for _ in texts]}
    else:
        result = {}

    resp = {
        "schema": "julie.embedding.sidecar",
        "version": 1,
        "request_id": req_id,
        "result": result,
    }
    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()
"#,
        )
        .expect("test sidecar script should be written");

        sidecar_script
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
            parse_provider_preference("sidecar").unwrap(),
            EmbeddingBackend::Sidecar
        );
    }

    #[test]
    fn test_parse_provider_preference_rejects_ort_with_helpful_error() {
        let err = parse_provider_preference("ort").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("ORT embedding backend has been removed"),
            "expected removal message, got: {message}"
        );
        assert!(
            message.contains("auto") || message.contains("sidecar"),
            "expected alternative hint in error, got: {message}"
        );
    }

    #[test]
    fn test_parse_provider_preference_accepts_sidecar() {
        assert_eq!(
            parse_provider_preference("sidecar").unwrap(),
            EmbeddingBackend::Sidecar
        );
    }

    #[test]
    fn test_parse_provider_preference_rejects_unknown_values() {
        let err = parse_provider_preference("not-a-real-provider").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("auto|sidecar"),
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
            &EmbeddingBackend::Sidecar,
            false,
            Some("CPU only; no GPU detected")
        ));
        assert!(!should_disable_for_strict_acceleration(
            false,
            &EmbeddingBackend::Sidecar,
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
            &EmbeddingBackend::Sidecar,
            false,
            None
        ));
        assert!(!should_disable_for_strict_acceleration(
            true,
            &EmbeddingBackend::Sidecar,
            true,
            None
        ));
    }

    #[test]
    fn test_resolver_auto_resolves_to_sidecar_when_available() {
        for (os, arch) in [
            ("macos", "aarch64"),
            ("linux", "x86_64"),
            ("windows", "x86_64"),
        ] {
            let capabilities = BackendResolverCapabilities {
                sidecar_available: true,
                target_os: os,
                target_arch: arch,
            };
            let resolved =
                resolve_backend_preference(EmbeddingBackend::Auto, &capabilities).unwrap();
            assert_eq!(
                resolved,
                EmbeddingBackend::Sidecar,
                "Auto should resolve to Sidecar on {os}-{arch}"
            );
        }
    }

    #[test]
    fn test_resolver_auto_errors_when_no_backend_available() {
        let capabilities = BackendResolverCapabilities {
            sidecar_available: false,
            target_os: "macos",
            target_arch: "aarch64",
        };

        let err = resolve_backend_preference(EmbeddingBackend::Auto, &capabilities).unwrap_err();
        assert!(
            err.to_string().contains("No embedding backend available"),
            "expected no-backend error, got: {err}"
        );
    }

    #[test]
    fn test_resolver_errors_when_explicit_sidecar_unavailable() {
        let capabilities = BackendResolverCapabilities {
            sidecar_available: false,
            target_os: "linux",
            target_arch: "x86_64",
        };

        let err =
            resolve_backend_preference(EmbeddingBackend::Sidecar, &capabilities).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("sidecar") && message.contains("not available"),
            "expected clear sidecar availability error, got: {message}"
        );
    }

    #[test]
    fn test_embedding_runtime_status_captures_init_state() {
        let status = EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Sidecar,
            accelerated: true,
            degraded_reason: None,
        };

        assert_eq!(status.requested_backend, EmbeddingBackend::Auto);
        assert_eq!(status.resolved_backend, EmbeddingBackend::Sidecar);
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
            resolved_backend: EmbeddingBackend::Sidecar,
            accelerated: true,
            degraded_reason: None,
        };
        let provider_info = DeviceInfo {
            runtime: "sidecar-mps".to_string(),
            device: "Metal (MPS)".to_string(),
            model_name: "bge-small-en-v1.5".to_string(),
            dimensions: 384,
        };

        let fields =
            build_embedding_runtime_log_fields(&status, Some(&provider_info), false, false);
        assert_eq!(fields.requested_backend, "auto");
        assert_eq!(fields.resolved_backend, "sidecar");
        assert_eq!(fields.runtime, "sidecar-mps");
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
            resolved_backend: EmbeddingBackend::Sidecar,
            accelerated: false,
            degraded_reason: Some("fallback to CPU".to_string()),
        };

        let fields = build_embedding_runtime_log_fields(&status, None, true, true);
        assert_eq!(fields.requested_backend, "auto");
        assert_eq!(fields.resolved_backend, "sidecar");
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
            resolved_backend: EmbeddingBackend::Sidecar,
            accelerated: false,
            degraded_reason: None,
        };
        let provider_info = DeviceInfo {
            runtime: "sidecar (pytorch)".to_string(),
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
            runtime: "ort (ONNX Runtime)".to_string(),
            device: "CPU".to_string(),
            model_name: "BGE-small-en-v1.5".to_string(),
            dimensions: 384,
        };
        assert!(!cpu_fallback.is_accelerated());

        let metal_gpu = DeviceInfo {
            runtime: "sidecar".to_string(),
            device: "Metal (MPS)".to_string(),
            model_name: "BGE-small-en-v1.5".to_string(),
            dimensions: 384,
        };
        assert!(metal_gpu.is_accelerated());

        let directml_gpu = DeviceInfo {
            runtime: "onnxruntime-directml".to_string(),
            device: "DirectML".to_string(),
            model_name: "BGE-small-en-v1.5".to_string(),
            dimensions: 384,
        };
        assert!(directml_gpu.is_accelerated());
    }

    #[tokio::test]
    #[serial(embedding_env)]
    async fn test_invalid_provider_sets_unresolved_runtime_status() {
        unsafe {
            std::env::set_var("JULIE_EMBEDDING_PROVIDER", "definitely-not-valid");
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
        }

        let temp_dir = TempDir::new().unwrap();
        let mut workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        workspace.initialize_embedding_provider();

        let status = workspace
            .embedding_runtime_status
            .as_ref()
            .expect("runtime status should be captured");

        assert!(matches!(
            status.requested_backend,
            EmbeddingBackend::Invalid(ref provider) if provider == "definitely-not-valid"
        ));
        assert_eq!(status.resolved_backend, EmbeddingBackend::Unresolved);
        assert!(!status.accelerated);

        unsafe {
            std::env::remove_var("JULIE_EMBEDDING_PROVIDER");
            std::env::remove_var("JULIE_SKIP_SEARCH_INDEX");
        }
    }

    #[tokio::test]
    #[serial(embedding_env)]
    async fn test_provider_none_disables_embeddings_silently() {
        unsafe {
            std::env::set_var("JULIE_EMBEDDING_PROVIDER", "none");
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
        }

        let temp_dir = TempDir::new().unwrap();
        let mut workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        workspace.initialize_embedding_provider();

        // Provider should be None (disabled, not failed)
        assert!(
            workspace.embedding_provider.is_none(),
            "Embedding provider should be None when disabled"
        );

        // Runtime status should also be None — never attempted, not an error
        assert!(
            workspace.embedding_runtime_status.is_none(),
            "Runtime status should be None when explicitly disabled"
        );

        unsafe {
            std::env::remove_var("JULIE_EMBEDDING_PROVIDER");
            std::env::remove_var("JULIE_SKIP_SEARCH_INDEX");
        }
    }

    #[cfg(feature = "embeddings-sidecar")]
    #[tokio::test]
    #[serial(embedding_env)]
    async fn test_workspace_init_strict_accel_disables_sidecar_when_unaccelerated() {
        let temp_dir = TempDir::new().unwrap();
        let sidecar_script = write_fake_sidecar_script(&temp_dir);

        unsafe {
            std::env::set_var("JULIE_EMBEDDING_PROVIDER", "sidecar");
            std::env::set_var("JULIE_EMBEDDING_STRICT_ACCEL", "on");
            std::env::set_var("JULIE_EMBEDDING_SIDECAR_PROGRAM", test_python_interpreter());
            std::env::set_var("JULIE_EMBEDDING_SIDECAR_SCRIPT", sidecar_script.as_os_str());
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
        }

        let mut workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        workspace.initialize_embedding_provider();

        let status = workspace
            .embedding_runtime_status
            .as_ref()
            .expect("runtime status should be captured");

        assert_eq!(status.requested_backend, EmbeddingBackend::Sidecar);
        assert_eq!(status.resolved_backend, EmbeddingBackend::Sidecar);
        assert!(
            workspace.embedding_provider.is_none(),
            "strict accel mode should disable unaccelerated sidecar runtime"
        );
        assert!(
            status
                .degraded_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("strict acceleration")
                    && reason.contains("JULIE_EMBEDDING_STRICT_ACCEL")),
            "expected strict acceleration disable reason, got: {:?}",
            status.degraded_reason
        );

        unsafe {
            std::env::remove_var("JULIE_EMBEDDING_PROVIDER");
            std::env::remove_var("JULIE_EMBEDDING_STRICT_ACCEL");
            std::env::remove_var("JULIE_EMBEDDING_SIDECAR_PROGRAM");
            std::env::remove_var("JULIE_EMBEDDING_SIDECAR_SCRIPT");
            std::env::remove_var("JULIE_SKIP_SEARCH_INDEX");
        }
    }

    #[test]
    fn test_provider_factory_rejects_unknown_provider() {
        let config = EmbeddingConfig {
            provider: "not-a-real-provider".to_string(),
            cache_dir: None,
        };

        let err = match EmbeddingProviderFactory::create(&config) {
            Ok(_) => panic!("Factory should reject unknown provider"),
            Err(err) => err,
        };
        assert!(
            err.to_string().contains("auto|sidecar"),
            "Expected unknown provider error, got: {err}"
        );
    }

    #[test]
    fn test_provider_factory_rejects_ort_with_helpful_message() {
        let config = EmbeddingConfig {
            provider: "ort".to_string(),
            cache_dir: None,
        };

        let err = match EmbeddingProviderFactory::create(&config) {
            Ok(_) => panic!("Factory should reject removed ORT provider"),
            Err(err) => err,
        };
        assert!(
            err.to_string().contains("ORT embedding backend has been removed"),
            "Expected ORT removal message, got: {err}"
        );
    }

    #[test]
    #[serial(embedding_env)]
    fn test_create_embedding_provider_returns_runtime_status() {
        // JULIE_EMBEDDING_PROVIDER=none should disable embeddings:
        // provider = None, runtime_status = None (never attempted, not an error)
        unsafe {
            std::env::set_var("JULIE_EMBEDDING_PROVIDER", "none");
        }

        let (provider, status) = create_embedding_provider();

        assert!(provider.is_none(), "provider should be None when disabled");
        assert!(
            status.is_none(),
            "runtime status should be None when explicitly disabled"
        );

        unsafe {
            std::env::remove_var("JULIE_EMBEDDING_PROVIDER");
        }
    }
}
