# Python Sidecar Embeddings Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make a managed Python/PyTorch sidecar the default embedding runtime in Julie while preserving 384-d vector compatibility and automatic fallback to in-process providers.

**Architecture:** Add a new `sidecar` backend to Julie's embedding factory, implemented by a Rust provider that supervises a local Python process and communicates over framed stdio IPC. Keep the existing `EmbeddingProvider` trait unchanged so indexing and query flows remain stable. On sidecar failure, degrade to existing providers (`ort`, `candle`) and keep keyword search unaffected.

**Tech Stack:** Rust (`tokio`, `serde`, `anyhow`, `tracing`, MessagePack framing), Python (`venv`, `pytest`, `torch`, `sentence-transformers`), existing SQLite + sqlite-vec + Tantivy stack

---

### Task 1: Add `sidecar` backend to embedding resolver (default in `auto`)

**Files:**
- Modify: `src/embeddings/mod.rs`
- Modify: `src/embeddings/factory.rs`
- Test: `src/tests/core/embedding_provider.rs`

**Step 1: Write the failing tests**

Add tests in `src/tests/core/embedding_provider.rs`:

```rust
#[test]
fn test_parse_provider_preference_accepts_sidecar() {
    assert_eq!(
        parse_provider_preference("sidecar").unwrap(),
        EmbeddingBackend::Sidecar
    );
}

#[test]
fn test_resolver_auto_prefers_sidecar_when_available() {
    let caps = BackendResolverCapabilities {
        sidecar_available: true,
        ort_available: true,
        candle_available: true,
        target_os: "macos",
        target_arch: "aarch64",
    };
    assert_eq!(
        resolve_backend_preference(EmbeddingBackend::Auto, &caps).unwrap(),
        EmbeddingBackend::Sidecar
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_provider::test_resolver_auto_prefers_sidecar_when_available 2>&1 | tail -20`

Expected: FAIL because `Sidecar` backend variant and resolver path do not exist yet.

**Step 3: Write minimal implementation**

- Add `EmbeddingBackend::Sidecar` and `as_str()` mapping.
- Extend `parse_provider_preference` to accept `sidecar`.
- Extend `BackendResolverCapabilities` with `sidecar_available`.
- Update `resolve_backend_preference` policy to prefer sidecar for `Auto`.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_provider::test_resolver_auto_prefers_sidecar_when_available 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add src/embeddings/mod.rs src/embeddings/factory.rs src/tests/core/embedding_provider.rs && git commit -m "feat: add sidecar embedding backend and auto default resolution"`

---

### Task 2: Add sidecar feature/dependency wiring and compile guards

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/embeddings/mod.rs`
- Test: `src/tests/core/embedding_deps.rs`

**Step 1: Write the failing test/build check**

Add compile guard tests in `src/tests/core/embedding_deps.rs`:

```rust
#[test]
fn test_default_build_enables_sidecar_backend_feature() {
    assert!(
        cfg!(feature = "embeddings-sidecar"),
        "Default build should enable embeddings-sidecar feature"
    );
}
```

**Step 2: Run checks to verify failure first**

Run:
- `cargo test --lib embedding_deps::test_default_build_enables_sidecar_backend_feature 2>&1 | tail -20`
- `cargo check --features embeddings-sidecar 2>&1 | tail -20`

Expected: FAIL before feature is added.

**Step 3: Write minimal implementation**

- Add `embeddings-sidecar` feature in `Cargo.toml`.
- Include it in default features.
- Gate sidecar module exports in `src/embeddings/mod.rs`.

**Step 4: Run checks to verify pass**

Run:
- `cargo test --lib embedding_deps::test_default_build_enables_sidecar_backend_feature 2>&1 | tail -20`
- `cargo check --features embeddings-sidecar 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add Cargo.toml src/embeddings/mod.rs src/tests/core/embedding_deps.rs && git commit -m "build: wire embeddings-sidecar feature as default backend capability"`

---

### Task 3: Implement Rust-side sidecar protocol types and validation

**Files:**
- Create: `src/embeddings/sidecar_protocol.rs`
- Modify: `src/embeddings/mod.rs`
- Create: `src/tests/core/embedding_sidecar_protocol.rs`
- Modify: `src/tests/mod.rs`

**Step 1: Write the failing tests**

