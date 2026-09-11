#!/usr/bin/env python3
"""Head-to-head retrieval matrix: Julie vs Miller on the public corpus."""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import subprocess
import sys
import time
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from mcp_client import McpProcess, content_text

ROOT = Path(__file__).resolve().parents[3]
DEFAULT_CASES = Path(__file__).resolve().parent / "cases.json"
DEFAULT_RESULTS = Path(__file__).resolve().parent / "results"
DEFAULT_JULIE = ROOT / "target/release/julie-server"
DEFAULT_MILLER = Path.home() / "source/miller/src/Miller.Server/bin/Release/net10.0/miller"

JULIE_TEN_TOOLS = {
    "fast_search",
    "get_symbols",
    "deep_dive",
    "fast_refs",
    "call_path",
    "blast_radius",
    "get_context",
    "patterns",
    "edit_file",
    "manage_workspace",
}
SUPPORTED_JULIE_TOOLS = {
    "fast_search",
    "get_symbols",
    "deep_dive",
    "fast_refs",
    "call_path",
    "blast_radius",
    "get_context",
    "patterns",
}
SUPPORTED_MILLER_TOOLS = {"search", "inspect", "context", "trace", "impact", "patterns"}
SUPPORTED_SCORING = {"path_top", "path_top5"}
REQUIRED_ROW_KEYS = {"id", "repo", "task_class", "intent", "julie", "miller", "expected", "scoring"}
PATH_LINE_RE = re.compile(r"(?P<path>[A-Za-z0-9_@./+\-]+\.[A-Za-z0-9_+\-]+):(?P<line>\d+)")
FILE_HEADER_RE = re.compile(r"^(?P<path>\S[^\n]*\.[A-Za-z0-9_+-]+):\s*$")


def split_filter(value: str) -> set[str]:
    if value.strip().lower() == "all":
        return set()
    return {item.strip() for item in value.split(",") if item.strip()}


def git_head(path: Path) -> str | None:
    try:
        proc = subprocess.run(
            ["git", "-C", str(path), "rev-parse", "HEAD"],
            check=False,
            capture_output=True,
            text=True,
        )
    except OSError:
        return None
    if proc.returncode != 0:
        return None
    return proc.stdout.strip()


