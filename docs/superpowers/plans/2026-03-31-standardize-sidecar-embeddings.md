# Standardize on Sidecar Embeddings (Remove ORT Backend)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the ORT/fastembed embedding backend and standardize on the Python sidecar with CUDA support on Windows, eliminating the dual-pipeline complexity that caused the DirectML debugging nightmare.

**Architecture:** The sidecar becomes the sole embedding backend. PyTorch handles device selection natively (CUDA > DirectML > MPS > CPU). The Rust bootstrap detects CUDA availability and installs the correct torch variant. The `EmbeddingBackend` enum, factory, and init code simplify dramatically. ORT code is deleted, not hidden behind a feature flag.

**Tech Stack:** Rust (bootstrap/supervisor), Python (sentence-transformers, torch, torch-directml), uv (venv/package management)

---

## File Structure

### Files to DELETE (7 files)
- `src/embeddings/ort_provider.rs` - Entire ORT implementation (22KB)
- `src/embeddings/windows_directml.rs` - DXGI adapter enumeration (4KB)
- `src/tests/core/windows_embedding_policy.rs` - DirectML adapter selection tests
- `src/tests/integration/embedding_pipeline.rs` - ORT-only pipeline tests (file-level `#![cfg]`)
- `src/tests/integration/embedding_incremental.rs` - ORT-only incremental tests (file-level `#![cfg]`)
- `src/tests/tools/search_quality/hybrid_search_dogfood.rs` - ORT-only dogfood test
- `src/tests/tools/search_quality/semantic_similarity_dogfood.rs` - ORT-only dogfood test

### Files to MODIFY (11 files)
- `Cargo.toml` - Remove `embeddings-ort` feature, `fastembed`/`ort` deps, `windows` dep
- `src/embeddings/mod.rs` - Remove ORT modules, `Ort` variant, ORT re-exports
- `src/embeddings/factory.rs` - Simplify to sidecar-only resolution
- `src/embeddings/init.rs` - Remove ORT warmup probe, simplify init flow
- `src/embeddings/sidecar_bootstrap.rs` - Add CUDA detection and torch variant install
- `src/tests/core/embedding_provider.rs` - Remove ORT-specific tests (~15 tests)
- `src/tests/core/embedding_deps.rs` - Remove fastembed smoke tests
- `src/tests/core/mod.rs` - Remove `windows_embedding_policy` module declaration
- `src/tests/tools/workspace/mod_tests.rs` - Update `EmbeddingBackend::Ort` in test fixtures
- `.github/workflows/release.yml` - Update Windows build (no `--features embeddings-ort`)
- `README.md` - Update embedding provider docs, remove ORT env vars

### Files to CREATE (2 files)
- `src/tests/integration/sidecar_embedding_pipeline.rs` - Sidecar versions of deleted pipeline tests
- `src/tests/integration/sidecar_embedding_incremental.rs` - Sidecar versions of deleted incremental tests

### Files UNCHANGED (kept as-is)
- `src/embeddings/sidecar_provider.rs` - No changes needed
- `src/embeddings/sidecar_supervisor.rs` - No changes needed
- `src/embeddings/sidecar_protocol.rs` - No changes needed
- `src/embeddings/pipeline.rs` - Provider-agnostic, no changes
- `python/embeddings_sidecar/` - Python runtime already handles CUDA, no changes
- All sidecar test files (provider, protocol, supervisor, embedded) - Unchanged

---

## Task 1: Add CUDA Detection to Sidecar Bootstrap

**Files:**
- Modify: `src/embeddings/sidecar_bootstrap.rs:274-345` (ensure_sidecar_package_installed)
- Test: `src/tests/core/sidecar_supervisor_tests.rs` (add new tests)

This is the key new feature. When bootstrapping the sidecar venv on Windows, detect if CUDA is available and install the CUDA torch variant instead of CPU torch.

- [ ] **Step 1: Write failing tests for CUDA detection**

Add to `src/tests/core/sidecar_supervisor_tests.rs`:

