#!/usr/bin/env python3
"""
Julie Embedding Quality Benchmark
==================================

Evaluates embedding model quality across reference workspaces by measuring:
- Coverage: what % of symbols get embedded per language
- Similarity quality: are KNN results semantically meaningful
- Diversity: do results come from different files (not just siblings)
- Cross-kind matching: do functions find related classes/modules

Prerequisites:
    All target workspaces must be indexed with embeddings via Julie MCP.
    Use `benchmarks/setup_workspaces.sh` or add them manually.

Usage:
    # Run against all indexed workspaces
    python benchmarks/embedding_quality.py

    # Run against a single workspace
    python benchmarks/embedding_quality.py --workspace phoenix

    # Save structured results for model comparison
    python benchmarks/embedding_quality.py --output results/bge-small.json --report results/bge-small.md

    # Compare two model runs
    python benchmarks/compare_models.py results/bge-small.json results/nomic-code.json

To benchmark a different model:
    1. Set JULIE_EMBEDDING_SIDECAR_MODEL_ID=<model_id>
    2. Restart Julie (cargo build --release, restart Claude Code)
    3. Force re-embed all workspaces via Julie MCP:
       manage_workspace(operation="refresh", workspace_id="...", force=true)
    4. Re-run this script with a new --output path
"""

import argparse
import json
import sqlite3
import struct
import sys
from collections import defaultdict
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Optional

# ============================================================================
# Configuration
# ============================================================================

PIVOT_COUNT = 5       # Pivot symbols per workspace (highest centrality)
KNN_LIMIT = 10        # Similar symbols to retrieve per pivot
MIN_SIMILARITY = 0.3  # Floor for inclusion in results

# Symbol kinds that should be embedded (mirrors EMBEDDABLE_KINDS in metadata.rs)
EMBEDDABLE_KINDS = frozenset({
    "function", "method", "class", "struct", "interface",
    "trait", "enum", "type", "module", "namespace", "union",
})

# Kinds that are enriched into parent embeddings (not a gap)
ENRICHED_KINDS = frozenset({"property", "field", "enum_member"})

# Kinds where the gap matters (should arguably be embedded)
GAP_KINDS = frozenset({"constructor", "constant", "export", "destructor", "operator"})

# Languages excluded from embedding (data/docs)
NON_EMBEDDABLE_LANGUAGES = frozenset({
    "markdown", "json", "jsonl", "toml", "yaml", "css", "html", "regex", "sql",
})

# Expected primary language per workspace (for reporting)
WORKSPACE_LANGUAGES = {
    "phoenix":      "elixir",
    "flask":        "python",
    "express":      "javascript",
    "zod":          "typescript",
    "guava":        "java",
    "newtonsoft":   "csharp",
    "slim":         "php",
    "sinatra":      "ruby",
    "alamofire":    "swift",
    "moshi":        "kotlin",
    "cats":         "scala",
    "cobra":        "go",
    "jq":           "c",
    "nlohmann":     "cpp",
    "riverpod":     "dart",
    "zls":          "zig",
    "lite":         "lua",
    "kirigami":     "qml",
    "blazor":       "razor",
    "labhandbook":  "typescript",
    "julie":        "rust",
}


# ============================================================================
# Database helpers
# ============================================================================

def find_workspace_dbs(julie_dir: Path) -> list[tuple[str, Path]]:
    """Discover all workspace databases in .julie/indexes/."""
    indexes_dir = julie_dir / "indexes"
    if not indexes_dir.exists():
        return []
    results = []
    for ws_dir in sorted(indexes_dir.iterdir()):
        if not ws_dir.is_dir():
            continue
        db_path = ws_dir / "db" / "symbols.db"
        if db_path.exists():
            results.append((ws_dir.name, db_path))
    return results


def get_embedding_config(conn: sqlite3.Connection) -> dict:
    """Read the embedding model config from the database."""
    try:
        row = conn.execute(
            "SELECT id, model_name, dimensions FROM embedding_config LIMIT 1"
        ).fetchone()
        if row:
            return {"id": row[0], "model_name": row[1], "dimensions": row[2]}
    except sqlite3.OperationalError:
        pass
    return {"id": 0, "model_name": "unknown", "dimensions": 0}


