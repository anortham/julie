# Julie Improvement Roadmap Design

**Status:** Direction approved; written design awaiting approval before implementation planning.

## Purpose

Improve Julie in four ordered areas: durable projection observability, oversized-module boundaries, mixed relationship/web traversal, and a quiet reproducible macOS toolchain. Each phase must leave Julie shippable and independently verifiable.

## Goals

- Report durable freshness for every production projection, beginning with `tantivy` and `web_edges`.
- Reduce the four touched implementation files above 500 lines without changing their public behavior during the split.
- Extend opt-in impact traversal across ordinary relationships and web edges only after an evaluation corpus proves the semantics.
- Remove the macOS object-version linker warning without raising Julie's minimum supported macOS version.

## Non-Goals

- Do not revive the detached in-process dashboard architecture.
- Do not add language-specific traversal rules; traversal consumes canonical relationships and web edges already emitted by the language-agnostic indexing pipeline.
- Do not combine module moves with behavior changes.
- Do not raise the minimum macOS deployment target to silence a local warning.
- Do not change default navigation or impact output when web traversal is not requested.

## Architecture Quality

**Affected modules:** `src/health/**`, the compile-only dashboard health stub plus its contracts and templates, `crates/julie-index/src/search/index.rs`, `crates/julie-runtime/src/watcher/runtime.rs`, `xtask/src/{changed,runner}.rs`, `crates/julie-tools/src/impact/**`, and macOS build configuration. Phase 1 also audits the existing web-edge writers in `crates/julie-pipeline/src/indexing_core/persistence.rs`, `crates/julie-runtime/src/watcher/handlers.rs`, `src/tools/workspace/indexing/index.rs`, and `src/startup.rs`; the startup repair flow is in scope because one repair branch currently skips web-edge reconciliation.

**Caller-facing interfaces:** system health output, dashboard health JSON/template data, `SearchIndex`, watcher runtime entry points, xtask CLI output, and opt-in impact traversal.

**Depth/locality check:** projection-state interpretation belongs in one health module; each oversized facade retains its existing public surface while implementation moves behind focused submodules; mixed traversal remains inside the impact graph walk.

**Test surface:** system-health and dashboard snapshots for projection state, existing public APIs for module splits, caller-visible impact results for traversal, and a real macOS build/test command for toolchain validation.

**Seams/adapters:** one generic `ProjectionHealth` output contract is justified by two durable projections. Its interpreter accepts projection-specific policy instead of pretending the projections have identical mechanics: stable name, optional physical-readiness probe, revision-fallback policy, and detail wording. No compatibility adapter is needed because Julie does not promise backward compatibility for its stdio tool contract.

**Rejected shortcuts:** duplicate `search_projection` and `web_edges_projection` structures, treating web-edge lag as search unavailability, resurrecting detached dashboard state, behavior changes during file splits, recursive traversal without evaluation evidence, and raising the deployment target.

**Architecture risk:** high overall. Projection health is medium risk, module splits range from low to high, mixed traversal is high risk because false links damage agent decisions, and toolchain work is medium risk because it affects supported builds.

## Phase 1: Projection Freshness

### Contract

Replace the Tantivy-specific health shape with a generic projection record:

- `name`: stable projection name (`tantivy` or `web_edges`).
- `level`, `state`, `freshness`, `workspace_id`.
- `canonical_revision`, `projected_revision`, `revision_lag`.
- `repair_needed`, `detail`.

`DataPlaneHealth` exposes a deterministic `projections` list ordered as `tantivy`, then `web_edges`. Readiness continues to depend on canonical SQLite plus the Tantivy projection. A lagging `web_edges` projection degrades overall health and requests repair, but it does not make lexical/search readiness unavailable.

The generic record does not make projection mechanics generic. Durable-row interpretation is shared, while a small projection policy supplies the stable name, whether a physical-readiness probe is required, whether a legacy `Ready` row may fall back from `projected_revision` to its stored canonical revision, and projection-specific detail text. Tantivy receives the live in-process index signal in system health. `web_edges` has no physical sidecar and relies on its durable row.

