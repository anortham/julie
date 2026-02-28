use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use candle_core::{DType, Device, Tensor};
#[cfg(target_os = "macos")]
use candle_coreml::{Config as CoreMLConfig, CoreMLModel};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use hf_hub::api::sync::{ApiBuilder, ApiRepo};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

use super::{DeviceInfo, EmbeddingProvider, EXPECTED_EMBEDDING_DIMENSIONS};

const DEFAULT_MODEL_ID: &str = "BAAI/bge-small-en-v1.5";
const CONFIG_FILE: &str = "config.json";
const TOKENIZER_FILE: &str = "tokenizer.json";
const WEIGHTS_FILE: &str = "model.safetensors";
const MAX_SEQUENCE_LENGTH: usize = 512;
const DEFAULT_VOCAB_SIZE: usize = 30_522;
const DEFAULT_COREML_TOKENIZER_MODEL_ID: &str = "BAAI/bge-small-en-v1.5";
const DEFAULT_COREML_MODEL_FILE: &str = "coreml/feature-extraction/float16_model.mlpackage";
const DEFAULT_COREML_OUTPUT_NAME: &str = "last_hidden_state";
const DEFAULT_COREML_INPUT_NAMES: &str = "input_ids,token_type_ids,attention_mask";
const DEFAULT_COREML_SEQUENCE_LENGTH: usize = 128;

/// Files within an `.mlpackage` directory bundle that must be downloaded individually.
/// HuggingFace `hf-hub` can only fetch single files, but `.mlpackage` is a macOS
/// directory bundle — we download each component and reconstruct the directory locally.
const MLPACKAGE_BUNDLE_FILES: &[&str] = &[
    "Manifest.json",
    "Data/com.apple.CoreML/model.mlmodel",
    "Data/com.apple.CoreML/weights/weight.bin",
];

