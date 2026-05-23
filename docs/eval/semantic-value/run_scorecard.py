#!/usr/bin/env python3
"""Run Julie semantic-value scorecard against local repos."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
import tomllib
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
DEFAULT_SCORECARD = ROOT / "docs/eval/semantic-value/scorecard.toml"
DEFAULT_RESULTS_DIR = ROOT / "docs/eval/semantic-value/results"
HIT_RE = re.compile(r"^\s{2}([^:\n]+):")
FALLBACK_PATTERNS = (
    "falling back",
    "fell back",
    "fallback to lexical",
    "semantic backend unavailable",
    "semantic search unavailable",
)


@dataclass(frozen=True)
class BackendResult:
    backend: str
    ok: bool
    rank: int | None
    hits: list[str]
    latency_ms: int
    fallback: bool
    error: str | None
    text: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scorecard", type=Path, default=DEFAULT_SCORECARD)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--repo", action="append", default=[])
    parser.add_argument("--case", action="append", default=[])
    parser.add_argument("--backend", action="append", default=[])
    parser.add_argument("--limit-cases", type=int)
    parser.add_argument("--timeout", type=int, default=90)
    parser.add_argument("--no-write", action="store_true")
    return parser.parse_args()


def load_scorecard(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def repo_map(scorecard: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {repo["name"]: repo for repo in scorecard.get("repos", [])}


def selected_cases(scorecard: dict[str, Any], args: argparse.Namespace) -> list[dict[str, Any]]:
    cases = scorecard.get("cases", [])
    if args.repo:
        repos = set(args.repo)
        cases = [case for case in cases if case["repo"] in repos]
    if args.case:
        wanted = set(args.case)
        cases = [case for case in cases if case["id"] in wanted]
    if args.limit_cases:
        cases = cases[: args.limit_cases]
    return cases


def selected_backends(scorecard: dict[str, Any], args: argparse.Namespace) -> list[str]:
    return args.backend or scorecard.get("settings", {}).get("backends", ["lexical", "semantic", "hybrid"])


def parse_json_payload(stdout: str) -> dict[str, Any]:
    text = stdout.strip()
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass

    for index, char in enumerate(stdout):
        if char != "{":
            continue
        candidate = stdout[index:].strip()
        try:
            return json.loads(candidate)
        except json.JSONDecodeError:
            continue

    raise ValueError("no JSON object found in julie-server output")


def result_text(payload: dict[str, Any]) -> str:
    parts = []
    for item in payload.get("content", []):
        if item.get("type") == "text":
            parts.append(item.get("text", ""))
    if not parts and "text" in payload:
        parts.append(str(payload["text"]))
    return "\n".join(parts)


def extract_hits(text: str) -> list[str]:
    hits = []
    for line in text.splitlines():
        match = HIT_RE.match(line)
        if match:
            hits.append(match.group(1))
    return hits


def first_expected_rank(hits: list[str], expected: list[str]) -> int | None:
    for index, hit in enumerate(hits, start=1):
        if any(fragment in hit for fragment in expected):
            return index
    return None


def run_backend(
    binary: Path,
    repo_path: Path,
    case: dict[str, Any],
    backend: str,
    settings: dict[str, Any],
    timeout: int,
) -> BackendResult:
    params: dict[str, Any] = {
        "query": case["query"],
        "backend": backend,
        "return_format": settings.get("return_format", "locations"),
        "limit": case.get("limit", settings.get("limit", 8)),
    }

    exclude_tests = case.get("exclude_tests", settings.get("exclude_tests"))
    if exclude_tests is not None:
        params["exclude_tests"] = exclude_tests
    if "file_pattern" in case:
        params["file_pattern"] = case["file_pattern"]

    command = [
        str(binary),
        "tool",
        "fast_search",
        "--workspace",
        str(repo_path),
        "--json",
        "-p",
        json.dumps(params, separators=(",", ":")),
    ]

    started = time.perf_counter()
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
        latency_ms = int((time.perf_counter() - started) * 1000)
        payload = parse_json_payload(completed.stdout)
        text = result_text(payload)
        hits = extract_hits(text)
        rank = first_expected_rank(hits, case.get("expected_any", []))
        lower_text = text.lower()
        fallback = any(pattern in lower_text for pattern in FALLBACK_PATTERNS)
        if completed.returncode != 0 or payload.get("isError"):
            message = text or (completed.stderr or completed.stdout).strip()
            return BackendResult(backend, False, rank, hits, latency_ms, fallback, message, text)
        return BackendResult(backend, True, rank, hits, latency_ms, fallback, None, text)
    except Exception as error:
        latency_ms = int((time.perf_counter() - started) * 1000)
        return BackendResult(backend, False, None, [], latency_ms, False, str(error), "")


def mrr(rank: int | None) -> float:
    return 0.0 if rank is None else 1.0 / rank


def topk(rank: int | None, limit: int) -> int:
    return int(rank is not None and rank <= limit)


def comparable_rank(rank: int | None) -> int:
    return rank if rank is not None else 1_000_000


def outcome(row: dict[str, Any]) -> str:
    lexical_result = row["results"].get("lexical", {})
    lexical = lexical_result.get("rank") if lexical_result.get("ok") else None
    semantic_candidates = [
        result.get("rank")
        for backend in ("semantic", "hybrid")
        if (result := row["results"].get(backend, {})).get("ok") and not result.get("fallback")
    ]
    if not semantic_candidates:
        return "semantic_unavailable"

    lexical_rank = comparable_rank(lexical)
    semantic_rank = min(comparable_rank(rank) for rank in semantic_candidates)

    if lexical_rank == semantic_rank:
        return "tie"
    if semantic_rank < lexical_rank:
        return "semantic_win"
    if lexical_rank < semantic_rank:
        return "lexical_win"
    return "both_miss"


def summarize(rows: list[dict[str, Any]], backends: list[str]) -> dict[str, Any]:
    scored = [row for row in rows if row["score"]]
    backend_metrics: dict[str, Any] = {}
    for backend in backends:
        eligible = [
            row["results"][backend]
            for row in scored
            if backend in row["results"]
            and row["results"][backend].get("ok")
            and not row["results"][backend].get("fallback")
        ]
        ranks = [result.get("rank") for result in eligible]
        count = len(ranks)
        backend_metrics[backend] = {
            "cases": count,
            "top1": sum(topk(rank, 1) for rank in ranks) / count if count else 0,
            "top3": sum(topk(rank, 3) for rank in ranks) / count if count else 0,
            "top5": sum(topk(rank, 5) for rank in ranks) / count if count else 0,
            "top8": sum(topk(rank, 8) for rank in ranks) / count if count else 0,
            "mrr": sum(mrr(rank) for rank in ranks) / count if count else 0,
            "p95_latency_ms": p95(
                [
                    result["latency_ms"]
                    for result in eligible
                ]
            ),
        }

    outcomes: dict[str, int] = {}
    for row in scored:
        result = outcome(row)
        row["outcome"] = result
        outcomes[result] = outcomes.get(result, 0) + 1

    fallback_cases = [
        {
            "id": row["id"],
            "repo": row["repo"],
            "fallback_backends": [
                backend
                for backend, result in row["results"].items()
                if result.get("fallback")
            ],
        }
        for row in rows
        if not row["score"] or any(result.get("fallback") for result in row["results"].values())
    ]

    return {
        "scored_cases": len(scored),
        "backend_metrics": backend_metrics,
        "outcomes": outcomes,
        "fallback_cases": fallback_cases,
        "errors": [
            {
                "id": row["id"],
                "repo": row["repo"],
                "backend": backend,
                "error": result["error"],
            }
            for row in rows
            for backend, result in row["results"].items()
            if result.get("error")
        ],
    }


def p95(values: list[int]) -> int | None:
    if not values:
        return None
    ordered = sorted(values)
    index = int(round((len(ordered) - 1) * 0.95))
    return ordered[index]


def validate(scorecard: dict[str, Any], repos: dict[str, dict[str, Any]], cases: list[dict[str, Any]]) -> list[str]:
    warnings = []
    for case in cases:
        repo = repos.get(case["repo"])
        if repo is None:
            warnings.append(f"{case['id']}: unknown repo {case['repo']}")
            continue
        repo_path = Path(repo["path"])
        if not repo_path.exists():
            warnings.append(f"{case['id']}: repo path does not exist: {repo_path}")
            continue
        for fragment in case.get("expected_any", []):
            if any(char in fragment for char in "*?[]"):
                continue
            if not (repo_path / fragment).exists():
                warnings.append(f"{case['id']}: expected path missing: {repo_path / fragment}")
    return warnings


def as_dict(result: BackendResult) -> dict[str, Any]:
    return {
        "ok": result.ok,
        "rank": result.rank,
        "hits": result.hits,
        "latency_ms": result.latency_ms,
        "fallback": result.fallback,
        "error": result.error,
    }


def run_scorecard(scorecard: dict[str, Any], args: argparse.Namespace) -> dict[str, Any]:
    settings = scorecard.get("settings", {})
    binary = args.binary or Path(settings.get("binary", "./target/debug/julie-server"))
    if not binary.is_absolute():
        binary = ROOT / binary
    repos = repo_map(scorecard)
    cases = selected_cases(scorecard, args)
    backends = selected_backends(scorecard, args)
    warnings = validate(scorecard, repos, cases)

    rows = []
    for case in cases:
        repo = repos[case["repo"]]
        row = {
            "id": case["id"],
            "repo": case["repo"],
            "language": repo.get("language"),
            "category": case.get("category"),
            "query": case["query"],
            "intent": case.get("intent"),
            "expected_any": case.get("expected_any", []),
            "score": case.get("score", True),
            "results": {},
        }
        for backend in backends:
            result = run_backend(binary, Path(repo["path"]), case, backend, settings, args.timeout)
            row["results"][backend] = as_dict(result)
            print(
                f"{case['id']} {backend}: "
                f"rank={result.rank or '-'} hits={len(result.hits)} "
                f"latency={result.latency_ms}ms",
                flush=True,
            )
            if result.error:
                print(f"  error: {result.error}", file=sys.stderr)
        rows.append(row)

    summary = summarize(rows, backends)
    return {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "scorecard": str(args.scorecard),
        "backends": backends,
        "warnings": warnings,
        "summary": summary,
        "cases": rows,
    }


def percent(value: float) -> str:
    return f"{value * 100:.1f}%"


def markdown_report(result: dict[str, Any]) -> str:
    lines = [
        "# Semantic Search Value Scorecard Results",
        "",
        f"Generated: `{result['generated_at']}`",
        f"Scored cases: `{result['summary']['scored_cases']}`",
        "",
        "## Backend Metrics",
        "",
        "| backend | cases | top1 | top3 | top5 | top8 | mrr | p95 latency |",
        "|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for backend, metrics in result["summary"]["backend_metrics"].items():
        p95_value = metrics["p95_latency_ms"]
        p95_text = "-" if p95_value is None else f"{p95_value}ms"
        lines.append(
            "| {backend} | {cases} | {top1} | {top3} | {top5} | {top8} | {mrr:.3f} | {p95} |".format(
                backend=backend,
                cases=metrics["cases"],
                top1=percent(metrics["top1"]),
                top3=percent(metrics["top3"]),
                top5=percent(metrics["top5"]),
                top8=percent(metrics["top8"]),
                mrr=metrics["mrr"],
                p95=p95_text,
            )
        )

    lines.extend(["", "## Outcomes", ""])
    outcomes = result["summary"]["outcomes"]
    for name in sorted(outcomes):
        lines.append(f"- `{name}`: {outcomes[name]}")

    if result.get("warnings"):
        lines.extend(["", "## Warnings", ""])
        for warning in result["warnings"]:
            lines.append(f"- {warning}")

    lines.extend(
        [
            "",
            "## Case Results",
            "",
            "| case | repo | category | lexical | semantic | hybrid | outcome |",
            "|---|---|---|---:|---:|---:|---|",
        ]
    )
    for row in result["cases"]:
        results = row["results"]
        lines.append(
            "| {case} | {repo} | {category} | {lexical} | {semantic} | {hybrid} | {outcome} |".format(
                case=row["id"],
                repo=row["repo"],
                category=row.get("category") or "",
                lexical=rank_text(results.get("lexical")),
                semantic=rank_text(results.get("semantic")),
                hybrid=rank_text(results.get("hybrid")),
                outcome=row.get("outcome", "-"),
            )
        )

    fallback_cases = result["summary"]["fallback_cases"]
    if fallback_cases:
        lines.extend(["", "## Fallback Cases", ""])
        for case in fallback_cases:
            backends = ", ".join(case["fallback_backends"]) or "none detected"
            lines.append(f"- `{case['id']}` ({case['repo']}): {backends}")

    if result["summary"]["errors"]:
        lines.extend(["", "## Errors", ""])
        for error in result["summary"]["errors"]:
            lines.append(f"- `{error['id']}` {error['backend']}: {error['error']}")

    return "\n".join(lines) + "\n"


def rank_text(result: dict[str, Any] | None) -> str:
    if not result:
        return "-"
    if result.get("error"):
        return "error"
    if result.get("fallback"):
        return "fallback"
    return str(result.get("rank") or "-")


def write_outputs(result: dict[str, Any]) -> tuple[Path, Path]:
    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H-%M-%SZ")
    DEFAULT_RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    json_path = DEFAULT_RESULTS_DIR / f"{timestamp}.json"
    md_path = DEFAULT_RESULTS_DIR / f"{timestamp}.md"
    json_path.write_text(json.dumps(result, indent=2) + "\n")
    md_path.write_text(markdown_report(result))
    return json_path, md_path


def main() -> int:
    args = parse_args()
    scorecard = load_scorecard(args.scorecard)
    result = run_scorecard(scorecard, args)
    if not args.no_write:
        json_path, md_path = write_outputs(result)
        print(f"Wrote {json_path}")
        print(f"Wrote {md_path}")
    print(markdown_report(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
