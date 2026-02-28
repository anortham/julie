# Python PyTorch Embeddings Sidecar - Design Document

> **Date:** 2026-02-28
> **Status:** Approved (brainstormed with user)
> **Scope:** Replace default in-process embedding runtime with a managed Python/PyTorch sidecar while preserving Julie's current vector contract and fallback resilience

## Problem Statement

Julie currently embeds through in-process runtimes (`ort` and optional `candle`). That keeps deployment simple, but it constrains acceleration options and makes runtime behavior sensitive to platform-specific backend quirks.

The goal is to make a Python/PyTorch sidecar the default embedding path so indexing can better exploit hardware acceleration, while preserving Julie's reliability guarantees:

- Keyword search must continue even if embeddings fail.
- Semantic embeddings should degrade gracefully.
- Existing vector schema and ranking assumptions should remain compatible.

## User-Confirmed Constraints

1. **Default runtime:** Sidecar is default (not opt-in).
2. **Fallback behavior:** Keep automatic fallback if sidecar is unavailable.
3. **Vector compatibility:** Preserve **384 dimensions**.
4. **Packaging:** Julie manages a local Python venv.
5. **Primary success metric:** Faster embedding indexing throughput.
6. **Rollout mode:** Immediate default (not feature-flag-only rollout).

## Goals

- Make sidecar-backed embeddings the default `auto` provider path.
- Improve bulk indexing throughput and incremental embedding speed.
- Preserve existing DB/vector compatibility (`float[384]` in sqlite-vec).
- Keep current non-fatal embedding startup semantics.
- Expose runtime status (backend/device/acceleration/degraded reason).

## Non-Goals

- Replacing Tantivy or SQLite.
- Changing hybrid scoring/ranking logic.
- Migrating vector schema to new dimensions.
- Removing in-process providers in this phase.
- Introducing remote/networked embedding service requirements.

## Approaches Considered

### 1) Persistent local sidecar process (chosen)

- Julie starts and supervises one local Python process.
- Model stays warm; requests are sent over local IPC.
- Best throughput for indexing due to microbatching and reduced startup overhead.

### 2) Spawn-per-request Python

- Very simple implementation.
- Rejected due to process startup overhead and poor query latency.

### 3) External standalone service

- Good for centralized GPU infra, but adds ops/network complexity.
- Rejected for now; local managed runtime is the desired UX.

## High-Level Architecture

### Rust-side components

1. **New provider implementation:** `PyTorchSidecarEmbeddingProvider` implementing `EmbeddingProvider`.
2. **Factory integration:** extend backend preference parsing and resolver to include `sidecar`.
3. **Workspace init integration:** sidecar bootstrap/supervision in embedding provider initialization path.
4. **Diagnostics integration:** report sidecar runtime status in health/stats outputs.

### Python-side components

1. **Sidecar entrypoint:** process lifecycle + IPC loop.
2. **Model runtime module:** load embedding model, select device, execute batched inference.
3. **Protocol handlers:** `health`, `embed_query`, `embed_batch`, `shutdown`.
4. **Runtime telemetry:** model/device/version/capability metadata for Rust diagnostics.

### Compatibility contract

- Sidecar **must** return 384-d vectors.
- Returned vector count must match request count for batch requests.
- Existing `run_embedding_pipeline`, `reembed_symbols_for_file`, and query-time semantic path remain call-site compatible via the existing trait.

## Transport and Protocol Design

### Transport

- Use local subprocess IPC over stdio with framed messages.
- Framing: 4-byte length prefix + MessagePack payload.
- No open TCP ports in the default local mode.

### Message model (conceptual)

- Request envelope: `id`, `method`, `params`, `schema_version`.
- Response envelope: `id`, `ok`, `result` or `error`.

### Core methods

1. `health`
   - Returns runtime identity and readiness.
2. `embed_query`
   - Input: single text.
   - Output: one 384-d vector.
3. `embed_batch`
   - Input: list of texts.
   - Output: N vectors with strict count and dimension guarantees.
4. `shutdown`
   - Graceful termination for workspace shutdown.

### Response metadata

Health and embedding responses include useful runtime details:

- runtime name (`pytorch-sidecar`)
- model id
- device backend (`cuda`, `mps`, `cpu`, etc.)
- acceleration boolean
- optional degraded reason

## Data Flow

### Startup flow

