# Julie revival hardening and replacement roadmap

**Status:** Proposed plans for owner review. Planning is authorized; implementation and publication have not started.

**Source:** [Replacement assessment](../findings/2026-09-11-julie-miller-replacement-review.md), reviewed against Julie `eabf93cb`, Miller `3ed3920b`, and julie-plugin `db4dd894`.

**Goal:** Make Julie reliable and easy to install, then decide Miller retirement using completed-task evidence and explicit capability decisions.

## Sequence and deliverables

| Plan | Deliverable | Dependencies | Exit decision |
|---|---|---|---|
| [1. Correctness](2026-09-11-revival-correctness-plan.md) | Fresh indexes, independent request semantics, recoverable partial embeddings | None | Required before release qualification |
| [2. Retrieval contracts](2026-09-11-revival-retrieval-plan.md) | Exact reference evidence, useful scoped content search, replayable pages, honest body completeness | Can develop alongside 1; integrate after 1 | Required before navigation promotion |
| [3. Runtime lifecycle](2026-09-11-revival-runtime-plan.md) | One production lifecycle, independent cold initialization, bounded idle retention | 1; measurement before optimization | Required before multi-checkout qualification |
| [4. Deployment and guidance](2026-09-11-revival-deployment-plan.md) | Consistent instructions, safe versioned installation, verified packages | Guidance and launcher work can start early; full qualification needs 1–3 | Release candidate, not permission to publish |
| [5. Replacement qualification](2026-09-11-revival-qualification-plan.md) | Current paired measurements, real agent tasks, capability decisions and retirement verdict | Experiment contract can start now; runs need 1–4 | Owner chooses navigation default and eventual Miller retirement separately |

Default execution order is 1, 2, 3, 4, 5. Use parallel editing only where the exact ownership tables permit it. Run one Cargo command at a time across the team. Each plan leaves a working, independently reviewable commit series and its own evidence ledger.

This work takes priority over the previously queued general cleanup and dashboard expansion. Runtime consolidation is the narrowly justified cleanup. Keep dashboard redesign and broader troubleshooting UI outside these plans; expose required readiness/error evidence through existing status paths.

## Architecture and scope decisions

- Preserve the approved [machine-service design](2026-09-09-machine-service-design.md): one service per machine, one writer per checkout, `facts.sqlite` plus Tantivy, one machine registry, explicit absolute workspace path or registered ID on scoped MCP calls.
- Preserve the OS-held service singleton and existing per-workspace mutation gate. No PID election, leases, fencing, brokers, new durable coordination stores, derived-index migrations, or live service handoff.
- **Narrow recovery exception:** Plan 1 restores the already-advertised `needs_rescan` behavior through the active checkout-store path and existing mutation gate. It does not revive the old parallel repair architecture. This records the explicit design exception required by section 4 of the machine-service design.
- **Narrow lifecycle synchronization exception:** Plan 3 may use one per-key in-memory slot for initialization and teardown while releasing the existing global map guard before expensive work. Existing request Arcs retain request ownership; cooperative task joins prove writer quiescence. It replaces the current critical section and incomplete cancellation, adding no owner election, public leases or durable state. Tiny empty-slot retention is measured and disclosed separately from bounded resident runtimes.
- Stateless paging means arguments plus offsets or source ranges. No durable continuation pages, expiry jobs, or cross-request retained snapshots. Report snapshot/source changes honestly.
- Preserve current tool names. The existing phase-6b naming decision remains after sufficient real telemetry, no earlier than approximately 2026-09-25. Added correctness fields are permitted; a wholesale tool rename is not part of this program.
- The assessment's bridge claim is corrected: `call_path(mode="web")` exists. Improve its precision and measure provider coverage; do not build a second traversal subsystem.
- Reuse source metadata and generic contracts. Never route or score by assuming `docs/`, `src/`, Rust, or three favored languages.
- Enumerate the pinned extractor's actual language/capability inventory when qualifying precision or adding intelligence. Record implemented or verified-not-applicable for each concept, with source evidence. Do not confuse the older “36 languages” prose with the current canonical inventory, or mark missing support not-applicable.
- No additional dependency unless existing code and the standard library cannot satisfy a concrete accepted requirement.

## Common execution contract

These are worker implementation briefs with exact owned paths, observable behavior, and regression scenarios. Astra owns design, integration, and final review; `sol_worker` owns implementation and diagnostics. Use `razorback:subagent-driven-development` after implementation authorization. Workers are not alone and must preserve unrelated edits.

Before execution, inventory worktrees and refresh the referenced symbols with Julie. Reuse the current task worktree where appropriate; do not evade uncommitted state. Create task worktrees only when implementation begins. The planning checkout currently holds the assessment, these plans, and unrelated user config/memory changes. Move task artifacts deliberately if they must follow a worktree; do not strand them or stash them away.

**Commit mode:** `parallel-lead-commit`. Workers hand verified owned changes to Astra; Astra reviews, stages only intentional files plus task memory, and commits. After implementation approval, local commits are authorized within scope. Push, force-push, tag, release, installation into live user profiles, or service replacement remain separately approved actions. External reviewer defaults to none.

