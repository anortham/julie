# Embedding Quality Evaluation Design (Dogfood + Cross-Language)

## Context

Recent patch releases improved embedding text quality:

- `v3.6.4` enriched container symbols with child names.
- `v3.6.5` enriched enums with variant names and increased metadata text cap (`400 -> 600`).

These changes improved representation for classes/interfaces/enums, but `Variable` symbols remain excluded from embedding. In a real polyglot workspace (`LabHandbookV2`), that exclusion drops many semantically important symbols (for example Vue composables, store exports, and app-level state bindings).

The next iteration should target:

1. Better retrieval precision on real dogfood queries.
2. Better cross-language consistency between ASP.NET API symbols and Vue/TypeScript SPA symbols.
3. Hard overhead guardrails to avoid index/runtime blowup.

## Goals

- Improve retrieval quality on real dogfood queries (precision-oriented).
- Improve cross-language matching consistency for known API/UI counterparts.
- Keep embedding count and pipeline/runtime growth within `+20%`.
- Preserve current graceful degradation behavior (keyword search remains available if embeddings fail).

## Non-goals

- Changing embedding model/provider architecture.
- Changing vector dimension/storage format.
- Replacing existing hybrid search fusion logic.
- Expanding this iteration to broad query-time ranking redesign.

## Approaches Considered

### A) Export/Public allowlist only

Embed variables only when they look like API surface (exported/public/const/doc-rich).

- **Pros:** low risk, likely low volume increase.
- **Cons:** misses many high-impact internal symbols.

### B) Budgeted centrality inclusion (chosen)

Keep API-surface allowlist, then include additional high-signal variables ranked by language-agnostic importance up to a strict budget cap.

- **Pros:** best quality upside while retaining a hard cost ceiling.
- **Cons:** requires deterministic budgeting and stale-vector cleanup.

### C) Retrieval-only fallback

Do not change embedding coverage; only adjust query-time behavior.

- **Pros:** near-zero indexing overhead.
- **Cons:** lower upside for semantic and cross-language quality.

## Chosen Approach: Budgeted Variable Inclusion

### 1) Candidate set

- Start from all symbols with `kind == Variable`.
- Keep existing embeddable kinds unchanged.

### 2) Variable signal scoring (language-agnostic)

Rank variable candidates using a weighted score composed of:

- **API surface signal:** exported/public visibility where available.
- **Graph signal:** `reference_score` (incoming relationship centrality).
- **Shape signal:** function-like/semantic metadata when extractors provide it.
- **Noise penalties:** destructuring-only locals, throwaway names, obviously local-only patterns.

The exact weights should be configuration-driven and logged for observability.

### 3) Budget cap

- Let `N_base` be the number of non-variable symbols selected for embedding.
- Allow at most `floor(N_base * 0.20)` variable embeddings.
- If candidate count exceeds the cap, keep top-ranked candidates only.
- Tie-break deterministically (stable score + symbol id ordering).

### 4) Stale vector cleanup

On full embedding runs, delete vectors for variable symbols that are no longer selected by policy before storing new vectors. This prevents stale vectors from older policy decisions from polluting semantic search.

### 5) Incremental behavior

For file-change embedding paths, apply the same variable policy logic to changed-file symbols so behavior is consistent between full and incremental runs.

## Evaluation Harness (Real Dogfood)

### Dataset and workspace

- Add `LabHandbookV2` as a reference workspace.
- Pin evaluation to a fixed commit during each experiment window.
- Use real dogfood query corpus (no synthetic query generation).

Suggested corpus file:

- `fixtures/benchmarks/labhandbookv2_dogfood_queries.jsonl`

Minimum fields:

- `query`
- `intent_tag`
- `expected_symbol_ids` (optional at first, then incrementally labeled)

### A/B protocol

Run both variants over the same workspace snapshot and query corpus:

- **Baseline:** current behavior (no variable embeddings).
- **Candidate:** budgeted variable inclusion policy.

### Quality metrics

Primary retrieval metrics:

- `Hit@1`, `Hit@3`, `Hit@5`
- `MRR@10`
- `OffTopic@5` (guardrail against noisy variable flooding)

Cross-language consistency metrics:

- Curated counterpart sets (ASP.NET controllers/services/DTOs <-> Vue/TS composables/stores/types).
- `CrossLangRecall@5`
- Median counterpart rank in semantic neighbors.

### Overhead metrics and hard gates

Candidate fails if any gate is exceeded:

- `embedding_count_delta > +20%`
- `full_pipeline_runtime_delta > +20%`
- `incremental_reembed_p95_delta > +20%`

## Rollout Plan

### Phase 1: Guarded dogfood rollout

- Ship policy behind a config flag (default OFF).
- Enable only in dogfood environments first.
- Log policy stats per run:
  - variable candidates seen
  - variable candidates selected
  - rejected-by-rule counts
  - budget cap and utilization
  - total embedding delta

### Phase 2: Promotion criteria

Enable by default only after:

- Overhead gates pass for 3 consecutive dogfood runs.
- Retrieval quality improves (or is neutral with tighter off-topic behavior).
- Cross-language metrics show stable improvement.

### Phase 3: Rollback plan

- Disable variable policy flag.
- Run one cleanup pass to remove variable vectors.
- Revert to baseline behavior immediately.

## Error Handling and Operational Safety

- Keep current non-fatal embedding behavior: semantic failures must not break keyword search.
- Keep deterministic selection and logging to simplify incident debugging.
- Prefer explicit counters over inference in logs for post-run analysis.

## Testing Strategy

### Unit tests

- Candidate scoring behavior for representative symbol shapes.
- Budget cap enforcement and deterministic tie-break behavior.
- Cleanup selection logic for de-selected variable vectors.

### Integration tests

- Full pipeline: variable policy ON/OFF delta checks.
- Incremental embedding: changed file applies same policy.
- Hybrid search smoke checks for query relevance and no obvious off-topic drift.

### Validation commands (targeted)

- `cargo test --lib embedding_metadata 2>&1 | tail -30`
- `cargo test --lib embedding_incremental 2>&1 | tail -30`
- `cargo test --lib semantic_similarity_dogfood --features embeddings-ort 2>&1 | tail -30`
- `cargo test --lib -- --skip search_quality 2>&1 | tail -20`

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Variable embeddings flood semantic space | Lower precision / noisy neighbors | Hard `+20%` budget cap + `OffTopic@5` gate |
| Language-specific heuristics bias selection | Cross-language inconsistency | Base ranking on language-agnostic `reference_score` plus generic signals |
| Incremental and full pipeline diverge | Non-reproducible behavior | Share policy logic between full and incremental paths |
| Stale vectors remain after policy changes | Silent quality regression | Explicit variable-vector cleanup before store on full runs |

## Exit Criteria

- Dogfood query quality improves on primary metrics without off-topic regression.
- Cross-language consistency metrics improve for curated counterpart sets.
- All overhead gates remain within `+20%`.
- Rollback path is tested and operationally simple.
