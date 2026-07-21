#!/usr/bin/env python3
"""Read a Julie index and print a dead-code candidate inventory."""

from __future__ import annotations

import argparse
import json
import sqlite3
import sys
from pathlib import Path
from typing import Any


AUDIT_KINDS = (
    "function",
    "method",
    "struct",
    "enum",
    "trait",
    "class",
    "interface",
    "type",
    "module",
)

DATA_LANGUAGES = (
    "css",
    "html",
    "json",
    "markdown",
    "toml",
    "yaml",
)


def path_test_expr(path_expr: str) -> str:
    return f"""(
        {path_expr} LIKE '%/test/%'
        OR {path_expr} LIKE '%/tests/%'
        OR {path_expr} LIKE 'test/%'
        OR {path_expr} LIKE 'tests/%'
        OR {path_expr} LIKE '%/spec/%'
        OR {path_expr} LIKE '%/specs/%'
        OR {path_expr} LIKE 'spec/%'
        OR {path_expr} LIKE 'specs/%'
        OR {path_expr} LIKE '%/__tests__/%'
        OR {path_expr} LIKE '__tests__/%'
        OR {path_expr} LIKE '%.Tests/%'
        OR {path_expr} LIKE '%.Test/%'
        OR {path_expr} LIKE '%/test\\_%' ESCAPE '\\'
        OR {path_expr} LIKE 'test\\_%' ESCAPE '\\'
        OR {path_expr} LIKE '%\\_test.go' ESCAPE '\\'
        OR {path_expr} LIKE '%\\_test.c' ESCAPE '\\'
        OR {path_expr} LIKE '%\\_test.cc' ESCAPE '\\'
        OR {path_expr} LIKE '%\\_test.cpp' ESCAPE '\\'
        OR {path_expr} LIKE '%.test.ts'
        OR {path_expr} LIKE '%.test.tsx'
        OR {path_expr} LIKE '%.test.js'
        OR {path_expr} LIKE '%.test.jsx'
        OR {path_expr} LIKE '%.spec.ts'
        OR {path_expr} LIKE '%.spec.tsx'
        OR {path_expr} LIKE '%.spec.js'
        OR {path_expr} LIKE '%.spec.jsx'
    )"""


def non_source_filter(path_expr: str, include_fixtures: bool) -> str:
    if include_fixtures:
        return "1 = 1"
    return f"""(
        {path_expr} NOT LIKE 'fixtures/%'
        AND {path_expr} NOT LIKE '%/fixtures/%'
        AND {path_expr} NOT LIKE 'examples/%'
        AND {path_expr} NOT LIKE '%/examples/%'
        AND {path_expr} NOT LIKE 'benchmarks/%'
        AND {path_expr} NOT LIKE '%/benchmarks/%'
        AND {path_expr} NOT LIKE 'benches/%'
        AND {path_expr} NOT LIKE '%/benches/%'
        AND {path_expr} NOT LIKE 'docs/%'
        AND {path_expr} NOT LIKE '%/docs/%'
    )"""


def resolve_db_path(args: argparse.Namespace) -> Path:
    if args.db:
        return Path(args.db).expanduser()
    if not args.workspace_id:
        raise SystemExit("pass --db or --workspace-id")

    daemon_db = Path.home() / ".julie" / "indexes" / args.workspace_id / "db" / "symbols.db"
    if daemon_db.exists():
        return daemon_db

    local_db = Path.cwd() / ".julie" / "indexes" / args.workspace_id / "db" / "symbols.db"
    return local_db


def connect_read_only(db_path: Path) -> sqlite3.Connection:
    if not db_path.exists():
        raise SystemExit(f"index db not found: {db_path}")
    uri = db_path.resolve().as_uri() + "?mode=ro"
    conn = sqlite3.connect(uri, uri=True)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA query_only = ON")
    return conn


def query_rows(conn: sqlite3.Connection, sql: str, params: tuple[Any, ...]) -> list[dict[str, Any]]:
    return [dict(row) for row in conn.execute(sql, params)]


