# Search Consolidation Plan

**Status:** Draft — awaiting kickoff
**Created:** 2026-05-17
**Owner:** main session (Claude) + Codex review loop
**Branch base:** `main` @ commit `83189a45`

---

## Problem

Julie's search ranking pipeline has accreted nine post-retrieval layers
(weighted Tantivy → AND/OR fallback → RRF rescaling → centrality →
reranker → NL path prior → language affinity → exact-name promotion →
DB rescue). Each layer was added in response to a specific failure.
Each was individually defensible. The cumulative result:

- Nobody can predict ranking behavior on an unseen query.
- A controlled benchmark from a sibling project (eros — later identified
  as benchmark-overfit) put julie at 0.67 MRR vs eros 0.98, with the
  biggest gaps in test-intent (0.21 vs 1.00) and symbol-intent (0.53 vs
  0.98). Even discounting overfit, the gap is real.
- Codex review of the pipeline (2026-05-17) found two concrete defects
  we shipped without noticing and a category of complexity that's not
  earning its keep.

The fix is **consolidation gated by ablation**, not "add another layer."

---

## Findings driving this plan

### Confirmed bugs (must fix regardless of ablation outcome)

**B1. NL path prior applied twice.**
`SearchIndex::search_symbols` (`src/search/index.rs:602`) applies
`apply_nl_path_prior`. `definition_search_with_index` calls it again
(`src/tools/search/text_search.rs:592`). Same candidate gets the
multiplier applied twice on the keyword path. Caught by Codex.

**B2. `capability_flags` is dead schema baggage.**
Schema field at `src/search/schema.rs:95`. Stored as empty
(`src/search/index.rs:387`). Carried through `SymbolSearchResult` but
not consumed by the reranker (`src/search/reranker.rs:62`). One of 19
schema fields earning nothing. Cleanup, no quality risk.

**B3. The reranker has two integration paths.**
The public `rerank()` method (`src/search/reranker.rs:217`) is mostly
test-surface. Production rebuilds `Candidate`s and calls
`rerank_symbol_score` per result with duplicated intent logic
(`src/tools/search/text_search.rs:348-372`). Two paths to maintain.

### Suspected non-earners (require ablation before deletion)

**S1. The reranker.**
Comment in `src/tools/search/text_search.rs:237` records that flag-on
dogfood matched flag-off baseline 33/35. The reranker may add nothing
on real queries. ~1000 lines of code if it goes.

**S2. The hybrid (semantic + RRF) path.**
Hybrid degrades cleanly to keyword-only when no embedding provider is
available (`src/search/hybrid.rs:186`). The Python sidecar + PyTorch +
CodeRankEmbed dependency cost is paid by default
(`Cargo.toml:34`, `src/embeddings/factory.rs:81`). Dogfood comparisons
haven't shown hybrid pulling obviously better answers on
identifier-shaped queries.

**S3. The RRF→BM25 ×200 rescaling.**
`src/tools/search/text_search.rs:503`. A band-aid because layers were
designed for different score magnitudes. If hybrid stays, this needs a
principled replacement. If hybrid goes, this goes with it.

### Codex-corrected items (claude was wrong)

- Public targets are **three** (content / definitions / files), not four.
  Annotations is query syntax inside definition search.
- "Collapse to one packed search doc" misses that **content target uses
  neutral score 0.0** intentionally (`src/tools/search/execution.rs:225,262`).
  Conflating it with symbol scoring would break that semantic.
- **DB rescue is not a centrality problem.** It rescues qualified-name
  truncation cases like `Phoenix.Router` when the user searched just
  `Router` (`src/tools/search/text_search.rs:613`). A stored field
  weight on centrality would not fix this.
- **Daemon lifecycle complexity is load-bearing.** Singleton startup,
  watcher pooling, drain on shutdown all justified. Refactor, don't
  delete.

---

## Plan: ordered work items

Each step is a separate commit. Each ends with a verification row in
the ledger below.

### Phase 1: Fix the confirmed bugs

**P1.1 — Eliminate the double-applied NL path prior.**
Decide which call site owns the prior — `SearchIndex::search_symbols`
(closer to candidate retrieval) or `definition_search_with_index`
(part of the assembled pipeline). Remove the duplicate.

- Tests: RED test asserts a single multiplier is applied to the same
  candidate; GREEN after the fix.
- Files: `src/search/index.rs:602`, `src/tools/search/text_search.rs:592`.
- Risk: Low. Removing a duplicate operation; deterministic.