enum CandleRuntime {
    Transformers(BertModel),
    #[cfg(target_os = "macos")]
    CoreMl(CoreMLRuntimeConfig),
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct CoreMLRuntimeConfig {
    model_path: PathBuf,
    model_config: CoreMLConfig,
}

struct CandleModelState {
    runtime: CandleRuntime,
    tokenizer: Tokenizer,
    device: Device,
    coreml_input_names: Vec<String>,
}

pub struct CandleEmbeddingProvider {
    state: Mutex<CandleModelState>,
    dimensions: usize,
    model_name: String,
    runtime_label: String,
    device_label: String,
    accelerated: bool,
    degraded_reason: Option<String>,
}

impl CandleEmbeddingProvider {
    pub fn try_new(cache_dir: Option<PathBuf>) -> Result<Self> {
        let model_name =
            std::env::var("JULIE_CANDLE_MODEL_ID").unwrap_or_else(|_| DEFAULT_MODEL_ID.to_string());

        let coreml_model_id = std::env::var("JULIE_CANDLE_COREML_MODEL_ID")
            .ok()
            .or_else(|| {
                default_coreml_model_id_for_platform(std::env::consts::OS, std::env::consts::ARCH)
            });
        let mut coreml_fallback_reason = None;
        if coreml_runtime_requested(coreml_model_id.as_deref(), cfg!(target_os = "macos")) {
            let requested_coreml_model_id = coreml_model_id
                .clone()
                .expect("coreml model id should exist when runtime requested");

            match try_new_coreml_state(&requested_coreml_model_id, cache_dir.clone()) {
                Ok((state, device_label, accelerated, degraded_reason)) => {
                    return Ok(Self {
                        state: Mutex::new(state),
                        dimensions: EXPECTED_EMBEDDING_DIMENSIONS,
                        model_name: requested_coreml_model_id,
                        runtime_label: "candle-coreml".to_string(),
                        device_label,
                        accelerated,
                        degraded_reason,
                    });
                }
                Err(err) => {
                    coreml_fallback_reason = Some(format!(
                        "CoreML runtime initialization failed; falling back to Candle transformers: {err:#}"
                    ));
                }
            }
        }

        let repo = open_model_repo(&model_name, cache_dir)?;
        let config_path = repo
            .get(CONFIG_FILE)
            .with_context(|| format!("Failed to fetch {CONFIG_FILE} for {model_name}"))?;
        let tokenizer_path = repo
            .get(TOKENIZER_FILE)
            .with_context(|| format!("Failed to fetch {TOKENIZER_FILE} for {model_name}"))?;
        let weights_path = repo
            .get(WEIGHTS_FILE)
            .with_context(|| format!("Failed to fetch {WEIGHTS_FILE} for {model_name}"))?;

        let config = load_model_config(&config_path)?;
        validate_output_dimensions(Some(config.hidden_size))?;

        let tokenizer = load_tokenizer(&tokenizer_path)?;
        let (mut device, mut device_label, mut accelerated, base_degraded_reason) =
            select_runtime_device();
        let mut state = CandleModelState {
            runtime: CandleRuntime::Transformers(load_model(
                &weights_path,
                &config,
                &device,
                &model_name,
            )?),
            tokenizer,
            device: device.clone(),
            coreml_input_names: vec![],
        };

        if let Err(warmup_error) = run_warmup_inference(&mut state) {
            if device.is_metal() {
                device = Device::Cpu;
                state.runtime = CandleRuntime::Transformers(load_model(
                    &weights_path,
                    &config,
                    &device,
                    &model_name,
                )?);
                state.device = device.clone();
                run_warmup_inference(&mut state).context("Candle CPU fallback warmup failed")?;

                device_label = "CPU".to_string();
                accelerated = false;
                let fallback_reason =
                    format!("Candle Metal runtime warmup failed, fell back to CPU: {warmup_error:#}");
                let degraded_reason =
                    combine_degraded_reasons(base_degraded_reason, Some(fallback_reason));
                let degraded_reason =
                    combine_degraded_reasons(degraded_reason, coreml_fallback_reason);

                return Ok(Self {
                    state: Mutex::new(state),
                    dimensions: EXPECTED_EMBEDDING_DIMENSIONS,
                    model_name,
                    runtime_label: "candle-transformers".to_string(),
                    device_label,
                    accelerated,
                    degraded_reason,
                });
            }

            return Err(warmup_error).context("Candle warmup inference failed");
        }

        let degraded_reason =
            combine_degraded_reasons(base_degraded_reason, coreml_fallback_reason);
        Ok(Self {
            state: Mutex::new(state),
            dimensions: EXPECTED_EMBEDDING_DIMENSIONS,
            model_name,
            runtime_label: "candle-transformers".to_string(),
            device_label,
            accelerated,
            degraded_reason,
        })
    }
}

fn combine_degraded_reasons(primary: Option<String>, secondary: Option<String>) -> Option<String> {
    match (primary, secondary) {
        (Some(a), Some(b)) => Some(format!("{a}; {b}")),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn open_model_repo(model_name: &str, cache_dir: Option<PathBuf>) -> Result<ApiRepo> {
    let mut builder = ApiBuilder::new().with_progress(false);
    if let Some(cache_dir) = cache_dir {
        builder = builder.with_cache_dir(cache_dir);
    }
    if let Ok(endpoint) = std::env::var("HF_ENDPOINT") {
        builder = builder.with_endpoint(endpoint);
    }

    let api = builder
        .build()
        .context("Failed to initialize HuggingFace API for Candle model")?;
    Ok(api.model(model_name.to_string()))
}

/// Downloads all files within an `.mlpackage` directory bundle from HuggingFace,
/// then copies them with resolved symlinks so CoreML can load the bundle.
///
/// hf-hub caches files as symlinks to content-addressed blobs. Apple's CoreML
/// framework may not follow these symlinks when compiling `.mlpackage` bundles,
/// so we create a real copy with dereferenced files.
fn download_mlpackage(repo: &ApiRepo, mlpackage_path: &str) -> Result<PathBuf> {
    // Download Manifest.json first — it's at the root of the bundle,
    // so its parent path gives us the .mlpackage directory
    let manifest = repo
        .get(&format!("{mlpackage_path}/Manifest.json"))
        .with_context(|| format!("Failed to fetch {mlpackage_path}/Manifest.json"))?;
    let symlinked_dir = manifest
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine mlpackage directory from manifest path"))?
        .to_path_buf();

    // Download remaining bundle files (model weights + mlmodel)
    for file in &MLPACKAGE_BUNDLE_FILES[1..] {
        repo.get(&format!("{mlpackage_path}/{file}"))
            .with_context(|| format!("Failed to fetch {mlpackage_path}/{file}"))?;
    }

    // Resolve symlinks: copy to a sibling directory with real files
    let resolved_dir = symlinked_dir.with_extension("resolved.mlpackage");
    if !resolved_dir.join("Manifest.json").exists() {
        resolve_mlpackage_symlinks(&symlinked_dir, &resolved_dir)?;
    }

    Ok(resolved_dir)
}

/// Copies mlpackage bundle files from a symlinked hf-hub cache to a real directory.
fn resolve_mlpackage_symlinks(src: &Path, dst: &Path) -> Result<()> {
    for file in MLPACKAGE_BUNDLE_FILES {
        let src_file = src.join(file);
        let dst_file = dst.join(file);
        if let Some(parent) = dst_file.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory for {file}"))?;
        }
        std::fs::copy(&src_file, &dst_file)
            .with_context(|| format!("Failed to copy {file} to resolved mlpackage"))?;
    }
    Ok(())
}

fn load_model_config(config_path: &Path) -> Result<BertConfig> {
    let config_json = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read model config at {}", config_path.display()))?;

    serde_json::from_str::<BertConfig>(&config_json)
        .with_context(|| format!("Failed to parse model config at {}", config_path.display()))
}

fn load_tokenizer(tokenizer_path: &Path) -> Result<Tokenizer> {
    let mut tokenizer = Tokenizer::from_file(tokenizer_path)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("Failed to load tokenizer at {}", tokenizer_path.display()))?;

    tokenizer.with_padding(Some(PaddingParams {
        strategy: PaddingStrategy::BatchLongest,
        ..Default::default()
    }));

    tokenizer
        .with_truncation(Some(TruncationParams {
            max_length: MAX_SEQUENCE_LENGTH,
            ..Default::default()
        }))
        .map_err(anyhow::Error::msg)
        .context("Failed to configure tokenizer truncation")?;

    Ok(tokenizer)
}

/// Like `load_tokenizer` but pads to a fixed length instead of batch-longest.
/// CoreML models are exported with fixed input shapes, so every input must be
/// padded to exactly that length.
fn load_tokenizer_fixed_length(tokenizer_path: &Path, sequence_length: usize) -> Result<Tokenizer> {
    let mut tokenizer = Tokenizer::from_file(tokenizer_path)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("Failed to load tokenizer at {}", tokenizer_path.display()))?;