Add tests in `src/tests/core/embedding_sidecar_protocol.rs`:

```rust
#[test]
fn test_embed_batch_response_rejects_dimension_mismatch() {
    let resp = EmbedBatchResult {
        dims: 768,
        vectors: vec![vec![0.0; 768]],
    };
    assert!(validate_batch_response(&resp, 1).is_err());
}

#[test]
fn test_embed_batch_response_rejects_count_mismatch() {
    let resp = EmbedBatchResult {
        dims: 384,
        vectors: vec![],
    };
    assert!(validate_batch_response(&resp, 1).is_err());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_sidecar_protocol 2>&1 | tail -20`

Expected: FAIL because module and validation functions do not exist.

**Step 3: Write minimal implementation**

- Define request/response envelopes and method payload structs.
- Add protocol constants (`SCHEMA_VERSION`, `EXPECTED_DIMS=384`).
- Add validation helpers for count and dims.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_sidecar_protocol 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add src/embeddings/sidecar_protocol.rs src/embeddings/mod.rs src/tests/core/embedding_sidecar_protocol.rs src/tests/mod.rs && git commit -m "test: add sidecar protocol contract validation for count and dimensions"`

---

### Task 4: Scaffold Python sidecar package and protocol loop

**Files:**
- Create: `python/embeddings_sidecar/pyproject.toml`
- Create: `python/embeddings_sidecar/sidecar/__init__.py`
- Create: `python/embeddings_sidecar/sidecar/main.py`
- Create: `python/embeddings_sidecar/sidecar/protocol.py`
- Create: `python/embeddings_sidecar/sidecar/runtime.py`
- Create: `python/embeddings_sidecar/tests/test_protocol.py`

**Step 1: Write the failing tests**

Add tests in `python/embeddings_sidecar/tests/test_protocol.py`:

```python
def test_health_returns_ready_runtime_metadata():
    runtime = FakeRuntime()
    out = handle_health(runtime)
    assert out["ready"] is True
    assert "runtime" in out

def test_embed_batch_contract_preserves_count_and_dims():
    runtime = FakeRuntime(dims=384)
    out = handle_embed_batch(runtime, ["a", "b"])
    assert out["dims"] == 384
    assert len(out["vectors"]) == 2
```

**Step 2: Run test to verify it fails**

Run: `python3 -m pytest -q python/embeddings_sidecar/tests/test_protocol.py --tb=short`

Expected: FAIL because package and handlers do not exist.

**Step 3: Write minimal implementation**

- Implement framed stdio protocol loop.
- Implement method dispatch for `health`, `embed_query`, `embed_batch`, `shutdown`.
- Implement deterministic fake runtime for initial tests.

**Step 4: Run test to verify it passes**

Run: `python3 -m pytest -q python/embeddings_sidecar/tests/test_protocol.py --tb=short`

Expected: PASS.

**Step 5: Commit**

`git add python/embeddings_sidecar && git commit -m "feat: scaffold pytorch sidecar protocol server with health and embed endpoints"`

---

### Task 5: Implement real PyTorch embedding runtime (384-d hard guard)

**Files:**
- Modify: `python/embeddings_sidecar/sidecar/runtime.py`
- Modify: `python/embeddings_sidecar/pyproject.toml`
- Create: `python/embeddings_sidecar/tests/test_runtime.py`

**Step 1: Write the failing tests**

Add tests in `python/embeddings_sidecar/tests/test_runtime.py`:

```python
def test_runtime_reports_384_dimensions():
    rt = build_runtime(model_id="BAAI/bge-small-en-v1.5")
    assert rt.dimensions == 384

def test_runtime_embed_batch_matches_input_count():
    rt = build_runtime()
    vectors = rt.embed_batch(["foo", "bar", "baz"])
    assert len(vectors) == 3
    assert all(len(v) == 384 for v in vectors)