**TDD:** Every defect begins with a deterministic failing behavioral test. The test names in the plans are proposed, not claims that tests already exist. Reuse the named fixture helpers. Verify RED, implement the smallest root-cause fix, run `cargo check`, then verify GREEN. Each new test receives one RED and one GREEN run; a failure starts diagnosis, not blind retries. Use barriers, controlled inputs, and existing in-memory seams rather than sleeps. Workers report any source evidence that refutes an assessment finding before changing code.

## Common verification strategy

**Source of truth:** Current `AGENTS.md`, `docs/TESTING_GUIDE.md`, `xtask/src/runner.rs`, and [ledger template](verification-ledger-template.md). The current tiers supersede old plan references to `fast` and `system`.

- **Worker red/green:** `cargo nextest run -p <owning-package> --lib <exact_test_name>`. Root package is `julie`; crate tests use their owning package such as `julie-runtime` or `julie-tools`. Never use an unfiltered library run. Check the owning test target before running; new target names in these plans are explicitly marked proposed.
- **Worker ceiling:** Exact test only. No worker runs `dev`, `dogfood`, or `full`; lead serializes all Cargo activity. Preserve exit codes when piping output.
- **Lead affected-change:** `cargo xtask test dev` once per completed coherent batch. After search, ranking, graph, or index-freshness changes, also run `cargo xtask test dogfood`.
- **Branch gate:** `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo xtask test full` before merge. Reuse only exact-SHA, matching-scope evidence; do not run dev/dogfood immediately before full if full supplies the same required branch check.
- **Plugin:** `node --test hooks/*.test.cjs` in the chosen plugin worktree. New launcher tests use Node's existing test runner, with one exact-name RED/GREEN cycle before the full plugin gate.
- **Security scope:** None declared for a blanket scan. Plan 4 requires targeted archive-path, authentication, token-output, and upgrade checks. Do not claim a dependency/CVE audit was run.
- **Live evidence:** Isolated `JULIE_HOME`, temporary profiles, representative fixture checkouts. No writes to the maintainer's live registry or installed plugin. Native checks and real model downloads are outside unit-test timings; missing machines/access are reported as unverified gates, not replaced with mocks.
- **Assigned failures:** Diagnose and fix within scope; do not weaken assertions, classify regressions as known failures, or silently skip an unavailable gate.
- **Ledger:** Every plan starts with an empty ledger. Record command, invariant, scope label, exact SHA, timestamp, result, and evidence location. Do not copy historical passes into new execution ledgers.

## Assessment coverage

| Assessment item | Owning task |
|---|---|
| Missing watcher recovery, dropped second save | 1A |
| Concurrent semantic mode race | 1B |
| Partial vectors after interruption, misleading readiness | 1C |
| Reference identity/span loss | 2A |
| Document/content retrieval | 2B |
| Paging loses request arguments | 2C |
| Bounded inspection and complete-value evidence | 2D |
| Duplicate inactive lifecycle, global cold-start lock, retention | 3A–3D |
| Workspace management guidance, stale instruction delivery | 4A |
| Old executable fallback and explicit upgrade | 4B |
| Cold/offline semantics and native prerequisites | 4C |
| Artifact verification and awaited plugin publication | 4D |
| Fresh native/harness installation qualification | 4E |
| Small/stale benchmark evidence and whole-task quality | 5A–5C |
| Symbol refactors, bridge, external content, continuous tests | 5D decision briefs |

## Miller capability decisions

These are scheduled deliverables, not silently promised feature ports. Plan 5D must produce a disposition and evidence for each before recommending full Miller retirement.

1. **Symbol operations:** Prefer body/signature replacement using existing edit preview and validation; evaluate exact multi-file rename after reference precision. Record transaction/conflict limits and all-language applicability before approving implementation.
2. **Bridge traversal:** Qualify existing web/SQL traversal, including a symbol containing both matched and unmatched calls and correct per-call provenance. Compare actual supported providers with Miller before proposing additions.
3. **External content:** Evaluate the current file-import workflow against Miller's bounded content tools. A new machine-wide corpus requires an explicit storage/trust design because it would change the approved durable-root model. Begin with owned ordinary workspace files if they meet the use cases; browser acquisition remains outside core.
4. **Continuous testing:** Preserve the separate design decision. Establish demand, opt-in behavior, process execution boundaries, runner support, and resource budget before implementation. Absence of a runner is not evidence that a language cannot be tested.

Full Miller retirement requires every needed capability implemented or explicitly waived by the owner. Navigation-default promotion can happen earlier with a documented retained-tool list. Silence never waives a capability or authorizes publication.

## Completion of this planning task

- [x] All five plans have source-grounded ownership, test scenarios, dependencies, and exit criteria.
- [x] Independent Sol challenges have been reconciled by Astra, including a scoped confirmation of lifecycle and correctness changes.
- [x] Links, proposed-versus-existing paths, and instruction consistency have been checked.
- [x] Brief and checkpoint point to the roadmap without declaring implementation complete.

## Verification ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
