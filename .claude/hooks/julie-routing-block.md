# Julie routing

- `fast_search`: find code by text, symbol name, path fragment, or concept. `file_pattern` and `language` scope it. `backend` lexical, semantic, hybrid; omitted is semantic for prose queries. `regions` filters to `source_regions` kinds.
- `get_symbols`: file structure without reading it. `target` plus `mode="minimal"` extracts one symbol.
- `deep_dive`: one symbol: definition, callers, callees, children, types, `complexity_metrics`.
- `fast_refs`: every reference to a symbol; `reference_kind` filters.
- `call_path`: one shortest call path between two symbols.
- `get_context`: token-budgeted area orientation for a task; give `entry_symbols`, `edited_files`, `stack_trace`, or `failing_test`.
- `blast_radius`: impact of changed files or symbols plus likely tests. With no arguments it reads the working-tree git diff (`git=true`).
- `patterns`: query persisted `structural_facts` (routes, config keys, SQL, document structure). No arguments lists pattern ids.
- `edit_file`: edit without reading first; `old_text` is fuzzy matched. Always `dry_run=true` first.
- `manage_workspace`: index, list, open, remove, refresh, rebuild, health, status, dashboard.

Compact by default. More rows end with a `next:` line; pass `offset` for that page. `return_format="full"` adds code context. Omit `workspace` for this checkout, or pass an id from `manage_workspace(operation="list")`.

## Editing

`edit_file` is the default. Preview with `dry_run=true`, then apply. Read + Edit is the fallback when Julie cannot handle the edit.

## Workflows

- New task: get_context > deep_dive key symbols > fast_refs > implement
- Flow tracing: call_path > deep_dive hops you need
- Change impact: blast_radius > inspect likely callers/tests > implement > rerun
- Extractor dependency changes: re-pin, then `cargo xtask test dev`
- Bug fix: fast_search > deep_dive > write failing test > fix
- Refactor: fast_refs > deep_dive

Do not grep/find when Julie tools exist. Do not read files without get_symbols first. Do not chain tools when deep_dive does it in one call.

## CLI

Named wrappers for quick checks before live MCP: `julie-server call-path FROM TO` (`--from-file` / `--to-file` if names clash); `blast-radius` (git diff) or `--files path`; `patterns --operation search --query route --json`; `search "TODO" --regions comment,doc_comment --json`; `tool call_path --params '{"from":"...","to":"..."}' --json`. Standalone CLI does not prove MCP serving or handler binding. Capture stderr `julie: mode=...` for execution-path evidence.

## External extract

`julie-server extract` writes parser data to a caller-owned SQLite DB. Not MCP, Tantivy, or embeddings. Do not call it through `tool` or `manage_workspace`. For Go/C#/other-runtime hosts. See `docs/EXTERNAL_EXTRACT.md`.
- `extract scan --root DIR --db FILE --json` (`--force` rebuilds)
- `extract update|delete --root DIR --db FILE --file PATH --json`
- `extract analyze|info --db FILE --json`

## Subagents

Subagents do not receive Julie session guidance. Paste:

    ## Code Intelligence Tools (use instead of Grep/Glob/Read)
    You have Julie MCP tools. Use them instead of Glob/Grep/Read:
    - fast_search(query, backend?, regions?, offset?) mixed-kind results. Omit backend: natural-language queries run semantic when vectors are ready, else lexical. backend="lexical"|"semantic"|"hybrid". regions: comment, doc_comment, string_literal, embedded.
    - get_symbols(file_path) before reading
    - deep_dive(symbol) before modifying
    - fast_refs(symbol) before any change
    - call_path(from, to) shortest caller chain
    - get_context(query, entry_symbols?, edited_files?, ...) task context
    - blast_radius(file_paths?, symbol_ids?, git?) impact; no args = git diff
    - patterns(...) structural_facts
    - edit_file(old_text, new_text, dry_run=true)
    Compact by default; next: line pages with offset. Do not fall back to Glob/Read/Grep.