def base_ctes(include_fixtures: bool, include_data_languages: bool) -> str:
    kinds = ",".join(f"'{kind}'" for kind in AUDIT_KINDS)
    data_languages = ",".join(f"'{language}'" for language in DATA_LANGUAGES)
    source_filter = non_source_filter("s.file_path", include_fixtures)
    import_source_filter = non_source_filter("imp.file_path", include_fixtures)
    symbol_test_expr = path_test_expr("s.file_path")
    import_test_expr = path_test_expr("imp.file_path")
    ident_test_expr = path_test_expr("i.file_path")
    language_filter = "1 = 1" if include_data_languages else f"s.language NOT IN ({data_languages})"
    import_language_filter = "1 = 1" if include_data_languages else f"imp.language NOT IN ({data_languages})"
    return f"""
    WITH symbol_flags AS (
        SELECT
            s.*,
            CASE
                WHEN (
                    CASE
                        WHEN json_valid(COALESCE(s.metadata, 'null'))
                        THEN COALESCE(json_extract(s.metadata, '$.is_test'), 0)
                        ELSE 0
                    END
                ) = 1
                OR (
                    CASE
                        WHEN json_valid(COALESCE(s.metadata, 'null'))
                        THEN json_type(s.metadata, '$.test_role') IS NOT NULL
                        ELSE 0
                    END
                )
                OR {symbol_test_expr}
                THEN 1
                ELSE 0
            END AS is_testish,
            CASE
                WHEN COALESCE(s.code_context, '') LIKE '%cfg(test)%'
                THEN 1
                ELSE 0
            END AS has_cfg_test_marker
        FROM symbols s
        WHERE s.content_type IS NULL
          AND s.kind IN ({kinds})
          AND {language_filter}
          AND {source_filter}
    ),
    incoming AS (
        SELECT
            r.to_symbol_id AS id,
            SUM(CASE WHEN COALESCE(src.is_testish, 0) = 0 THEN 1 ELSE 0 END) AS prod_rel_refs,
            SUM(CASE WHEN COALESCE(src.is_testish, 0) = 1 THEN 1 ELSE 0 END) AS test_rel_refs,
            COUNT(*) AS total_rel_refs
        FROM relationships r
        LEFT JOIN symbol_flags src ON src.id = r.from_symbol_id
        GROUP BY r.to_symbol_id
    ),
    import_hits AS (
        SELECT
            target.id AS id,
            SUM(CASE WHEN {import_test_expr} THEN 0 ELSE 1 END) AS prod_import_hits,
            SUM(CASE WHEN {import_test_expr} THEN 1 ELSE 0 END) AS test_import_hits
        FROM symbol_flags target
        JOIN symbols imp ON imp.kind = 'import' AND imp.name = target.name
        WHERE imp.content_type IS NULL
          AND {import_language_filter}
          AND {import_source_filter}
          AND NOT (imp.file_path = target.file_path AND imp.start_line = target.start_line)
        GROUP BY target.id
    ),
    ident_hits AS (
        SELECT
            target.id AS id,
            SUM(
                CASE
                    WHEN COALESCE(container.is_testish, CASE WHEN {ident_test_expr} THEN 1 ELSE 0 END) = 0
                    THEN 1
                    ELSE 0
                END
            ) AS prod_name_hits,
            SUM(
                CASE
                    WHEN COALESCE(container.is_testish, CASE WHEN {ident_test_expr} THEN 1 ELSE 0 END) = 1
                    THEN 1
                    ELSE 0
                END
            ) AS test_name_hits
        FROM symbol_flags target
        JOIN identifiers i ON i.name = target.name
        LEFT JOIN symbol_flags container ON container.id = i.containing_symbol_id
        WHERE NOT (i.file_path = target.file_path AND i.start_line = target.start_line)
        GROUP BY target.id
    ),
    candidate_base AS (
        SELECT
            s.id,
            s.name,
            s.kind,
            COALESCE(s.visibility, '') AS visibility,
            s.language,
            s.file_path,
            s.start_line,
            s.signature,
            ROUND(s.reference_score, 2) AS reference_score,
            s.has_cfg_test_marker,
            COALESCE(i.prod_rel_refs, 0) AS prod_rel_refs,
            COALESCE(i.test_rel_refs, 0) AS test_rel_refs,
            COALESCE(i.total_rel_refs, 0) AS total_rel_refs,
            COALESCE(im.prod_import_hits, 0) AS prod_import_hits,
            COALESCE(im.test_import_hits, 0) AS test_import_hits,
            COALESCE(h.prod_name_hits, 0) AS prod_name_hits,
            COALESCE(h.test_name_hits, 0) AS test_name_hits
        FROM symbol_flags s
        LEFT JOIN incoming i ON i.id = s.id
        LEFT JOIN import_hits im ON im.id = s.id
        LEFT JOIN ident_hits h ON h.id = s.id
        WHERE s.is_testish = 0
    )
    """


def candidate_query(include_fixtures: bool, include_data_languages: bool) -> str:
    return (
        base_ctes(include_fixtures, include_data_languages)
        + """
        SELECT
            id,
            name,
            kind,
            visibility,
            language,
            file_path,
            start_line,
            signature,
            reference_score,
            prod_rel_refs,
            test_rel_refs,
            prod_import_hits,
            test_import_hits,
            prod_name_hits,
            test_name_hits,
            has_cfg_test_marker
        FROM candidate_base
        """
    )


def limited_section(
    rows: list[dict[str, Any]],
    predicate: Any,
    sort_key: Any,
    limit: int,
) -> dict[str, Any]:
    matches = [row for row in rows if predicate(row)]
    matches.sort(key=sort_key)
    return {"total": len(matches), "rows": matches[:limit]}


