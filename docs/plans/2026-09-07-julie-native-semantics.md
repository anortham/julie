# Julie native semantics implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when delegation is available. Fall back to razorback:executing-plans for tightly sequential execution. This document authorizes a handoff plan, not a release. Root Codex reviews the completed implementation.

**Goal:** Make julie-semantic-sidecar a fully usable, measurable alternative to the Python provider without removing conceptual retrieval or silently degrading full-mode operation.

**Architecture:** Preserve the common EmbeddingProvider boundary, add a native broker client, and make vector compatibility depend on complete encoder identity. Use the workspace runtime and semantic readiness contract from the request-engine/lifecycle plans. Keep the Python provider as the comparison baseline until the evaluation plan supports promotion.

**Tech Stack:** Rust, Tokio, SQLite/sqlite-vec, existing sidecar JSON-lines protocol, Unix local sockets and Windows named pipes, native BGE/Qwen models.

**Architecture Quality:** Risk high. Native process acquisition and reconnect belong in julie-pipeline; encoder identity and vector metadata belong in julie-core; request readiness remains in src/request_engine. No new global workspace daemon, no inference implementation in Julie, and no synchronous broker wait on a Tokio executor thread. Public behavior is tested through the real provider, request engine and CLI; fake engines are transport fixtures only.

## Global Constraints

- Prerequisites: reviewed extractor migration, request-engine/MCP/CLI plan and workspace-lifecycle plan. Read their final contracts before starting. Their new paths below are planned prerequisites, not baseline files.
- Planning baseline: Julie `0432158c`; sidecar main `9ed082b`. The sidecar fixes `35c8f13` and `5c41f1d` were in separate clean worktrees. The coordinator must review/reconcile them and record the resulting source SHA and binary SHA-256 before acceptance. Never consume a dirty worktree binary as a release artifact.
- Preserve Python `sidecar` selection and lexical operation. Add `native` selection; do not silently switch the default model. No removal of semantics or existing language coverage.
- New configuration: `JULIE_EMBEDDING_PROVIDER=native`, `JULIE_NATIVE_SIDECAR_PROGRAM` as an explicit executable path, and `JULIE_NATIVE_SIDECAR_MODEL` as a manifest ID. Existing `auto` behavior remains until an explicit promotion decision.
- Supported native IDs at the inspected baseline: `bge-small-en-v1.5-f32` with 384 dimensions and `qwen3-0.6b-f16` served at 512 dimensions. CodeRankEmbed is not supported by that sidecar. Do not relabel models as equivalent.
- `Off` semantic mode performs zero provider acquisition, model preparation, inference or vector query work. `Required` never returns keyword-only results disguised as full functionality.
- New implementation files at most 500 lines, test files at most 1000. Split touched oversized modules by responsibility; no test narration comments. Tests live in each crate's src/tests or Julie src/tests, fixture data in fixtures.
- No pushes, releases, destructive cleanup or spending money without owner approval. Preserve unrelated worktrees. Downloading public model artifacts is an explicit prepare operation, not a side effect of lexical queries.
- Continuous testing is excluded. All test execution below is ordinary worker/lead verification.

## Grounded locations and interface inputs

- `crates/julie-core/src/embeddings_contract.rs`: synchronous Send+Sync EmbeddingProvider; DeviceInfo and runtime status.
- `crates/julie-core/src/database/vectors.rs`: store_embeddings, knn_search, get_embedding_config and vector recreation; `database/migrations.rs` dispatches migrations.
- `crates/julie-pipeline/src/embeddings/{factory.rs,init.rs,sidecar_protocol.rs,sidecar_provider.rs,pipeline.rs}`: selection, launch, protocol, recovery and rebuild.
- `crates/julie-index/src/search/{hybrid.rs,similarity.rs}` and `crates/julie-tools/src/{search/execution.rs,deep_dive/data.rs,navigation/fast_refs.rs}`: all semantic readers must share compatibility checks.
- Existing fake-sidecar tests: `src/tests/core/embedding_sidecar_provider.rs`; protocol tests: `crates/julie-pipeline/src/tests/embedding_sidecar_protocol.rs`; database fixture patterns: `crates/julie-core/src/tests/database/embeddings.rs`.
- Native source contracts: sibling `julie-semantic-sidecar/src/{main.rs,protocol.rs,health.rs,manifest.rs,broker/mod.rs}`. CLI is `prepare --model ID` and `broker --model ID --endpoint PATH --lock PATH --accelerator-lock PATH`. Do not invent broker flags.
- Prerequisite runtime provides WorkspaceBinding, request deadline/cancellation, SemanticMode::{Off,Auto,Required}, SemanticRuntime::ensure_ready and readiness metadata. Extend Ready to carry encoder identity, vector generation and eligible/embedded counts.

