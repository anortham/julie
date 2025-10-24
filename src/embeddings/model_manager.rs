// Model Management - Download and cache ONNX models from HuggingFace
//
// This module handles downloading BGE-Small-EN-V1.5 ONNX model and tokenizer
// from HuggingFace Hub with caching to avoid repeated downloads.

use anyhow::{Context, Result};
use hf_hub::api::tokio::{Api, ApiBuilder};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Paths to the downloaded model files
#[derive(Debug, Clone)]
pub struct ModelPaths {
    /// Path to the ONNX model file (model.onnx or model_quantized.onnx)
    pub model: PathBuf,
    /// Path to the tokenizer configuration (tokenizer.json)
    pub tokenizer: PathBuf,
}

/// Manages downloading and caching of ONNX embedding models
pub struct ModelManager {
    cache_dir: PathBuf,
    api: Api,
}

impl ModelManager {
    /// Create a new model manager with the specified cache directory
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        // Ensure cache directory exists
        std::fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create cache directory: {:?}", cache_dir))?;

        // Create HuggingFace API client with custom cache location
        let api = ApiBuilder::new()
            .with_cache_dir(cache_dir.clone())
            .build()
            .context("Failed to create HuggingFace API client")?;

        Ok(Self { cache_dir, api })
    }

    /// Ensure the model is downloaded and return paths to model files
    ///
    /// Downloads from HuggingFace if not already cached.
    /// Currently supports:
    /// - `bge-small` → BAAI/bge-small-en-v1.5
    pub async fn ensure_model_downloaded(&self, model_name: &str) -> Result<ModelPaths> {
        match model_name {
            "bge-small" | "bge-small-en-v1.5" => self.download_bge_small().await,
            _ => {
                anyhow::bail!(
                    "Unsupported model: {}. Currently only 'bge-small' is supported.",
                    model_name
                )
            }
        }
    }

    /// Download BGE-Small-EN-V1.5 model from HuggingFace
    ///
    /// Model: BAAI/bge-small-en-v1.5
    /// Files: model.onnx (~130MB), tokenizer.json (~450KB)
    async fn download_bge_small(&self) -> Result<ModelPaths> {
        let repo_id = "BAAI/bge-small-en-v1.5";

        info!("📥 Ensuring BGE-Small-EN-V1.5 model is available...");

        // Get the repository handle
        let repo = self.api.model(repo_id.to_string());

        // Download required files
        // HuggingFace Hub will cache these and skip download if already present
        info!("📥 Downloading model.onnx (this may take a while on first run)...");
        let model_path = repo
            .get("onnx/model.onnx")
            .await
            .with_context(|| format!("Failed to download model.onnx from {}", repo_id))?;

        info!("📥 Downloading tokenizer.json...");
        let tokenizer_path = repo
            .get("tokenizer.json")
            .await
            .with_context(|| format!("Failed to download tokenizer.json from {}", repo_id))?;

        // Verify files exist and are readable
        if !model_path.exists() {
            anyhow::bail!("Model file not found after download: {:?}", model_path);
        }
        if !tokenizer_path.exists() {
            anyhow::bail!(
                "Tokenizer file not found after download: {:?}",
                tokenizer_path
            );
        }

        info!("✅ BGE-Small model ready");
        info!("   Model: {:?}", model_path);
        info!("   Tokenizer: {:?}", tokenizer_path);

        Ok(ModelPaths {
            model: model_path,
            tokenizer: tokenizer_path,
        })
    }

    /// Get the cache directory path
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Check if a model is already cached (doesn't require async)
    pub fn is_model_cached(&self, _model_name: &str) -> bool {
        // HuggingFace Hub uses specific cache structure
        // For now, just check if cache directory exists
        // The actual check will happen during download attempt
        let cache_exists = self.cache_dir.exists();

        if cache_exists {
            info!("📂 Cache directory exists: {:?}", self.cache_dir);
        } else {
            warn!(
                "📂 Cache directory does not exist yet: {:?}",
                self.cache_dir
            );
        }

        cache_exists
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_model_manager_creation() {
        let temp_dir = tempdir().unwrap();
        let cache_dir = temp_dir.path().join("models");

        let manager = ModelManager::new(cache_dir.clone()).unwrap();
        assert_eq!(manager.cache_dir(), cache_dir);
        assert!(cache_dir.exists(), "Cache directory should be created");
    }

    #[test]
    fn test_unsupported_model() {
        let temp_dir = tempdir().unwrap();
        let manager = ModelManager::new(temp_dir.path().to_path_buf()).unwrap();

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result =
            runtime.block_on(async { manager.ensure_model_downloaded("unsupported-model").await });

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unsupported model"));
    }

    // Note: Actual download test would require network access and is slow
    // Skip in unit tests, test in integration tests instead
    #[tokio::test]
    #[ignore] // Only run with --ignored flag
    async fn test_download_bge_small() {
        let temp_dir = tempdir().unwrap();
        let manager = ModelManager::new(temp_dir.path().to_path_buf()).unwrap();

        let paths = manager.ensure_model_downloaded("bge-small").await.unwrap();

        assert!(paths.model.exists(), "Model file should exist");
        assert!(paths.tokenizer.exists(), "Tokenizer file should exist");

        // Verify model file is reasonably sized (~130MB)
        let model_size = std::fs::metadata(&paths.model).unwrap().len();
        assert!(model_size > 100_000_000, "Model should be >100MB");

        // Verify tokenizer file exists
        let tokenizer_size = std::fs::metadata(&paths.tokenizer).unwrap().len();
        assert!(tokenizer_size > 1000, "Tokenizer should be >1KB");
    }
}
