# Post-Review Fixes and Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the high/medium risks found in the 24-hour review (sidecar reliability, deferred init locking, extractor correctness, and docs drift) while preserving current behavior where intentional.

**Architecture:** Apply strict TDD for each risk: add a focused failing regression test, implement the smallest safe fix, then re-run targeted suites. Keep fixes localized to sidecar IPC/runtime, workspace initialization flow, and relationship extractors; avoid broad refactors unless required by tests.

**Tech Stack:** Rust (Tokio, anyhow, tree-sitter), Python (pytest, sidecar protocol/runtime), Cargo test suites, repo docs in Markdown.

---

## Phase 1: Sidecar IPC and Runtime Robustness

### Task 1: Make sidecar timeout/protocol errors connection-fatal

**Files:**
- Modify: `src/tests/core/embedding_sidecar_provider.rs`
- Modify: `src/embeddings/sidecar_provider.rs`

**Step 1: Write failing tests**

Add two tests in `src/tests/core/embedding_sidecar_provider.rs`:

```rust
#[test]
fn test_sidecar_provider_timeout_forces_process_reset() {
    // script mode: first embed_query sleeps longer than timeout,
    // second embed_query should succeed after provider restarts process.
}

#[test]
fn test_sidecar_provider_request_id_mismatch_forces_process_reset() {
    // script mode: first response uses wrong request_id,
    // next request must recover instead of staying desynced.
}
```

**Step 2: Run tests to verify failure**

Run: `cargo test --lib test_sidecar_provider_timeout_forces_process_reset test_sidecar_provider_request_id_mismatch_forces_process_reset 2>&1 | tail -20`
Expected: FAIL with timeout / request-id mismatch followed by non-recovering behavior.

**Step 3: Implement minimal fix**

In `src/embeddings/sidecar_provider.rs`, update `send_request_with_timeout` to treat timeout, stream decode failures, and response envelope mismatches as fatal for current process:

```rust
// pseudo-shape
match recv_result {
    Err(RecvTimeoutError::Timeout) => {
        self.terminate();
        bail!("timed out ... process terminated for recovery");
    }
    // ...
}

let envelope: ResponseEnvelope<Resp> = serde_json::from_str(...)
    .inspect_err(|_| self.terminate())?;
validate_response_envelope(&envelope, &request_id)
    .inspect_err(|_| self.terminate())?;
```

**Step 4: Run tests to verify pass**

Run: `cargo test --lib test_sidecar_provider_timeout_forces_process_reset test_sidecar_provider_request_id_mismatch_forces_process_reset 2>&1 | tail -20`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/embeddings/sidecar_provider.rs src/tests/core/embedding_sidecar_provider.rs
git commit -m "fix: reset sidecar process after timeout and protocol desync"
```

### Task 2: Validate request schema/version in Python sidecar protocol

**Files:**
- Modify: `python/embeddings_sidecar/tests/test_protocol.py`
- Modify: `python/embeddings_sidecar/sidecar/protocol.py`

**Step 1: Write failing tests**

Add protocol tests:

```python
def test_dispatch_request_rejects_wrong_schema():
    req = {"schema": "wrong.schema", "version": 1, "request_id": "r1", "method": "health", "params": {}}
    resp = dispatch_request(runtime, req)
    assert resp["error"]["code"] == "invalid_request"

def test_dispatch_request_rejects_wrong_version():
    req = {"schema": SIDECAR_PROTOCOL_SCHEMA, "version": 999, "request_id": "r1", "method": "health", "params": {}}
    resp = dispatch_request(runtime, req)
    assert resp["error"]["code"] == "invalid_request"
