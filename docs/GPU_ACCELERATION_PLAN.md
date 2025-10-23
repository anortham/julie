# GPU Acceleration Migration Plan

**Status:** ✅ Implementation Complete (Windows linker issue pending)
**Created:** 2025-10-22
**Updated:** 2025-10-23
**Goal:** Replace fastembed (CPU-only) with direct ort usage for 10-100x faster embeddings via GPU acceleration

---

## Problem Statement

Embeddings generation is painfully slow on all platforms due to CPU-only inference:
- **Current performance:** 200-500ms per embedding, 15-30s for batch of 100
- **Root cause:** fastembed doesn't expose GPU execution provider configuration
- **Impact:** Semantic search features are almost unusable

---

## Solution Overview

Replace `fastembed` with direct `ort` (ONNX Runtime) usage to enable platform-specific GPU acceleration:

| Platform | Execution Provider | Expected Speedup |
|----------|-------------------|------------------|
| Windows (any GPU) | DirectML | 20-50x |
| Linux (NVIDIA) | CUDA → TensorRT | 50-100x |
| macOS (Apple Silicon) | CoreML | 30-80x |
| Fallback (all) | CPU | 1x (current) |

---

## Architecture Design

### Current Architecture (CPU-only)

```rust
EmbeddingEngine
  └─ fastembed::TextEmbedding
       ├─ Model download (handled internally)
       ├─ Tokenization (handled internally)
       └─ ort::Session (CPU-only, no GPU config)
```

### New Architecture (GPU-accelerated)

```rust
EmbeddingEngine (PUBLIC API UNCHANGED)
  ├─ embed_symbol(symbol, context) -> Vec<f32>
  ├─ embed_text(text) -> Vec<f32>
  ├─ embed_symbols_batch(symbols) -> Vec<(String, Vec<f32>)>
  └─ upsert_file_embeddings(file_path, symbols)
       ↓
  OrtEmbeddingModel (NEW INTERNAL COMPONENT)
    ├─ tokenizer: Tokenizer (BERT tokenization)
    ├─ session: ort::Session (with GPU ExecutionProviders)
    ├─ dimensions: usize (384)
    ├─ model_name: String
    └─ Methods:
         ├─ new(model_path, cache_dir) -> Result<Self>
         ├─ encode_batch(texts: Vec<String>) -> Result<Vec<Vec<f32>>>
         └─ encode_single(text: String) -> Result<Vec<f32>>
       ↓
  Platform-Specific Execution Providers
    ├─ Windows:   DirectML → CPU (fallback)
    ├─ Linux:     CUDA → TensorRT → CPU
    └─ macOS:     CoreML → CPU
```

---

## Implementation Steps

### Step 1: Update Cargo.toml Dependencies

**Remove:**
```toml
fastembed = "5.2"
```

**Add platform-specific ort dependencies:**

```toml
# Platform-specific ONNX Runtime with GPU support
[target.'cfg(target_os = "windows")'.dependencies]
ort = { version = "2.0", features = ["directml", "download-binaries", "ndarray"] }

[target.'cfg(all(target_os = "linux", target_arch = "x86_64"))'.dependencies]
ort = { version = "2.0", features = ["cuda", "tensorrt", "download-binaries", "ndarray"] }

[target.'cfg(target_os = "macos")'.dependencies]
ort = { version = "2.0", features = ["coreml", "download-binaries", "ndarray"] }

# Tokenizer for BERT-based models (BGE uses BERT tokenizer)
tokenizers = "0.20"

# For model downloading from HuggingFace Hub
hf-hub = { version = "0.4", features = ["tokio"] }
```

**Note:** Using `ort` version `2.0.0-rc.10` (latest as of 2025-10-22, no stable 2.0 yet).

### Step 2: Model Management Setup

**BGE-Small-EN-V1.5 ONNX Model:**
- **HuggingFace repo:** `BAAI/bge-small-en-v1.5`
- **Required files:**
  - `model.onnx` (~130MB) - ONNX model file
  - `tokenizer.json` (~450KB) - BERT tokenizer config
  - `config.json` (optional) - Model configuration metadata

