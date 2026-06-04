# Julie Rescue Phase 2 — ToolContext Facade + julie-tools / julie-runtime / julie-pipeline

**Date:** 2026-06-04
**Status:** Design — synthesized by an 11-agent planning workflow (6 investigate → 1 synthesize → 4 adversarial verify, ~1M tokens) and revised against 5 verified blockers. Pending user sign-off before implementation.
**Branch:** `julie-rescue`
**Supersedes (for Phase 2 specifics):** §Phase 2 of `docs/plans/2026-06-03-julie-rescue-design.md`. The parent design's high-level intent stands; this doc corrects the boundaries against the post-Phase-1 codebase (HEAD `4f2c2353`).

---

## TL;DR

Peel three more crates out of the top crate, **lowest-first**, behind a `ToolContext` facade that frees the MCP tools from the 24-field `JulieServerHandler` god-object (`src/handler.rs:191`). The win is the same as Phase 0/1: per-crate test binaries kill the relink tax for the peeled code. Phase 2 is bigger than Phase 0/1 because the tools are **handler-bound, not service-bound** — every tool entry point takes `&JulieServerHandler` today.

**Shipped as three sub-PRs:** 2a `julie-pipeline` (clean, no facade) → 2b `julie-context` + facade swap + `julie-tools` → 2c `julie-runtime`.

