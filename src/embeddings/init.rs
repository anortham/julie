//! Standalone embedding provider initialization.
//!
//! Extracted from `JulieWorkspace::initialize_embedding_provider()` so that
//! daemon mode can create an embedding provider without a workspace instance.

use std::sync::Arc;

use tracing::{info, warn};

use crate::embeddings::{
    BackendResolverCapabilities, EmbeddingBackend, EmbeddingConfig, EmbeddingProvider,
    EmbeddingProviderFactory, EmbeddingRuntimeStatus, fallback_backend_after_init_failure,
    parse_provider_preference, resolve_backend_preference, should_disable_for_strict_acceleration,
    strict_acceleration_enabled_from_env_value,
};
use crate::workspace::build_embedding_runtime_log_fields;

/// Create an embedding provider by reading environment variables and resolving
/// the backend preference. Returns the provider (if successful) and runtime
/// status (if initialization was attempted).
///
/// This is a pure function with no workspace dependency. Callers assign the
/// results to whatever owns the provider (workspace, daemon service, etc.).
pub fn create_embedding_provider() -> (
    Option<Arc<dyn EmbeddingProvider>>,
    Option<EmbeddingRuntimeStatus>,
) {
    let strict_accel = std::env::var("JULIE_EMBEDDING_STRICT_ACCEL")
        .ok()
        .is_some_and(|value| strict_acceleration_enabled_from_env_value(&value));

    let strict_reason = |base_reason: &str| {
        format!(
            "Embedding disabled by strict acceleration mode (JULIE_EMBEDDING_STRICT_ACCEL): {base_reason}"
        )
    };

    let mut config = EmbeddingConfig::default();
    if let Ok(provider) = std::env::var("JULIE_EMBEDDING_PROVIDER") {
        config.provider = provider;
    }
    config.cache_dir = std::env::var("JULIE_EMBEDDING_CACHE_DIR")
        .ok()
        .map(std::path::PathBuf::from);
    config.ort_model_id = std::env::var("JULIE_EMBEDDING_ORT_MODEL_ID").ok();

    // Allow explicit disabling (e.g. CI, tests, offline environments)
    if matches!(
        config.provider.trim().to_ascii_lowercase().as_str(),
        "none" | "disabled" | "off"
    ) {
        info!(
            "Embedding disabled via JULIE_EMBEDDING_PROVIDER={}",
            config.provider
        );
        return (None, None);
    }

    let requested_backend = match parse_provider_preference(&config.provider) {
        Ok(backend) => backend,
        Err(err) => {
            let status = EmbeddingRuntimeStatus {
                requested_backend: EmbeddingBackend::Invalid(config.provider.clone()),
                resolved_backend: EmbeddingBackend::Unresolved,
                accelerated: false,
                degraded_reason: Some(err.to_string()),
            };
            log_runtime_status(None, &status, strict_accel, false);
            warn!(
                "Embedding provider unavailable (keyword search unaffected): {}",
                err
            );
            return (None, Some(status));
        }
    };

    let capabilities = BackendResolverCapabilities::current();
    let resolved_backend =
        match resolve_backend_preference(requested_backend.clone(), &capabilities) {
            Ok(backend) => backend,
            Err(err) => {
                let reason = if strict_accel {
                    strict_reason(&err.to_string())
                } else {
                    err.to_string()
                };
                let status = EmbeddingRuntimeStatus {
                    requested_backend,
                    resolved_backend: EmbeddingBackend::Unresolved,
                    accelerated: false,
                    degraded_reason: Some(reason.clone()),
                };
                log_runtime_status(None, &status, strict_accel, false);
                warn!(
                    "Embedding provider unavailable (keyword search unaffected): {}",
                    reason
                );
                return (None, Some(status));
            }
        };

    match EmbeddingProviderFactory::create(&config) {
        Ok(provider) => {
            let device_info = provider.device_info();
            info!(
                "Embedding provider initialized: {} ({}, {}d)",
                device_info.model_name, device_info.device, device_info.dimensions
            );

            // Warmup probe: run a single inference to verify the GPU compute graph
            // actually works, not just that initialization succeeded. DirectML can
            // init fine but fail on the first real LayerNorm op due to VRAM pressure,
            // driver quirks, or fused-op bugs. Catching it here means the fallback
            // in run_with_cpu_fallback fires on a tiny 1-text call instead of the
            // first 32-text sub-batch of a multi-thousand-symbol pipeline.
            {
                let warmup_start = std::time::Instant::now();
                match provider.embed_query("warmup probe") {
                    Ok(vec) => {
                        info!(
                            "Embedding warmup probe passed ({} dims, {:.0}ms)",
                            vec.len(),
                            warmup_start.elapsed().as_secs_f64() * 1000.0,
                        );
                    }
                    Err(err) => {
                        warn!(
                            "Embedding warmup probe failed ({:.0}ms): {err:#}",
                            warmup_start.elapsed().as_secs_f64() * 1000.0,
                        );
                        // The provider's internal state has already been switched
                        // to CPU by run_with_cpu_fallback, so we continue with
                        // the same provider instance (now on CPU).
                    }
                }
            }

            let degraded_reason = provider.degraded_reason();
            let accelerated = provider
                .accelerated()
                .unwrap_or_else(|| device_info.is_accelerated());

            if should_disable_for_strict_acceleration(
                strict_accel,
                &resolved_backend,
                accelerated,
                degraded_reason.as_deref(),
            ) {
                let strict_degraded_reason =
                    strict_reason(degraded_reason.as_deref().unwrap_or("degraded runtime"));
                warn!(
                    "Embedding provider unavailable (keyword search unaffected): {}",
                    strict_degraded_reason
                );
                let status = EmbeddingRuntimeStatus {
                    requested_backend,
                    resolved_backend,
                    accelerated: false,
                    degraded_reason: Some(strict_degraded_reason),
                };
                log_runtime_status(None, &status, strict_accel, false);
                return (None, Some(status));
            }

            let status = EmbeddingRuntimeStatus {
                requested_backend,
                resolved_backend,
                accelerated,
                degraded_reason,
            };
            log_runtime_status(Some(&*provider), &status, strict_accel, false);
            (Some(provider), Some(status))
        }
        Err(e) => {
            if let Some(fallback_backend) = fallback_backend_after_init_failure(
                requested_backend.clone(),
                resolved_backend.clone(),
                strict_accel,
                capabilities,
            ) {
                warn!(
                    "Embedding backend '{}' failed to initialize, \
                     falling back to '{}': {:#}",
                    resolved_backend.as_str(),
                    fallback_backend.as_str(),
                    e,
                );
                if resolved_backend == EmbeddingBackend::Sidecar {
                    warn!(
                        "Python sidecar unavailable -- common causes: \
                         Python 3.10-3.13 not installed, uv not on PATH, \
                         or sidecar source not found. \
                         Check: uv python install 3.12 && uv --version"
                    );
                }
                let mut fallback_config = config.clone();
                fallback_config.provider = fallback_backend.as_str().to_string();

                match EmbeddingProviderFactory::create(&fallback_config) {
                    Ok(provider) => {
                        let device_info = provider.device_info();
                        info!(
                            "Embedding provider initialized via fallback: {} ({}, {}d)",
                            device_info.model_name, device_info.device, device_info.dimensions
                        );

                        let provider_degraded_reason = provider.degraded_reason();
                        let accelerated = provider
                            .accelerated()
                            .unwrap_or_else(|| device_info.is_accelerated());
                        let fallback_reason = format!(
                            "Auto backend '{}' failed to initialize, fell back to '{}': {}",
                            resolved_backend.as_str(),
                            fallback_backend.as_str(),
                            e
                        );
                        let degraded_reason = provider_degraded_reason
                            .map(|reason| {
                                format!("{fallback_reason}; fallback runtime detail: {reason}")
                            })
                            .or(Some(fallback_reason));

                        let status = EmbeddingRuntimeStatus {
                            requested_backend,
                            resolved_backend: fallback_backend,
                            accelerated,
                            degraded_reason,
                        };
                        log_runtime_status(Some(&*provider), &status, strict_accel, true);
                        return (Some(provider), Some(status));
                    }
                    Err(fallback_error) => {
                        warn!(
                            "Embedding fallback to '{}' failed (keyword search unaffected): {}",
                            fallback_backend.as_str(),
                            fallback_error
                        );
                    }
                }
            }

            warn!(
                "Embedding provider unavailable (keyword search unaffected): {}",
                e
            );
            let status = EmbeddingRuntimeStatus {
                requested_backend,
                resolved_backend,
                accelerated: false,
                degraded_reason: Some(e.to_string()),
            };
            log_runtime_status(None, &status, strict_accel, false);
            (None, Some(status))
        }
    }
}

/// Log embedding runtime status. Replaces the closure that previously read
/// from `self` on `JulieWorkspace`.
fn log_runtime_status(
    provider: Option<&dyn EmbeddingProvider>,
    status: &EmbeddingRuntimeStatus,
    strict_accel: bool,
    fallback_used: bool,
) {
    let provider_info = provider.map(|p| p.device_info());
    let fields = build_embedding_runtime_log_fields(
        status,
        provider_info.as_ref(),
        strict_accel,
        fallback_used,
    );

    info!(
        requested_backend = %fields.requested_backend,
        resolved_backend = %fields.resolved_backend,
        runtime = %fields.runtime,
        device = %fields.device,
        accelerated = fields.accelerated,
        degraded_reason = %fields.degraded_reason,
        telemetry_confidence = %fields.telemetry_confidence,
        strict_mode = fields.strict_mode,
        fallback_used = fields.fallback_used,
        "Embedding runtime status"
    );
}
