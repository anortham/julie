//! ONNX Runtime embedding provider using fastembed.
//!
//! Wraps `fastembed::TextEmbedding` (BGE-small-en-v1.5, 384-dim) with a `Mutex`
//! to satisfy the `EmbeddingProvider` trait's `&self` requirement despite fastembed's
//! `&mut self` on `embed()`.
//!
//! GPU acceleration strategy:
//! - **Windows**: DirectML EP → CPU fallback
//! - **macOS/Linux**: CPU only (CoreML EP dropped — 13GB+ memory bloat for 33M-param model)

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, ExecutionProviderDispatch, InitOptions, TextEmbedding};
use ort::execution_providers::CPUExecutionProvider;
#[cfg(target_os = "windows")]
use ort::execution_providers::DirectMLExecutionProvider;

use super::{DeviceInfo, EmbeddingProvider};

const BGE_SMALL_DIMENSIONS: usize = 384;

/// Production embedding provider using ONNX Runtime via fastembed.
///
/// Uses BGE-small-en-v1.5 (384 dimensions, ~30MB model).
/// Thread-safe via internal `Mutex<TextEmbedding>`.
pub struct OrtEmbeddingProvider {
    model: Mutex<TextEmbedding>,
    cache_dir: PathBuf,
    dimensions: usize,
    model_name: String,
    device: String,
    accelerated: bool,
    degraded_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrtRuntimeSignal {
    pub device: String,
    pub accelerated: bool,
    pub degraded_reason: Option<String>,
}

impl OrtEmbeddingProvider {
    /// Create a new provider, downloading the model if not cached.
    ///
    /// The model is cached at `cache_dir` (defaults to `~/.cache/fastembed/`).
    /// First initialization on a machine triggers a ~30MB download.
    ///
    /// On macOS, tries CoreML EP first (GPU + Neural Engine acceleration).
    /// On Windows, tries DirectML EP first (GPU acceleration).
    /// Falls back to CPU if the accelerated EP fails.
    ///
    /// Returns `Err` if model download fails or ONNX runtime can't initialize.
    /// Callers should treat this as non-fatal — keyword search works without embeddings.
    pub fn try_new(cache_dir: Option<PathBuf>) -> Result<Self> {
        let cache = cache_dir.unwrap_or_else(default_cache_dir);
        let policy = ort_execution_provider_policy();

        let (model, signal) = if policy.is_empty() {
            // No accelerated EP for this platform — CPU only
            let model = TextEmbedding::try_new(base_init_options(cache.clone()))
                .context("Failed to initialize fastembed ONNX model")?;
            (model, ort_runtime_signal(false))
        } else {
            // Try accelerated EP first, fall back to CPU
            let ep_name = accelerated_ep_name();
            let primary = TextEmbedding::try_new(
                base_init_options(cache.clone()).with_execution_providers(policy),
            );

            match primary {
                Ok(model) => (model, ort_runtime_signal(false)),
                Err(primary_error) => {
                    tracing::warn!(
                        "ORT {ep_name} EP failed, falling back to CPU: {primary_error:#}"
                    );
                    let model = TextEmbedding::try_new(
                        base_init_options(cache.clone())
                            .with_execution_providers(vec![CPUExecutionProvider::default().build()]),
                    )
                    .with_context(|| {
                        format!(
                            "Failed to initialize fastembed ONNX model ({ep_name} attempt failed first: {primary_error})"
                        )
                    })?;
                    (model, ort_runtime_signal(true))
                }
            }
        };

        Ok(Self {
            model: Mutex::new(model),
            cache_dir: cache,
            dimensions: BGE_SMALL_DIMENSIONS,
            model_name: "BGE-small-en-v1.5".to_string(),
            device: signal.device,
            accelerated: signal.accelerated,
            degraded_reason: signal.degraded_reason,
        })
    }