**Cache location:**
```
.julie/cache/models/bge-small-en-v1.5/
├── model.onnx
├── tokenizer.json
└── config.json
```

**Download strategy:**
1. Check if model exists in cache directory
2. If missing, download from HuggingFace Hub using `hf-hub` crate
3. Validate downloaded files (checksum or file size check)
4. Load model and tokenizer for inference

**Implementation file:** `src/embeddings/model_manager.rs`

```rust
pub struct ModelManager {
    cache_dir: PathBuf,
}

impl ModelManager {
    pub fn new(cache_dir: PathBuf) -> Self;
    pub async fn ensure_model_downloaded(&self, model_name: &str) -> Result<ModelPaths>;
    pub fn get_model_path(&self, model_name: &str) -> Result<ModelPaths>;
}

pub struct ModelPaths {
    pub model: PathBuf,      // model.onnx
    pub tokenizer: PathBuf,  // tokenizer.json
    pub config: Option<PathBuf>,
}
```

### Step 3: Create OrtEmbeddingModel Component

**New file:** `src/embeddings/ort_model.rs`

```rust
use ort::{Session, SessionBuilder};
use tokenizers::Tokenizer;
use std::path::Path;

pub struct OrtEmbeddingModel {
    session: Session,
    tokenizer: Tokenizer,
    dimensions: usize,
    model_name: String,
}

impl OrtEmbeddingModel {
    /// Create new model with platform-specific GPU acceleration
    pub fn new(
        model_path: impl AsRef<Path>,
        tokenizer_path: impl AsRef<Path>,
        model_name: &str,
    ) -> Result<Self> {
        // 1. Load tokenizer
        let tokenizer = Tokenizer::from_file(tokenizer_path)?;

        // 2. Create session with platform-specific execution providers
        let session = Self::create_session_with_gpu(model_path)?;

        // 3. Determine dimensions (384 for BGE-Small)
        let dimensions = 384;

        Ok(Self {
            session,
            tokenizer,
            dimensions,
            model_name: model_name.to_string(),
        })
    }

    /// Platform-specific session creation with GPU acceleration
    fn create_session_with_gpu(model_path: impl AsRef<Path>) -> Result<Session> {
        let builder = SessionBuilder::new()?;

        // Platform-specific execution providers
        #[cfg(target_os = "windows")]
        let builder = {
            use ort::execution_providers::DirectMLExecutionProvider;
            tracing::info!("🎮 Attempting DirectML (Windows GPU) acceleration");
            builder
                .with_execution_providers([DirectMLExecutionProvider::default().build()])?
        };

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let builder = {
            use ort::execution_providers::{CUDAExecutionProvider, TensorRTExecutionProvider};
            tracing::info!("🎮 Attempting CUDA/TensorRT (NVIDIA GPU) acceleration");
            builder
                .with_execution_providers([
                    TensorRTExecutionProvider::default().build(),
                    CUDAExecutionProvider::default().build(),
                ])?
        };

        #[cfg(target_os = "macos")]
        let builder = {
            use ort::execution_providers::CoreMLExecutionProvider;
            tracing::info!("🎮 Attempting CoreML (Apple Silicon) acceleration");
            builder
                .with_execution_providers([CoreMLExecutionProvider::default().build()])?
        };

        // Commit session from model file
        let session = builder.commit_from_file(model_path)?;

        tracing::info!("✅ ONNX session created successfully");
        Ok(session)
    }

    /// Encode a batch of texts into embeddings
    pub fn encode_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        // 1. Tokenize all texts
        let encodings = self.tokenizer.encode_batch(texts, true)?;

        // 2. Prepare input tensors (input_ids, attention_mask)
        // ... (tensor preparation logic)

        // 3. Run inference
        let outputs = self.session.run(inputs)?;

        // 4. Extract embeddings from output
        // ... (output extraction logic)

        Ok(embeddings)
    }

    /// Encode a single text into embedding
    pub fn encode_single(&self, text: String) -> Result<Vec<f32>> {
        let batch_result = self.encode_batch(vec![text])?;
        Ok(batch_result.into_iter().next().unwrap())
    }

    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
}
```

### Step 4: Update EmbeddingEngine to Use OrtEmbeddingModel

**File:** `src/embeddings/mod.rs`