def validate_manifest(document: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    repos = document.get("repos")
    if not isinstance(repos, dict):
        errors.append("manifest must contain a 'repos' object")
        repos = {}

    repo_meta: dict[str, dict[str, str]] = {}
    for name, spec in repos.items():
        label = f"repo {name}"
        if not isinstance(spec, dict):
            errors.append(f"{label}: must be an object with path and commit")
            continue
        path = spec.get("path")
        commit = spec.get("commit")
        if not isinstance(path, str) or not path.strip():
            errors.append(f"{label}: 'path' must be a non-empty string")
            continue
        if not isinstance(commit, str) or not commit.strip():
            errors.append(f"{label}: 'commit' must be a non-empty string")
            continue
        root = Path(path)
        if not root.is_dir():
            errors.append(f"{label}: path does not exist: {path}")
            continue
        head = git_head(root)
        if head is None:
            errors.append(f"{label}: git rev-parse HEAD failed")
        elif head != commit:
            errors.append(f"{label}: HEAD differs from pinned commit ({head} != {commit})")
        repo_meta[str(name)] = {"path": path, "commit": commit}

    rows = document.get("rows")
    if not isinstance(rows, list):
        errors.append("manifest must contain a 'rows' list")
        return errors

    seen_ids: set[str] = set()
    for index, row in enumerate(rows):
        label = f"row {index}"
        if not isinstance(row, dict):
            errors.append(f"{label}: expected object row")
            continue
        if isinstance(row.get("id"), str) and row["id"].strip():
            label = f"row {index} ({row['id']})"
        missing = sorted(REQUIRED_ROW_KEYS - set(row))
        for key in missing:
            errors.append(f"{label}: missing required key '{key}'")

        row_id = row.get("id")
        if isinstance(row_id, str):
            if row_id in seen_ids:
                errors.append(f"{label}: duplicate id '{row_id}'")
            seen_ids.add(row_id)
        else:
            errors.append(f"{label}: 'id' must be a string")

        repo = row.get("repo")
        if not isinstance(repo, str) or not repo.strip():
            errors.append(f"{label}: 'repo' must be a non-empty string")
        elif repo not in repo_meta:
            errors.append(f"{label}: unknown repo '{repo}'")

        for key in ["task_class", "intent"]:
            if key in row and (not isinstance(row[key], str) or not row[key].strip()):
                errors.append(f"{label}: '{key}' must be a non-empty string")

        julie = row.get("julie")
        if not isinstance(julie, dict):
            errors.append(f"{label}: 'julie' must be an object")
        else:
            tool = julie.get("tool")
            if not isinstance(tool, str) or not tool.strip():
                errors.append(f"{label}: 'julie.tool' must be a non-empty string")
            elif tool not in JULIE_TEN_TOOLS:
                errors.append(f"{label}: unsupported julie tool '{tool}'")
            if not isinstance(julie.get("args"), dict):
                errors.append(f"{label}: 'julie.args' must be an object")

        miller = row.get("miller")
        if not isinstance(miller, dict):
            errors.append(f"{label}: 'miller' must be an object")
        else:
            tool = miller.get("tool")
            if not isinstance(tool, str) or not tool.strip():
                errors.append(f"{label}: 'miller.tool' must be a non-empty string")
            elif tool not in SUPPORTED_MILLER_TOOLS:
                errors.append(f"{label}: unsupported miller tool '{tool}'")
            if not isinstance(miller.get("args"), dict):
                errors.append(f"{label}: 'miller.args' must be an object")

        expected = row.get("expected")
        if not isinstance(expected, dict):
            errors.append(f"{label}: 'expected' must be an object")
        else:
            path = expected.get("path")
            if not isinstance(path, str) or not path.strip():
                errors.append(f"{label}: 'expected.path' must be a non-empty string")
            elif isinstance(repo, str) and repo in repo_meta:
                full = Path(repo_meta[repo]["path"]) / path
                if not full.is_file():
                    errors.append(f"{label}: expected.path does not exist: {path}")
            if "anchor" in expected and not isinstance(expected.get("anchor"), str):
                errors.append(f"{label}: 'expected.anchor' must be a string")

        scoring = row.get("scoring")
        if not isinstance(scoring, dict):
            errors.append(f"{label}: 'scoring' must be an object")
        elif scoring.get("mode") not in SUPPORTED_SCORING:
            errors.append(f"{label}: unsupported scoring.mode {scoring.get('mode')!r}")

    return errors


def json_row_path(row: dict[str, Any]) -> str:
    for key in ("file", "path", "display_path", "file_path", "relative_path"):
        value = row.get(key)
        if isinstance(value, str) and value.strip():
            return value.replace("\\", "/")
    loc = row.get("location")
    if isinstance(loc, dict):
        return json_row_path(loc)
    return ""


def paths_from_json(value: Any) -> list[str]:
    paths: list[str] = []

    def add(path: str) -> None:
        path = path.replace("\\", "/")
        if path and path not in paths:
            paths.append(path)

    if isinstance(value, list):
        for item in value:
            if isinstance(item, dict):
                path = json_row_path(item)
                if path:
                    add(path)
                elif "results" in item:
                    paths.extend(p for p in paths_from_json(item["results"]) if p not in paths)
            elif isinstance(item, str):
                add(item)
        return paths
    if isinstance(value, dict):
        for key in ("results", "hits", "symbols", "items"):
            if key in value:
                nested = paths_from_json(value[key])
                if nested:
                    return nested
        path = json_row_path(value)
        if path:
            add(path)
            return paths
        for nested in value.values():
            if isinstance(nested, (dict, list)):
                for path in paths_from_json(nested):
                    add(path)
    return paths


def ranked_paths(text: str) -> list[str]:
    text = text.strip()
    if text.startswith("{") or text.startswith("["):
        try:
            parsed = json.loads(text)
        except json.JSONDecodeError:
            parsed = None
        if parsed is not None:
            paths = paths_from_json(parsed)
            if paths:
                return paths
    paths: list[str] = []

    def add(path: str) -> None:
        path = path.replace("\\", "/")
        if path and path not in paths:
            paths.append(path)

    for line in text.splitlines():
        header = FILE_HEADER_RE.match(line)
        if header:
            add(header.group("path"))
            continue
        match = PATH_LINE_RE.search(line)
        if match:
            add(match.group("path"))
    return paths


def score_row(text: str, expected_path: str, mode: str) -> dict[str, Any]:
    ranked = ranked_paths(text)
    present = expected_path in text or expected_path in ranked
    top = bool(ranked) and ranked[0] == expected_path
    top5 = expected_path in ranked[:5]
    if mode == "path_top":
        passed = top
    else:
        passed = top5
    return {
        "empty": not bool(text.strip()),
        "expected_present": present,
        "expected_top": top,
        "expected_top5": top5,
        "scoring_pass": passed,
        "first_path": ranked[0] if ranked else "",
        "ranked_paths": ranked[:8],
        "output_chars": len(text),
        "output_bytes": len(text.encode("utf-8")),
    }


def percentile(values: list[int], pct: float) -> int:
    if not values:
        return 0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, math.ceil(pct / 100 * len(ordered)) - 1))
    return ordered[index]


