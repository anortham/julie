# Semantic Embedding Providers: Operations & Architecture Guide

This document is the authoritative operations guide for semantic embedding runtimes in Julie. It covers provider discovery, executable resolution, explicit model preparation, execution mode semantics (`Off`, `Auto`, `Required`), model switching, generation lifecycles, and failure recovery.

---

## 1. Architectural Overview & Provider Taxonomy

Julie provides LSP-quality code intelligence across 36 programming languages using a hybrid retrieval architecture:
1. **Lexical Retrieval**: Tantivy full-text search with customized stemming, code tokenizers, and field-weighted BM25 scoring.
2. **Relational Retrieval**: SQLite structured storage tracking symbols, identifiers, scopes, definitions, calls, and hierarchical relations.
3. **Semantic Retrieval**: High-dimensional vector embeddings and approximate nearest neighbors (ANN/KNN) for natural language conceptual search, semantic similarity, and symbol associations.

### Native Provider Architecture

Julie has one embedding provider runtime behind the `EmbeddingProvider` trait:

```text
               +--------------------------------------+
               |         RequestEngine / CLI          |
               +--------------------------------------+
                                  |
                   (JULIE_EMBEDDING_PROVIDER=auto|native)
                                  |
                                  v
                    +---------------------------+
                    |  NativeEmbeddingProvider  |
                    |   (Rust / llama.cpp GGUF) |
                    +---------------------------+
                                  |
                          stdin / stdout (NDJSON)
                                  |
                                  v
                    +---------------------------+
                    |  julie-semantic-sidecar   |
                    |       (one service child) |
                    +---------------------------+
```

The machine service owns one native sidecar child. Requests share that child through the semantic runtime; it speaks the versioned NDJSON protocol over stdio.

### Provider Facts

| Operational Dimension | Native Provider (`native`) |
|-----------------------|----------------------------|
| **Implementation** | Standalone native Rust binary (`llama.cpp`) |
| **Default Model** | `bge-small-en-v1.5-f32` (384 dims) |
| **Alternative Model** | `qwen3-0.6b-f16` (512 dims via MRL) |
| **Model Weights** | Local GGUF format (`~/.cache/julie-semantic`) |
| **Memory Footprint** | ~150 MiB (BGE) / ~1,200 MiB (Qwen) |
| **Startup / Warming** | < 350 ms (instant GGUF mmap load) |
| **Transport** | Stdio NDJSON |
| **Sharing** | One service-owned child |
| **Quantization** | Native GGUF FP32 (BGE) / FP16 (Qwen) |
| **Hardware Acceleration** | Metal (Apple), Vulkan, CUDA, CPU |

---

## 2. Provider Discovery & Configuration

Selection and discovery are managed through environment variables evaluated during workspace initialization and semantic runtime startup.

### Environment Variable Reference

| Variable | Values | Default | Purpose |
|----------|--------|---------|---------|
| `JULIE_EMBEDDING_PROVIDER` | `auto`, `native`, `none` (`off` and `disabled` are aliases of `none`) | `auto` | Primary backend selector. `auto` uses the native sidecar when its binary is found, else `none`. `native` requires the sidecar. Tests get `none` from `.cargo/config.toml`. |
| `JULIE_NATIVE_SIDECAR_PROGRAM` | Absolute or relative filesystem path | Auto-discovered | Explicit path override to the `julie-semantic-sidecar` binary. |
| `JULIE_NATIVE_SIDECAR_MODEL` | `bge-small-en-v1.5-f32`, `qwen3-0.6b-f16` | `bge-small-en-v1.5-f32` | Manifest model ID for the native provider. |
| `JULIE_EMBEDDING_CACHE_DIR` | Absolute filesystem directory | `~/.cache/julie-semantic` | Shared root for GGUF model files. |
| `JULIE_SIDECAR_FORCE_BACKEND` | `cpu`, `metal`, `vulkan`, `cuda` | Auto-benchmarked | Bypasses sidecar hardware benchmarking. `cpu` skips device discovery and runs strictly on CPU. |
| `JULIE_EMBEDDING_STRICT_ACCEL` | `1`, `true`, `on` | Unset (`false`) | When enabled, disables semantic embeddings if hardware acceleration is unavailable or degraded. |

### Discovery Resolution Order

