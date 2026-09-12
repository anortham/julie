# Julie

Scoped calls require `workspace` as an absolute path or registered ID. Open the intended checkout with `manage_workspace(operation="open", path="/absolute/project")`. Start with `fast_search` or `get_context`, then inspect evidence before editing. Search, navigation, and editing calls require `workspace`.

## Rules

1. `fast_search` before writing new code; use `backend="lexical"` for prose, literals, paths, and exact comparisons.
2. `deep_dive` before modifying a symbol; use `get_symbols` before reading a whole file.
3. Use Julie evidence first. Fall back to direct inspection only when the result is unavailable, stale, truncated, or insufficient; then narrow the fallback and return to Julie evidence.

## Tools

- `fast_search`: text, symbol, path, or concept search. `file_pattern`, `language`, and `backend` narrow it; `regions` filters stored `source_regions`.
- `get_symbols`: file structure. `target` plus `mode="minimal"` extracts one symbol.
- `deep_dive`: definition, callers, callees, children, types, and `complexity_metrics`.
- `fast_refs`: references to a symbol. `call_path`: shortest route between symbols.
- `get_context`: token-budgeted orientation. `blast_radius`: changed-file or symbol impact and likely tests.
- `patterns`: query `structural_facts`. `edit_file`: safe edit; use `dry_run=true` first.
- `manage_workspace`: `open`, `index`, `refresh`, `rebuild`, `remove`, `list`, `status`, `health`, `recover_edit`, `dashboard`.

## Workflows

- New task: `get_context` > `deep_dive` key symbols > `fast_refs` > implement.
- Bug: `fast_search` > `deep_dive` > failing test > fix. Refactor: `fast_refs` > `deep_dive`.
- Management: `open` accepts `path` or `workspace_id`; `health`, `refresh`, and `remove` require `workspace_id`. Global `list` and `status` need no selector.

Replay `next: <tool> <JSON>` continuations. Preserve `offset` and `source_hash` paging arguments; restart body paging at `body_offset=0` if the source changed.
