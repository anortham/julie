# Semantic Search Value Scorecard

This scorecard exists to answer one question: does Julie's semantic stack earn its cost in the open-source product?

Eros is the commercial superset of Julie, not a companion tool agents will run next to Julie. So this evaluation is not "Julie plus Eros"; it is a product-boundary check:

- Julie: fast lexical search, graph ranking, navigation, editing, and token savings.
- Eros: all of Julie's baseline capability plus heavier intelligence, semantic retrieval, dashboards, and commercial workflows.

## What Gets Measured

Each case is a human-style query against a real repository under `~/source`. The runner calls the existing `fast_search` tool three ways:

- `backend = "lexical"`
- `backend = "semantic"`
- `backend = "hybrid"`

Every run uses `return_format = "locations"` and checks whether an expected source file appears in the top 8 hits.

Metrics:

- `top1`, `top3`, `top5`, `top8`: fraction of scored cases where the expected file appears by that rank.
- `mrr`: mean reciprocal rank. Misses score 0.
- `semantic_vs_lexical`: per-case outcome using the best of semantic and hybrid against lexical.
- `latency_ms`: wall-clock time for the CLI tool call.
- `fallback_cases`: zero-vector repos or unavailable semantic backend behavior. These are excluded from quality scoring.

## Decision Bar

Keep semantic search in Julie core only if the scored sample shows all of this:

- Hybrid beats lexical by at least 20% relative MRR.
- Hybrid improves top-5 success by at least 10 percentage points.
- Semantic/hybrid wins at least twice as many cases as it regresses.
- Severe noise clusters are explainable and fixable without adding more semantic-specific machinery.
- p95 latency stays reasonable for interactive agent use.

If semantic mainly helps `deep_dive` related-symbol sections, occasional conceptual lookups, or Eros-style intelligence workflows, move that stack out of Julie core. Julie should stay lexical/graph/token-saving.

## Running

```bash
python3 docs/eval/semantic-value/run_scorecard.py
```

Useful filters:

```bash
python3 docs/eval/semantic-value/run_scorecard.py --repo julie
python3 docs/eval/semantic-value/run_scorecard.py --case julie-mutation-gate
python3 docs/eval/semantic-value/run_scorecard.py --backend lexical --backend hybrid
```

Results are written under `docs/eval/semantic-value/results/`.