The initialization pipeline processes embedding configuration in two stages:
1. `parse_provider_preference`:
   - `none`, `off`, `disabled`: Completely disables embeddings without spawning a child or preparing model files.
   - `auto`: Resolves to `NativeEmbeddingProvider` when the sidecar binary is found, else `none`.
   - `native`: Resolves to `NativeEmbeddingProvider`.
   - `sidecar` and `ort` fail with "has been removed"; any other string fails with an explicit error.
2. `resolve_backend_preference`:
   - Verifies platform capabilities and compilation feature flags before returning the resolved backend.

---

## 3. Executable Path Resolution

When `JULIE_EMBEDDING_PROVIDER=native` is selected, `NativeEmbeddingProvider` resolves the sidecar binary using a deterministic, multi-tiered discovery algorithm (`find_and_hash_sidecar_binary` in `crates/julie-pipeline/src/embeddings/native/launch.rs`):

```text
                               +------------------------------------+
                               |  Explicit program in config?       |
                               +------------------------------------+
                                           | Yes            | No
                                           v                v
                                    [Use Explicit]   +------------------------------------+
                                                     |  JULIE_NATIVE_SIDECAR_PROGRAM set? |
                                                     +------------------------------------+
                                                                 | Yes            | No
                                                                 v                v
                                                          [Use Env Var]    +------------------------------------+
                                                                           | Colocated beside julie-server?     |
                                                                           +------------------------------------+
                                                                                       | Yes            | No
                                                                                       v                v
                                                                                [Use Colocated]  +------------------------------------+
                                                                                                 | Sibling dev repo target/?          |
                                                                                                 +------------------------------------+
                                                                                                             | Yes            | No
                                                                                                             v                v
                                                                                                      [Use Dev Build]  +------------------------------------+
                                                                                                                       | User cache: ~/.cache/.../bin       |
                                                                                                                       +------------------------------------+
                                                                                                                                   | Yes            | No
                                                                                                                                   v                v
                                                                                                                            [Use Cache Bin]  +------------------------------------+
                                                                                                                                             | System PATH search                 |
                                                                                                                                             +------------------------------------+
                                                                                                                                                         | Yes            | No
                                                                                                                                                         v                v
                                                                                                                                                  [Use PATH Exe]   [NATIVE_SIDECAR_MISSING]
```

### Discovery Tiers in Detail

1. **Explicit Programmatic Configuration**: Specified in `EmbeddingConfig.native_program`.
2. **Environment Variable Override**: `JULIE_NATIVE_SIDECAR_PROGRAM`. Must point directly to an executable file.
3. **Colocated Binary**: `std::env::current_exe().parent().join("julie-semantic-sidecar")` (`.exe` on Windows). Standard for production tarball installations.
4. **Development Workspace Targets**: Inspects sibling compilation targets:
   - `/home/murphy/source/julie-semantic-sidecar/target/release/julie-semantic-sidecar`
   - `/home/murphy/source/julie-semantic-sidecar/target/debug/julie-semantic-sidecar`
5. **User Cache Binary Directory**:
   - Linux/macOS: `~/.cache/julie-semantic/bin/julie-semantic-sidecar`
   - Windows: `%LOCALAPPDATA%\julie-semantic\bin\julie-semantic-sidecar.exe`
6. **System `PATH` Lookup**: Searches all directories listed in the host `$PATH`.
7. **Failure Mode**: If all locations fail, Julie aborts with:
   ```text
   NATIVE_SIDECAR_MISSING: unable to locate julie-semantic-sidecar binary in PATH, cache, or target
   ```

### Executable SHA-256 Fingerprinting

Upon locating the binary, Julie reads the binary file and computes its SHA-256 digest:
- The hash identifies the executable reported by service status and qualification records.
- A changed executable hash is evidence that a service restart is required before clients use the new binary.
- The full 64-character hash is recorded on `NativeLaunchConfig.executable_sha256`, validated in qualification records (`NativeQualificationRecord.executable_sha256`), and verified against live running binaries. Workspace qualification explicitly reconciles the qualification record against the live running binary and provider (reported executable SHA-256, encoder identity, backend, and device). `EncoderIdentity.runtime_build` separately records the engine build identifier (e.g. `llama.cpp-b3560`).

---

## 4. Model Inventory & Explicit Model Preparation

Julie archives contain the native sidecar binary and its libraries, never model weights. With native semantics enabled, Julie starts one shared background `prepare` when the default model is absent; lexical requests continue without waiting. `--semantics off` and `JULIE_EMBEDDING_PROVIDER=none` never prepare a model.

