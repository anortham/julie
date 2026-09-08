# Julie head-to-head evaluation implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development where delegation is available; otherwise razorback:executing-plans. Root Codex reviews the implementation and the interpretation of results. Do not select a winner by construction.

**Goal:** Build and run a reproducible comparison of revived Julie, Miller and raw tools, including full-function operation, and produce task-level evidence for subsequent product decisions.

**Architecture:** Extend the existing xtask-eval package with a separate revival command and modules for contracts, process execution, scoring and reporting. Evaluate products through their real CLI/MCP paths, not private search internals. Keep manifests/answers/results outside indexed source roots; distinguish deterministic retrieval results from agent-run task outcomes.

**Tech Stack:** Rust, serde JSON/TOML, existing xtask-eval CLI and report conventions, real subprocesses, OS process-tree resource measurements, fixed source checkouts.

**Architecture Quality:** Risk medium. Evaluation has no production ranking authority. Product adapters translate requests/results only; relevance judgments use source-grounded expected targets independent of either product's IDs or ranking. A result importer can reject incomplete evidence but must never invent a missing agent outcome.

## Global Constraints

- Planning baseline Julie `0432158c`, Miller `40f7c71a`; record actual tested revisions afresh. Prior telemetry and May semantic scorecards are discovery evidence, not acceptance baselines.
- Requires reviewed extractor/request-engine/lifecycle plans for full-product execution and native-semantics plan for native candidate qualification. Harness work can start earlier against protocol fixtures; no full-mode result until dependencies pass.
- Continuous testing is excluded. Raw baseline means ordinary shell/source tools, not a new testing service.
- Run products separately for performance measurements; run coexistence contention as its own labeled experiment. Do not let another active worker build or test in a measured corpus during a sample.
- Freeze tasks, expected targets, config and pass criteria before viewing candidate results. Separate development/tuning set and held-out set. Do not promote a model on the corpus used to tune its thresholds.
- Identical corpus commits, extraction coverage, exclusions, hardware, agent model/prompt and task budgets for paired comparisons. Best-product comparison may use different encoders; stack-isolation experiments must use the same encoder and record unsupported combinations rather than fabricate parity.
- Never index task instructions, expected answers, benchmark reports or outcome exports. Runtime validator checks path ancestry and index exclusion evidence before launching products.
- Agent episodes use existing approved worker mechanisms. Do not purchase API credits, invoke paid external services, push or publish without approval. This plan does not install a new agent runner or assume CLI authentication.
- Source changes to fix comparison failures belong in the responsible implementation branch with tests; rerun affected cases and then the frozen held-out suite. Do not rewrite answers to remove failures.
- New implementation files at most 500 lines. New test code in xtask-eval/src/tests; fixture inputs in fixtures/revival-eval. No fake product data in accepted results.

## Grounded locations

`xtask-eval/src/main.rs` dispatches CliCommand; `xtask-eval/src/cli.rs` owns parsing; `xtask-eval/src/lib.rs` exports modules; `xtask-eval/src/search_matrix.rs` and `search_matrix_report.rs` demonstrate current execution/report conventions. `.cargo/config.toml` already supplies the xtask-eval alias. `xtask/test_tiers.toml` has an xtask-eval bucket running the package tests. Preserve existing search-matrix and eval commands.

Create focused `xtask-eval/src/revival/{mod.rs,contract.rs,runner.rs,adapters.rs,scoring.rs,report.rs}` and tests under `xtask-eval/src/tests/revival/`. Source corpus fixtures live in `fixtures/revival-eval/corpora/`; manifest/answers are copied to a separate run root before invocation. Long fixtures are files, not strings embedded in implementation code.

## Frozen experiment contract

Add these proposed commands, keeping names explicit and documenting actual help output:

```text
cargo xtask-eval revival validate --manifest /absolute/run/manifest.json
cargo xtask-eval revival run --manifest /absolute/run/manifest.json --out /absolute/run/results
cargo xtask-eval revival import-outcomes --manifest /absolute/run/manifest.json --input /absolute/run/agent-outcomes.jsonl --out /absolute/run/results
cargo xtask-eval revival report --input /absolute/run/results --out /absolute/run/report
```

