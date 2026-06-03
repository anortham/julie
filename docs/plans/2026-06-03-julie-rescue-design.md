# Julie Rescue: De-slop the Packaging, Keep the Moat

**Date:** 2026-06-03
**Status:** Design — revised after Codex (gpt-5.5) adversarial spec review; pending final user sign-off
**Authors:** Alan + Claude (brainstorm session)
**Supersedes the implicit question:** "Can we save Julie, or does Miller replace her?"

---

## TL;DR

Julie's momentum problem is **packaging, not language.** Two deletable structures are
strangling iteration: a **monolithic lib crate** that relinks 126k test LOC on every edit,
and a **bespoke ~11.5k-LOC daemon** that is both accidental complexity and the home of the
unsolved hang/disconnect bugs. Neither is a Rust verdict. We **save Julie in place** by
fixing the packaging, and we **harvest** the best ideas from Miller and eros (already-built
proofs of the ".NET host" and "Python host" rewrites) rather than switching hosts and
re-earning the moat from scratch.

The moat — which Miller/eros both dropped to look clean — is: semantic/hybrid search +
graph-centrality reranking + token-budgeted `get_context` + true 34-language breadth + a
shipped plugin and CLI.

**A Codex review (below) confirmed the relink diagnosis but found the boundaries are *less*
bounded than the first draft claimed. This revision adds a Phase 0 "boundary proof" that
de-risks the split on one slice before committing the broad work, and rewrites Phase 3 as a
real coordinator-replacement spec.**

---

## 1. Diagnosis (evidence-backed)

### 1.1 The relink tax (the acute pain)
- `src/lib.rs` pulls the entire `src/tests/` tree — **395 files, ~126k LOC, 2,727 test
  functions, 60% of the crate** — into a single `#[cfg(test)]` test binary.
- That binary **relinks on every source edit**, including before running a *single*
  targeted test. "Run the narrowest test" cannot help — narrowing cuts *execution* time, not
  *relink* time. Under agentic development (tests re-run ~20× per change) this compounds hard.
- `cargo check` is **3.6s** on 210k LOC — Rust is not the bottleneck. The packaging is.

### 1.2 The daemon (the chronic pain)
- `src/daemon/` = **10,126 LOC**, `src/adapter/` = **1,343 LOC**, only **12 test markers** —
  ~11.5k of *production* code. Roughly **7–9k is "we run a bespoke daemon" tax**
  (pid.rs 682, a second SQLite registry DB ~1,360, discovery/singleton/token-file 813,
  HTTP-bridge + pools 1,134, lifecycle/shutdown/legacy-migration/app ~2,450, and the whole
  `adapter/` 1,343 that exists only to bridge stdio↔daemon-HTTP).
- This layer hosts the never-root-caused `fast_search` hang / `deep_dive` disconnect.

### 1.3 God-objects
- `handler.rs` = 2,518 LOC / 95 methods / a **24-field state struct** (`handler.rs:191`)
  reaching into every subsystem. `search/index.rs` = 2,249 LOC. This coupling is what makes
  both problems above hard — see §3.3 and Phase 2.

### 1.4 What is NOT broken
- 12-tool surface is lean; xtask bucket/tier/`changed` runner is well-engineered.
- Output already defaults to **optimized plain text** (verified: `src/tools/search/` renders
  via text builders; `format` = `full`|`locations`, both text) — *not* verbose JSON.
- Extraction is already externalized to `julie-extractors` (pinned git-dep).

---

## 2. Strategic verdict

**Save Julie. Harvest Miller and eros. Do not switch hosts** — conditioned on the one-shot
moat measurement in Phase 0 (§4) not coming back damning.

| Option | Verdict |
|---|---|
| (A) Save in place: crate-split + daemon teardown + tool consolidation | **CHOSEN.** Fixes the fixable; preserves the moat. |
| (B) Replace with Miller (.NET) | Rejected now. Miller is clean (4.7s build, 1,303 tests in 4s) but has **no embeddings, no centrality ranking, a .NET-only cross-language bridge, no CLI, no plugin.** Switching = re-earning the moat from scratch. |
| (C) Re-platform host in Python/TS over Rust cores | Already built — that's **eros** (Python) and **Miller** (.NET). Both shed the moat. Realistic for batch/index/analyze; the live watcher+daemon must stay a resident Rust process. Long-game shape, not a near-term rescue. |