The plugin launcher requires Node.js 22.5 or newer for `process.getBuiltinModule`; a native archive or direct binary invocation does not require Node.

### Pinned Model Manifest

The sidecar embeds a cryptographically pinned model manifest (`src/manifest.rs`):

| Attribute | `bge-small-en-v1.5-f32` (Default) | `qwen3-0.6b-f16` (Fallback / Large) |
|---|---|---|
| **Upstream Architecture** | BAAI BGE Small English v1.5 | Qwen3 Embedding 0.6B |
| **GGUF File** | `bge-small-en-v1.5-f32.gguf` | `Qwen3-Embedding-0.6B-f16.gguf` |
| **Expected SHA-256** | `bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65` | `421a27e58d165478cc7acb984a688c2aa41404968b0203e7cd743ece44c54340` |
| **File Size** | 133,609,568 bytes (~133.6 MB) | 1,197,629,632 bytes (~1.2 GB) |
| **Native Dimensions** | 384 | 1024 |
| **Served Dimensions** | **384** | **512** (via Matryoshka Representation Learning) |
| **MRL Support** | Single lane `[384]` | Matryoshka lanes `[256, 512, 1024]` |
| **Pooling Mode** | `cls` (Leading token) | `last` (Trailing token) |
| **EOS Token Marker** | None | `<|endoftext|>` |
| **Max Context Length** | 512 tokens | 32,768 tokens |
| **Instruction Policy** | `v1` | `v1` |
| **Query Prompt** | `"Represent this sentence for searching relevant passages: "` | `"Instruct: Given a code search query, retrieve the code or documentation that answers it\nQuery: "` |
| **Document Prompt** | `""` (no prefix) | `""` (no prefix) |
| **Upstream Source** | CompendiumLabs / HuggingFace | Qwen / HuggingFace |

### Explicit Preparation Commands

To prepare a model, invoke the sidecar with the `prepare` verb:

```bash
# Prepare the default model (bge-small-en-v1.5-f32):
julie-semantic-sidecar prepare

# Explicitly prepare the default BGE model:
julie-semantic-sidecar prepare --model bge-small-en-v1.5-f32

# Explicitly prepare the Qwen3 comparison model:
julie-semantic-sidecar prepare --model qwen3-0.6b-f16
```

The effective cache root is `JULIE_EMBEDDING_CACHE_DIR` when set; otherwise it is `~/.cache/julie-semantic` on macOS/Linux and `%LOCALAPPDATA%/julie-semantic` on Windows. The default file is `bge-small-en-v1.5-f32.gguf` beneath that root. When offline preparation fails, the recovery command is `julie-semantic-sidecar prepare --model bge-small-en-v1.5-f32` after network access is restored.

### Preparation Safety & Integrity

During execution of `prepare`, the sidecar enforces strict safety guards:
1. **Disk Preflight**: Inspects target partition free space against `size_bytes` before initiating network requests.
2. **Advisory File Lock**: Acquires an exclusive file lock (`<cache_dir>/<model_id>.lock`) to serialize concurrent `prepare` invocations across multiple agent processes.
3. **Atomic Streaming Download**: Streams chunks into `.julie-prepare-<model_id>.<random>.partial`, calculating SHA-256 hashes incrementally.
4. **Digest Verification**: Compares the calculated digest against the pinned manifest SHA-256. If a mismatch occurs, the partial file is removed and exit code `1` is returned.
5. **Atomic Promotion**: Atomically renames the verified partial file to its permanent name (`<file>.gguf`).
6. **Structured NDJSON Events**: Progress is emitted to `stdout` in real-time:
   ```json
   {"event":"waiting","model_id":"bge-small-en-v1.5-f32"}
   {"event":"progress","model_id":"bge-small-en-v1.5-f32","received_bytes":67108864,"total_bytes":133609568}
   {"event":"done","model_id":"bge-small-en-v1.5-f32","path":"/home/murphy/.cache/julie-semantic/bge-small-en-v1.5-f32.gguf","sha256":"bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65"}
   ```

---

## 5. Execution Mode Semantics

Julie exposes three execution modes controlling semantic capabilities across CLI subcommands and MCP protocol endpoints:

```bash
julie-server fast-search "authentication handler" --semantics auto      # Default
julie-server fast-search "authentication handler" --semantics required  # Strict fail-closed
julie-server fast-search "authentication handler" --semantics off       # Pure lexical
```