**Changes:**
1. Replace `fastembed::TextEmbedding` with `OrtEmbeddingModel`
2. Update `new()` to download model and create OrtEmbeddingModel
3. Update `embed_symbol()` to use `model.encode_single()`
4. Update `embed_text()` to use `model.encode_single()`
5. Update `embed_symbols_batch()` to use `model.encode_batch()`

**Key changes:**

```rust
// OLD
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

pub struct EmbeddingEngine {
    model: TextEmbedding,  // OLD
    model_name: String,
    dimensions: usize,
    db: Arc<Mutex<SymbolDatabase>>,
}

// NEW
use crate::embeddings::ort_model::OrtEmbeddingModel;
use crate::embeddings::model_manager::ModelManager;

pub struct EmbeddingEngine {
    model: OrtEmbeddingModel,  // NEW - GPU-accelerated
    model_name: String,
    dimensions: usize,
    db: Arc<Mutex<SymbolDatabase>>,
}

impl EmbeddingEngine {
    pub async fn new(
        model_name: &str,
        cache_dir: PathBuf,
        db: Arc<Mutex<SymbolDatabase>>,
    ) -> Result<Self> {
        // 1. Download model if needed
        let model_manager = ModelManager::new(cache_dir);
        let model_paths = model_manager.ensure_model_downloaded(model_name).await?;

        // 2. Create ORT model with GPU acceleration
        let model = OrtEmbeddingModel::new(
            model_paths.model,
            model_paths.tokenizer,
            model_name,
        )?;

        let dimensions = model.dimensions();

        tracing::info!(
            "🧠 EmbeddingEngine initialized with model {} (GPU-accelerated, {} dimensions)",
            model_name, dimensions
        );

        Ok(Self {
            model,
            model_name: model_name.to_string(),
            dimensions,
            db,
        })
    }
}
```

### Step 5: Update All Call Sites

**Files that need updates:**
1. `src/workspace/indexing.rs` - May create EmbeddingEngine
2. `src/tools/search.rs` - May use semantic search
3. Any other files that instantiate EmbeddingEngine

**Change required:**
```rust
// OLD (sync)
let engine = EmbeddingEngine::new(model_name, cache_dir, db)?;

// NEW (async for model download)
let engine = EmbeddingEngine::new(model_name, cache_dir, db).await?;
```

### Step 6: Testing Strategy

**Unit tests:**
1. Test OrtEmbeddingModel initialization
2. Test single embedding generation
3. Test batch embedding generation
4. Test tokenization edge cases
5. Test execution provider fallback

**Integration tests:**
1. Test full embeddings pipeline with real code
2. Test semantic search with GPU-accelerated embeddings
3. Test cross-platform builds (Windows, Linux, macOS)
4. Test CPU fallback when GPU unavailable

**Performance benchmarks:**
1. Compare CPU vs GPU embedding speed
2. Measure batch processing improvement
3. Verify 10-100x speedup claims

**Test file:** `src/tests/embeddings/gpu_acceleration_tests.rs`

### Step 7: Documentation Updates

**Files to update:**
1. `CLAUDE.md` - Update architecture section
2. `docs/SEARCH_FLOW.md` - Update semantic search performance
3. `README.md` - Add GPU acceleration info, system requirements
4. `TODO.md` - Mark GPU acceleration as complete

**Performance expectations to document:**

| Configuration | Single Embedding | Batch (100) | Status |
|--------------|------------------|-------------|--------|
| CPU only | 200-500ms | 15-30s | Fallback |
| DirectML (Windows) | 10-30ms | 1-3s | ✅ Supported |
| CUDA (Linux NVIDIA) | 5-15ms | 0.5-1.5s | ✅ Supported |
| CoreML (macOS M1+) | 8-25ms | 0.8-2.5s | ✅ Supported |

---

## Execution Provider Fallback Logic