def get_embedded_count(conn: sqlite3.Connection) -> int:
    """Count embedded symbols via the sqlite-vec rowids table."""
    try:
        row = conn.execute("SELECT COUNT(*) FROM symbol_vectors_rowids").fetchone()
        return row[0] if row else 0
    except sqlite3.OperationalError:
        return 0


# ============================================================================
# Coverage analysis
# ============================================================================

@dataclass
class LanguageCoverage:
    language: str
    total: int
    embeddable: int        # In EMBEDDABLE_KINDS
    variables: int         # Variable kind (budget-limited)
    enriched: int          # Property/field/enum_member (indirect coverage)
    gap: int               # Constructor/constant/export (missing)
    imports: int           # Import kind (covered by relationships)
    other: int             # Anything else
    embeddable_pct: float  # embeddable / total


def compute_coverage(conn: sqlite3.Connection) -> dict:
    """Compute detailed embedding coverage statistics."""
    rows = conn.execute("""
        SELECT language, kind, COUNT(*) as cnt
        FROM symbols
        GROUP BY language, kind
    """).fetchall()

    by_lang: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    for lang, kind, cnt in rows:
        by_lang[lang][kind] = cnt

    total_symbols = conn.execute("SELECT COUNT(*) FROM symbols").fetchone()[0]
    embedded_count = get_embedded_count(conn)

    lang_coverages = []
    for lang, kinds in sorted(by_lang.items()):
        total = sum(kinds.values())
        embeddable = sum(cnt for k, cnt in kinds.items() if k in EMBEDDABLE_KINDS)
        variables = kinds.get("variable", 0)
        enriched = sum(cnt for k, cnt in kinds.items() if k in ENRICHED_KINDS)
        gap = sum(cnt for k, cnt in kinds.items() if k in GAP_KINDS)
        imports = kinds.get("import", 0)
        other = total - embeddable - variables - enriched - gap - imports

        lang_coverages.append(LanguageCoverage(
            language=lang,
            total=total,
            embeddable=embeddable,
            variables=variables,
            enriched=enriched,
            gap=gap,
            imports=imports,
            other=other,
            embeddable_pct=round(embeddable / total * 100, 1) if total > 0 else 0,
        ))

    return {
        "total_symbols": total_symbols,
        "embedded_count": embedded_count,
        "overall_coverage_pct": round(embedded_count / total_symbols * 100, 1) if total_symbols > 0 else 0,
        "languages": [asdict(lc) for lc in lang_coverages],
    }


# ============================================================================
# Pivot selection
# ============================================================================

def select_pivots(conn: sqlite3.Connection, limit: int = PIVOT_COUNT) -> list[dict]:
    """Select pivot symbols: highest incoming reference count, non-test, embeddable kinds.

    Uses a two-pass approach:
    1. Count incoming references per symbol (cross-file only)
    2. Pick the top-N by ref count, breaking ties by name for determinism
    """
    rows = conn.execute("""
        SELECT
            s.id,
            s.name,
            s.kind,
            s.language,
            s.file_path,
            s.visibility,
            s.signature,
            COUNT(DISTINCT i.id) as ref_count
        FROM symbols s
        LEFT JOIN identifiers i
            ON i.name = s.name
            AND i.file_path != s.file_path
        WHERE s.kind IN ('function', 'method', 'class', 'struct', 'interface',
                         'trait', 'enum', 'type', 'module', 'namespace', 'union')
          AND s.file_path NOT LIKE '%%test%%'
          AND s.file_path NOT LIKE '%%spec%%'
          AND s.file_path NOT LIKE '%%fixture%%'
          AND s.file_path NOT LIKE '%%example%%'
          AND s.file_path NOT LIKE '%%.min.%%'
          AND s.language NOT IN ('markdown', 'json', 'toml', 'yaml')
        GROUP BY s.id
        ORDER BY ref_count DESC, s.name ASC
        LIMIT ?
    """, (limit,)).fetchall()

    return [{
        "id": r[0],
        "name": r[1],
        "kind": r[2],
        "language": r[3],
        "file_path": r[4],
        "visibility": r[5],
        "signature": r[6] or "",
        "ref_count": r[7],
    } for r in rows]