```rust
#[test]
fn test_detect_cuda_from_nvidia_smi() {
    // nvidia-smi is present on machines with NVIDIA drivers
    // The function should return true/false based on command availability
    let result = crate::embeddings::sidecar_bootstrap::detect_nvidia_cuda();
    // On CI or machines without NVIDIA GPUs, this returns false
    // On machines with NVIDIA GPUs (like our dev machine), this returns true
    // We can't assert a specific value, but we can assert it doesn't panic
    let _ = result;
}

#[test]
fn test_cuda_torch_index_url() {
    let url = crate::embeddings::sidecar_bootstrap::cuda_torch_index_url();
    assert!(url.starts_with("https://download.pytorch.org/whl/cu"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tests::core::sidecar_supervisor_tests::test_detect_cuda 2>&1 | tail -10`
Expected: FAIL with "cannot find function `detect_nvidia_cuda`"

- [ ] **Step 3: Implement CUDA detection**

In `src/embeddings/sidecar_bootstrap.rs`, add:

```rust
/// Detect whether NVIDIA CUDA is available by probing for nvidia-smi.
/// This is a build-time check (run during venv creation), not a runtime check.
/// The Python sidecar handles runtime device selection via torch.cuda.is_available().
pub(crate) fn detect_nvidia_cuda() -> bool {
    let mut cmd = Command::new("nvidia-smi");
    cmd.arg("--query-gpu=driver_version")
        .arg("--format=csv,noheader")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    suppress_console_window(&mut cmd);
    cmd.status().is_ok_and(|s| s.success())
}

/// PyTorch CUDA wheel index URL. Uses CUDA 12.4 which supports
/// Ampere (RTX 30xx, A-series) and newer architectures.
pub(crate) fn cuda_torch_index_url() -> &'static str {
    "https://download.pytorch.org/whl/cu124"
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tests::core::sidecar_supervisor_tests::test_detect_cuda 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Integrate CUDA torch install into bootstrap**

In `ensure_sidecar_package_installed`, after the main `uv pip install` (or `pip install`) succeeds, add the CUDA torch swap:

```rust
// After successful base install, swap torch for CUDA variant if available.
// The base install pulls torch+cpu from PyPI. If NVIDIA CUDA is detected,
// reinstall torch from the PyTorch CUDA index. This gives us CUDA support
// while keeping all other deps (sentence-transformers, etc.) from PyPI.
if cfg!(target_os = "windows") || cfg!(target_os = "linux") {
    if detect_nvidia_cuda() {
        tracing::info!("NVIDIA CUDA detected, installing CUDA-enabled torch");

        let torch_version = read_installed_torch_version(venv_python);

        let mut cuda_cmd = if command_exists(OsStr::new("uv")) {
            let mut cmd = Command::new("uv");
            cmd.arg("pip")
                .arg("install")
                .arg("--python")
                .arg(venv_python)
                .arg("--reinstall-package")
                .arg("torch");
            if let Some(ref ver) = torch_version {
                cmd.arg(format!("torch=={ver}"));
            } else {
                cmd.arg("torch");
            }
            cmd.arg("--index-url")
                .arg(cuda_torch_index_url());
            cmd
        } else {
            let mut cmd = Command::new(venv_python);
            cmd.arg("-m")
                .arg("pip")
                .arg("install")
                .arg("--disable-pip-version-check");
            if let Some(ref ver) = torch_version {
                cmd.arg(format!("torch=={ver}"));
            } else {
                cmd.arg("torch");
            }
            cmd.arg("--index-url")
                .arg(cuda_torch_index_url());
            cmd
        };

        match run_command(&mut cuda_cmd, "CUDA torch install") {
            Ok(()) => tracing::info!("CUDA-enabled torch installed successfully"),
            Err(err) => {
                // Non-fatal: CPU torch still works, sidecar falls back gracefully
                tracing::warn!("CUDA torch install failed (CPU fallback available): {err:#}");
            }
        }
    }
}
```

Also add the helper to read the installed torch version:

```rust
/// Read the currently installed torch version from the venv so we can
/// pin the same version when swapping for the CUDA variant.
fn read_installed_torch_version(venv_python: &Path) -> Option<String> {
    let mut cmd = Command::new(venv_python);
    cmd.arg("-c")
        .arg("import torch; v=torch.__version__; print(v.split('+')[0])")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    suppress_console_window(&mut cmd);
    let output = cmd.output().ok()?;
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !version.is_empty() {
            return Some(version);
        }
    }
    None
}
```

- [ ] **Step 6: Update the install marker version**

In `src/embeddings/sidecar_supervisor.rs`, bump the marker version so existing venvs get the CUDA treatment:

```rust
// Change from:
pub(crate) const INSTALL_MARKER_VERSION: &str = "v9-vram-aware-batching";
// To:
pub(crate) const INSTALL_MARKER_VERSION: &str = "v10-cuda-torch";
```

- [ ] **Step 7: Run sidecar tests**

Run: `cargo test --lib tests::core::sidecar_supervisor_tests 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/embeddings/sidecar_bootstrap.rs src/embeddings/sidecar_supervisor.rs src/tests/core/sidecar_supervisor_tests.rs
git commit -m "$(cat <<'EOF'
feat(embeddings): detect CUDA and install CUDA-enabled torch in sidecar bootstrap

