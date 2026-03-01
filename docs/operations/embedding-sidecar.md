# Embedding Sidecar Operations

Julie defaults to a Python sidecar runtime for embeddings (`JULIE_EMBEDDING_PROVIDER=auto` resolves to `sidecar` first when the feature is compiled in).

## Bootstrap Flow (Managed venv)

1. Workspace init builds embedding config from env (`JULIE_EMBEDDING_PROVIDER`, `JULIE_EMBEDDING_CACHE_DIR`).
2. Backend resolver chooses `sidecar` first for `auto` when available.
3. Sidecar launch config is resolved in this order:
   - If `JULIE_EMBEDDING_SIDECAR_PROGRAM` is set, Julie launches that program override.
     - Default override mode: Julie still applies Python-style entrypoint args (`JULIE_EMBEDDING_SIDECAR_SCRIPT` or `-m JULIE_EMBEDDING_SIDECAR_MODULE`) and may inject `PYTHONPATH`.
     - Raw override mode (`JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM=1|true|on`): Julie executes the program with no implicit args and no injected env.
   - Otherwise Julie uses a managed venv (`.../embeddings/sidecar/venv`) and launches its Python.
4. Managed venv behavior:
   - If `uv` is available, creates venv with `uv venv --python 3.12` (handles uv's standalone Python builds correctly).
   - If no compatible Python 3.10-3.13 is found, auto-installs one via `uv python install 3.12`.
   - Falls back to `python -m venv` only when `uv` is not available.
   - Installs sidecar package with `uv pip install --editable .[runtime]` (or `pip install` as fallback).
   - Writes install marker `.julie-sidecar-install-root` in the venv (versioned marker content + root) to avoid unnecessary reinstalls.
5. Sidecar process is health-probed (`health` IPC) before being accepted. Health response includes device (cuda/mps/directml/cpu), runtime, model ID, and dimensions.

Operational caveat (release binaries): default sidecar root is derived from build-time `CARGO_MANIFEST_DIR` (`<manifest>/python/embeddings_sidecar`). On prebuilt binaries that path may not exist on the target machine.

- Recommended overrides for release-binary usage:
  - Set `JULIE_EMBEDDING_SIDECAR_PROGRAM` to a Python interpreter or sidecar launcher available on the target host.
  - Optionally set `JULIE_EMBEDDING_SIDECAR_SCRIPT` (or `JULIE_EMBEDDING_SIDECAR_MODULE`) for explicit entrypoint control.
  - Or set `JULIE_EMBEDDING_SIDECAR_ROOT` to a valid deployed sidecar package root.
- If sidecar bootstrap/init fails in `auto` mode and strict mode is off, Julie falls back to `ort` when available.

## Environment Variables

### Core embedding controls

- `JULIE_EMBEDDING_PROVIDER`: `auto|sidecar|ort` (default: `auto`).
- `JULIE_EMBEDDING_STRICT_ACCEL`: `1|true|on` enables strict acceleration mode.
  - In strict mode, unaccelerated/degraded runtimes are disabled instead of used.
- `JULIE_EMBEDDING_CACHE_DIR`: base cache dir.
  - Used for managed sidecar venv location and ORT model cache.
  - If unset, cache base falls back to `XDG_CACHE_HOME/julie`, then `LOCALAPPDATA/julie`, `APPDATA/julie`, `~/.cache/julie`, then temp dir.

### Sidecar launch/override controls

- `JULIE_EMBEDDING_SIDECAR_ROOT`: sidecar package root (defaults to `python/embeddings_sidecar` in this repo).
- `JULIE_EMBEDDING_SIDECAR_VENV`: managed venv path override.
- `JULIE_EMBEDDING_SIDECAR_PROGRAM`: program to execute instead of managed venv Python.
- `JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM`: truthy flag (`1|true|on`) to run `JULIE_EMBEDDING_SIDECAR_PROGRAM` with no implicit args/env.
- `JULIE_EMBEDDING_SIDECAR_SCRIPT`: script path to run (used instead of `-m <module>`).
- `JULIE_EMBEDDING_SIDECAR_MODULE`: module for `-m` launch (default: `sidecar.main`).
- `JULIE_EMBEDDING_SIDECAR_BOOTSTRAP_PYTHON`: interpreter for creating managed venv.
- `JULIE_EMBEDDING_SIDECAR_TIMEOUT_MS`: per-request IPC timeout in milliseconds (default: `5000`).
- `JULIE_EMBEDDING_SIDECAR_INIT_TIMEOUT_MS`: health-probe/init timeout in milliseconds (default: `120000`).
- `JULIE_EMBEDDING_SIDECAR_MODEL_ID`: sidecar model id override (default: `BAAI/bge-small-en-v1.5`).
- `JULIE_EMBEDDING_SIDECAR_BATCH_SIZE`: embedding batch size override for sidecar runtime (default: `32`).

## Interpreting Health and Stats

`manage_workspace(operation="health")` and `manage_workspace(operation="stats")` report:

- `Runtime`: provider runtime identity.
- `Backend`: resolved backend (`sidecar`, `ort`, `unresolved`).
- `Device`: provider device label.
- `Accelerated`: runtime acceleration flag (`true|false`), and may be `unknown` in `stats` for non-primary workspaces.
- `Degraded`: reason text if fallback/degraded/unavailable state exists.

Health-only status labels:

- `Embedding Status: INITIALIZED` + `Degraded: none`: embeddings active and not degraded.
- `Embedding Status: DEGRADED`: embeddings active, but runtime had a fallback/degraded reason.
- `Embedding Status: UNAVAILABLE`: embeddings disabled/unavailable; keyword search still works.
- `Embedding Status: NOT INITIALIZED`: runtime metadata not set yet.

`stats` does not emit `Embedding Status: ...` labels; it reports runtime/backend/device/accelerated/degraded fields directly.

Sidecar telemetry reports the actual device from the Python runtime (e.g., `Runtime: python-sidecar (sentence-transformers)`, `Device: cuda`).

## Fallback Behavior

- `auto` preference:
  - Tries `sidecar` first.
  - If sidecar initialization fails and strict mode is off, falls back to `ort`.
  - Degraded reason includes sidecar failure context.
- Explicit provider (`sidecar`, `ort`): no automatic cross-backend fallback on init failure.
- Strict acceleration mode (`JULIE_EMBEDDING_STRICT_ACCEL` enabled):
  - Disables embeddings when runtime is unaccelerated/degraded.
  - Disables auto-fallback after init failures.
- In all failure paths, keyword search remains available.

## Troubleshooting Matrix

| Symptom | Likely Cause | Action |
| --- | --- | --- |
| `Embedding provider unavailable ... sidecar bootstrap failed: no Python 3.10-3.13 interpreter found` | No compatible Python on PATH and `uv` unavailable or auto-install failed | Install `uv` (recommended — Julie will auto-install Python 3.12), or install Python 3.12 manually, or set `JULIE_EMBEDDING_SIDECAR_BOOTSTRAP_PYTHON` |
| `... sidecar root '...' does not exist` or missing `pyproject.toml` | Invalid sidecar root override | Fix `JULIE_EMBEDDING_SIDECAR_ROOT` or remove override |
| `timed out waiting for sidecar response ...` | Sidecar hung or too slow | Increase `JULIE_EMBEDDING_SIDECAR_TIMEOUT_MS`; inspect sidecar runtime load |
| `Embedding Status: UNAVAILABLE` with strict acceleration reason | Strict mode rejected unaccelerated/degraded runtime | Disable strict mode or ensure accelerated backend is available |
| `Backend: ort` with degraded reason mentioning sidecar | Auto sidecar failed and ORT fallback activated | Review degraded reason for root cause; fix sidecar bootstrap/runtime if sidecar is desired |
| `Unknown embedding provider` | Invalid `JULIE_EMBEDDING_PROVIDER` value | Use one of `auto|sidecar|ort` |
