# Machine Service Phase 4: Semantics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development whenever delegation is available and permitted, including for one task; serialize dependent tasks. Use razorback:executing-plans only when delegation is unavailable or the user/session explicitly selected single-agent execution.

**Goal:** Replace the native sidecar broker client with one `julie-semantic-sidecar serve` child per service, report the embedding child and vector scan latency on the status page, and record the model scorecard finding.

**Architecture:** The service owns one embedding child, spawned in `serve` mode with NDJSON over the child's stdin and stdout. Every handler shares that one provider through the request engine's semantic runtime. Vectors already live in the `vectors` table of `facts.sqlite` (phase 3) and are scanned in memory by `VectorSet::scan`; phase 4 does not change storage. The broker mode, its socket, lock, and accelerator-lease files, the `/proc` child verification, and the broker challenge tests are deleted.

**Tech Stack:** Rust, `std::process::Command` with piped stdio, `serde_json` NDJSON, existing `julie_pipeline::embeddings::sidecar_protocol` envelopes, `julie-semantic-sidecar` v1 protocol (`health`, `embed_query`, `embed_batch`, `shutdown`).

**Architecture Quality:** Approved shape per `docs/plans/2026-09-09-machine-service-design.md` section 8: one child per service, restarted on exit, no broker, no sockets, no accelerator lease; lexical-only mode does zero semantic work; encoder identity is one row. Main risk: the child restart path and deadline handling, so Task 1 carries it first. Design section 12 memory budget: phase 3 measured resident memory with semantics off, so phase 4 records the semantics-on number as the new baseline instead of gating against phase 3 plus 20%.

## Global Constraints