When bootstrapping the sidecar venv on Windows/Linux, probe for NVIDIA
CUDA via nvidia-smi. If present, reinstall torch from the PyTorch CUDA
index (cu124) after the base install. This gives automatic CUDA GPU
acceleration without manual torch swapping.

Non-fatal: if CUDA torch install fails, CPU torch still works and the
Python sidecar falls back gracefully.
EOF
)"
```

---

## Task 2: Port ORT Integration Tests to Sidecar

Before removing ORT, we need sidecar equivalents of the critical integration tests. The existing ORT tests in `embedding_pipeline.rs` and `embedding_incremental.rs` validate important pipeline behavior (model change detection, incremental skip, KNN search). We need sidecar versions.

**Files:**
- Create: `src/tests/integration/sidecar_embedding_pipeline.rs`
- Create: `src/tests/integration/sidecar_embedding_incremental.rs`
- Modify: `src/tests/integration/mod.rs`
- Reference: `src/tests/integration/sidecar_test_helpers.rs` (existing helper)

- [ ] **Step 1: Read existing ORT tests to understand what to port**

Read `src/tests/integration/embedding_pipeline.rs` and `src/tests/integration/embedding_incremental.rs` to understand the test scenarios. The key behaviors to preserve:
- Pipeline embeds the correct count of symbols
- KNN search works after embedding
- Empty database is handled
- Already-embedded symbols are skipped
- Model name change triggers re-embedding
- File-level incremental embedding works
- Stale embeddings are replaced

- [ ] **Step 2: Create sidecar pipeline tests**

Create `src/tests/integration/sidecar_embedding_pipeline.rs`. Use `create_test_sidecar_provider()` from `sidecar_test_helpers.rs` instead of `OrtEmbeddingProvider::try_new_cpu_only()`. The test structure mirrors the ORT tests but uses the fake sidecar provider.

Key tests to port:
- `test_sidecar_pipeline_embeds_correct_count`
- `test_sidecar_pipeline_skips_already_embedded`
- `test_sidecar_pipeline_reembeds_on_model_name_change`
- `test_sidecar_pipeline_empty_database`

Feature gate: `#![cfg(feature = "embeddings-sidecar")]`

- [ ] **Step 3: Create sidecar incremental tests**

Create `src/tests/integration/sidecar_embedding_incremental.rs`. Port the key incremental scenarios:
- `test_sidecar_embed_symbols_for_file_creates_embeddings`
- `test_sidecar_file_change_re_embeds`
- `test_sidecar_reembed_replaces_stale_embeddings`

Feature gate: `#![cfg(feature = "embeddings-sidecar")]`

- [ ] **Step 4: Register new test modules**

In `src/tests/integration/mod.rs`, add:
```rust
#[cfg(feature = "embeddings-sidecar")]
mod sidecar_embedding_pipeline;
#[cfg(feature = "embeddings-sidecar")]
mod sidecar_embedding_incremental;
```

- [ ] **Step 5: Run new tests**

Run: `cargo test --lib tests::integration::sidecar_embedding 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/tests/integration/sidecar_embedding_pipeline.rs src/tests/integration/sidecar_embedding_incremental.rs src/tests/integration/mod.rs
git commit -m "$(cat <<'EOF'
test(embeddings): port pipeline and incremental tests to sidecar provider

Sidecar equivalents of the ORT-only integration tests, using the fake
sidecar provider from sidecar_test_helpers. Validates pipeline count,
incremental skip, model change detection, and stale replacement.

Prepares for ORT removal by ensuring test coverage continuity.
EOF
)"
```

---

## Task 3: Remove ORT from Cargo.toml and Delete ORT Source Files

