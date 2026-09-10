use crate::embeddings::{EmbeddingProvider, EmbeddingRuntimeStatus};

use super::{EmbeddingRuntimeHealth, EmbeddingState, HealthLevel};

pub(crate) fn project_embedding_runtime(
    runtime_status: Option<EmbeddingRuntimeStatus>,
    provider: Option<&dyn EmbeddingProvider>,
) -> EmbeddingRuntimeHealth {
    match (runtime_status, provider) {
        (Some(runtime), Some(provider)) => {
            let device_info = provider.device_info();
            let degraded_reason = runtime.degraded_reason.clone();
            let level = if degraded_reason.is_some() {
                HealthLevel::Degraded
            } else {
                HealthLevel::Ready
            };
            let state = if degraded_reason.is_some() {
                EmbeddingState::Degraded
            } else {
                EmbeddingState::Initialized
            };

            EmbeddingRuntimeHealth {
                level,
                state,
                runtime: device_info.runtime,
                requested_backend: runtime.requested_backend.as_str().to_string(),
                backend: runtime.resolved_backend.as_str().to_string(),
                device: device_info.device,
                accelerated: runtime.accelerated,
                detail: degraded_reason.unwrap_or_else(|| "none".to_string()),
                query_fallback: "semantic".to_string(),
            }
        }
        (Some(runtime), None) => EmbeddingRuntimeHealth {
            level: HealthLevel::Unavailable,
            state: EmbeddingState::Unavailable,
            runtime: "unavailable".to_string(),
            requested_backend: runtime.requested_backend.as_str().to_string(),
            backend: runtime.resolved_backend.as_str().to_string(),
            device: "unavailable".to_string(),
            accelerated: runtime.accelerated,
            detail: runtime.degraded_reason.unwrap_or_else(|| {
                "embedding runtime metadata exists but provider is missing".to_string()
            }),
            query_fallback: "keyword-only".to_string(),
        },
        (None, Some(provider)) => {
            let device_info = provider.device_info();
            let accelerated = provider
                .accelerated()
                .unwrap_or_else(|| device_info.is_accelerated());

            EmbeddingRuntimeHealth {
                level: HealthLevel::Degraded,
                state: EmbeddingState::NotInitialized,
                runtime: device_info.runtime,
                requested_backend: "unresolved".to_string(),
                backend: "unresolved".to_string(),
                device: device_info.device,
                accelerated,
                detail: "provider exists without runtime metadata".to_string(),
                query_fallback: "semantic".to_string(),
            }
        }
        (None, None) => EmbeddingRuntimeHealth {
            level: HealthLevel::Unavailable,
            state: EmbeddingState::NotInitialized,
            runtime: "unavailable".to_string(),
            requested_backend: "unresolved".to_string(),
            backend: "unresolved".to_string(),
            device: "unavailable".to_string(),
            accelerated: false,
            detail: "none".to_string(),
            query_fallback: "keyword-only".to_string(),
        },
    }
}
