# Julie

## Rules

1. `fast_search` before writing new code.
2. `deep_dive` before modifying a symbol; it replaces search, symbols, refs, and Read.
3. Trust results; do not re-verify with grep, find, or Read.

## Tools

- `fast_search`: text, symbol, path, or concept search. `file_pattern` and `language` scope it; `regions` filters `source_regions`; `backend` selects lexical, semantic, or hybrid.
- `get_symbols`: file structure. `target` plus `mode="minimal"` extracts one symbol.
- `deep_dive`: one symbol: definition, callers, callees, children, types, `complexity_metrics`.
- `fast_refs`: references to a symbol; `reference_kind` filters.
- `call_path`: shortest call path between two symbols.
- `get_context`: token-budgeted task orientation from symbols, files, or a failing test.
- `blast_radius`: impact of changed files or symbols plus likely tests. No args reads the git diff.
- `patterns`: query persisted `structural_facts` (routes, config keys, SQL). No args lists pattern ids.
- `edit_file`: edit without reading; `old_text` is fuzzy matched. `dry_run=true` first.
- `manage_workspace`: index, list, open, remove, refresh, rebuild, health, status, dashboard.

## Workflows

- New task: `get_context` > `deep_dive` key symbols > `fast_refs` > implement
- Flow tracing: `call_path` > `deep_dive` needed hops
- Change impact: `blast_radius` > check callers and tests > implement > rerun
- Bug fix: `fast_search` > `deep_dive` > write a failing test > fix
- Refactor: `fast_refs` > `deep_dive`

Use Julie instead of grep, find, or Read. Run `get_symbols` before whole-file reads.

Search, navigation, and editing calls require `workspace`: an absolute path or registered ID. Register with `manage_workspace(operation="open", path="/absolute/project")`; other manage calls use `workspace_id`. Global `list` and `status` need none.

Replay `next: <tool> <JSON>` continuations. `offset` is best effort after edits. Body pages bind `source_hash`; restart with `body_offset=0` on mismatch.