    /// Create a CPU-only provider (deterministic — no GPU non-determinism).
    /// Use this in tests where ranking order must be reproducible.
    #[cfg(test)]
    pub fn try_new_cpu_only(cache_dir: Option<PathBuf>) -> Result<Self> {
        let cache = cache_dir.unwrap_or_else(default_cache_dir);
        let model = TextEmbedding::try_new(
            base_init_options(cache.clone())
                .with_execution_providers(vec![CPUExecutionProvider::default().build()]),
        )
        .context("Failed to initialize fastembed ONNX model (CPU-only)")?;

        Ok(Self {
            model: Mutex::new(model),
            cache_dir: cache,
            dimensions: BGE_SMALL_DIMENSIONS,
            model_name: "BGE-small-en-v1.5".to_string(),
            device: "cpu".to_string(),
            accelerated: false,
            degraded_reason: None,
        })
    }
}

fn base_init_options(cache_dir: PathBuf) -> InitOptions {
    InitOptions::new(EmbeddingModel::BGESmallENV15)
        .with_cache_dir(cache_dir)
        .with_show_download_progress(false)
}

/// Build the execution provider dispatch list for the current platform.
///
/// Returns an empty vec for platforms without GPU acceleration (Linux).
fn ort_execution_provider_policy() -> Vec<ExecutionProviderDispatch> {
    // macOS: CPU only. CoreML EP causes 13GB+ memory bloat for a 33M-param model
    // due to runtime graph conversion and multi-device tensor staging.
    // BGE-small is small enough that CPU is fast and memory-efficient.
    #[cfg(target_os = "macos")]
    {
        vec![]
    }

    #[cfg(target_os = "windows")]
    {
        vec![
            DirectMLExecutionProvider::default()
                .build()
                .error_on_failure(),
            CPUExecutionProvider::default().build(),
        ]
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        vec![]
    }
}

/// Human-readable name of the accelerated EP for this platform.
fn accelerated_ep_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "DirectML"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "None"
    }
}

pub fn ort_execution_provider_policy_kinds() -> Vec<&'static str> {
    #[cfg(target_os = "windows")]
    {
        vec!["directml", "cpu"]
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec![]
    }
}

pub fn ort_runtime_signal(accelerated_ep_fallback_to_cpu: bool) -> OrtRuntimeSignal {
    let ep_name = accelerated_ep_name();

    if ep_name == "None" {
        // No accelerated EP available — pure CPU
        return OrtRuntimeSignal {
            device: "CPU".to_string(),
            accelerated: false,
            degraded_reason: None,
        };
    }

    if accelerated_ep_fallback_to_cpu {
        OrtRuntimeSignal {
            device: "CPU".to_string(),
            accelerated: false,
            degraded_reason: Some(format!("ORT {ep_name} EP requested but fell back to CPU")),
        }
    } else {
        OrtRuntimeSignal {
            device: format!("{ep_name} (GPU)"),
            accelerated: true,
            degraded_reason: None,
        }
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

        match model.embed(texts.to_vec(), None) {
            Ok(result) => Ok(result),
            Err(gpu_err) if self.accelerated => {
                // GPU driver crash (e.g. DirectML 887A0020) — rebuild with CPU and retry.
                // Once we swap in the CPU model, all subsequent batches use CPU too.
                tracing::warn!(
                    "GPU embedding failed, falling back to CPU: {gpu_err:#}"
                );
                let mut cpu_model = TextEmbedding::try_new(
                    base_init_options(self.cache_dir.clone())
                        .with_execution_providers(vec![CPUExecutionProvider::default().build()]),
                )
                .context("Failed to initialize CPU fallback embedding model")?;

                let result = cpu_model
                    .embed(texts.to_vec(), None)
                    .context("CPU fallback embedding also failed")?;

                *model = cpu_model;
                tracing::info!(
                    "GPU→CPU embedding fallback successful; subsequent batches will use CPU"
                );
                Ok(result)
            }
            Err(err) => Err(err).context("Failed to embed batch"),
        }
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "ort (ONNX Runtime)".to_string(),
            device: self.device.clone(),
            model_name: self.model_name.clone(),
            dimensions: self.dimensions,
        }
    }

    fn accelerated(&self) -> Option<bool> {
        Some(self.accelerated)
    }

    fn degraded_reason(&self) -> Option<String> {
        self.degraded_reason.clone()
    }
}

/// Default cache directory: `~/.cache/fastembed/`
fn default_cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".cache").join("fastembed")
}