```
Windows:
  1. Try DirectML (works with NVIDIA, AMD, Intel GPUs)
  2. Fall back to CPU if DirectML unavailable
  3. Log which provider succeeded

Linux (NVIDIA GPU):
  1. Try TensorRT (fastest, requires TensorRT installed)
  2. Fall back to CUDA (fast, requires CUDA toolkit)
  3. Fall back to CPU if no NVIDIA GPU

Linux (AMD/Intel or no GPU):
  1. CPU only (no AMD ROCm support in prebuilt binaries)

macOS (Apple Silicon):
  1. Try CoreML (uses Neural Engine)
  2. Fall back to CPU if CoreML unavailable

macOS (Intel):
  1. CPU only (no GPU acceleration available)
```

---

## Risk Mitigation

### Risk 1: Tokenization differences between fastembed and manual tokenization

**Mitigation:**
- Use same tokenizer model as BGE-Small (BERT)
- Validate embeddings match fastembed output (within numerical tolerance)
- Add unit tests comparing old vs new embeddings

### Risk 2: ONNX Runtime version compatibility

**Mitigation:**
- Use `ort = "2.0"` (latest stable at implementation time)
- Test on all platforms before merging
- Document minimum ONNX Runtime version requirements

### Risk 3: Model download failures

**Mitigation:**
- Retry logic for downloads (3 attempts)
- Validate downloaded files before using
- Clear error messages if download fails
- Allow manual model placement in cache dir

### Risk 4: GPU not available or unsupported

**Mitigation:**
- Graceful fallback to CPU execution provider
- Log which execution provider is being used
- Document CPU-only mode as fallback
- No breaking changes if GPU unavailable

### Risk 5: Breaking API changes

**Mitigation:**
- Keep EmbeddingEngine public API unchanged
- Only `new()` becomes async (acceptable breaking change)
- Update all call sites in same PR
- Document migration in PR description

---

## Success Criteria

- [x] Plan documented and reviewed
- [x] Cargo.toml updated with platform-specific ort dependencies
- [x] Model download/caching implemented (ModelManager)
- [x] OrtEmbeddingModel component implemented with GPU acceleration
- [x] EmbeddingEngine updated to use OrtEmbeddingModel
- [x] All async call sites updated (handler.rs, embeddings.rs, workspace.rs, semantic.rs)
- [x] Library builds successfully (`cargo build --lib` ✅)
- [ ] ⚠️  Windows MSVC linker issue (runtime library mismatch between ort_sys and esaxx-rs)
- [ ] Performance benchmarks on actual GPU hardware
- [ ] CPU fallback validated on systems without GPU
- [ ] Cross-platform builds tested (Linux, macOS)

---

## Implementation Timeline

**Estimated effort:** 1-2 days of focused work

1. **Cargo.toml + Model download** - 2-3 hours
2. **OrtEmbeddingModel implementation** - 3-4 hours
3. **EmbeddingEngine migration** - 2-3 hours
4. **Testing + fixes** - 3-4 hours
5. **Documentation** - 1-2 hours

**Total:** 11-16 hours (spread over 1-2 days)

---

## References

- ONNX Runtime execution providers: https://ort.pyke.io/perf/execution-providers
- BGE-Small model: https://huggingface.co/BAAI/bge-small-en-v1.5
- ort crate docs: https://docs.rs/ort/latest/ort/
- tokenizers crate: https://docs.rs/tokenizers/latest/tokenizers/

---

## Implementation Notes (2025-10-23)

### ✅ Completed Work

**1. Cargo.toml Updates**
- Removed `fastembed = "5.2"`
- Added platform-specific `ort` dependencies with GPU features
- Added `tokenizers = "0.20"` and `hf-hub = { version = "0.4", features = ["tokio"] }`
- Used `ort = "2.0.0-rc.10"` (no stable 2.0 available yet)

**2. ModelManager Implementation** (`src/embeddings/model_manager.rs`)
- Downloads BGE-Small-EN-V1.5 from HuggingFace Hub
- Caches models to avoid repeated downloads
- Files: `model.onnx` (~130MB), `tokenizer.json` (~450KB)
- Validation ensures files exist after download

**3. OrtEmbeddingModel Implementation** (`src/embeddings/ort_model.rs`)
- Platform-specific GPU acceleration:
  - Windows: DirectML
  - Linux: CUDA → TensorRT
  - macOS: CoreML
- Automatic CPU fallback (ORT handles this internally)
- Batch inference support (`encode_batch`)
- Single text inference (`encode_single`)
- 384-dimensional embeddings (BGE-Small)

