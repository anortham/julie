# Julie Rescue Phase 2 — Implementation Plan

**Date:** 2026-06-04
**Design:** `docs/plans/2026-06-04-julie-phase2-crate-split-design.md`
**Branch base:** `julie-rescue` (== `main` @ `4f2c2353`)
**Shape:** three sub-PRs, lowest-first. Each is independently green and merges to main before the next starts (Phase 0/1 discipline).

> **Execution model:** lead orchestrates via `razorback:subagent-driven-development`. Subagents run **narrow targeted tests only** (`cargo nextest run -p <crate> --no-run` to prove compile incl. `cfg(test)`, plus the one test they wrote). The **lead** runs `cargo xtask test changed` during the loop and the batch gate (`dev` + the noted tiers) once per PR. The Phase-1 lesson stands: the crate-split gate is `nextest --no-run`, because `cargo check` skips `cfg(test)`.

---

## PR 2a — `julie-pipeline` (lowest, no facade)

Extract the indexing+embedding engine, which has **zero `JulieServerHandler` refs**. Includes the P0 leaf relocations because pipeline needs several of them, and they unblock 2b/2c too.

### Tasks
- **T2a.1 — P0 relocations to julie-core.** Move to `crates/julie-core`: `tools::shared` `BLACKLISTED_*`/`NOISE_CALLEE_NAMES`; workspace leaves `registry.rs` (`generate_workspace_id`), `root_safety.rs`, `mutation_gate.rs` (`MutationGuard`/`Registry`), `startup_hint.rs`; `external_extract::paths`; `health/types.rs` state enums + `SystemStatus`; **and (Blocker 3)** `utils::{serde_lenient, file_utils, token_estimation, language, cross_language_intelligence, string_similarity}` + `mcp_compat`. Repoint `src/utils/{paths.rs,walk.rs}` to import from julie-core (severs the still-open §3.2 `utils→tools` edges).
- **T2a.2 — split `external_extract`** (Blocker / new cycle): `paths` already moved to core in T2a.1; confirm `operations/report/cli` stay top-crate and call down. This must precede the indexing_core move.
- **T2a.3 — relocate file_policy + indexing `state.rs`** to julie-core (R4 safest home: watcher/startup/indexing_core all reach down). Clears the `watcher→tools` and `indexing_core→tools` `file_policy`/`state` edges and the `JulieWorkspace.indexing_runtime` field type at once.
- **T2a.4 — split `finalize.rs`**: handler-free `resolve_pending_relationships` → julie-pipeline; `analyze_batch` stays (moves to runtime in 2c).
- **T2a.5 — relocate embedding-log-fields cluster** (`build_embedding_runtime_log_fields` + `EmbeddingRuntimeLogFields` + `embedding_telemetry_confidence`) from `workspace/mod.rs` into pipeline (severs the `embeddings→workspace` inversion). Must land before the embeddings move.
- **T2a.6 — replace `extract_symbols_static`** call (`indexing_core/extraction.rs:318`) with a free fn over `julie_extractors::extract_canonical`.
- **T2a.7 — create `crates/julie-pipeline`**, move `src/indexing_core/**` + `src/embeddings/**` + the pure-extraction half of `src/tools/workspace/indexing`. Relocate the handler-free pipeline/embeddings unit tests (~175 embeddings + ~55 indexing fns) into the crate.
- **T2a.8 — tripwire + xtask routing.** Add `crates/julie-pipeline/tests/no_upward_deps.rs` (forbid every higher sibling by name — R7). Add `core-pipeline` bucket (`cargo nextest run -p julie-pipeline`) to `test_tiers.toml` (dev+full), the `sort_bucket_names` order, and a `changed.rs` prefix arm for `crates/julie-pipeline/src/` that co-targets retained behavioral buckets (R6).

### Acceptance (PR 2a)
- `crates/julie-core/tests/no_upward_deps.rs` + `crates/julie-pipeline/tests/no_upward_deps.rs` green.
- `cargo nextest -p julie-core --no-run` and `-p julie-pipeline --no-run` compile (cfg(test) included).
- `rg "crate::tools"` returns zero hits in `src/utils/`, `src/watcher/filtering.rs`, `src/indexing_core/`.
- **Relink-cure check:** touch a `crates/julie-pipeline/src` file → only `-p julie-pipeline` (+ co-targeted buckets) rebuild, NOT the top-crate test binary. Record wall-clock in the ledger.
- `cargo xtask test dev` green; live smoke `./target/debug/julie-server search "@test" --target definitions --workspace . --standalone --json` returns hits.

---

## PR 2b — `julie-context` + facade swap + `julie-tools`

The gating PR. Stand up the abstraction, do the mechanical swap, then physically extract tools.