**Files:**
- Modify: `Cargo.toml:27-28,117-118` (features and deps)
- Delete: `src/embeddings/ort_provider.rs`
- Delete: `src/embeddings/windows_directml.rs`
- Modify: `src/embeddings/mod.rs:16-17,29-30,41,51,137-140`

- [ ] **Step 1: Remove ORT feature and dependencies from Cargo.toml**

```toml
# Change default features:
default = ["embeddings-sidecar"]

# Delete these lines entirely:
# embeddings-ort = ["dep:fastembed", "dep:ort", "ort/directml"]
# fastembed = { ... }
# ort = { ... }
```

Also remove the `windows` dependency if it was only used by `windows_directml.rs`:
```toml
# Check if windows dep is used elsewhere before removing:
# [target.'cfg(windows)'.dependencies]
# windows = { ... }
```

- [ ] **Step 2: Delete ORT implementation files**

```bash
rm src/embeddings/ort_provider.rs
rm src/embeddings/windows_directml.rs
```

- [ ] **Step 3: Clean up mod.rs**

In `src/embeddings/mod.rs`:
- Remove `#[cfg(feature = "embeddings-ort")] pub mod ort_provider;`
- Remove `#[cfg(feature = "embeddings-ort")] pub mod windows_directml;`
- Remove `EmbeddingBackend::Ort` variant from enum
- Remove `Self::Ort => "ort"` from `as_str()`
- Remove ORT re-exports (lines 137-140)
- Remove `SIDECAR_BACKEND_COMPILED` constant (no longer needed for selection)

The `EmbeddingBackend` enum simplifies to:
```rust
pub enum EmbeddingBackend {
    Auto,
    Sidecar,
    Unresolved,
    Invalid(String),
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check 2>&1 | tail -20`
Expected: Compilation errors in factory.rs, init.rs, and test files (expected, fixed in next tasks)

- [ ] **Step 5: Commit (WIP, won't compile yet)**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor(embeddings): remove ORT backend, feature flag, and dependencies

Delete ort_provider.rs, windows_directml.rs, and the embeddings-ort
Cargo feature. Removes fastembed and ort crate dependencies.

Simplifies EmbeddingBackend enum to Auto/Sidecar/Unresolved/Invalid.

WIP: factory.rs, init.rs, and tests need updating (next commits).
EOF
)"
```

---

## Task 4: Simplify Factory and Init

**Files:**
- Modify: `src/embeddings/factory.rs` (major simplification)
- Modify: `src/embeddings/init.rs` (remove ORT warmup, simplify fallback)

- [ ] **Step 1: Simplify factory.rs**

The `BackendResolverCapabilities` struct drops `ort_available`:
```rust
pub struct BackendResolverCapabilities {
    pub sidecar_available: bool,
    pub target_os: &'static str,
    pub target_arch: &'static str,
}
```

`resolve_backend_preference` simplifies dramatically:
```rust
pub fn resolve_backend_preference(
    requested_backend: EmbeddingBackend,
    capabilities: &BackendResolverCapabilities,
) -> Result<EmbeddingBackend> {
    let resolved = match requested_backend {
        EmbeddingBackend::Auto | EmbeddingBackend::Sidecar => {
            if capabilities.sidecar_available {
                EmbeddingBackend::Sidecar
            } else {
                bail!(
                    "No embedding backend available for platform {}-{}",
                    capabilities.target_os,
                    capabilities.target_arch,
                )
            }
        }
        EmbeddingBackend::Unresolved => {
            bail!("Cannot resolve embedding backend from unresolved preference")
        }
        EmbeddingBackend::Invalid(provider) => {
            bail!("Cannot resolve embedding backend from invalid preference: {provider}")
        }
    };
    Ok(resolved)
}
```

`parse_provider_preference` drops the "ort" option:
```rust
pub fn parse_provider_preference(provider: &str) -> Result<EmbeddingBackend> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(EmbeddingBackend::Auto),
        "sidecar" => Ok(EmbeddingBackend::Sidecar),
        "ort" => bail!(
            "ORT embedding backend has been removed. Use 'auto' or 'sidecar' instead. \
             The sidecar provides GPU acceleration via CUDA (NVIDIA) or DirectML (AMD/Intel)."
        ),
        unknown => bail!(
            "Unknown embedding provider: {} (valid: auto|sidecar)",
            unknown
        ),
    }
}
```

`EmbeddingProviderFactory::create` simplifies to sidecar-only:
```rust
pub fn create(config: &EmbeddingConfig) -> Result<Arc<dyn EmbeddingProvider>> {
    #[cfg(feature = "embeddings-sidecar")]
    {
        return Ok(Arc::new(SidecarEmbeddingProvider::try_new()?));
    }

    #[cfg(not(feature = "embeddings-sidecar"))]
    {
        bail!("No embedding backend available in this build");
    }
}
```

Remove `EmbeddingConfig.ort_model_id` field (no longer needed).

Remove `fallback_backend_after_init_failure` function entirely (no ORT to fall back to).

Remove `should_disable_for_strict_acceleration` and `strict_acceleration_enabled_from_env_value` if strict accel was only relevant for ORT. Check if sidecar uses it too; if so, keep.

- [ ] **Step 2: Simplify init.rs**

Remove the warmup probe (the sidecar does its own probe in `build_runtime`). Remove the ORT-specific fallback chain. The `create_embedding_provider` function becomes much shorter:

- Remove `JULIE_EMBEDDING_ORT_MODEL_ID` env var handling
- Remove `fallback_backend_after_init_failure` call
- Remove the ORT-to-sidecar and sidecar-to-ORT fallback paths
- Keep strict acceleration mode if it applies to sidecar too

- [ ] **Step 3: Verify it compiles**

Run: `cargo check 2>&1 | tail -20`
Expected: May have errors in test files (fixed in next task)

- [ ] **Step 4: Commit**

```bash
git add src/embeddings/factory.rs src/embeddings/init.rs
git commit -m "$(cat <<'EOF'
refactor(embeddings): simplify factory and init to sidecar-only

