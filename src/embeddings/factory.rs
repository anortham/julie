use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Result};

use super::EmbeddingProvider;
#[cfg(feature = "embeddings-ort")]
use super::OrtEmbeddingProvider;

/// Runtime configuration for embedding provider selection.
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub cache_dir: Option<PathBuf>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "ort".to_string(),
            cache_dir: None,
        }
    }
}

pub struct EmbeddingProviderFactory;

impl EmbeddingProviderFactory {
    pub fn create(config: &EmbeddingConfig) -> Result<Arc<dyn EmbeddingProvider>> {
        match config.provider.to_ascii_lowercase().as_str() {
            "ort" => {
                #[cfg(feature = "embeddings-ort")]
                {
                    return Ok(Arc::new(OrtEmbeddingProvider::try_new(
                        config.cache_dir.clone(),
                    )?));
                }

                #[cfg(not(feature = "embeddings-ort"))]
                {
                    bail!("Embedding provider 'ort' is not available in this build");
                }
            }
            unknown => bail!("Unknown embedding provider: {}", unknown),
        }
    }
}