# ============================================================================
# Vector operations (pure Python, no sqlite-vec dependency)
# ============================================================================

def load_all_embeddings(conn: sqlite3.Connection) -> dict[str, tuple[float, ...]]:
    """Load all embeddings from the sqlite-vec backing tables.

    sqlite-vec stores vectors in chunks (up to 1024 vectors per chunk blob).
    Each vector's position is given by (chunk_id, chunk_offset) in the rowids table.
    We read the dimension count from embedding_config to extract correctly.
    """
    config = get_embedding_config(conn)
    dims = config.get("dimensions", 384)
    vec_bytes = dims * 4  # float32

    embeddings = {}
    try:
        # Load all chunks into memory
        chunk_rows = conn.execute(
            "SELECT rowid, vectors FROM symbol_vectors_vector_chunks00"
        ).fetchall()
        chunks = {row[0]: row[1] for row in chunk_rows}

        # Map each symbol to its vector
        rowid_rows = conn.execute(
            "SELECT id, chunk_id, chunk_offset FROM symbol_vectors_rowids"
        ).fetchall()
        for symbol_id, chunk_id, chunk_offset in rowid_rows:
            blob = chunks.get(chunk_id)
            if blob is None:
                continue
            start = chunk_offset * vec_bytes
            end = start + vec_bytes
            if end > len(blob):
                continue
            vec = struct.unpack(f"<{dims}f", blob[start:end])
            embeddings[symbol_id] = vec
    except sqlite3.OperationalError as e:
        print(f"    Warning: failed to load embeddings: {e}", file=sys.stderr)
    return embeddings


def cosine_similarity(a, b) -> float:
    """Cosine similarity between two vectors (tuples or lists)."""
    dot = sum(x * y for x, y in zip(a, b))
    norm_a = sum(x * x for x in a) ** 0.5
    norm_b = sum(x * x for x in b) ** 0.5
    if norm_a == 0 or norm_b == 0:
        return 0.0
    return dot / (norm_a * norm_b)


def knn_search(
    pivot_id: str,
    pivot_vec,
    all_embeddings: dict[str, tuple],
    limit: int = KNN_LIMIT,
    min_sim: float = MIN_SIMILARITY,
) -> list[tuple[str, float]]:
    """Brute-force KNN via cosine similarity. Returns (symbol_id, similarity) pairs."""
    scored = []
    for sid, vec in all_embeddings.items():
        if sid == pivot_id:
            continue
        sim = cosine_similarity(pivot_vec, vec)
        if sim >= min_sim:
            scored.append((sid, sim))
    scored.sort(key=lambda x: -x[1])
    return scored[:limit]


def lookup_symbols(conn: sqlite3.Connection, ids: list[str]) -> dict[str, dict]:
    """Batch-lookup symbol metadata by ID."""
    if not ids:
        return {}
    placeholders = ",".join("?" for _ in ids)
    rows = conn.execute(f"""
        SELECT id, name, kind, language, file_path, visibility, signature
        FROM symbols WHERE id IN ({placeholders})
    """, ids).fetchall()
    return {
        r[0]: {
            "id": r[0], "name": r[1], "kind": r[2], "language": r[3],
            "file_path": r[4], "visibility": r[5], "signature": r[6] or "",
        }
        for r in rows
    }


# ============================================================================
# Quality metrics
# ============================================================================