- Design: `docs/plans/2026-09-09-machine-service-design.md` sections 8, 10, 12, and 14 item 4.
- Durable files per machine stay exactly `registry.db`, `service.json`, and `indexes/` (test `durable_roots_hold_only_the_registry_service_record_and_indexes` in `src/tests/service/durable_roots.rs`). No `embedding-host.*` files in `JULIE_HOME`.
- `JULIE_EMBEDDING_PROVIDER` keeps `auto`, `native`, `none` (`crates/julie-pipeline/src/embeddings/init.rs`). Every test binary runs with `none` via `.cargo/config.toml`.
- The sidecar `serve` verb and its v1 protocol (`julie.embedding.sidecar`, version 1) are the contract. Do not change the sidecar repo.
- Python host (`python/embeddings_sidecar/`) stays as the evaluation baseline only. Product code has no reference to it today; keep it that way.
- Sidecar binary discovery: sibling of the running binary, `JULIE_NATIVE_SIDECAR_PROGRAM`, the model cache `bin/`, then `PATH`. Delete the hard-coded `/home/murphy/...` candidates in `find_default_sidecar_binary`.
- Commit messages: conventional commits, one commit per task, on branch `semantics` in `/home/murphy/source/julie/.worktrees/semantics`.
- No pushes, no releases, no `.gitignore` or `.git/info/exclude` edits, no tool caches committed.
- File size: no line limit. Split only by responsibility.
- Test rules from `CLAUDE.md`: workers run exact tests only, at most two runs per change. The lead runs `cargo xtask test changed` and `cargo xtask test dev`.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` sections "RUNNING TESTS" and "Canonical Test Tiers"; `xtask/test_tiers.toml`.

**Worker red/green scope:** `cargo nextest run -p julie-pipeline <exact_test_name>` for pipeline tests; `cargo nextest run --lib <exact_test_name>` for top-crate tests.

**Worker ceiling:** the exact test names written in each task. Workers never run `cargo xtask test …` or an unfiltered `cargo nextest run`.

**Worker gate invariant:** each task lists its invariant under **Acceptance criteria**.

**Lead affected-change scope:** `cargo xtask test changed` after each task lands; `cargo xtask test bucket core-pipeline` and `cargo xtask test bucket service` after Tasks 1 and 2.

**Branch gate:** `cargo xtask test dev`, then `cargo xtask test system` (startup and workspace flows change), then `cargo xtask test full`. Then the phase 4 gate finding (Task 5).

**Security scope:** none declared.

**Replay/metric evidence:** hard gates: every test named in this plan passes; `durable_roots` passes; `fast` median under 10 s; trimmed `full` median under 120 s. Report-only: resident memory with semantics on, `vector_scan_millis`, scorecard top-5 per model.

**Escalation triggers:** any change to `src/service/`, `src/request_engine/`, or `crates/julie-runtime/` runs `cargo xtask test system`. Any change to scoring or the semantic backend runs `cargo xtask test dogfood`.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** `docs/plans/2026-09-10-machine-service-phase4-ledger.md` using `docs/plans/verification-ledger-template.md`. Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse evidence only when scope label and HEAD match exactly.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Stdio child provider | None - serial | Create `crates/julie-pipeline/src/embeddings/native/child.rs`, `crates/julie-pipeline/src/tests/native_child.rs`. Modify `crates/julie-pipeline/src/embeddings/native/{mod.rs,launch.rs,health.rs}`, `crates/julie-pipeline/src/tests/mod.rs`, `crates/julie-pipeline/Cargo.toml`. Delete `crates/julie-pipeline/src/embeddings/native/{client.rs,lifecycle.rs}`, `crates/julie-pipeline/src/tests/{native_broker_replacement_challenge.rs,native_challenge.rs,native_provider_challenges.rs,native_provider.rs}`. Modify `src/tests/integration/native_semantic_lifecycle.rs`, `src/tests/service/durable_roots.rs`. | Yes | Every later task builds on the new provider. |
| Task 2: One child per service | None - serial | Modify `src/service/mod.rs`, `src/request_engine/dispatch.rs`, `src/request_engine/runtime_factory.rs`, `src/request_engine/semantic.rs`, `src/tools/workspace/indexing/embeddings.rs`, `crates/julie-runtime/src/workspace/mod.rs`, `crates/julie-runtime/src/watcher/*` (provider handoff only), `src/tests/service/semantic_child.rs` (new), `src/tests/service/mod.rs`. | Yes | Needs Task 1's provider. |
| Task 3: Status page | None - serial | Modify `src/service/status.rs`, `src/service/http.rs`, `src/tools/workspace/commands/registry/status.rs`, `crates/julie-index/src/vectors/mod.rs`, `crates/julie-index/src/tests/vectors.rs`, `src/tests/service/http_api.rs`, `src/dashboard/routes/*` (render the new fields). | Yes | Reads the child state Task 2 introduces. |
| Task 4: Model scorecard finding | None - serial | Create `docs/findings/2026-09-1X-model-scorecard.md`, `docs/eval/semantic-value/results/<timestamp>.{json,md}`. Modify `docs/eval/semantic-value/scorecard.toml` (paths). | Yes | Needs a working child from Tasks 1 and 2. |
| Task 5: Phase 4 gate finding | None - serial | Create `docs/findings/2026-09-1X-machine-service-phase4-gate.md`, `docs/plans/2026-09-10-machine-service-phase4-ledger.md`. Modify `CLAUDE.md` and `AGENTS.md` (semantics bullet), `docs/plans/2026-09-09-machine-service-design.md` (phase 4 note). | Yes | Measures the finished branch. |

Commit mode for every task: `serial-worker-commit`.

---

## Task 1: Stdio child provider

**Files:**
- Create: `crates/julie-pipeline/src/embeddings/native/child.rs`
- Create: `crates/julie-pipeline/src/tests/native_child.rs`
- Modify: `crates/julie-pipeline/src/embeddings/native/mod.rs` (provider struct and trait impl)
- Modify: `crates/julie-pipeline/src/embeddings/native/launch.rs` (keep `DEFAULT_NATIVE_MODEL`, `find_and_hash_sidecar_binary`, `run_prepare`, `NativeLaunchConfig`; delete `BrokerPaths`, `derive_broker_paths`, `spawn_broker`, `verify_launched_child_sha`, the hard-coded home paths)
- Modify: `crates/julie-pipeline/src/embeddings/native/health.rs` (keep `validate_native_health`; rewrite `query_and_validate_health` to take `&mut SidecarChild`)
- Modify: `crates/julie-pipeline/src/embeddings/sidecar_protocol.rs` (delete `check_reconnect_health_match`)
- Delete: `crates/julie-pipeline/src/embeddings/native/client.rs`, `crates/julie-pipeline/src/embeddings/native/lifecycle.rs`
- Delete: `crates/julie-pipeline/src/tests/native_broker_replacement_challenge.rs`, `native_challenge.rs`, `native_provider_challenges.rs`, `native_provider.rs`
- Modify: `crates/julie-pipeline/src/tests/mod.rs` (module list), `crates/julie-pipeline/Cargo.toml` (drop `libc`; keep `sha2`, `hex` if still used)
- Note: `service_modules_contain_no_coordination_words` in `src/tests/service/budget.rs` bans the word `broker` in service modules; after this task the word must not appear anywhere under `src/` or `crates/` outside tests (rename doc comments in `src/request_engine/semantic.rs` and `crates/julie-core/src/embeddings_contract.rs` too).
- Modify: `src/tests/integration/native_semantic_lifecycle.rs` (delete `challenge_required_mode_fails_closed_on_unstarted_broker`; the mock sidecar compiled by `compile_mock_sidecar` must speak stdio, which it already does through `run_loop` style NDJSON; drop the `derive_broker_paths` import)
- Modify: `src/tests/service/durable_roots.rs` (no change expected; it must still pass with `JULIE_EMBEDDING_PROVIDER=native` when a sidecar is present, so add the assertion in step 5)

**Interfaces:** `NativeEmbeddingProvider::try_new(&EmbeddingConfig) -> Result<Self>` stays. `EmbeddingProvider` trait in `crates/julie-core/src/embeddings_contract.rs` stays; `running_executable_sha()` returns the SHA-256 of the executable the provider spawned.

**Contract inputs:** sidecar v1 protocol methods `health`, `embed_query`, `embed_batch`, `shutdown`; `RequestEnvelope`/`ResponseEnvelope` in `sidecar_protocol.rs`; `MAX_PAYLOAD_BYTES` (32 MiB) moves from `client.rs` to `child.rs`.

**File ownership:** see the contract table. **Serialization required:** Yes. **Dependency reason:** every later task builds on the new provider.

**Step 1, RED.** Write `crates/julie-pipeline/src/tests/native_child.rs`. The test compiles a tiny fake sidecar as a Rust script is too slow; use a shell script instead (unix only, `#[cfg(unix)]`). The fake reads one line, answers `health`, `embed_query`, and `embed_batch`, and exits on `shutdown` or EOF.

```rust
use std::io::Write;
use std::time::Duration;

use julie_core::embeddings_contract::{EmbeddingProvider, EmbeddingRequestBudget};

use crate::embeddings::native::child::SidecarChild;

const FAKE_SIDECAR: &str = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"method":"health"'*)
      printf '{"schema":"julie.embedding.sidecar","version":1,"request_id":"%s","result":{"ready":true,"model_id":"fake","model_sha256":"%s","dims":3,"pooling":"cls","normalization":"l2","instruction_policy_version":1,"llama_cpp_build":"fake-build","device":"cpu","runtime":"fake","accelerated":false}}\n' "$id" "$(printf 'a%.0s' $(seq 64))" ;;
    *'"method":"embed_query"'*)
      printf '{"schema":"julie.embedding.sidecar","version":1,"request_id":"%s","result":{"vector":[1.0,0.0,0.0],"dims":3}}\n' "$id" ;;
    *'"method":"embed_batch"'*)
      printf '{"schema":"julie.embedding.sidecar","version":1,"request_id":"%s","result":{"vectors":[[1.0,0.0,0.0]],"dims":3}}\n' "$id" ;;
    *'"method":"shutdown"'*) exit 0 ;;
    *'"method":"die"'*) exit 7 ;;
  esac
done
"#;

fn fake_sidecar(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("fake-sidecar.sh");
    std::fs::write(&path, FAKE_SIDECAR).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn child_answers_health_and_embeds_over_stdio() {
    let dir = tempfile::tempdir().unwrap();
    let exe = fake_sidecar(dir.path());
    let mut child = SidecarChild::spawn(&exe, "fake").unwrap();
    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(5));
    let health = child.health(&budget).unwrap();
    assert_eq!(health.dims, Some(3));
    let vector: Vec<f32> = child.embed_query("hello", &budget).unwrap();
    assert_eq!(vector, vec![1.0, 0.0, 0.0]);
}

#[cfg(unix)]
#[test]
fn provider_respawns_the_child_after_it_exits() {
    let dir = tempfile::tempdir().unwrap();
    let exe = fake_sidecar(dir.path());
    let provider = crate::embeddings::native::NativeEmbeddingProvider::try_new(
        &crate::embeddings::EmbeddingConfig {
            provider: "native".into(),
            cache_dir: Some(dir.path().to_path_buf()),
            native_program: Some(exe),
            native_model: Some("fake".into()),
        },
    )
    .unwrap();
    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(5));
    let first_pid = provider.child_pid().unwrap();
    provider.kill_child_for_test();
    let vector = provider.embed_query("again", &budget).unwrap();
    assert_eq!(vector.len(), 3);
    assert_ne!(provider.child_pid().unwrap(), first_pid);
}

#[cfg(unix)]
#[test]
fn child_request_respects_the_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sleepy.sh");
    std::fs::write(&path, "#!/bin/sh\nsleep 30\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = SidecarChild::spawn(&path, "fake").unwrap();
    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(200));
    let started = std::time::Instant::now();
    assert!(child.health(&budget).is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
}
```

Register `pub mod native_child;` in `crates/julie-pipeline/src/tests/mod.rs` and remove the four deleted modules.

**Step 2.** Run `cargo nextest run -p julie-pipeline child_answers_health_and_embeds_over_stdio`. Expect a compile error: `child` module missing.

**Step 3, implementation.** Create `crates/julie-pipeline/src/embeddings/native/child.rs`:

```rust
//! One `julie-semantic-sidecar serve` child, NDJSON over its stdin and stdout.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

use julie_core::embeddings_contract::EmbeddingRequestBudget;

use crate::embeddings::sidecar_protocol::{
    EmbedBatchRequest, EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, HealthResult,
    RequestEnvelope, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
    validate_batch_response, validate_health_response, validate_query_response,
    validate_response_envelope,
};

pub const MAX_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;

pub struct SidecarChild {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    next_id: u64,
}

impl SidecarChild {
    pub fn spawn(executable: &Path, model_id: &str) -> Result<Self> {
        let mut child = Command::new(executable)
            .arg("serve")
            .arg("--model")
            .arg(model_id)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawn {}", executable.display()))?;
        let stdin = child.stdin.take().context("child stdin")?;
        let stdout = child.stdout.take().context("child stdout")?;
        Ok(Self {
            child,
            stdin,
            lines: reader_thread(stdout),
            next_id: 1,
        })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn health(&mut self, budget: &EmbeddingRequestBudget) -> Result<HealthResult> {
        let health: HealthResult = self.round_trip("health", serde_json::json!({}), budget)?;
        validate_health_response(&health)?;
        Ok(health)
    }

    pub fn embed_query(&mut self, text: &str, budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        let result: EmbedQueryResult =
            self.round_trip("embed_query", EmbedQueryRequest { text: text.to_string(), remaining_budget_ms: Some(budget.remaining_time().as_millis() as u64) }, budget)?;
        validate_query_response(&result, result.dims)?;
        Ok(result.vector)
    }

    pub fn embed_batch(&mut self, texts: &[String], budget: &EmbeddingRequestBudget) -> Result<Vec<Vec<f32>>> {
        let result: EmbedBatchResult =
            self.round_trip("embed_batch", EmbedBatchRequest { texts: texts.to_vec(), remaining_budget_ms: Some(budget.remaining_time().as_millis() as u64) }, budget)?;
        validate_batch_response(&result, texts.len(), result.dims)?;
        Ok(result.vectors)
    }

    pub fn shutdown(mut self) {
        let _ = self.round_trip::<_, serde_json::Value>(
            "shutdown",
            serde_json::json!({}),
            &EmbeddingRequestBudget::with_timeout(Duration::from_millis(500)),
        );
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn round_trip<P: Serialize, R: DeserializeOwned>(
        &mut self,
        method: &str,
        params: P,
        budget: &EmbeddingRequestBudget,
    ) -> Result<R> {
        budget.check_budget()?;
        let id = self.next_id.to_string();
        self.next_id += 1;
        let request = RequestEnvelope {
            schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
            version: SIDECAR_PROTOCOL_VERSION,
            request_id: id.clone(),
            method: method.to_string(),
            params,
        };
        let mut line = serde_json::to_vec(&request)?;
        line.push(b'\n');
        self.stdin.write_all(&line).context("write to sidecar")?;
        self.stdin.flush()?;
        let raw = match self.lines.recv_timeout(budget.remaining_time()) {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(err)) => bail!("sidecar stdout: {err}"),
            Err(mpsc::RecvTimeoutError::Timeout) => bail!("sidecar {method} exceeded the request deadline"),
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("sidecar exited"),
        };
        let envelope: ResponseEnvelope<R> = serde_json::from_slice(&raw).context("sidecar reply")?;
        validate_response_envelope(&envelope, &id)?;
        match (envelope.result, envelope.error) {
            (Some(result), _) => Ok(result),
            (None, Some(err)) => bail!("sidecar {method}: {} ({})", err.message, err.code),
            (None, None) => bail!("sidecar {method}: empty reply"),
        }
    }
}

impl Drop for SidecarChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn reader_thread(stdout: ChildStdout) -> mpsc::Receiver<std::io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut buf = Vec::new();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(n) if n > MAX_PAYLOAD_BYTES => {
                    let _ = tx.send(Err(std::io::Error::other("sidecar line over 32 MiB")));
                    break;
                }
                Ok(_) => {
                    if tx.send(Ok(buf)).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err(err));
                    break;
                }
            }
        }
    });
    rx
}
```

Field names above were read from `sidecar_protocol.rs` (`request_id`, `dims`, `remaining_budget_ms`, `HealthResult.ready`, `model_sha256`, `instruction_policy_version: Option<u64>`). `validate_health_response` only requires `dims` when `ready` is true; `query_and_validate_health` in `health.rs` requires `model_id`, `model_sha256` (64 hex chars), `dims`, `pooling`, `normalization`, `instruction_policy_version`, and `llama_cpp_build`, which is why the fake reply carries all of them. Note: `read_until` on a `BufReader` over a pipe returns after the newline, so one line per `recv`. A line over the cap is reported once and the reader stops; the provider then respawns.

Rewrite `NativeEmbeddingProvider` in `native/mod.rs`:

```rust
pub struct NativeEmbeddingProvider {
    config: NativeLaunchConfig,
    child: Mutex<Option<SidecarChild>>,
    identity: EncoderIdentity,
    runtime_facts: Mutex<NativeRuntimeFacts>,
}

impl NativeEmbeddingProvider {
    pub fn try_new(embedding_config: &EmbeddingConfig) -> Result<Self> {
        let config = NativeLaunchConfig::try_new(
            embedding_config.native_program.as_deref(),
            embedding_config.native_model.as_deref(),
            embedding_config.cache_dir.as_deref(),
        )?;
        let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(10));
        let (child, identity, facts) = Self::spawn_and_probe(&config, &budget, None)?;
        Ok(Self {
            config,
            child: Mutex::new(Some(child)),
            identity,
            runtime_facts: Mutex::new(facts),
        })
    }

    fn spawn_and_probe(
        config: &NativeLaunchConfig,
        budget: &EmbeddingRequestBudget,
        expected: Option<&EncoderIdentity>,
    ) -> Result<(SidecarChild, EncoderIdentity, NativeRuntimeFacts)> {
        let mut child = SidecarChild::spawn(&config.executable_path, &config.model_id)?;
        let (health, identity, device_info) = query_and_validate_health(&mut child, budget)?;
        if let Some(expected) = expected {
            if identity != *expected {
                bail!("sidecar restarted with a different encoder identity");
            }
        }
        Ok((child, identity, NativeRuntimeFacts { device_info, accelerated: health.accelerated, degraded_reason: health.degraded_reason }))
    }

    fn with_child<R>(
        &self,
        budget: &EmbeddingRequestBudget,
        call: impl FnOnce(&mut SidecarChild) -> Result<R>,
    ) -> Result<R> {
        let mut guard = self.child.lock().map_err(|_| anyhow::anyhow!("sidecar mutex poisoned"))?;
        if guard.as_mut().is_none_or(|child| !child.is_alive()) {
            let (child, _, facts) = Self::spawn_and_probe(&self.config, budget, Some(&self.identity))?;
            *guard = Some(child);
            if let Ok(mut f) = self.runtime_facts.lock() {
                *f = facts;
            }
        }
        let child = guard.as_mut().expect("spawned above");
        match call(child) {
            Ok(value) => Ok(value),
            Err(err) => {
                *guard = None;
                Err(err)
            }
        }
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.child.lock().ok()?.as_ref().map(SidecarChild::pid)
    }

    pub fn kill_child_for_test(&self) {
        if let Ok(mut guard) = self.child.lock() {
            *guard = None;
        }
    }
}
```

`embed_query`, `embed_batch`, and `health_check` call `self.with_child(budget, |c| c.embed_query(text, budget))` and so on. `shutdown()` takes the child out of the mutex and calls `SidecarChild::shutdown`. `running_executable_sha()` returns `Some(self.config.executable_sha256.clone())`. Drop `request_counter`, `_child_stdin`, `running_executable_sha` field, `from_connected`, `launch_and_attach`, `ensure_connected`, `execute_round_trip`. Dropping the child on error is the restart policy: the next request respawns.

`health.rs`: `query_and_validate_health(child: &mut SidecarChild, budget) -> Result<(HealthResult, EncoderIdentity, DeviceInfo)>`; keep the existing identity and device derivation, remove the connection argument.

`launch.rs`: delete `BrokerPaths`, `derive_broker_paths`, `spawn_broker`, `verify_launched_child_sha`, and the `broker_paths` field on `NativeLaunchConfig`. In `find_default_sidecar_binary`, delete the two `/home/murphy/...` candidates. Keep `find_and_hash_sidecar_binary` (the status page reports the SHA) and `run_prepare`.

`mod.rs` re-exports: replace the `client::*` and `launch::{BrokerPaths, derive_broker_paths, spawn_broker, verify_launched_child_sha}` exports with `pub use child::{MAX_PAYLOAD_BYTES, SidecarChild};`. Then fix every compile error the deletions cause with `cargo check -p julie-pipeline` and `cargo check --workspace --all-targets`; expect hits in `src/tests/integration/native_semantic_lifecycle.rs` and `src/tests/service/budget.rs` (search `broker` there and delete or rename the broker assertions).

**Step 4.** Run `cargo nextest run -p julie-pipeline native_child`. All three pass. Run `cargo nextest run --lib tests::service::durable_roots`. Passes.

**Step 5.** Add to `durable_roots_hold_only_the_registry_service_record_and_indexes` nothing; instead confirm by search that `embedding-host` no longer appears in `src` or `crates`: `fast_search(query="embedding-host", workspace=...)` returns zero product hits.

**Step 6.** Commit: `refactor(semantics): replace the sidecar broker with one stdio serve child`.

**Acceptance criteria:**
- [x] `cargo nextest run -p julie-pipeline native_child` passes (3 tests).
- [x] `cargo check --workspace --all-targets` is clean of errors.
- [x] `native/client.rs`, `native/lifecycle.rs`, and the four broker test files are deleted; `libc` is gone from `crates/julie-pipeline/Cargo.toml`.
- [x] No `embedding-host`, `accelerator_lock`, `BrokerPaths`, or `/home/murphy` string remains under `src/` or `crates/`.
- [x] `cargo nextest run --lib tests::service::durable_roots` passes.

---

## Task 2: One child per service

**Files:**
- Modify: `src/request_engine/semantic.rs` (`DefaultSemanticRuntime` already caches one provider and single-flights its creation; it stays the owner)
- Modify: `src/request_engine/dispatch.rs` (the `RequestEngine::new` path builds one `DefaultSemanticRuntime::from_registry_paths`; keep)
- Modify: `src/request_engine/runtime_factory.rs` (`create_bound_runtime` and `create_unbound_runtime` call `handler.set_injected_embedding_provider(runtime.provider())` when a provider exists, and the semantic runtime pushes later-acquired providers to every live handler)
- Modify: `crates/julie-runtime/src/workspace/mod.rs` (delete `initialize_embedding_provider`; `JulieWorkspace.embedding_provider` is set by the handler from the shared runtime)
- Modify: `src/tools/workspace/indexing/embeddings.rs` (`spawn_workspace_embedding` gets the provider from the semantic runtime through the handler; delete the deferred `initialize_embedding_provider` block)
- Modify: `crates/julie-runtime/src/watcher/*` only where `update_embedding_provider` is called
- Create: `src/tests/service/semantic_child.rs`; register in `src/tests/service/mod.rs`

**Interfaces:** `SemanticRuntime::provider() -> Option<Arc<dyn EmbeddingProvider>>` (exists). New: `RuntimeFactory::set_semantic_runtime(Arc<dyn SemanticRuntime>)` or constructor argument, so handlers built later see the same runtime. Find the current constructor order with `fast_refs(symbol="RuntimeFactory::new")` and `get_symbols(file_path="src/request_engine/dispatch.rs", mode="full")` before editing.

**Contract inputs:** design section 5: "One embedding child per service, spawned in `serve` mode, restarted on exit." Design section 8: "Lexical-only mode does zero semantic work."

**File ownership:** see the contract table. **Serialization required:** Yes. **Dependency reason:** needs Task 1.

**Step 1, RED.** `src/tests/service/semantic_child.rs`:

```rust
use super::http_api::Running;
use serde_json::json;

#[cfg(unix)]
#[tokio::test]
async fn two_checkouts_share_one_embedding_child() {
    let fake_dir = tempfile::tempdir().unwrap();
    let fake = crate::tests::helpers::fake_sidecar::write(fake_dir.path());
    let mut env = crate::tests::helpers::env::EnvVarGuard::new();
    env.set("JULIE_NATIVE_SIDECAR_PROGRAM", fake.as_os_str());
    env.set("JULIE_EMBEDDING_PROVIDER", "native");
    let running = Running::start(None).await;
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    for dir in [&a, &b] {
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join(".git"), "gitdir: nowhere\n").unwrap();
        std::fs::write(root.join("lib.rs"), "pub fn shared_child_probe() {}\n").unwrap();
        let response = running.api("manage_workspace", json!({"operation": "index", "path": root.to_string_lossy()})).await;
        assert_eq!(response.status(), 200);
    }
    let status = running.status().await;
    let child = &status["embedding_child"];
    assert_eq!(child["state"], "ready", "got {status}");
    let pid = child["pid"].as_u64().unwrap();
    let after = running.status().await;
    assert_eq!(after["embedding_child"]["pid"].as_u64().unwrap(), pid, "one child, one pid");
}

#[tokio::test]
async fn lexical_only_mode_never_spawns_the_child() {
    let mut env = crate::tests::helpers::env::EnvVarGuard::new();
    env.set("JULIE_EMBEDDING_PROVIDER", "none");
    let running = Running::start(None).await;
    let root_dir = tempfile::tempdir().unwrap();
    let root = root_dir.path().canonicalize().unwrap();
    std::fs::write(root.join(".git"), "gitdir: nowhere\n").unwrap();
    std::fs::write(root.join("lib.rs"), "pub fn lexical_probe() {}\n").unwrap();
    let response = running.api("manage_workspace", json!({"operation": "index", "path": root.to_string_lossy()})).await;
    assert_eq!(response.status(), 200);
    let search = running.api("fast_search", json!({"query": "lexical_probe", "workspace": root.to_string_lossy()})).await;
    assert_eq!(search.status(), 200);
    assert_eq!(running.status().await["embedding_child"]["state"], "absent");
}
```

Copy the `FAKE_SIDECAR` script from Task 1 into a new helper `src/tests/helpers/fake_sidecar.rs` (`pub fn write(dir: &Path) -> PathBuf`, registered in `src/tests/helpers/mod.rs`); the pipeline test keeps its own copy because crates cannot see `src/tests`. `Running` (`src/tests/service/http_api.rs`) has no `api` or `status` helper today; add `pub(crate) async fn api(&self, tool: &str, params: serde_json::Value) -> reqwest::Response` (POST `{base}/api/{tool}` with `bearer_auth(&self.token)` and `.json(&params)`) and `pub(crate) async fn status(&self) -> serde_json::Value` (GET `{base}/status`), copying the request shape from `status_carries_every_checkout`. `EnvVarGuard` is `let mut env = EnvVarGuard::new(); env.set(key, value);` and restores on drop.

**Step 2.** `cargo nextest run --lib two_checkouts_share_one_embedding_child`. Expect a failure: no `embedding_child` field yet (it lands in Task 3, so for this task assert the pid through a temporary `provider.child_pid()` read on the shared runtime: `running.engine().semantic_runtime.provider()` downcast is not available, so expose `SemanticRuntime::child_pid(&self) -> Option<u32>` as a default trait method returning `None`, implemented on `DefaultSemanticRuntime` by downcasting through a new `EmbeddingProvider::child_pid(&self) -> Option<u32>` default method). Use that in the test until Task 3 replaces it with the status field. Keep the trait method; Task 3 reads it.

**Step 3, implementation.**
1. `EmbeddingProvider::child_pid(&self) -> Option<u32> { None }` in `crates/julie-core/src/embeddings_contract.rs`; `NativeEmbeddingProvider` overrides it.
2. `RuntimeFactory` holds `semantic_runtime: Arc<dyn SemanticRuntime>`; `RequestEngine::new` passes the runtime it builds. Every `create_*_runtime` calls `handler.set_injected_embedding_provider(self.semantic_runtime.provider())`.
3. `DefaultSemanticRuntime::get_or_acquire_provider` already single-flights `acquire_in_process_embedding_provider`. After it stores a new provider, it must reach handlers built earlier: add `subscribers: Arc<std::sync::Mutex<Vec<std::sync::Weak<JulieServerHandler>>>>` is one option; the simpler one is that `JulieServerHandler::embedding_provider()` asks the runtime every time. Do the simple one: give the handler an `Arc<dyn SemanticRuntime>` field (`semantic_runtime`), set by the factory, and make `embedding_provider()` return `self.semantic_runtime.provider()` after the `semantics_disabled` and injected checks. Delete the per-workspace branch (`ws.embedding_provider`).
4. `spawn_workspace_embedding`: replace the deferred-init block with `let Some(provider) = handler.acquire_embedding_provider(Duration::from_secs(30)).await else { return EmbeddingOutcome::skipped() };` where `acquire_embedding_provider` calls `semantic_runtime.ensure_ready(binding, SemanticRequirement::Query, SemanticMode::Auto, deadline, &CancellationToken::new())` and then `provider()`. Verify the exact `ensure_ready` signature with `get_symbols(file_path="src/request_engine/semantic.rs", target="ensure_ready", mode="minimal")`.
5. Delete `JulieWorkspace::initialize_embedding_provider` and `initialize_all_components`'s call to it; the watcher receives the provider through `update_embedding_provider` from the handler after the shared runtime resolves (find the call sites with `fast_refs(symbol="update_embedding_provider")`).
6. `JULIE_EMBEDDING_PROVIDER=none` short-circuits in `create_embedding_provider` already; `DefaultSemanticRuntime` must not call it in `Off` mode (`ensure_ready` rule 1 already returns before acquisition). The `index` operation must not acquire the provider when embeddings are disabled: `spawn_workspace_embedding` checks `julie_pipeline::embeddings::init::embeddings_disabled_by_env()` first and returns `skipped`.

**Step 4.** `cargo nextest run --lib two_checkouts_share_one_embedding_child lexical_only_mode_never_spawns_the_child`. Both pass. Then `cargo nextest run --lib tests::integration::native_semantic_lifecycle`: the remaining tests pass or are rewritten to the shared-runtime shape (the `native_semantics_becomes_ready_without_client_restart` test keeps its meaning: a request before the child is ready degrades, a later one succeeds).

**Step 5.** Commit: `feat(service): share one embedding child across every checkout`.

**Acceptance criteria:**
- [ ] `cargo nextest run --lib tests::service::semantic_child` passes (2 tests).
- [ ] `cargo nextest run --lib tests::integration::native_semantic_lifecycle` passes.
- [ ] `JulieWorkspace::initialize_embedding_provider` is deleted; `fast_refs(symbol="create_embedding_provider")` shows exactly one product caller (`acquire_in_process_embedding_provider`).
- [ ] With `JULIE_EMBEDDING_PROVIDER=none`, indexing and searching never call `create_embedding_provider` (the lexical test proves it through the child state).

---

## Task 3: Status page: embedding child and vector scan latency

**Files:**
- Modify: `src/service/status.rs` (`StatusDocument` gains `embedding_child: EmbeddingChildStatus`)
- Modify: `src/service/http.rs` (`status` handler fills it from the engine's semantic runtime)
- Modify: `src/request_engine/semantic.rs` (`SemanticRuntime::child_status(&self) -> EmbeddingChildStatus` default `Absent`)
- Modify: `src/tools/workspace/commands/registry/status.rs` (`CheckoutStatus.vector_scan_millis: Option<u64>`)
- Modify: `crates/julie-index/src/vectors/mod.rs` (`VectorSet::scan` records the last scan duration in an `AtomicU64` micros field; `last_scan_micros() -> Option<u64>`)
- Modify: `crates/julie-index/src/tests/vectors.rs` (one test)
- Modify: `src/tests/service/http_api.rs` (status document assertions)
- Modify: `src/dashboard/routes/*` where the status document is rendered (add the two fields; find with `fast_search(query="graph_resident_bytes", file_pattern="src/dashboard/**")`)

**Interfaces:**

```rust
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EmbeddingChildStatus {
    Absent,
    Starting,
    Ready { pid: u32, model_id: String, dimensions: usize, executable_sha256: String },
    Degraded { reason: String, retryable: bool },
}
```

**Contract inputs:** design section 10 lists "embedding child state" for the service and "vector count" per checkout; the phase 3 gate finding asks for `vector_scan_millis`.

**File ownership:** see the contract table. **Serialization required:** Yes. **Dependency reason:** reads Task 2's shared runtime.

**Step 1, RED.** In `crates/julie-index/src/tests/vectors.rs` add:

```rust
#[test]
fn scan_records_its_last_duration() {
    let set = VectorSet::from_rows(None, &[], |_| None);
    assert_eq!(set.last_scan_micros(), None);
    let _ = set.scan(&[1.0, 0.0], 5);
    assert!(set.last_scan_micros().is_some());
}
```

In `src/tests/service/http_api.rs`, in the existing status test (find it with `get_symbols`), add `assert_eq!(status["embedding_child"]["state"], "absent");` and, for the checkout entry after an index, `assert!(checkout.get("vector_scan_millis").is_some());`. Replace the temporary `child_pid` assertion in Task 2's test with `status["embedding_child"]["pid"]`.

**Step 2.** `cargo nextest run -p julie-index scan_records_its_last_duration`. Expect a compile error (`last_scan_micros` missing).

**Step 3, implementation.** `VectorSet` gains `last_scan_micros: AtomicU64` (0 means never; store `micros + 1`). `scan` wraps its body in an `Instant::now()` measurement. `CheckoutStatus.vector_scan_millis` reads `snapshot.vectors().last_scan_micros().map(|m| m / 1000)`; find how `CheckoutStatus` reaches the snapshot with `get_symbols(file_path="src/tools/workspace/commands/registry/status.rs", mode="full")`. `DefaultSemanticRuntime::child_status` maps `RuntimeProviderState` plus `provider().child_pid()`, `device_info()`, `dimensions()`, `running_executable_sha()`.

**Step 4.** `cargo nextest run -p julie-index scan_records_its_last_duration`, then `cargo nextest run --lib tests::service::http_api tests::service::semantic_child`. Pass.

**Step 5.** Commit: `feat(status): report the embedding child and vector scan latency`.

**Acceptance criteria:**
- [ ] `GET /status` carries `embedding_child` with the four states and `checkouts[].vector_scan_millis`.
- [ ] The dashboard renders both fields.
- [ ] Tests above pass.

---

## Task 4: Model scorecard finding

**Files:**
- Modify: `docs/eval/semantic-value/scorecard.toml` (paths from `/Users/murphy` to `/home/murphy`; only `julie` and `eros` exist on this machine, so the run uses `--repo julie --repo eros`)
- Create: `docs/eval/semantic-value/results/<timestamp>.json` and `.md` per model (the harness writes them)
- Create: `docs/findings/2026-09-1X-model-scorecard.md`

**Interfaces:** `python3 docs/eval/semantic-value/run_scorecard.py --binary target/release/julie-server --repo julie --repo eros --backend lexical --backend semantic --backend hybrid`. The model is chosen with `JULIE_NATIVE_SIDECAR_MODEL=<id>`; the sidecar manifest ids are `bge-small-en-v1.5-f32` and `qwen3-0.6b-f16` (`~/source/julie-semantic-sidecar/src/manifest.rs`). Prepare each model first: `julie-semantic-sidecar prepare --model <id>`.

**Contract inputs:** design section 8: "Ship the smallest model within one top-five case of the Python baseline. Record the result as a finding." Baseline: `docs/eval/semantic-value/results/2026-05-23T18-10-58Z.md` (the last Python-host run: 30 cases, semantic top-5 93.3%, hybrid top-5 83.3%). Its header does not name the model; cite the Python host's default model from `python/embeddings_sidecar` (find it with `fast_search(query="model", file_pattern="python/embeddings_sidecar/**")`) in the finding.

**File ownership:** see the contract table. **Serialization required:** Yes. **Dependency reason:** needs a working child.

**Steps:**
1. `cargo build --release` in the worktree; `target/release/julie-server service restart` is not needed: the harness runs the binary directly. Use a temporary `JULIE_HOME` (`export JULIE_HOME=$(mktemp -d)`) so the run indexes into a scratch home.
2. For each model id: prepare, export `JULIE_NATIVE_SIDECAR_MODEL`, index both repos through the binary (`julie-server workspace index --path <repo>` or the harness's own step, check `run_scorecard.py` for how it indexes), run the harness, keep the two result files.
3. CodeRankEmbed: the sidecar manifest has no entry, so it cannot run. Record that in the finding as "not available in `julie-semantic-sidecar` manifest as of `<sidecar commit>`; requires a sidecar release with a GGUF conversion" with the manifest line as evidence. That is a verified exclusion, not a skip.
4. Write the finding: per model, top-1/3/5/8 on `julie` and `eros`, query latency median, prepare time, model file size, resident memory of the child (read `/proc/<pid>/status` VmRSS while the run is active). Compare top-5 with the Python baseline. State the recommended default per the design rule and whether `DEFAULT_NATIVE_MODEL` changes. Do not change the default in this task; the finding is the input to the owner's decision.
5. Commit: `docs(semantics): model scorecard finding for bge-small and qwen3-0.6b`.

**Acceptance criteria:**
- [ ] Result files for both models exist under `docs/eval/semantic-value/results/`.
- [ ] The finding records both models, the CodeRankEmbed exclusion with evidence, and a recommendation.
- [ ] `scorecard.toml` paths point at this machine.

---

## Task 5: Phase 4 gate finding

**Files:**
- Create: `docs/findings/2026-09-1X-machine-service-phase4-gate.md`
- Create: `docs/plans/2026-09-10-machine-service-phase4-ledger.md`
- Modify: `CLAUDE.md` and `AGENTS.md` (Core Design Decisions item 8: sidecar child, no broker; item 3: no `embedding-host.*` files). Keep both files identical.
- Modify: `docs/plans/2026-09-09-machine-service-design.md` section 14 item 4: add a "*Landed:*" note with the commit range.

**Steps:**
1. Run the branch gate in order: `cargo xtask test dev`, `cargo xtask test system`, `cargo xtask test full` three times for the median, `cargo xtask test fast` three times. Record each in the ledger with the HEAD SHA.
2. Measure with semantics on (`JULIE_EMBEDDING_PROVIDER=native`, temp `JULIE_HOME`) on `/home/murphy/source/julie` and `/home/murphy/source/miller`: `julie-server workspace index`, run five `fast_search` calls with `backend=semantic`, then `julie-server service status`. Record `rss_bytes`, `graph_resident_bytes`, `vector_count`, `vector_scan_millis`, and the child's VmRSS.
3. Net lines: `tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust` at the branch base (`eacfc98f`) and at HEAD. Phase 4 must be net negative.
4. Durable roots: `find $JULIE_HOME -maxdepth 1` shows only `registry.db` (+wal/shm), `service.json`, `indexes`.
5. Write the finding with the same table shape as `docs/findings/2026-09-10-machine-service-phase3-gate.md`, verdict first.
6. Commit: `docs(machine-service): phase 4 gate finding and ledger`.

**Acceptance criteria:**
- [ ] Every gate command in the ledger has a row with SHA, result, and timestamp.
- [ ] The finding states pass or fail per budget with the numbers.
- [ ] `CLAUDE.md` equals `AGENTS.md`.

---

## Verification Ledger

See `docs/plans/2026-09-10-machine-service-phase4-ledger.md` (created in Task 5; earlier tasks append rows as they land).
