//! ONNX Runtime embedding provider using fastembed.
//!
//! Supports multiple models via fastembed's `EmbeddingModel` enum:
//! - **Jina-code-v2** (768d): Code-optimized, default on Windows (DirectML GPU)
//! - **BGE-small-en-v1.5** (384d): General-purpose, smaller/faster
//!
//! GPU acceleration strategy:
//! - **Windows**: DirectML EP → CPU fallback
//! - **macOS/Linux**: CPU only (CoreML EP dropped — 13GB+ memory bloat for 33M-param model)

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use fastembed::{EmbeddingModel, ExecutionProviderDispatch, InitOptions, TextEmbedding};
use ort::execution_providers::CPUExecutionProvider;
#[cfg(target_os = "windows")]
use ort::execution_providers::DirectMLExecutionProvider;

#[cfg(target_os = "windows")]
use super::windows_directml::{choose_directml_adapter, directml_device_label};
use super::{DeviceInfo, EmbeddingProvider};

/// Resolve a model ID string to fastembed enum + dimensions + display name.
///
/// Recognized values (case-insensitive):
/// - `"jina-code-v2"` or `"jinaai/jina-embeddings-v2-base-code"` → Jina code model (768d)
/// - `"bge-small"` or `"BAAI/bge-small-en-v1.5"` → BGE-small (384d)
///
/// Returns `(EmbeddingModel, dimensions, display_name)`.
fn resolve_ort_model(model_id: Option<&str>) -> Result<(EmbeddingModel, usize, &'static str)> {
    let id = model_id
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_else(|| platform_default_ort_model().to_string());

    match id.as_str() {
        "jina-code-v2" | "jinaai/jina-embeddings-v2-base-code" | "jina-embeddings-v2-base-code" => {
            Ok((
                EmbeddingModel::JinaEmbeddingsV2BaseCode,
                768,
                "Jina-code-v2",
            ))
        }
        "bge-small" | "bge-small-en-v1.5" | "baai/bge-small-en-v1.5" => {
            Ok((EmbeddingModel::BGESmallENV15, 384, "BGE-small-en-v1.5"))
        }
        other => bail!(
            "Unknown ORT model '{}'. Supported: jina-code-v2, bge-small",
            other
        ),
    }
}

/// Platform default: Jina-code-v2 on Windows (DirectML GPU), BGE-small elsewhere.
fn platform_default_ort_model() -> &'static str {
    if cfg!(target_os = "windows") {
        "jina-code-v2"
    } else {
        "bge-small"
    }
}