`run` accepts argv arrays for verified binaries, never shell strings. One manifest binds binaries and file hashes, working directories, environment allowlist, corpus roots/commits/diff hashes, extractor identity, encoder identity, retrieval mode, seed and limits. Store UTC timestamps, monotonic durations, raw stdout/stderr hashes and exit status per invocation. Use a new run directory; never overwrite an accepted report.

Minimum held-out retrieval set: 60 manually verified cases, 10 each for exact symbols, identifier fragments, literal strings/configuration, documentation, conceptual search and references/related implementations. Span at least Rust, C#, Python, TypeScript, Java and Go plus a mixed-language repository; include known negatives and ambiguous names in every applicable category. Add discovery/representative operation checks for every entry in the pinned extractor capability list with explicit verified-not-applicable classifications for concepts absent from a language. The 60-case set is not a substitute for that coverage ledger.

Minimum agent set: 12 tasks, two each for locate a defect, explain a call path, identify references, safe edit/rename, ambiguous-name disambiguation, and concurrent two-worktree work. Each has immutable source commit, task prompt, expected evidence and independent acceptance command/output predicate. Use three fresh paired episodes per task/product with seeded order; report variation, not only the best trial. Start with local approved worker sessions and import their evidence through the same validator; no commercial model invocation baked into the harness.

Performance matrix: fresh-index cold readiness, warm repeated reads, first full-semantic readiness, edit-to-search freshness, 1/4/8 clients on one worktree, and 4 clients across four worktrees. At least three indexing runs and 30 warm query samples per category/configuration. Record model preparation separately from cold process start. Collect peak memory for the entire process tree, CPU time, disk allocated/logical size, timeouts and process cleanup. Distinguish OS cache state; do not claim cold cache from a process restart alone or drop machine caches without approval.

Hard release-readiness failures: any wrong-worktree result/write, overwritten conflicting acknowledged edit, ready result from incompatible vector/index generation, unbounded hang beyond explicit deadline, malformed structured output or mislabeled semantic fallback. Correct empty results are not failures. Quality/performance differences are measured outcomes, not grounds for changing test answers.

Default promotion is NOT automatic. Recommend native only if all safety gates pass, it completes at least as many held-out agent tasks as Python, and no retrieval category loses more than one correct top-five case out of ten; report paired differences and uncertainty. A failure keeps native selectable and Python available. Recommend Julie over Miller only with per-task evidence; a trade-off is an acceptable outcome. The user makes the product/default decision.

## Verification Strategy

**Project source of truth:** AGENTS.md, docs/TESTING_GUIDE.md, xtask/test_tiers.toml.

**Worker red/green scope:** Exact `cargo nextest run -p xtask-eval --lib NAME`; `cargo check -p xtask-eval` before GREEN. Test processes use bounded waits and fixture binaries, not the full measured product suite.

**Worker ceiling:** One exact test per RED/GREEN cycle, no concurrent cargo test processes. Workers implementing the harness do not run the expensive comparison while others are building.

**Worker gate invariant:** Manifest safety/provenance, strict output parsing, independent scoring, missing-outcome refusal and timeout cleanup are mandatory. Tests must exercise those decisions, not only serialization snapshots.

**Lead affected-change scope:** `cargo xtask test bucket xtask-eval` after a coherent harness batch.

**Branch gate:** `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo xtask test dev`, plus xtask-eval bucket if not covered at the same SHA. Run `cargo xtask test dogfood` only when fixing actual search behavior, not merely changing reports.

**Security scope:** none declared as a separate tier. Manifest-controlled argv execution must reject shell interpolation; outputs and environment records must omit credentials.

**Replay/metric evidence:** Safety criteria above are hard gates. Latency/RSS/ranking are report-only measured results unless a pre-frozen qualification rule explicitly applies. Missing real episodes means evaluation incomplete, not pass.

**Escalation triggers:** Product defects are sent to the matching plan and verified there. Resource stress runs require an idle controlled machine and captured hardware/software facts.

**Assigned verification failure:** Diagnose and repair harness defects; do not conceal product errors by marking their samples missing. Failure status is itself a retained outcome.