    tokenizer.with_padding(Some(PaddingParams {
        strategy: PaddingStrategy::Fixed(sequence_length),
        ..Default::default()
    }));

    tokenizer
        .with_truncation(Some(TruncationParams {
            max_length: sequence_length,
            ..Default::default()
        }))
        .map_err(anyhow::Error::msg)
        .context("Failed to configure tokenizer truncation")?;

    Ok(tokenizer)
}

fn select_runtime_device() -> (Device, String, bool, Option<String>) {
    if std::env::consts::OS == "macos" {
        match Device::metal_if_available(0) {
            Ok(device) if device.is_metal() => (device, "Metal (MPS)".to_string(), true, None),
            Ok(device) => (
                device,
                "CPU".to_string(),
                false,
                Some("Candle Metal backend unavailable; using CPU".to_string()),
            ),
            Err(err) => (
                Device::Cpu,
                "CPU".to_string(),
                false,
                Some(format!(
                    "Candle Metal device initialization failed; using CPU: {err}"
                )),
            ),
        }
    } else {
        (Device::Cpu, "CPU".to_string(), false, None)
    }
}

fn load_model(
    weights_path: &Path,
    config: &BertConfig,
    device: &Device,
    model_name: &str,
) -> Result<BertModel> {
    let var_builder = unsafe {
        VarBuilder::from_mmaped_safetensors(&[weights_path.to_path_buf()], DType::F32, device)
            .with_context(|| {
                format!(
                    "Failed to map Candle safetensors model for {} from {}",
                    model_name,
                    weights_path.display()
                )
            })?
    };

    BertModel::load(var_builder, config)
        .with_context(|| format!("Failed to initialize Candle BERT model for {model_name}"))
}