/// Production embedding provider using ONNX Runtime via fastembed.
///
/// Model depends on platform: Jina-code-v2 (768d) on Windows via DirectML,
/// BGE-small-en-v1.5 (384d) elsewhere. Override with `JULIE_EMBEDDING_ORT_MODEL_ID`.
/// Thread-safe via internal `Mutex<TextEmbedding>`.
pub struct OrtEmbeddingProvider {
    model: Mutex<TextEmbedding>,
    runtime_state: Mutex<OrtRuntimeState>,
    cache_dir: PathBuf,
    embedding_model: EmbeddingModel,
    dimensions: usize,
    model_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrtRuntimeSignal {
    pub device: String,
    pub accelerated: bool,
    pub degraded_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OrtRuntimeState {
    pub device: String,
    pub accelerated: bool,
    pub degraded_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct OrtExecutionProviderPolicy {
    providers: Vec<ExecutionProviderDispatch>,
    signal_on_success: OrtRuntimeSignal,
}

impl OrtRuntimeState {
    pub(crate) fn from_signal(signal: OrtRuntimeSignal) -> Self {
        Self {
            device: signal.device,
            accelerated: signal.accelerated,
            degraded_reason: signal.degraded_reason,
        }
    }

    pub(crate) fn mark_cpu_fallback(&mut self, reason: String) {
        self.device = "CPU".to_string();
        self.accelerated = false;
        self.degraded_reason = Some(reason);
    }
}

pub(crate) fn run_with_cpu_fallback<T, Model, Primary, Cpu>(
    runtime_state: &Mutex<OrtRuntimeState>,
    model: &mut Model,
    primary: Primary,
    cpu_fallback: Cpu,
) -> Result<T>
where
    Primary: FnOnce(&mut Model) -> Result<T>,
    Cpu: FnOnce(&anyhow::Error, &mut Model) -> Result<T>,
{
    let (was_accelerated, previous_device) = runtime_state
        .lock()
        .map_err(|e| anyhow::anyhow!("ORT runtime state mutex poisoned: {e}"))
        .map(|state| (state.accelerated, state.device.clone()))?;

    match primary(model) {
        Ok(result) => Ok(result),
        Err(primary_error) if was_accelerated => {
            match cpu_fallback(&primary_error, model) {
                Ok(result) => {
                    runtime_state
                        .lock()
                        .map_err(|e| anyhow::anyhow!("ORT runtime state mutex poisoned: {e}"))?
                        .mark_cpu_fallback(format!(
                            "GPU embedding failed after {previous_device} initialization; switched to CPU: {primary_error}"
                        ));
                    Ok(result)
                }
                Err(cpu_err) => {
                    // CPU rebuild also failed. Mark the provider as degraded so subsequent
                    // calls skip the expensive fallback attempt and just return the GPU error
                    // directly, instead of retrying the rebuild on every single call.
                    tracing::error!(
                        "GPU embedding failed and CPU fallback also failed; \
                         marking provider as permanently degraded. \
                         GPU error: {primary_error:#}. CPU error: {cpu_err:#}"
                    );
                    if let Ok(mut state) = runtime_state.lock() {
                        state.mark_cpu_fallback(format!(
                            "GPU failed ({primary_error}) and CPU fallback failed ({cpu_err})"
                        ));
                    }
                    Err(primary_error)
                }
            }
        }
        Err(primary_error) => Err(primary_error),
    }
}

impl OrtEmbeddingProvider {
    /// Create a new provider, downloading the model if not cached.
    ///
    /// The model is cached at `cache_dir` (defaults to `~/.cache/fastembed/`).
    /// First initialization on a machine triggers a model download (~30-270MB).
    ///
    /// On Windows, tries the selected DirectML adapter first.
    /// On macOS/Linux, uses CPU-only ORT.
    /// Falls back to CPU if the DirectML path fails.
    ///
    /// Returns `Err` if model download fails or ONNX runtime can't initialize.
    /// Callers should treat this as non-fatal — keyword search works without embeddings.
    pub fn try_new(cache_dir: Option<PathBuf>, model_id: Option<&str>) -> Result<Self> {
        let cache = cache_dir.unwrap_or_else(default_cache_dir);
        let (embedding_model, dimensions, model_name) = resolve_ort_model(model_id)?;
        let OrtExecutionProviderPolicy {
            providers,
            signal_on_success,
        } = ort_execution_provider_policy();

        let (model, signal) = if providers.is_empty() {
            // No accelerated EP for this platform — CPU only
            let model =
                TextEmbedding::try_new(base_init_options(embedding_model.clone(), cache.clone()))
                    .context("Failed to initialize fastembed ONNX model")?;
            (model, signal_on_success)
        } else {
            // Try accelerated EP first, fall back to CPU
            let requested_device = signal_on_success.device.clone();
            let primary = TextEmbedding::try_new(
                base_init_options(embedding_model.clone(), cache.clone())
                    .with_execution_providers(providers),
            );

            match primary {
                Ok(model) => (model, signal_on_success),
                Err(primary_error) => {
                    tracing::warn!(
                        "ORT DirectML EP failed for {requested_device}, falling back to CPU: {primary_error:#}"
                    );
                    let model = TextEmbedding::try_new(
                        base_init_options(embedding_model.clone(), cache.clone())
                            .with_execution_providers(vec![CPUExecutionProvider::default().build()]),
                    )
                    .with_context(|| {
                        format!(
                            "Failed to initialize fastembed ONNX model (DirectML attempt failed first for {requested_device}: {primary_error})"
                        )
                    })?;
                    (
                        model,
                        ort_runtime_signal_for_directml_device(&requested_device, true),
                    )
                }
            }
        };

        tracing::info!("ORT embedding model: {model_name} ({dimensions}d)");

        Ok(Self {
            model: Mutex::new(model),
            runtime_state: Mutex::new(OrtRuntimeState::from_signal(signal)),
            cache_dir: cache,
            embedding_model,
            dimensions,
            model_name: model_name.to_string(),
        })
    }

    /// Create a CPU-only provider (deterministic — no GPU non-determinism).
    /// Use this in tests where ranking order must be reproducible.
    #[cfg(test)]
    pub fn try_new_cpu_only(cache_dir: Option<PathBuf>) -> Result<Self> {
        let cache = cache_dir.unwrap_or_else(default_cache_dir);
        // Tests use BGE-small (smaller download, faster, deterministic)
        let model = TextEmbedding::try_new(
            base_init_options(EmbeddingModel::BGESmallENV15, cache.clone())
                .with_execution_providers(vec![CPUExecutionProvider::default().build()]),
        )
        .context("Failed to initialize fastembed ONNX model (CPU-only)")?;

        Ok(Self {
            model: Mutex::new(model),
            runtime_state: Mutex::new(OrtRuntimeState {
                device: "cpu".to_string(),
                accelerated: false,
                degraded_reason: None,
            }),
            cache_dir: cache,
            embedding_model: EmbeddingModel::BGESmallENV15,
            dimensions: 384,
            model_name: "BGE-small-en-v1.5".to_string(),
        })
    }
}

fn base_init_options(model: EmbeddingModel, cache_dir: PathBuf) -> InitOptions {
    InitOptions::new(model)
        .with_cache_dir(cache_dir)
        .with_show_download_progress(false)
}

/// Build the execution provider dispatch list for the current platform.
///
/// Returns an empty vec for platforms without GPU acceleration (Linux).
fn ort_execution_provider_policy() -> OrtExecutionProviderPolicy {
    // macOS: CPU only. CoreML EP causes 13GB+ memory bloat for a 33M-param model
    // due to runtime graph conversion and multi-device tensor staging.
    // BGE-small is small enough that CPU is fast and memory-efficient.
    #[cfg(target_os = "macos")]
    {
        OrtExecutionProviderPolicy {
            providers: vec![],
            signal_on_success: cpu_runtime_signal(None),
        }
    }

    #[cfg(target_os = "windows")]
    {
        match choose_directml_adapter() {
            Ok(Some(adapter)) => {
                let device_label = directml_device_label(&adapter);
                OrtExecutionProviderPolicy {
                    providers: vec![
                        DirectMLExecutionProvider::default()
                            .with_device_id(adapter.index)
                            .build()
                            .error_on_failure(),
                        CPUExecutionProvider::default().build(),
                    ],
                    signal_on_success: ort_runtime_signal_for_directml_device(
                        &device_label,
                        false,
                    ),
                }
            }
            Ok(None) => OrtExecutionProviderPolicy {
                providers: vec![],
                signal_on_success: cpu_runtime_signal(Some(
                    "No eligible DirectML adapter found; skipped software, remote, or virtual adapters"
                        .to_string(),
                )),
            },
            Err(err) => {
                tracing::warn!(
                    "DirectML adapter enumeration failed, falling back to CPU: {err:#}"
                );
                OrtExecutionProviderPolicy {
                    providers: vec![],
                    signal_on_success: cpu_runtime_signal(Some(format!(
                        "DirectML adapter enumeration failed; using CPU: {err}"
                    ))),
                }
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        OrtExecutionProviderPolicy {
            providers: vec![],
            signal_on_success: cpu_runtime_signal(None),
        }
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

fn cpu_runtime_signal(degraded_reason: Option<String>) -> OrtRuntimeSignal {
    OrtRuntimeSignal {
        device: "CPU".to_string(),
        accelerated: false,
        degraded_reason,
    }
}

pub fn ort_runtime_signal_for_directml_device(
    device_label: &str,
    accelerated_ep_fallback_to_cpu: bool,
) -> OrtRuntimeSignal {
    if accelerated_ep_fallback_to_cpu {
        cpu_runtime_signal(Some(format!(
            "ORT DirectML EP requested for {device_label}, but fell back to CPU"
        )))
    } else {
        OrtRuntimeSignal {
            device: device_label.to_string(),
            accelerated: true,
            degraded_reason: None,
        }
    }
}

pub fn ort_runtime_signal(accelerated_ep_fallback_to_cpu: bool) -> OrtRuntimeSignal {
    let ep_name = accelerated_ep_name();

    if ep_name == "None" {
        // No accelerated EP available — pure CPU
        return cpu_runtime_signal(None);
    }

    #[cfg(target_os = "windows")]
    {
        ort_runtime_signal_for_directml_device("DirectML (GPU)", accelerated_ep_fallback_to_cpu)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = accelerated_ep_fallback_to_cpu;
        cpu_runtime_signal(None)
    }
}

impl EmbeddingProvider for OrtEmbeddingProvider {
    fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let mut model = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("Embedding model mutex poisoned: {e}"))?;

        run_with_cpu_fallback(
            &self.runtime_state,
            &mut *model,
            |model| {
                let mut results = model
                    .embed(vec![text.to_string()], None)
                    .context("Failed to embed query")?;
                results
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Embedding returned empty results"))
            },
            |gpu_err, model| {
                tracing::warn!("GPU query embedding failed, falling back to CPU: {gpu_err:#}");
                let mut cpu_model = TextEmbedding::try_new(
                    base_init_options(self.embedding_model.clone(), self.cache_dir.clone())
                        .with_execution_providers(vec![CPUExecutionProvider::default().build()]),
                )
                .context("Failed to initialize CPU fallback embedding model")?;

                let mut results = cpu_model
                    .embed(vec![text.to_string()], None)
                    .context("CPU fallback query embedding also failed")?;
                let result = results
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Embedding returned empty results"))?;

                *model = cpu_model;
                tracing::info!(
                    "GPU→CPU query fallback successful; subsequent requests will use CPU"
                );
                Ok(result)
            },
        )
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // Sub-batch size for ORT inference. Fastembed pads all texts in a
        // sub-batch to the longest sequence, so a single long text forces
        // massive padding for every other text in the same sub-batch. Small
        // sub-batches (32) keep each ORT call's tensor size manageable and
        // prevent overflowing GPU VRAM on models with long context windows
        // (Jina-code-v2 supports 8192 tokens). Without this, a 250-text
        // batch with one long text creates a tensor that exceeds 6GB VRAM,
        // causing DirectML to silently fall back to CPU.
        let ort_sub_batch: Option<usize> = Some(32);

        let mut model = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("Embedding model mutex poisoned: {e}"))?;

        run_with_cpu_fallback(
            &self.runtime_state,
            &mut *model,
            |model| {
                model
                    .embed(texts.to_vec(), ort_sub_batch)
                    .context("Failed to embed batch")
            },
            |gpu_err, model| {
                // GPU driver crash (e.g. DirectML 887A0020) -- rebuild with CPU and retry.
                // Once we swap in the CPU model, all subsequent batches use CPU too.
                tracing::warn!("GPU embedding failed, falling back to CPU: {gpu_err:#}");
                let mut cpu_model = TextEmbedding::try_new(
                    base_init_options(self.embedding_model.clone(), self.cache_dir.clone())
                        .with_execution_providers(vec![CPUExecutionProvider::default().build()]),
                )
                .context("Failed to initialize CPU fallback embedding model")?;

                let result = cpu_model
                    .embed(texts.to_vec(), ort_sub_batch)
                    .context("CPU fallback embedding also failed")?;

                *model = cpu_model;
                tracing::info!(
                    "GPU→CPU embedding fallback successful; subsequent batches will use CPU"
                );
                Ok(result)
            },
        )
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn device_info(&self) -> DeviceInfo {
        let runtime_state = match self.runtime_state.lock() {
            Ok(state) => state.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };

        DeviceInfo {
            runtime: "ort (ONNX Runtime)".to_string(),
            device: runtime_state.device,
            model_name: self.model_name.clone(),
            dimensions: self.dimensions,
        }
    }

    fn accelerated(&self) -> Option<bool> {
        let runtime_state = match self.runtime_state.lock() {
            Ok(state) => state.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };

        Some(runtime_state.accelerated)
    }

    fn degraded_reason(&self) -> Option<String> {
        let runtime_state = match self.runtime_state.lock() {
            Ok(state) => state.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };

        runtime_state.degraded_reason
    }
}

/// Default cache directory for fastembed model files.
///
/// Platform-aware: uses LOCALAPPDATA on Windows (avoids OneDrive-synced
/// profile dirs), XDG cache on Linux, ~/Library/Caches on macOS.
fn default_cache_dir() -> PathBuf {
    // Windows: LOCALAPPDATA (e.g. C:\Users\X\AppData\Local) is not synced by OneDrive.
    // Using $HOME/.cache would land in the user profile, triggering OneDrive sync of
    // hundreds of MB of ONNX model weights.
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(local).join("fastembed");
    }
    // macOS: ~/Library/Caches is the standard non-synced cache location.
    if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Caches")
                .join("fastembed");
        }
    }
    // Linux / fallback: XDG_CACHE_HOME or ~/.cache
    // Use std::env::temp_dir() as final fallback instead of hardcoded "/tmp"
    // so this works on Windows where /tmp doesn't exist.
    let cache_base = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir());
            home.join(".cache")
        });
    cache_base.join("fastembed")
}