```

**Step 2: Run test to verify it fails**

Run: `python3 -m pytest -q python/embeddings_sidecar/tests/test_runtime.py --tb=short`

Expected: FAIL before PyTorch runtime/model wiring exists.

**Step 3: Write minimal implementation**

- Add `torch` + `sentence-transformers` dependencies.
- Implement device selection (`cuda`, `mps`, fallback `cpu`).
- Add hard guard: runtime initialization fails unless output dim is exactly 384.
- Add microbatch support for large batch requests.

**Step 4: Run test to verify it passes**

Run: `python3 -m pytest -q python/embeddings_sidecar/tests/test_runtime.py --tb=short`

Expected: PASS.

**Step 5: Commit**

`git add python/embeddings_sidecar/sidecar/runtime.py python/embeddings_sidecar/pyproject.toml python/embeddings_sidecar/tests/test_runtime.py && git commit -m "feat: add pytorch embedding runtime with strict 384-d contract"`

---

### Task 6: Implement Rust sidecar provider client (process + IPC)

**Files:**
- Create: `src/embeddings/sidecar_provider.rs`
- Modify: `src/embeddings/factory.rs`
- Modify: `src/embeddings/mod.rs`
- Create: `src/tests/core/embedding_sidecar_provider.rs`
- Modify: `src/tests/mod.rs`

**Step 1: Write the failing tests**

Add tests in `src/tests/core/embedding_sidecar_provider.rs`:

```rust
#[tokio::test]
async fn test_sidecar_provider_embed_batch_roundtrip() {
    let provider = build_test_sidecar_provider().await;
    let out = provider.embed_batch(&["a".into(), "b".into()]).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].len(), 384);
}

#[tokio::test]
async fn test_sidecar_provider_rejects_bad_dimensions() {
    let provider = build_bad_dim_sidecar_provider().await;
    let err = provider.embed_query("x").unwrap_err();
    assert!(err.to_string().contains("384"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_sidecar_provider 2>&1 | tail -20`

Expected: FAIL because sidecar provider/client does not exist.

**Step 3: Write minimal implementation**

- Spawn sidecar subprocess with framed stdio client.
- Implement `EmbeddingProvider` for sidecar.
- Validate response count and dimensions before returning vectors.
- Wire `EmbeddingProviderFactory::create` branch for `sidecar`.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_sidecar_provider 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add src/embeddings/sidecar_provider.rs src/embeddings/factory.rs src/embeddings/mod.rs src/tests/core/embedding_sidecar_provider.rs src/tests/mod.rs && git commit -m "feat: add rust sidecar embedding provider with protocol validation"`

---

### Task 7: Add managed venv bootstrap and sidecar supervision in workspace init

**Files:**
- Create: `src/embeddings/sidecar_supervisor.rs`
- Modify: `src/workspace/mod.rs`
- Modify: `src/tests/core/embedding_provider.rs`

**Step 1: Write the failing tests**

Add tests in `src/tests/core/embedding_provider.rs`:

```rust
#[tokio::test]
async fn test_workspace_init_sidecar_bootstrap_failure_falls_back_to_ort() {
    // Arrange env/config to force sidecar bootstrap failure.
    // Expect resolved backend to be ORT and degraded reason to mention sidecar bootstrap.
}

#[tokio::test]
async fn test_workspace_init_strict_accel_disables_embeddings_when_sidecar_unaccelerated() {
    // Arrange strict accel + sidecar cpu-only runtime.
    // Expect embedding_provider None and strict-mode degraded reason.
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_provider::test_workspace_init_sidecar_bootstrap_failure_falls_back_to_ort 2>&1 | tail -20`

Expected: FAIL because sidecar bootstrap/supervision path is missing.

**Step 3: Write minimal implementation**

- Implement venv bootstrap helper and sidecar launch command assembly.
- Add supervisor retry budget and unhealthy-state handling.
- Integrate sidecar startup into `initialize_embedding_provider`.
- Preserve fallback to ORT/Candle on sidecar failures.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_provider::test_workspace_init_sidecar_bootstrap_failure_falls_back_to_ort 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add src/embeddings/sidecar_supervisor.rs src/workspace/mod.rs src/tests/core/embedding_provider.rs && git commit -m "feat: add managed sidecar bootstrap and supervised fallback lifecycle"`

---

### Task 8: Surface sidecar runtime status in workspace diagnostics

**Files:**
- Modify: `src/tools/workspace/commands/registry/health.rs`
- Modify: `src/tools/workspace/commands/registry/refresh_stats.rs`
- Modify: `src/tests/tools/workspace/runtime_status_stats.rs`
- Modify: `src/tests/tools/workspace/mod_tests.rs`

**Step 1: Write the failing tests**

Add assertions that health/stats output includes sidecar-specific fields:

```rust
assert!(stats.contains("Embedding Runtime: pytorch-sidecar"));
assert!(stats.contains("Embedding Device:"));
assert!(stats.contains("Accelerated:"));
assert!(stats.contains("Degraded:"));
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib runtime_status_stats 2>&1 | tail -20`

Expected: FAIL because sidecar runtime fields are not rendered.

**Step 3: Write minimal implementation**

- Include sidecar runtime identity and device metadata in stats/health output.
- Keep existing output format backward-compatible.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib runtime_status_stats 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add src/tools/workspace/commands/registry/health.rs src/tools/workspace/commands/registry/refresh_stats.rs src/tests/tools/workspace/runtime_status_stats.rs src/tests/tools/workspace/mod_tests.rs && git commit -m "feat: expose sidecar embedding runtime status in health and stats"`

---

### Task 9: End-to-end verification for pipeline throughput and fallback safety

**Files:**
- Modify: `src/tests/integration/embedding_pipeline.rs`
- Modify: `src/tests/integration/embedding_incremental.rs`
- Modify: `src/tests/tools/hybrid_search_tests.rs`
- Create: `scripts/benchmarks/embedding_sidecar_baseline.sh`

**Step 1: Write the failing tests/bench harness**

1) Add integration test proving embedding pipeline succeeds with sidecar provider.
2) Add integration test proving sidecar timeout/crash triggers fallback provider usage.
3) Add benchmark script capturing symbols/sec for index embedding pass.