## Verification Strategy

**Project source of truth:** AGENTS.md, docs/TESTING_GUIDE.md and xtask/test_tiers.toml. Use `cargo check` before GREEN test runs. Respect package owners after the crate split.

**Worker red/green scope:** Each task below names one exact test in the owning package. Two runs per normal cycle, RED then GREEN; no concurrent test commands. Additional failure diagnosis is reported, not hidden by weakening assertions.

**Worker ceiling:** Exact named tests only. Fake-broker cases must use bounded process waits and isolated roots; no model downloads in ordinary tests.

**Lead affected-change scope:** `cargo xtask test changed`; if OverBudget, explicitly record why `cargo xtask test changed --scale` is used. Never interpret OverBudget as a passing gate.

**Branch gate:** Sequential `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo xtask test dev`, `cargo xtask test system`, `cargo xtask test dogfood`. Reuse matching scope/SHA evidence only on an unchanged tree. Run once per completed branch, not per task.

**Security scope:** none declared as a separate repository tier. Acceptance still rejects executable-shell interpolation, unverified downloads, world-readable private endpoints and malformed unbounded protocol frames.

**Replay/metric evidence:** Wrong encoder, stale generation, false readiness, wrong workspace, unbounded shutdown and broken lexical isolation are hard failures. Throughput/RSS are report-only until evaluation thresholds are frozen. CPU qualification is required; unsupported physical GPU lanes are explicitly unverified, not passed.

**Escalation triggers:** Database changes require database coverage; reader/ranking changes require dogfood; lifecycle changes require system and real multi-process checks. Native package changes require its own repository verification by that owner, not an unreviewed patch hidden in this plan.

**Assigned verification failure:** Diagnose within the task; report a contract mismatch rather than replacing the gate. Record failed evidence too.

**Verification ledger:** Use the empty table at the end. Record the tested tree diff hash as extra evidence when work is uncommitted; never reuse HEAD-only evidence after edits.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| N1 Encoder identity | None - serial | embeddings_contract.rs; new core embeddings_identity.rs and tests/embeddings_identity.rs; core lib/tests registration; pipeline sidecar_protocol.rs,sidecar_provider.rs,rpc_client.rs,host_server.rs,pipeline.rs,metadata.rs and protocol tests; src/registry/embedding_service.rs; index search/hybrid.rs; Python runtime.py and new model_identity.py; coordinator's exact provider/consumer inventory | Yes | Establishes reader/provider and per-call budget contract atomically |
| N2 Native client | None - serial | new pipeline embeddings/native/{mod,client,launch,health}.rs; embeddings/mod.rs,factory.rs,init.rs; pipeline tests/native_provider.rs and test registration; fixtures/embeddings/native_broker/ | Yes | Requires N1 and owns provider selection |
| N3 Compatible generations | None - serial | core database/vectors.rs,migrations.rs and new database/embedding_generation.rs; core database tests/embedding_generation.rs; pipeline embeddings/pipeline.rs; index search/hybrid.rs,similarity.rs; tools semantic readers | Yes | Requires stable N1/N2 identity and provider |
| N4 Runtime recovery | None - serial | prerequisite request_engine/semantic.rs; src/handler/embedding_init.rs; src/server_in_process.rs; src/tools/workspace/indexing/embeddings.rs; runtime watcher binding; src/tests/integration/native_semantic_lifecycle.rs | Yes | Requires N3 generation semantics and lifecycle plan |
| N5 Qualification and operations | None - serial | new docs/SEMANTIC_PROVIDERS.md; new src/request_engine/semantic_qualification.rs and module registration; src/tests/integration/native_semantic_acceptance.rs and integration module registration; docs/plans/native-semantics-verification.md; native launch path from N2 if qualification exposes a defect | Yes | Qualifies completed behavior; no parallel provider mutations |

Commit mode for all tasks: serial-worker-commit on the assigned implementation branch after narrow verification and self-review. Checkpoint before commits. Root Codex can require follow-up fixes before accepting the branch. Do not commit in main or publish a release.

