# Windows ORT DirectML Policy Design

## Goal

Make Windows embeddings use ONNX Runtime with DirectML as the only supported accelerated path, align source builds with release builds, and add deterministic DirectML adapter selection that prefers a discrete GPU while skipping remote or virtual adapters.

## Decisions

- Windows treats the Python sidecar as unsupported for embeddings.
- `auto` resolves to `ort` on Windows even when the sidecar feature is compiled.
- Explicit `sidecar` on Windows is rejected as unavailable with a clear error.
- DirectML adapter selection is handled only in the ORT backend.
- Adapter selection skips Microsoft Remote Desktop / Remote Display style adapters and other non-physical adapters.
- If both discrete and integrated GPUs are present, discrete wins.

## Architecture

### Backend Resolution

Update backend resolution so platform policy is encoded in runtime logic rather than only in release packaging.

- Windows:
  - `auto` -> `ort`
  - `sidecar` -> unavailable error
  - `ort` -> normal ORT initialization
- macOS:
  - preserve sidecar-first policy for MPS
- Linux:
  - preserve sidecar-first policy for CUDA

This keeps release binaries and source builds consistent.

### DirectML Adapter Selection

Add a Windows-only adapter selection helper used by the ORT provider.

- Enumerate DXGI adapters.
- Ignore adapters that are software, remote, display-mirror, or clearly virtualized.
- Explicitly skip Microsoft Remote Desktop / Remote Display adapters by vendor/name heuristics.
- Rank remaining adapters so discrete GPUs beat integrated GPUs.
- Use the winning adapter index with DirectML execution provider configuration instead of relying on the default adapter.
- If no eligible adapter exists, fall back to CPU and record a degraded reason.

### Diagnostics

Improve ORT device telemetry so Windows reports the selected adapter instead of a generic `DirectML (GPU)` label.

- Include adapter name and index in device info.
- Preserve truthful degraded reasons for CPU fallback.
- Keep health/stats output able to explain why DirectML was not used.

## Testing Strategy

Follow TDD for each behavior change.

- Resolver tests for Windows `auto -> ort`.
- Resolver tests for Windows explicit `sidecar` rejection.
- Windows-only adapter ranking tests using fake adapter metadata:
  - discrete beats integrated
  - remote/virtual adapters are skipped
  - CPU fallback when no eligible adapter exists
- ORT telemetry tests for selected adapter labeling and degraded reasons.
- Docs updates verified by targeted file review.

## Documentation Changes

- Update `README.md` so Windows is documented as ORT + DirectML only.
- Update `docs/operations/embedding-sidecar.md` so sidecar acceleration is described as macOS MPS and Linux CUDA, not Windows.

## Non-Goals

- Reworking Linux CUDA sidecar behavior.
- Reworking macOS MPS sidecar behavior.
- Adding a user-facing adapter override in this change.
- Keeping Windows sidecar as an advanced escape hatch.

## Risks

- DXGI heuristics may need conservative matching so we do not filter valid GPUs.
- Adapter ordering must remain deterministic and testable without Windows hardware in CI.
- ORT fallback telemetry must stay accurate after runtime fallback to CPU.
