# Miller Retrieval Port: Design

Date: 2026-04-19
Status: Proposed — each experiment gates its own merge on benchmark evidence

## Context

Miller's 2026-04 retrieval experiments built and benchmarked a series of single-engine LanceDB improvements that lifted Miller's own `answer_quality_per_token` but still trail Julie by 21x (`0.000100` vs `0.002101`). Julie's current stack — custom code-aware Tantivy tokenizers, sqlite-vec, SQLite canonical storage — already beats every Miller variant tested to date, including the split-engine LanceDB + Tantivy prototype.

This design answers the open question "which Miller-side tricks are worth bringing into Julie?" after an evidence-backed audit of Julie's current retrieval code (2026-04-19) showed Julie already has most of the Miller retrieval ingredients: query variant expansion (`src/search/expansion.rs`), RRF merge (`src/search/hybrid.rs`), query intent classification (`src/search/weights.rs`), code-aware tokenization (`src/search/tokenizer.rs`), source-only rescue (`src/tools/search/text_search.rs:335-399`), and the `return_format="locations"` token escape.

The five experiments below cover the gaps the audit found. The vector-store swap question (sqlite-vec → LanceDB) was explicitly rejected — Miller has been on LanceDB the whole time and still trails, so the storage layer is not where the gap lives.

## Approach

Each experiment ships as a labeled, individually-benchmarked change. Per Miller's retrieval doctrine:

1. Primary metric: `answer_quality_per_token` against Julie's current baseline (`julie-dogfood-baseline.json` in Miller's benchmark suite).
2. Secondary metrics: latency, total tokens, per-case quality.
3. One experiment = one artifact = one decision record. If the experiment regresses, it does not merge; the decision record explains what was tested and why it failed.
4. Complexity is a cost. If an experiment shows only marginal improvement, prefer simpler alternatives or defer.

The Miller benchmark suite already emits the `canonical-v1` shared case set (`symbol-lookup-01`, `error-handling-01`, `navigation-01`, `filtering-01`), and cross-system comparison via `python/benchmarks/compare_results.py` is proven working. Individual per-experiment implementation plans will be authored in this directory when each experiment begins.

**Ordering (low-risk-first, quality-before-efficiency):**

1. Broader default noise filters
2. Symbol-kind weighting in RRF
3. Public API alias expansion
4. Entrypoint-aware priors
5. Token-budgeted response shaping — gated on whether 1-4 close the gap enough

## Experiments

### Experiment 1: Broader default noise filters

Julie's existing test-exclusion default only triggers for NL queries. Extend it to also auto-exclude docs, fixtures, and memory directories (`.memories`, `docs/`, `fixtures/`) when `search_target` is not `definitions` and the query is code-investigation-shaped. Preserve opt-in for querying those paths when explicitly wanted.

**Extend:**
- `src/tools/search/text_search.rs:70-73` — `exclude_tests_resolved` seam
- `src/search/scoring.rs:155-253` — path detection heuristics

**Add:** `default_code_noise_excluded(path)` helper in `scoring.rs`, resolved at the same seam as `exclude_tests_resolved`.

**Acceptance:**
- NL code queries default-exclude docs/fixtures/memory directories alongside tests
- Explicit `include_docs=true` or `search_target="definitions"` bypasses the exclusion
- Benchmark artifact `julie-broader-filters-retrieval.json` beats baseline on AQPT or fails honestly in a decision record

### Experiment 2: Symbol-kind weighting in RRF

Julie indexes `kind` but gives it no scoring weight. Add per-kind weights into RRF so function/class/method results out-rank field/parameter/local results for code-investigation queries. Conceptual queries keep current behavior.

**Extend:**
- `src/search/weights.rs:55-109` — `classify_query()` and per-intent weight profiles
- `src/search/hybrid.rs:50-115` — `weighted_rrf_merge()`, apply kind multipliers at merge time

**Add:** `KIND_WEIGHTS` table in `weights.rs` keyed by `QueryIntent`. Start conservative for `SymbolLookup`: function=1.0, class=1.0, method=0.95, field=0.7, parameter=0.4, local=0.3.

**Acceptance:**
- Weights apply only for `SymbolLookup` and `Mixed`, never `Conceptual`
- Weight table is tunable from one place, not scattered constants
- Benchmark artifact `julie-kind-weighting-retrieval.json` beats the broader-filters baseline on AQPT or fails honestly

### Experiment 3: Public API alias expansion

Julie's `expansion.rs` has within-query alias support but no mapping from NL phrases to public API / tool names. Miller maps phrases like "find references" to symbol names like `fast_refs`. Port that second-pass alias table.

**Extend:**
- `src/search/expansion.rs:70-116` — existing phrase-based alias table

