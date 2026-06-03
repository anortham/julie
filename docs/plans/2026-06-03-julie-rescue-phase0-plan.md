# Julie Rescue — Phase 0 (Boundary Proof + One-Shot Bakeoff) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Prove the leaf-crate split is real by extracting `julie-core` (database + the `EmbeddingProvider` trait + `connection_pool` + path helpers) and a `julie-test-support` crate, relocating the ~108 pure database tests so editing one relinks **only** `julie-core` — and run a single three-way retrieval ranking (julie/miller/eros) as a decision input. If either the relink win or the moat fails to materialize, the whole rescue verdict reopens *before* the broad work.

**Architecture:** Convert the existing 2-member Cargo workspace (`.`, `xtask`) into one that also contains `crates/julie-core` (bottom leaf) and `crates/julie-test-support` (dev-only). Move the database module + its two real upward dependencies down into `julie-core`; keep every existing `crate::database::*` / `crate::embeddings::EmbeddingProvider` call site compiling via `pub use` re-export shims in the top `julie` crate. Relocate the pure database test slice into `julie-core` so it compiles into a separate test binary. The bakeoff is an independent track.

**Tech Stack:** Rust 2024 (Cargo workspace, rusqlite 0.39 bundled, tokio, anyhow, `julie-extractors` git dep tag v2.0.3, cargo-nextest); Python 3.12 + `uv` (eros eval harness); .NET 10 (Miller, spawned read-only by the bakeoff driver).

**Architecture Quality:** Approved shape — `julie-core` is the bottom crate holding `database`, the `EmbeddingProvider` trait (+ `DeviceInfo`/`EmbeddingRuntimeStatus`/`EmbeddingBackend`), `connection_pool` (`PooledConn`/`WorkspaceConnectionPool`), and the leaf path helpers (`to_relative_unix_style` + private siblings). The `julie` crate re-exports these so the ~80 call sites and ~90 test files do **not** change. `julie-test-support` holds only the handler-free test helpers and is a **dev-dependency** of `julie-core` (normal dep would be a Cargo cycle). Architecture risk: **medium-high** — workspace surgery touches build structure and module visibility — mitigated by (a) re-export shims keeping callers unchanged, (b) the existing test suite as the behavior guard, (c) doing it on `julie-rescue`, lowest layer first, green between tasks. If code reality contradicts this shape (e.g. a hidden upward edge from `database`), the worker reports a plan mismatch rather than redesigning locally.

---

## Scope

**In Phase 0:** the `julie-core` + `julie-test-support` crates; moving `database`, the `EmbeddingProvider` trait, `connection_pool`, and `to_relative_unix_style` down; relocating the pure database test slice; the relink-cure proof; the dep-direction tripwire; the one-shot three-way bakeoff.

**Deferred to Phase 1 (NOT in this plan):** merging `search`+`analysis` into `julie-index`; relocating `BLACKLISTED_DIRECTORIES`, `matches_glob_pattern`, `file_policy`, `extract_symbols_static`, and the `WorkspaceResolutionFailure` error types; the `tools↔indexing_core` cycle break. Those are needed for `julie-index`/`julie-pipeline`, not for the database slice proof. Phase 0 deliberately proves the pattern on the smallest clean slice.

