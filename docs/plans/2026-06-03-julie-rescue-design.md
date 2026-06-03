# Julie Rescue: De-slop the Packaging, Keep the Moat

**Date:** 2026-06-03
**Status:** Design — approved direction, pending written-spec review
**Authors:** Alan + Claude (brainstorm session)
**Supersedes the implicit question:** "Can we save Julie, or does Miller replace her?"

---

## TL;DR

Julie's momentum problem is **packaging, not language.** Two deletable structures are
strangling iteration: a **monolithic lib crate** that relinks 126k test LOC on every edit,
and a **bespoke ~11.5k-LOC daemon** that is both accidental complexity and the home of the
unsolved hang/disconnect bugs. Neither is a Rust verdict. We **save Julie in place** by
fixing the packaging, and we **harvest** the best ideas from Miller and eros (which are
already-built proofs of the ".NET host" and "Python host" rewrites) rather than switching
hosts and re-earning Julie's moat.

The moat we are protecting — and that Miller/eros both dropped to look clean — is:
semantic/hybrid search + graph-centrality reranking + token-budgeted `get_context` +
true 34-language breadth + a shipped plugin and CLI.

---

## 1. Diagnosis (evidence-backed)

### 1.1 The relink tax (the acute pain)
- `src/lib.rs` pulls the entire `src/tests/` tree — **395 files, ~126k LOC, 2,727 test
  functions, 60% of the crate** — into a single `#[cfg(test)]` test binary.
- That binary **relinks on every source edit**, including before running a *single*
  targeted test (`cargo nextest run --lib <one_test>` still links the monolith first).
- **Consequence:** "run the narrowest test" discipline cannot help — narrowing cuts
  *execution* time, not *relink* time. Under agentic development (tests re-run ~20× per
  change), the dev tier's ~32 min of bucket time compounds into hours.
- `cargo check` is **3.6s** on 210k LOC — Rust is not the bottleneck. The packaging is.

### 1.2 The daemon (the chronic pain)
- `src/daemon/` = **10,126 LOC**, `src/adapter/` = **1,343 LOC**, with only **12 test
  markers** — i.e. ~11.5k of *production* code.
- Roughly **7–9k of that is pure "we run a bespoke daemon" tax**: `pid.rs` (682 LOC), a
  second SQLite registry DB (~1,360 LOC), discovery/singleton/token-file (813),
  HTTP-bridge + connection pools (1,134), lifecycle/shutdown/legacy-migration/app (~2,450),
  and the entire `adapter/` (1,343) which exists *only* to bridge stdio↔daemon-HTTP.
- This is where the never-root-caused `fast_search` hang / `deep_dive` disconnect lives.

### 1.3 God-objects
- `handler.rs` = 2,518 LOC / 95 methods / a 24-field state struct reaching into every
  subsystem. `search/index.rs` = 2,249 LOC. Both are bug blast-centers and the coupling
  that makes the above two problems hard.

### 1.4 What is NOT broken
- The 12-tool surface is lean (not dozens), with thoughtful cross-referencing descriptions.
- The xtask bucket/tier/`changed`-selection runner is genuinely well-engineered.
- Output already defaults to **optimized plain text**, not verbose JSON (verified:
  `src/tools/search/` renders via text builders; `format` = `full`|`locations`, both text).
- Extraction is already externalized to `julie-extractors` (consumed as a pinned git-dep).

---

## 2. Strategic verdict

**Save Julie. Harvest Miller and eros. Do not switch hosts.**

| Option | Verdict |
|---|---|
| (A) Save in place: crate-split + daemon teardown + tool consolidation | **CHOSEN.** Fixes the fixable; preserves the moat. |
| (B) Replace with Miller (.NET) | Rejected now. Miller is clean (4.7s build, 1,303 tests in 4s) but has **no embeddings, no centrality ranking, a .NET-only cross-language bridge, no CLI, no plugin.** Switching = re-earning two years. |
| (C) Re-platform host in Python/TS over Rust cores | Already built — that's **eros** (Python) and **Miller** (.NET). Both shed the moat. Realistic for batch/index/analyze; the live watcher+daemon must stay a resident Rust process. Long-game shape, not a near-term rescue. |

**Key reframe:** all three projects consume the *same* `julie-extract` binary. The parser
layer is settled and shared, so this entire question is about the **host** layer. The two
rewrites the owner mused about already exist as Miller and eros; their lesson is "you can be
fast by dropping the moat," which is exactly the trade we decline.

---

## 3. Validated target architecture

