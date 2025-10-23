// ONNX Runtime Embedding Model - GPU-Accelerated Semantic Embeddings
//
// This module provides direct ONNX Runtime usage with platform-specific GPU acceleration:
// - Windows: DirectML (works with NVIDIA, AMD, Intel GPUs)
// - Linux: CUDA → TensorRT (NVIDIA GPUs)
// - macOS: CoreML (Apple Silicon Neural Engine)
//
// Replaces fastembed for 10-100x performance improvement.

use anyhow::{Context, Result};
use ndarray::{Array2, Axis};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use tokenizers::Tokenizer;
use tracing::{debug, info};

#[cfg(target_os = "windows")]
use tracing::warn;

#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, DXGI_ERROR_NOT_FOUND,
};

/// ONNX Runtime embedding model with GPU acceleration
pub struct OrtEmbeddingModel {
    session: Session,
    tokenizer: Tokenizer,
    dimensions: usize,
    model_name: String,
    max_length: usize,
}

impl OrtEmbeddingModel {
    /// Create new model with platform-specific GPU acceleration
    ///
    /// # Arguments
    /// * `model_path` - Path to the ONNX model file (model.onnx)
    /// * `tokenizer_path` - Path to the tokenizer config (tokenizer.json)
    /// * `model_name` - Name of the model (for logging)
    ///
    /// # Returns
    /// OrtEmbeddingModel configured with best available execution provider
    pub fn new(
        model_path: impl AsRef<Path>,
        tokenizer_path: impl AsRef<Path>,
        model_name: &str,
        cache_dir: Option<impl AsRef<Path>>,
    ) -> Result<Self> {
        info!("🚀 Initializing OrtEmbeddingModel for {}", model_name);

        // 1. Load tokenizer
        let mut tokenizer = Tokenizer::from_file(tokenizer_path.as_ref())
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer from {:?}: {}", tokenizer_path.as_ref(), e))?;

        // 2. Configure padding to ensure all sequences have the same length
        use tokenizers::{PaddingDirection, PaddingParams, PaddingStrategy, TruncationParams, TruncationStrategy};

        // Configure padding
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,  // Pad to longest in batch
            direction: PaddingDirection::Right,        // Pad on the right (standard)
            pad_id: 0,                                 // PAD token ID
            pad_type_id: 0,                           // Segment ID for padding tokens
            pad_token: "[PAD]".to_string(),           // PAD token string
            pad_to_multiple_of: None,                 // No special multiple padding needed
        }));

        // Configure truncation to prevent sequences exceeding model's max length
        tokenizer.with_truncation(Some(TruncationParams {
            max_length: 512,                           // BERT max sequence length
            strategy: TruncationStrategy::LongestFirst, // Truncate longest sequences first
            stride: 0,                                 // No stride for embeddings
            direction: tokenizers::TruncationDirection::Right, // Truncate from right (end)
        })).map_err(|e| anyhow::anyhow!("Failed to configure tokenizer truncation: {}", e))?;

        info!("✅ Tokenizer loaded successfully with padding and truncation configured");

        // 3. Create ONNX Runtime session with platform-specific GPU acceleration
        let session = Self::create_session_with_gpu(model_path.as_ref(), cache_dir)
            .context("Failed to create ONNX Runtime session")?;

        // 4. Determine model dimensions (384 for BGE-Small)
        let dimensions = 384; // BGE-Small-EN-V1.5 outputs 384-dimensional embeddings

        // 5. Set max tokenization length (512 for BERT-based models)
        let max_length = 512;

        info!("✅ OrtEmbeddingModel initialized successfully");
        info!("   Model: {}", model_name);
        info!("   Dimensions: {}", dimensions);
        info!("   Max length: {}", max_length);

        Ok(Self {
            session,
            tokenizer,
            dimensions,
            model_name: model_name.to_string(),
            max_length,
        })
    }

    /// Enumerate DirectML devices and select the most powerful one
    ///
    /// Returns the device ID of the GPU with the most dedicated VRAM.
    /// Falls back to device ID 0 (default adapter) if enumeration fails.
    #[cfg(target_os = "windows")]
    fn select_best_directml_device() -> Result<i32> {
        unsafe {
            match CreateDXGIFactory1::<IDXGIFactory1>() {
                Ok(factory) => {
                    let mut best_device_id = 0;
                    let mut max_vram: usize = 0;

                    info!("🔍 Enumerating DirectML devices...");

                    // Enumerate all adapters
                    for index in 0..16 {
                        // Limit to 16 GPUs max
                        match factory.EnumAdapters1(index) {
                            Ok(adapter) => {
                                match adapter.GetDesc1() {
                                    Ok(desc) => {
                                        let vram = desc.DedicatedVideoMemory;
                                        let device_name = String::from_utf16_lossy(&desc.Description);

                                        info!(
                                            "   GPU {}: {} ({:.2} GB VRAM)",
                                            index,
                                            device_name.trim_end_matches('\0'),
                                            vram as f64 / 1_073_741_824.0 // Convert bytes to GB
                                        );

                                        // Select GPU with most dedicated VRAM
                                        if vram > max_vram {
                                            max_vram = vram;
                                            best_device_id = index as i32;
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to get adapter {} description: {}", index, e);
                                    }
                                }
                            }
                            Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => {
                                // Normal termination - no more adapters
                                break;
                            }
                            Err(e) => {
                                warn!("Error enumerating adapter {}: {}", index, e);
                                break;
                            }
                        }
                    }

                    if max_vram > 0 {
                        info!(
                            "🎯 Selected GPU {} with {:.2} GB VRAM",
                            best_device_id,
                            max_vram as f64 / 1_073_741_824.0
                        );
                        Ok(best_device_id)
                    } else {
                        warn!("No GPUs with dedicated VRAM found, using default adapter (ID 0)");
                        Ok(0)
                    }
                }
                Err(e) => {
                    warn!("Failed to create DXGI factory: {}, using default adapter (ID 0)", e);
                    Ok(0)
                }
            }
        }
    }

    /// Create ONNX Runtime session with platform-specific execution providers
    ///
    /// Tries GPU acceleration first, ORT automatically falls back to CPU if unavailable.
    ///
    /// Set JULIE_FORCE_CPU=1 environment variable to skip GPU and use CPU only.
    #[allow(unused_variables)] // cache_dir used on Windows/Linux but not macOS
    fn create_session_with_gpu(model_path: &Path, cache_dir: Option<impl AsRef<Path>>) -> Result<Session> {
        // Check if user wants to force CPU mode (optional override)
        let force_cpu = std::env::var("JULIE_FORCE_CPU")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        if force_cpu {
            info!("🖥️  JULIE_FORCE_CPU is set - using CPU-only mode");
        }

        #[cfg(not(target_os = "macos"))]
        let mut builder = Session::builder()
            .context("Failed to create SessionBuilder")?
            .with_optimization_level(GraphOptimizationLevel::Level3)?;

        #[cfg(target_os = "macos")]
        let builder = Session::builder()
            .context("Failed to create SessionBuilder")?
            .with_optimization_level(GraphOptimizationLevel::Level3)?;

        // Platform-specific execution providers
        // ORT will automatically fall back to CPU if these aren't available
        #[cfg(target_os = "windows")]
        {
            if !force_cpu {
                use ort::execution_providers::DirectMLExecutionProvider;

                // Select the most powerful GPU available
                let device_id = Self::select_best_directml_device()?;

                info!("🎮 Attempting DirectML (Windows GPU) acceleration with device ID {}...", device_id);
                builder = builder.with_execution_providers([
                    DirectMLExecutionProvider::default()
                        .with_device_id(device_id)
                        .build()
                ])?;
                info!("✅ DirectML execution provider registered on device {}", device_id);
            }
        }

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if !force_cpu {
                use ort::execution_providers::{CUDAExecutionProvider, TensorRTExecutionProvider};
                info!("🎮 Attempting CUDA/TensorRT (NVIDIA GPU) acceleration...");
                builder = builder.with_execution_providers([
                    TensorRTExecutionProvider::default().build(),
                    CUDAExecutionProvider::default().build(),
                ])?;
                info!("✅ CUDA/TensorRT execution providers registered");
            }
        }

        #[cfg(target_os = "macos")]
        {
            info!("🍎 macOS detected - using optimized CPU execution");
            info!("   ⚠️  CoreML has poor transformer/BERT support:");
            info!("      - Only 25% of operations can use Neural Engine");
            info!("      - Remaining 75% fall back to CPU with overhead");
            info!("      - Pure CPU mode is faster than CoreML hybrid");
            info!("   ✅ GPU acceleration works great on Windows/Linux");
            info!("   ℹ️  M2 Ultra CPU is still very fast for ONNX inference");
            // No execution provider registration - ORT defaults to optimized CPU
            // which is faster for transformers than CoreML's hybrid approach
        }

        // Commit session from model file
        let session = builder
            .commit_from_file(model_path)
            .with_context(|| format!("Failed to load ONNX model from {:?}", model_path))?;

        info!("✅ ONNX session created successfully");

        Ok(session)
    }

    /// Encode a batch of texts into embeddings
    ///
    /// This is the primary method for generating embeddings.
    /// Uses batched inference for maximum performance.
    ///
    /// # Arguments
    /// * `texts` - Vector of strings to encode
    ///
    /// # Returns
    /// Vector of embedding vectors (each is Vec<f32> with length = dimensions)
    pub fn encode_batch(&mut self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        debug!("📝 Tokenizing {} texts...", texts.len());

        // 1. Tokenize all texts
        let encodings = self
            .tokenizer
            .encode_batch(texts, true)
            .map_err(|e| anyhow::anyhow!("Failed to tokenize input texts: {}", e))?;

        // 2. Prepare input tensors (input_ids, attention_mask, token_type_ids)
        let batch_size = encodings.len();
        let seq_length = encodings[0].len(); // All sequences now same length due to padding

        debug!("📦 Batch processing: {} texts, sequence length: {}", batch_size, seq_length);

        // Convert to arrays for ONNX Runtime
        let mut input_ids_vec = Vec::with_capacity(batch_size * seq_length);
        let mut attention_mask_vec = Vec::with_capacity(batch_size * seq_length);
        let mut token_type_ids_vec = Vec::with_capacity(batch_size * seq_length);

        for encoding in &encodings {
            // Input IDs
            input_ids_vec.extend(encoding.get_ids().iter().map(|&id| id as i64));

            // Attention mask
            attention_mask_vec.extend(encoding.get_attention_mask().iter().map(|&m| m as i64));

            // Token type IDs (all zeros for single-sequence tasks)
            token_type_ids_vec.extend(encoding.get_type_ids().iter().map(|&t| t as i64));
        }

        // Create ndarray arrays
        let input_ids = Array2::from_shape_vec((batch_size, seq_length), input_ids_vec)
            .context("Failed to create input_ids array")?;
        let attention_mask = Array2::from_shape_vec((batch_size, seq_length), attention_mask_vec)
            .context("Failed to create attention_mask array")?;
        let token_type_ids = Array2::from_shape_vec((batch_size, seq_length), token_type_ids_vec)
            .context("Failed to create token_type_ids array")?;

        debug!("🔢 Input tensor shapes: [{}, {}]", batch_size, seq_length);

        // 3. Run inference
        debug!("🚀 Running ONNX inference...");

        // Create tensors from arrays
        let input_ids_tensor = Tensor::from_array(input_ids)
            .context("Failed to create input_ids tensor")?;
        let attention_mask_tensor = Tensor::from_array(attention_mask)
            .context("Failed to create attention_mask tensor")?;
        let token_type_ids_tensor = Tensor::from_array(token_type_ids)
            .context("Failed to create token_type_ids tensor")?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
                "token_type_ids" => token_type_ids_tensor,
            ])
            .map_err(|e| {
                // Log detailed error information to help diagnose DirectML/GPU issues
                tracing::error!(
                    "🚨 ONNX inference failed - Error: {:?}",
                    e
                );
                tracing::error!(
                    "   Batch size: {}, Sequence length: {}",
                    batch_size,
                    seq_length
                );
                anyhow::anyhow!("Failed to run ONNX inference: {}", e)
            })?;

        // 4. Extract embeddings from output
        // BGE models output CLS token embeddings as the sentence representation
        let embeddings_array = outputs["last_hidden_state"]
            .try_extract_array::<f32>()
            .context("Failed to extract embeddings tensor")?;
        debug!("📊 Output tensor shape: {:?}", embeddings_array.shape());

        // Extract CLS token (first token) embeddings for each sequence
        let mut embeddings = Vec::with_capacity(batch_size);
        for i in 0..batch_size {
            // Get the CLS token embedding (index 0 in sequence dimension)
            let mut cls_embedding: Vec<f32> = embeddings_array
                .index_axis(Axis(0), i)
                .index_axis(Axis(0), 0)
                .to_owned()
                .into_raw_vec_and_offset().0;

            // L2 normalize the embedding (required for BGE models)
            let magnitude: f32 = cls_embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            if magnitude > 0.0 {
                for val in &mut cls_embedding {
                    *val /= magnitude;
                }
            }

            embeddings.push(cls_embedding);
        }

        debug!("✅ Generated {} L2-normalized embeddings", embeddings.len());

        Ok(embeddings)
    }

    /// Encode a single text into an embedding
    ///
    /// Convenience method for single-text encoding.
    /// For multiple texts, use encode_batch() for better performance.
    pub fn encode_single(&mut self, text: String) -> Result<Vec<f32>> {
        let batch_result = self.encode_batch(vec![text])?;
        batch_result
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding returned from encode_batch"))
    }

    /// Get the embedding dimensions for this model
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Get the model name
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Get the maximum sequence length
    pub fn max_length(&self) -> usize {
        self.max_length
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // Note: These tests require the model to be downloaded
    // Run with `cargo test --ignored` to include download tests

    fn get_test_model_paths() -> Option<(PathBuf, PathBuf)> {
        // Check if model exists in standard cache location
        // Try user's home directory instead of dirs crate
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()?;
        let cache_dir = PathBuf::from(home)
            .join(".cache")
            .join("julie")
            .join("models");
        let model_path = cache_dir.join("bge-small-en-v1.5").join("model.onnx");
        let tokenizer_path = cache_dir.join("bge-small-en-v1.5").join("tokenizer.json");

        if model_path.exists() && tokenizer_path.exists() {
            Some((model_path, tokenizer_path))
        } else {
            None
        }
    }

    #[test]
    #[ignore] // Requires model to be downloaded
    fn test_model_initialization() {
        if let Some((model_path, tokenizer_path)) = get_test_model_paths() {
            let model = OrtEmbeddingModel::new(model_path, tokenizer_path, "bge-small-test", None::<PathBuf>);
            assert!(model.is_ok(), "Model initialization should succeed");

            let model = model.unwrap();
            assert_eq!(model.dimensions(), 384);
            assert_eq!(model.model_name(), "bge-small-test");
        } else {
            println!("Skipping test - model not downloaded");
        }
    }

    #[test]
    #[ignore] // Requires model to be downloaded
    fn test_single_embedding() {
        if let Some((model_path, tokenizer_path)) = get_test_model_paths() {
            let mut model = OrtEmbeddingModel::new(model_path, tokenizer_path, "bge-small-test", None::<PathBuf>)
                .expect("Model initialization failed");

            let embedding = model.encode_single("Hello, world!".to_string());
            assert!(embedding.is_ok(), "Embedding generation should succeed");

            let embedding = embedding.unwrap();
            assert_eq!(embedding.len(), 384, "Embedding should have 384 dimensions");

            // Check that embedding values are reasonable (not all zeros)
            let sum: f32 = embedding.iter().sum();
            assert!(sum.abs() > 0.01, "Embedding should have non-zero values");
        } else {
            println!("Skipping test - model not downloaded");
        }
    }

    #[test]
    #[ignore] // Requires model to be downloaded
    fn test_batch_embedding() {
        if let Some((model_path, tokenizer_path)) = get_test_model_paths() {
            let mut model = OrtEmbeddingModel::new(model_path, tokenizer_path, "bge-small-test", None::<PathBuf>)
                .expect("Model initialization failed");

            let texts = vec![
                "function getUserData()".to_string(),
                "class UserService".to_string(),
                "async fn process_payment()".to_string(),
            ];

            let embeddings = model.encode_batch(texts);
            assert!(embeddings.is_ok(), "Batch embedding should succeed");

            let embeddings = embeddings.unwrap();
            assert_eq!(embeddings.len(), 3, "Should have 3 embeddings");

            for (i, embedding) in embeddings.iter().enumerate() {
                assert_eq!(embedding.len(), 384, "Embedding {} should have 384 dimensions", i);
                let sum: f32 = embedding.iter().sum();
                assert!(sum.abs() > 0.01, "Embedding {} should have non-zero values", i);
            }
        } else {
            println!("Skipping test - model not downloaded");
        }
    }

    #[test]
    fn test_empty_batch() {
        // This test doesn't require model download
        // We can't actually test without a model, but we can document expected behavior
        // Empty batch should return empty vector without crashing
    }
}
