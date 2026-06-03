#!/usr/bin/env python3
"""
Three-way retrieval bakeoff driver: Julie / Miller / Eros
=========================================================
Run with uv from the eros project directory so eros.eval.* imports resolve:

    cd ~/source/eros
    EROS_JULIE_COMMAND=/tmp/julie-bakeoff/target/release/julie-server \
      uv run python /tmp/julie-bakeoff/scripts/bakeoff/miller_mcp_driver.py \
        --corpus ~/.eros-eval/phase0-corpus/<artifact>.json [--limit 5]

IMPORTANT NOTES:
- Eros column uses backend="sqlite" (the hub's built-in SQLite scorer).
  lancedb is not installed in this environment so lancedb-hybrid-coderank is
  unavailable. Eros-sqlite is the eros baseline used in the eros bakeoff.
- Miller column spawns the Miller.Server native binary as an MCP-stdio
  subprocess per query with cwd=repo.
- Julie column runs julie-server --standalone subprocess per query.
- --standalone julie latency is NOT representative of Julie in daemon mode.
- Scoring uses eros's own _first_matching_rank / _rank_metrics / _result_paths
  functions — NOT reimplemented here.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from collections.abc import Mapping
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Eros imports — must be run via `uv run` inside the eros project
# ---------------------------------------------------------------------------
from eros.client.http import HubHttpClient
from eros.eval.compare import (
    _first_matching_rank,
    _parse_julie_results,
    _rank_metrics,
    _result_paths,
)
from eros.eval.corpus import load_query_corpus

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
MILLER_BINARY = Path(
    os.environ.get(
        "MILLER_BINARY",
        os.path.expanduser(
            "~/source/miller/src/Miller.Server/bin/Release/net10.0/miller"
        ),
    )
)

JULIE_COMMAND = os.environ.get(
    "EROS_JULIE_COMMAND",
    str(Path(__file__).parent.parent.parent / "target" / "release" / "julie-server"),
)

MCP_PROTOCOL_VERSION = "2024-11-05"

# ---------------------------------------------------------------------------
# Miller MCP stdio helpers
# ---------------------------------------------------------------------------

def _miller_rpc_message(obj: dict) -> bytes:
    """Encode a JSON-RPC message as UTF-8 bytes followed by newline."""
    return (json.dumps(obj) + "\n").encode("utf-8")


def _miller_search(
    repo: Path, query: str, limit: int
) -> tuple[list[dict[str, Any]], str | None]:
    """
    Spawn Miller as MCP stdio server with cwd=repo, run initialize then search,
    return (results_list, error_str). Results have 'path' remapped from Miller's
    native 'file' key so _result_paths() can find them.
    """
    if not MILLER_BINARY.exists():
        return [], f"Miller binary not found: {MILLER_BINARY}"

    try:
        proc = subprocess.Popen(
            [str(MILLER_BINARY)],
            cwd=str(repo),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=False,  # binary mode for precise line-by-line reading
        )
    except OSError as exc:
        return [], f"Failed to spawn Miller: {exc}"

    try:
        # 1. Send initialize
        proc.stdin.write(_miller_rpc_message({  # type: ignore[arg-type]
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "bakeoff-runner", "version": "1.0"},
            },
        }))
        proc.stdin.flush()  # type: ignore[union-attr]

        # Read initialize response (one line)
        init_line = proc.stdout.readline()  # type: ignore[union-attr]
        if not init_line:
            return [], "Miller: empty response to initialize"

        # 2. Send initialized notification
        proc.stdin.write(_miller_rpc_message({  # type: ignore[arg-type]
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {},
        }))
        proc.stdin.flush()  # type: ignore[union-attr]

        # 3. Send search tool call
        proc.stdin.write(_miller_rpc_message({  # type: ignore[arg-type]
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "search",
                "arguments": {
                    "query": query,
                    "format": "json",
                    "limit": limit,
                },
            },
        }))
        proc.stdin.flush()  # type: ignore[union-attr]

        # Read search response — Miller may emit notification lines first.
        search_response = None
        deadline = time.monotonic() + 15.0
        while time.monotonic() < deadline:
            line = proc.stdout.readline()  # type: ignore[union-attr]
            if not line:
                break
            try:
                obj = json.loads(line.decode("utf-8", errors="replace"))
            except json.JSONDecodeError:
                continue
            if obj.get("id") == 2:
                search_response = obj
                break

        if search_response is None:
            return [], "Miller: no search response within timeout"

        # Parse result — MCP tool result is in result.content[0].text (JSON array)
        result_obj = search_response.get("result", {})
        content = result_obj.get("content", [])
        if not content:
            return [], None  # No results

        raw_text = content[0].get("text", "[]") if isinstance(content[0], dict) else "[]"
        try:
            raw_results = json.loads(raw_text)
        except (json.JSONDecodeError, TypeError):
            return [], f"Miller: could not parse search result JSON: {raw_text[:200]}"

        if not isinstance(raw_results, list):
            return [], None

        # Remap Miller's "file" key → "path" so _result_paths() can rank it.
        # Miller JSON schema: {name, kind, file, line, signature, score, symbol_id}
        remapped = []
        for item in raw_results:
            if not isinstance(item, dict):
                continue
            new_item = dict(item)
            if "file" in new_item and "path" not in new_item:
                new_item["path"] = new_item["file"]
            remapped.append(new_item)

        return remapped, None

    except Exception as exc:  # noqa: BLE001
        return [], f"Miller: unexpected error: {exc}"
    finally:
        try:
            proc.stdin.close()  # type: ignore[union-attr]
        except Exception:  # noqa: BLE001
            pass
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()


# ---------------------------------------------------------------------------
# Julie subprocess helper
# ---------------------------------------------------------------------------

def _julie_search_target(category: str) -> str:
    cat = category.lower()
    if "symbol" in cat or "exact" in cat or "caller" in cat or "reference" in cat:
        return "definitions"
    return "all"


def _julie_search(
    repo: Path, query: str, limit: int, category: str
) -> tuple[list[dict[str, Any]], str | None]:
    """Run julie-server --standalone and return (results, error)."""
    target = _julie_search_target(category)
    command = [
        JULIE_COMMAND,
        "--workspace", str(repo),
        "--json",
        "--standalone",
        "search",
        "--target", target,
        "--limit", str(limit),
        query,
    ]
    try:
        result = subprocess.run(
            command, capture_output=True, text=True, timeout=45, check=False
        )
    except subprocess.TimeoutExpired:
        return [], "Julie: timed out"
    except OSError as exc:
        return [], f"Julie: {exc}"

    # Use eros's own parser — handles both {results:[...]} and {content:[...]} formats
    parsed = _parse_julie_results(result.stdout)
    return list(parsed), None


# ---------------------------------------------------------------------------
# Eros hub helper — uses sqlite backend (lancedb not installed in this env)
# ---------------------------------------------------------------------------

def _eros_open_workspace(
    client: HubHttpClient, repo: Path
) -> tuple[str | None, str | None]:
    """Add + index a workspace in eros hub. No projection_refresh (avoids lancedb 503)."""
    try:
        add_resp = client.call_tool("workspace", {"operation": "add", "path": str(repo)})
        workspace_id = add_resp["data"]["id"]
        client.call_tool("workspace", {"operation": "index", "workspace_id": workspace_id})
        return workspace_id, None
    except Exception as exc:  # noqa: BLE001
        return None, f"Eros: workspace setup failed: {exc}"


def _eros_search(
    client: HubHttpClient,
    workspace_id: str,
    query: str,
    limit: int,
) -> tuple[list[dict[str, Any]], str | None]:
    """Query eros sqlite backend for an already-indexed workspace.

    NOTE: uses backend="sqlite" explicitly to avoid the lancedb-hybrid-coderank
    409/503 errors. Results live at response["results"], not response["data"]["results"].
    """
    try:
        response = client.call_tool(
            "search_code",
            {
                "workspace_id": workspace_id,
                "query": query,
                "limit": limit,
                "backend": "sqlite",
            },
        )
        # Eros sqlite returns results at top level, not under "data"
        results = response.get("results", [])
        if isinstance(results, list):
            return results, None
        return [], f"Eros: unexpected shape: {str(response)[:200]}"
    except Exception as exc:  # noqa: BLE001
        return [], f"Eros: {exc}"


# ---------------------------------------------------------------------------
# Main bakeoff logic
# ---------------------------------------------------------------------------

def run_bakeoff(corpus_path: Path, limit: int) -> dict[str, Any]:
    print(f"Loading corpus from {corpus_path}", flush=True)
    queries_by_repo = load_query_corpus(corpus_path)
    total_queries = sum(len(qs) for qs in queries_by_repo.values())
    print(
        f"Corpus: {len(queries_by_repo)} repos, {total_queries} queries",
        flush=True,
    )

    client: HubHttpClient | None = None
    eros_available = False
    try:
        client = HubHttpClient()
        health = client.health()
        eros_available = health.get("ready", False)
        print(f"Eros hub: ready={eros_available}", flush=True)
    except Exception as exc:  # noqa: BLE001
        print(f"WARNING: Eros hub not reachable ({exc}). Eros column will be N/A.", flush=True)

    rows: list[dict[str, Any]] = []

    for repo, queries in queries_by_repo.items():
        print(f"\nRepo: {repo} ({len(queries)} queries)", flush=True)

        eros_workspace_id: str | None = None
        if eros_available and client is not None:
            eros_workspace_id, err = _eros_open_workspace(client, repo)
            if err:
                print(f"  [eros setup] {err}", flush=True)

        for query in queries:
            qtext = query.text
            expected = query.expected_paths
            cat = query.category

            # --- Julie ---
            julie_results, julie_err = _julie_search(repo, qtext, limit, cat)
            julie_rank = _first_matching_rank(julie_results, expected)
            if julie_err:
                print(f"  [julie] {qtext!r}: {julie_err}", flush=True)

            # --- Eros ---
            eros_rank: int | None = None
            if eros_available and client is not None and eros_workspace_id is not None:
                eros_results, eros_err = _eros_search(client, eros_workspace_id, qtext, limit)
                eros_rank = _first_matching_rank(eros_results, expected)
                if eros_err:
                    print(f"  [eros] {qtext!r}: {eros_err}", flush=True)

            # --- Miller ---
            miller_results, miller_err = _miller_search(repo, qtext, limit)
            miller_rank = _first_matching_rank(miller_results, expected)
            if miller_err:
                print(f"  [miller] {qtext!r}: {miller_err}", flush=True)

            rows.append({
                "query_id": query.id,
                "query": qtext,
                "category": cat,
                "repo": str(repo),
                "expected_paths": list(expected),
                "julie_rank": julie_rank,
                "eros_rank": eros_rank,
                "miller_rank": miller_rank,
            })
            j5 = julie_rank is not None and julie_rank <= limit
            e5 = eros_rank is not None and eros_rank <= limit
            m5 = miller_rank is not None and miller_rank <= limit
            hit_str = (
                f"julie={'✓' if j5 else '✗'}@{julie_rank or '-'} "
                f"eros={'✓' if e5 else '✗'}@{eros_rank or '-'} "
                f"miller={'✓' if m5 else '✗'}@{miller_rank or '-'}"
            )
            print(f"  {qtext!r:40s}  {hit_str}", flush=True)

    # Aggregate
    julie_ranks = [r["julie_rank"] for r in rows]
    eros_ranks = [r["eros_rank"] for r in rows]
    miller_ranks = [r["miller_rank"] for r in rows]

    overall = {
        "julie": _rank_metrics(julie_ranks),
        "eros": _rank_metrics(eros_ranks),
        "miller": _rank_metrics(miller_ranks),
    }

    categories: dict[str, dict] = {}
    for cat in sorted({r["category"] for r in rows}):
        cat_rows = [r for r in rows if r["category"] == cat]
        categories[cat] = {
            "count": len(cat_rows),
            "julie": _rank_metrics(r["julie_rank"] for r in cat_rows),
            "eros": _rank_metrics(r["eros_rank"] for r in cat_rows),
            "miller": _rank_metrics(r["miller_rank"] for r in cat_rows),
        }

    return {
        "corpus_path": str(corpus_path),
        "limit": limit,
        "total_queries": total_queries,
        "eros_backend_note": (
            "eros-sqlite (backend=sqlite). lancedb-hybrid-coderank unavailable on "
            "this machine (lancedb extra not installed)."
        ),
        "rows": rows,
        "overall": overall,
        "by_category": categories,
    }


def _print_results_table(results: dict[str, Any]) -> None:
    overall = results["overall"]
    print("\n" + "=" * 70)
    print(
        "OVERALL RESULTS (limit=%d, n=%d queries)"
        % (results["limit"], results["total_queries"])
    )
    print("=" * 70)
    print(f"{'System':<12}  {'top5-hits':>9}  {'top5-rate':>9}  {'MRR':>6}")
    print("-" * 45)
    for system in ("eros", "julie", "miller"):
        m = overall[system]
        print(
            f"{system:<12}  {m['top5_hits']:>9}  {m['top5_rate']:>9.3f}  {m['mrr']:>6.3f}"
        )

    print()
    print("BY CATEGORY:")
    print(
        f"{'Category':<32}  {'n':>4}  "
        f"{'J-top5':>6}  {'E-top5':>6}  {'M-top5':>6}  "
        f"{'J-MRR':>5}  {'E-MRR':>5}  {'M-MRR':>5}"
    )
    print("-" * 85)
    for cat, cat_data in sorted(results["by_category"].items()):
        j = cat_data["julie"]
        e = cat_data["eros"]
        m = cat_data["miller"]
        print(
            f"{cat:<32}  {cat_data['count']:>4}  "
            f"{j['top5_rate']:>6.2f}  {e['top5_rate']:>6.2f}  {m['top5_rate']:>6.2f}  "
            f"{j['mrr']:>5.3f}  {e['mrr']:>5.3f}  {m['mrr']:>5.3f}"
        )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Three-way retrieval bakeoff: Julie / Miller / Eros"
    )
    parser.add_argument("--corpus", required=True, help="Path to corpus JSON artifact")
    parser.add_argument(
        "--limit", type=int, default=5, help="Top-N results per query (default 5)"
    )
    parser.add_argument("--output-json", help="Write full results JSON to this path")
    args = parser.parse_args()

    corpus_path = Path(args.corpus).expanduser()
    if not corpus_path.exists():
        print(f"ERROR: corpus not found: {corpus_path}", file=sys.stderr)
        sys.exit(1)

    results = run_bakeoff(corpus_path, args.limit)
    _print_results_table(results)

    if args.output_json:
        output_path = Path(args.output_json).expanduser()
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(json.dumps(results, indent=2), encoding="utf-8")
        print(f"\nFull results written to: {output_path}")


if __name__ == "__main__":
    main()