**P1.2 — Remove `capability_flags` from the Tantivy schema and
SymbolSearchResult.**
Schema cleanup. Touches indexing (write empty), retrieval (don't read),
and any code reading the field.

- Tests: schema compatibility signature test will detect the change;
  regenerate. Field removal must not break the FTS index format check.
- Files: `src/search/schema.rs`, `src/search/index.rs`, any consumer.
- Risk: Low–medium. Index migration required (force-reindex on first
  load with new binary). Document in commit message.

### Phase 2: Build the ablation harness

**P2.1 — Author a small but real labeled query set.**
Not the eros corpus (overfit). Author 40–60 queries from real
coding-agent traces or human-written queries, spanning:
- Exact-symbol lookup (CamelCase, snake_case, qualified names)
- Symbol-intent (multi-word that should resolve to one symbol)
- Test-intent (covers the bug we just fixed)
- File-path lookup
- Concept/behavior queries ("how does X work")
- Reference/caller queries

Each entry: `{query, expected_paths, category, source}`. Store at
`docs/eval/julie-search-corpus-v1.json`. Commit with the corpus.

- Risk: Low. This is data, not code.

**P2.2 — Ablation runner (`xtask` subcommand or test bucket).**
Run the labeled corpus through four modes:
- `keyword-only` (`JULIE_RERANKER_ENABLED=0`, `JULIE_EMBEDDING_PROVIDER=none`)
- `keyword+reranker` (`JULIE_EMBEDDING_PROVIDER=none`)
- `hybrid-only` (`JULIE_RERANKER_ENABLED=0`)
- `hybrid+reranker` (default)

Capture per-query: top-1 path, top-1 relevant (bool), MRR@10, latency.
Aggregate by category and total. Emit JSON + a delta table.

- Files: new `xtask/src/search_ablation.rs` (or similar); shares the
  julie binary in standalone mode for determinism.
- Risk: Medium. The harness itself is novel code; treat as a small
  project, not a one-line tool.

**P2.3 — Baseline run.**
Run the ablation against current `main`. Commit results at
`docs/eval/julie-search-ablation/<date>-<commit>-baseline.json`.

### Phase 3: Cut what isn't earning

Each of these is a separate PR. Each is gated on ablation evidence.

**P3.1 — Reranker decision.**
If ablation shows reranker delivers <5% MRR lift across any category,
remove. If it shows lift only on one category, narrow it to that path.
- Files affected if removed: `src/search/reranker.rs`,
  `src/search/query_parse.rs`, reranker invocations in
  `src/tools/search/text_search.rs:322,408`, schema fields
  (`role`, `test_role`) if no longer consumed.

**Decision recorded 2026-05-17 (commit `287a64d9`, updated post-Codex
review): DEFER reranker removal/narrowing — evidence is too thin to
take action either way.**

The original framing here said "KEEP as-is." Codex review correctly
pointed out that's too definitive: the data isn't strong enough to
justify a positive "keep" verdict, only to justify *not* taking an
action that could regress something we can't currently measure.

Baseline data
(`docs/eval/julie-search-ablation/2026-05-17-ccffdab2-baseline.json`,
re-emitted after the schema renames below):
- Global MRR@10: keyword 0.324 → +reranker 0.332 (+2.5%, below the
  5% threshold the plan named for "remove if no category clears it").
- Per-category lift: only `concept` moves (0.113 → 0.150, +33%).
  exact-symbol, qualified-name, test-intent, symbol-intent: no
  measurable change. file-path goes through `search_files`, not the
  reranker.
- Inspecting per-query data, the *entire* concept-category lift comes
  from one query (`concept-9 "schema compatibility detection and
  rebuild"`, rank 8 → rank 2). With 10 queries per category, that's
  one data point. A t-test wouldn't rescue n=10 with one moving
  point; expanding the corpus is the only honest way out.

Latency cost is ~4ms mean (16.5 → 20.6ms) — not large enough to
force a cut on its own.

What "defer" means in practice: leave the code alone, but the next
person who touches this area should rerun the harness against an
expanded corpus before assuming the reranker is load-bearing.

Follow-up before re-evaluating:
1. Expand corpus to 25-30 queries per category (especially `concept`
   and `symbol-intent`) so per-category MRR has statistical power.
2. Regenerate the fixture so the indexed code matches what the
   corpus references (current fixture is Feb 27, corpus authored
   May 17 — some `expected_paths` may not exist in the fixture at
   all, depressing absolute numbers uniformly across modes).
3. Rerun ablation; revisit P3.1 with a stronger signal.

**P3.2 — Hybrid/embeddings decision.**
If ablation shows hybrid delivers <5% MRR lift over keyword-only
(reranker held constant), make embeddings opt-in (off by default).
Sidecar startup, PyTorch dependency, CodeRankEmbed model download
become user-flag-gated.
- Files: `src/embeddings/factory.rs:81`, `Cargo.toml:34`,
  `src/search/hybrid.rs` (still available when opt-in), docs.

**Decision deferred 2026-05-17: data-blocked, not silent-deferred.**

The P2.2 ablation harness ran the corpus through keyword-only and
keyword+reranker modes. The two hybrid modes were structurally wired
but `status="skipped"` because the fixture at
`fixtures/databases/julie-snapshot/symbols.db` has no
`symbol_embeddings` table — without precomputed embeddings, KNN
returns nothing and "hybrid" degenerates to keyword.

DoD #5 ("at least one of S1/S2 has a decision recorded") is satisfied
by P3.1 above. P3.2 explicitly cannot be settled without hybrid
numbers.

What unblocks P3.2:
1. **Easiest path:** extend the harness with `--source <julie_home>`
   to point at `~/.julie/indexes/<id>/` (the daemon's storage, which
   has embeddings populated by the sidecar). One-time copy to a temp
   dir to avoid contention with the running daemon, then run all 4
   modes in-process. Estimated ~30-60 min of harness work.
2. Regenerate the fixture with `embeddings-sidecar` feature enabled
   so the SQLite snapshot has `symbol_embeddings` populated. Also
   makes hybrid runs reproducible offline.

When either lands, rerun `cargo xtask eval ablation` and record P3.2
in this section with the same level of per-category breakdown as
P3.1.

**P3.3 — Consolidate post-processing in `definition_search_with_index`.**
After bug fixes and ablation cuts, the assembled pipeline should have
ONE pass each of: centrality, path prior, language prior, exact-name
promotion, optional DB rescue. No duplicate invocations, no scattered
re-sorts.
- File: `src/tools/search/text_search.rs:459-701`.

**Verified 2026-05-17 (commit `287a64d9`): pipeline already has the
canonical one-pass structure.** Audit of `definition_search_with_index`:

| Concern | Hybrid branch | Keyword branch |
|---|---|---|
| centrality boost | 1× (`text_search.rs:527`) | 1× (`text_search.rs:585`) |
| reranker | 1× (`text_search.rs:529`) | 1× (`text_search.rs:588`) |
| NL path prior | 1× (`text_search.rs:533`) | 1× (`text_search.rs:589`) |
| language affinity | 1× (`text_search.rs:537`) | 1× (`text_search.rs:591`) |
| exact-name promotion | 1× (`text_search.rs:538`) | 1× (`text_search.rs:592`) |
| DB rescue | none (intentional — hybrid already has semantic recall) | 1× (`text_search.rs:613-690`) |

P1.1 was the only real duplicate (NL path prior at both retrieval and
assembly). After that fix, no other post-processing pass is invoked
twice. `apply_important_patterns_boost` (a `×1.5` name-pattern boost)
still lives inside `SearchIndex::search_symbols` / `_relaxed` rather
than at the assembly layer — an architectural quirk inconsistent with
the B1 contract, but it's applied exactly once per query (the AND and
OR paths are mutually exclusive), so it does not cause double
application. Moving it to the assembly layer would change behavior
(centrality + reranker both consume the boosted score); deferring
until ablation shows it helps or hurts.

