"""Julie-only eval — mirrors eros bakeoff scoring without the slow lancedb candidates.

Reads the same corpus, runs the same julie-server command shape, computes
top1/top5 via the same `_first_matching_rank` + `_path_matches` predicates.
Output: per-repo and overall top1, top5, broken down by category.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import time
from collections import defaultdict
from pathlib import Path

CORPUS = "/Users/murphy/.eros-eval/eval/multi-lang-corpus/latest.json"
REPO_ROOT = Path(__file__).resolve().parents[3]
LIMIT = 5
TIMEOUT = 60


def git_head() -> str:
    proc = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        return "unknown"
    return proc.stdout.strip() or "unknown"


def resolve_julie_bin() -> Path:
    explicit = os.environ.get("JULIE_BIN")
    candidates = []
    if explicit:
        candidates.append(Path(explicit))
    candidates.extend(
        [
            REPO_ROOT / "target" / "debug" / "julie-server",
            REPO_ROOT / "target" / "release" / "julie-server",
        ]
    )
    discovered = shutil.which("julie-server")
    if discovered:
        candidates.append(Path(discovered))

    for candidate in candidates:
        if candidate.is_file():
            return candidate

    searched = ", ".join(str(candidate) for candidate in candidates)
    raise SystemExit(
        "julie-server binary not found. Build one with `cargo build` or set "
        f"JULIE_BIN. Searched: {searched}"
    )


def binary_metadata(path: Path) -> dict:
    stat = path.stat()
    return {
        "path": str(path),
        "mtime": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(stat.st_mtime)),
        "size_bytes": stat.st_size,
    }


def julie_search_target(category: str) -> str:
    if category in {"exact symbol lookup", "symbol intent lookup"}:
        return "definitions"
    if category in {"file/path search", "likely test lookup", "test intent lookup"}:
        return "files"
    return "content"


def normalize_path(path: str) -> str:
    return path.replace("\\", "/").strip()


def path_matches(candidate: str, expected: str) -> bool:
    return (
        candidate == expected
        or candidate.endswith(f"/{expected}")
        or expected.endswith(f"/{candidate}")
    )


_LINE_PATH_REGEX = re.compile(
    r"((?:[A-Za-z0-9_.-]+[/\\])*[A-Za-z0-9_.-]+\.[A-Za-z0-9]{1,10})(?::\d+)?"
)


def parse_julie_text_result(text: str):
    """Mirrors eros._parse_julie_text_result: extract first path-like token on each line."""
    out = []
    for line in text.splitlines():
        stripped = line.strip()
        m = _LINE_PATH_REGEX.match(stripped)
        if m:
            out.append({"path": normalize_path(m.group(1)), "text": stripped})
    return out or [{"text": text}]


def result_paths(result: dict):
    for key in ("path", "file_path", "filepath", "relative_path"):
        v = result.get(key)
        if isinstance(v, str):
            yield normalize_path(v)
    prov = result.get("provenance")
    if isinstance(prov, dict):
        v = prov.get("path")
        if isinstance(v, str):
            yield normalize_path(v)


def first_matching_rank(results, expected_paths):
    for i, r in enumerate(results, start=1):
        cands = list(result_paths(r))
        if any(path_matches(c, e) for c in cands for e in expected_paths):
            return i
    return None


def parse_julie_results(stdout: str):
    try:
        payload = json.loads(stdout)
    except json.JSONDecodeError:
        return tuple({"text": line} for line in stdout.splitlines() if line.strip())
    if isinstance(payload, dict):
        if isinstance(payload.get("results"), list):
            return tuple(payload["results"])
        if isinstance(payload.get("content"), list):
            out = []
            for item in payload["content"]:
                if isinstance(item, dict):
                    if isinstance(item.get("text"), str):
                        out.extend(parse_julie_text_result(item["text"]))
                    else:
                        out.append(item)
                elif isinstance(item, str):
                    out.extend(parse_julie_text_result(item))
            return tuple(out)
    if isinstance(payload, list):
        return tuple(x for x in payload if isinstance(x, dict))
    return ({"value": payload},)


def run_julie(julie_bin: Path, repo: str, query: str, target: str):
    cmd = [
        str(julie_bin),
        "--workspace", repo,
        "--json",
        "--standalone",
        "search",
        "--target", target,
        "--limit", str(LIMIT),
        query,
    ]
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        return None, "timeout"
    if proc.returncode != 0 and not proc.stdout.strip():
        return None, f"exit {proc.returncode}: {proc.stderr[:200]}"
    return parse_julie_results(proc.stdout), None


def main():
    commit = git_head()
    julie_bin = resolve_julie_bin()
    binary = binary_metadata(julie_bin)

    with open(CORPUS) as f:
        corpus = json.load(f)

    queries = corpus["queries"]
    n = len(queries)
    print(f"corpus: {n} queries across {len({q['repo'] for q in queries})} repos", flush=True)
    print(f"commit: {commit}", flush=True)
    print(
        f"julie-server: {binary['path']} "
        f"(mtime={binary['mtime']}, size={binary['size_bytes']})",
        flush=True,
    )

    rank_counts = defaultdict(int)  # rank -> count
    by_cat = defaultdict(lambda: defaultdict(int))  # category -> rank -> count
    by_repo = defaultdict(lambda: defaultdict(int))  # repo -> rank -> count
    unavailable = []
    misses_top1 = []  # queries that didn't hit top1

    start = time.time()
    for i, q in enumerate(queries, start=1):
        target = julie_search_target(q["category"])
        results, err = run_julie(julie_bin, q["repo"], q["query"], target)
        if results is None:
            unavailable.append((q["id"], err))
            rank_counts["unavailable"] += 1
            by_cat[q["category"]]["unavailable"] += 1
            repo_name = Path(q["repo"]).name
            by_repo[repo_name]["unavailable"] += 1
            continue
        rank = first_matching_rank(results, q["expected_paths"])
        repo_name = Path(q["repo"]).name
        if rank is None:
            rank_counts["miss"] += 1
            by_cat[q["category"]]["miss"] += 1
            by_repo[repo_name]["miss"] += 1
            misses_top1.append(q["id"])
        else:
            rank_counts[rank] += 1
            by_cat[q["category"]][rank] += 1
            by_repo[repo_name][rank] += 1
            if rank != 1:
                misses_top1.append(q["id"])

        if i % 25 == 0:
            elapsed = time.time() - start
            top1 = rank_counts[1]
            print(f"  [{i}/{n}] elapsed={elapsed:.1f}s  top1={top1}/{i} ({100*top1/i:.1f}%)", flush=True)

    elapsed = time.time() - start
    top1 = rank_counts[1]
    top5 = sum(rank_counts[r] for r in (1, 2, 3, 4, 5))
    print()
    print(f"=== JULIE-ONLY EVAL RESULT (commit {commit}) ===")
    print(f"Total queries: {n}")
    print(f"Elapsed: {elapsed:.1f}s ({elapsed/n:.2f}s/query)")
    print(f"Top1: {top1}/{n} ({100*top1/n:.1f}%)")
    print(f"Top5: {top5}/{n} ({100*top5/n:.1f}%)")
    print(f"Misses (rank=None): {rank_counts['miss']}")
    print(f"Unavailable (errors/timeouts): {rank_counts['unavailable']}")
    print()
    print("Rank distribution:")
    for r in sorted([k for k in rank_counts if isinstance(k, int)]):
        print(f"  rank {r}: {rank_counts[r]}")
    print(f"  miss: {rank_counts['miss']}")
    print(f"  unavailable: {rank_counts['unavailable']}")
    print()
    print("By category:")
    for cat in sorted(by_cat):
        total = sum(by_cat[cat].values())
        t1 = by_cat[cat].get(1, 0)
        miss = by_cat[cat].get("miss", 0)
        unavail = by_cat[cat].get("unavailable", 0)
        print(f"  {cat}: top1={t1}/{total} ({100*t1/total:.1f}%) miss={miss} unavail={unavail}")
    print()
    print("By repo:")
    for repo in sorted(by_repo):
        total = sum(by_repo[repo].values())
        t1 = by_repo[repo].get(1, 0)
        print(f"  {repo}: top1={t1}/{total}")

    # Write artifact
    out_path = "/tmp/julie-only-eval-result.json"
    with open(out_path, "w") as f:
        json.dump({
            "commit": commit,
            "binary": binary,
            "total_queries": n,
            "top1": top1,
            "top5": top5,
            "elapsed_seconds": elapsed,
            "rank_counts": {str(k): v for k, v in rank_counts.items()},
            "by_category": {k: {str(rk): rv for rk, rv in v.items()} for k, v in by_cat.items()},
            "by_repo": {k: {str(rk): rv for rk, rv in v.items()} for k, v in by_repo.items()},
            "unavailable": unavailable,
            "misses_top1": misses_top1,
        }, f, indent=2)
    print(f"\nArtifact: {out_path}")
    print(f"T12 gate: top1 >= 350/406. {'PASS' if top1 >= 350 else 'FAIL'} (margin {top1-350:+d}).")


if __name__ == "__main__":
    main()