def analyze_quality(pivot: dict, similar: list[dict]) -> dict:
    """Compute quality metrics for a set of similarity results.

    Metrics:
    - result_count: how many results above threshold
    - avg_similarity / top_similarity: score distribution
    - diversity_score: 1.0 = all from different files, 0.0 = all same file
    - same_kind_ratio: fraction of results sharing the pivot's kind
    - namespace_overlap_ratio: fraction sharing a name component with the pivot
    - cross_language: whether results span multiple languages
    """
    if not similar:
        return {"has_results": False}

    n = len(similar)
    same_file = sum(1 for s in similar if s["file_path"] == pivot["file_path"])
    same_kind = sum(1 for s in similar if s["kind"] == pivot["kind"])
    same_lang = sum(1 for s in similar if s["language"] == pivot["language"])

    # Namespace overlap: shared dot-separated or :: components
    pivot_parts = set(pivot["name"].replace("::", ".").split("."))
    ns_overlap = 0
    for s in similar:
        s_parts = set(s["name"].replace("::", ".").split("."))
        if pivot_parts & s_parts:
            ns_overlap += 1

    # Unique files in results
    unique_files = len(set(s["file_path"] for s in similar))
    languages = set(s["language"] for s in similar)

    sims = [s["similarity"] for s in similar]

    return {
        "has_results": True,
        "result_count": n,
        "top_similarity": round(max(sims), 4),
        "avg_similarity": round(sum(sims) / n, 4),
        "min_similarity": round(min(sims), 4),
        "diversity_score": round(1.0 - (same_file / n), 3),
        "unique_files": unique_files,
        "same_kind_ratio": round(same_kind / n, 3),
        "same_language_ratio": round(same_lang / n, 3),
        "namespace_overlap_ratio": round(ns_overlap / n, 3),
        "cross_language": len(languages) > 1,
        "languages": sorted(languages),
    }


# ============================================================================
# Benchmark runner
# ============================================================================

def benchmark_workspace(ws_id: str, db_path: Path, pivot_count: int = PIVOT_COUNT, knn_limit: int = KNN_LIMIT) -> dict:
    """Run the full benchmark on a single workspace."""
    conn = sqlite3.connect(str(db_path))

    config = get_embedding_config(conn)
    coverage = compute_coverage(conn)
    pivots = select_pivots(conn, pivot_count)

    # Load all embeddings once (avoids N per-pivot full scans)
    all_embeddings = load_all_embeddings(conn)

    pivot_results = []
    for pivot in pivots:
        pivot_vec = all_embeddings.get(pivot["id"])
        if pivot_vec is None:
            pivot_results.append({
                "pivot": pivot,
                "similar": [],
                "quality": {"has_results": False, "reason": "no_embedding"},
            })
            continue

        # KNN search
        knn_hits = knn_search(pivot["id"], pivot_vec, all_embeddings, knn_limit, MIN_SIMILARITY)

        # Lookup metadata for results
        hit_ids = [sid for sid, _ in knn_hits]
        symbol_meta = lookup_symbols(conn, hit_ids)

        similar = []
        for sid, sim in knn_hits:
            meta = symbol_meta.get(sid)
            if meta:
                meta["similarity"] = round(sim, 4)
                similar.append(meta)

        quality = analyze_quality(pivot, similar)
        pivot_results.append({
            "pivot": pivot,
            "similar": similar,
            "quality": quality,
        })

    conn.close()

    # Determine primary language
    primary_lang = "unknown"
    for prefix, lang in WORKSPACE_LANGUAGES.items():
        if prefix in ws_id.lower():
            primary_lang = lang
            break

    return {
        "workspace_id": ws_id,
        "primary_language": primary_lang,
        "model": config,
        "coverage": coverage,
        "pivot_results": pivot_results,
    }


# ============================================================================
# Report formatting
# ============================================================================

