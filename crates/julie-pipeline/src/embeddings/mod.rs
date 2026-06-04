//! Semantic embedding infrastructure for Julie.
//!
//! This module provides vector embedding generation for symbol metadata,
//! enabling semantic search that bridges vocabulary mismatch
//! (e.g., "error handling" → `CircuitBreakerService`).
//!
//! # Architecture
//!
//! - [`EmbeddingProvider`] — trait abstracting embedding generation
//! - [`SidecarEmbeddingProvider`] — production implementation using a managed Python sidecar
//! - Vector storage lives in `database::vectors` (sqlite-vec)

pub mod factory;
pub mod init;
pub mod log_fields;
pub mod metadata;
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

// Core embedding contract types live in julie-core (bottom leaf crate) so
// that any future sibling crate can share the same definitions without
// depending on the full `julie` crate. All existing `crate::embeddings::*`
// import paths remain valid through these re-exports.
pub use julie_core::embeddings_contract::{
    DeviceInfo, EmbeddingBackend, EmbeddingProvider, EmbeddingRuntimeStatus,
};

// Re-exports
pub use factory::{
    BackendResolverCapabilities, EmbeddingConfig, EmbeddingProviderFactory,
    parse_provider_preference, resolve_backend_preference, should_disable_for_strict_acceleration,
    strict_acceleration_enabled_from_env_value,
};
pub use init::create_embedding_provider;
#[cfg(feature = "embeddings-sidecar")]
pub use sidecar_protocol::{
    DeviceBackendCapabilities, DeviceBackendCapability, DeviceLoadPolicy, EmbedBatchRequest,
    EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, HealthResult, ProtocolError,
    RequestEnvelope, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
    validate_batch_response, validate_health_response, validate_query_response,
    validate_response_envelope,
};
#[cfg(feature = "embeddings-sidecar")]
pub use sidecar_provider::SidecarEmbeddingProvider;
#[cfg(feature = "embeddings-sidecar")]
pub use sidecar_supervisor::{
    SidecarLaunchConfig, build_sidecar_launch_config, managed_venv_path, sidecar_root_path,
};