### Mode Comparison Matrix

| Mode | Provider Requirement | Missing / Degraded Sidecar | Missing / Incompatible Vectors | Tool Execution Result | Exit Code |
|---|---|---|---|---|---|
| **`Off`** | `None` (`semantic_mode_needs_provider` = `false`) | Completely ignored | Completely ignored | Pure lexical & relational search | `0` |
| **`Auto`** | `Optional` (`semantic_mode_needs_provider` = `true`) | Degrades to lexical | Degrades to lexical | Lexical search with truthful degradation warning | `0` |
| **`Required`** | `Mandatory` (`semantic_mode_needs_provider` = `true`) | **Aborts execution** | **Aborts execution** | Fails closed with `SEMANTICS_NOT_READY` | `4` |

### Detailed Mode Contracts

#### 1. `Off` Mode: Zero-Overhead Lexical Isolation
- Enforces strict isolation: zero sidecar processes spawned, zero UDS/named pipe connections opened, zero model files touched, zero SQLite vector queries executed.
- Used in low-latency CI loops, code editing verification, or environments without model weights.
- Emits `SemanticReadiness::Disabled`.

#### 2. `Auto` Mode: Truthful Resilience
- Attempts full hybrid retrieval (BM25 lexical score + KNN cosine vector similarity).
- If the sidecar is starting, preparing, unreachable, or SQLite vectors are stale or building, the request does not fail.
- Instead, it falls back to Tantivy lexical search while truthfully reporting the degradation in the response envelope:
  ```json
  {
    "ok": true,
    "readiness": {
      "mode": "auto",
      "status": "degraded: GENERATION_BUILDING",
      "coverage": "missing"
    },
    "results": [...]
  }
  ```

#### 3. `Required` Mode: Strict Fail-Closed Integrity
- Designed for semantic testing, benchmark evaluation, and strict query integrity.
- Prohibits returning lexical results disguised as semantic answers.
- Checks a 6-point verification gate in `src/request_engine/semantic_store.rs`:
  1. Embedding provider process is reachable and healthy.
  2. Database tables `symbol_vectors`, `embedding_config`, and `embedding_generations` exist.
  3. Stored model encoder key and dimensions in `embedding_config` match the active provider's cryptographic `storage_key` and output dimensions.
  4. Format version equals `CURRENT_EMBEDDING_FORMAT_VERSION` (v3).
  5. Vector embeddings exist for all eligible symbols.
  6. The active embedding generation is fully published with `status = 'ready'`, has source revision equal to or newer than the canonical symbol revision, and complete coverage (`embedded_symbols >= eligible_symbols`).
- If any check fails, immediately terminates with error code `SEMANTICS_NOT_READY` (CLI exit code 4):
  ```json
  {
    "ok": false,
    "error": {
      "code": "SEMANTICS_NOT_READY",
      "message": "Stored vectors are incompatible: stored 3e4b... (768d) vs expected a1b2... (384d)",
      "data": {
        "coverage": "incompatible",
        "stored_model": "3e4b...",
        "stored_dimensions": 768,
        "expected_encoder_key": "a1b2...",
        "provider_model": "bge-small-en-v1.5-f32",
        "provider_dimensions": 384
      }
    }
  }
  ```

---

## 6. Model Switching & Embedding Generation Lifecycle

Switching embedding models—whether changing dimensions (e.g. 384d BGE → 512d Qwen) or switching between two distinct models sharing identical dimensions—requires complete, atomic vector invalidation. Comparing vector distances across different embedding spaces produces mathematically meaningless noise.

### Cryptographic Identity: `EncoderIdentity`

Julie binds every vector collection to an immutable cryptographic identity (`crates/julie-core/src/embeddings_identity.rs`):

```rust
pub struct EncoderIdentity {
    pub schema: u32,                  // Identity schema version (1)
    pub model_id: String,             // Manifest model identifier
    pub weights_sha256: String,       // SHA-256 of model weights file
    pub dimensions: usize,            // Output vector dimensionality
    pub pooling: String,              // Pooling strategy ("cls" or "last")
    pub normalization: String,        // Normalization ("l2")
    pub instruction_policy: String,   // Prompt formatting version ("v1")
    pub text_format: u32,             // Chunking/encoding format
    pub runtime_build: String,        // Engine build identity
}
```