No code change required for P3.3.

### Phase 4: Refactor (not delete) the daemon

Out of scope for this plan — separate plan doc. Codex flagged
`src/daemon/mcp_session.rs:91` as the consolidation target for
admission/shutdown/session cleanup. Track separately.

---

## What this plan does NOT do

- Does not collapse the three search targets into one. Codex was right
  that content's neutral-score semantics make this not a slam-dunk;
  revisit only if Phase 3 leaves obvious need.
- Does not redesign the schema (beyond removing `capability_flags`).
  Schema is fine; the problem is the post-processing stack on top.
- Does not touch the daemon. Separate plan.
- Does not chase eros's MRR target. The eros benchmark is overfit.
  Julie's quality target is "agents pick the right answer on real
  queries"; the labeled corpus is the measure.

---

## Definition of done

1. Both confirmed bugs (B1, B2) shipped with regression tests.
2. Labeled query corpus exists, committed.
3. Ablation harness exists, runnable from xtask.
4. Baseline ablation result captured at HEAD.
5. At least one of the suspected non-earners (S1 reranker, S2 hybrid)
   has a decision recorded in the ledger backed by ablation numbers.
6. `definition_search_with_index` has exactly one pass per
   post-processing concern after cuts land.
7. `cargo xtask test changed` passes after each commit.
8. `cargo xtask test full` passes before the plan is closed.

