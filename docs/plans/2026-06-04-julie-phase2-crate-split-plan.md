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

### PR 2a — STATUS: COMPLETE (U1–U4 landed)

Commits: `551634c8` → `435fc2f6` (U1) · `63b3810c` (U2) · `25ac35da` → `f3e1b2d4` → `690d62b0` (U3) · `039f85d6` (severance) · `7ce28607` (U4). All acceptance rows recorded in the ledger below. Highlights and corrections:

- **Relink cure measured: ~2.9× wall / ~4.1× build** (pipeline dev relink 6.2s vs top-crate test-binary relink 18.2s after the same touch). In line with Phase 0 (~5.8×) and Phase 1 (3.2×).
- **Correction to a carried assumption:** `cargo nextest run -p <crate>` **DOES** run the crate's `tests/no_upward_deps.rs` integration binary (verified: core 140/2-binaries, index 373/2-binaries, pipeline 144/2-binaries all include the tripwire). The earlier belief that "nextest skips integration binaries, so the tier must use `cargo test --test`" was **wrong** — no separate `cargo test --test` command is needed, and the existing core-index bucket comment was already correct. (The true Phase-1 lesson stands and is unaffected: `cargo check`/`cargo build` skip `cfg(test)`, so the crate-split *compile* gate must be `nextest --no-run`. This bit again here — `cargo build` reported a file_policy shim re-export as "unused" while a `cfg(test)` consumer still needed it.)
- **Regression caught + fixed by the U4 gate:** U3 relocated 118 embedding tests into julie-pipeline but left `core-embeddings` filtering for them; a zero-match nextest filter exits 4, so `dev`+`smoke` were silently RED at the U3 HEAD. Fixed by dropping the dead filters (coverage now in `core-pipeline`).
- **`dev` branch-gate: GREEN (across two runs, HEAD `e9150e61`).** The full `dev` tier WAS run as the pre-merge gate and earned its keep — it caught a second orphaned `--lib` filter that the targeted scans missed: `core-fast` ran `utils::string_similarity::tests`, but U1 relocated `string_similarity` to julie-core, so the filter matched zero tests (nextest exits 4). Fixed (coverage preserved by `core-database`). Run 1 (`73767fe1`): 31 buckets green, `core-fast` RED. Fix + Run 2: `core-fast` + the 4 tail buckets (`core-handler-telemetry`, `daemon`, `dashboard`, `extractor-dep-integration`) all green; the 31 run-1 buckets are unchanged (no product code touched by the fix). All 36 dev buckets green.
- **Orphan-scan method (banked lesson):** the reliable 0-match scan is to substring-match EVERY `--lib` filter against `cargo nextest list --lib` output — NOT `nextest list <filter>` exit codes (exits 0 on zero-match, unlike `nextest run` which exits 4), and NOT just `tests::*` filters (source-unit filters like `utils::*` orphan too).

---

## PR 2b — `julie-context` + facade swap + `julie-tools`

The gating PR. Stand up the abstraction, do the mechanical swap, then physically extract tools.

### Tasks
- **T2b.1 — create `crates/julie-context`**: the `#[async_trait] ToolContext` (**18 methods** — see the VALIDATED surface + U5 nameability resolution below; incl. the `resolve_workspace_target` / `ensure_target_workspace_indexed_if_pending` / `system_readiness` purpose-methods). Relocate `SpilloverStore` from `src/tools/spillover/store.rs` AND the `WorkspaceTarget` enum from `src/tools/navigation/resolution.rs:43` into julie-context (both are return types named by the trait — NB1). Tripwire: forbid handler/daemon/watcher/workspace-runtime; allow core/index.
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

### PR 2b — VALIDATED against post-2a `main` (HEAD `07fe4a27`)

Re-validated by an 11-agent read-only workflow (6 map → 1 draft → 3 adversarial-verify → 1 synthesize, ~1M tokens). All 3 adversarial lenses (raw-fields, async-cross-ws, index-pending-daemon) returned **surface complete** — `draft.uncovered_uses == []`. The boundary holds; the deltas below are corrections to the pre-2a design, plus lead decisions on the open questions.