def format_report(results: list[dict], pivot_count: int = PIVOT_COUNT, knn_limit: int = KNN_LIMIT) -> str:
    """Format benchmark results as a markdown report."""
    lines = []

    # Header
    model_name = "unknown"
    dimensions = 0
    for r in results:
        if r["model"]["model_name"] != "unknown":
            model_name = r["model"]["model_name"]
            dimensions = r["model"]["dimensions"]
            break

    lines.append("# Julie Embedding Quality Benchmark")
    lines.append("")
    lines.append(f"**Model:** `{model_name}` ({dimensions} dimensions)")
    lines.append(f"**Workspaces:** {len(results)}")
    lines.append(f"**Pivots per workspace:** {pivot_count}")
    lines.append(f"**KNN limit:** {knn_limit}")
    lines.append(f"**Min similarity threshold:** {MIN_SIMILARITY}")
    lines.append("")

    # ---- Coverage summary ----
    lines.append("## 1. Coverage Summary")
    lines.append("")
    lines.append("| Workspace | Language | Total | Embedded | Coverage | Embeddable % | Vars | Gap Kinds | Enriched |")
    lines.append("|-----------|----------|-------|----------|----------|-------------|------|-----------|----------|")
    for r in sorted(results, key=lambda x: x["primary_language"]):
        ws = r["workspace_id"]
        cov = r["coverage"]
        lang = r["primary_language"]

        # Find the primary language's stats
        lang_stats = None
        for lc in cov["languages"]:
            if lc["language"] == lang:
                lang_stats = lc
                break
        if not lang_stats:
            # Fallback: largest language
            lang_stats = max(cov["languages"], key=lambda x: x["total"])
            lang = lang_stats["language"]

        lines.append(
            f"| {ws[:25]} | {lang} | {cov['total_symbols']} | {cov['embedded_count']} "
            f"| {cov['overall_coverage_pct']}% | {lang_stats['embeddable_pct']}% "
            f"| {lang_stats['variables']} | {lang_stats['gap']} | {lang_stats['enriched']} |"
        )
    lines.append("")

    # ---- Quality summary ----
    lines.append("## 2. Quality Summary")
    lines.append("")
    lines.append("| Workspace | Language | Pivots | Avg Top Sim | Avg Diversity | Avg NS Overlap | Cross-lang |")
    lines.append("|-----------|----------|--------|-------------|---------------|----------------|------------|")
    for r in sorted(results, key=lambda x: x["primary_language"]):
        ws = r["workspace_id"]
        lang = r["primary_language"]
        pr = r["pivot_results"]
        with_results = [p for p in pr if p["quality"].get("has_results", False)]

        if not with_results:
            lines.append(f"| {ws[:25]} | {lang} | 0/{len(pr)} | - | - | - | - |")
            continue

        top_sims = [p["quality"]["top_similarity"] for p in with_results]
        diversities = [p["quality"]["diversity_score"] for p in with_results]
        ns_overlaps = [p["quality"]["namespace_overlap_ratio"] for p in with_results]
        cross_lang = any(p["quality"].get("cross_language", False) for p in with_results)

        avg_top = round(sum(top_sims) / len(top_sims), 3)
        avg_div = round(sum(diversities) / len(diversities), 3)
        avg_ns = round(sum(ns_overlaps) / len(ns_overlaps), 3)

        lines.append(
            f"| {ws[:25]} | {lang} | {len(with_results)}/{len(pr)} "
            f"| {avg_top} | {avg_div} | {avg_ns} | {'yes' if cross_lang else 'no'} |"
        )
    lines.append("")

    # ---- Aggregate metrics ----
    all_qualities = [
        p["quality"]
        for r in results
        for p in r["pivot_results"]
        if p["quality"].get("has_results", False)
    ]
    if all_qualities:
        lines.append("## 3. Aggregate Metrics")
        lines.append("")
        avg_top_all = round(sum(q["top_similarity"] for q in all_qualities) / len(all_qualities), 3)
        avg_avg_all = round(sum(q["avg_similarity"] for q in all_qualities) / len(all_qualities), 3)
        avg_div_all = round(sum(q["diversity_score"] for q in all_qualities) / len(all_qualities), 3)
        avg_ns_all = round(sum(q["namespace_overlap_ratio"] for q in all_qualities) / len(all_qualities), 3)
        avg_kind_all = round(sum(q["same_kind_ratio"] for q in all_qualities) / len(all_qualities), 3)
        cross_lang_pct = round(sum(1 for q in all_qualities if q.get("cross_language")) / len(all_qualities) * 100, 1)

        lines.append(f"- **Total pivot queries:** {len(all_qualities)}")
        lines.append(f"- **Avg top similarity:** {avg_top_all}")
        lines.append(f"- **Avg mean similarity:** {avg_avg_all}")
        lines.append(f"- **Avg diversity (cross-file):** {avg_div_all}")
        lines.append(f"- **Avg namespace overlap:** {avg_ns_all}")
        lines.append(f"- **Avg same-kind ratio:** {avg_kind_all}")
        lines.append(f"- **Cross-language results:** {cross_lang_pct}%")
        lines.append("")

    # ---- Detailed per-workspace results ----
    lines.append("## 4. Detailed Results")
    lines.append("")

    for r in sorted(results, key=lambda x: x["primary_language"]):
        ws = r["workspace_id"]
        lang = r["primary_language"]
        lines.append(f"### {ws} ({lang})")
        lines.append("")

        for pr in r["pivot_results"]:
            pivot = pr["pivot"]
            lines.append(
                f"**Pivot:** `{pivot['name']}` ({pivot['kind']}, {pivot['language']}, "
                f"refs={pivot['ref_count']})  "
            )
            lines.append(f"File: `{pivot['file_path']}`")
            lines.append("")

            if not pr["similar"]:
                reason = pr["quality"].get("reason", "no results above threshold")
                lines.append(f"*No similar symbols found ({reason})*")
                lines.append("")
                continue

            lines.append("| # | Sim | Name | Kind | File |")
            lines.append("|---|-----|------|------|------|")
            for i, s in enumerate(pr["similar"], 1):
                fp = s["file_path"]
                if len(fp) > 55:
                    fp = "..." + fp[-52:]
                lines.append(
                    f"| {i} | {s['similarity']:.3f} | `{s['name']}` | {s['kind']} | `{fp}` |"
                )

            q = pr["quality"]
            lines.append("")
            lines.append(
                f"Quality: diversity={q['diversity_score']}, "
                f"same_kind={q['same_kind_ratio']}, "
                f"ns_overlap={q['namespace_overlap_ratio']}, "
                f"unique_files={q['unique_files']}"
            )
            lines.append("")

    return "\n".join(lines)