**Add:** Second-pass alias table mapping NL phrases to API surface names. Keep the `MAX_ADDED_TERMS` cap. Initial entries sourced from `~/source/miller/python/miller/embeddings/query_analysis.py`.

**Acceptance:**
- "find references" expands to include `fast_refs`; "look up symbol" → `fast_lookup`; similar for `trace_call_path`, `get_symbols`
- `MAX_ADDED_TERMS` cap prevents query blowup
- Benchmark artifact `julie-api-alias-retrieval.json` improves `navigation-01` case

### Experiment 4: Entrypoint-aware priors

For tool-path / wrapper-style queries, boost symbols in entrypoint positions (`main`, top-level wrappers, `#[tool]`-tagged in Rust, MCP tool module surfaces).

**Extend:**
- `src/search/scoring.rs:122-153` — `apply_nl_path_prior()`, add companion `apply_entrypoint_prior()`
- `src/search/weights.rs` — intent classification may need a `ToolPath` subtype of `Mixed`

**Add:** Tool-path detection via heuristics (contains "tool", "wrapper", "mcp", "handler"). Boost symbols in tool/wrapper directories or with public-entrypoint kinds.

**Acceptance:**
- Tool-path detection is deterministic and does not trigger on generic NL queries
- Boost size stays ≤1.1 and only applies when the intent matches
- Benchmark artifact `julie-entrypoint-priors-retrieval.json` moves appropriate cases without regressing others

### Experiment 5: Token-budgeted response shaping (gated)

Julie already has `return_format="locations"` (70-90% token savings). Only run this experiment if experiments 1-4 collectively leave a meaningful token gap. Miller's reranker-budgeting lands only if evidence after 1-4 says tokens are the remaining bottleneck.

**Gate:** After 1-4 merge or fail, run full comparison against Miller's best. If tokens/latency still dominate the remaining gap, proceed. Otherwise defer.

**Acceptance:**
- Labeled as a probe per Miller doctrine unless the gate fires
- Only proceeds on post-1-4 evidence

## Critical Files

**Julie (changes land here):**
- `src/search/scoring.rs` — path priors, noise filters, entrypoint priors
- `src/search/weights.rs` — intent classification, kind weights
- `src/search/expansion.rs` — alias table extension
- `src/search/hybrid.rs` — RRF merge updates
- `src/tools/search/text_search.rs` — filter resolution seam
- `src/tools/search/mod.rs` — tool surface for any new opt-in flags

**Miller (reference only, do not modify from Julie-side work):**
- `python/miller/embeddings/query_analysis.py` — source of alias and entrypoint logic
- `python/miller/tools/search_filters.py` — source of filter defaults
- `python/miller/tools/search.py` — source of routing behavior

**Benchmarks (run from Miller):**
- `python/benchmarks/baseline_matrix.py` — artifact generation
- `python/benchmarks/compare_results.py` — delta reporting
- `python/benchmarks/results/baselines/julie-dogfood-baseline.json` — Julie baseline

## Verification

Per experiment, before marking acceptance:

```bash
# In ~/source/julie
cargo nextest run --lib search     # unit tests for changed module
cargo xtask test changed           # affected subsystem
```

Full suite before merge:

```bash
cargo xtask test dev               # Julie batch gate
```

Benchmark each experiment individually:

```bash
# In ~/source/miller
uv run python python/benchmarks/baseline_matrix.py \
  --baseline-label "julie-<experiment-slug>" \
  --workspace primary \
  --benchmark-types retrieval \
  --output-dir python/benchmarks/results

uv run python python/benchmarks/compare_results.py \
  python/benchmarks/results/baselines/julie-dogfood-baseline.json \
  python/benchmarks/results/baselines/julie-<experiment-slug>-retrieval.json
```

Each experiment produces a decision record in this directory named `2026-04-XX-julie-<experiment-slug>-decision.md`, following the shape of existing decision records in Miller.

## Out of Scope

- Vector-store swap (sqlite-vec → LanceDB) — rejected by audit, confidence 92
- Changes to Julie's canonical-storage-projections architecture
- Wholesale port of Miller's Python reranker — only backend-agnostic tricks
- Eros work — this happens entirely in Julie

## Open Questions

1. Does Julie have a native benchmark harness yet, or do we continue to drive retrieval benchmarks from Miller's suite pointed at Julie artifacts? Cross-system comparison works, but Julie's world-class-systems program may add a native harness that these experiments should prefer.
2. The Julie `canonical-projections` plan is in-flight. If it lands mid-experiment, files like `src/search/index.rs` may move to `src/search/projection.rs`. Experiments should retarget to new locations rather than block on sequencing.