The canonical storage key is computed via:
```rust
// Serialized to canonical JSON bytes with lowercased hex digests and normalization:
let canonical_bytes = serde_json::to_vec(&CanonicalPayload {
    schema,
    model_id,
    weights_sha256,
    dimensions,
    pooling,
    normalization,
    instruction_policy,
    text_format,
    runtime_build,
})?;
storage_key = format!("{:064x}", Sha256::digest(&canonical_bytes));
```
Any change to prompt templates, pooling, weights, or dimensions produces a totally distinct `storage_key`, preventing cross-model corruption.

### SQLite Generation Tracking: Migration 32

Vector compatibility is tracked via table `embedding_generations` in SQLite:

```sql
CREATE TABLE IF NOT EXISTS embedding_generations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    encoder_key TEXT NOT NULL,
    source_revision INTEGER NOT NULL,
    dimensions INTEGER NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('building', 'ready', 'failed', 'stale', 'superseded')),
    eligible_symbols INTEGER NOT NULL DEFAULT 0,
    embedded_symbols INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_embedding_generations_lookup
ON embedding_generations(encoder_key, source_revision, status);
CREATE INDEX IF NOT EXISTS idx_embedding_generations_status
ON embedding_generations(status);
```

### Generation Lifecycle Workflow

```text
[Model Switch or Index Rescan]
              |
              v
  begin_embedding_generation()
              |
              +---> Atomically inserts generation row with status = 'building'
              +---> Marks prior building or ready generations as 'superseded'
              +---> Semantic readers immediately fail-closed (or degrade in Auto)
              |
              v
   [Batch Embedding Stream]
              |
              +---> Atomically writes vectors into symbol_vectors scoped to active building generation
              +---> Rejects writes from superseded or cancelled generations
              |
              v
 publish_embedding_generation()
              |
              +---> Verifies status == 'building' and source revision matches
              +---> Verifies canonical revision matches source revision
              +---> Sets embedded_symbols and eligible_symbols counts
              +---> Marks status = 'ready'
              |
              v
[Semantic Readers Activated]
```

> **Note on Vector Schema**: `symbol_vectors` is a `vec0` virtual table `(symbol_id INTEGER PRIMARY KEY, embedding float[{}])` scoped to the active building generation; it does not contain a `generation_id` column. Generations, revisions, and status tracking are maintained in `embedding_generations` (Migration 32).

### Fail-Closed Rebuild Guarantees

1. **Source Fact Preservation**: Replacing vector embeddings never touches AST symbols, definitions, references, or syntax relations in `symbols.db`.
2. **Zero Mixed-Generation Reads**: Readers query vectors belonging only to the published generation.
3. **No Lock Contention**: Inference runs completely outside SQLite write transactions. Transactions are held only for short batch writes.

---

## 7. Service Child, Concurrency, and Failure Recovery

### Shared Sidecar Child

Julie owns one `julie-semantic-sidecar serve` child for the machine service. It is not a socket service and does not expose a broker endpoint. The runtime serializes child protocol access, verifies the encoder identity at startup, and restarts a failed child on the next bounded request.

### Readiness and Recovery

`DefaultSemanticRuntime` keeps a single in-flight initialization/prepare task. Empty caches begin preparation in the background; Auto reports degradation while lexical work proceeds, and Required waits only for its request deadline. A failed offline preparation retains its error plus the model, cache path, and manual recovery command.

### Dynamic Recovery

- `DefaultSemanticRuntime` tracks provider states dynamically: `Disabled`, `Starting`, `Ready`, and `Degraded { reason, retryable }`.
- When a request arrives while degraded, if `retryable == true`, Julie attempts single-flight re-acquisition.
- Once the model is prepared and the child passes its health handshake, the runtime transitions to `Ready` without client restart.

### Bounded Child Requests

Every child request is constrained by `EmbeddingRequestBudget`. A timeout, malformed reply, or child exit discards the child; a later request may construct a replacement only when its deadline permits. Replacement identity must match the prior provider before vectors are used.

---

## 8. Qualification & Telemetry

Julie provides built-in tools for qualification and operational inspection.

### Machine-Readable Qualification Records

To qualify a native semantic installation for production or benchmarking, run the qualification validator (`src/request_engine/semantic_qualification.rs`):