**Key reframe:** all three projects consume the *same* `julie-extract` binary. The parser
layer is settled and shared, so this is entirely a question about the **host** layer. The two
rewrites already exist as Miller and eros; their lesson is "you can be fast by dropping the
moat," which is the trade we decline.

---

## 3. Validated target architecture

### 3.1 Crate DAG (bottom → top)
```
julie-core      database + utils + paths + extractors(re-export) + relocated shared types
   ↑            + the EmbeddingProvider TRAIT (abstraction lives low; impl stays in pipeline)
julie-index     search + analysis  (MERGED — they cycle on language-config types)
   ↑
julie-pipeline  indexing_core (extract→persist orchestration) + embeddings sidecar impl
   ↑
julie-tools     the MCP tools — requires a ToolContext facade first (§Phase 2)
   ↑
julie-runtime   watcher + workspace
   ↑
julie-daemon    daemon + adapter   (gutted in Phase 3)
   ↑
julie-server    handler + dashboard + startup + health + cli + bins   (top)
```
Granularity is a target, not a mandate — extract lowest-first, only as far as each phase
needs.

### 3.2 Why this DAG is real — and where the first draft was wrong (measured `crate::` edges)
The split is blocked by a **bounded** set of back-edges and cycles, severed *before*
extraction. Codex caught one the first draft missed (the embedding edge):

| Back-edge / cycle | Evidence | Fix |
|---|---|---|
| `search ↔ embeddings` cycle **(MISSED in v1)** | `search::hybrid` imports & exposes `crate::embeddings::EmbeddingProvider` (`src/search/hybrid.rs:20,189`); embeddings also references search | Move the **`EmbeddingProvider` trait** down to `julie-core`; the sidecar-backed impl stays in `julie-pipeline`. Then `julie-index` compiles below embeddings. |
| `search ↔ analysis` cycle | cross-ref language-config types (`LanguageConfigs`, `TestRoleConfig`, `LiteralCarrierConfig`, `TestEvidenceConfig`) | **Merge search+analysis** into `julie-index` |
| `database → daemon` | `database/mod.rs:14` imports `daemon::connection_pool::PooledConn` | Move `PooledConn` down into `julie-core` |
| `utils → tools` | `tools::shared::BLACKLISTED_DIRECTORIES`, `tools::navigation::resolution::*` (`src/utils/paths.rs:9`, `src/utils/walk.rs`) | Relocate both into `julie-core` |
| `search/analysis → tools` | `tools::search::matches_glob_pattern` (`src/search/index.rs:32`, `src/analysis/early_warnings.rs:9`) | Relocate helper into `julie-core` |
| `tools ↔ indexing_core` cycle | indexing_core uses `tools::workspace::indexing::file_policy`, `ManageWorkspaceTool::extract_symbols_static` (`src/indexing_core/extraction.rs:14,308`) | Push those into `julie-pipeline`/`julie-core` |

Several raw back-edge counts are inflated by inline `#[cfg(test)]` blocks and vanish when
tests move to their crate — but the embedding edge above is **production**, not test noise.

### 3.3 The relink cure — honest mechanism and its limit
The win is **per-crate test binaries**: `cargo nextest run -p julie-core <test>` links only
`julie-core`'s test binary. **But** (Codex finding 3) the suite is centrally organized:
~80 references to `crate::tests::helpers`, and representative tool/search tests instantiate a
full `JulieServerHandler` + `ManageWorkspaceTool` (`src/tests/tools/get_symbols.rs:9,50`;
`src/tests/tools/search_quality/helpers.rs:217`; the fixture builder itself,
`src/tests/fixtures/julie_db.rs:35`).

**Therefore the win is proportional to the unit-vs-integration test split, not automatic.**
A `julie-core` test that spins up a handler relinks the whole stack. Two requirements fall
out, both handled in Phase 0:
- A **`julie-test-support`** crate for shared helpers/fixtures.
- A rule (compile- or test-enforced): **low-crate tests must not depend on handler/tools.**
  Handler-integration tests stay in the top crate's binary and run at batch boundaries;
  pure db/scoring tests relocate and go fast.

---

## 4. Phased program

> Sequencing: prove the boundary on **one** slice first (Phase 0), then complete the leaf
> split (Phase 1) so the bulk of the codebase iterates fast, then untangle the handler to peel
> tools (Phase 2), then the daemon teardown (Phase 3) inside an already-fast loop. Doing the
> daemon first = riskiest teardown while the loop is slow. Backwards.

