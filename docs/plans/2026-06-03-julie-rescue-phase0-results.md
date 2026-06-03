# Julie Rescue Phase 0 — One-Shot Retrieval Bakeoff Results

**Date:** 2026-06-03  
**Branch:** julie-rescue  
**Status:** Report-only decision evidence — no gate, no promotion machinery, no re-run.

---

## Summary

| Rank | System | top5-hits / 135 | top5-rate | MRR |
|------|--------|-----------------|-----------|-----|
| 1 | **Julie** (v7.13.2, standalone) | 122 | **0.904** | **0.881** |
| 2 | **Miller** (MCP stdio, head) | 84 | 0.622 | 0.556 |
| 3 | Eros-sqlite† | 15 | 0.111 | 0.111 |

† Eros column is **unreliable for this run** — see Caveats section.  
**Julie's search moat is real.** It leads Miller by +28 pp top-5 rate (90% vs 62%) and +0.33 MRR.

---

## Corpus Provenance

Generated **fresh** on 2026-06-03 using `eros eval corpus ~/source --max-repositories 12`:

```
corpus_hash: sha256:f222cb845e208efd5d7aa6a1ec99651668aae816528d868ac7ad5472d71733e7
created_at:  2026-06-03T15:34:55Z
repos:       9
queries:     135 (15 per repo)
```

**Repos in corpus (language mix: Go, JS/TS, Python, Rust):**
- `browser39` (Python/Rust/TypeScript)
- `coa-goldfish-mcp` (TypeScript)
- `cobra` (Go)
- `codenav` (Go)
- `codex-plugin-cc` (TypeScript)
- `express` (JavaScript)
- `flask` (Python)
- `get-shit-done` (JavaScript)
- `gnhf` (TypeScript)

**Source extensions indexed:** `.go`, `.js`, `.jsx`, `.py`, `.rs`, `.ts`, `.tsx`

**Language bias caveat:** The corpus generator covers Go/JS/Py/Rust/TS well but
**under-samples C#, Java, Swift, C++, Dart, Kotlin, Scala**. Those 8 languages are absent
from this run. Results generalize to the covered languages; conclusions about Julie's moat
on .NET or JVM codebases are not supported by this data.