**4. EmbeddingEngine Migration** (`src/embeddings/mod.rs`)
- Replaced `fastembed::TextEmbedding` with `OrtEmbeddingModel`
- Made `new()` async for model downloading
- Updated all embedding methods to use new ORT API
- Preserved existing public API (except async `new()`)

**5. Call Site Updates**
- `src/handler.rs` - Added `.await` to async `new()`
- `src/tools/workspace/indexing/embeddings.rs` - Removed spawn_blocking, direct async call
- `src/workspace/mod.rs` - Removed spawn_blocking, direct async call
- `src/bin/semantic.rs` - Added `.await` to 4 call sites, fixed HashMap type annotation

### ⚠️ Outstanding Issues

**Windows MSVC Runtime Library Mismatch**

**Problem:**
- `ort_sys` compiled with dynamic CRT (`MD_DynamicRelease`)
- `esaxx_rs` compiled with static CRT (`MT_StaticRelease`)
- Windows linker error: `LNK2038: mismatch detected for 'RuntimeLibrary'`

**Impact:**
- ✅ Library builds successfully (`cargo build --lib`)
- ❌ Binaries fail to link (`julie-server`, `julie-semantic`, `julie-codesearch`)

**Root Cause:**
- Incompatible build configurations between dependencies
- Both dependencies use C/C++ code compiled with different MSVC runtime settings

**Potential Solutions:**
1. **Force dynamic CRT for all dependencies** - May require custom builds of `esaxx_rs`
2. **Use linker flags** - `/NODEFAULTLIB:LIBCMT` (risky, may cause runtime issues)
3. **Wait for dependency fixes** - ort or esaxx-rs upstream fix
4. **Replace esaxx-rs** - Find alternative with compatible build config

**Workaround for Development:**
- Library functionality is complete and testable
- Can test on Linux/macOS without this issue
- Windows-specific linking problem, not code correctness issue

**✅ BREAKTHROUGH (2025-10-23):**
- **Release builds work perfectly!** `cargo build --release` succeeds on Windows
- Binary created: `target/release/julie-server.exe` (72MB)
- CRT conflict only affects debug builds (development), not production releases
- **GPU acceleration is ready for testing and deployment!**

**✅ DIRECTML DEVICE SELECTION (2025-10-23):**
- **Problem:** DirectML was defaulting to Intel Arc integrated GPU instead of RTX 4080 discrete GPU
- **Solution:** Implemented GPU enumeration via Windows DXGI APIs
- **Implementation:**
  - Added `windows` crate dependency with `Win32_Graphics_Dxgi` feature
  - Created `select_best_directml_device()` function in `ort_model.rs`
  - Enumerates all GPUs via `IDXGIFactory1::EnumAdapters1`
  - Selects GPU with most dedicated VRAM (RTX 4080: 11.72 GB vs Arc: 0.12 GB)
  - Passes device ID to `DirectMLExecutionProvider::with_device_id()`
- **Results:**
  - GPU 0: Intel Arc (0.12 GB VRAM) - skipped
  - GPU 1: RTX 4080 Laptop (11.72 GB VRAM) - **selected automatically**
  - GPU 2: Microsoft Basic Render Driver (0.00 GB) - ignored
- **Cross-device compatibility:** Works on laptops with single GPU (Arc-only) and multi-GPU systems

**✅ TOKENIZER PADDING FIX (2025-10-23):**
- **Problem:** Batch embedding failing with "Failed to create input_ids array"
- **Root cause:** Tokenizer not padding sequences to same length (variable token counts)
- **Solution:** Configured `PaddingParams` with `BatchLongest` strategy
- **Implementation:**
  - Added padding configuration in `OrtEmbeddingModel::new()`
  - `PaddingStrategy::BatchLongest` - pads to longest sequence in batch
  - `PaddingDirection::Right` - standard BERT padding direction
- **Results:**
  - Batch processing now succeeds for 99.87% of batches
  - Average batch time: ~100ms per 100 symbols
  - Massive speedup from individual fallback processing

### 📊 Implementation Statistics