### N1: Carry complete encoder identity without breaking Python

**Files/ownership:** N1 row above. **Interfaces:** consumes provider health, produces proposed EncoderIdentity and `EmbeddingProvider::encoder_identity() -> anyhow::Result<EncoderIdentity>`. **Contract inputs:** model checksum/revision, served dimensions, pooling, normalization, instruction policy, embedding text format and relevant runtime build identity. **Serialization required:** Yes; all later tasks consume identity.

**Step 1, RED:** Add this exact pure identity test in the new core test module. The constructor and type below are proposed additions, not existing APIs.

```rust
#[test]
fn encoder_identity_changes_when_instruction_policy_changes() {
    let first = EncoderIdentity {
        schema: 1,
        model_id: "bge-small-en-v1.5-f32".into(),
        weights_sha256: "a".repeat(64),
        dimensions: 384,
        pooling: "cls".into(),
        normalization: "l2".into(),
        instruction_policy: "v1".into(),
        text_format: 1,
        runtime_build: "fixture-runtime".into(),
    };
    let mut second = first.clone();
    second.instruction_policy = "v2".into();
    assert_ne!(first, second);
    assert_ne!(first.storage_key().unwrap(), second.storage_key().unwrap());
}
```

**Step 2:** `cargo nextest run -p julie-core --lib encoder_identity_changes_when_instruction_policy_changes`. Expect missing type/method before implementation; do not accept a zero-test run.

**Step 3, implementation:** Add a serde-serializable, Eq identity with the fields used above. `storage_key` uses a canonical field-order serialization and SHA-256, never DefaultHasher. Validate nonempty identifiers, 64-character hex checksum, positive supported dimensions and known normalization. Add protocol health fields with serde defaults for old Python replies; native readiness requires complete identity. Read native actual `model_sha256`, `pooling`, `normalization`, `instruction_policy_version`, `llama_cpp_build`, not inferred values. Extend device capabilities for Metal/Vulkan.

Python remains functional. Extend its health producer in `python/embeddings_sidecar/sidecar/runtime.py` using a new `model_identity.py` helper that hashes the resolved immutable model/config/tokenizer snapshot once at preparation. Cache the verified manifest and invalidate it when model files change; do not derive weights identity from model name or installation path. An old Python reply with no trustworthy digest is explicitly `IdentityUnavailable`, never an EncoderIdentity with a fabricated checksum. It may continue the pre-migration legacy path during N1/N2 only; N3 requires the updated Python producer for persistent generation reuse and full-mode acceptance. Failure to locate weights is a visible prerequisite failure for full-mode qualification, not permission to drop Python. This keeps the transition compiling without declaring unverified identities compatible.

Before changing the trait, the coordinator closes an exact reference inventory for all providers and consumers. N1 owns `sidecar_provider.rs`, `rpc_client.rs`, `host_server.rs`, `src/registry/embedding_service.rs`, their exact test/fake-provider files, `crates/julie-index/src/search/hybrid.rs`, `crates/julie-pipeline/src/embeddings/{pipeline.rs,metadata.rs}`, and the request/workspace adapters that call them, in addition to the table row. Add a required `EmbeddingRequestBudget { deadline: std::time::Instant, cancelled: Arc<AtomicBool> }` argument to `embed_query` and `embed_batch` in one atomic compiling slice. Every real provider observes it on connection, lock admission, read/write and batch boundaries; no mutable process-global deadline. Update the Python transport and resident-host RPC bridge to carry a remaining budget explicitly and clamp it on the receiving side, using a synchronized host protocol revision with no external native-v1 shape change. The native broker wire has no cancellation field: the native client enforces deadlines, drops timed-out connections and discards late results. Update all fake providers/call sites with the same signature. Work already in flight may finish inside the broker, but cannot publish stale generations or block the cancelled caller. Use `cargo check --workspace --all-targets` to prove this migration, not only core/pipeline checks.