def _first_workspace_id(value: Any) -> str | None:
    if isinstance(value, dict):
        raw = value.get("workspace_id")
        if isinstance(raw, str) and raw.strip() and raw not in {"primary", "current"}:
            return raw.strip()
        for nested in value.values():
            found = _first_workspace_id(nested)
            if found:
                return found
    elif isinstance(value, list):
        for nested in value:
            found = _first_workspace_id(nested)
            if found:
                return found
    return None


def parse_workspace_id(text: str, fallback: str) -> str:
    compact = re.search(r"^workspace_id:\s*(\S+)", text, re.MULTILINE)
    if compact:
        return compact.group(1)
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        parsed = None
    found = _first_workspace_id(parsed) if parsed is not None else None
    if found:
        return found
    return fallback


def miller_index_ready(text: str) -> bool:
    if "artifact_missing" in text:
        return False
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        parsed = None
    if isinstance(parsed, dict):
        store = parsed.get("store")
        if isinstance(store, dict) and store.get("state") == "ready":
            return True
        index = parsed.get("index")
        if isinstance(index, dict) and int(index.get("document_count") or 0) > 0:
            return True
    return bool(re.search(r"store[=:] *ready|document_count\": [1-9]", text))


def wait_miller_ready(mcp: McpProcess, workspace_id: str, path: str) -> None:
    deadline = time.time() + 180
    last = ""
    while time.time() < deadline:
        _, response = mcp.call_tool(
            "workspace",
            {"operation": "status", "workspace_id": workspace_id, "format": "json"},
            timeout=60,
        )
        last = content_text(response)
        if miller_index_ready(last):
            return
        time.sleep(2)
    print(f"miller index not ready for {path}: {last[:200]}", file=sys.stderr)


def skipped(row: dict[str, Any], product: str, reason: str) -> dict[str, Any]:
    return {
        "id": row["id"],
        "repo": row["repo"],
        "task_class": row["task_class"],
        "product": product,
        "tool": row[product]["tool"],
        "ms": 0,
        "output_bytes": 0,
        "scoring_pass": False,
        "expected_top": False,
        "expected_top5": False,
        "expected_present": False,
        "first_path": "",
        "ranked_paths": [],
        "error": reason,
        "text": "",
    }


def execute_call(
    row: dict[str, Any],
    product: str,
    mcp: McpProcess,
    args: dict[str, Any],
) -> dict[str, Any]:
    tool = row[product]["tool"]
    try:
        ms, response = mcp.call_tool(tool, args, timeout=120)
    except Exception as exc:  # noqa: BLE001 — harness must record product failures
        return skipped(row, product, str(exc))
    text = content_text(response)
    if "error" in response:
        error = json.dumps(response["error"])
        scored = score_row(text, row["expected"]["path"], row["scoring"]["mode"])
        scored.update(
            {
                "id": row["id"],
                "repo": row["repo"],
                "task_class": row["task_class"],
                "product": product,
                "tool": tool,
                "ms": ms,
                "error": error,
                "text": text[:8000],
            }
        )
        return scored
    scored = score_row(text, row["expected"]["path"], row["scoring"]["mode"])
    scored.update(
        {
            "id": row["id"],
            "repo": row["repo"],
            "task_class": row["task_class"],
            "product": product,
            "tool": tool,
            "ms": ms,
            "error": "",
            "text": text[:8000],
        }
    )
    return scored


