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
pub mod metadata;
#[cfg(feature = "embeddings-ort")]
pub mod ort_provider;
pub mod pipeline;

use anyhow::Result;

/// Information about the embedding device/runtime.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub runtime: String,
    pub device: String,
    pub model_name: String,
    pub dimensions: usize,
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
}

// Re-exports
pub use factory::{EmbeddingConfig, EmbeddingProviderFactory};
#[cfg(feature = "embeddings-ort")]
pub use ort_provider::OrtEmbeddingProvider;
