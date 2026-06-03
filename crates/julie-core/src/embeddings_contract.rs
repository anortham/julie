//! Shared embedding contract types — the trait and its companion structs.
//!
//! Lives here in `julie-core` (the bottom leaf crate) so that both the main
//! `julie` crate and any future sibling crates can depend on a single
//! definition. The `julie::embeddings` module re-exports everything from here,
//! so all existing `crate::embeddings::*` import paths remain valid.

use anyhow::Result;
use std::time::Duration;

/// Supported embedding backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddingBackend {
    Auto,
    Sidecar,
    Unresolved,
    Invalid(String),
}

impl EmbeddingBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Sidecar => "sidecar",
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
/// The trait uses `&self`; implementors are expected to use interior mutability
/// (e.g., `Mutex`) if their underlying runtime requires `&mut self`.
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

    /// Wait for the provider's underlying child process to exit, up to `timeout`.
    ///
    /// Returns `true` if the child exited within the timeout, `false` if the
    /// timeout elapsed before the child exited. Providers without a child
    /// process return `true` immediately (no-op default).
    ///
    /// This is a blocking call. Callers in async context should run it via
    /// `tokio::task::spawn_blocking`.
    fn wait_for_exit(&self, _timeout: Duration) -> bool {
        true
    }
}