**Verification ledger:** Fill the empty table only after execution; record tested SHA and dirty-tree hash. Expensive evidence reuse requires identical scope/SHA/config/corpus/model/binary hashes.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| E1 Manifest and corpus contract | None - serial | revival/contract.rs,mod.rs; tests/revival/contract.rs; src/lib.rs test/module registration; fixtures/revival-eval/ | Yes | Freezes independent judgments before adapter execution |
| E2 Real product runner | Batch A | revival/runner.rs,adapters.rs; tests/revival/runner.rs and fixture executables | No | None - safe parallel batch. Consumes E1 only |
| E3 Scoring and outcome import | Batch A | revival/scoring.rs,report.rs; tests/revival/scoring.rs; docs/eval/revival-protocol.md | No | None - safe parallel batch. Consumes E1 only |
| E4 CLI and acceptance campaign | None - serial | src/cli.rs,main.rs; revival/mod.rs; tests/revival/cli.rs; docs/plans/revival-evaluation-verification.md | Yes | Integrates E2/E3 and runs reviewed products |

E1/E4 use serial-worker-commit; E2/E3 use parallel-lead-commit. Lead owns module registration merges and serializes all test runs. Checkpoint before every commit. Do not execute production changes while measuring.

### E1: Validate provenance and isolate the answer corpus

**Files/ownership:** E1 row. **Interfaces:** proposed RunManifest, ProductConfig, CorpusSpec, RetrievalCase, AgentTask and `validate_manifest(&RunManifest) -> anyhow::Result<()>`; serialized schema `julie-revival-manifest-v1`. **Contract inputs:** frozen experiment above. **Serialization required:** Yes; E2/E3 consume the same contract.

**Step 1, RED:** Define `paths_are_isolated(&Path,&Path) -> anyhow::Result<bool>` and its test. Corpus, manifest and answer paths must exist so canonicalization catches symlink aliases. For a new output directory, create only its requested parent under the designated task-owned run root, canonicalize that parent and reject overlap before creating the final directory. Recheck canonical output identity after creation and immediately before launching. Do not create output directories inside an indexed root merely to validate them.

```rust
#[test]
fn revival_rejects_answers_inside_indexed_corpus() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let answers = corpus.join("answers");
    std::fs::create_dir_all(&answers).unwrap();
    assert!(!paths_are_isolated(&corpus, &answers).unwrap());
}
```

**Step 2:** `cargo nextest run -p xtask-eval --lib revival_rejects_answers_inside_indexed_corpus`. Expected missing function before implementation.

**Step 3, implementation:** Canonicalize both paths; refuse equality or either being an ancestor of the other. Resolve case/UNC semantics through platform-native paths, not a hardcoded string slash rule. Check every manifest/answer/output path against every indexed root. Require all product/source hashes, stable case IDs, retrieval categories and explicit timeouts. Reject uncommitted measured corpora unless their exact diff hash is recorded in a deliberately labeled experiment; accepted baseline is clean. Verify expected targets exist by path/span/source hash; never use Julie or Miller IDs as the only oracle.

```rust
pub fn paths_are_isolated(a: &std::path::Path, b: &std::path::Path) -> anyhow::Result<bool> {
    let a = a.canonicalize()?;
    let b = b.canonicalize()?;
    Ok(!a.starts_with(&b) && !b.starts_with(&a))
}
```

Create the actual 60-case and 12-task manifests from fixed corpora. Each expected answer needs a human-readable justification tied to source and a deterministic verifier where applicable. Seed fixtures with positive and negative examples; label synthetic discovery cases separately from real-code quality cases. Retain original corpus licensing/provenance. The plan does not permit placeholder expected targets or automatically treating top-ranked output as truth.

**Step 4:** `cargo check -p xtask-eval`, then exact RED command. Lead verifies symlink nesting, unknown categories, absent commits, duplicate case IDs and output pollution tests.

**Step 5:** Apply commit mode. **Acceptance:**
- [ ] Complete frozen manifests, source-grounded expected targets and full capability ledger.
- [ ] Results/answers cannot be indexed accidentally; all run identity fields are validated.
- [ ] Root Codex reviews held-out judgments before candidate output is inspected.

