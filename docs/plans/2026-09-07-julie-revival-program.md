# Julie revival implementation program

> **For agentic workers:** Execute only the plan assigned by the coordinator, using razorback:subagent-driven-development or razorback:executing-plans as that plan specifies. Root Codex is the final implementation reviewer. This file is the dependency and acceptance contract for the program; task-level implementation instructions are in the linked plans.

**Goal:** Restore a current, full-function Julie with a flexible CLI and stateless-MCP support, safe concurrent worktrees, optional qualified native semantics, and a fair comparison against Miller.

**Architecture:** Keep canonical extraction upstream, source representation and retrieval in Julie, and inference behind a provider boundary. CLI and MCP share request execution. Workspace/index/model lifetime is independent of an individual request, while each request explicitly identifies its workspace and limits.

**Tech Stack:** Rust, current reviewed julie-extractors, Tantivy, SQLite, rmcp 3.x/MCP 2026-07-28, retained Python and selectable native semantic providers.

**Architecture Quality:** High risk at syntax, request, process ownership and vector-generation boundaries. Plans establish these contracts before parallel implementation. Workers cannot replace a hard feature with a stub, silently shrink language coverage or declare success from a compiling dependency bump.

## Global Constraints

- The user requested plan files in the main checkout paths. This planning session does not execute the plans. Implementation workers inventory worktrees and select an intentional task branch/worktree before production edits, preserving existing unrelated state.
- The owner will have the upstream extractor plan implemented first. Do not start Julie's dependency migration against an unreviewed API or guessed future tag.
- Final implementation review belongs to root Codex, not to the worker that produced the change. External-model review is not implicitly requested; no paid reviewer/service is authorized by this plan.
- CLI flexibility is required, including complete tool access, structured inputs/outputs, discoverable schemas and autonomous subprocess verification without interactive MCP registration/restarts.
- MCP target is the official date-versioned `2026-07-28` protocol, not a guessed SDK version named MCP 2.0. The request-engine plan pins the verified rmcp release and preserves intended legacy interoperability.
- Source edits, indexing ownership, protocol request lifetime and model lifecycle are separate responsibilities. Protocol statelessness does not make source mutations idempotent or require model/index recreation per request.
- Keep semantics and Python available until measured replacement supports the product decision. Native integration is required as an alternative; default promotion and Python deletion require an explicit owner decision after evaluation.
- No continuous testing. No global daemon revival, remote HTTP deployment, hosted service, new language-specific ranking shortcuts, or unrelated Miller implementation changes.
- All advertised languages are peers. Classify each capability as implemented or positively verified not applicable; do not substitute a handful of convenient languages for complete coverage.
- Preserve original byte spans, CRLF, Unicode, path containment, fresh edit preconditions and precise error evidence. Never turn parser failure into unsafe text replacement.
- No push/release/publication/destructive cleanup or overwriting unrelated changes without authorization. Plan completion does not approve a release.

## Upstream release update

