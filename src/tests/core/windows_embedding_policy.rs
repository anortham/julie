//! Focused tests for Windows embedding backend policy and DirectML adapter selection.

#[cfg(test)]
mod tests {
    use crate::embeddings::{
        BackendResolverCapabilities, EmbeddingBackend, resolve_backend_preference,
    };

    #[cfg(feature = "embeddings-ort")]
    use crate::embeddings::ort_provider::{
        OrtRuntimeState, ort_runtime_signal_for_directml_device, run_with_cpu_fallback,
    };
    #[cfg(feature = "embeddings-ort")]
    use std::cell::Cell;
    #[cfg(feature = "embeddings-ort")]
    use std::sync::Mutex;

    #[cfg(feature = "embeddings-ort")]
    use crate::embeddings::windows_directml::{DirectMlAdapterInfo, select_best_adapter};

    #[test]
    fn test_windows_auto_prefers_ort_even_when_sidecar_is_compiled() {
        let capabilities = BackendResolverCapabilities {
            sidecar_available: true,
            ort_available: true,
            target_os: "windows",
            target_arch: "x86_64",
        };

        let resolved = resolve_backend_preference(EmbeddingBackend::Auto, &capabilities)
            .expect("windows auto should resolve cleanly");

        assert_eq!(resolved, EmbeddingBackend::Ort);
    }

    #[test]
    fn test_windows_explicit_sidecar_is_allowed() {
        let capabilities = BackendResolverCapabilities {
            sidecar_available: true,
            ort_available: true,
            target_os: "windows",
            target_arch: "x86_64",
        };

        let resolved = resolve_backend_preference(EmbeddingBackend::Sidecar, &capabilities)
            .expect("windows explicit sidecar should be allowed");

        assert_eq!(resolved, EmbeddingBackend::Sidecar);
    }

    #[cfg(feature = "embeddings-ort")]
    #[test]
    fn test_select_best_adapter_prefers_discrete_gpu_and_skips_remote_virtual() {
        let adapters = vec![
            DirectMlAdapterInfo {
                index: 0,
                name: "Microsoft Remote Display Adapter".to_string(),
                is_software: false,
                is_discrete: false,
                dedicated_video_memory: 0,
                is_remote: true,
                is_virtual: true,
            },
            DirectMlAdapterInfo {
                index: 1,
                name: "Intel(R) Iris(R) Xe Graphics".to_string(),
                is_software: false,
                is_discrete: false,
                dedicated_video_memory: 0,
                is_remote: false,
                is_virtual: false,
            },
            DirectMlAdapterInfo {
                index: 2,
                name: "NVIDIA GeForce RTX 4080".to_string(),
                is_software: false,
                is_discrete: true,
                dedicated_video_memory: 16 * 1024 * 1024 * 1024,
                is_remote: false,
                is_virtual: false,
            },
        ];

        let adapter =
            select_best_adapter(&adapters).expect("expected a physical adapter to be selected");

        assert_eq!(adapter.index, 2);
        assert!(adapter.name.contains("RTX 4080"));
    }

    #[cfg(feature = "embeddings-ort")]
    #[test]
    fn test_select_best_adapter_prefers_larger_discrete_gpu_memory() {
        let adapters = vec![
            DirectMlAdapterInfo {
                index: 0,
                name: "NVIDIA GeForce RTX 3050".to_string(),
                is_software: false,
                is_discrete: true,
                dedicated_video_memory: 4 * 1024 * 1024 * 1024,
                is_remote: false,
                is_virtual: false,
            },
            DirectMlAdapterInfo {
                index: 1,
                name: "NVIDIA GeForce RTX 4080".to_string(),
                is_software: false,
                is_discrete: true,
                dedicated_video_memory: 16 * 1024 * 1024 * 1024,
                is_remote: false,
                is_virtual: false,
            },
        ];

        let adapter = select_best_adapter(&adapters).expect("expected discrete GPU");

        assert_eq!(adapter.index, 1);
        assert!(adapter.name.contains("RTX 4080"));
    }

    #[cfg(feature = "embeddings-ort")]
    #[test]
    fn test_select_best_adapter_returns_none_when_only_remote_or_software_adapters_exist() {
        let adapters = vec![
            DirectMlAdapterInfo {
                index: 0,
                name: "Microsoft Remote Desktop Adapter".to_string(),
                is_software: false,
                is_discrete: false,
                dedicated_video_memory: 0,
                is_remote: true,
                is_virtual: true,
            },
            DirectMlAdapterInfo {
                index: 1,
                name: "Microsoft Basic Render Driver".to_string(),
                is_software: true,
                is_discrete: false,
                dedicated_video_memory: 0,
                is_remote: false,
                is_virtual: true,
            },
        ];

        assert!(select_best_adapter(&adapters).is_none());
    }

    #[cfg(feature = "embeddings-ort")]
    #[test]
    fn test_ort_runtime_signal_for_directml_device_includes_adapter_label() {
        let signal = ort_runtime_signal_for_directml_device("NVIDIA GeForce RTX 4080", false);

        assert!(signal.device.contains("RTX 4080"));
        assert!(signal.accelerated);
        assert!(signal.degraded_reason.is_none());
    }

    #[cfg(feature = "embeddings-ort")]
    #[test]
    fn test_runtime_state_marks_cpu_fallback_truthfully() {
        let signal = ort_runtime_signal_for_directml_device("NVIDIA GeForce RTX 4080", false);
        let mut state = OrtRuntimeState::from_signal(signal);

        state.mark_cpu_fallback("GPU device removed".to_string());

        assert_eq!(state.device, "CPU");
        assert!(!state.accelerated);
        assert!(
            state
                .degraded_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("GPU device removed"))
        );
    }

    #[cfg(feature = "embeddings-ort")]
    #[test]
    fn test_run_with_cpu_fallback_updates_state_and_returns_retry_result() {
        let runtime_state = Mutex::new(OrtRuntimeState::from_signal(
            ort_runtime_signal_for_directml_device("NVIDIA GeForce RTX 4080", false),
        ));
        let cpu_retry_ran = Cell::new(false);
        let mut marker = ();

        let result = run_with_cpu_fallback(
            &runtime_state,
            &mut marker,
            |_model| Err(anyhow::anyhow!("DirectML device removed")),
            |_err, _model| {
                cpu_retry_ran.set(true);
                Ok(vec![1.0_f32, 2.0, 3.0])
            },
        )
        .expect("CPU retry should recover the request");

        let state = runtime_state
            .lock()
            .expect("runtime state lock should succeed");
        assert!(cpu_retry_ran.get());
        assert_eq!(result, vec![1.0_f32, 2.0, 3.0]);
        assert_eq!(state.device, "CPU");
        assert!(!state.accelerated);
        assert!(
            state
                .degraded_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("DirectML device removed"))
        );
    }
}
