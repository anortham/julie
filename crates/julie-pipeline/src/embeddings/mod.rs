//! Semantic embedding infrastructure for Julie.
//!
//! This module provides vector embedding generation for symbol metadata,
//! enabling semantic search that bridges vocabulary mismatch
//! (e.g., "error handling" → `CircuitBreakerService`).
//!
//! # Architecture
//!
//! - [`EmbeddingProvider`] — trait abstracting embedding generation
//! - [`NativeEmbeddingProvider`] — production implementation over the native `julie-semantic-sidecar`
//! - Vectors are fact rows in `facts.sqlite`, served from the snapshot vector set

pub mod factory;
pub mod init;
pub mod log_fields;
pub mod metadata;
pub mod native;
pub mod pipeline;
pub mod sidecar_protocol;

// Core embedding contract types live in julie-core (bottom leaf crate) so
// that any future sibling crate can share the same definitions without
// depending on the full `julie` crate. All existing `crate::embeddings::*`
// import paths remain valid through these re-exports.
pub use julie_core::embeddings_contract::{
    DeviceInfo, EmbeddingBackend, EmbeddingProvider, EmbeddingRequestBudget,
    EmbeddingRuntimeStatus, EncoderIdentity,
};

pub use factory::{
    BackendResolverCapabilities, EmbeddingConfig, EmbeddingProviderFactory,
    parse_provider_preference, resolve_backend_preference, should_disable_for_strict_acceleration,
    strict_acceleration_enabled_from_env_value,
};
pub use init::create_embedding_provider;
pub use native::NativeEmbeddingProvider;
pub use sidecar_protocol::{
    DeviceBackendCapabilities, DeviceBackendCapability, DeviceLoadPolicy, EmbedBatchRequest,
    EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, HealthResult, ProtocolError,
    RequestEnvelope, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
    validate_batch_response, validate_health_response, validate_query_response,
    validate_response_envelope,
};