---

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| P1.1 NL path prior applied exactly once across keyword pipeline | `cargo nextest run --lib nl_path_prior_pipeline_tests` | targeted | `00b71b37` | PASS (3/3) | 2026-05-17T20:04Z | no |
| P1.1 no search regressions | `cargo xtask test changed` | tooling+dogfood | `00b71b37` | PASS (11 buckets, 305.7s) | 2026-05-17T20:04Z | no |
| P1.1 dev tier green | `cargo xtask test dev` | dev | `00b71b37` | PASS (32 buckets, 469.0s) | 2026-05-17T20:04Z | no |
| P1.2 capability_flags removed without regression | `cargo xtask test changed` | tooling+dogfood+daemon+system | `88dc0930` | PASS (16 buckets, 392.2s) | 2026-05-17T20:14Z | no |
| P1.2 dev tier green | `cargo xtask test dev` | dev | `88dc0930` | PASS (32 buckets, 481.4s) | 2026-05-17T20:21Z | no |
| P2.2 ablation harness builds and runs end-to-end | `cargo run -p xtask -- eval ablation --out /tmp/ablation-smoke.json` | xtask | `1f77cd93` | PASS (45 queries × 2 ran modes, 2 skipped) | 2026-05-17T20:55Z | no |
| P2.3 baseline ablation captured at HEAD | `cargo run -p xtask -- eval ablation` | xtask | `1f77cd93` | PASS — keyword-only MRR@10=0.324, keyword+reranker MRR@10=0.332 (+2.5% global; concept-category +33%). Hybrid modes skipped — fixture has no embeddings. | 2026-05-17T20:57Z | no |
| P3.1 decision recorded (KEEP reranker, revisit after corpus expansion) | manual review of baseline JSON | docs-only | `287a64d9` | PASS — rationale documented above; no code change | 2026-05-17T21:30Z | no |
| P3.3 verified — pipeline has exactly one pass per concern | manual code audit of `definition_search_with_index` | docs-only | `287a64d9` | PASS — audit table documented above; no code change | 2026-05-17T21:30Z | no |
| Phase 3 batch — dev tier green | `cargo xtask test dev` | dev | `eb429d58` | PASS (32 buckets, 490.3s) | 2026-05-17T21:38Z | no |
| Codex review applied: hybrid NL-prior contract tests pinned | `cargo nextest run --lib nl_path_prior_pipeline_tests` | targeted | (this commit) | PASS (5/5, 0.44s) | 2026-05-17T22:00Z | no |
| Codex review applied: harness fixes (env RAII, mrr_at_k rename, symbol_vectors probe) compile & rerun clean | `cargo run -p xtask -- eval ablation` | xtask | (this commit) | PASS — identical metrics (keyword 0.324, keyword+reranker 0.332) with corrected JSON schema | 2026-05-17T22:00Z | no |

---

## Open questions (track, don't drift)

- Q1: Should the labeled corpus include cross-language workspaces
  (e.g., a Vue + Rust project) to exercise the language affinity
  prior? Probably yes — answer when authoring the corpus.
- Q2: If ablation shows reranker lift only on test-intent queries,
  is it cheaper to keep the reranker for that category or to lift
  test-intent handling into the path prior we just added? Defer.
- Q3: `capability_flags` removal — does any external consumer
  (dashboard, MCP client) read this field? Check before P1.2.

---

## Non-goals captured to prevent drift

- "Just add `<new layer>` to fix `<new failure>`" — not without
  ablation showing existing layers can't be tuned to handle it.
- "Rewrite Tantivy schema to mirror eros" — explicitly rejected.
- "Match eros's 0.98 MRR" — explicitly rejected. The number is
  benchmark-fit.
- "Cut the daemon" — explicitly rejected. Refactor only, separate plan.