```json
{
  "schema": "julie-native-qualification-v1",
  "julie_source_sha": "0432158c88e9d0d696f408f0c6e8fd1657513b0a",
  "sidecar_source_sha": "9ed082ba511aa8b10c9e7b47110c3a4dd1e98d59",
  "executable_sha256": "3d7267551e975884c11c964814b019618b959b894ab19437ff641eca648ac51d",
  "encoder_identity": {
    "schema": 1,
    "model_id": "bge-small-en-v1.5-f32",
    "weights_sha256": "bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65",
    "dimensions": 384,
    "pooling": "cls",
    "normalization": "l2",
    "instruction_policy": "v1",
    "text_format": 1,
    "runtime_build": "llama.cpp-b3560"
  },
  "device": "cpu",
  "resolved_backend": "cpu",
  "accelerated": false,
  "corpus_commit": "HEAD",
  "canonical_revision": 142,
  "vector_revision": 142,
  "eligible_symbols": 8420,
  "embedded_symbols": 8420,
  "qualified": true,
  "qualified_at": "2026-09-09T02:00:00Z"
}
```

### Inspecting Runtime Health

Use `manage-workspace` to verify semantic status:

```bash
# Check detailed workspace health:
julie-server workspace --operation health

# Check every checkout's vector and symbol counts:
julie-server workspace --operation status
```

Example health output:
```text
Runtime Plane
Runtime Plane Level: OK
Embedding Status: INITIALIZED
Provider: native (julie-semantic-sidecar 0.1.0)
Model: bge-small-en-v1.5-f32 (384 dims)
Backend: native
Device: cpu
Accelerated: false
Degraded: none
Query Fallback: false

Native Qualification
Qualified: true
Schema: julie-native-qualification-v1
Model ID: bge-small-en-v1.5-f32
Sidecar Executable SHA-256: 3d7267551e975884c11c964814b019618b959b894ab19437ff641eca648ac51d
Revisions: canonical 142 / vector 142 (equal)
Vector Coverage: 8420 / 8420 symbols (100%)
```

---

## 9. Troubleshooting & Operational Playbook

### Troubleshooting Matrix

| Symptom / Error | Root Cause | Remediation Step |
|---|---|---|
| `NATIVE_SIDECAR_MISSING` | Binary not found in PATH, cache, or target directories | 1. Ensure sidecar is compiled: `cargo build --release` in sidecar repo.<br>2. Set `export JULIE_NATIVE_SIDECAR_PROGRAM=/path/to/julie-semantic-sidecar`. |
| `MODEL_NOT_PREPARED` | GGUF model weights missing from cache | Run `julie-semantic-sidecar prepare --model <model_id>`. |
| `SEMANTICS_NOT_READY` (`coverage: missing`) | Workspace has symbols but no vector embeddings generated | Run indexing or `manage_workspace(operation="refresh")` to trigger embedding generation. |
| `SEMANTICS_NOT_READY` (`coverage: incompatible`) | Model switch occurred; stored vectors belong to a different model | Rescan workspace to rebuild vectors for the newly selected model. |
| `SEMANTICS_NOT_READY` (`coverage: building`) | Embedding generation is actively computing vectors in background | Wait for indexing to complete or run with `--semantics auto` for lexical fallback. |
| `Vulkan / GPU initialization failed` | Incompatible GPU driver or shader compilation issue | Set `export JULIE_SIDECAR_FORCE_BACKEND=cpu` to force CPU execution. |

### Common Operational Workflows

#### 1. Verifying Native Semantic Search End-to-End
```bash
# 1. Prepare model
julie-semantic-sidecar prepare --model bge-small-en-v1.5-f32

# 2. Run query in strict required mode
JULIE_EMBEDDING_PROVIDER=native \
julie-server fast-search "repository lifecycle lock" --semantics required --json
```

#### 2. Switching Native Models
```bash
# 1. Update environment
export JULIE_EMBEDDING_PROVIDER=native
export JULIE_NATIVE_SIDECAR_MODEL=qwen3-0.6b-f16

# 2. Prepare native weights
julie-semantic-sidecar prepare --model qwen3-0.6b-f16

# 3. Refresh to build the generation for the new model
julie-server workspace --operation refresh
```

#### 3. Forcing CPU Execution for Predictable CI/Benchmarks
```bash
export JULIE_EMBEDDING_PROVIDER=native
export JULIE_SIDECAR_FORCE_BACKEND=cpu
julie-server fast-search "symbol indexing" --semantics required
```

---
