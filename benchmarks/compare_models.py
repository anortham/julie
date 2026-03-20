#!/usr/bin/env python3
"""
Compare embedding benchmark results across different models.

Usage:
    python benchmarks/compare_models.py results/bge-small.json results/nomic-code.json

Reads JSON output from embedding_quality.py and produces a side-by-side comparison.
"""

import json
import sys
from pathlib import Path


def load_results(path: str) -> list[dict]:
    with open(path) as f:
        return json.load(f)


def model_name(results: list[dict]) -> str:
    for r in results:
        if r["model"]["model_name"] != "unknown":
            return r["model"]["model_name"]
    return "unknown"


def aggregate_quality(results: list[dict]) -> dict:
    """Compute aggregate quality metrics across all workspaces."""
    qualities = [
        p["quality"]
        for r in results
        for p in r["pivot_results"]
        if p["quality"].get("has_results", False)
    ]
    if not qualities:
        return {}

    n = len(qualities)
    return {
        "pivot_count": n,
        "avg_top_sim": round(sum(q["top_similarity"] for q in qualities) / n, 4),
        "avg_mean_sim": round(sum(q["avg_similarity"] for q in qualities) / n, 4),
        "avg_diversity": round(sum(q["diversity_score"] for q in qualities) / n, 4),
        "avg_ns_overlap": round(sum(q["namespace_overlap_ratio"] for q in qualities) / n, 4),
        "avg_same_kind": round(sum(q["same_kind_ratio"] for q in qualities) / n, 4),
        "cross_lang_pct": round(
            sum(1 for q in qualities if q.get("cross_language")) / n * 100, 1
        ),
    }


def per_workspace_quality(results: list[dict]) -> dict[str, dict]:
    """Per-workspace quality summary."""
    out = {}
    for r in results:
        ws = r["workspace_id"]
        lang = r["primary_language"]
        pivots = [p for p in r["pivot_results"] if p["quality"].get("has_results")]
        if not pivots:
            out[ws] = {"language": lang, "pivots": 0}
            continue

        top_sims = [p["quality"]["top_similarity"] for p in pivots]
        diversities = [p["quality"]["diversity_score"] for p in pivots]
        ns_overlaps = [p["quality"]["namespace_overlap_ratio"] for p in pivots]

        out[ws] = {
            "language": lang,
            "pivots": len(pivots),
            "avg_top_sim": round(sum(top_sims) / len(top_sims), 4),
            "avg_diversity": round(sum(diversities) / len(diversities), 4),
            "avg_ns_overlap": round(sum(ns_overlaps) / len(ns_overlaps), 4),
        }
    return out


def compare_pivot_results(results_a: list[dict], results_b: list[dict]) -> list[dict]:
    """Find the most interesting differences between two model runs.

    Looks for pivots where the top results changed significantly.
    """
    # Build lookup: (workspace_id, pivot_name) -> pivot_results
    def build_index(results):
        idx = {}
        for r in results:
            for p in r["pivot_results"]:
                key = (r["workspace_id"], p["pivot"]["name"])
                idx[key] = p
        return idx

    idx_a = build_index(results_a)
    idx_b = build_index(results_b)

    diffs = []
    for key in sorted(set(idx_a.keys()) & set(idx_b.keys())):
        pa = idx_a[key]
        pb = idx_b[key]

        qa = pa["quality"]
        qb = pb["quality"]

        if not qa.get("has_results") or not qb.get("has_results"):
            continue

        top_diff = qb["top_similarity"] - qa["top_similarity"]
        div_diff = qb["diversity_score"] - qa["diversity_score"]

        # Top result names
        top_a = pa["similar"][0]["name"] if pa["similar"] else "-"
        top_b = pb["similar"][0]["name"] if pb["similar"] else "-"

        diffs.append({
            "workspace": key[0],
            "pivot": key[1],
            "top_sim_a": qa["top_similarity"],
            "top_sim_b": qb["top_similarity"],
            "top_sim_delta": round(top_diff, 4),
            "div_a": qa["diversity_score"],
            "div_b": qb["diversity_score"],
            "div_delta": round(div_diff, 3),
            "top_result_a": top_a,
            "top_result_b": top_b,
            "top_changed": top_a != top_b,
        })

    # Sort by absolute top_sim_delta descending (biggest changes first)
    diffs.sort(key=lambda x: -abs(x["top_sim_delta"]))
    return diffs