**One new crate beyond the parent design:** `julie-context`, a tiny leaf holding the `ToolContext` trait (it must name `SearchIndex` from julie-index, so it can't live in the pure-data julie-core leaf).

---

## 1. Target crate DAG (post-Phase-2)

```
julie-core      data leaf: database, connection_pool, embeddings_contract (EmbeddingProvider trait),
   ↑            glob, paths, test_support  + Phase-2 relocations (see §3 P0)
julie-index     search + analysis (merged, Phase 1) — unchanged
   ↑
julie-context   NEW tiny leaf: the ToolContext async trait + relocated SpilloverStore
   ↑            + ResolvedTarget / RefactorMetrics value types.  (deps: core, index)
   ↑
julie-pipeline  NEW: indexing_core + embeddings(sidecar impl) + file_policy + indexing state
   ↑            + resolve_pending_relationships + embedding-log-fields.  (deps: core, index)
   ↑
julie-tools     NEW: the read+edit MCP tools, behind &dyn ToolContext, names NO handler/daemon.
   ↑            (deps: core, index, context, pipeline)
julie-runtime   NEW: watcher + workspace + manage_workspace lifecycle commands.
   ↑            (deps: core, index, context, pipeline, tools)
julie (top)     handler, daemon, adapter, dashboard, startup, health, cli, bins.
                JulieServerHandler impls julie_context::ToolContext here.
```

`julie-context` and `julie-pipeline` are **siblings** above julie-index (neither depends on the other). The vertical layout is presentation; the real edges are in the `(deps: …)` annotations.

Acyclic chain: `core → index → {context, pipeline} → tools → runtime → julie`.

---

## 2. The `ToolContext` facade

**Decision: `#[async_trait] pub trait ToolContext: Send + Sync`, not a struct.** The three highest-traffic accessors are already async on the handler — `embedding_provider()` (`handler.rs:1282`), `primary_pooled_database()` (`:1889`), `get_search_index_for_workspace()` (`:2261`). A trait lets `JulieServerHandler` implement it in the top crate with the **existing accessor methods as the bodies verbatim**, while julie-tools depends only on `&dyn ToolContext`. A struct would have to copy/own every Arc handle and re-implement the daemon-vs-stdio branching the handler accessors already encapsulate.

**Where it lives:** the new `julie-context` crate. Putting it in julie-core would force julie-core to depend on julie-index (for `SearchIndex`), polluting the data leaf.

**Method surface (~20 methods — bigger than the synthesizer's first claim of 16):**
- identity/session (sync): `current_workspace_id`, `current_workspace_root`, `require_primary_workspace_identity`, `require_primary_workspace_root`, `loaded_workspace_id`, `is_primary_workspace_swap_in_progress`, `session_id`
- primary DB + index (async): `primary_pooled_database`, `primary_pooled_database_and_search_index`
- cross-workspace (async): `get_pooled_database_for_workspace`, `get_search_index_for_workspace`, `get_workspace_root_for_target`, **`resolve_workspace_target`** (purpose-method, see Blocker 2)
- embeddings (async): `embedding_provider`, `ensure_embedding_provider(timeout)` (replaces the nl_embeddings stdio lazy-init+wait)
- spillover/editing: `spillover_store`, `acquire_mutation_gate`
- purpose-methods that keep `DaemonDatabase` / `ManageWorkspaceTool` out of tools: **`record_refactor_metrics`**, **`ensure_target_workspace_indexed_if_pending`** (see Blockers 1–2), and `system_readiness` (default method if it truly only needs the two DB/index accessors — verify, see Blocker 5).

**Tests:** a `FakeToolContext` in `julie-test-support`, built from primitives the handler-free tests already construct (`open_test_connection` from `julie-core/test_support`, an optional `SearchIndex`, a stub `EmbeddingProvider`, a fresh `SpilloverStore`, fixed session/workspace ids). The ~498 currently handler-free tool unit tests wrap those in `FakeToolContext` and **relocate into julie-tools** — this is the relink cure. The 159 handler-instantiating tests keep the real handler (which impls `ToolContext`) and stay in the top-crate test binary.

---

## 3. Severance plan (how the back-edges get cut)

Full edge-by-edge list lives in the implementation plan. The shape:

**P0 — move shared leaves DOWN to julie-core first** (so nothing above core imports `crate::tools`):
- `tools::shared` `BLACKLISTED_*` / `NOISE_CALLEE_NAMES` const tables
- workspace leaf modules `registry.rs` (`generate_workspace_id`), `root_safety.rs`, `mutation_gate.rs` (`MutationGuard`), `startup_hint.rs` — all zero-dep
- `external_extract::paths`, `health/types.rs` state enums + `SystemStatus`
- fix `src/utils/{paths.rs,walk.rs}` to import these from julie-core, not `crate::tools` (the still-open §3.2 edges)

**Route through the facade** — swap all 36 `&JulieServerHandler` tool params to `&dyn ToolContext`; daemon_db reads become `resolve_workspace_target()` / `record_refactor_metrics()`.

**Split the indexing/lifecycle subtree:**
- pure extraction/persist/state/`file_policy` → DOWN to julie-pipeline
- session-state orchestration (`route.rs`, `index.rs`, embedding task-registration, `analyze_batch`) + `commands/**` lifecycle → UP to julie-runtime

---

## 4. The 5 blockers the adversarial pass caught

| # | Blocker | Fix |
|---|---|---|
| 1 | `fast_search` instantiates `crate::tools::ManageWorkspaceTool` and calls `call_tool_with_options` to index-on-pending (`search/mod.rs:671,679`). ManageWorkspaceTool moves UP to runtime → irreducible tools→runtime edge. | Facade method `ensure_target_workspace_indexed_if_pending()`, impl in top-crate handler; search never names the tool. |
| 2 | `resolve_workspace_filter` (`navigation/resolution.rs:135`, called by all 11 read+edit tools) needs 5 handler caps incl. a session-activation **mutation** (`activate_workspace_with_root`). The synthesizer's `resolve_workspace_target` only covered the daemon_db read. | Widen `resolve_workspace_target()` to encapsulate the **whole resolver verbatim** in the top crate. |
| 3 | P0 list omitted `crate::utils::{serde_lenient ×29, file_utils, token_estimation, language, cross_language_intelligence, string_similarity}` + `crate::mcp_compat`, all consumed by tool source. | Relocate them to julie-core (`mcp_compat` is a pure MCP-result wrapper, no upward deps). Without this, julie-tools won't compile. |
| 4 | Moving `tools/workspace/commands/**` wholesale to runtime breaks: `registry/cleanup.rs` + `registry/mod.rs` construct daemon types, and daemon sits ABOVE runtime. | Move only the 8 daemon-free command files to runtime; keep `cleanup.rs` + `registry/mod.rs` in the top crate with daemon. |
| 5 | `system_readiness` may not be a pure default method; one unlisted `file_policy → utils::file_utils` edge. | Verify `check_system_readiness` (`health/checker.rs:104`) truly only needs the two accessors; relocate `file_utils` to julie-core. |

Plus a **newly discovered cycle** not in the parent design: `external_extract ↔ indexing_core`. The `external_extract::paths` split must land in P0 before julie-pipeline extracts, or pipeline and external_extract deadlock.

All 4 verify lenses returned **design-needs-revision**, not design-broken: the core thesis and lowest-first ordering hold.

---

## 5. Risks (carried into the plan)

- **R1 — Handler decomposition is shallower than feared but high-volume.** Only TWO raw handler-field derefs in read tools (`spillover_store`, `session_metrics.session_id`); everything else already flows through accessors. The work is the 36-file `&JulieServerHandler → &dyn ToolContext` mechanical swap + the indexing/lifecycle split, not decomposing the 24 fields. Do the facade swap as one pass **before** the physical julie-tools extraction.
- **R2 — Workspace is the relink-cure outlier.** ~120 of ~240 workspace test fns instantiate a full handler and stay top-crate. Do not over-promise the cure for workspace; the strong cures are pipeline (~100%) and watcher (~100%).
- **R3 — `request_db` daemon-pool inversion** (`workspace/mod.rs:812`): runtime names a daemon type. Decide before 2c — recommend moving `request_db` UP to the handler (daemon-mode-only concern).
- **R4 — file_policy/state/blacklist crate-home.** Consumed by watcher (runtime), startup (top), and indexing_core (pipeline). Safest is to push them all the way to julie-core so every layer reaches down; mis-placing in pipeline risks a future startup/watcher edge that can't reach.
- **R5 — Test non-hermeticity (#33).** Relocated handler-instantiating tests must keep `.git`-marker isolation (`make_isolated_workspace_root`) or they resolve to a stray `/private/tmp/Cargo.toml` and fail local-only. `FakeToolContext` tests are immune.
- **R6 — xtask routing drift (Phase 0/1 lesson).** Each new crate needs a unique `-p <crate>` bucket command (the manifest bails on duplicate command strings), a tier entry, a `sort_bucket_names` order entry, AND a `changed.rs` prefix arm that **co-targets** the behavioral buckets whose retained tests still exercise moved code. Missing a co-target is a silent coverage gap.
- **R7 — Generalize the tripwire manifest check.** The Cargo.toml manifest check currently hard-codes only the top `julie` name. It must forbid every HIGHER sibling by name (pipeline rejects tools/runtime, tools rejects runtime) or an accidental upward path-dep slips past.

---

## 6. Explicitly NOT in Phase 2

- Not touching the daemon (that's Phase 3) or tool taxonomy (Phase 4).
- Not running the deferred R3 retrieval bakeoff (parent design Phase 0c) — still owed, deferred by owner decision; tracked as a standing risk, not a Phase 2 gate.
- Not decomposing the 24-field handler struct itself beyond what the facade requires.