### E2: Execute actual products with bounded cleanup

**Files/ownership:** E2 row. **Interfaces:** proposed `run_process(spec: &ProcessSpec) -> anyhow::Result<ProcessOutcome>`; ProcessSpec carries program, argv, cwd, env allowlist, deadline and output cap. Outcome distinguishes successful, failed, timed_out and malformed. **Contract inputs:** E1 immutable run identity and product adapter definitions. **Serialization required:** No; parallel with E3 after E1.

**Step 1, RED:** Add a fixture executable that writes valid output and spawns a waiting child. The exact test `revival_timeout_reaps_the_owned_process_tree` starts it with a short deadline, then proves both owned PIDs are gone and the outcome remains timed_out. Use test-only fixture binaries/Python scripts already available in the environment; do not use shell-specific sleep commands as cross-platform behavior.

Add this independent argv test for the proposed runner's command builder:

```rust
#[test]
fn revival_preserves_query_as_one_argument() {
    let args = search_argv("$(do-not-run) spaced query");
    assert_eq!(args, vec!["search", "$(do-not-run) spaced query"]);
}
```

**Step 2:** `cargo nextest run -p xtask-eval --lib revival_timeout_reaps_the_owned_process_tree`. Expected failure until process-tree cleanup works.

**Step 3, implementation:** Use Command/argv without shell evaluation. Execute approved binaries only; verify the executable hash immediately before starting. Implement Unix process-group and Windows Job Object ownership so timeout/cancellation cleans up only the invocation's processes. Bounded stdout/stderr collectors must drain without deadlocking and report truncation rather than parse a partial success. Capture monotonic startup, result and exit timestamps separately. Never include credentials from the parent's environment in the manifest.

Normalize product results to source-relative path, symbol/span, evidence kind, retrieval mode and freshness. Preserve raw output bytes and conversion errors. CLI adapters are version-specific and validated against real `--help`/capabilities; unsupported flags are a failed setup, not a fallback to defaults. Reference suggestions and exact references remain distinct. Separate setup/model preparation from timed work. Whole-process-tree resource collection must disclose unavailable counters on a platform. Use fresh isolated per-product homes/caches and record the runtime-reported PID/identity of every broker/index owner. Sample those owned shared-service PIDs even when reparented outside the CLI tree; include their memory/CPU once, not once per client. Refuse an unexplained connection to a pre-existing unmeasured shared service. Warm samples reuse this explicitly owned service set; separately report per-client incremental cost and total product cost. Coexistence runs get separate labels and cannot substitute for isolated resource comparisons.

**Step 4:** `cargo check -p xtask-eval`, then exact RED command. Lead runs bounded-output, nonzero-exit, malformed-JSON, argv literal and unknown-version fixture tests.

**Step 5:** Hand verified diff to lead, no worker commit. **Acceptance:**
- [ ] Real invocations preserve arguments, enforce deadlines and leave no owned orphan processes.
- [ ] Adapter cannot silently downgrade full mode or omit failed samples.
- [ ] Per-stage time and whole-tree resource evidence is tied to binary/config identity.

### E3: Score independently and refuse invented outcomes

**Files/ownership:** E3 row. **Interfaces:** proposed `first_relevant_rank(expected: &BTreeSet<String>, actual: &[String]) -> Option<usize>` and outcome importer keyed by run/case/product/episode identity. **Contract inputs:** E1 manifest; E2 outcome shape is defined in E1 before parallel dispatch. **Serialization required:** No; parallel with E2.

**Step 1, RED:** Add this scoring test:

```rust
#[test]
fn revival_missing_target_is_not_a_success() {
    let expected = std::collections::BTreeSet::from(["src/a.rs:target".to_owned()]);
    let actual = vec!["src/b.rs:other".to_owned()];
    assert_eq!(first_relevant_rank(&expected, &actual), None);
}
```

**Step 2:** `cargo nextest run -p xtask-eval --lib revival_missing_target_is_not_a_success`. Expected missing scorer before implementation.

**Step 3, implementation:** Compute top1/top5 and reciprocal rank per case/category using stable normalized source targets. For known-negative cases, score correct absence separately. Deduplicate repeated results without improving their original rank. Refuse comparisons whose source/config identity differs from the requested experiment. Full mode requires actual semantic readiness/coverage, not just a flag.