`web_edges` lag is anomalous rather than an expected idle state: force rebuild, incremental scan, single-file replace/delete, workspace indexing, and watcher create/update/delete already rebuild the projection and stamp its revision. Startup reconciliation already repairs it when no other repair plan exists, but a `MissingEmbeddings`-only plan can bypass that branch. Phase 1 makes projection reconciliation run after every successful startup repair outcome and adds regression coverage for every ownership path before allowing web-edge lag to affect overall health.

Projection reconciliation is a writer and must run under the workspace mutation gate. The repair path threads a `&MutationGuard<'_>` proof token into reconciliation when it already owns the gate. If first-run repair began before a workspace identity existed, it resolves the new identity and acquires that workspace's gate before reconciling; it never performs the post-repair projection writes ungated.

### State Rules

- An unbound or empty workspace reports projections as unavailable without requesting repair.
- A non-empty workspace with missing canonical revision metadata is degraded, but the repair owner is the canonical store. Projection rebuild must not claim it can repair missing canonical metadata; health detail identifies the blocking metadata repair explicitly.
- `Ready` with equal canonical and projected revisions is current.
- A lower projected revision is lagging.
- Missing, stale, or otherwise non-current durable state requires rebuild.
- Tantivy additionally requires its physical index-ready signal; `web_edges` relies on its durable projection row and revision stamp.

### Dashboard Boundary

The current dashboard projection reader is detached and always returns unavailable. Phase 1 mechanically updates that compile-only stub, its serialized contract, and its template for the new list shape, but it remains unavailable and gains no live dependency.

Dashboard completion is tied to the existing standalone-dashboard restoration. Durable-row interpretation is shared, while physical readiness is an injected probe: the live system-health path uses its in-process Tantivy handle and the standalone reader uses an on-disk index check. The standalone reader opens each workspace's `symbols.db` and renders the same deterministic projection records without inventing a second freshness interpretation.

### Acceptance

- System health reports both projections and their revisions.
- Lag in either projection degrades overall health and names the stale projection.
- Search readiness remains fully ready when Tantivy is current even if `web_edges` lags.
- Startup repair restores both projections to current after every successful repair-plan outcome when canonical revision metadata exists, including a `MissingEmbeddings`-only plan with no file changes.
- Projection reconciliation cannot be called without a `MutationGuard<'_>` proof token once a workspace identity exists.
- Every canonical mutation path either stamps `web_edges` current or has a regression test proving startup reconciliation closes the gap.
- The eventual standalone dashboard renders the same durable projection records rather than a second interpretation.

## Phase 2: Focused Module Boundaries

Execute four behavior-preserving plans in increasing product risk:

1. Split `xtask/src/runner.rs` into prebuild derivation, command execution, and summary rendering behind the existing runner interface.
2. Split `xtask/src/changed.rs` into diff mapping, selection/budget policy, and output rendering behind the existing changed-selection interface.
3. Split `crates/julie-runtime/src/watcher/runtime.rs` into batch commit, repair-state handling, event processing, and runtime state while retaining the watcher runtime facade.
4. Split `crates/julie-index/src/search/index.rs` into schema/compatibility, open lifecycle, writer mutation, and query execution while retaining `SearchIndex` as the public facade.

Every new implementation file must stay at or below 500 lines. A split task fails if caller-facing behavior, serialized output, CLI output, or concurrency semantics change. Each split receives its own impact analysis and verification ledger; search-index work runs last because it has the broadest caller surface and requires dogfood verification.

## Phase 3: Evaluated Mixed Traversal

Build the evaluation corpus before production behavior. It must cover:

- Ordinary caller to HTTP client to matched backend handler.
- Backend handler to ordinary downstream call.
- SQL query to uniquely resolved table.
- Ambiguous HTTP or SQL targets remaining external and terminal.
- Cycles, self-calls, duplicate edges, depth limits, and deterministic ordering.
- Multiple existing framework/language families without adding language-specific traversal branches.

The traversal algorithm uses one typed breadth-first walk over ordinary relationship edges and internal web edges. Seed symbols begin in the visited set, external targets are terminal, duplicate symbol candidates are emitted once at their shortest distance, and stable ordering breaks equal-score ties. Web edges remain opt-in. `max_depth` applies to the combined graph, not separately to each edge family.