def summarize(results: list[dict[str, Any]]) -> dict[str, Any]:
    by_class: dict[str, dict[str, list[dict[str, Any]]]] = defaultdict(lambda: defaultdict(list))
    by_tool: dict[str, list[int]] = defaultdict(list)
    by_tool_bytes: dict[str, list[int]] = defaultdict(list)
    for row in results:
        by_class[row["task_class"]][row["product"]].append(row)
        key = f"{row['product']}.{row['tool']}"
        by_tool[key].append(int(row["ms"]))
        by_tool_bytes[key].append(int(row["output_bytes"]))

    classes: dict[str, Any] = {}
    for task_class, products in sorted(by_class.items()):
        entry: dict[str, Any] = {}
        for product, rows in products.items():
            n = len(rows)
            entry[product] = {
                "n": n,
                "top1": sum(1 for row in rows if row.get("expected_top")) / n if n else 0,
                "top5": sum(1 for row in rows if row.get("expected_top5")) / n if n else 0,
                "present": sum(1 for row in rows if row.get("expected_present")) / n if n else 0,
                "pass": sum(1 for row in rows if row.get("scoring_pass")) / n if n else 0,
            }
        classes[task_class] = entry

    tools: dict[str, Any] = {}
    for key, values in sorted(by_tool.items()):
        tools[key] = {
            "n": len(values),
            "p50_ms": percentile(values, 50),
            "p95_ms": percentile(values, 95),
            "p50_bytes": percentile(by_tool_bytes[key], 50),
            "p95_bytes": percentile(by_tool_bytes[key], 95),
        }
    return {"task_class": classes, "tools": tools}


def render_markdown(stamp: str, summary: dict[str, Any], results: list[dict[str, Any]]) -> str:
    lines = [
        f"# Head-to-head retrieval matrix ({stamp})",
        "",
        "Retrieval matrix only.",
        "",
        "## Per task class",
        "",
        "| class | julie top-1 | julie top-5 | miller top-1 | miller top-5 | n |",
        "| --- | ---: | ---: | ---: | ---: | ---: |",
    ]
    for task_class, entry in summary["task_class"].items():
        julie = entry.get("julie", {})
        miller = entry.get("miller", {})
        n = julie.get("n") or miller.get("n") or 0
        lines.append(
            "| {cls} | {jt1:.0%} | {jt5:.0%} | {mt1:.0%} | {mt5:.0%} | {n} |".format(
                cls=task_class,
                jt1=julie.get("top1", 0),
                jt5=julie.get("top5", 0),
                mt1=miller.get("top1", 0),
                mt5=miller.get("top5", 0),
                n=n,
            )
        )
    lines.extend(
        [
            "",
            "## Per tool latency",
            "",
            "| tool | n | p50 ms | p95 ms | p50 bytes | p95 bytes |",
            "| --- | ---: | ---: | ---: | ---: | ---: |",
        ]
    )
    for tool, entry in summary["tools"].items():
        lines.append(
            f"| {tool} | {entry['n']} | {entry['p50_ms']} | {entry['p95_ms']} | {entry['p50_bytes']} | {entry['p95_bytes']} |"
        )
    disagreements = [
        row_id
        for row_id in sorted({row["id"] for row in results})
        if _disagree(results, row_id)
    ]
    lines.extend(["", "## Disagreement row ids", ""])
    if disagreements:
        for row_id in disagreements:
            lines.append(f"- `{row_id}`")
    else:
        lines.append("- none")
    lines.append("")
    return "\n".join(lines)


def _disagree(results: list[dict[str, Any]], row_id: str) -> bool:
    pair = {row["product"]: row for row in results if row["id"] == row_id}
    if "julie" not in pair or "miller" not in pair:
        return True
    return bool(pair["julie"].get("scoring_pass")) != bool(pair["miller"].get("scoring_pass"))