### Phase 0 — Boundary Proof + one-shot moat measurement  *(de-risk before the broad work)*
This is the first, narrow slice of the leaf-crate split — it proves the premise.
- **0a. Dep-direction enforcement:** a compile/test gate asserting no lower crate depends on a
  higher one. Move the `EmbeddingProvider` trait to `julie-core` (resolves the `search↔embeddings`
  cycle).
- **0b. Test-support + one vertical slice:** stand up `julie-test-support`; extract **one**
  crate (`julie-core`-shaped, starting with `database`) and relocate *only* its decoupled
  tests. **Prove** editing+testing that slice relinks only its own test binary — measured
  wall-clock in a ledger row. If a slice's tests can't decouple from the handler, that's the
  signal the boundary isn't real yet.
- **0c. One-shot three-way bakeoff:** a *single* shared-corpus comparison ranking
  julie/miller/eros retrieval (reuse eros's harness + `julie-cli` baseline). Run once, read the
  ranking, record it. **This is a decision input, not a gate that re-runs on tweaks** — no
  promotion-gate machinery, no tuning loop.

**Gate to proceed:** the slice proof passes (real per-crate relink win), and the bakeoff
ranking doesn't contradict the "moat worth keeping" premise.

### Phase 1 — Complete the leaf-crate split
- Sever the remaining §3.2 back-edges; extract `julie-core` and `julie-index` (search+analysis
  merged). Relocate their decoupled tests; integration tests stay up-stack.

**Acceptance:** full suite green, behavior identical; editing `julie-core`/`julie-index` relinks
only that crate's test binary (measured); `julie-server` live smoke (search/inspect/edit) unchanged.

### Phase 2 — ToolContext facade, then peel tools / runtime / pipeline
Codex finding 2: tools are **handler-bound, not service-bound** — `fast_search` execution takes
`&JulieServerHandler` (`src/tools/search/execution.rs:47`); `get_context`/`spillover_get` reach
into handler session state. So:
- **Prerequisite:** define a **`ToolContext` / workspace-access facade** that gives tools the
  workspace, search index, spillover store, and embedding handle without the concrete
  `JulieServerHandler`. This is the start of decomposing the 24-field god-object.
- Then extract `julie-tools`, `julie-runtime` (watcher+workspace), `julie-pipeline`
  (indexing_core+embeddings impl), tests moving with them.

**Acceptance:** editing one tool relinks only `julie-tools`' test binary (measured); a tool's
tests construct a `ToolContext`, not a full handler; full suite green; live smoke unchanged.

### Phase 3 — Daemon teardown  *(coordinator-replacement spec, not a sketch)*
Codex finding 4: the daemon is more than GPU pooling. This phase is a real spec:
- **Resident embedding-host:** one process owns the single PyTorch sidecar (reuses
  `embedding_service.rs` + sidecar supervisor); ref-counted / idle-timeout; local socket.
  *The only always-on process — because N sessions each loading CodeRankEmbed into VRAM would OOM.*
- **Write leadership:** an OS leader-election lock; the lock-holder runs the **sole** watcher
  (this is what dedups watchers across processes — replacing `WatcherPool`) and owns writes.
- **Cross-process mutation lock:** the current mutation gate is **process-local**
  (`src/workspace/mutation_gate.rs:42,60`). The replacement must serialize all 8 canonical
  writers (watcher events, repair scan, repair-replay, Tantivy retry, catch-up, force-reindex,
  refresh-stats, register) **across processes**, not just within one.
- **Cross-resource recovery:** a write = SQLite write → Tantivy projection → projection-state.
  A writer killed mid-pipeline must be recoverable. **Julie already has the machinery** —
  catch-up indexing on connect + watcher repair scan/replay — so the work is *running that
  recovery on leader handoff*, not inventing atomicity. Define it explicitly.
- **Reads:** SQLite WAL (already enabled, `database/mod.rs:111`) + Tantivy mmap readers.
  Resolve the cross-process Tantivy projection protocol (validate readers survive segment
  merges, or adopt Miller's rebuild-from-SQLite-per-process model).
- **Registry/dashboard:** registry data → shared SQLite file; dashboard rides the
  embedding-host or a CLI-launched server.
- **Delete:** `adapter/` entirely, plus pid/singleton/discovery/connection_pool/
  http_transport/transport/legacy_migration and most of the daemon registry server.
- **Prerequisite:** capture a reproducible `fast_search` hang / `deep_dive` disconnect repro
  *first*, so the teardown is verified against it.

**Acceptance:** 3 concurrent sessions share one sidecar (one model in VRAM); killing the writer
degrades freshness only and a new leader recovers via repair/catch-up (verified); `adapter/`
deleted, daemon reduced to lock/coord + embedding-host; the captured repro no longer occurs.

### Phase 4 — Tool taxonomy 12→7 + harvests  *(independent; interleave)*
- **Consolidate to 7 tools:** edit trio → `edit(operation=…)`; `fast_refs`+`call_path` →
  `trace(mode=refs|path)`; demote `spillover_get` to a continuation convention; shared read-tail
  (`format`, `workspace_id`, `ensure_fresh`).
- **Enforce in code:** APPROVED-tool-list assertion (count can't silently regrow).
- **Self-enforcing test gate:** a convention test that **fails the build** if a subprocess/slow
  test isn't tagged heavy (Miller's pattern) — turns CLAUDE.md discipline into mechanism.
- **token-ROI telemetry:** record source_bytes vs output_bytes per tool call (eros) — measure
  "intelligence per token."
- **Dropped (per owner):** no formal recurring retrieval promotion gate — the existing
  `search_quality` dogfood bucket already guards regressions; we don't build tuning-loop machinery.

**Acceptance:** MCP surface = 7 tools, enforced; untagged slow test fails CI; tool-call telemetry visible.

---

## 5. Risks & open questions
- **R1 — Cross-process Tantivy + projection recovery** (Phase 3). The single biggest unknown:
  not just "readers survive merges," but the SQLite↔Tantivy projection protocol under
  writer-kill. Mitigated (not erased) by Julie's existing repair/catch-up machinery.
- **R2 — Handler decomposition is a gating interface problem, not friction.** The `ToolContext`
  facade (Phase 2) is now a named deliverable; if it's harder than expected it gates the tools crate.
- **R3 — Moat is measured once, early.** Phase 0c's one-shot bakeoff replaces the v1 "assert the
  moat" hand-wave. Explicitly *not* an iterative gate.
- **R4 — Migration churn.** The split rewrites imports tree-wide; do it on `julie-rescue`,
  lowest crate first, existing suite as guard, green between extractions.
- **R5 — Test decoupling cost.** Some current tests can only be "unit" after they stop
  instantiating a handler; that rewrite cost is real and surfaces in Phase 0's slice proof.

## 6. Explicitly NOT doing
- Not switching the host to .NET/Python. Not rewriting from scratch.
- Not dropping embeddings/semantic search (the moat; VRAM-pooled via the embedding-host).
- Not building a recurring bakeoff / retrieval promotion-gate / tuning loop — one comparison, decide.
- Not copying eros's repo-specific path-penalty heuristics (violates the language-agnostic mandate).
- Not adding an output format switch as a "token win" — output already defaults to plain text.

---

## Appendix A — measured evidence (HEAD 5bdcd785, 2026-06-03)
- src/ total: 210,082 LOC (84,111 non-test impl / 125,971 test) across 694 files.
- Top non-test LOC by module: tools 25,044 · daemon 10,126 · database 9,035 · search 7,741 ·
  dashboard 4,136 · embeddings 3,692 · watcher 3,020 · analysis 2,667 · utils 2,644 ·
  handler 2,518 · adapter 1,343 · indexing_core 838.
- Tests: 2,727 functions, 395 files; dev tier ~1,914s expected sequential bucket time.
- Daemon: 10,126 LOC / 12 test markers. Adapter: 1,343 LOC.
- Dependency back-edges/cycles blocking the split: see §3.2 (all bounded, all severable).

## Appendix B — Codex (gpt-5.5, high) adversarial review, 2026-06-03
Verdict: would not sign off on the v1 Phase 1 as written; architecture risk High. Findings,
all incorporated above: (1) missed production `search→embeddings` edge → trait-down fix;
(2) handler/tool coupling is a gating interface problem → ToolContext facade promoted to a
Phase 2 deliverable; (3) test relocation underplayed → `julie-test-support` + decoupling rule +
Phase 0 slice proof; (4) daemon is more than an embedding resident → Phase 3 rewritten as a
coordinator-replacement spec; (5) verdict premature → one-shot moat bakeoff moved to Phase 0.
Claude's one push-back: v1's "split first" survives — leaf *production* extraction is viable;
the real gate is test-coupling + the embedding trait, which Phase 0 now proves.