1. `initialize_embedding_provider` resolves `auto` to sidecar first.
2. Julie ensures managed venv exists and dependencies are installed.
3. Julie launches sidecar and performs handshake (`health`).
4. If handshake succeeds, provider becomes active and runtime status is recorded.
5. If startup fails, fallback resolution continues to in-process providers.

### Indexing flow (priority path)

1. Existing indexing code assembles symbol metadata strings.
2. Rust provider sends `embed_batch` requests.
3. Python runtime performs internal microbatching/device execution.
4. Rust validates count + 384 dims and stores vectors.

### Incremental flow

- `reembed_symbols_for_file` uses the same sidecar batch method with small payloads.
- Existing stale-vector deletion semantics remain unchanged.

### Query flow

- `run_semantic_search` calls `embed_query`.
- Sidecar returns one vector for KNN lookup.
- Existing hybrid merge/scoring logic remains unchanged.

## Packaging and Runtime Management

### Managed venv strategy

- Julie owns a versioned venv under a local cache path.
- Dependency set is pinned by sidecar version (to ensure reproducibility).
- Bootstrap is idempotent and safe to rerun.

### Process supervision

- Rust maintains a sidecar supervisor with:
  - liveness checks
  - restart policy with bounded retry budget
  - explicit unhealthy state when retry budget is exceeded

### Configuration contract

- Extend provider value set to include sidecar:
  - `JULIE_EMBEDDING_PROVIDER=auto|sidecar|ort|candle`
- Keep strict mode behavior:
  - `JULIE_EMBEDDING_STRICT_ACCEL=0|1`
- Add sidecar tuning knobs (defaults chosen for stability):
  - model id
  - query/batch timeout
  - max batch size
  - optional Python executable override

## Error Handling and Fallback Semantics

### Failure classes

1. **Bootstrap failure:** venv/pip install/setup errors.
2. **Startup failure:** sidecar process/model init failure.
3. **Protocol failure:** malformed/invalid response.
4. **Runtime failure:** timeout, device failure, OOM, repeated crashes.

### Behavior

- Any sidecar failure degrades to fallback provider resolution where possible.
- Degraded reason is persisted in embedding runtime status and surfaced via workspace diagnostics.
- Embedding failures remain non-fatal to keyword search.

### Strict acceleration mode

- If strict acceleration is enabled and sidecar cannot run accelerated, embeddings are disabled rather than silently degraded.

## Testing Strategy (TDD)

### Rust unit tests

- Provider resolution matrix with sidecar as `auto` default.
- Fallback behavior for startup/protocol/runtime failures.
- Timeout and retry-policy tests.
- Response validation tests (count mismatch, dim mismatch).

### Python unit tests

- Protocol handler correctness.
- Device selection behavior.
- Shape/count contract enforcement.
- Microbatch behavior for large batch requests.

### Integration tests

- End-to-end embedding pipeline with live sidecar.
- Incremental re-embed path with sidecar.
- Semantic query embedding path with sidecar.
- Crash/timeout injection proving fallback continuity.

### Performance validation (primary KPI)

- Compare indexing throughput before/after sidecar default.
- Measure incremental re-embed latency.
- Ensure query embedding latency does not regress unacceptably.

## Rollout Plan

1. Ship sidecar as default in `auto` resolution.
2. Preserve explicit override to `ort`/`candle` as escape hatch.
3. Track runtime status and degraded events in health/stats.
4. Evaluate indexing throughput improvement against baseline.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Python bootstrap fails on user machines | Sidecar unavailable | Automatic fallback to existing providers + clear degraded reason |
| Serialization/IPC overhead offsets gains | Lower-than-expected speedup | Binary-framed protocol + batch/microbatch tuning |
| Device support variance across platforms | Inconsistent acceleration | Health metadata visibility + strict mode + fallback |
| Sidecar crash loops | Unstable embeddings | Supervisor retry budget and circuit-break to fallback |
| Model change accidentally breaks 384 contract | Storage/query incompatibility | Hard dimension guard at sidecar init and response validation |

## Exit Criteria

- `auto` selects sidecar by default.
- Sidecar-backed indexing shows measurable throughput improvement.
- Existing sqlite-vec schema and semantic search behavior remain compatible.
- Fallback to in-process providers works automatically.
- Health/stats clearly show sidecar runtime and degraded state.
