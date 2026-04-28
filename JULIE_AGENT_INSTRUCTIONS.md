# Julie - Code Intelligence Server

## Rules

1. **Search before coding**: Always `fast_search` before writing new code.
2. **Structure before reading**: Always `get_symbols` before Read (70-90% token savings).
3. **References before changes**: Always `fast_refs` before modifying any symbol.
4. **Deep dive before modifying**: Use `deep_dive` before changing a symbol. One call replaces chaining fast_search + get_symbols + fast_refs + Read.
5. **Trust results**: Pre-indexed and accurate. Never verify with grep/find/Read.

## Tools

- `fast_search`: Find code by text. `search_target="definitions"` promotes exact symbol matches. `file_pattern` scopes searches to matching paths, such as `src/**/*.rs`, `tests/**`, or a specific file. For symbol structure within a specific file, prefer `get_symbols(file_path=...)` over `file_pattern`.
- `get_symbols`: File structure without reading full content. Use `target` + `mode="minimal"` to extract one symbol.
- `deep_dive`: Investigate a symbol: definition, callers, callees, children, types. Always use before modifying.
- `fast_refs`: All references to a symbol. Required before any change. Use `reference_kind` to filter.
- `call_path`: One shortest call-graph path between two symbols. Use it for "how does A reach B?" or "what caller chain connects these symbols?" questions. Traverses calls, instantiations, and overrides only. Use `from_file_path` / `to_file_path` when names are ambiguous.
- `get_context`: Token-budgeted area orientation (pivots + neighbors). Supports task inputs like `edited_files`, `entry_symbols`, `stack_trace`, `failing_test`, `max_hops`, and `prefer_tests`.
- `blast_radius`: Deterministic impact analysis for changed files, internal symbol IDs, or revision ranges. Returns impacts ranked by centrality and hops plus linked tests. Use before refactoring or after a change. Prefer `file_paths` when you know a symbol name or file path; `symbol_ids` are internal Julie IDs, not names like `AuthService::validate`.
- `spillover_get`: Fetch the next page for large `get_context` or `blast_radius` result sets when a spillover handle is returned.
- `rename_symbol`: Workspace-wide rename. Always preview with `dry_run=true` first.
- `manage_workspace`: Index, open, register/remove workspace metadata, list, refresh, stats, and health-check workspaces. For cross-workspace work in daemon mode, call `operation="open"` first, then pass the returned `workspace_id` to search, navigation, and editing tools.
- `edit_file`: Edit a file without reading it first. DMP fuzzy matching for old_text. Always `dry_run=true` first.
- `rewrite_symbol`: Rewrite a symbol by name. Operations: replace_full, replace_body, replace_signature, insert_after, insert_before, add_doc. Always `dry_run=true` first.

## Editing Workflow

`edit_file` and `rewrite_symbol` are the DEFAULT for file modifications. They edit without reading the file first.
- Code symbols: `deep_dive` > `rewrite_symbol` (`dry_run=true` first)
- Any text: `edit_file(old_text=..., new_text=..., dry_run=true)`
- Read + Edit is the FALLBACK, not the default. Use only when Julie tools genuinely cannot handle the edit.

## Other Workflows

- **New task**: get_context > deep_dive key symbols > fast_refs > implement
- **Flow tracing**: call_path > deep_dive the hops you need to understand in detail
- **Change impact**: blast_radius > inspect likely callers/tests > implement > rerun blast_radius if needed
- **Bug fix**: fast_search > deep_dive > write failing test > fix
- **Refactor**: fast_refs > deep_dive > rename_symbol (dry_run first)

## CLI Dogfooding

Use named CLI wrappers when testing tool behavior from a shell:

- Flow tracing: `julie-server call-path "LoginButton::onClick" "insert_session" --standalone`
- Ambiguous symbols: `julie-server call-path handle_request write_response --from-file src/server.rs --to-file src/response.rs --standalone`
- Impact checks: `julie-server blast-radius --files src/auth/login_flow.rs --standalone`
- Generic fallback remains available for raw MCP parameters: `julie-server tool call_path --params '{"from":"handle_request","to":"write_response"}' --standalone`

Do not use grep/find when Julie tools are available. Do not read files without get_symbols first. Do not chain multiple tools when deep_dive does it in one call.

## Subagent Dispatching

Subagents (Agent tool) do NOT receive Julie's session guidance. When dispatching subagents that will explore or modify code, paste this block into the prompt:

    ## Code Intelligence Tools (use instead of Grep/Glob/Read)
    You have Julie MCP tools. Use them instead of basic Glob/Grep/Read chains:
    - fast_search(query, search_target="definitions") to find code. file_pattern scopes text/path searches; for symbol structure in one file, use get_symbols(file_path=...)
    - get_symbols(file_path) to see file structure before reading
    - deep_dive(symbol) to understand a symbol before modifying it
    - fast_refs(symbol) to find all references (REQUIRED before any change)
    - call_path(from, to, from_file_path?, to_file_path?, max_hops?) to trace one shortest caller chain between symbols
    - get_context(query, edited_files?, entry_symbols?, stack_trace?, failing_test?, max_hops?, prefer_tests?) for task-shaped context
    - blast_radius(file_paths?, symbol_ids?, from_revision?, to_revision?, max_depth?, include_tests?) for likely impact and linked tests. Prefer file_paths for human-facing symbol or file work; symbol_ids are internal Julie IDs returned by search/navigation tools, not names like AuthService::validate
    - spillover_get(spillover_handle) to continue a large paged result
    - edit_file(old_text, new_text, dry_run=true) to edit without reading first
    - rewrite_symbol(symbol, operation, content, dry_run=true) to edit by name
    Do NOT fall back to Glob/Read/Grep chains. Julie tools return targeted context in 1-2 calls.