U1 is implemented and published as [v2.41.0](https://github.com/anortham/julie-extractors/releases/tag/v2.41.0), peeled commit `a71e18c1a6fae67b15d2d0aaa20793330fda901f`. Its public syntax and relationship contracts match the downstream plan. See [verification scope](../findings/2026-09-07-julie-extractors-v2.41.0-readiness.md). J1 now has a concrete released input; Julie production dependencies remain v2.34.3 until its worker implements the atomic migration. The earlier baseline/proposed-release language is historical context, not a requirement to create another upstream release.

## Execution order

| Order | Plan | Deliverable | Must be accepted before |
|---|---|---|---|
| U1 | [Extractor syntax API](../../../julie-extractors/docs/plans/2026-09-07-julie-consumer-syntax-api.md) | Supported syntax/tree/diagnostic and bounded parse contract; reachable public relationship types; default-feature compatibility | J1 |
| J1 | [Julie extractor migration](2026-09-07-julie-extractor-migration.md) | One reviewed upstream dependency identity; owned source bodies; retained facts/receiver evidence; safe editing and reindex behavior | J2 |
| J2 | [Request engine, CLI and MCP](2026-09-07-julie-request-engine-mcp-cli.md) | One guarded executor; complete CLI; modern direct requests and intended legacy wire behavior | J3 |
| J3 | [Workspace lifecycle](2026-09-07-julie-workspace-lifecycle.md) | Transferable index ownership; safe edits from all sessions; bounded jobs; recovery and durable continuations | J4 and full-product J5 |
| J4 | [Native semantics](2026-09-07-julie-native-semantics.md) | Selectable native runtime, complete encoder identity, compatible vectors, late readiness and recovery | Native qualification in J5 |
| J5 | [Head-to-head evaluation](2026-09-07-julie-head-to-head-evaluation.md) | Frozen source-grounded corpus, real process runner, actual agent outcomes, quality/resource/recovery report | Any winner/default-promotion claim |

J5 harness/schema work may proceed after J2's interface is frozen, independently of J3/J4, provided its owner touches only xtask-eval and evaluation fixtures/docs. Running full-product comparisons waits for all relevant dependencies and a controlled machine. All cargo test commands remain serialized.

U1's release handoff means a reviewed clean commit or an approved published tag containing the complete API. A public tag/push still requires owner authorization. If the owner prefers consuming a reviewed commit SHA temporarily, J1 must record that exact SHA in every manifest and adapt the engine-identity guard accordingly; no moving branch pin. Never invent the version number that the next upstream release will use.

Native sidecar prerequisite work is deliberately not a hidden Julie edit. Its two existing unmerged fixes must be reviewed/reconciled by that repository's owner; the accepted artifact includes source/binary/model identity. If further sidecar defects are found, send a bounded issue/reproduction to that workstream and obtain its reviewed artifact. Do not silently pull unrelated worktree content into a build.

## Cross-plan contracts and their owners

| Contract | Owner | Consumers and invariant |
|---|---|---|
| `julie_extractors::syntax` and root relationship exports | U1 | J1 uses public APIs only; bounded parse options thread through request cancellation, host/embedded limits remain explicit |
| Julie-owned symbol source projection | J1 | Retrieval, embedding, storage and editing use the same extracted source snapshot and checked spans |
| ToolRequest/ToolReply, WorkspaceBinding and request deadline | J2 | J3 replaces runtime construction without another dispatch implementation; J4 extends semantic readiness; J5 invokes actual public contracts |
| Source-edit coordination namespace | J3 | All source writers share a canonical-root lock even across shared/standalone index modes and different homes; index ownership remains per physical index |
| Writer proof and coherent published revisions | J3 | Every canonical/derived writer has authority; J4 never publishes vectors for stale source or old encoder generations |
| SemanticMode/SemanticRequirement and readiness | J2, extended by J4 | Off is zero work; Required means provider plus operation-required current compatible vectors; Auto labels degradation |
| Encoder identity and per-call embedding budget | J4 | Every real provider, fake provider and caller migrates atomically; no cached vectors silently cross identities |
| Frozen experiment and artifact identity | J5 | Raw outcomes and reports are tied to exact source/binary/config; no partial experiment advertised as complete |

Exact names and types live in the owner plan. If a reviewed implementation changes one of these contracts, update the owning plan and affected downstream plans before dispatch, then have root Codex review the delta. This is a source-grounded reconciliation step, not permission to redesign during a worker task.

## Worker startup packet

Every dispatched worker receives:

1. Absolute implementation worktree path, branch and starting commit, plus existing dirty-state inventory.
2. Its plan path and task IDs, exact owned files, prerequisite commit IDs and the frozen public interfaces it consumes.
3. Applicable AGENTS.md instructions and the requirement to use indexed exploration, inspect/trace/impact before changes, and preview edits.
4. The exact RED/GREEN tests with package selectors, the invariant each test proves, and its test-run ceiling. Missing symbols can be initial contract RED; environment/download errors are not RED evidence.
5. Commit mode from that plan. Shared-file task registration and broad verification belong to the coordinator.
6. The requirement to preserve tests and complete all actionable in-scope defects, including failed safety/recovery scenarios. A failing or skipped gate is not a completed task.

Routine safe choices and fixes do not require permission. A real prerequisite mismatch, unavailable platform access, incompatible product decision, release boundary or risk to unrelated user changes does. Questions identify the exact missing decision, after completing independent safe work.

## Verification Strategy

**Project source of truth:** Each repository's AGENTS.md and test runner docs. Upstream uses its own xtask/cargo contract/certification rules; Julie uses its own calibrated tiers and package selectors. Do not copy one repository's tier command into the other.

**Worker red/green scope:** Exact tests listed by each task, one test process at a time. New fixture/test modules must actually register in the owning package and select at least one case. A zero-test pass is failure of the gate.

**Worker ceiling:** No worker-owned dev/full/system/dogfood sweeps. Coordinator owns coherent-batch and branch gates. Do not spend the machine on repeated unchanged broad tests.

**Lead affected-change scope:** Per-plan changed/targeted tier. OverBudget is a selection decision, not pass. Record escalation and run the required scope.

**Branch gate:** Per-plan fmt/check, dev and specialized gates. Startup/workspace changes require system/reliability coverage; search/body/vector-reader changes require dogfood. Upstream optional-feature tests must run explicitly because default-feature gates alone omit them.

**Security scope:** Upstream declares cargo deny; Julie has no separate declared security tier in these plans. This is not a claim of a complete security audit. Endpoint permissions, path safety, conflict preservation, bounded untrusted protocol frames and no shell interpolation remain mandatory implementation checks.

**Replay/metric evidence:** Wrong workspace, lost acknowledged edits, false freshness, mixed encoders, unbounded hangs, malformed output or absent required features block readiness. Performance and relevance differences are reported with provenance; do not tune thresholds after viewing held-out outcomes.

**Escalation triggers:** Platform-specific OS locks/replacements require actual platform runs; unsupported hardware lanes remain explicitly unverified. A missing physical runner cannot justify claiming cross-platform full-function performance.

**Assigned verification failure:** Fix source-rooted defects in the responsible plan, preserving acceptance criteria. Record the original failure and final proof. Review scope changes with root Codex.

**Verification ledger:** Every plan has an initially empty ledger. Record invariant, command, scope, SHA, UTC result and evidence reuse. Also record dirty-diff hash, binary hash and environment identity where needed. Same HEAD with changed working content invalidates evidence reuse.

## Parallel Execution Contract

| Workstream | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| U1 | None - serial | Upstream plan ownership only | Yes | Public API prerequisite for J1 |
| J1 then J2 then J3 then J4 | None - serial across plans | Exact task tables in each plan | Yes | Shared symbol, handler/runtime and semantic contracts |
| J5 harness after J2 contract freeze | Evaluation lane | xtask-eval, fixtures/revival-eval, evaluation docs only | No | None - safe parallel batch. Does not mutate product implementations |
| J5 actual benchmark campaign | None - serial | Isolated run roots and reviewed binaries | Yes | Controlled resource measurements and completed prerequisites |
| Root Codex review | Read-only alongside workers where useful | Review notes; fixes assigned to owning task | No | None - safe parallel batch. No competing edits or test runs |

## Required handoff to root Codex

Workers/coordinators provide a compact summary plus linked artifacts, not an unsupported completion declaration:

- Paths, branch/HEAD, all related worktrees and dirty states; commits/diff ranges and unmerged prerequisites.
- Exact dependency/source/binary/model identities and complete changed-file inventory.
- Completed task checkboxes with RED/GREEN evidence and fresh broad gate ledger rows.
- Source/control edit outputs, language/feature ledger, CLI/MCP parity table and raw subprocess failure/recovery traces relevant to the plan.
- Deliberate limitations and failed/unrun gates. Report hardware absence honestly; do not hide it as a skipped success.
- Recommended review focus, rollback procedure and any temporary compatibility route that remains.

Root Codex verifies source against requirements, reviews compiler/test selection and outcomes, challenges failure handling, and asks for fixes until the assigned plan is complete. Final review occurs again on the integrated program tree because individually accepted plans can interact. No implementation is considered finally accepted solely because a worker marked every checkbox.

## Integrated readiness checklist

- [ ] U1 public contracts and default-feature compatibility are reviewed and the exact upstream identity is recorded.
- [ ] J1 preserves full facts, bodies and existing safe edit operations across supported language entries, with automatic index invalidation/recovery.
- [ ] J2 exposes every tool/parameter via CLI and handles modern no-handshake and intended legacy MCP through the same guarded executor.
- [ ] J3 handles concurrent same-root and multiple-worktree agents, owner loss, source conflicts, partial edit recovery, projection consistency and bounded work.
- [ ] J4 provides full native semantic operation with verified identities/current vectors and same-runtime recovery, retaining the Python comparison path.
- [ ] J5 contains actual held-out retrieval/task outcomes and correctly attributed resource/freshness/recovery evidence; no winner presumed.
- [ ] Current user documentation matches implemented commands/storage/model support and no longer describes this reviewed revival as permanently retired. Apply these docs changes only when the corresponding behavior is real.
- [ ] Root Codex has reviewed the integrated code and closed actionable findings, and the coordinator has run the narrowest required final integration gates on the exact reported tree.
- [ ] Any remaining release/push/model-promotion decision is explicitly separated from implementation acceptance.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
