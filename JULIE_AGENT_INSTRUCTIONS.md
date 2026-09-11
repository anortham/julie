# Julie code intelligence

## Rules

1. `fast_search` before writing new code.
2. `deep_dive` before modifying a symbol; one call replaces search, symbols, refs, and Read.
3. Trust results; do not re-verify with grep, find, or Read.

## Tools

- `fast_search`: text, symbol name, path fragment, or concept search. `file_pattern` and `language` scope it. `backend` lexical, semantic, hybrid; omitted picks semantic for prose. `regions` filters to `source_regions` kinds.
- `get_symbols`: file structure without reading it. `target` plus `mode="minimal"` extracts one symbol.
- `deep_dive`: one symbol: definition, callers, callees, children, types, `complexity_metrics`.
- `fast_refs`: every reference to a symbol; `reference_kind` filters.
- `call_path`: one shortest call path between two symbols.
- `get_context`: token-budgeted task orientation; give `entry_symbols`, `edited_files`, `stack_trace`, or `failing_test`.
- `blast_radius`: impact of changed files or symbols plus likely tests. No arguments reads the git diff.
- `patterns`: query persisted `structural_facts` (routes, config keys, SQL). No arguments lists pattern ids.
- `edit_file`: edit without reading; `old_text` is fuzzy matched. `dry_run=true` first.
- `manage_workspace`: index, list, open, remove, refresh, rebuild, health, status, dashboard.

## Workflows

- New task: `get_context` > `deep_dive` key symbols > `fast_refs` > implement
- Flow tracing: `call_path` > `deep_dive` needed hops
- Change impact: `blast_radius` > check callers and tests > implement > rerun
- Bug fix: `fast_search` > `deep_dive` > write a failing test > fix
- Refactor: `fast_refs` > `deep_dive`

Do not grep or find when Julie tools exist. Do not read a file without `get_symbols` first.

Output is compact. More rows end with a `next:` line; pass `offset` for that page. `return_format="full"` adds code context. Omit `workspace` for this checkout, or pass an id from `manage_workspace(operation="list")`.