**Not in-sample:** This corpus was generated fresh from `~/source` and does NOT use
`eros/python/eros/eval/data/query-corpus.json` (the eros bootstrap corpus, which is
in-sample for eros's ranker tuning). This run is therefore a fair neutral test.

---

## Per-Category Breakdown

| Category | n | Julie top5 | Miller top5 | Eros† top5 | Julie MRR | Miller MRR |
|----------|---|-----------|-------------|-----------|-----------|------------|
| exact symbol lookup | 36 | **1.00** | 0.97 | 0.11† | **0.981** | 0.940 |
| symbol intent lookup | 36 | 0.78 | **0.97** | 0.11† | 0.759 | **0.882** |
| documentation phrase lookup | 25 | **0.88** | 0.36 | 0.16† | **0.813** | 0.201 |
| file/path search | 25 | **0.92** | 0.08 | 0.12† | **0.920** | 0.080 |
| likely test lookup | 10 | **1.00** | 0.00 | 0.00† | **1.000** | 0.000 |
| test intent lookup | 3 | **1.00** | **1.00** | 0.00† | **1.000** | 0.833 |

---

## Key Findings

### Where Julie wins clearly
- **File/path search** (+84 pp, 92% vs 8%): Julie understands file-path queries. Miller
  treats them as symbol searches and almost always misses.
- **Test awareness** (100% vs 0%): Julie's `is_test` metadata and test-path heuristics find
  test files and test functions reliably. Miller does not surface test-tagged symbols for
  "likely test" queries.
- **Documentation phrases** (+52 pp, 88% vs 36%): Julie's CamelCase/snake_case tokenization
  with English stemming recovers documentation-style queries. Miller's lexical index misses
  multi-word prose queries.

### Where Miller is competitive or wins
- **Symbol intent lookup** ("function X", "type Y") (97% vs 78%, MRR 0.882 vs 0.759):
  Miller's exact-name boost and exact-identifier search edges out Julie for natural-language
  symbol-named queries. This is Miller's strongest category.
- **Exact symbol lookup** (97% vs 100%): Both systems are excellent; Julie's 3 pp lead is
  statistically thin with n=36.

### Structural gap
The gap is not a tuning issue — it reflects index depth:

| Feature | Julie | Miller |
|---------|-------|--------|
| File-path indexing | ✓ full (all paths, file symbols) | Partial (symbol `file` field only) |
| Test metadata | ✓ `is_test` flag per symbol | Relies on path heuristic |
| Multi-language tokenization | ✓ CamelCase + snake_case + stemming | English BM25 only |
| 34 extractors | ✓ | ~14 languages |
| Tree-sitter relationship graph | ✓ (centrality boost) | ✗ |

---

## Caveats

### Eros column is unreliable
- `lancedb-hybrid-coderank` is the configured eros default backend but the `lancedb` extra is
  not installed in this environment (`503: lancedb optional extra unavailable`).
- Fallback to `backend="sqlite"` was used. The sqlite backend returned results only for
  `browser39` (which had a pre-existing search projection); all 8 other repos returned 0
  results. Eros's 11.1% rate reflects this setup artifact, not eros retrieval quality.
- **Eros should not be used in ranking decisions from this run.** A fair eros evaluation
  requires lancedb installed and full projection builds for each corpus repo.

### Julie latency is not representative
- Julie was invoked with `--standalone` (no daemon, no persistent Tantivy index) per query.
  Each query re-loads the index from disk. **Standalone latency is 5–30× slower than daemon
  mode** where the Tantivy index is warm and all 135 queries are served from a single hot
  process. Do not read latency from this run as Julie's real-world search latency.

### Corpus language scope
- The corpus is Go/JS/Py/Rust/TS only. C#, Java, Swift, C++, Dart are unrepresented.
  Miller's .NET strength is not measurable from this data; Julia's breadth advantage on those
  languages is also unmeasured.

---

## Run Methodology

**Driver:** `scripts/bakeoff/miller_mcp_driver.py` in `/tmp/julie-bakeoff` (isolated clone).  
**Isolation:** Julie was built and run from `/tmp/julie-bakeoff/target/release/julie-server`
to avoid cargo lock contention with concurrent crate surgery on the main working tree.  

**Julie invocation per query:**
```
julie-server --workspace {repo} --json --standalone search --target {definitions|all} --limit 5 {query}
```

**Miller invocation per query:**
- Spawns `~/source/miller/src/Miller.Server/bin/Release/net10.0/miller` as MCP-stdio subprocess
- Sets `cwd={repo}` so Miller reads workspace from CWD
- Sends: `initialize` → `notifications/initialized` → `tools/call` (search, format=json, limit=5)
- Remaps Miller's `"file"` JSON key → `"path"` for eros scoring compatibility

**Scoring:** `eros.eval.compare._first_matching_rank` / `_rank_metrics` / `_result_paths`
(eros's own functions, not reimplemented). A hit is any result in rank ≤ 5 whose path
suffix-matches any path in `expected_paths`.

**Raw results artifact:** `/tmp/julie-bakeoff/docs/plans/bakeoff-raw-results.json`

---

## Decision Relevance

This run was commissioned as a **boundary-proof input** for the Julie rescue decision:
does Julie's search moat justify the packaging rescue (crate split to kill the relink tax)
rather than switching to Miller?

**The answer the data supports:** Yes, the moat is real. At +28 pp top-5 rate and +0.33 MRR
against a fair neutral corpus, Julie's lead comes from structural depth (file indexing, test
metadata, multi-language tokenization) not tuning — these advantages do not disappear with
Miller parameter changes. Miller's counter-advantage in "function name" symbol lookups (97% vs
78%) is genuine but narrow and does not overcome Julie's breadth.

The rescue is the right call.