```

**Step 2: Run tests to verify failure**

Run: `python3 -m pytest -q python/embeddings_sidecar/tests/test_protocol.py -k "wrong_schema or wrong_version" --tb=short`
Expected: FAIL because current dispatcher accepts mismatched schema/version.

**Step 3: Implement minimal fix**

In `python/embeddings_sidecar/sidecar/protocol.py`, validate `schema` and `version` before method dispatch and return structured `invalid_request` errors.

**Step 4: Run tests to verify pass**

Run: `python3 -m pytest -q python/embeddings_sidecar/tests/test_protocol.py --tb=short`
Expected: PASS.

**Step 5: Commit**

```bash
git add python/embeddings_sidecar/sidecar/protocol.py python/embeddings_sidecar/tests/test_protocol.py
git commit -m "fix(sidecar): reject incompatible protocol schema/version"
```

### Task 3: Normalize DirectML device telemetry for strict acceleration logic

**Files:**
- Modify: `python/embeddings_sidecar/tests/test_runtime.py`
- Modify: `python/embeddings_sidecar/sidecar/runtime.py`
- Modify: `src/tests/core/embedding_provider.rs`
- Modify: `src/embeddings/mod.rs` (only if normalization done Rust-side)

**Step 1: Write failing tests**

Add/adjust tests to assert DirectML is treated as accelerated even when underlying device text is `privateuseone:0`.

```python
def test_device_selection_normalizes_directml_label():
    rt = build_runtime(..., dml_module=_dml_stub(available=True))
    assert rt.device.startswith("directml")
```

```rust
#[test]
fn test_device_info_acceleration_heuristic_handles_privateuseone_directml() {
    let info = DeviceInfo {
        runtime: "python-sidecar (sentence-transformers directml)".into(),
        device: "privateuseone:0".into(),
        model_name: "x".into(),
        dimensions: 384,
    };
    assert!(info.is_accelerated());
}
```

**Step 2: Run tests to verify failure**

Run: `python3 -m pytest -q python/embeddings_sidecar/tests/test_runtime.py -k directml --tb=short`
Run: `cargo test --lib test_device_info_acceleration_heuristic_handles_privateuseone_directml 2>&1 | tail -20`
Expected: At least one FAIL before fix.

**Step 3: Implement minimal fix**

Preferred: normalize device string in `runtime.py` to `directml`/`directml:0` when DML is selected.
Fallback: expand Rust `is_accelerated()` hints to include `privateuseone` only when runtime hints include sidecar/directml.

**Step 4: Run tests to verify pass**

Re-run the two commands above.
Expected: PASS.

**Step 5: Commit**

```bash
git add python/embeddings_sidecar/sidecar/runtime.py python/embeddings_sidecar/tests/test_runtime.py src/tests/core/embedding_provider.rs src/embeddings/mod.rs
git commit -m "fix: classify DirectML telemetry as accelerated in strict mode"
```

### Task 4: Support true raw sidecar program override mode

**Files:**
- Modify: `src/tests/core/embedding_provider.rs`
- Modify: `src/embeddings/sidecar_supervisor.rs`

**Step 1: Write failing test**

Add a test that sets `JULIE_EMBEDDING_SIDECAR_PROGRAM` to a standalone launcher that expects no implicit `-m` or script args, and assert launch config respects raw mode.

```rust
#[test]
fn test_sidecar_program_override_raw_mode_uses_no_implicit_args() {
    // set env for raw mode, build launch config, assert args.is_empty()
}
```

**Step 2: Run test to verify failure**

Run: `cargo test --lib test_sidecar_program_override_raw_mode_uses_no_implicit_args 2>&1 | tail -20`
Expected: FAIL because current code always appends module/script args.

**Step 3: Implement minimal fix**

In `src/embeddings/sidecar_supervisor.rs`, add explicit raw mode (for example `JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM=1`) that bypasses `launch_args()` and PYTHONPATH injection.

**Step 4: Run test to verify pass**

Run: `cargo test --lib test_sidecar_program_override_raw_mode_uses_no_implicit_args 2>&1 | tail -20`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/embeddings/sidecar_supervisor.rs src/tests/core/embedding_provider.rs
git commit -m "feat(sidecar): add raw program override mode without implicit args"
```

---

## Phase 2: Workspace Initialization and Search Availability

### Task 5: Remove long blocking work from workspace write-lock path

**Files:**
- Modify: `src/tools/workspace/indexing/embeddings.rs`
- Modify: `src/workspace/mod.rs` (optional helper extraction)
- Modify: `src/tests/tools/workspace/mod_tests.rs`

**Step 1: Write failing regression test**

Add an async test that demonstrates read access is blocked while deferred provider init runs, then codifies expected non-blocking behavior.

