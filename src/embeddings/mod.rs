//! Semantic embedding infrastructure — relocated to `julie_pipeline::embeddings`.

// Re-export all public items from julie_pipeline::embeddings
pub use julie_pipeline::embeddings::*;

// Re-export submodules so `crate::embeddings::factory::*` etc. remain valid
pub use julie_pipeline::embeddings::factory;
pub use julie_pipeline::embeddings::init;
pub use julie_pipeline::embeddings::metadata;
pub use julie_pipeline::embeddings::pipeline;
#[cfg(feature = "embeddings-sidecar")]
pub use julie_pipeline::embeddings::sidecar_bootstrap;
#[cfg(feature = "embeddings-sidecar")]
pub use julie_pipeline::embeddings::sidecar_embedded;
#[cfg(feature = "embeddings-sidecar")]
pub use julie_pipeline::embeddings::sidecar_protocol;
#[cfg(feature = "embeddings-sidecar")]
pub use julie_pipeline::embeddings::sidecar_provider;
#[cfg(feature = "embeddings-sidecar")]
pub use julie_pipeline::embeddings::sidecar_supervisor;

// log_fields re-exported for the top crate
pub use julie_pipeline::embeddings::log_fields;
