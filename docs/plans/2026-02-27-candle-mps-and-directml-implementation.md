# Candle MPS and DirectML Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Auto-select Candle on Apple Silicon for embedding acceleration, enforce DirectML-first ORT behavior on Windows, and expose degraded fallback status in workspace diagnostics.

**Architecture:** Keep the existing `EmbeddingProvider` + factory seam and add a deterministic backend resolver (`auto|ort|candle`) with platform-aware defaults. Return structured runtime status from provider initialization so fallback behavior is visible in `health`/`stats`. Preserve current best-effort startup and keyword-search continuity, with optional strict acceleration mode for CI and validation.

**Tech Stack:** Rust, fastembed/ORT, Candle (feature-gated), tokio, existing Julie workspace init/registry commands, Rust test harness.

---

### Task 1: Add Backend Preference + Runtime Status Types

**Files:**
- Modify: `src/embeddings/factory.rs`
- Modify: `src/embeddings/mod.rs`
- Modify: `src/workspace/mod.rs`
- Test: `src/tests/core/embedding_provider.rs`

**Step 1: Write the failing test**

Add unit tests for config parsing/default behavior:

```rust
#[test]
fn test_embedding_config_defaults_to_auto() {
    let cfg = EmbeddingConfig::default();
    assert_eq!(cfg.provider, "auto");
}

#[test]
fn test_embedding_config_rejects_unknown_provider() {
    let err = parse_provider("wat").unwrap_err();
    assert!(err.to_string().contains("auto|ort|candle"));
}
```

Also add a failing assertion that `JulieWorkspace` initializes with `embedding_runtime_status: None` before provider init.

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_provider::test_embedding_config_defaults_to_auto 2>&1 | tail -20`

Expected: FAIL because default is currently `ort` and runtime status field does not exist.

**Step 3: Write minimal implementation**

- Add provider preference parsing (`auto|ort|candle`) in factory config.
- Add runtime status struct in embeddings module (requested backend, resolved backend, accelerated bool, degraded reason).
- Add `embedding_runtime_status` field to `JulieWorkspace` and clone/initialization wiring.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_provider::test_embedding_config_defaults_to_auto 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add src/embeddings/factory.rs src/embeddings/mod.rs src/workspace/mod.rs src/tests/core/embedding_provider.rs && git commit -m "refactor: add embedding backend preference and runtime status types"`

---

### Task 2: Implement Deterministic Backend Resolver (Auto Policy)

**Files:**
- Modify: `src/embeddings/factory.rs`
- Test: `src/tests/core/embedding_provider.rs`

**Step 1: Write the failing test**

Add resolver matrix tests using a pure helper (no real model initialization):

```rust
#[test]
fn test_auto_selects_candle_on_macos_arm64() {
    let out = resolve_backend("auto", PlatformInfo::new("macos", "aarch64"), caps(true, true)).unwrap();
    assert_eq!(out.backend, "candle");
}

#[test]
fn test_auto_selects_ort_elsewhere() {
    let out = resolve_backend("auto", PlatformInfo::new("windows", "x86_64"), caps(true, true)).unwrap();
    assert_eq!(out.backend, "ort");
}
```

Add tests for feature-unavailable cases (`candle` requested but candle feature unavailable => error).

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_provider::test_auto_selects_candle_on_macos_arm64 2>&1 | tail -20`

Expected: FAIL because resolver helper does not exist yet.

**Step 3: Write minimal implementation**

- Implement `resolve_backend(...)` helper in factory.
- Keep explicit provider overrides higher priority than auto.
- Use compile-time feature capability inputs for production path.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_provider::test_auto_selects_candle_on_macos_arm64 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add src/embeddings/factory.rs src/tests/core/embedding_provider.rs && git commit -m "feat: resolve embedding backend with apple-silicon candle auto policy"`

---

### Task 3: Add ORT Windows Execution Provider Policy (DirectML -> CPU)

**Files:**
- Modify: `src/embeddings/ort_provider.rs`
- Modify: `src/embeddings/factory.rs`
- Test: `src/tests/core/embedding_provider.rs`