Remove all ORT-specific factory logic, EmbeddingConfig.ort_model_id,
fallback_backend_after_init_failure, and the ORT warmup probe.

The sidecar handles its own GPU probe and CPU fallback internally.
Factory resolution is now: Auto -> Sidecar, done.

Provides a clear error if someone passes JULIE_EMBEDDING_PROVIDER=ort
pointing them to the sidecar alternative.
EOF
)"
```

---

## Task 5: Clean Up Tests

**Files:**
- Delete: `src/tests/core/windows_embedding_policy.rs`
- Delete: `src/tests/integration/embedding_pipeline.rs`
- Delete: `src/tests/integration/embedding_incremental.rs`
- Delete: `src/tests/tools/search_quality/hybrid_search_dogfood.rs`
- Delete: `src/tests/tools/search_quality/semantic_similarity_dogfood.rs`
- Modify: `src/tests/core/embedding_provider.rs` (remove ORT tests)
- Modify: `src/tests/core/embedding_deps.rs` (remove fastembed tests)
- Modify: `src/tests/core/mod.rs` (remove module declarations)
- Modify: `src/tests/integration/mod.rs` (remove module declarations)
- Modify: `src/tests/tools/workspace/mod_tests.rs` (update fixtures)
- Modify: `xtask/test_tiers.toml` (update test buckets)

- [ ] **Step 1: Delete ORT-only test files**

```bash
rm src/tests/core/windows_embedding_policy.rs
rm src/tests/integration/embedding_pipeline.rs
rm src/tests/integration/embedding_incremental.rs
rm src/tests/tools/search_quality/hybrid_search_dogfood.rs
rm src/tests/tools/search_quality/semantic_similarity_dogfood.rs
```

- [ ] **Step 2: Remove module declarations**

In `src/tests/core/mod.rs`, remove:
```rust
#[cfg(windows)]
mod windows_embedding_policy;
```

In `src/tests/integration/mod.rs`, remove:
```rust
#[cfg(feature = "embeddings-ort")]
mod embedding_pipeline;
#[cfg(feature = "embeddings-ort")]
mod embedding_incremental;
```

In `src/tests/tools/search_quality/mod.rs`, remove the module declarations for the deleted dogfood tests.

- [ ] **Step 3: Clean up embedding_provider.rs**

Remove all `#[cfg(feature = "embeddings-ort")]` blocks and the tests within them:
- `test_ort_execution_provider_policy_for_current_platform`
- `test_ort_runtime_signal_*` tests
- `test_try_new_succeeds`, `test_embed_query_returns_correct_dimensions`, etc.
- The sidecar-to-ORT fallback test
- Update any remaining tests that reference `EmbeddingBackend::Ort`

- [ ] **Step 4: Clean up embedding_deps.rs**