```rust
pub fn first_relevant_rank(
    expected: &std::collections::BTreeSet<String>,
    actual: &[String],
) -> Option<usize> {
    actual.iter().position(|hit| expected.contains(hit)).map(|index| index + 1)
}
```

Import agent outcomes only with exact manifest hash, task/product/episode ID, agent model/prompt identity, timing, transcript hash and independent acceptance result. Missing tokens stay unknown; bytes divided by four are explicitly estimates. Reject duplicate/conflicting outcomes and incomplete identity joins. A successful tool call does not set task success. Reports show all denominators, timeouts, absent observations, median/p95 with defined percentile calculation, per-task differences and outliers. Report both inclusive totals and clearly labeled sensitivity analyses; do not remove an oversized impact task to advertise a win.

Document promotion criteria from the frozen contract and uncertainty due to small samples. Historical telemetry can select cases but cannot count as fresh measured samples. Add exact tests for duplicate outcome refusal, mismatched commit, incomplete agent set and semantic fallback misclassification.

**Step 4:** `cargo check -p xtask-eval`, then exact RED command. Lead verifies positive/negative/rank ties and strict outcome joins.

**Step 5:** Hand verified diff to lead, no worker commit. **Acceptance:**
- [ ] Deterministic scoring independent of either product's IDs; every exclusion/unknown is visible.
- [ ] Missing episodes cannot produce a complete or passing task report.
- [ ] Model/default recommendations follow frozen rules and retain raw evidence.

### E4: Expose the workflow and run the acceptance campaign

**Files/ownership:** E4 row. **Interfaces:** CliCommand::Revival and the four commands specified above; JSON/Markdown reports. **Contract inputs:** reviewed E1-E3 plus reviewed product builds. **Serialization required:** Yes; integration and performance campaign are serial.

**Step 1, RED:** Add `revival_cli_rejects_unvalidated_run_manifest` using the real parse/dispatch path with an invalid manifest outside fixture roots. Require nonzero result and no product subprocess invocation. A fake executable sentinel file proves the process did not start. Preserve tests for existing search-matrix/eval commands.

**Step 2:** `cargo nextest run -p xtask-eval --lib revival_cli_rejects_unvalidated_run_manifest`. Expected failure before wiring validator-first dispatch.

**Step 3, implementation:** Add the new enum/parse path and dispatch into focused modules. Validation must finish before any preparation/execution. CLI JSON status distinguishes invalid, incomplete, completed and failed; completed means all required samples exist, not that Julie won. Reports link raw evidence by content hash.

Run lead gates, then freeze executable/source/config hashes. First run deterministic retrieval and lifecycle cases; fix any correctness gates in the relevant product plan before continuing. Coordinate agent episodes with the owner's approved worker tools using the immutable prompts, fresh context and seeded order. Import their actual outcomes. Run controlled resource samples when the machine is idle; record pre-existing load and reject contaminated samples under the frozen exclusion policy, retaining them in the audit log.

If an agent runner or target hardware is unavailable, complete the harness but mark that experiment incomplete with the missing access named. Do not mark the overall comparison complete until the required episodes exist. No tuning on held-out failures; fixes selected from that set require a new untouched confirmation set for a final superiority claim.

**Step 4:** `cargo check -p xtask-eval`, exact RED command, lead gates, then the documented four-command workflow on real inputs.

**Step 5:** Apply commit mode. **Acceptance:**
- [ ] CLI workflow works independently of an interactive MCP session; original eval commands preserved.
- [ ] Required deterministic and agent episodes have actual source-bound evidence.
- [ ] Safety failures fixed or explicitly block readiness; performance/quality trade-offs reported honestly.
- [ ] Root Codex reviews methodology, raw outliers and per-task outcomes before any winner/default claim.

## Final review and handoff

Provide implementation commits, worktree status, ledger, frozen manifests, executable/model identities, raw evidence directory, capability ledger, complete result report and outcome import audit. Root Codex reviews both code and conclusions. No model promotion, Python deletion, push or release is authorized by passing this plan.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