def build_report(
    conn: sqlite3.Connection,
    limit: int,
    include_fixtures: bool,
    include_data_languages: bool,
) -> dict[str, Any]:
    rows = query_rows(conn, candidate_query(include_fixtures, include_data_languages), ())
    sections = {
        "test_only_relationship_refs": limited_section(
            rows,
            lambda row: row["prod_rel_refs"] == 0
            and row["prod_import_hits"] == 0
            and row["test_rel_refs"] > 0,
            lambda row: (-row["test_rel_refs"], row["prod_name_hits"], row["file_path"], row["start_line"]),
            limit,
        ),
        "zero_relationship_refs_no_prod_identifier_hits": limited_section(
            rows,
            lambda row: row["prod_rel_refs"] == 0
            and row["prod_import_hits"] == 0
            and row["test_rel_refs"] == 0
            and row["test_import_hits"] == 0
            and row["prod_name_hits"] == 0,
            lambda row: (row["reference_score"], row["file_path"], row["start_line"]),
            limit,
        ),
        "likely_graph_gaps": limited_section(
            rows,
            lambda row: row["prod_rel_refs"] == 0
            and (row["prod_import_hits"] > 0 or row["prod_name_hits"] > 0),
            lambda row: (
                -row["prod_import_hits"],
                -row["prod_name_hits"],
                -row["test_rel_refs"],
                row["file_path"],
                row["start_line"],
            ),
            limit,
        ),
        "cfg_test_markers_in_source_paths": limited_section(
            rows,
            lambda row: row["has_cfg_test_marker"] == 1,
            lambda row: (row["file_path"], row["start_line"]),
            limit,
        ),
    }
    summary = {name: section["total"] for name, section in sections.items()}
    summary["symbols"] = conn.execute("SELECT COUNT(*) FROM symbols").fetchone()[0]
    summary["relationships"] = conn.execute("SELECT COUNT(*) FROM relationships").fetchone()[0]
    summary["identifiers"] = conn.execute("SELECT COUNT(*) FROM identifiers").fetchone()[0]
    return {"summary": summary, "sections": sections}


def render_table(rows: list[dict[str, Any]]) -> str:
    if not rows:
        return "_No rows._\n"
    lines = []
    for row in rows:
        loc = f"{row['file_path']}:{row['start_line']}"
        flags = []
        if row["prod_name_hits"]:
            flags.append(f"prod_name_hits={row['prod_name_hits']}")
        if row["test_name_hits"]:
            flags.append(f"test_name_hits={row['test_name_hits']}")
        if row["prod_import_hits"]:
            flags.append(f"prod_import_hits={row['prod_import_hits']}")
        if row["test_import_hits"]:
            flags.append(f"test_import_hits={row['test_import_hits']}")
        if row["has_cfg_test_marker"]:
            flags.append("cfg_test_marker")
        flag_text = f" ({', '.join(flags)})" if flags else ""
        lines.append(
            f"- `{row['name']}` {row['kind']} {row['visibility']} at `{loc}`: "
            f"prod_rel={row['prod_rel_refs']}, test_rel={row['test_rel_refs']}, "
            f"score={row['reference_score']}{flag_text}"
        )
    return "\n".join(lines) + "\n"


def render_markdown(db_path: Path, report: dict[str, Any]) -> str:
    section_titles = {
        "test_only_relationship_refs": "Test-Only Relationship Refs",
        "zero_relationship_refs_no_prod_identifier_hits": "Zero Relationship Refs And No Production Identifier Hits",
        "likely_graph_gaps": "Likely Graph Gaps",
        "cfg_test_markers_in_source_paths": "Cfg-Test Markers In Source Paths",
    }
    out = [f"# Dead-Code Inventory\n\nIndex: `{db_path}`\n"]
    out.append("## Summary\n")
    for key, value in report["summary"].items():
        out.append(f"- {key}: {value}")
    out.append("\n## Sections\n")
    for key, section in report["sections"].items():
        out.append(f"### {section_titles[key]}\n")
        out.append(f"Matched rows: {section['total']}\n")
        out.append(render_table(section["rows"]))
    out.append(
        "\nTreat this as candidate inventory. Confirm each target with "
        "`fast_refs`, `deep_dive`, and `blast_radius` before editing.\n"
    )
    return "\n".join(out)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build a Julie dead-code candidate inventory.")
    parser.add_argument("--db", help="Path to symbols.db")
    parser.add_argument("--workspace-id", help="Julie workspace id, for example julie_528d4264")
    parser.add_argument("--limit", type=int, default=25, help="Rows per section")
    parser.add_argument("--include-fixtures", action="store_true", help="Include fixtures, docs, and examples")
    parser.add_argument("--include-data-languages", action="store_true", help="Include JSON, TOML, YAML, HTML, CSS, and Markdown")
    parser.add_argument("--json", action="store_true", help="Emit JSON")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    db_path = resolve_db_path(args)
    conn = connect_read_only(db_path)
    report = build_report(conn, args.limit, args.include_fixtures, args.include_data_languages)
    if args.json:
        print(json.dumps({"db": str(db_path), **report}, indent=2))
    else:
        print(render_markdown(db_path, report))
    return 0


if __name__ == "__main__":
    sys.exit(main())