### 3.1 Crate DAG (bottom → top)
```
julie-core      database + utils + paths + extractors(re-export) + relocated shared types
   ↑            (Symbol/Identifier/Relationship model comes from external julie-extractors)
julie-index     search + analysis  (MERGED — they cycle on language-config types)
   ↑
julie-pipeline  indexing_core (extract→persist orchestration) + embeddings client
   ↑
julie-tools     the MCP tools
   ↑
julie-runtime   watcher + workspace
   ↑
julie-daemon    daemon + adapter   (gutted in Phase 3)
   ↑
julie-server    handler + dashboard + startup + health + cli + bins   (top)
```
Granularity is a target, not a mandate — extract lowest-first and only as far as each phase
needs. Start coarse; refine if a boundary proves load-bearing.

### 3.2 Why this DAG is real (measured `crate::` edges, production code)
The split is blocked by a **bounded** set of back-edges and one cycle, all of which Phase 1
severs *before* extraction:

| Back-edge / cycle | Evidence | Fix |
|---|---|---|
| `search ↔ analysis` cycle | cross-ref language-config types (`LanguageConfigs`, `TestRoleConfig`, `LiteralCarrierConfig`, `TestEvidenceConfig`) | **Merge search+analysis** into `julie-index` |
| `database → daemon` | `database/mod.rs:14` imports `daemon::connection_pool::PooledConn` | Move `PooledConn` down into `julie-core` |
| `utils → tools` | `tools::shared::BLACKLISTED_DIRECTORIES`, `tools::navigation::resolution::*` | Relocate both into `julie-core` |
| `search/analysis → tools` | `tools::search::matches_glob_pattern` (3 sites) | Relocate helper into `julie-core` |
| `tools ↔ indexing_core` cycle | indexing_core uses `tools::workspace::indexing::file_policy`, `ManageWorkspaceTool::extract_symbols_static`, `tools::shared` | Push those into `julie-pipeline`/`julie-core` |

Several raw back-edge counts (e.g. `embeddings→tools:14`) are dominated by inline
`#[cfg(test)]` blocks and **disappear when tests move to their crate** — production coupling
is even smaller than the graph suggests.

### 3.3 The relink cure mechanism
The win is **per-crate test binaries.** Today: one 126k-LOC test binary, relinked on any
edit. After the split: `cargo nextest run -p julie-tools <test>` links only `julie-tools`'
test binary. Editing a top crate (`julie-server`) recompiles nothing downstream. This is
why moving tests to a top-level `tests/` dir is rejected — those still link the whole lib.

---

## 4. Phased program

> Sequencing rationale: split the clean lower layers **first** (most LOC, most tests, fewest
> upward deps → biggest relink win at lowest risk), so the high-risk daemon surgery happens
> *inside an already-fast loop*. Doing the daemon first means the riskiest teardown while the
> loop is still 20-min-×-20. That's backwards.

### Phase 1 — Untangle + leaf-crate split  *(the relink cure; start here)*
**1a. Untangle (behavior-preserving, lands in the monolith first):**
- Relocate the back-edge symbols from §3.2 down to their correct layer.
- Consolidate the search/analysis shared language-config types into one home.
- No behavior change; the *existing* suite is the guard.

**1b. Workspace conversion + extract bottom crates:**
- Convert to a Cargo workspace; extract `julie-core` and `julie-index`.
- Move their tests (`src/tests/core/**`, search/analysis tests) into the crates.

**Acceptance criteria:**
- [ ] `cargo check` and full `cargo xtask test dev` (or successor) green, behavior identical.
- [ ] Editing a file in `julie-core` or `julie-index` and running one of its tests relinks
      **only that crate's test binary** — measured wall-clock recorded in a ledger row.
- [ ] No `crate::`-equivalent back-edge from a lower crate to a higher one (enforced by the
      workspace compiling at all + a documented dep-direction check).
- [ ] `julie-server` binary builds and passes a live smoke (search/inspect/edit) unchanged.

### Phase 2 — Peel off tools / runtime / pipeline
- Extract `julie-tools`, `julie-runtime` (watcher+workspace), `julie-pipeline`
  (indexing_core+embeddings). Move their tests with them.
- After this, the bulk of LOC and tests are out of the top crate; `handler`/`daemon` remain.

**Acceptance criteria:**
- [ ] Editing a single tool relinks only `julie-tools`' test binary (measured).
- [ ] Full suite green; live smoke unchanged.

### Phase 3 — Daemon teardown  *(delete the slop + the bug nest)*
Replace the bespoke daemon with the separated-concerns model:
- **GPU/VRAM pooling** → one tiny resident **embedding-host** process owns the single
  PyTorch sidecar (reuses `embedding_service.rs` + sidecar supervisor); ref-counted /
  idle-timeout; reached over a local socket. *This is the only always-on process, and it
  exists because N sessions each loading CodeRankEmbed into VRAM would OOM.*