fn encode_inputs(
    tokenizer: &mut Tokenizer,
    texts: &[String],
    device: &Device,
) -> Result<(Tensor, Tensor)> {
    let encodings = tokenizer
        .encode_batch(texts.to_vec(), true)
        .map_err(anyhow::Error::msg)
        .context("Failed to tokenize input batch for Candle embeddings")?;

    let input_ids = encodings
        .iter()
        .map(|encoding| Tensor::new(encoding.get_ids(), device))
        .collect::<candle_core::Result<Vec<_>>>()?;
    let attention_masks = encodings
        .iter()
        .map(|encoding| Tensor::new(encoding.get_attention_mask(), device))
        .collect::<candle_core::Result<Vec<_>>>()?;

    let input_ids = Tensor::stack(&input_ids, 0).context("Failed to stack Candle input IDs")?;
    let attention_masks =
        Tensor::stack(&attention_masks, 0).context("Failed to stack Candle attention masks")?;
    Ok((input_ids, attention_masks))
}

fn mean_pool(token_embeddings: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
    let attention_mask = attention_mask.to_dtype(DType::F32)?.unsqueeze(2)?;
    let sum_mask = attention_mask.sum(1)?;
    let pooled = (token_embeddings.broadcast_mul(&attention_mask)?).sum(1)?;
    Ok(pooled.broadcast_div(&sum_mask)?)
}

fn normalize_l2(vectors: &Tensor) -> Result<Tensor> {
    Ok(vectors.broadcast_div(&vectors.sqr()?.sum_keepdim(1)?.sqrt()?)?)
}

fn run_warmup_inference(state: &mut CandleModelState) -> Result<()> {
    let warmup_inputs = vec!["warmup sentence".to_string()];
    let embeddings = compute_embeddings_for_state(state, &warmup_inputs)?;
    validate_output_dimensions(embeddings.first().map(Vec::len))
}

fn compute_embeddings_for_state(
    state: &mut CandleModelState,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(vec![]);
    }

    // CoreML models are compiled with fixed batch size 1 — process one at a time
    #[cfg(target_os = "macos")]
    if matches!(state.runtime, CandleRuntime::CoreMl(_)) {
        return compute_embeddings_coreml_sequential(state, texts);
    }

    compute_embeddings_batch(state, texts)
}

/// Batch inference for the transformer runtime (handles variable batch sizes).
fn compute_embeddings_batch(
    state: &mut CandleModelState,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    let device = state.device.clone();
    let (input_ids, attention_mask) = encode_inputs(&mut state.tokenizer, texts, &device)?;
    let token_type_ids = input_ids.zeros_like()?;

    let token_embeddings = match &state.runtime {
        CandleRuntime::Transformers(model) => model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask))
            .context("Candle transformers forward pass failed")?,
        #[cfg(target_os = "macos")]
        CandleRuntime::CoreMl(_) => unreachable!("CoreML uses sequential path"),
    };

    finalize_embeddings(&token_embeddings, &attention_mask)
}

/// Sequential inference for CoreML (batch size 1 per forward pass).
#[cfg(target_os = "macos")]
fn compute_embeddings_coreml_sequential(
    state: &mut CandleModelState,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    let device = state.device.clone();
    let coreml_input_names = state.coreml_input_names.clone();
    let runtime = match &state.runtime {
        CandleRuntime::CoreMl(rt) => rt,
        _ => unreachable!("called coreml_sequential with non-CoreML runtime"),
    };

    let model = load_coreml_model(runtime)?;

    let mut all_embeddings = Vec::with_capacity(texts.len());
    for text in texts {
        let single = &[text.clone()];
        let (input_ids, attention_mask) = encode_inputs(&mut state.tokenizer, single, &device)?;
        let token_type_ids = input_ids.zeros_like()?;

        let token_embeddings =
            coreml_forward(&model, &input_ids, &token_type_ids, &attention_mask, &coreml_input_names)?;

        let mut embeddings = finalize_embeddings(&token_embeddings, &attention_mask)?;
        all_embeddings.append(&mut embeddings);
    }

    Ok(all_embeddings)
}

