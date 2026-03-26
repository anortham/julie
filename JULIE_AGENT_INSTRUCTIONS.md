# Julie - Code Intelligence Server

## Rules

1. **Search before coding**: Always `fast_search` before writing new code.
2. **Structure before reading**: Always `get_symbols` before Read (70-90% token savings).
3. **References before changes**: Always `fast_refs` before modifying any symbol.
4. **Deep dive before modifying**: Use `deep_dive` before changing a symbol. One call replaces chaining fast_search + get_symbols + fast_refs + Read.
5. **Trust results**: Pre-indexed and accurate. Never verify with grep/find/Read.

## Tools

- `fast_search`: Find code by text. `search_target="definitions"` promotes exact symbol matches.
- `get_symbols`: File structure without reading full content. Use `target` + `mode="minimal"` to extract one symbol.
- `deep_dive`: Investigate a symbol: definition, callers, callees, children, types. Always use before modifying.
- `fast_refs`: All references to a symbol. Required before any change. Use `reference_kind` to filter.
- `get_context`: Token-budgeted area orientation (pivots + neighbors). Use at start of task.
- `rename_symbol`: Workspace-wide rename. Always preview with `dry_run=true` first.
- `manage_workspace`: Index, add/remove references, health check. First action in new workspace: `operation="index"`.
- `query_metrics`: Code health (security/change risk, test coverage), session stats, trend history.

## Workflow

- **New task**: get_context > deep_dive key symbols > fast_refs > implement
- **Bug fix**: fast_search > deep_dive > write failing test > fix
- **Refactor**: fast_refs > deep_dive > rename_symbol (dry_run first)

Don't use grep/find when Julie tools are available. Don't read files without get_symbols first. Don't chain multiple tools when deep_dive does it in one call.