```rust
#[tokio::test]
async fn test_spawn_workspace_embedding_does_not_hold_write_lock_during_provider_init() {
    // orchestrate init delay; ensure concurrent read lock acquisition completes quickly
}
```

**Step 2: Run test to verify failure**

Run: `cargo test --lib test_spawn_workspace_embedding_does_not_hold_write_lock_during_provider_init 2>&1 | tail -20`
Expected: FAIL (timeout / delayed lock acquisition) with current implementation.

**Step 3: Implement minimal fix**

In `src/tools/workspace/indexing/embeddings.rs`, use a short lock scope for state check/set, then run heavy init in `tokio::task::spawn_blocking` without holding workspace write lock. Re-acquire lock only to publish initialized provider/state.

**Step 4: Run test to verify pass**

Run: `cargo test --lib test_spawn_workspace_embedding_does_not_hold_write_lock_during_provider_init 2>&1 | tail -20`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/tools/workspace/indexing/embeddings.rs src/workspace/mod.rs src/tests/tools/workspace/mod_tests.rs
git commit -m "fix(workspace): avoid holding write lock during deferred embedding init"
```

### Task 6: Ensure NL definition search can trigger lazy embedding availability

**Files:**
- Modify: `src/tools/search/text_search.rs`
- Modify: `src/tests/tools/hybrid_search_tests.rs`
- Modify: `src/tests/core/embedding_provider.rs` (if helper coverage needed)

**Step 1: Write failing test**

Add a test where provider is initially `None`, query is NL-like, and search path initializes embeddings lazily (or schedules it) so hybrid can be used without explicit indexing command.

```rust
#[tokio::test]
async fn test_nl_definition_search_can_enable_hybrid_without_prior_index_embedding() {
    // initialize handler/workspace with deferred provider, run NL definitions query,
    // assert provider status becomes initialized or hybrid path is used after lazy init.
}
```

**Step 2: Run test to verify failure**

Run: `cargo test --lib test_nl_definition_search_can_enable_hybrid_without_prior_index_embedding 2>&1 | tail -20`
Expected: FAIL with current keyword-only fallback behavior.

**Step 3: Implement minimal fix**

In `src/tools/search/text_search.rs`, when `search_target == "definitions"` and query is NL-like and provider is `None`, trigger one-time lazy provider initialization (non-blocking or bounded blocking) before deciding hybrid path.

**Step 4: Run test to verify pass**

Run: `cargo test --lib test_nl_definition_search_can_enable_hybrid_without_prior_index_embedding 2>&1 | tail -20`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/tools/search/text_search.rs src/tests/tools/hybrid_search_tests.rs src/tests/core/embedding_provider.rs
git commit -m "fix(search): allow lazy embedding init from NL definition queries"
```

---

## Phase 3: Extractor Correctness and Cross-Language Consistency

### Task 7: Capture C# top-level DI registrations

**Files:**
- Modify: `crates/julie-extractors/src/tests/csharp/di_registration_relationships.rs`
- Modify: `crates/julie-extractors/src/csharp/di_relationships.rs`

**Step 1: Write failing test**

Add test with top-level statements in `Program.cs` (no containing class):

```rust
#[test]
fn test_di_top_level_program_registration_creates_instantiates_pending() {
    // builder.Services.AddScoped<IService, Service>(); at top-level
    // assert pending/direct Instantiates is emitted from file-level container symbol.
}
```

**Step 2: Run test to verify failure**

Run: `cargo test -p julie-extractors --lib test_di_top_level_program_registration_creates_instantiates_pending 2>&1 | tail -20`
Expected: FAIL because extractor returns early when no containing class.

**Step 3: Implement minimal fix**

In `crates/julie-extractors/src/csharp/di_relationships.rs`, when `find_containing_class` is `None`, map `from_symbol_id` to a file-level owner symbol (existing module/file symbol if present, otherwise create a deterministic synthetic owner path).

**Step 4: Run test to verify pass**