Minimum implementation shape:

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct EncoderIdentity {
    pub schema: u32,
    pub model_id: String,
    pub weights_sha256: String,
    pub dimensions: usize,
    pub pooling: String,
    pub normalization: String,
    pub instruction_policy: String,
    pub text_format: u32,
    pub runtime_build: String,
}
```

Add negative tests for same dimensions/different weights and missing native provenance. Never attach native defaults to a Python reply. N1 also owns new `python/embeddings_sidecar/tests/test_model_identity.py`: use a stdlib unittest fixture with a temporary snapshot directory containing a weights file and tokenizer config, call the actual new snapshot fingerprint helper, replace weights bytes while keeping the model ID unchanged, and assert the resulting fingerprint changes. Check unchanged content stays stable and missing required files returns IdentityUnavailable. Register the exact test method `test_model_identity_changes_after_weights_replaced`; RED/GREEN command from the repository root is `PYTHONPATH=python/embeddings_sidecar python3 -m unittest discover -s python/embeddings_sidecar/tests -p test_model_identity.py -k model_identity_changes_after_weights_replaced`. Set that environment variable through the native process API on Windows. Use stdlib unittest here so identity tests need neither PyTorch nor model downloads; ordinary existing Python tests remain untouched.

**Step 4:** `cargo check --workspace --all-targets`, then the exact RED command. Expected one test PASS.

**Step 5:** Apply commit mode. **Acceptance:**
- [ ] Every encoder-changing field changes compatibility; harmless request IDs/device telemetry do not.
- [ ] Native health round trips all identity/capability fields; legacy provider still works explicitly.
- [ ] No fake-provider compilation failures; no raw secret/model file content in logs.

### N2: Acquire and use the native shared broker

**Files/ownership:** N2 row. **Interfaces:** produces NativeEmbeddingProvider implementing the existing trait plus N1 identity, NativeLaunchConfig and bounded broker RPC. **Contract inputs:** native v1 RequestEnvelope/ResponseEnvelope from sidecar_protocol.rs; native CLI shown above. **Serialization required:** Yes; provider selection follows N1.

**Step 1, RED:** Define a fixture broker program under fixtures/embeddings/native_broker that speaks the real JSON-lines envelope and records request methods to a temp file. It must support health, embed_query, embed_batch, delayed reply, wrong request_id, bad dimensions, EOF and malformed frames. Test vectors are fixture-only, never a product fallback. New proposed pure reply decoder has this anchor test:

```rust
#[test]
fn native_reply_rejects_mismatched_request_id() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"other","result":{"dims":2,"vector":[0.1,0.2]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "wanted", 2);
    assert!(result.is_err());
}
```

**Step 2:** `cargo nextest run -p julie-pipeline --lib native_reply_rejects_mismatched_request_id`. Expect missing decoder initially.

**Step 3, implementation:** Use typed ResponseEnvelope<EmbedQueryResult>; require matching schema/version/request_id, exactly one result/error, expected dimensions/count and finite floats. Enforce broker-advertised max_request_bytes/max_batch_items with a local hard cap before allocation. Client does not add query instructions again: sidecar owns model instructions. Use source input policy and upstream token limit; record truncation and never silently drop a batch item.

Launch executable with argv, never a shell command string. `prepare` is explicit and bounded; missing executable/model reports `NATIVE_SIDECAR_MISSING`/`MODEL_NOT_PREPARED`. Derive the broker acquisition key from inputs known before connection: canonical cache root, pinned executable SHA-256 and manifest model ID. After connecting, validate full N1 encoder identity from health before exposing the provider or using vectors; never need broker health to discover its own endpoint. Use the sidecar's service lock and accelerator lock contracts; losing a broker-start race reconnects without deleting the winner's endpoint. Keep owner stdin behavior explicit, and reconnect surviving clients if the owner exits. Windows named pipe setup must match native transport, not Unix path emulation.

Each synchronous embed call runs outside async executor threads. Bound connect/read/write by the caller's remaining deadline. Serialize calls per connection or route replies by unique request IDs; never share an uncorrelated reader. On timeout drop the connection and discard late replies; reconnect on a subsequent bounded request. Retry a lost read-only embed once within its deadline, never indefinitely. Shutdown releases only client resources; it must not kill a broker shared with another workspace/session. Do not introduce a second global semaphore that holds indexing admission while waiting for inference.

**Step 4:** `cargo check -p julie-pipeline`, then exact RED command. Lead also runs fixture cases for malformed frame, timeout, restart and two clients sharing one broker using the new module's exact tests.

**Step 5:** Apply commit mode. **Acceptance:**
- [ ] Native selected explicitly; Python and auto defaults preserved.
- [ ] Correct vectors/order/dimensions through real protocol, bounded failure paths, no shell interpolation.
- [ ] Two processes share a broker; loser cannot unlink the winner; no lexical-triggered downloads.
- [ ] Linux and Windows transport behavior has native-platform evidence before claiming support.

### N3: Prevent mixed-encoder and stale-vector reads

**Files/ownership:** N3 row. **Interfaces:** proposed EmbeddingGeneration metadata with encoder_key, source_revision, status and counts; `begin_embedding_generation`, `publish_embedding_generation` and generation-checked semantic reads on SymbolDatabase. **Contract inputs:** N1 identity, canonical workspace revision from lifecycle plan. **Serialization required:** Yes; every reader must migrate together.

**Step 1, RED:** Add a database regression in the existing SymbolDatabase fixture style. This anchor uses newly proposed metadata methods and requires no real model:

```rust
#[test]
fn embedding_generation_is_unreadable_until_published() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();
    let generation = db.begin_embedding_generation("encoder-a", 12, 384).unwrap();
    assert!(!db.embedding_generation_ready("encoder-a", 12).unwrap());
    db.publish_embedding_generation(generation, 12, 0, 0).unwrap();
    assert!(db.embedding_generation_ready("encoder-a", 12).unwrap());
    assert!(!db.embedding_generation_ready("encoder-b", 12).unwrap());
}
```

**Step 2:** `cargo nextest run -p julie-core --lib embedding_generation_is_unreadable_until_published`. Expect missing generation API.

**Step 3, implementation:** Add an idempotent numbered SQLite migration using the next free number at execution. Metadata belongs in a separate focused module. Preserve canonical symbols/files; replacing derived vectors never deletes source facts. At generation start, atomically mark building and invalidate old compatibility before recreating vectors. Persist each batch with the generation identifier; refuse writes from a cancelled/old generation. Publish ready only when counts reconcile and the canonical revision still matches. If source advances, resume/reconcile dirty files; do not stamp the newer revision on old vectors.

Choose the simpler fail-closed rebuild: lexical stays available; semantic readers return not-ready while replacing vectors. Do not hold a database transaction or mutation guard during inference. Acquire a read transaction that checks identity/status and runs KNN against that same generation; close it promptly. All readers, including deep_dive related symbols and fast_refs fallback, use this path. A same-dimensional encoder change is incompatible. Process death while building leaves metadata non-ready; restart repairs it. Partial coverage may be reported in Auto, but Required refuses it for whole-workspace semantic search. A zero-eligible-symbol workspace is a valid empty index, not a failed embed job.

Implement exact tests for old-generation late batch rejection, same-dimensional model switch, source revision advancing during embed, crash/reopen and unchanged lexical results. Those invariants are mandatory even though only the smallest anchor is printed here.

**Step 4:** `cargo check --workspace --all-targets`, then exact RED command. Lead runs the affected database/index/pipeline scopes before accepting this atomic reader/writer migration.

**Step 5:** Apply commit mode. **Acceptance:**
- [ ] No consumer can compare vectors across identities or read a half-built generation as ready.
- [ ] Cancellation/restart retains source facts and recovers coverage; no database lock spans inference.
- [ ] Watcher updates invalidate/rebuild relevant vectors and report honest freshness.

### N4: Attach readiness and recover within the same runtime

**Files/ownership:** N4 row. **Interfaces:** consumes prerequisite SemanticRuntime::ensure_ready with binding, mode, deadline and cancellation; returns extended semantic metadata and compatible generation. **Serialization required:** Yes; depends on N3 and reviewed lifecycle implementation.

**Step 1, RED:** Add a subprocess fixture scenario `native_semantics_becomes_ready_without_client_restart`. Start Julie with a delayed fixture broker, make a lexical request, observe Starting in Auto, release the broker via a test barrier, populate a compatible generation, and issue Required in the same process. Required must succeed without a new initialized notification. The scenario uses the request engine's real structured CLI request format; do not invent a second test-only transport.

Add this pure mode anchor to the new root integration module:

```rust
#[test]
fn semantic_off_requires_no_provider() {
    assert!(!semantic_mode_needs_provider(SemanticMode::Off));
    assert!(semantic_mode_needs_provider(SemanticMode::Required));
}
```

**Step 2:** `cargo nextest run --lib native_semantics_becomes_ready_without_client_restart`. Expected behavioral failure before replacing startup-only attachment. Register the integration module in src/tests/integration/mod.rs.

**Step 3, implementation:** Replace overlapping startup/lazy-init decisions with one runtime-owned provider state. Keep the existing Python adapter behind the same state machine. Proposed phases: Disabled, Starting, Ready and Degraded with retryable reason. Shared provider acquisition is single-flight per encoder, not per request. An abandoned caller cancels its wait, not other callers' shared initialization. Retryable failure can transition to Starting on bounded backoff and then Ready; permanent model/protocol mismatch remains degraded until configuration changes. Never permanently lose the successful future after a startup timeout.

```rust
pub fn semantic_mode_needs_provider(mode: SemanticMode) -> bool {
    !matches!(mode, SemanticMode::Off)
}
```

Required combines provider readiness with N3 compatibility/coverage for the requested operation. Health surfaces requested/resolved backend, actual acceleration, identity, eligible/embedded counts, generation and error reason. Use machine limits without retaining an index-job permit while waiting on the model. Preserve suggestion labels: semantic similarity is never an exact reference.

**Step 4:** `cargo check`, then exact RED command. Lead verifies broker death/reconnect, two workspaces with distinct identities, full-mode failure at deadline and zero calls to the fake broker in Off mode.

**Step 5:** Apply commit mode. **Acceptance:**
- [ ] Same-process late attachment and recovery work without MCP/session restart.
- [ ] Off emits no model work; Required never silently returns lexical-only; Auto reports degradation.
- [ ] Shutdown and cancelled requests finish within their configured deadlines, without harming other clients.

### N5: Qualify native operation and retain a reversible selection

**Files/ownership:** N5 row. **Interfaces:** operational documentation and machine-readable qualification consumed by evaluation plan. **Contract inputs:** reconciled sidecar source/binary identities and N1-N4 implementation. **Serialization required:** Yes; runs after full integration.

**Step 1, RED:** Add `native_semantic_qualification_rejects_unverified_artifact` to the root acceptance module. The proposed qualification validator rejects a record missing binary checksum, encoder identity, actual backend, corpus commit or vector coverage. A record cannot claim native performance when health resolved Python or CPU contrary to a required accelerator policy.

```rust
#[test]
fn native_semantic_qualification_rejects_unverified_artifact() {
    let value = serde_json::json!({"schema": "julie-native-qualification-v1"});
    assert!(validate_native_qualification(&value).is_err());
}
```

**Step 2:** `cargo nextest run --lib native_semantic_qualification_rejects_unverified_artifact`. Expect missing validator before implementation.

**Step 3, implementation:** Implement `validate_native_qualification` in the owned request-engine semantic_qualification module and expose its validated record through workspace health structured output. The evaluator consumes that production record; this is not a test-only validator. Required fields are schema, Julie/sidecar source SHA, executable SHA-256, EncoderIdentity, actual backend/device, corpus commit, canonical/vector revisions, eligible/embedded counts and qualification timestamp. Validate field types, digest formats, revision equality and complete eligible coverage before emitting qualified=true. Document executable/config discovery, explicit prepare, lexical/auto/required behavior, model switching and recovery. Keep qualification output outside indexed corpora. Capture source/binary/model hashes, actual device/backend, request counts, vector coverage and task result links; do not copy historical 60998de results as current proof. Run a small real CPU corpus with BGE and Qwen using prepared public files, then a Python baseline in separate index roots. Record prepare/startup/query/shutdown times separately and whole process-tree peak RSS. These measurements do not change defaults.

Do not automatically merge/release the sidecar worktree fixes. The coordinator supplies their reviewed artifact. If native cannot reproduce required hardware/model behavior, preserve selectable Python and report that exact lane; never claim the Python removal complete. Native integration can be accepted with explicit CPU qualification while optional physical GPU lanes remain unverified. Default promotion/removal of Python is a separate owner decision after the evaluation plan.

**Step 4:** `cargo check`, then exact RED command; run lead branch gates sequentially and append fresh qualification evidence.

**Step 5:** Apply commit mode. **Acceptance:**
- [ ] Working selectable native provider, real CPU semantic queries, current artifact evidence and documented unsupported lanes.
- [ ] Python comparison path retained and model/default unchanged absent approval.
- [ ] Root Codex receives commit range, diff summary, ledger, raw process traces and remaining limitations.

## Final review and handoff

Root Codex reviews all tasks against provider identity, cancellation, cross-process sharing, all semantic readers and full-mode truthfulness. Workers must report repository/worktree/branch/HEAD, dirty state, upstream binary identity, all RED/GREEN results and lead gate evidence. Findings are work to fix; a worker's passing report is not final acceptance. No release action is included.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