def run_matrix(
    document: dict[str, Any],
    selected: list[dict[str, Any]],
    julie_bin: Path,
    miller_bin: Path,
    skip_julie: bool,
    skip_miller: bool,
) -> list[dict[str, Any]]:
    repos: dict[str, dict[str, str]] = document["repos"]
    results: list[dict[str, Any]] = []
    miller: McpProcess | None = None
    miller_ids: dict[str, str] = {}
    julie_procs: dict[str, McpProcess] = {}
    try:
        if not skip_miller:
            miller = McpProcess([str(miller_bin), "serve"], timeout=60)
        for row in selected:
            repo = row["repo"]
            path = repos[repo]["path"]
            print(f"== {row['id']} ==", file=sys.stderr)
            if skip_miller:
                results.append(skipped(row, "miller", "--skip-miller"))
            else:
                assert miller is not None
                if repo not in miller_ids:
                    _, response = miller.call_tool(
                        "workspace",
                        {"operation": "open", "path": path, "format": "json"},
                        timeout=300,
                    )
                    workspace_id = parse_workspace_id(content_text(response), path)
                    wait_miller_ready(miller, workspace_id, path)
                    miller_ids[repo] = workspace_id
                args = dict(row["miller"]["args"])
                args["workspace_id"] = miller_ids[repo]
                results.append(execute_call(row, "miller", miller, args))
            if skip_julie:
                results.append(skipped(row, "julie", "--skip-julie"))
                continue
            if row["julie"]["tool"] not in SUPPORTED_JULIE_TOOLS:
                results.append(skipped(row, "julie", f"runner does not call {row['julie']['tool']}"))
                continue
            if repo not in julie_procs:
                proc = McpProcess([str(julie_bin)], timeout=60, cwd=path)
                proc.call_tool("manage_workspace", {"operation": "open", "path": path}, timeout=300)
                julie_procs[repo] = proc
            results.append(execute_call(row, "julie", julie_procs[repo], dict(row["julie"]["args"])))
        return results
    finally:
        if miller is not None:
            miller.close()
        for proc in julie_procs.values():
            proc.close()


def load_cases(path: Path) -> tuple[dict[str, Any], list[str]]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return {}, [f"manifest not found: {path}"]
    except json.JSONDecodeError as exc:
        return {}, [f"manifest JSON parse failed: {exc}"]
    if not isinstance(document, dict):
        return {}, ["manifest must be a JSON object"]
    return document, validate_manifest(document)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", default=str(DEFAULT_CASES))
    parser.add_argument("--julie-bin", default=str(DEFAULT_JULIE))
    parser.add_argument("--miller-bin", default=str(DEFAULT_MILLER))
    parser.add_argument("--validate", action="store_true")
    parser.add_argument("--skip-miller", action="store_true")
    parser.add_argument("--skip-julie", action="store_true")
    parser.add_argument("--repos", default="all")
    parser.add_argument("--tasks", default="all")
    parser.add_argument("--out-dir", default=str(DEFAULT_RESULTS))
    args = parser.parse_args()

    document, errors = load_cases(Path(args.cases))
    if errors:
        print("manifest validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 2
    rows = document["rows"]
    print(f"{len(rows)} rows valid")
    if args.validate:
        return 0

    repo_filter = split_filter(args.repos)
    task_filter = split_filter(args.tasks)
    selected = [
        row
        for row in rows
        if (not repo_filter or row["repo"] in repo_filter)
        and (not task_filter or row["task_class"] in task_filter or row["id"] in task_filter)
    ]
    if not selected:
        print("filters selected 0 rows", file=sys.stderr)
        return 2

    julie_bin = Path(args.julie_bin)
    miller_bin = Path(args.miller_bin)
    if not args.skip_julie and not julie_bin.is_file():
        print(f"Julie binary not found: {julie_bin}", file=sys.stderr)
        return 2
    if not args.skip_miller and not miller_bin.is_file():
        print(f"Miller binary not found: {miller_bin}", file=sys.stderr)
        return 2

    results = run_matrix(document, selected, julie_bin, miller_bin, args.skip_julie, args.skip_miller)
    summary = summarize(results)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    payload = {
        "timestamp": stamp,
        "julie_bin": str(julie_bin),
        "miller_bin": str(miller_bin),
        "pid": os.getpid(),
        "summary": summary,
        "results": results,
    }
    json_path = out_dir / f"{stamp}.json"
    md_path = out_dir / f"{stamp}.md"
    json_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    md_path.write_text(render_markdown(stamp, summary, results), encoding="utf-8")
    print(json_path)
    print(md_path)
    return 0


if __name__ == "__main__" and "--self-check" in sys.argv:
    bad = {"schema_version": 1, "repos": {}, "rows": [{"id": "x", "repo": "nope", "task_class": "retrieval.symbol",
           "julie": {"tool": "search", "args": {}}, "miller": {"tool": "search", "args": {}},
           "expected": {"path": "missing.rs", "anchor": ""}, "scoring": {"mode": "path_top"}}]}
    errors = validate_manifest(bad)
    assert any("unknown repo" in e for e in errors), errors
    assert any("julie tool" in e for e in errors), errors
    print("self-check ok")
    sys.exit(0)

if __name__ == "__main__":
    raise SystemExit(main())