Remove:
- `test_fastembed_single_and_batch_embedding`
- `test_ort_policy_matches_platform`
- `test_default_build_enables_ort_backend_feature`

Update `test_default_build_enables_sidecar_backend_feature` to be the primary feature check.

- [ ] **Step 5: Update workspace test fixtures**

In `src/tests/tools/workspace/mod_tests.rs`, change any `EmbeddingBackend::Ort` references in test data to `EmbeddingBackend::Sidecar`.

- [ ] **Step 6: Update xtask test tiers**

In `xtask/test_tiers.toml`, remove `windows_embedding_policy` from the `core-embeddings` bucket. Remove the deleted dogfood test entries.

- [ ] **Step 7: Run full test suite**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: PASS (all ORT tests removed, sidecar tests still pass)

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
test(embeddings): remove ORT-only tests, clean up remaining references

Delete windows_embedding_policy, ORT embedding_pipeline, ORT
embedding_incremental, and ORT-only dogfood tests. Remove ORT-specific
tests from embedding_provider.rs and embedding_deps.rs.

Sidecar equivalents from Task 2 provide equivalent coverage.
Update workspace test fixtures to use EmbeddingBackend::Sidecar.
EOF
)"
```

---

## Task 6: Update CI and Documentation

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `README.md`
- Modify: `CLAUDE.md` (if embedding backend references exist)

- [ ] **Step 1: Update release workflow**

In `.github/workflows/release.yml`, the Windows build currently uses:
```yaml
cargo_features: "--no-default-features --features embeddings-ort"
```

Change to:
```yaml
cargo_features: ""  # default features include embeddings-sidecar
```

Or if the workflow needs explicit features:
```yaml
cargo_features: "--features embeddings-sidecar"
```

Remove any comments referencing DirectML or ORT.

- [ ] **Step 2: Update README.md**

- Remove all references to ORT, DirectML, ONNX Runtime, fastembed
- Remove `JULIE_EMBEDDING_ORT_MODEL_ID` from env var table
- Update `JULIE_EMBEDDING_PROVIDER` valid values: `auto|sidecar|disabled` (remove `ort`)
- Update the embedding architecture description to reference sidecar + PyTorch
- Update the GPU acceleration description: CUDA (NVIDIA), DirectML via torch-directml (AMD/Intel), MPS (Apple Silicon)
- Note that CUDA is automatically detected and installed during first launch

- [ ] **Step 3: Update CLAUDE.md if needed**

Check for any references to ORT, DirectML, or dual embedding pipelines in CLAUDE.md and update them.

- [ ] **Step 4: Run full test suite one final time**

Run: `cargo xtask test full 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml README.md CLAUDE.md
git commit -m "$(cat <<'EOF'
docs: update CI and documentation for sidecar-only embeddings

Remove ORT/DirectML references from release workflow, README, and
CLAUDE.md. Windows build no longer needs embeddings-ort feature.

Embedding GPU support is now: CUDA (auto-detected), DirectML (via
torch-directml), MPS (macOS). All handled by the Python sidecar.
EOF
)"
```

---

## Risk Notes

1. **Binary size reduction**: Removing fastembed/ort should significantly reduce the binary size (the ORT DirectML.dll alone is 18MB). Verify with `ls -lh target/release/julie-server.exe` before and after.

2. **First-launch latency**: The sidecar venv creation + CUDA torch download adds ~2-5 minutes on first launch. This is a one-time cost, cached in `$LOCALAPPDATA/julie/embeddings/sidecar/venv/`. Document this in README.

3. **CUDA torch download size**: The CUDA torch wheel is ~2.5GB. On slow corporate networks this could be painful. The bootstrap should log progress. If the download fails, CPU torch still works.

4. **torch-directml coexistence**: When CUDA torch is installed, torch-directml is still in the venv. This is fine; `_select_device()` checks CUDA first. Both can coexist.

5. **Existing user env vars**: Users with `JULIE_EMBEDDING_PROVIDER=ort` will get a clear error message pointing them to `auto` or `sidecar`. This is a breaking change that needs a release note.

6. **Dogfood test gap**: The ORT dogfood tests (`hybrid_search_dogfood`, `semantic_similarity_dogfood`) validated search quality with real embeddings on the 100MB fixture. We should eventually create sidecar equivalents, but they require a live Python environment. Consider adding these as a separate follow-up.
