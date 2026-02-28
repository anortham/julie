# Candle MPS and Windows DirectML Design

## Context

Julie already has the backend seam needed for runtime pluggability:

- `EmbeddingProvider` trait abstraction
- `EmbeddingProviderFactory` + `EmbeddingConfig`
- feature gates for `embeddings-ort` (default) and `embeddings-candle` (placeholder)

The remaining gaps are:

1. Apple Silicon should prefer Candle automatically (MPS acceleration).
2. Windows ORT should prefer DirectML, not silently run CPU-only.
3. Fallback behavior must be visible in health/stats, not only logs.

## Goals

- Auto-select Candle on Apple Silicon by default.
- Keep ORT as cross-platform fallback and explicit override.
- On Windows ORT path, attempt `DirectML -> CPU` in that order.
- Mark fallback-to-CPU as degraded status (not silent).
- Preserve current non-fatal embedding startup behavior (keyword search still works).
- Preserve 384-dimension vector compatibility with existing sqlite-vec schema.

## Non-goals

- Full embedding storage migration away from `float[384]`.
- Replacing ORT globally.
- Changing semantic scoring/ranking behavior.
- Forcing acceleration-only behavior by default on all platforms.

## Chosen Approach

Use a unified runtime selector with platform-aware defaults and explicit degraded status:

- Backend preference is resolved centrally in `EmbeddingProviderFactory`.
- Default selection policy:
  - Apple Silicon (`macos` + `aarch64`): Candle
  - Everything else: ORT
- Explicit provider override remains supported through env/config.
- Windows ORT uses execution provider order `DirectML -> CPU`.
- If preferred acceleration is unavailable, fallback to CPU is allowed by default but reported as degraded.
- Strict mode is available for CI/local validation to fail embedding init when acceleration cannot be used.

## Configuration Contract

### Environment variables

- `JULIE_EMBEDDING_PROVIDER=auto|ort|candle`
  - default: `auto`
- `JULIE_EMBEDDING_STRICT_ACCEL=0|1`
  - default: `0`
  - `1` means no fallback from preferred accelerator backend/device

### Validation rules

- Unknown provider values fail fast with clear guidance.
- `candle` on builds without `embeddings-candle` is an explicit initialization error.
- `ort` on builds without `embeddings-ort` is an explicit initialization error.

## Architecture

### 1) Backend resolution in factory

Add explicit resolution logic in `EmbeddingProviderFactory`:

- Inputs: provider preference (`auto|ort|candle`), platform info, feature availability.
- Output: provider instance and initialization status metadata.

Initialization status metadata includes:

- requested backend
- resolved backend
- runtime/device
- accelerated (bool)
- degraded reason (optional)

### 2) Provider implementations

#### ORT provider

- Keep existing `OrtEmbeddingProvider` as baseline implementation.
- On Windows, configure ORT execution providers in explicit order:
  - `DirectML`
  - `CPU`
- CPU fallback is acceptable when DirectML registration or availability fails, but it must mark degraded status.

Implementation note:

- Use `fastembed` execution-provider configuration (`with_execution_providers(...)`) and ensure ORT build features enable DirectML registration path on Windows.
- If required by dependency constraints, enable a compatible ORT registration mode (for example `load-dynamic`) as part of ORT backend feature wiring.

#### Candle provider

- Add `CandleEmbeddingProvider` behind `embeddings-candle`.
- Must provide embeddings compatible with existing storage/search assumptions:
  - dimension must be exactly 384.
- If Candle model/config yields non-384 dimensions, fail init with actionable error (do not silently project vectors).

### 3) Workspace state and diagnostics

`JulieWorkspace` stores both:

- active `embedding_provider` (existing)
- embedding runtime status metadata (new)

Expose this in:

- `manage_workspace health`
- `manage_workspace stats`

so users can see acceleration status and degradation reason directly.

## Data Flow

1. `initialize_all_components()` calls `initialize_embedding_provider()`.
2. Provider preference and strict mode are read from env/config.
3. Factory resolves backend using platform-aware `auto` policy.
4. Factory attempts provider creation:
   - Candle on Apple Silicon when selected and available
   - ORT otherwise
5. ORT on Windows attempts `DirectML -> CPU`.
6. Workspace stores provider + runtime status metadata.
7. Health/stats commands render backend/device/degraded information.

## Error Handling and Fallback

- Default behavior: best-effort init, never block keyword search.
- Degraded behavior (fallback happened):
  - Provider remains usable.
  - Runtime status records degraded reason.
  - Health/stats display degraded marker.
- Strict mode (`JULIE_EMBEDDING_STRICT_ACCEL=1`):
  - If preferred acceleration cannot be activated, embeddings are set to unavailable (`None`) and reason is reported.

## Testing Strategy

### Unit tests

- Factory resolution matrix for:
  - `auto`, `ort`, `candle`
  - Apple Silicon, macOS x64, Windows, Linux
  - feature combinations (`embeddings-ort`, `embeddings-candle`, both)
- Invalid provider value handling.
- Strict mode behavior.
- Degraded status formation when fallback occurs.

### ORT policy tests

- Windows EP selection helper tests ensure preferred order is `DirectML -> CPU`.
- Non-Windows behavior remains unchanged.

### Workspace/command tests

- `stats` and `health` outputs include backend/device/accelerated/degraded fields.
- Existing embedding count reporting remains intact.

### Dimension guard tests

- Candle/ORT provider init rejects dimension mismatch for the configured vector schema.
- Existing vector storage and KNN tests remain green with 384-dim expectations.

### Targeted verification commands

- `cargo test --lib embedding_provider 2>&1 | tail -20`
- `cargo test --lib embedding_deps 2>&1 | tail -20`
- `cargo test --lib reference_workspace 2>&1 | tail -20`
- `cargo test --lib -- --skip search_quality 2>&1 | tail -20`
- feature checks:
  - `cargo check --features embeddings-ort 2>&1 | tail -20`
  - `cargo check --features embeddings-candle 2>&1 | tail -20`
  - `cargo check --features "embeddings-ort embeddings-candle" 2>&1 | tail -20`

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| DirectML registration path not active in dependency build | Windows still CPU-only | Add explicit dependency/feature wiring test and runtime status assertion in CI |
| Candle model dimension differs from 384 | Incompatible with sqlite-vec schema | Hard fail at provider init with explicit error; do not silently reshape/project |
| Auto-selection surprises existing users | Behavior confusion | Preserve explicit override via env; report selected backend/device in stats/health |
| Acceleration unavailable on user machine | Lower performance | Degraded fallback by default + strict mode for validation |

## Exit Criteria

- Apple Silicon defaults to Candle when `embeddings-candle` is enabled.
- Windows ORT path attempts DirectML and reports degraded CPU fallback when needed.
- Diagnostics show backend/device/acceleration/degraded reason.
- No regressions in existing embedding/search behavior.
- Fast-tier tests pass.