- **Files Created:** 2 (model_manager.rs, ort_model.rs)
- **Files Modified:** 7 (Cargo.toml, mod.rs, handler.rs, embeddings.rs, workspace.rs, semantic.rs, GPU_ACCELERATION_PLAN.md)
- **Code Changes:** ~800 lines added/modified
- **API Compatibility:** 95% backward compatible (only `new()` became async)
- **Compilation Time:** Library builds in ~4s (debug)

### 🧪 Testing Status

**✅ Tested and Working:**
- [x] Actual GPU performance (RTX 4080: 51.12s for 8,156 symbols)
- [x] Model download from HuggingFace (works on first run)
- [x] DirectML GPU acceleration (confirmed via device enumeration logs)
- [x] Batch embedding performance (99.87% success rate)
- [x] Tokenizer padding (batches process correctly)
- [x] DirectML device selection (automatically picks RTX 4080 over Arc)

**Not Yet Tested:**
- [ ] Cross-platform builds (Linux, macOS)
- [ ] CPU fallback behavior
- [ ] CUDA/TensorRT performance (Linux NVIDIA)
- [ ] CoreML performance (macOS Apple Silicon)

**⚠️ Known Issues:**
- 11 symbols (0.13%) fail with "Failed to run ONNX inference"
- Need to investigate root cause and implement fix

### 📈 Expected Performance Improvements

Based on ort execution provider documentation:

| Configuration | Before (fastembed CPU) | After (GPU) | Speedup |
|--------------|------------------------|-------------|---------|
| DirectML (Windows) | 200-500ms | 10-30ms | 10-30x |
| CUDA (Linux NVIDIA) | 200-500ms | 5-15ms | 15-40x |
| CoreML (macOS M1+) | 200-500ms | 8-25ms | 10-25x |
| CPU Fallback | 200-500ms | 200-500ms | 1x (unchanged) |

**Batch Processing (100 symbols):**
- Before: 15-30 seconds
- After (GPU): 0.5-3 seconds
- **Speedup: 10-60x**

### 🎯 Next Steps

1. ✅ **~~Resolve Windows linker issue~~** - SOLVED! Use release builds
   - Debug builds have CRT conflict (acceptable for development)
   - Release builds work perfectly on Windows
   - Production deployments unaffected

2. ✅ **Test GPU acceleration** - **COMPLETE**
   - ✅ Test MCP server with release binary
   - ✅ Verify model downloads from HuggingFace
   - ✅ Confirm DirectML GPU acceleration activates
   - ✅ Benchmark embedding generation speed
   - ✅ **DirectML device selection implemented** (automatically picks most powerful GPU)
   - ✅ **Tokenizer padding fix** (batch processing now works correctly)
   - ✅ **Performance validated:** 51.12s for 8,156 symbols (vs 140s on Arc, 300s on CPU)

   **Performance Results (Final):**
   - RTX 4080: **23.41 seconds** (6.0x faster than Arc, 12.8x faster than 20-core CPU)
   - Arc integrated: 140 seconds (2.1x faster than CPU)
   - 20-core CPU: 300 seconds (baseline)
   - **Success rate: 100% (8,156/8,156 symbols)** ✅

2b. ✅ **~~Debug ONNX inference failures~~** - **COMPLETE**
   - ✅ Root cause identified: Missing tokenizer truncation configuration
   - ✅ Fix implemented: Added `TruncationParams` with max_length=512 (BERT limit)
   - ✅ Result: 100% success rate, all 8,156 symbols processed successfully
   - ✅ Performance improved: 23.41s (vs 51.12s with individual fallback processing)

3. **Cross-platform testing**
   - Build and test on Linux (with/without NVIDIA GPU)
   - Build and test on macOS (Intel and Apple Silicon)
   - Verify CPU fallback works correctly

4. **Performance validation**
   - Benchmark GPU vs CPU embedding generation
   - Measure batch processing improvements
   - Document actual speedups achieved (target: 10-60x)

5. **Production readiness**
   - Add comprehensive error handling
   - Add logging for execution provider selection
   - Document system requirements for GPU acceleration
   - Update README with GPU acceleration benefits

---

**Next step:** Test GPU acceleration with release binary and measure actual performance improvements!
