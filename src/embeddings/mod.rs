//! Semantic embedding infrastructure for Julie.
//!
//! This module provides vector embedding generation for symbol metadata,
//! enabling semantic search that bridges vocabulary mismatch
//! (e.g., "error handling" → `CircuitBreakerService`).
//!
//! # Architecture
//!
//! - [`EmbeddingProvider`] — trait abstracting embedding generation
//! - [`OrtEmbeddingProvider`] — production implementation using fastembed (ONNX Runtime)
//! - Vector storage lives in `database::vectors` (sqlite-vec)

pub mod factory;
pub mod init;
pub mod metadata;
#[cfg(feature = "embeddings-ort")]
pub mod ort_provider;
pub mod pipeline;
#[cfg(feature = "embeddings-sidecar")]
pub mod sidecar_bootstrap;
#[cfg(feature = "embeddings-sidecar")]
pub mod sidecar_embedded;
#[cfg(feature = "embeddings-sidecar")]
pub mod sidecar_protocol;
#[cfg(feature = "embeddings-sidecar")]
pub mod sidecar_provider;
#[cfg(feature = "embeddings-sidecar")]
pub mod sidecar_supervisor;
#[cfg(feature = "embeddings-ort")]
pub mod windows_directml;

use anyhow::Result;

pub const SIDECAR_BACKEND_COMPILED: bool = cfg!(feature = "embeddings-sidecar");

/// Supported embedding backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddingBackend {
    Auto,
    Sidecar,
    Ort,
    Unresolved,
    Invalid(String),
}

impl EmbeddingBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Sidecar => "sidecar",
            Self::Ort => "ort",
            Self::Unresolved => "unresolved",
            Self::Invalid(_) => "invalid",
        }
    }
}

/// Runtime status for embedding initialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingRuntimeStatus {
    pub requested_backend: EmbeddingBackend,
    pub resolved_backend: EmbeddingBackend,
    pub accelerated: bool,
    pub degraded_reason: Option<String>,
}

/// Information about the embedding device/runtime.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub runtime: String,
    pub device: String,
    pub model_name: String,
    pub dimensions: usize,
}

impl DeviceInfo {
    /// Best-effort hardware acceleration detection for diagnostics.
    pub fn is_accelerated(&self) -> bool {
        let combined = format!(
            "{} {}",
            self.runtime.to_ascii_lowercase(),
            self.device.to_ascii_lowercase()
        );

        if combined.contains("cpu") {
            return false;
        }

        [
            "gpu", "cuda", "rocm", "mps", "metal", "directml", "dml", "vulkan", "coreml",
        ]
        .iter()
        .any(|hint| combined.contains(hint))
    }
}

/// Trait abstracting vector embedding generation.
///
/// Implementations must be `Send + Sync` for use behind `Arc` in async contexts.
/// The trait uses `&self` despite fastembed's `&mut self` requirement — implementors
/// are expected to use interior mutability (e.g., `Mutex`).
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single query string. Returns a vector of `dimensions()` floats.
    fn embed_query(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed a batch of texts. Returns one vector per input text.
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// The dimensionality of produced embeddings (e.g., 384 for BGE-small).
    fn dimensions(&self) -> usize;

    /// Runtime and device information for diagnostics.
    fn device_info(&self) -> DeviceInfo;

    /// Provider-reported acceleration state, if known.
    fn accelerated(&self) -> Option<bool> {
        None
    }

    /// Provider-reported degraded runtime reason, if known.
    fn degraded_reason(&self) -> Option<String> {
        None
    }

    /// Explicitly shut down the provider, releasing any child processes.
    /// Default is a no-op; sidecar providers override to kill the child process.
    fn shutdown(&self) {}
}

// Re-exports
pub use factory::{
    BackendResolverCapabilities, EmbeddingConfig, EmbeddingProviderFactory,
    fallback_backend_after_init_failure, parse_provider_preference, resolve_backend_preference,
    should_disable_for_strict_acceleration, strict_acceleration_enabled_from_env_value,
};
pub use init::create_embedding_provider;
#[cfg(feature = "embeddings-ort")]
pub use ort_provider::{
    OrtEmbeddingProvider, ort_execution_provider_policy_kinds, ort_runtime_signal,
};
#[cfg(feature = "embeddings-sidecar")]
pub use sidecar_protocol::{
    EmbedBatchRequest, EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, ProtocolError,
    RequestEnvelope, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
    validate_batch_response, validate_query_response, validate_response_envelope,
};
#[cfg(feature = "embeddings-sidecar")]
pub use sidecar_provider::SidecarEmbeddingProvider;
#[cfg(feature = "embeddings-sidecar")]
pub use sidecar_supervisor::{
    SidecarLaunchConfig, build_sidecar_launch_config, managed_venv_path, sidecar_root_path,
};
