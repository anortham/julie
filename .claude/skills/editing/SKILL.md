---
name: editing
description: Use when editing code files to save tokens. Guides usage of edit_file and edit_symbol tools which don't require reading files first.
allowed-tools: mcp__julie__edit_file, mcp__julie__edit_symbol, mcp__julie__get_symbols, mcp__julie__deep_dive, mcp__julie__fast_search
---

# Token-Efficient Editing Workflow

Use Julie's editing tools to modify files without the Read + Edit cycle.

## Workflow

1. **Understand the target** using `get_symbols` or `deep_dive`
2. **For code changes by symbol name**: use `edit_symbol`
   - `operation: "replace"` to swap an entire definition
   - `operation: "insert_after"` to add code after a symbol
   - `operation: "insert_before"` to add code before a symbol
3. **For arbitrary text changes**: use `edit_file`
   - Provide `old_text` (what to find) and `new_text` (replacement)
   - DMP fuzzy matching tolerates minor whitespace differences
   - Use `occurrence: "all"` to replace every match
4. **Always preview first**: `dry_run=true` (the default), review the diff, then `dry_run=false`
5. **Fall back to Read + Edit** only if Julie's tools can't handle the case (e.g., creating a new file)