/// Post-process raw token embeddings: mean-pool, normalize, validate dimensions.
fn finalize_embeddings(
    token_embeddings: &Tensor,
    attention_mask: &Tensor,
) -> Result<Vec<Vec<f32>>> {
    let pooled = match token_embeddings.rank() {
        3 => mean_pool(token_embeddings, attention_mask)
            .context("Failed to mean-pool token embeddings")?,
        2 => token_embeddings.clone(),
        rank => bail!(
            "Unsupported embedding output rank {rank}; expected rank 2 or 3 for Candle runtime"
        ),
    };

    let normalized = normalize_l2(&pooled).context("Failed to normalize Candle embeddings")?;
    let normalized = normalized.to_device(&Device::Cpu)?;
    let embeddings = normalized
        .to_vec2::<f32>()
        .context("Failed to extract embeddings from tensor")?;

    validate_output_dimensions(embeddings.first().map(Vec::len))?;
    if embeddings
        .iter()
        .any(|embedding| embedding.len() != EXPECTED_EMBEDDING_DIMENSIONS)
    {
        bail!(
            "Candle embedding output dimensions are inconsistent (expected {} for all rows)",
            EXPECTED_EMBEDDING_DIMENSIONS
        );
    }

    Ok(embeddings)
}

#[cfg(target_os = "macos")]
fn load_coreml_model(runtime: &CoreMLRuntimeConfig) -> Result<CoreMLModel> {
    CoreMLModel::load_from_file(&runtime.model_path, &runtime.model_config)
        .map_err(anyhow::Error::msg)
        .with_context(|| {
            format!(
                "Failed to load CoreML embedding model from {}",
                runtime.model_path.display()
            )
        })
}

#[cfg(target_os = "macos")]
fn coreml_forward(
    model: &CoreMLModel,
    input_ids: &Tensor,
    token_type_ids: &Tensor,
    attention_mask: &Tensor,
    input_names: &[String],
) -> Result<Tensor> {
    // CoreML only accepts F32 and I64 tensors, but tokenizers produce U32.
    let input_ids_i64 = input_ids.to_dtype(DType::I64)?;
    let token_type_ids_i64 = token_type_ids.to_dtype(DType::I64)?;
    let attention_mask_i64 = attention_mask.to_dtype(DType::I64)?;

    let mut inputs = Vec::with_capacity(input_names.len());
    for input_name in input_names {
        match input_name.as_str() {
            "input_ids" => inputs.push(&input_ids_i64),
            "token_type_ids" => inputs.push(&token_type_ids_i64),
            "attention_mask" => inputs.push(&attention_mask_i64),
            unknown => {
                bail!(
                    "Unsupported CoreML input name '{unknown}'. Use input names composed of input_ids, token_type_ids, attention_mask."
                )
            }
        }
    }

    model
        .forward(&inputs)
        .map_err(anyhow::Error::msg)
        .context("CoreML forward pass failed")
}

pub(crate) fn coreml_runtime_requested(model_id: Option<&str>, is_macos: bool) -> bool {
    is_macos
        && model_id
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
}

pub(crate) fn default_coreml_model_id_for_platform(_os: &str, _arch: &str) -> Option<String> {
    // CoreML uses batch=1 sequential inference, which is slower than
    // Candle Transformers + Metal batched inference for bulk embedding.
    // Opt-in via JULIE_CANDLE_COREML_MODEL_ID env var if needed.
    None
}

pub(crate) fn parse_coreml_input_names(raw: Option<&str>) -> Vec<String> {
    let parsed = raw
        .unwrap_or(DEFAULT_COREML_INPUT_NAMES)
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if parsed.is_empty() {
        return vec![
            "input_ids".to_string(),
            "token_type_ids".to_string(),
            "attention_mask".to_string(),
        ];
    }

    parsed
}