Promotion requires every expected internal path in the curated corpus, zero unexpected internal symbol links, unchanged output with web traversal disabled, and bounded latency recorded against the existing impact baseline. Recall and latency beyond those hard assertions are reported for judgment rather than hidden behind a pass label.

## Phase 4: macOS Toolchain Consistency

Reproduce the warning under the current Homebrew Rust toolchain and a current rustup-managed toolchain using the same narrow test binary. Inspect project Cargo linker configuration, environment deployment-target inputs, and CI toolchain setup before changing configuration.

Prefer a repository-pinned rustup toolchain and documented linker setup if it removes the warning while preserving the current deployment target. Establish that pinned formatter as the canonical baseline and normalize the existing repository-wide rustfmt drift once, rather than letting local toolchains generate recurring mechanical diffs. Do not mask linker messages globally. Acceptance requires a warning-free narrow test, a clean `cargo fmt --check`, and unchanged Linux and Windows CI configuration semantics.

## Doubt Pass Resolution

A read-only hostile review was checked against the live index before planning:

- Accepted: the shared interpreter must expose projection-specific readiness, revision-fallback, and wording policy rather than embedding Tantivy semantics in the generic record.
- Accepted: the standalone dashboard needs an injectable physical-readiness probe; a database-only reader cannot reuse the current in-process boolean unchanged.
- Accepted: Phase 1 must mechanically migrate the detached dashboard stub so the type replacement remains compile-safe.
- Accepted: missing canonical metadata is a canonical-store repair condition, not something a projection rebuild can promise to fix.
- Accepted: projection reconciliation must run after successful startup repair plans as well as the no-plan branch; otherwise a `MissingEmbeddings`-only repair can leave `web_edges` lagging.
- Accepted: the new post-repair reconciliation must preserve mutation-gate serialization. Existing bound-workspace repair already owns the gate; first-run repair must acquire the newly resolved workspace gate before reconciliation, and the reconciliation helper receives the proof token.
- Rejected: `web_edges` is not lazily maintained only at traversal time. Current persistence, watcher, workspace-indexing, and startup paths rebuild or ensure it and stamp the current revision.
- Rejected: pending-relationship resolution does not invalidate a stamped web-edge projection. It neither advances canonical revision nor supplies the structural facts from which web edges are derived.
- Rejected: `get_current_canonical_revision` and `get_latest_canonical_revision` do not represent competing revision sources; the former delegates directly to the latter and extracts its revision number.
- Rejected: the watcher workspace-ID fallback does not establish a current key-divergence bug. `generate_workspace_id` follows the same path normalization as binding and its normalization helper currently returns `Ok` for every input.

## Delivery Sequence

1. Commit the completed review remediation as its own checkpoint.
2. Create an isolated `codex/` worktree for this roadmap.
3. Write and approve a separate implementation plan for Phase 1, then execute it as the first vertical slice using TDD.
4. Execute each Phase 2 split under its own approved behavior-preserving implementation plan.
5. Write and approve a Phase 3 evaluation-and-traversal plan; build and review the evaluation corpus before enabling mixed traversal.
6. Write and approve a Phase 4 diagnosis plan; change toolchain configuration only after live reproduction identifies the warning source.

This roadmap is the ordering and architecture contract, not one oversized executable plan. Each phase has its own review and approval gate so later work can incorporate evidence from earlier phases.

Each phase runs `cargo check`, the narrowest exact tests during RED/GREEN, `cargo xtask test changed` after a coherent batch, and `cargo xtask test dev` before handoff. Phase 1 adds `cargo xtask test system`; Phase 3 adds `cargo xtask test dogfood`; the `SearchIndex` split adds both dogfood and broad pre-merge verification.

## Roadmap Acceptance Criteria

- [ ] Both production projections have one durable health interpretation with explicit projection-specific policy.
- [ ] The standalone dashboard consumes that interpretation when restored.
- [ ] The four oversized implementation files are decomposed behind stable facades.
- [ ] Mixed traversal ships only after precision and default-output gates pass.
- [ ] macOS builds no longer emit object-version mismatch warnings.
- [ ] Every phase has a verification ledger tied to its exact commit.