def main():
    if len(sys.argv) < 3:
        print("Usage: compare_models.py <results_a.json> <results_b.json>")
        sys.exit(1)

    path_a, path_b = sys.argv[1], sys.argv[2]
    results_a = load_results(path_a)
    results_b = load_results(path_b)

    name_a = model_name(results_a)
    name_b = model_name(results_b)

    print(f"# Embedding Model Comparison")
    print(f"")
    print(f"**Model A:** `{name_a}` (from `{Path(path_a).name}`)")
    print(f"**Model B:** `{name_b}` (from `{Path(path_b).name}`)")
    print(f"")

    # Aggregate comparison
    agg_a = aggregate_quality(results_a)
    agg_b = aggregate_quality(results_b)

    print(f"## Aggregate Metrics")
    print(f"")
    print(f"| Metric | {name_a} | {name_b} | Delta |")
    print(f"|--------|{'---' * 5}|{'---' * 5}|-------|")
    for key in ["avg_top_sim", "avg_mean_sim", "avg_diversity", "avg_ns_overlap", "avg_same_kind", "cross_lang_pct"]:
        va = agg_a.get(key, 0)
        vb = agg_b.get(key, 0)
        delta = round(vb - va, 4)
        arrow = "+" if delta > 0 else ""
        print(f"| {key} | {va} | {vb} | {arrow}{delta} |")
    print(f"")

    # Per-workspace comparison
    per_a = per_workspace_quality(results_a)
    per_b = per_workspace_quality(results_b)

    print(f"## Per-Workspace Comparison (avg top similarity)")
    print(f"")
    print(f"| Workspace | Language | {name_a} | {name_b} | Delta |")
    print(f"|-----------|----------|{'---' * 5}|{'---' * 5}|-------|")
    for ws in sorted(set(per_a.keys()) | set(per_b.keys())):
        qa = per_a.get(ws, {})
        qb = per_b.get(ws, {})
        lang = qa.get("language", qb.get("language", "?"))
        sim_a = qa.get("avg_top_sim", "-")
        sim_b = qb.get("avg_top_sim", "-")
        if isinstance(sim_a, (int, float)) and isinstance(sim_b, (int, float)):
            delta = round(sim_b - sim_a, 4)
            arrow = "+" if delta > 0 else ""
            print(f"| {ws[:30]} | {lang} | {sim_a} | {sim_b} | {arrow}{delta} |")
        else:
            print(f"| {ws[:30]} | {lang} | {sim_a} | {sim_b} | - |")
    print(f"")

    # Biggest changes
    diffs = compare_pivot_results(results_a, results_b)
    if diffs:
        print(f"## Biggest Changes (top 15)")
        print(f"")
        print(f"| Workspace | Pivot | Top Sim A | Top Sim B | Delta | Top Result Changed? |")
        print(f"|-----------|-------|-----------|-----------|-------|---------------------|")
        for d in diffs[:15]:
            ws = d["workspace"][:20]
            arrow = "+" if d["top_sim_delta"] > 0 else ""
            changed = "YES" if d["top_changed"] else "no"
            print(
                f"| {ws} | `{d['pivot'][:25]}` | {d['top_sim_a']:.3f} | {d['top_sim_b']:.3f} "
                f"| {arrow}{d['top_sim_delta']:.3f} | {changed} |"
            )
        print(f"")

        # Show changed top results
        changed = [d for d in diffs if d["top_changed"]][:10]
        if changed:
            print(f"## Top Result Changes (first 10)")
            print(f"")
            for d in changed:
                print(f"**{d['workspace']}** / `{d['pivot']}`:")
                print(f"  - A: `{d['top_result_a']}` ({d['top_sim_a']:.3f})")
                print(f"  - B: `{d['top_result_b']}` ({d['top_sim_b']:.3f})")
                print(f"")


if __name__ == "__main__":
    main()