### Tasks
- **T2b.1 — create `crates/julie-context`**: the `#[async_trait] ToolContext` (the ~20 methods from design §2, incl. the `resolve_workspace_target` / `record_refactor_metrics` / `ensure_target_workspace_indexed_if_pending` purpose-methods and the `system_readiness` default). Relocate `SpilloverStore` from `src/tools/spillover/store.rs` into julie-context. Add `ResolvedTarget` / `RefactorMetrics` value types. Tripwire: forbid handler/daemon/watcher/workspace-runtime; allow core/index.
- **T2b.2 — `impl ToolContext for JulieServerHandler`** in the top crate, bodies = existing accessor methods verbatim. For Blocker 1/2, the `ensure_target_workspace_indexed_if_pending` and full `resolve_workspace_target` impls live here (they may name `ManageWorkspaceTool`/`DaemonDatabase` — legal in the top crate).
- **T2b.3 — `FakeToolContext` in julie-test-support** (R5: no workspace-root walk, so hermetic).
- **T2b.4 — facade swap (R1, one mechanical pass):** every read+edit tool entry point `&JulieServerHandler → &dyn ToolContext` (search/execution, get_context run-wrappers, spillover, impact, deep_dive, navigation, symbols, editing, refactoring). daemon_db reads → `resolve_workspace_target()`/`record_refactor_metrics()`. nl_embeddings provider-wait → pushed to pipeline; stdio lazy-init → runtime, both behind `ensure_embedding_provider()`. Relocate the 2 `#[cfg(test)]` handler-using mods (nl_embeddings, text_search) out.
- **T2b.5 — extract `crates/julie-tools`**: move the now-handler-free `src/tools/{search, deep_dive, navigation, get_context, impact, symbols, spillover-shim, editing, refactoring}` + `external_extract::{operations,report,cli}`. Relocate the ~498 handler-free tool unit tests INTO julie-tools.
- **T2b.6 — tripwire + xtask routing** for julie-tools (forbid handler/daemon/runtime; **must NOT** forbid `julie_context::ToolContext`; allow core/index/context/pipeline). Add tools bucket(s) + order + `changed.rs` arm.

### Acceptance (PR 2b)
- `rg "crate::handler::JulieServerHandler|crate::daemon|EmbeddingServiceSettled"` → zero production hits in the moved tool dirs.
- `crates/julie-context` + `crates/julie-tools` tripwires green; `-p julie-context --no-run`, `-p julie-tools --no-run` compile.
- The 498 handler-free tests pass against `FakeToolContext`; the 159 handler tests still pass via the real handler.
- **Relink-cure check:** touch a `crates/julie-tools/src` file → ~498 tool unit tests rebuild in `-p julie-tools`, not the top crate. Record wall-clock.
- `cargo xtask test dev` + `cargo xtask test dogfood` green (search/scoring touched); live smoke search/deep_dive/fast_refs/get_context/edit_file identical.

---

## PR 2c — `julie-runtime` (highest of the three)

### Tasks
- **T2c.1 — resolve `request_db` inversion** (R3): move `request_db` UP to the handler/daemon (daemon-mode-only) so runtime never names `daemon::connection_pool`.
- **T2c.2 — create `crates/julie-runtime`**, move `src/watcher/**` + `src/workspace/**` (leaves already in core) + the **8 daemon-free** `tools/workspace/commands/**` files (Blocker 4: keep `registry/cleanup.rs` + `registry/mod.rs` top-crate) + the session-state orchestration half of `tools/workspace/indexing` (`route.rs`, `index.rs` orchestration, embeddings task-registration, `analyze_batch`). The `manage_workspace` MCP dispatch shim in the top crate calls down into runtime.
- **T2c.3 — relocate watcher tests (~72 fns, near-total cure)** and the relocatable workspace tests; handler-instantiating workspace tests (~120 fns) stay top-crate (R2). Preserve `make_isolated_workspace_root` (R5).
- **T2c.4 — tripwire + xtask routing** for julie-runtime (forbid handler/daemon/top `julie::`; allow core/index/context/pipeline/tools). Repoint old `src/watcher` → and `src/workspace` → arms to the new crate.

### Acceptance (PR 2c)
- `crates/julie-runtime/tests/no_upward_deps.rs` green; `-p julie-runtime --no-run` compiles; zero `crate::daemon` refs in the relocated command subset.
- **Relink-cure check:** touch `crates/julie-runtime/src/watcher` → ~72 watcher fns rebuild in `-p julie-runtime`, not the top crate. Record wall-clock.
- `cargo xtask test dev` + `cargo xtask test system` + `cargo xtask test reliability` green; live smoke `manage_workspace` index/refresh/health + a watcher save-and-reindex cycle (`grep "Watcher: extracted" daemon.log`).

---

## Verification Ledger

One row per verification run. Reuse only when Scope Label + Commit SHA match current HEAD exactly (see `verification-ledger-template.md`).

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|

---

## Open items to resolve during implementation
- Confirm `check_system_readiness` (`health/checker.rs:104`) truly depends only on the two DB/index accessors → keep `system_readiness` a default method; else make it a purpose-method (Blocker 5).
- Finalize the exact `ToolContext` method count after folding the resolver + index-pending shims (~20, not 16).
- R3 retrieval bakeoff (parent design Phase 0c) remains deferred — not a Phase 2 gate.