**Step 1: Write the failing test**

Add tests around a pure EP policy helper:

```rust
#[test]
fn test_windows_ep_policy_prefers_directml_then_cpu() {
    let policy = ort_ep_policy("windows");
    assert_eq!(policy, vec!["directml", "cpu"]);
}

#[test]
fn test_non_windows_ep_policy_uses_cpu_default() {
    let policy = ort_ep_policy("linux");
    assert_eq!(policy, vec!["cpu"]);
}
```

Add test that fallback from DirectML sets degraded status metadata.

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_provider::test_windows_ep_policy_prefers_directml_then_cpu 2>&1 | tail -20`

Expected: FAIL because policy helper and degraded metadata are not implemented.

**Step 3: Write minimal implementation**

- Add EP policy helper in `ort_provider.rs`.
- Pass explicit execution providers to `fastembed::InitOptions`.
- Update factory/provider init path to annotate degraded status when DirectML is unavailable and CPU fallback is used.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_provider::test_windows_ep_policy_prefers_directml_then_cpu 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add src/embeddings/ort_provider.rs src/embeddings/factory.rs src/tests/core/embedding_provider.rs && git commit -m "feat: prefer DirectML on windows with explicit degraded CPU fallback"`

---

### Task 4: Implement Strict Acceleration Mode

**Files:**
- Modify: `src/workspace/mod.rs`
- Modify: `src/embeddings/factory.rs`
- Test: `src/tests/core/embedding_provider.rs`

**Step 1: Write the failing test**

Add tests for strict mode behavior:

```rust
#[test]
fn test_strict_accel_rejects_degraded_provider() {
    let cfg = TestInitCfg { strict_accel: true, force_degraded: true };
    let err = initialize_with_test_cfg(cfg).unwrap_err();
    assert!(err.to_string().contains("strict acceleration"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_provider::test_strict_accel_rejects_degraded_provider 2>&1 | tail -20`

Expected: FAIL because strict mode is not implemented.

**Step 3: Write minimal implementation**

- Parse `JULIE_EMBEDDING_STRICT_ACCEL` in workspace init path.
- If provider init result is degraded and strict mode is on, set `embedding_provider = None` and preserve status reason.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_provider::test_strict_accel_rejects_degraded_provider 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add src/workspace/mod.rs src/embeddings/factory.rs src/tests/core/embedding_provider.rs && git commit -m "feat: add strict acceleration mode for embedding backend init"`

---

### Task 5: Add Candle Provider (Feature-Gated) with 384-Dim Guard

**Files:**
- Create: `src/embeddings/candle_provider.rs`
- Modify: `src/embeddings/mod.rs`
- Modify: `src/embeddings/factory.rs`
- Test: `src/tests/core/embedding_provider.rs`

**Step 1: Write the failing test**

Add tests for Candle selection and dimension guard:

```rust
#[test]
fn test_factory_creates_candle_provider_when_enabled() {
    let cfg = EmbeddingConfig { provider: "candle".into(), cache_dir: None };
    let provider = EmbeddingProviderFactory::create(&cfg).unwrap();
    assert_eq!(provider.dimensions(), 384);
}

#[test]
fn test_candle_provider_rejects_non_384_dimensions() {
    let err = build_candle_provider_for_test(768).unwrap_err();
    assert!(err.to_string().contains("384"));
}
```

Guard tests with `#[cfg(feature = "embeddings-candle")]` as needed.

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_provider::test_factory_creates_candle_provider_when_enabled --features embeddings-candle 2>&1 | tail -20`

Expected: FAIL because Candle provider module does not exist.

**Step 3: Write minimal implementation**

- Implement `CandleEmbeddingProvider` behind `embeddings-candle`.
- Ensure `dimensions() == 384` for selected Candle model/config.
- Wire factory `"candle"` branch.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_provider::test_factory_creates_candle_provider_when_enabled --features embeddings-candle 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add src/embeddings/candle_provider.rs src/embeddings/mod.rs src/embeddings/factory.rs src/tests/core/embedding_provider.rs && git commit -m "feat: add candle embedding provider for apple-silicon acceleration"`

