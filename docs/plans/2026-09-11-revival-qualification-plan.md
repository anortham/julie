# Plan 5: Prove replacement and decide remaining Miller workflows

**Status:** Proposed. No implementation or experiment run has started.
**Goal:** Produce a defensible navigation-default recommendation and a separate full-retirement decision.
**Depends on:** Freeze workload before ranking changes where possible. Run final qualification on the integrated Plans 1–4 release candidate.
**Execution:** Follow the [roadmap contract](2026-09-11-revival-roadmap.md). This plan does not authorize spending on external models or modifying real user repositories for experiments.

## Architecture quality

Reuse `docs/eval/head-to-head/run_matrix.py`, `cases.json`, `mcp_client.py`, and the service scorecard. Evaluation has no production ranking authority. Fixtures, answers and results stay outside indexed corpora. Do not build the unimplemented September 7 `xtask-eval revival` command merely because an older plan names it. Existing tools plus a small task manifest and outcome records are sufficient. Risk is medium because biased scoring or stale service identity could support the wrong product decision.

## Ownership and ordering

| Task | Owned files/artifacts | Serialization / reason |
|---|---|---|
| 5A experiment contract | `docs/eval/head-to-head/cases.json`, proposed `docs/eval/head-to-head/agent-tasks.json`, proposed `docs/findings/revival-qualification-contract.md` | First; freeze before comparative runs |
| 5B runner correctness | `docs/eval/head-to-head/run_matrix.py`, `mcp_client.py`, `docs/eval/semantic-value/run_scorecard.py`; proposed `src/tests/eval_contract.rs` and `src/tests/mod.rs` registration | After 5A; runner must implement the frozen contract |
| 5C experiments | Existing `docs/eval/head-to-head/results/`; proposed `docs/findings/revival-replacement-qualification.md` | After 5B and integrated candidate; serial product resource runs |
| 5D capability decisions | Proposed `docs/plans/revival-miller-capability-decisions.md`, linked capability-specific design briefs only for accepted ports | Can collect usage alongside 5A; finalize after 5C |

The new root test module tests the existing Python scorer/parser through small fixtures using the repository's established script-invocation pattern. If a direct existing Python unittest target is discovered, prefer it and update the exact owned test target before implementation; do not create a second test framework.

## 5A. Freeze representative workloads and verdicts

**Inputs:** The existing ten pinned public repositories, assessment failures, and existing Miller telemetry. **Produces:** Versioned manifest, expected outcomes, inclusion rules and pass criteria written before running the candidate.

Keep the existing 23 retrieval cases as a regression set. Add held-out exact name/path, prose/document, literal/configuration, same-line references, body completeness and filter/paging cases. Represent different project layouts. Expected results may contain multiple valid paths/sites; judge each against source evidence. Do not tune the manifest after seeing which product wins.

Define 12 agent tasks, two each for: locating a defect, explaining a flow, enumerating precise references, applying a small verified edit, finding documentation/configuration evidence, and targeting independent worktrees. Each task records prompt, fixture commit, permitted changes, correctness assertions, timeout/output budget, and forbidden wrong-workspace edits. Add bridge/content/CT tasks only to the separate capability track, so missing optional features do not distort basic navigation comparisons.

Compare Julie, Miller and a raw-tools baseline in independent fresh sessions using the same agent model and prompt. Counterbalance product order across tasks. Use the existing approved native worker mechanism; do not silently buy API credits. Run products separately for resource measurements, then run a separate concurrent-agent stress scenario.

**Proposed promotion criteria:**

- [ ] Zero wrong-workspace edits, lost acknowledged edits, falsely exact references or results presented as fresh after a detected failed update.
- [ ] Julie completes at least as many core tasks correctly as Miller, with no regression in a required task class concealed by an aggregate total.
- [ ] New regression cases pass and existing retrieval misses are explained; accepted alternative files are scored correctly.
- [ ] Warm latency/output and cold readiness are reported separately. A core-task p95 regression over 20% against the matched Miller run needs explicit owner acceptance rather than automatic promotion.
- [ ] Plan 3 resource budgets and Plan 4 native/install gates pass.

These are proposed acceptance criteria in the plan, not current measurements. Full Miller retirement additionally requires 5D dispositions; navigation-default promotion does not imply every specialized tool has been replaced.

## 5B. Make the existing runner enforce the contract

**Inputs:** `score_row`, `execute_call`, `validate_manifest`, existing MCP process wrappers and current service readiness. **Produces:** Comparable, identity-bound, failure-aware results without a new harness architecture.

1. Inspect actual tool schemas and pass explicit workspace on every Julie scoped call, including inspect and setup/status calls. Verify the running service's version/source identity rather than assuming the selected shim executable determines the running server.
2. Extend expected results to accept a declared set of valid paths while preserving existing manifests. A tool error cannot pass merely because its message mentions the expected filename. Count empty structured results as empty, not only empty text.
3. Replace `--require-semantics`'s current “any vectors exist” condition with compatible eligible coverage from Plan 1. Reject partial/missing/mismatched readiness as full-semantic qualification. Standalone lexical fallback does not count as semantic evidence.
4. Record both products' source and binary identity, corpus commits, extractor/model identity, readiness, transport, warm/cold status, output size and raw responses. Rebuild/verify both products rather than comparing current Julie with an unexplained old Miller executable.
5. Ensure corpus checkouts exclude evaluation manifests, expected answers and results. Do not use output from one product to select the target for the other.

