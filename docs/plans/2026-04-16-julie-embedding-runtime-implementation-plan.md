# Julie Embedding Runtime Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use @razorback:executing-plans to implement this plan task-by-task.

**Goal:** Make embedding startup, backend capability detection, degraded mode, and query-path fallback explicit and observable across daemon mode, stdio mode, and supported platforms.

**Architecture:** Keep the daemon embedding service as the runtime gate, separate capability reporting from model load and fallback policy, and route indexing plus query paths through settled runtime states rather than silent side effects. Surface runtime truth through the shared health and dashboard layers.

**Tech Stack:** Rust, Tokio, Python sidecar, sentence-transformers, PyTorch, DirectML, CUDA, MPS

---

**Execution rules:** Use @razorback:test-driven-development and @razorback:systematic-debugging. Start from protocol and health contracts, then move through sidecar runtime, daemon service, and query-path fallback.

### Task 1: Normalize Embedding Runtime Status And Capability Reporting

**Files:**
- Modify: `src/embeddings/mod.rs:32-140`
- Modify: `src/embeddings/factory.rs:11-146`
- Modify: `src/embeddings/init.rs:24-201`
- Modify: `src/daemon/embedding_service.rs:37-300`
- Test: `src/tests/daemon/embedding_service.rs`
- Test: `src/tests/core/embedding_sidecar_protocol.rs`

**What to build:** Expand `EmbeddingRuntimeStatus`, backend capability reporting, and `EmbeddingService` settled states until Julie can say which backend was requested, which backend was chosen, whether acceleration exists, and why degraded mode was selected.

**Approach:** Keep `src/embeddings/mod.rs` as the contract surface and feed the richer status through the existing factory and init code. Ensure `EmbeddingService` reports settled states that indexing and query paths can consume without guessing.

**Acceptance criteria:**
- [ ] runtime status captures requested backend, resolved backend, acceleration, and degraded reason
- [ ] `EmbeddingService` exposes settled states that indexing and search paths can consume directly
- [ ] protocol and embedding-service tests pin ready, unavailable, and timed-out states
- [ ] daemon and stdio paths share the same runtime status model

### Task 2: Separate Sidecar Capability Probe From Runtime Load Policy

**Files:**
- Modify: `python/embeddings_sidecar/sidecar/protocol.py`
- Modify: `python/embeddings_sidecar/sidecar/main.py`
- Modify: `python/embeddings_sidecar/sidecar/runtime.py:240-528`
- Test: `src/tests/core/embedding_sidecar_protocol.rs`
- Test: `src/tests/integration/sidecar_embedding_pipeline.rs`

**What to build:** Split low-level backend capability and health reporting from model load, probe-encode, and fallback policy. Julie should be able to ask the sidecar what it can do and why it degraded, not infer that from stderr.

**Approach:** Keep the current protocol schema shape where possible, then enrich the health payload and runtime metadata returned by the sidecar. Preserve DirectML, CUDA, MPS, and CPU support while making their fallback paths structured and testable.

**Acceptance criteria:**
- [ ] sidecar health responses expose backend capability and degradation data explicitly
- [ ] runtime load path no longer hides backend fallback decisions inside opaque stderr-only messages
- [ ] integration tests cover successful health probe and degraded runtime cases
- [ ] protocol validation tests pin new fields and reject inconsistent responses

### Task 3: Make Query And Indexing Paths Honor Settled Embedding States

**Files:**
- Modify: `src/tools/workspace/indexing/embeddings.rs:23-264`
- Modify: `src/tools/search/nl_embeddings.rs:41-331`
- Modify: `src/handler.rs:1241-1257`
- Test: `src/tests/tools/workspace/index_embedding_tests.rs`
- Test: `src/tests/integration/sidecar_embedding_pipeline.rs`
- Test: `src/tests/daemon/embedding_service.rs`

**What to build:** Route indexing and NL-definition query paths through explicit settled embedding states so daemon mode does not spawn surprise stdio fallback providers and indexing does not skip embeddings without recording why.

**Approach:** Reuse the richer `EmbeddingServiceSettled` model from Task 1. Make daemon mode wait for settled states within bounded policy, then expose clean degraded behavior when the service is unavailable or timed out.

**Acceptance criteria:**
- [ ] workspace embedding waits for settled daemon states or records a bounded degraded outcome
- [ ] NL definition search does not start a second provider when daemon mode is still settling
- [ ] handler helpers expose the same runtime state to both indexing and query paths
- [ ] tests cover ready, unavailable, and timeout cases for daemon-mode embedding use

### Task 4: Surface Embedding Degradation Through Health And Dashboard

**Files:**
- Modify: `src/health.rs:13-242`
- Modify: `src/dashboard/state.rs:41-164`
- Modify: `src/dashboard/routes/status.rs:11-87`
- Modify: `src/tools/workspace/commands/registry/health.rs:11-262`
- Test: `src/tests/dashboard/state.rs`
- Test: `src/tests/tools/workspace/mod_tests.rs`

**What to build:** Show embedding mode, backend, device, acceleration status, degraded reason, and query fallback state through the shared health command and dashboard.

**Approach:** Reuse the runtime status contract and avoid dashboard-only logic. If embeddings are unavailable, degraded, or rebuilding, the dashboard and health command should say so in the same words the runtime uses.

**Acceptance criteria:**
- [ ] dashboard shows embedding availability, backend, device, acceleration, and degraded reason
- [ ] health command reports the same embedding status fields as the dashboard
- [ ] embedding degradation is visible without tailing logs
- [ ] tests pin healthy and degraded embedding states through the shared health surface