# ============================================================================
# Main
# ============================================================================

def main():
    parser = argparse.ArgumentParser(
        description="Julie Embedding Quality Benchmark",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--julie-dir", default=".julie",
        help="Path to .julie directory (default: .julie)",
    )
    parser.add_argument(
        "--output", default=None,
        help="Save structured JSON results to this path",
    )
    parser.add_argument(
        "--report", default=None,
        help="Save markdown report to this path (default: stdout)",
    )
    parser.add_argument(
        "--workspace", default=None,
        help="Run on a single workspace (substring match on ID)",
    )
    parser.add_argument(
        "--pivots", type=int, default=PIVOT_COUNT,
        help=f"Number of pivot symbols per workspace (default: {PIVOT_COUNT})",
    )
    parser.add_argument(
        "--knn", type=int, default=KNN_LIMIT,
        help=f"KNN results per pivot (default: {KNN_LIMIT})",
    )
    args = parser.parse_args()

    pivot_count = args.pivots
    knn_limit = args.knn

    julie_dir = Path(args.julie_dir)
    workspaces = find_workspace_dbs(julie_dir)

    if not workspaces:
        print(f"No workspaces found in {julie_dir}/indexes/", file=sys.stderr)
        sys.exit(1)

    if args.workspace:
        workspaces = [
            (wid, path) for wid, path in workspaces
            if args.workspace.lower() in wid.lower()
        ]
        if not workspaces:
            print(f"No workspace matching '{args.workspace}'", file=sys.stderr)
            sys.exit(1)

    print(f"Benchmarking {len(workspaces)} workspaces...", file=sys.stderr)

    results = []
    for ws_id, db_path in workspaces:
        print(f"  {ws_id}...", file=sys.stderr, end=" ", flush=True)
        try:
            result = benchmark_workspace(ws_id, db_path, pivot_count, knn_limit)
            results.append(result)
            emb = result["coverage"]["embedded_count"]
            tot = result["coverage"]["total_symbols"]
            piv = sum(1 for p in result["pivot_results"] if p["quality"].get("has_results"))
            print(f"{emb}/{tot} embedded, {piv}/{len(result['pivot_results'])} pivots with results", file=sys.stderr)
        except Exception as e:
            print(f"ERROR: {e}", file=sys.stderr)

    if not results:
        print("No results to report.", file=sys.stderr)
        sys.exit(1)

    # Save JSON
    if args.output:
        out_path = Path(args.output)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        with open(out_path, "w") as f:
            json.dump(results, f, indent=2)
        print(f"JSON results: {out_path}", file=sys.stderr)

    # Generate and save/print report
    report = format_report(results, pivot_count, knn_limit)
    if args.report:
        rpt_path = Path(args.report)
        rpt_path.parent.mkdir(parents=True, exist_ok=True)
        with open(rpt_path, "w") as f:
            f.write(report)
        print(f"Markdown report: {rpt_path}", file=sys.stderr)
    else:
        print(report)


if __name__ == "__main__":
    main()