- **Write coordination** → SQLite WAL (concurrent readers) + an OS leader-election lock; the
  lock-holder runs the watcher/indexer. No HTTP bridge, no pid dance, no adapter.
- **Registry data** (workspaces, tool-call history) → a plain shared SQLite file (WAL append).
- **Cross-workspace targeting** → open the other workspace's index files directly.
- **Dashboard** → rides the embedding-host or a CLI-launched local server.
- **Delete:** `adapter/` entirely, plus pid/singleton/discovery/connection_pool/
  http_transport/transport/legacy_migration and most of the daemon registry server.

**Embedded sub-fork (must resolve in this phase):** SQLite WAL cleanly shares the *symbol DB*
across processes, but the **Tantivy search index** is a separate on-disk store. Multiple
reader processes + one writer against a shared on-disk Tantivy index needs validating
(Tantivy enforces a single-writer lock; cross-process readers surviving segment merges is the
unproven part). Miller dodged this by **rebuilding a small in-memory BM25 index per process
from SQLite.** Decide: validate cross-process Tantivy, or adopt Miller's rebuild model.

**Acceptance criteria:**
- [ ] Three concurrent sessions share **one** sidecar (one model resident in VRAM) — verified.
- [ ] Killing the writer session degrades freshness only; reads continue; next session takes
      the lock (failover verified).
- [ ] `src/adapter/` deleted; daemon LOC reduced to a few hundred (lock/coord) + embedding-host.
- [ ] The reproducible hang/disconnect repro (to be captured first) no longer occurs.

### Phase 4 — Tool taxonomy 12→7 + harvests  *(independent; interleave freely)*
- **Consolidate to 7 tools:** merge edit trio → `edit(operation=…)`; fold
  `fast_refs`+`call_path` → `trace(mode=refs|path)`; demote `spillover_get` to a
  continuation convention; adopt a shared read-tail (`format`, `workspace_id`, `ensure_fresh`).
- **Enforce in code:** an APPROVED-tool-list assertion so the count cannot silently regrow
  (eros's pattern).
- **Self-enforcing test gate:** a convention test that **fails the build** if a
  subprocess/slow test isn't tagged `Scale`/heavy (Miller's pattern) — turns CLAUDE.md
  discipline into mechanism.
- **token-ROI telemetry:** record source_bytes vs output_bytes per tool call (eros) — finally
  *measure* "intelligence per token."
- **Optional later:** formal retrieval promotion gate; test-confidence subsystem.

**Acceptance criteria:**
- [ ] MCP surface = 7 tools; APPROVED-list test enforces it.
- [ ] Adding a slow test without the heavy tag fails CI locally.
- [ ] Tool-call telemetry visible in the dashboard.

---

## 5. Risks & open questions
- **R1 — Tantivy cross-process sharing** (Phase 3 sub-fork above). The single biggest
  unknown in the daemon teardown.
- **R2 — handler.rs decomposition.** Its 24-field struct is the coupling that the crate split
  pushes against; some Phase 2/3 friction will be untangling it into focused services.
- **R3 — No shared-corpus retrieval benchmark.** Julie's semantic+centrality "moat" is
  asserted, not measured against Miller's BM25+bridge or eros's lancedb-hybrid. eros already
  has a bakeoff harness with a `julie-cli` baseline. **A shared-corpus bakeoff is the one
  number that could revise the verdict** — run it before betting the next year. (No-regret:
  it also seeds Phase 4's promotion gate.)
- **R4 — Migration churn.** The split rewrites imports across the tree; do it on a branch with
  the existing suite as the guard, lowest crate first, green between each extraction.

## 6. Explicitly NOT doing
- Not switching the host to .NET/Python. Not rewriting from scratch.
- Not dropping embeddings/semantic search (the moat; VRAM-pooled via the embedding-host).
- Not copying eros's repo-specific path-penalty heuristics (violates the language-agnostic mandate).
- Not adding a format switch as a "token win" — output already defaults to plain text.

---

## Appendix A — measured evidence (HEAD 5bdcd785, 2026-06-03)
- src/ total: 210,082 LOC (84,111 non-test impl / 125,971 test) across 694 files.
- Top non-test LOC by module: tools 25,044 · daemon 10,126 · database 9,035 · search 7,741 ·
  dashboard 4,136 · embeddings 3,692 · watcher 3,020 · analysis 2,667 · utils 2,644 ·
  handler 2,518 · adapter 1,343 · indexing_core 838.
- Tests: 2,727 functions, 395 files; dev tier ~1,914s expected sequential bucket time.
- Daemon: 10,126 LOC / 12 test markers. Adapter: 1,343 LOC.
- Dependency back-edges/cycles blocking the split: see §3.2 (all bounded, all severable).