Example test sketch:

```rust
#[test]
fn test_embedding_pipeline_sidecar_fallback_on_timeout() {
    // Simulate sidecar timeout, assert fallback provider embeds successfully.
}
```

**Step 2: Run checks to verify failure first**

Run:
- `cargo test --lib embedding_pipeline 2>&1 | tail -20`
- `cargo test --lib embedding_incremental 2>&1 | tail -20`

Expected: at least one FAIL before fallback-path assertions are implemented.

**Step 3: Write minimal implementation**

- Add timeout/fallback path wiring needed for integration tests.
- Add benchmark script for before/after comparison (same corpus, same machine).

**Step 4: Run verification to pass**

Run:
- `cargo test --lib embedding_pipeline 2>&1 | tail -20`
- `cargo test --lib embedding_incremental 2>&1 | tail -20`
- `cargo test --lib hybrid_search_tests 2>&1 | tail -20`
- `cargo test --lib -- --skip search_quality 2>&1 | tail -20`

Run benchmark script:
- `bash scripts/benchmarks/embedding_sidecar_baseline.sh`

Expected: tests PASS; benchmark output reports indexing throughput and sidecar improvement.

**Step 5: Commit**

`git add src/tests/integration/embedding_pipeline.rs src/tests/integration/embedding_incremental.rs src/tests/tools/hybrid_search_tests.rs scripts/benchmarks/embedding_sidecar_baseline.sh && git commit -m "test: verify sidecar embedding e2e behavior and throughput baseline"`

---

### Task 10: Document operations and recovery paths

**Files:**
- Create: `docs/operations/embedding-sidecar.md`
- Modify: `README.md`

**Step 1: Write the failing docs check (manual)**

Define required sections for doc completeness:

- Setup/bootstrapping behavior
- Env var reference
- Health/stats interpretation
- Fallback behavior and troubleshooting

**Step 2: Run validation to verify missing sections**

Run: `grep -E "JULIE_EMBEDDING_PROVIDER|JULIE_EMBEDDING_STRICT_ACCEL|sidecar" README.md docs/operations/embedding-sidecar.md`

Expected: missing entries before writing docs.

**Step 3: Write minimal implementation**

- Add sidecar operations doc with troubleshooting matrix.
- Add short README section linking to operations guide.

**Step 4: Re-run validation**

Run: `grep -E "JULIE_EMBEDDING_PROVIDER|JULIE_EMBEDDING_STRICT_ACCEL|sidecar" README.md docs/operations/embedding-sidecar.md`

Expected: entries present.

**Step 5: Commit**

`git add docs/operations/embedding-sidecar.md README.md && git commit -m "docs: add embedding sidecar operations and troubleshooting guide"`