---

### Task 6: Update Cargo Feature Wiring for ORT + Candle

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/tests/core/embedding_deps.rs`

**Step 1: Write the failing test/build checks**

Add/adjust compile assertions in `embedding_deps`:

```rust
#[test]
fn test_feature_matrix_compile_guards() {
    assert!(cfg!(feature = "embeddings-ort") || cfg!(feature = "embeddings-candle"));
}
```

Add build-check expectations for all supported feature combinations.

**Step 2: Run checks to verify failure first**

Run:
- `cargo test --lib embedding_deps 2>&1 | tail -20`
- `cargo check --features embeddings-candle 2>&1 | tail -20`

Expected: at least one check fails before wiring is complete.

**Step 3: Write minimal implementation**

- Ensure features cleanly gate provider modules.
- Ensure ORT dependency wiring supports Windows DirectML registration path.
- Keep default behavior compatible (`embeddings-ort` remains enabled unless explicitly changed).

**Step 4: Run checks to verify pass**

Run:
- `cargo test --lib embedding_deps 2>&1 | tail -20`
- `cargo check --features embeddings-ort 2>&1 | tail -20`
- `cargo check --features embeddings-candle 2>&1 | tail -20`
- `cargo check --features "embeddings-ort embeddings-candle" 2>&1 | tail -20`

Expected: all PASS.

**Step 5: Commit**

`git add Cargo.toml src/tests/core/embedding_deps.rs && git commit -m "build: wire ort and candle embedding feature matrix"`

---

### Task 7: Surface Runtime Status in `stats` and `health`

**Files:**
- Modify: `src/tools/workspace/commands/registry/refresh_stats.rs`
- Modify: `src/tools/workspace/commands/registry/health.rs`
- Create: `src/tests/tools/workspace/embedding_runtime_status.rs`
- Modify: `src/tests/mod.rs`

**Step 1: Write the failing test**

Add tool-level tests asserting output contains runtime metadata fields:

```rust
assert!(text.contains("Embedding Backend:"));
assert!(text.contains("Embedding Device:"));
assert!(text.contains("Accelerated:"));
```

Add degraded-case assertion:

```rust
assert!(text.contains("Degraded:"));
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_runtime_status 2>&1 | tail -20`

Expected: FAIL because commands do not include runtime status fields.

**Step 3: Write minimal implementation**

- Extend stats/health output formatting with runtime status data from workspace.
- Keep existing embedding count output unchanged.

**Step 4: Run test to verify it passes**

Run: `cargo test --lib embedding_runtime_status 2>&1 | tail -20`

Expected: PASS.

**Step 5: Commit**

`git add src/tools/workspace/commands/registry/refresh_stats.rs src/tools/workspace/commands/registry/health.rs src/tests/tools/workspace/embedding_runtime_status.rs src/tests/mod.rs && git commit -m "feat: expose embedding runtime and degraded status in workspace diagnostics"`

---

### Task 8: End-to-End Verification and Regression Safety

**Files:**
- Modify (if needed): `src/tests/core/embedding_provider.rs`
- Modify (if needed): `src/tests/core/embedding_deps.rs`
- Modify (if needed): `src/tests/tools/workspace/embedding_runtime_status.rs`

**Step 1: Run focused test suites**

Run:
- `cargo test --lib embedding_provider 2>&1 | tail -20`
- `cargo test --lib embedding_deps 2>&1 | tail -20`
- `cargo test --lib embedding_runtime_status 2>&1 | tail -20`

**Step 2: Run integration sanity checks**

Run:
- `cargo test --lib reference_workspace 2>&1 | tail -20`
- `cargo test --lib embedding_incremental 2>&1 | tail -20`

**Step 3: Run fast-tier regression suite**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -20`

Expected: PASS.

**Step 4: Commit final test adjustments**

`git add src/tests/core/embedding_provider.rs src/tests/core/embedding_deps.rs src/tests/tools/workspace/embedding_runtime_status.rs && git commit -m "test: verify backend selection and acceleration diagnostics across platforms"`
