//! ONNX Runtime embedding provider using fastembed.
//!
//! Wraps `fastembed::TextEmbedding` (BGE-small-en-v1.5, 384-dim) with a `Mutex`
//! to satisfy the `EmbeddingProvider` trait's `&self` requirement despite fastembed's
//! `&mut self` on `embed()`.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use super::{DeviceInfo, EmbeddingProvider};

const BGE_SMALL_DIMENSIONS: usize = 384;

/// Production embedding provider using ONNX Runtime via fastembed.
///
/// Uses BGE-small-en-v1.5 (384 dimensions, ~30MB model).
/// Thread-safe via internal `Mutex<TextEmbedding>`.
pub struct OrtEmbeddingProvider {
    model: Mutex<TextEmbedding>,
    dimensions: usize,
    model_name: String,
}

impl OrtEmbeddingProvider {
    /// Create a new provider, downloading the model if not cached.
    ///
    /// The model is cached at `cache_dir` (defaults to `~/.cache/fastembed/`).
    /// First initialization on a machine triggers a ~30MB download.
    ///
    /// Returns `Err` if model download fails or ONNX runtime can't initialize.
    /// Callers should treat this as non-fatal — keyword search works without embeddings.
    pub fn try_new(cache_dir: Option<PathBuf>) -> Result<Self> {
        let cache = cache_dir.unwrap_or_else(default_cache_dir);

        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15)
                .with_cache_dir(cache)
                .with_show_download_progress(false),
        )
        .context("Failed to initialize fastembed ONNX model")?;

        Ok(Self {
            model: Mutex::new(model),
            dimensions: BGE_SMALL_DIMENSIONS,
            model_name: "BGE-small-en-v1.5".to_string(),
        })
    }
}

impl EmbeddingProvider for OrtEmbeddingProvider {
    fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let mut model = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("Embedding model mutex poisoned: {e}"))?;

        let mut results = model
            .embed(vec![text.to_string()], None)
            .context("Failed to embed query")?;

        results
            .pop()
            .ok_or_else(|| anyhow::anyhow!("Embedding returned empty results"))
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let mut model = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("Embedding model mutex poisoned: {e}"))?;

        model
            .embed(texts.to_vec(), None)
            .context("Failed to embed batch")
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "ort (ONNX Runtime)".to_string(),
            device: "CPU".to_string(),
            model_name: self.model_name.clone(),
            dimensions: self.dimensions,
        }
    }
}

/// Default cache directory: `~/.cache/fastembed/`
fn default_cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".cache").join("fastembed")
}