Proposed root-package behavioral tests, each exact RED/GREEN: `eval_accepts_declared_alternative_expected_paths`, `eval_tool_error_cannot_score_as_success`, `eval_rejects_partial_semantic_coverage`, and `eval_scoped_calls_include_explicit_workspace`. Test the actual runner functions with controlled JSON responses, not copied scoring logic.

Existing commands after runner updates:

```bash
python3 docs/eval/head-to-head/run_matrix.py --validate
python3 docs/eval/head-to-head/run_matrix.py --require-semantics --julie-backends auto,lexical,hybrid,semantic
python3 docs/eval/semantic-value/run_scorecard.py --service --backend lexical --backend hybrid --backend semantic
```

Supply the frozen manifest and explicit candidate binary paths using the existing `--cases`, `--julie-bin`, `--miller-bin`, and `--out-dir` flags. Do not run these commands against live maintainer homes. Record isolated-home configuration with each run.

**Acceptance:**

- [ ] Workspace, identity, encoder and coverage mismatches abort qualification rather than silently changing the workload.
- [ ] Alternative correct results pass; errors and missing evidence cannot earn success.
- [ ] Both current products participate in the kept paired run; historical Miller columns are not reused.
- [ ] Scoring fixtures and raw artifacts make every verdict auditable.

## 5C. Run product and agent qualification

Warm once, then collect three identical retrieval runs. Preserve all failures and discard only the declared warm-up, not unfavorable runs. Report per-class accuracy, correct-site counts, p50/p95, bytes, errors and semantic coverage. Resource runs record the complete process tree and stop owned processes afterward.

Execute the 12 frozen agent tasks for each product and the raw-tools baseline. Capture tool calls, shell fallback, corrections, final diff and independent acceptance results. Do not ask the same agent to self-certify its own edit. Astra checks outcomes against the frozen assertions and source; a task is not successful merely because every tool call returned success.

Run isolated lifecycle scenarios for rapid saves, overflow/failure recovery, two concurrent semantic modes, partial-backfill restart, cold checkout beside warm traffic, and eviction/reopen. Reuse Plans 1/3/4 evidence only when exact SHA and scope match. Report uncovered platforms or scenarios explicitly.

**Deliverable:** A concise recommendation with per-task evidence, remaining gaps and one of: retain Miller as default; make Julie navigation default while retaining named Miller capabilities; or recommend full retirement after owner waiver/implementation of every needed capability. Publication/retirement is an owner action, not an automatic side effect of benchmark success.

## 5D. Produce actionable capability decisions

Create the capability decision ledger with one row per item below. Each row contains observed demand, current Julie behavior, current Miller behavior, acceptance examples, all-language/provider applicability, implementation dependencies, and one recommendation: keep existing Julie behavior; extend Julie in a linked design; retain Miller temporarily; or request an explicit owner waiver. “Later” without a decision and a linked task is not a disposition.

| Capability | Concrete investigation and next deliverable |
|---|---|
| Symbol edits/refactors | Trace existing `PreparedEdit` and validation in `crates/julie-tools/src/editing/edit_file.rs` against Miller `Tools/EditTool.cs`. Draft a body/signature replacement slice first; separate multi-file rename with stale-source refusal, exact-site coverage, preview/apply parity and honest rollback limits. No port of Miller's entire edit service. |
| Bridge traversal | Compare Julie `call_path_web.rs` and graph web/SQL edges with Miller `Tools/TraceTool.cs`. Qualify matched plus unmatched calls, correct call-site attribution, dynamic routes and actual provider families. Propose only evidenced provider gaps under existing web mode. |
| External content | Replay representative logs, CI output and research tasks through existing workspace-file ingestion and Miller `Tools/ContentTool.cs`. Decide whether owned ordinary workspace files with bounded read/search/remove meet the need. A machine-global corpus requires owner approval of storage and trust boundaries. |
| Continuous testing | Measure actual `tests` usage and failures in Miller `Tools/TestsTool.cs`. Draft the separate opt-in runner/process/resource design only if demand justifies it. Discovery/status must start no processes; execution and stale-test selection are later accepted slices. |

Do not fabricate language support. For concept applicability, record each canonical language as implemented or verified-not-applicable with evidence; incomplete implementations remain visible as incomplete. For external test runners, a missing adapter is unsupported, not not-applicable. No code for these optional ports is authorized by the qualification plan alone.

## Verification and handoff

Runner code receives the normal dev/full gate. Actual benchmarks and agent tasks are `live` evidence outside unit-test timings. Preserve commands, identities, raw outcomes, and exact capability decisions in the report and Goldfish checkpoint. Update the brief to reflect the owner's decision only after it is actually made.

## Verification ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