#[cfg(target_os = "macos")]
fn try_new_coreml_state(
    coreml_model_id: &str,
    cache_dir: Option<PathBuf>,
) -> Result<(CandleModelState, String, bool, Option<String>)> {
    let repo = open_model_repo(coreml_model_id, cache_dir.clone())?;
    let tokenizer_model_id = std::env::var("JULIE_CANDLE_COREML_TOKENIZER_MODEL_ID")
        .unwrap_or_else(|_| DEFAULT_COREML_TOKENIZER_MODEL_ID.to_string());
    let tokenizer_repo = open_model_repo(&tokenizer_model_id, cache_dir)?;

    let model_file = std::env::var("JULIE_CANDLE_COREML_MODEL_FILE")
        .unwrap_or_else(|_| DEFAULT_COREML_MODEL_FILE.to_string());
    let output_name = std::env::var("JULIE_CANDLE_COREML_OUTPUT_NAME")
        .unwrap_or_else(|_| DEFAULT_COREML_OUTPUT_NAME.to_string());
    let input_names = parse_coreml_input_names(
        std::env::var("JULIE_CANDLE_COREML_INPUT_NAMES")
            .ok()
            .as_deref(),
    );

    let model_path = download_mlpackage(&repo, &model_file)
        .with_context(|| format!("Failed to fetch mlpackage {model_file} for {coreml_model_id}"))?;
    let tokenizer_path = tokenizer_repo.get(TOKENIZER_FILE).with_context(|| {
        format!("Failed to fetch {TOKENIZER_FILE} for tokenizer model {tokenizer_model_id}")
    })?;

    let coreml_sequence_length: usize = std::env::var("JULIE_CANDLE_COREML_SEQUENCE_LENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_COREML_SEQUENCE_LENGTH);

    let vocab_size = match tokenizer_repo.get(CONFIG_FILE) {
        Ok(config_path) => match load_model_config(&config_path) {
            Ok(config) => config.vocab_size,
            Err(_) => DEFAULT_VOCAB_SIZE,
        },
        Err(_) => DEFAULT_VOCAB_SIZE,
    };

    let coreml_config = CoreMLConfig {
        input_names: input_names.clone(),
        output_name,
        max_sequence_length: coreml_sequence_length,
        vocab_size,
        model_type: "bert".to_string(),
    };

    let runtime_device = match Device::metal_if_available(0) {
        Ok(device) => device,
        Err(_) => Device::Cpu,
    };

    let mut state = CandleModelState {
        runtime: CandleRuntime::CoreMl(CoreMLRuntimeConfig {
            model_path: model_path.clone(),
            model_config: coreml_config,
        }),
        tokenizer: load_tokenizer_fixed_length(&tokenizer_path, coreml_sequence_length)?,
        device: runtime_device,
        coreml_input_names: input_names,
    };

    let mut degraded_reason = None;
    if let Err(warmup_error) = run_warmup_inference(&mut state) {
        if state.device.is_metal() {
            state.device = Device::Cpu;
            run_warmup_inference(&mut state).with_context(|| {
                format!(
                    "CoreML warmup failed on Metal and CPU fallback also failed: {warmup_error:#}"
                )
            })?;
            degraded_reason = Some(format!(
                "CoreML warmup failed on Metal input tensors and fell back to CPU tensors: {warmup_error:#}"
            ));
        } else {
            return Err(warmup_error).context("CoreML warmup inference failed");
        }
    }

    let device_label = if state.device.is_metal() {
        "Metal (CoreML input)".to_string()
    } else {
        "CPU (CoreML input)".to_string()
    };
    let accelerated = state.device.is_metal();

    Ok((state, device_label, accelerated, degraded_reason))
}

#[cfg(not(target_os = "macos"))]
fn try_new_coreml_state(
    _coreml_model_id: &str,
    _cache_dir: Option<PathBuf>,
) -> Result<(CandleModelState, String, bool, Option<String>)> {
    bail!("CoreML runtime is only available on macOS")
}

pub(crate) fn validate_output_dimensions(effective_dimensions: Option<usize>) -> Result<()> {
    let Some(effective_dimensions) = effective_dimensions else {
        bail!(
            "candle embedding effective model output dim unavailable (expected {EXPECTED_EMBEDDING_DIMENSIONS})."
        );
    };

    if effective_dimensions != EXPECTED_EMBEDDING_DIMENSIONS {
        bail!(
            "candle embedding dimension mismatch: effective model output dim expected {EXPECTED_EMBEDDING_DIMENSIONS}, got {effective_dimensions}. Set JULIE_EMBEDDING_PROVIDER=ort or configure a 384-dim Candle/CoreML model."
        );
    }

    Ok(())
}

impl EmbeddingProvider for CandleEmbeddingProvider {
    fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed_batch(&[text.to_string()])?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Candle embedding returned empty results"))
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| anyhow::anyhow!("Candle model mutex poisoned: {e}"))?;
        compute_embeddings_for_state(&mut state, texts)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: self.runtime_label.clone(),
            device: self.device_label.clone(),
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