Run: `cargo test -p julie-extractors --lib test_di_top_level_program_registration_creates_instantiates_pending 2>&1 | tail -20`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/csharp/di_relationships.rs crates/julie-extractors/src/tests/csharp/di_registration_relationships.rs
git commit -m "fix(csharp): extract DI relationships from top-level Program statements"
```

### Task 8: Fix C# qualified interface inheritance kind inference

**Files:**
- Modify: `crates/julie-extractors/src/tests/csharp/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/csharp/relationships.rs`

**Step 1: Write failing test**

Add test for `public class A : Abstractions.IService` where target is cross-file.

```rust
#[test]
fn test_cross_file_qualified_interface_name_keeps_implements_kind() {
    // assert pending relationship kind is Implements, not Extends.
}
```

**Step 2: Run test to verify failure**

Run: `cargo test -p julie-extractors --lib test_cross_file_qualified_interface_name_keeps_implements_kind 2>&1 | tail -20`
Expected: FAIL with current `is_interface_name("Abstractions.IService")` heuristic.

**Step 3: Implement minimal fix**

In `crates/julie-extractors/src/csharp/relationships.rs`, parse base type node into terminal identifier before interface/class classification (e.g., `IService` from `Abstractions.IService`).

**Step 4: Run test to verify pass**

Run: `cargo test -p julie-extractors --lib test_cross_file_qualified_interface_name_keeps_implements_kind 2>&1 | tail -20`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/csharp/relationships.rs crates/julie-extractors/src/tests/csharp/cross_file_relationships.rs
git commit -m "fix(csharp): classify qualified interface bases as Implements"
```

### Task 9: Prevent tuple type false positives in C# member type extraction

**Files:**
- Modify: `crates/julie-extractors/src/tests/csharp/field_property_relationships.rs`
- Modify: `crates/julie-extractors/src/csharp/member_type_relationships.rs`

**Step 1: Write failing test**

```rust
#[test]
fn test_tuple_typed_field_does_not_emit_bogus_uses_relationship() {
    // field: (int a, int b) Coordinates;
    // assert no pending/uses for tuple text blob.
}
```

**Step 2: Run test to verify failure**

Run: `cargo test -p julie-extractors --lib test_tuple_typed_field_does_not_emit_bogus_uses_relationship 2>&1 | tail -20`
Expected: FAIL due fallback extracting raw tuple node text.

**Step 3: Implement minimal fix**

In `extract_type_name_from_node`, explicitly handle `tuple_type` as `None` (or structured element extraction if needed), and narrow fallback behavior to avoid arbitrary `*type*` node text.

**Step 4: Run test to verify pass**

Run: `cargo test -p julie-extractors --lib test_tuple_typed_field_does_not_emit_bogus_uses_relationship 2>&1 | tail -20`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/csharp/member_type_relationships.rs crates/julie-extractors/src/tests/csharp/field_property_relationships.rs
git commit -m "fix(csharp): ignore tuple_type in member type relationship extraction"
```

### Task 10: Support qualified heritage names in TypeScript and JavaScript

**Files:**
- Modify: `crates/julie-extractors/src/tests/typescript/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/javascript/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/typescript/relationships.rs`
- Modify: `crates/julie-extractors/src/javascript/relationships.rs`

**Step 1: Write failing tests**

Add coverage for:
- `class A extends Namespace.Base {}`
- `class Impl implements Api.IService {}` (TypeScript)

```rust
#[test]
fn test_cross_file_extends_namespace_base_creates_pending_relationship() {}

#[test]
fn test_cross_file_implements_qualified_interface_creates_pending_implements() {}
```

**Step 2: Run tests to verify failure**

Run: `cargo test -p julie-extractors --lib test_cross_file_extends_namespace_base_creates_pending_relationship test_cross_file_implements_qualified_interface_creates_pending_implements 2>&1 | tail -20`
Expected: FAIL because only direct identifier children are currently collected.

**Step 3: Implement minimal fix**

Traverse heritage clause expression trees and extract rightmost identifier/type_identifier from member expressions / qualified names.

**Step 4: Run tests to verify pass**

Run the same command as Step 2.
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/typescript/relationships.rs crates/julie-extractors/src/javascript/relationships.rs crates/julie-extractors/src/tests/typescript/cross_file_relationships.rs crates/julie-extractors/src/tests/javascript/cross_file_relationships.rs
git commit -m "fix(ts-js): resolve qualified heritage names for cross-file inheritance"
```

### Task 11: Standardize unresolved inheritance semantics across languages