**`ToolContext` surface = 18 methods** (design said ~20; map found 23; lead dropped 2 over-inclusions; signature-nameability sweep then dropped 3 more whose RETURN types live above julie-context — see decisions + the nameability resolution below). Final set:
- *identity (sync):* `current_workspace_id`, `require_primary_workspace_identity`, `require_primary_workspace_root`, `loaded_workspace_id`, `is_primary_workspace_swap_in_progress`, `session_id() -> &str` (**NEW accessor** wrapping the raw `session_metrics.session_id` field — only `.session_id` is read).
- *primary db/index (async):* `primary_pooled_database`, `primary_pooled_database_and_search_index`.
- *cross-workspace (async):* `get_pooled_database_for_workspace`, `get_database_for_workspace` (**NEW — missing from the design's enumeration; used at 4 probe sites in search/mod.rs**), `get_search_index_for_workspace`, `get_workspace_root_for_target` (also absorbs the `refactoring/mod.rs` daemon_db workspace-root read).
- *embeddings (async):* `embedding_provider`, `ensure_embedding_provider(timeout)` (**purpose-method**, encapsulates `wait_for_embedding_provider_settled`: daemon `EmbeddingServiceSettled` wait + stdio lazy-init).
- *spillover (sync):* `spillover_store() -> Arc<SpilloverStore>` (**NEW accessor** wrapping the raw field; the `SpilloverStore` TYPE relocates to julie-context).
- *purpose-methods (top-crate impls, keep daemon/tool types out of julie-tools):* `resolve_workspace_target` (Blocker 2 — encapsulates the WHOLE `resolve_workspace_filter` resolver verbatim, incl. the `activate_workspace_with_root` mutation), `ensure_target_workspace_indexed_if_pending` (Blocker 1 — the only `ManageWorkspaceTool` site), `system_readiness` (Blocker 5 — **CORRECTED to a purpose-method, NOT a default method**).

**Blocker corrections (vs design §4):**
- **B2:** resolver needs **6 caps + 1 mutation** (not 5). Resolver body spans `resolution.rs:94-162`, mutation at `:133`. Fix unchanged (encapsulate verbatim).
- **B5:** `system_readiness` is **NOT a pure default method** — the primary/None branch (`health/checker.rs:113 → system_snapshot`) reads `embedding_service.is_some()` (`:89`, daemon-only). Downgrade to a top-crate purpose-method (the design's own fallback). `SystemStatus` already in julie-core (`health_types.rs:12`).
- **B3 / file_policy:** fully RESOLVED in PR 2a. Residual 2b work = mechanical import repoint when tools physically move (T2b.5): `crate::utils::*` / `crate::mcp_compat::*` → `julie_core::*` (~29 `serde_lenient` string-literal `deserialize_with=` sites + mcp_compat). The shims live in the top crate and are unreachable from julie-tools.
- **B4:** confirmed a **2c** concern (commands not in the 2b move set).

**New blockers from the adversarial pass:**
- **NB1 (T2b.1 STRUCTURAL PRECONDITION):** `WorkspaceTarget` enum (return type of `resolve_workspace_target`) lives in `src/tools/navigation/resolution.rs:43` — a julie-tools file consumed by ~12 tool files. It MUST relocate **before** the trait can name it (else context→tools illegal). **Lead decision: relocate to julie-context** (co-located with the trait; only tools consume it, and tools dep context). Fold into T2b.1.
- **NB2 (T2b.4 enumeration risk, NOT a coverage gap):** `ensure_embedding_provider` has **4 production callers**, not 1: `nl_embeddings.rs:56`, `execution.rs:121` (direct), `text_search.rs:195` + `:214` (via `maybe_initialize_embeddings_for_nl_definitions`). The acceptance grep `EmbeddingServiceSettled → zero` will catch a miss in nl_embeddings but NOT `execution.rs:121` (it names no daemon type). **T2b.4 done-check MUST add:** `rg "wait_for_embedding_provider_settled" src/tools/` → zero after the swap.
- **NB3 (2c flag):** `registry/health.rs` (a "daemon-free" file slated for runtime in 2c) calls `HealthChecker` (top-crate, above runtime). Pre-flag for 2c; not a 2b concern.

**Lead decisions on the open questions:**
1. **OMIT `record_refactor_metrics`** — no moved tool writes refactor metrics (the daemon write is at the handler layer; `refactoring/mod.rs:184` is a workspace-root read covered by `get_workspace_root_for_target`). Add only if a consumer surfaces in T2b.4.
2. **DROP `acquire_mutation_gate` + `current_workspace_root`** (minimal trait): no moved tool calls either (the gate is acquired at handler/commands/startup; `symbols/primary.rs` uses a local var). (Took the surface to 21; the U5 nameability sweep then dropped 3 more → **18 final**.) Re-add in T2b.4 if a moved path needs one (cheap — trait is in julie-context).
3. **`session_id() -> &str`** (callers `.to_string()` where they need owned).
4. **`WorkspaceTarget` home = julie-context** (see NB1).
5. **`external_extract::{operations,report,cli}`**: keep in T2b.5 scope **with a handler-free verify-gate** — relocate if clean, defer if it pulls a daemon/runtime edge.

**Signature-nameability resolution (U5 — resolves the 21→18 gap; `t2b1_ready: true`).** The 11-agent coverage validation proved every USE was covered but did not check that every trait method's RETURN type is *nameable* from julie-context (deps = core+index only). A 6-agent sweep→re-express→adversarial-verify→synthesize workflow found **3 of the 21 methods return types that live ABOVE julie-context** and re-expressed them. Resolution is purely **subtractive — drop the 3 from the trait** (no relocation, no new value-structs, no signature changes to survivors). The 18 survivors are 100% nameable. Each dropped method **STAYS an inherent `pub` method on `JulieServerHandler`** (top-crate/runtime/test callers name the types legally from above); only its presence in the `ToolContext` trait is removed.

| Dropped method | Return type / home (above context) | Trait-surface caller(s) in 2b move-set | Caller rewrite (replacement is on the 18-method surface) |
|---|---|---|---|
| `get_workspace -> Result<Option<JulieWorkspace>>` | `JulieWorkspace` @ `src/workspace/mod.rs` → julie-runtime (2c) | `search/mod.rs:313` — reads **zero fields** (pure `.is_none()` presence check) | `handler.get_workspace().await?.is_none()` → `handler.require_primary_workspace_identity().is_err()` (already guarded by `!is_primary_workspace_swap_in_progress()` at :312, and the same identity call is made at :322 — hoists it one guard up; only selects between two NotReady messages, never correctness) |
| `require_primary_workspace_binding -> Result<PrimaryWorkspaceBinding>` | `PrimaryWorkspaceBinding` @ `src/handler/session_workspace.rs:8` (top crate) | `symbols/primary.rs:28-29` — reads only `.workspace_root` | `let binding = handler.require_primary_workspace_binding()?; let root = binding.workspace_root;` → `let root = handler.require_primary_workspace_root()?;` (exact via `?` propagation) |
| `primary_workspace_snapshot -> Result<PrimaryWorkspaceSnapshot>` | `PrimaryWorkspaceSnapshot` @ `src/handler.rs:45` (top crate; embeds `PrimaryWorkspaceBinding`) | `rewrite_symbol.rs:580-583` (reads `binding.{workspace_id,workspace_root}`); `line_mode.rs:588-591` (reads `.search_index`, after a separate `primary_pooled_database()` at :588) | rewrite_symbol → `let workspace_id = handler.require_primary_workspace_identity()?; let workspace_root = handler.require_primary_workspace_root()?;` (the **two primitives**, NOT the dropped binding). line_mode → collapse :588+:590-591 into `let (pooled_db, search_index) = handler.primary_pooled_database_and_search_index().await?;` |

**Retention invariant (T2b.2 done-check):** `require_primary_workspace_binding` (`handler.rs:1080`) and `primary_workspace_snapshot` (`handler.rs:1846`) MUST stay inherent `pub` methods on `JulieServerHandler` — 9 handler-bound snapshot test files + binding test files (`metrics_recording.rs:78`, `workspace_binding_metrics.rs:74`, `daemon/database.rs:832`) + 2c runtime caller `route.rs:207,214` call them directly and break if deleted (not merely removed from the trait). They are NOT in the trait; they remain on the struct.

**T2b.4 done-check greps (additive to NB2):** after the swaps, `rg "primary_workspace_snapshot|require_primary_workspace_binding|get_workspace\(\)" src/tools/{search,symbols,editing,navigation,deep_dive,impact,get_context,spillover,refactoring}/` → zero production hits (handler-bound *tests* in those dirs may retain them while still co-located pre-T2b.5). Cosmetic: line_mode's unreachable missing-index hint shifts `operation="refresh"` → `operation="index"`; verified no test asserts the `refresh` wording (the asserted `requires a Tantivy index` string originates in the higher `search/mod.rs` guard, which short-circuits first).

**Test inventory (relink-cure target):** ~**603 handler-free / ~246 handler-bound** across the 9 julie-tools dirs (design estimated ~498/~159; direction holds — strong cure — magnitude ~1.2–1.6× larger). `JulieServerHandler::new_for_test()` (`handler.rs:955`) is the dominant bound-test ctor (36 files). Acceptance numbers in the section above should read "~603 free / ~246 bound", not 498/159.

**`SpilloverStore` relocation:** deps are `std` + `anyhow` + `blake3` only (`store.rs:1-6`), both already julie-core deps → clean move to julie-context, handler-free confirmed.

**Validated execution order (strict sequential):** T2b.1 (julie-context: trait + relocate SpilloverStore + WorkspaceTarget) → T2b.2 (`impl ToolContext for JulieServerHandler`, incl. the 4 purpose-methods) → T2b.3 (`FakeToolContext`, may run parallel to T2b.4 once T2b.2 lands) → T2b.4 (facade swap — high-risk, gate each batch on `nextest -p julie --no-run`) → T2b.5 (physical julie-tools extraction + test relocation + import repoint) → T2b.6 (tripwire + xtask routing). T2b.4 must compile against the real handler-as-ToolContext BEFORE the physical move (T2b.5), because the move only succeeds once tools name zero handler/daemon types.

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
| U1: julie-core test binary compiles after leaf relocations | `cargo nextest run -p julie-core --no-run` | affected-change | 435fc2f6 | pass | 2026-06-04T07:34:31Z | no |
| U1: top-crate test binary compiles (cfg(test) — the gate `cargo build` missed) | `cargo nextest run -p julie --no-run` | affected-change | 435fc2f6 | pass | 2026-06-04T07:34:31Z | no |
| U1: dep-direction tripwires hold (no upward source refs / no cyclic+upward manifest deps) | `cargo test -p julie-core --test no_upward_deps && cargo test -p julie-index --test no_upward_deps` | affected-change | 435fc2f6 | pass | 2026-06-04T07:34:31Z | no |
| U1: top-crate lib build is warning-clean | `cargo build` (warning count = 0) | affected-change | 435fc2f6 | pass | 2026-06-04T07:34:31Z | no |
| U2: both test binaries compile after file_policy/state→core + log-fields move | `cargo nextest run -p julie-core --no-run && cargo nextest run -p julie --no-run` | affected-change | 63b3810c | pass | 2026-06-04T07:50:10Z | no |
| U2: embeddings→workspace + indexing_core→ManageWorkspaceTool inversions severed | `rg "crate::workspace" src/embeddings/ ; rg "extract_symbols_static\|ManageWorkspaceTool" src/indexing_core/` (zero hits) | affected-change | 63b3810c | pass | 2026-06-04T07:50:10Z | no |
| U2: dep-direction tripwire holds with new core modules | `cargo test -p julie-core --test no_upward_deps` | affected-change | 63b3810c | pass | 2026-06-04T07:50:10Z | no |
| U2: extract/index path intact (CLI live-smoke) | `./target/debug/julie-server search ... --standalone --json` | affected-change | 63b3810c | pass | 2026-06-04T07:50:10Z | no |
| U3: julie-pipeline crate extracted; relocated test suite passes | `cargo nextest run -p julie-pipeline` (142/142) | affected-change | 690d62b0 | pass | 2026-06-04T08:44:10Z | no |
| U3: top-crate test binary compiles after pipeline extraction (cfg(test)) | `cargo nextest run -p julie --no-run` | affected-change | 690d62b0 | pass | 2026-06-04T08:44:10Z | no |
| U3: core+index dep-direction tripwires still hold after extraction | `cargo test -p julie-core --test no_upward_deps && cargo test -p julie-index --test no_upward_deps` | affected-change | 690d62b0 | pass | 2026-06-04T08:44:10Z | no |
| U3: dev-mode sidecar_root_path regression fixed (crate move broke CARGO_MANIFEST_DIR priority-3) | `cargo nextest run -p julie --features embeddings-sidecar --lib test_sidecar_root_path_succeeds_from_source_checkout` | affected-change | 690d62b0 | pass | 2026-06-04T08:44:10Z | no |
| U3: top-crate lib build warning-clean | `cargo build` (warning count = 0) | affected-change | 690d62b0 | pass | 2026-06-04T08:44:10Z | no |
| U3: extract/index/search path intact (CLI live-smoke) | `./target/debug/julie-server search "@test" --target definitions --standalone --json` | affected-change | 690d62b0 | pass | 2026-06-04T08:44:10Z | no |
| U4: julie-pipeline dep-direction tripwire green (manifest allowlist negative-tested) | `cargo test -p julie-pipeline --test no_upward_deps` | affected-change | 7ce28607 | pass | 2026-06-04T09:16:25Z | no |
| U4: core-pipeline bucket runs lib tests + integration tripwire (nextest does NOT skip it — 144 across 2 binaries) | `cargo nextest run -p julie-pipeline` | affected-change | 7ce28607 | pass | 2026-06-04T09:16:25Z | no |
| U4: xtask manifest contract + changed-routing (incl new pipeline arm tests) green | `cargo nextest run -p xtask` (168/168) | affected-change | 7ce28607 | pass | 2026-06-04T09:16:25Z | no |
| U4: PR 2a severance — zero `crate::tools` in src/utils, src/watcher/filtering.rs, src/indexing_core | `rg -c "crate::tools" src/utils/ src/watcher/filtering.rs src/indexing_core/` | affected-change | 7ce28607 | pass (0 hits) | 2026-06-04T09:16:25Z | no |
| U4: U3 core-embeddings dead-filter regression fixed — smoke tier green (was RED: zero-match nextest filter exits 4) | `cargo xtask test smoke` | affected-change | 7ce28607 | pass | 2026-06-04T09:16:25Z | no |
| U4: lib + cfg(test) warning-clean after watcher severance repoint | `cargo build` (0 warn) + `cargo nextest run -p julie --no-run` | affected-change | 7ce28607 | pass | 2026-06-04T09:16:25Z | no |
| **U4: RELINK-CURE** — touch `crates/julie-pipeline/src/indexing_core/extraction.rs`, pipeline dev relink vs top-crate test-binary relink | `cargo nextest run -p julie-pipeline --no-run` (6.2s wall / 2.53s build) vs `cargo nextest run -p julie --no-run` (18.2s wall / 10.37s build) → **~2.9× wall / ~4.1× build cure** | affected-change | 7ce28607 | pass | 2026-06-04T08:50:00Z | no |
| PR 2a branch-gate: full dev tier green (caught + fixed core-fast `string_similarity` orphan from U1) | `cargo xtask test dev` (36 buckets; run 1 = 31 green + core-fast RED @ 73767fe1, fix + run 2 = core-fast + 4 tail green) | branch-gate | e9150e61 | pass | 2026-06-04T09:40:00Z | no |
| U5: PR 2b `ToolContext` surface is 100% signature-nameable from julie-context (sweep→re-express→adversarial-verify→synthesize, `t2b1_ready: true`) — 3 methods with above-context return types dropped from the trait (kept inherent on the handler), 18 survivors verified nameable, no new structs/signature changes | 6-agent read-only validation workflow (params + return types of all 21 candidates checked against source; re-expressions field-read-verified against every 2b move-set caller) | branch-gate (design) | 822c2d42 | pass | 2026-06-04T10:30:00Z | no |

---

## Open items to resolve during implementation
- ~~Confirm `check_system_readiness` depends only on the two DB/index accessors~~ → RESOLVED (B5): it reads `embedding_service.is_some()` (daemon-only), so `system_readiness` is a **top-crate purpose-method**, not a default.
- ~~Finalize the exact `ToolContext` method count~~ → RESOLVED (U5): **18 methods**, fully nameable from julie-context. See the U5 nameability resolution.
- R3 retrieval bakeoff (parent design Phase 0c) remains deferred — not a Phase 2 gate.
