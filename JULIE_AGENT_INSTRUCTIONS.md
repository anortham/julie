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
- `manage_workspace`: Index, open, add/remove workspace metadata, list, refresh, stats, and health-check workspaces. For cross-workspace work in daemon mode, call `operation="open"` first, then pass the returned `workspace_id` to search, navigation, and editing tools.
- `edit_file`: Edit a file without reading it first. DMP fuzzy matching for old_text. Always `dry_run=true` first.
- `edit_symbol`: Edit a symbol by name. Operations: replace, insert_after, insert_before. Always `dry_run=true` first.
- `query_metrics`: Code health (security/change risk, test coverage), session stats, trend history.

## Editing Workflow

`edit_file` and `edit_symbol` are the DEFAULT for all file modifications. They edit without reading the file first.
- Code symbols: `deep_dive` > `edit_symbol` (`dry_run=true` first)
- Any text: `edit_file(old_text=..., new_text=..., dry_run=true)`
- Read + Edit is the FALLBACK, not the default. Use only when Julie tools genuinely cannot handle the edit.

## Other Workflows

- **New task**: get_context > deep_dive key symbols > fast_refs > implement
- **Bug fix**: fast_search > deep_dive > write failing test > fix
- **Refactor**: fast_refs > deep_dive > rename_symbol (dry_run first)

Do not use grep/find when Julie tools are available. Do not read files without get_symbols first. Do not chain multiple tools when deep_dive does it in one call.

## Subagent Dispatching

Subagents (Agent tool) do NOT receive Julie's session guidance. When dispatching subagents that will explore or modify code, paste this block into the prompt:

    ## Code Intelligence Tools (use instead of Grep/Glob/Read)
    You have Julie MCP tools. Use them instead of basic Glob/Grep/Read chains:
    - fast_search(query, search_target="definitions") to find code
    - get_symbols(file_path) to see file structure before reading
    - deep_dive(symbol) to understand a symbol before modifying it
    - fast_refs(symbol) to find all references (REQUIRED before any change)
    - edit_file(old_text, new_text, dry_run=true) to edit without reading first
    - edit_symbol(symbol, operation, content, dry_run=true) to edit by name
    Do NOT fall back to Glob/Read/Grep chains. Julie tools return targeted context in 1-2 calls.