**Files:**
- Modify: `crates/julie-extractors/src/kotlin/relationships.rs`
- Modify: `crates/julie-extractors/src/swift/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/kotlin/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/swift/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/typescript/cross_file_relationships.rs`

**Step 1: Write failing tests**

Add explicit tests asserting chosen policy for unresolved interface/protocol bases (recommended policy: preserve clause-kind when syntactically known; otherwise fallback to `Extends`).

```rust
#[test]
fn test_kotlin_cross_file_interface_pending_kind_policy() {}

#[test]
fn test_swift_cross_file_protocol_pending_kind_policy() {}
```

Also fix the mislabeled TypeScript test fixture so `implements` tests actually use `implements`.

**Step 2: Run tests to verify failure**

Run: `cargo test -p julie-extractors --lib cross_file_relationships 2>&1 | tail -20`
Expected: FAIL for new policy assertions before implementation.

**Step 3: Implement minimal fix**

Update Kotlin/Swift pending relationship kind logic to match policy and adjust TS test fixture names/content.

**Step 4: Run tests to verify pass**

Run: `cargo test -p julie-extractors --lib cross_file_relationships 2>&1 | tail -20`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/kotlin/relationships.rs crates/julie-extractors/src/swift/relationships.rs crates/julie-extractors/src/tests/kotlin/cross_file_relationships.rs crates/julie-extractors/src/tests/swift/cross_file_relationships.rs crates/julie-extractors/src/tests/typescript/cross_file_relationships.rs
git commit -m "fix(extractors): align unresolved inheritance semantics across languages"
```

---

## Phase 4: Documentation and Final Verification

### Task 12: Sync embedding sidecar ops docs with current implementation

**Files:**
- Modify: `docs/operations/embedding-sidecar.md`

**Step 1: Write failing doc checks (lightweight)**

Create/update a docs sanity test (or lint script if already present) that asserts provider values in docs match supported values and required sidecar env vars are listed.

Example assertion targets:
- remove `candle` from valid provider list
- add `JULIE_EMBEDDING_SIDECAR_INIT_TIMEOUT_MS`
- add `JULIE_EMBEDDING_SIDECAR_MODEL_ID`
- add `JULIE_EMBEDDING_SIDECAR_BATCH_SIZE`
- document raw program override behavior from Task 4

**Step 2: Run check to verify failure**

Run: `python3 -m pytest -q python/embeddings_sidecar/tests/test_protocol.py -k docs --tb=short`
Expected: FAIL if you add explicit docs assertions; if no docs test harness exists, skip to Step 3 and treat this as a manual doc correction task.

**Step 3: Update docs**

Edit `docs/operations/embedding-sidecar.md` to align all env vars, fallback behavior, and supported providers with current code.

**Step 4: Verify docs accuracy against code**

Run quick consistency check:
- `cargo test --lib test_invalid_provider_sets_unresolved_runtime_status 2>&1 | tail -20`
- `python3 -m pytest -q python/embeddings_sidecar/tests/test_runtime.py --tb=short`

Expected: PASS and terminology aligns.

**Step 5: Commit**

```bash
git add docs/operations/embedding-sidecar.md
git commit -m "docs: align embedding sidecar operations guide with current runtime behavior"
```

### Task 13: Final verification sweep

**Files:**
- No new code; verify all changed files from Tasks 1-12

**Step 1: Run extractor suite**

Run: `cargo test -p julie-extractors --quiet`
Expected: PASS.

**Step 2: Run fast core Rust suite**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -20`
Expected: PASS summary with zero failures.

**Step 3: Run Python sidecar suite**

Run: `python3 -m pytest -q python/embeddings_sidecar/tests --tb=short`
Expected: PASS.

**Step 4: Review git diff for accidental changes**

Run: `git diff --stat`
Expected: only intended files from plan tasks.

**Step 5: Commit verification metadata**

```bash
git add -A
git commit -m "chore: finalize post-review hardening and regression coverage"
```

---

## Notes for Implementation Session

- Keep each task isolated; do not batch multiple risky behavior changes into one commit.
- Prefer deterministic test fixtures over sleep-heavy timing tests.
- For concurrency/timeout tests, use bounded timeouts and explicit synchronization primitives.
- If a planned test turns out flaky, first harden the test harness, then continue (do not weaken assertions).