**One scope flag for the reviewer (Task 9):** a true julie/miller/eros ranking needs a small one-time Miller MCP-stdio driver (~one script) because Miller has no headless query path. This is bounded, single-run, *not* tuning-loop machinery. The alternative is a julie-vs-eros-only run, which does not answer the decision (Miller is the successor candidate). Recommended: build the small driver. Confirm at approval.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` / `AGENTS.md` "RUNNING TESTS" section + `cargo xtask test` tiers.

**Worker red/green scope:** the narrowest proving command for the touched behavior:
- Before the database move: `cargo nextest run --lib tests::core::database` (and `tests::core::vector_storage`).
- After the database move: `cargo nextest run -p julie-core`.
- Trait/pool/paths moves: `cargo build` then the narrowest affected test (e.g. `cargo nextest run --lib tests::core::embedding_provider` for the trait; `cargo nextest run --lib tests::daemon::connection_pool_test` for the pool).

**Worker ceiling:** subagents run ONLY the specific narrow test(s) for their task (red → green, ≤2 runs), per CLAUDE.md's subagent rules. Subagents do NOT run `cargo xtask test changed`/`dev` or any tier — the lead owns regression gates.

**Worker gate invariant:** each move task proves "behavior unchanged" by the pre-existing tests for the moved code passing from their new location with no assertion edits (only import-path edits).

**Lead affected-change scope:** `cargo xtask test changed` during the local loop after each coherent task; if it falls back to `dev` (shared infra — `lib.rs`, `Cargo.toml` moved), accept it.

**Branch gate:** `cargo xtask test dev` once after the crate-split batch (Tasks 1–8) is complete; add `cargo xtask test system` because workspace/startup wiring is touched.

**Replay/metric evidence:** Task 8's relink proof is a **hard gate** (timed touch-and-rebuild must show julie-core-only relink). The Task 9 bakeoff ranking is **report-only** evidence feeding the Phase-0 decision — no metric is a CI gate (no promotion-gate machinery, per owner directive).

**Escalation triggers:** any new compile cycle Cargo rejects; any test that needs an *assertion* change to pass post-move (signals a behavior change, not a move); the relink proof failing to show isolation.

**Assigned verification failure:** workers stop and report; they do not "fix" a gate by weakening it.

**Verification ledger:** use `docs/plans/verification-ledger-template.md`. Record the relink-proof timings (touch a `julie-core` db test vs touch a top-crate test, wall-clock each) with the commit SHA.

---

## Model Routing

**Project source of truth:** repo-root `RAZORBACK.md`.

**Strategy tier** (this plan's lead, decomposition, the crate-boundary contract, finding triage): Codex `gpt-5.5 high` / Claude **Opus**.

**Implementation tier** (bounded move tasks once the re-export contract is fixed): Codex `gpt-5.5 low/medium` / Claude **Sonnet**. The module moves are mechanical-with-judgment (visibility + doc-link sweeps).

**Coupled implementation** (the Miller MCP-stdio bakeoff driver — new cross-process code): Codex `gpt-5.5 medium` / Claude **Sonnet high**.

**Mechanical tier** (Cargo.toml member additions, pure import re-points): Codex `gpt-5.4-mini` / Claude **Haiku/Sonnet-low** — but NOT for the relink proof or bakeoff interpretation (those are evidence, not mechanical).

**Escalation tier** (a rejected Cargo cycle, a visibility change rippling unexpectedly, post-move test breakage): Codex `gpt-5.5 high/xhigh` / Claude **Opus**.

**Worker eligibility:** implementation-tier workers may own a single move task with a fixed re-export contract and a narrow proving test. **Mechanical exclusion:** mechanical workers cannot own the relink proof (Task 8) or the bakeoff (Task 9).

**Unsupported harness behavior:** if a harness cannot select per-agent models, use `inherit` and note it.

---

## File Structure

```
crates/julie-core/
  Cargo.toml                 # new leaf crate; deps: rusqlite, anyhow, fs2, sqlite-vec, tracing, tokio, julie-extractors
  src/lib.rs                 # pub mod database; pub mod connection_pool; pub mod paths; pub mod embeddings_contract; re-exports
  src/database/**            # moved from src/database/**
  src/connection_pool.rs     # moved from src/daemon/connection_pool.rs
  src/paths.rs               # to_relative_unix_style + strip_unc_prefix + relative_by_normalized_string (leaf subset only)
  src/embeddings_contract.rs # EmbeddingProvider trait + DeviceInfo + EmbeddingRuntimeStatus + EmbeddingBackend
  src/tests/database/**      # moved pure database tests (cfg(test) or tests/ dir)

crates/julie-test-support/
  Cargo.toml                 # deps: julie-core, julie-extractors, tempfile/anyhow; NO julie, NO handler/tools
  src/lib.rs                 # db row builders, open_test_connection, unique_temp_dir, atomic_cleanup_julie_dir

# top julie crate (unchanged call sites via re-exports):
src/lib.rs                   # `pub use julie_core::database;` etc.
src/embeddings/mod.rs        # `pub use julie_core::embeddings_contract::{EmbeddingProvider, DeviceInfo, ...};`
src/daemon/mod.rs            # `pub use julie_core::connection_pool::{PooledConn, WorkspaceConnectionPool};`
src/utils/paths.rs           # `pub use julie_core::paths::to_relative_unix_style;`

scripts/bakeoff/             # new (Task 9): miller_mcp_driver.py + run-phase0-bakeoff.sh
docs/plans/2026-06-03-julie-rescue-phase0-results.md  # ledger + bakeoff ranking output
```

---

## Tasks

### Task 1: Scaffold `julie-core` crate + wire into the workspace

**Files:**
- Modify: `Cargo.toml:1-2` (workspace members)
- Create: `crates/julie-core/Cargo.toml`, `crates/julie-core/src/lib.rs`

**What to build:** An empty-but-compiling `julie-core` library crate added to the workspace, depending on the same `julie-extractors` git tag (v2.0.3), `rusqlite` (0.39 bundled), `anyhow`, `fs2`, `sqlite-vec`, `tracing`, `tokio`. The top `julie` crate gains `julie-core = { path = "crates/julie-core" }`.

**Approach:** Change `members = [".", "xtask"]` → `members = [".", "xtask", "crates/julie-core"]`. Use `edition.workspace = true`. Pin `julie-extractors` identically to the root (tag v2.0.3) so `julie_extractors::` types are the *same* types across crates. `lib.rs` starts empty (`//! julie-core: bottom leaf crate`). Do not move any code yet.

**Acceptance criteria:**
- [ ] `cargo build -p julie-core` succeeds (empty crate).
- [ ] `cargo build` (top crate) still succeeds with the new path dep present but unused.
- [ ] Committed.

### Task 2: Move the `EmbeddingProvider` trait + companion types into `julie-core`

**Files:**
- Create: `crates/julie-core/src/embeddings_contract.rs`
- Modify: `src/embeddings/mod.rs:32-49,52-58,61-88,95-133` (remove the moved items), add re-export
- Modify: `src/search/hybrid.rs:21` (re-point the one mandatory import)

**What to build:** Relocate the object-safe `EmbeddingProvider` trait (`src/embeddings/mod.rs:95-133`) plus `DeviceInfo` (`:61-88`), `EmbeddingRuntimeStatus` (`:52-58`), and `EmbeddingBackend` (`:32-49`) into `julie_core::embeddings_contract`. These are pure data + an object-safe trait; the only external dep is `anyhow`. **All concrete impls stay up** (`SidecarEmbeddingProvider`, factory, init, pipeline — verified to pull tokio/process/serde that must NOT enter core).

**Approach:** Move the four items verbatim. In `src/embeddings/mod.rs` add `pub use julie_core::embeddings_contract::{EmbeddingProvider, DeviceInfo, EmbeddingRuntimeStatus, EmbeddingBackend};` so the ~20 production sites and ~15 test mock-impls that import `crate::embeddings::EmbeddingProvider` keep compiling untouched. The **only** mandatory edit beyond the shim is `src/search/hybrid.rs:21` (it may also stay on `crate::embeddings::EmbeddingProvider` via the shim — prefer leaving it, making the shim the single change; re-point only if the architecture check wants search off the embeddings path). Sweep intra-doc links (`mod.rs:9-10`, `metadata.rs:104`).

**Acceptance criteria:**
- [ ] `cargo build` green; `cargo build -p julie-core` green; trait is `pub` and object-safe in core.
- [ ] No tokio/serde/process types leaked into `julie-core` (grep the new file).
- [ ] `cargo nextest run --lib tests::core::embedding_provider` and `tests::core::embedding_sidecar_provider` pass unchanged (import paths only).
- [ ] Committed.

### Task 3: Move `connection_pool` (`PooledConn` + `WorkspaceConnectionPool`) into `julie-core`

**Files:**
- Create: `crates/julie-core/src/connection_pool.rs` (moved from `src/daemon/connection_pool.rs:1-292`)
- Modify: `src/daemon/mod.rs:10,44` (replace `pub mod connection_pool;` + `pub use self::connection_pool::{...}` with `pub use julie_core::connection_pool::{PooledConn, WorkspaceConnectionPool};`)

**What to build:** Relocate the whole `connection_pool.rs` (it has **zero** `crate::` deps — only rusqlite/tokio/anyhow/tracing/std). `PooledConn` and `WorkspaceConnectionPool` must move together (PooledConn holds `Arc<WorkspaceConnectionPool>` and Drop pushes back into it).

**Approach:** Move the file; expose `pub mod connection_pool;` from `julie-core/src/lib.rs`. Re-export from `daemon/mod.rs` so all ~12 consumers using `crate::daemon::connection_pool::*` (handler.rs:2157+, workspace/mod.rs:812, workspace_pool.rs, target_workspace.rs:49, + 4 test files) keep resolving. Note: this pulls `tokio::sync::Notify` into core — acceptable (core is not tokio-free; documented in the design).

**Acceptance criteria:**
- [ ] `cargo build` green; `cargo nextest run --lib tests::daemon::connection_pool_test` and `tests::daemon::symbol_db_pooled_test` pass unchanged.
- [ ] `grep -rn 'crate::daemon::connection_pool' src` still resolves via the re-export.
- [ ] Committed.

### Task 4: Move `to_relative_unix_style` (+ private helpers) into `julie-core::paths`

**Files:**
- Create/extend: `crates/julie-core/src/paths.rs`
- Modify: `src/utils/paths.rs:190-241` (remove the moved fns), add `pub use julie_core::paths::to_relative_unix_style;`

**What to build:** Move ONLY `to_relative_unix_style` and its private siblings `strip_unc_prefix` + `relative_by_normalized_string` (`src/utils/paths.rs:190-241`) into `julie_core::paths`. **Do not move the rest of `utils/paths.rs`** — it is non-leaf (`paths.rs:9` imports `crate::tools::navigation::resolution`, which would drag tools into the leaf crate).

**Approach:** Move the three self-contained fns (std + anyhow only). Re-export from `utils/paths.rs` so all existing `crate::utils::paths::to_relative_unix_style` callers (including `src/database/files.rs:588`) keep working. After this, `database` has no remaining real upward edge except the doc-comment at `bulk/type_arguments.rs:9`.

**Acceptance criteria:**
- [ ] `cargo build` green; `cargo nextest run --lib tests::core::paths` passes unchanged.
- [ ] Committed.

### Task 5: Move the `database` module into `julie-core`

**Files:**
- Move: `src/database/**` → `crates/julie-core/src/database/**`
- Modify: `src/lib.rs:9` (`pub mod database;` → `pub use julie_core::database;`)
- Modify: moved files' `crate::extractors::*` → `julie_extractors::*` (33 edges, e.g. `database/mod.rs:16`, `type_queries.rs:16`, `impact_graph.rs:15`, `bulk/*`); strip/rewrite the doc-only `crate::indexing_core::batch` link at `bulk/type_arguments.rs:9`

**What to build:** Relocate the 24-file `database` module into `julie-core`. With Tasks 3–4 done, its only remaining cross-crate references are `julie_extractors` (a normal dep) and `julie_core::{connection_pool, paths}` (now siblings in the same crate).

**Approach:** Move files. In `julie-core/src/lib.rs`: `pub mod database;`. Rewrite the 33 `crate::extractors::X` → `julie_extractors::X`. Promote any `pub(crate)` items reached across the crate boundary to `pub` (e.g. `SymbolDatabaseConn`, `bulk::atomic::CanonicalWriteSet`, `AtomicPersistenceMetadata`, `LATEST_SCHEMA_VERSION`) so the `julie`-crate re-export reaches them. Add `pub use julie_core::database;` in `src/lib.rs`. Verify `xtask` (which depends on `julie` by path and uses `julie::database::*`) still builds via the re-export. Sweep broken intra-doc links.

**Acceptance criteria:**
- [ ] `cargo build` + `cargo build -p julie-core` + `cargo build -p xtask` all green.
- [ ] `cargo nextest run --lib tests::core::database` and `tests::core::vector_storage` pass **from the top crate** unchanged (tests not yet moved — proves the re-export is faithful before relocation).
- [ ] No `crate::daemon` / `crate::tools` / `crate::handler` reference remains in `crates/julie-core/src` (grep clean — the dep-direction invariant).
- [ ] Committed.

### Task 6: Create `julie-test-support` crate (handler-free helpers only)

**Files:**
- Create: `crates/julie-test-support/Cargo.toml`, `crates/julie-test-support/src/lib.rs`
- Modify: `Cargo.toml` workspace members; `crates/julie-core/Cargo.toml` `[dev-dependencies] julie-test-support = { path = "../julie-test-support" }`

**What to build:** A crate holding ONLY the low-stack test helpers the database slice needs: the db row builders (`src/tests/helpers/db/rows.rs` — `file_info_builder`, `symbol_builder`, `identifier_builder`, `relationship_builder`, `set_symbol_reference_scores`, `store_file_info_if_missing`), `open_test_connection` (lifted out of the inline `src/tests/mod.rs:236`), `unique_temp_dir` (`tempdir.rs:23`), `atomic_cleanup_julie_dir` (`cleanup.rs:14`).

**Approach:** Depend on `julie-core` (for `SymbolDatabase`/`FileInfo`) + `julie-extractors` + `tempfile`/`anyhow`. **Must be a dev-dependency of julie-core**, never a normal dep (Cargo would reject the cycle julie-core→test-support→julie-core). **Do NOT include** `helpers/workspace.rs` (imports `JulieServerHandler`/`DaemonDatabase`/`WorkspacePool`), `fixtures/julie_db.rs` `JulieTestFixture` (imports `ManageWorkspaceTool`), or `helpers/mcp.rs` (rmcp) — those stay top-crate. The top `julie` crate may also re-export from `julie-test-support` for the ~14 other suites that use `helpers::db` (they keep working in the top binary).

**Acceptance criteria:**
- [ ] `cargo build -p julie-test-support` green; its dep graph contains no `julie`/handler/tools edge (`cargo tree -p julie-test-support` clean).
- [ ] Committed.

### Task 7: Relocate the pure database test slice into `julie-core`

**Files:**
- Move: `src/tests/core/database/**` (12 files, 63 fns) + the pure siblings (`core/vector_storage.rs` 21, `core/memory_vectors.rs` 10, `core/database_lightweight_query.rs` 4, `core/database_row_mapping.rs` 3, `core/bulk_store_types_tests.rs` 4, `core/test_bulk_store_types.rs` 2, `core/database_init_race.rs` 1) → `crates/julie-core/src/tests/database/**` (or `crates/julie-core/tests/`)
- Modify: `src/tests/mod.rs:41` (remove `pub mod database;` from the `core` block) + remove the moved sibling decls; delete `src/tests/core/database.rs`

**What to build:** Move the ~108 verified handler/tools-free database tests into `julie-core` so they compile into julie-core's own test binary.

**Approach:** Rewrite imports in the moved tests: `crate::database` → `julie_core::database` (or `crate::` if inside julie-core), `crate::extractors` → `julie_extractors`, `crate::tests::helpers::db::*` → `julie_test_support::*`, `crate::tests::test_helpers::open_test_connection` → `julie_test_support::open_test_connection`. The `use super::*` head pattern means most rewrites are in the module-head files; the few children using fully-qualified `crate::` paths (`concurrency_wal.rs`, `migrations.rs`, `basic_storage.rs`, `relationships.rs`, `extractor_symbols.rs`) get direct edits. Note `basic_storage.rs:225-232` instantiates a real `GoExtractor` — fine, julie-core already depends on `julie-extractors`. Ensure no remaining `src/tests` module imports `crate::tests::core::database::*` (verified: none do).

**Acceptance criteria:**
- [ ] `cargo nextest run -p julie-core` runs the ~108 database tests and they pass (assertions unchanged — import-only edits).
- [ ] `cargo nextest run --lib tests::core::database` from the top crate now finds nothing (slice fully moved); the rest of `tests::core` still compiles.
- [ ] Committed.

### Task 8: Prove the relink cure + add the dep-direction tripwire  *(hard gate — lead-owned)*

**Files:**
- Create: `docs/plans/2026-06-03-julie-rescue-phase0-results.md` (ledger)
- Create: a tripwire test (e.g. `crates/julie-core/tests/no_upward_deps.rs` or a grep-based check in xtask)

**What to build:** Empirical proof that editing a `julie-core` database test relinks only `julie-core`, plus a guard that julie-core never gains a handler/tools dependency.

**Approach:** Timed touch-and-rebuild: (a) `touch crates/julie-core/src/tests/database/basic_storage.rs && time cargo nextest run -p julie-core --no-run` — record wall-clock; (b) `touch src/tests/<a top-crate test> && time cargo nextest run --lib --no-run` — record wall-clock. The first must be dramatically smaller and must NOT relink the top `julie` test binary. Record both timings + commit SHA in the ledger per `verification-ledger-template.md`. Tripwire: assert (compile-time via the absent dependency, plus a cheap grep test) that `crates/julie-core/src` contains no `crate::handler`/`crate::tools`/`julie::` reference.

**Acceptance criteria:**
- [ ] Ledger shows julie-core-test relink wall-clock ≪ top-crate-test relink wall-clock, with the julie-core build not relinking the monolith.
- [ ] Tripwire test fails if a handler/tools dep is added to julie-core (verified by a temporary spike, then reverted).
- [ ] Lead runs `cargo xtask test changed` (expect fallback to `dev` — `lib.rs`/`Cargo.toml` moved) and `cargo xtask test system`; both green.
- [ ] Committed.

### Task 9: One-shot three-way retrieval ranking (julie / miller / eros)  *(independent track)*

**Files:**
- Create: `scripts/bakeoff/miller_mcp_driver.py`, `scripts/bakeoff/run-phase0-bakeoff.sh`
- Append results to: `docs/plans/2026-06-03-julie-rescue-phase0-results.md`

**What to build:** A single shared-corpus ranking of julie, miller, and eros retrieval — run once, recorded as a decision input. **No promotion-gate, no tuning loop.**

**Approach:**
1. Build julie release: `cd ~/source/julie && cargo build --release` (the eros harness auto-discovers `target/release/julie-server`).
2. Generate a **fair** corpus (NOT the in-sample bootstrap corpus): `cd ~/source/eros && uv run eros eval corpus ~/source --max-repositories 12 --output-dir ~/.eros-eval/phase0-corpus`. Record corpus provenance + its language coverage caveat (generator covers Go/JS/Py/Rs/TS; C#/Java/Swift under-sampled).
3. Start the eros hub at the checkout commit, then run julie+eros columns: `EROS_JULIE_COMMAND=auto uv run eros eval bakeoff --corpus <corpus.json> --julie-command auto --limit 5 --progress`; summarize with `eros eval summarize`.
4. **Miller column** (`scripts/bakeoff/miller_mcp_driver.py`): a standalone Python driver that (a) loads the *same* corpus via `eros.eval.corpus.load_query_corpus`, (b) spawns Miller as an MCP-stdio subprocess (`dotnet .../Miller.Server.dll`), does `initialize` then `tools/call name="search" arguments={query, format:"json", limit:5}` per query, (c) maps Miller's result path field into the keys `eros.eval.compare._result_paths` understands, (d) scores with eros's **own** `_first_matching_rank` + `_rank_metrics` (import them — do NOT re-implement) against the same `expected_paths`.
5. Merge into one ranking by top5-hits then MRR over the identical query set.

**Acceptance criteria:**
- [ ] One results table ranks all three on the same corpus, queries, and metric (Miller scored via imported eros scoring fns).
- [ ] Corpus provenance + bias caveats recorded; latency numbers from `--standalone` julie explicitly marked non-representative.
- [ ] The ranking is written to the results doc as **report-only** decision evidence (no gate added).
- [ ] Driver is a single run; no loop, no harness or Miller source changes.
- [ ] Committed.

---

## Phase 0 exit decision (lead, after Tasks 8 + 9)
Proceed to Phase 1 iff: Task 8 shows a real per-crate relink win **and** Task 9's ranking does not show Julie's retrieval losing decisively to Miller/eros. If the relink win is marginal, reconsider the crate granularity. If Julie loses the ranking decisively, reopen the "save vs switch" verdict before the broad work. Record the decision in the results doc.
