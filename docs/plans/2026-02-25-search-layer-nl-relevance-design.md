# Search-Layer Natural Language Relevance Design

Date: 2026-02-25
Status: Approved (design)
Scope: `src/search/*` retrieval behavior for natural-language concept queries

## Problem

`get_context` improvements in v3.3.2 improved downstream pivot selection, but the search layer can still return mostly docs/tests for natural-language concept queries (for example: "workspace routing", "symbol extraction").

When retrieval returns only non-code candidates, downstream reranking cannot recover actionable code pivots.

## Goals

1. Improve recall of production code symbols for natural-language multi-word queries.
2. Keep behavior deterministic and testable (no embeddings/external models).
3. Preserve identifier-query quality (`snake_case`, `CamelCase`, exact names).
4. Avoid API/schema changes to tools.

## Non-Goals

- No semantic embedding layer.
- No runtime-learned synonym dictionary.
- No breaking query schema changes.

## Recommended Approach

Implement deterministic query expansion in the search layer, then apply a mild path prior for NL-like queries.

### Why this approach

- Fixes the root cause in retrieval, not just `get_context`.
- Benefits any caller of `search_symbols`/`search_content`.
- Keeps existing AND->OR fallback and field boosts intact.
- Lower complexity than full two-pass retrieval while materially improving recall.

## Architecture

### 1) Query expansion module

Add a small module (proposed: `src/search/expansion.rs`) that transforms user query text into weighted term groups:

- `original_terms` (highest priority)
- `phrase_alias_terms` (curated concept aliases)
- `normalized_terms` (safe deterministic token normalization)

Expansion remains bounded:

- max added terms cap
- deduplication
- min term length threshold
- stopword-like filtering

### 2) Integration points

- `src/search/index.rs`
  - Keep current tokenizer and AND->OR fallback behavior.
  - Build expanded term groups once per query and pass them into query builders.
- `src/search/query.rs`
  - Add builders that accept weighted groups and preserve existing MUST filters:
    - `doc_type`
    - optional `language`
    - optional `kind`
  - Original terms remain strongest contributors.

### 3) Mild path prior

After Tantivy retrieval, apply a small deterministic score adjustment for NL-like multiword queries:

- boost `src/**`
- penalize `docs/**`, `src/tests/**`, `fixtures/**`

The prior is intentionally weak so exact identifier queries are not overridden.

## Detailed Behavior

### NL-query detection

Treat query as NL-like when all are true:

- user has multiple whitespace-separated words
- query is not an obvious identifier form (`snake_case`, `CamelCase`, `kebab-case`)
- query length and token profile pass minimum quality checks

When not NL-like, search path is effectively current behavior.

### Expansion strategy

1. Tokenize (existing code tokenizer behavior).
2. Phrase alias lookup from curated static map (examples: "workspace routing" variants).
3. Normalize token variants with conservative rules (safe suffix trimming/abbreviation mapping only).
4. Drop noisy/short duplicates.
5. Enforce fan-out cap.

### Ranking composition

- Existing field weights remain (`name > signature > doc_comment > body`).
- Original terms retain top influence.
- Alias/normalized terms are additive SHOULD clauses with lower boosts.
- Existing centrality and language-specific boosts continue to apply.

## Alternatives Considered

### A) Post-search reranking only

Pros: tiny implementation, minimal risk.
Cons: cannot recover when retrieval set is docs/tests-only.

### B) Two-pass retrieval fallback

Pros: strong safety, only aggressive when needed.
Cons: extra latency and branching complexity.

### C) Deterministic expansion + mild path prior (chosen)

Pros: best ROI now; root-cause fix; broad tool benefit.
Cons: requires curation and guardrails to avoid query blow-up.

## Testing Strategy (TDD)

### Red tests first

Add failing tests under `src/tests/search/` for:

1. NL concept query returns at least one `src/**` symbol in top-K where baseline favors docs/tests.
2. Identifier query regression guard (exact code symbol relevance preserved).
3. Expansion guardrails: dedup, max fan-out, min-length filtering, NL-detector gating.
4. Path prior applies only to NL-like queries.

### Verification commands

- Targeted: `cargo test --lib tests::search`
- Fast tier once green: `cargo test --lib -- --skip search_quality`

## Rollout / Risk Management

1. Land deterministic expansion first with strict caps.
2. Add path prior with conservative weight.
3. Validate on existing get_context quality query set.
4. Tune alias map incrementally from observed misses.

## Success Criteria

1. Better top-K code hit rate for NL concept queries in fixture-based checks.
2. No meaningful regression for identifier-oriented searches.
3. No tool API changes and stable deterministic behavior.

## Open Questions

1. Initial alias dictionary size and ownership process.
2. Exact score multipliers for alias terms and path prior.
3. Whether two-pass fallback should be added later if recall remains weak.
