# Windows ORT DirectML Policy Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Windows embeddings resolve to ORT only, reject the Python sidecar on Windows, and choose the best DirectML adapter while skipping remote or virtual adapters.

**Architecture:** Keep platform policy in `src/embeddings/factory.rs`, move Windows DirectML adapter selection into a focused helper module, and keep `src/embeddings/ort_provider.rs` responsible for ORT initialization plus runtime fallback/telemetry. Add Windows-specific tests in a new focused test module so existing large embedding tests do not get even messier.

**Tech Stack:** Rust, ONNX Runtime via `ort`, DXGI adapter enumeration on Windows, cargo unit tests, markdown docs.

---

## File Map

- Modify: `src/embeddings/factory.rs` - Windows backend resolution policy.
- Modify: `src/embeddings/mod.rs` - export any new Windows DirectML helper if needed.
- Create: `src/embeddings/windows_directml.rs` - Windows-only adapter enumeration, filtering, ranking, and device-label helpers.
- Modify: `src/embeddings/ort_provider.rs` - use explicit DirectML device id, improve telemetry, keep CPU fallback truthful.
- Modify: `Cargo.toml` - add Windows-only dependency for DXGI enumeration if required.
- Create: `src/tests/core/windows_embedding_policy.rs` - focused tests for Windows resolver and adapter ranking.
- Modify: `src/tests/mod.rs` - register the new test module.
- Modify: `README.md` - document Windows as ORT + DirectML only.
- Modify: `docs/operations/embedding-sidecar.md` - remove Windows sidecar claims.

## Chunk 1: Lock Windows Policy With Failing Tests

### Task 1: Add failing Windows policy tests

**Files:**
- Create: `src/tests/core/windows_embedding_policy.rs`
- Modify: `src/tests/mod.rs`
- Read for context: `src/embeddings/factory.rs`, `src/embeddings/ort_provider.rs`

- [ ] **Step 1: Write a failing resolver test for Windows auto policy**

```rust
#[test]
fn test_windows_auto_prefers_ort_even_when_sidecar_is_compiled() {
    let capabilities = BackendResolverCapabilities {
        sidecar_available: true,
        ort_available: true,
        target_os: "windows",
        target_arch: "x86_64",
    };

    let resolved = resolve_backend_preference(EmbeddingBackend::Auto, &capabilities).unwrap();
    assert_eq!(resolved, EmbeddingBackend::Ort);
}
```

- [ ] **Step 2: Write a failing resolver test for explicit Windows sidecar rejection**

```rust
#[test]
fn test_windows_explicit_sidecar_is_rejected() {
    let capabilities = BackendResolverCapabilities {
        sidecar_available: true,
        ort_available: true,
        target_os: "windows",
        target_arch: "x86_64",
    };

    let err = resolve_backend_preference(EmbeddingBackend::Sidecar, &capabilities).unwrap_err();
    assert!(err.to_string().contains("Windows"));
    assert!(err.to_string().contains("sidecar"));
}
```

- [ ] **Step 3: Register the new test module**

```rust
// Inside the existing `pub mod core { ... }` block in `src/tests/mod.rs`
pub mod windows_embedding_policy;
```

- [ ] **Step 4: Run the targeted tests and verify RED**

Run: `cargo test --lib windows_embedding_policy 2>&1 | tail -20`
Expected: FAIL because Windows policy still resolves `auto` to `sidecar` and accepts explicit `sidecar`.

## Chunk 2: Implement Windows Resolver And DirectML Adapter Selection

### Task 2: Implement Windows backend policy and adapter ranking

**Files:**
- Modify: `src/embeddings/factory.rs`
- Create: `src/embeddings/windows_directml.rs`
- Modify: `src/embeddings/mod.rs`
- Modify: `src/embeddings/ort_provider.rs`
- Modify: `src/tests/core/windows_embedding_policy.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Update resolver logic for Windows policy**

Implement the smallest change that makes the resolver tests pass:

```rust
if capabilities.target_os == "windows" {
    match requested_backend {
        EmbeddingBackend::Auto => return Ok(EmbeddingBackend::Ort),
        EmbeddingBackend::Sidecar => bail!("Embedding backend 'sidecar' is not supported on Windows; use 'ort'")?,
        _ => {}
    }
}
```

- [ ] **Step 2: Write failing adapter-ranking tests before adding implementation**

Add tests that model adapters with fields like `index`, `name`, `is_software`, `is_discrete`, and assert:

```rust
DirectMlAdapterInfo {
    index: 2,
    name: "Microsoft Remote Display Adapter".into(),
    is_software: false,
    is_discrete: false,
    is_remote: true,
    is_virtual: true,
}
```

```rust
assert_eq!(select_best_adapter(&adapters).unwrap().index, 1);
```

for:
- discrete beats integrated
- Microsoft Remote Desktop / Remote Display adapters are skipped
- display-mirror and clearly virtualized adapters are skipped
- software adapters are skipped
- `None` when no eligible adapter remains

- [ ] **Step 3: Run the targeted tests and verify RED**

Run: `cargo test --lib windows_embedding_policy 2>&1 | tail -20`
Expected: FAIL because adapter ranking helper does not exist yet.

- [ ] **Step 4: Add the Windows DirectML helper module**

Create a focused module with:

```rust
pub(crate) struct DirectMlAdapterInfo {
    pub index: i32,
    pub name: String,
    pub is_software: bool,
    pub is_discrete: bool,
    pub is_remote: bool,
    pub is_virtual: bool,
}

pub(crate) fn select_best_adapter(adapters: &[DirectMlAdapterInfo]) -> Option<DirectMlAdapterInfo>
pub(crate) fn choose_directml_adapter() -> Result<Option<DirectMlAdapterInfo>>
pub(crate) fn directml_device_label(adapter: &DirectMlAdapterInfo) -> String
```

Use Windows-only DXGI enumeration inside `choose_directml_adapter()`. Keep ranking logic pure so tests can run without Windows hardware.

- [ ] **Step 5: Wire ORT to use the selected adapter id**

In `src/embeddings/ort_provider.rs`, replace the default DirectML provider with explicit device selection:

```rust
DirectMLExecutionProvider::default()
    .with_device_id(adapter.index)
    .build()
    .error_on_failure()
```

Use the adapter label in runtime telemetry, and if no adapter is eligible, initialize CPU with a degraded reason explaining that no eligible DirectML adapter was found.

- [ ] **Step 6: Keep runtime fallback telemetry truthful**

When batch-time DirectML execution falls back to CPU, make sure later calls to `device_info()`, `accelerated()`, and `degraded_reason()` reflect CPU state rather than stale GPU state.

- [ ] **Step 7: Run the targeted tests and verify GREEN**

Run: `cargo test --lib windows_embedding_policy 2>&1 | tail -20`
Expected: PASS

## Chunk 3: Cover ORT Telemetry And Update Docs

### Task 3: Add ORT telemetry tests and sync documentation

**Files:**
- Modify: `src/tests/core/windows_embedding_policy.rs`
- Modify: `src/embeddings/ort_provider.rs`
- Modify: `README.md`
- Modify: `docs/operations/embedding-sidecar.md`

- [ ] **Step 1: Add a failing ORT telemetry test**

Add tests for helper-level telemetry, for example:

```rust
#[test]
fn test_directml_runtime_signal_includes_adapter_label() {
    let signal = ort_runtime_signal_for_adapter("NVIDIA GeForce RTX 4080", false);
    assert!(signal.device.contains("RTX 4080"));
    assert!(signal.accelerated);
}
```

and a fallback case that asserts CPU plus degraded reason.

- [ ] **Step 2: Run the targeted tests and verify RED**

Run: `cargo test --lib windows_embedding_policy 2>&1 | tail -20`
Expected: FAIL because telemetry helper does not expose adapter-aware labeling yet.

- [ ] **Step 3: Implement the minimal telemetry helper changes**

Refactor `ort_runtime_signal(...)` if needed so Windows can report adapter-aware device labels without duplicating fallback logic.

- [ ] **Step 4: Update docs**

Make the docs explicit:
- `README.md`: Windows uses ORT + DirectML, macOS uses sidecar + MPS, Linux uses sidecar + CUDA.
- `docs/operations/embedding-sidecar.md`: Windows should no longer be described as a sidecar DirectML path.

- [ ] **Step 5: Run focused verification commands**

Run:
- `cargo test --lib windows_embedding_policy 2>&1 | tail -20`
- `cargo test --lib test_ort_execution_provider_policy_for_current_platform 2>&1 | tail -20`
- `python3 -m pytest -q python/embeddings_sidecar/tests/test_runtime.py -k "device_selection or directml" --tb=short`

Expected:
- Rust policy tests pass
- Existing ORT policy test still passes
- Python runtime tests still pass on non-Windows device-selection logic

- [ ] **Step 6: Run fast-tier regression verification**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -20`
Expected: PASS, unless blocked by a known unrelated baseline failure that was already present before this work.
