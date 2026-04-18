---
name: editing
description: >-
  Use before making changes to existing files. Routes to Julie's edit_file,
  rewrite_symbol, and rename_symbol tools without a Read + Edit loop. Trigger
  on fix, change, update, modify, refactor, rename, replace, add, remove,
  move, or any task involving an existing file.
allowed-tools: mcp__julie__edit_file, mcp__julie__rewrite_symbol, mcp__julie__rename_symbol, mcp__julie__get_symbols, mcp__julie__deep_dive, mcp__julie__fast_search, mcp__julie__fast_refs
---

# Editing Files with Julie

Julie's edit tools change existing files without reading them first. This is
the default path for file modifications.

## Which tool do I use?

- **Creating a new file?** Use the Write tool. This skill doesn't apply.
- **Renaming a symbol across the workspace?** Use `rename_symbol`, the semantic path. It understands scope, updates references, and surfaces conflicts.
- **Changing one symbol (function, struct, class, method)?** Use `deep_dive` to understand it, then `rewrite_symbol` to change it.
- **Changing arbitrary text in a file?** Use `edit_file` with `old_text` and `new_text`.
- **Need to understand the file first?** Use `get_symbols` for structure or `deep_dive` for full context, then use `rewrite_symbol`. Not Read.

## Stop and check

If you catch yourself thinking any of these, you're about to waste tokens:

| Thought | What to do instead |
|---------|-------------------|
| "I need to read the file first" | No. `edit_file` uses DMP fuzzy matching on `old_text`. `rewrite_symbol` resolves a symbol by name. Neither needs Read. |
| "It's a quick change" | `edit_file` handles that well. `edit_file(old_text=..., new_text=..., dry_run=true)` is the fast path. |
| "I'm not sure which symbol I need" | Use `fast_search`, `get_symbols`, or `deep_dive`, then use `rewrite_symbol`. Still no Read + Edit loop. |
| "This isn't a code file" | `edit_file` works on ANY text file: YAML, TOML, Markdown, .gitignore, configs, everything. |
| "The symbol edit is complex" | Use `dry_run=true` first. If the symbol shape or operation does not fit, fall back to `edit_file`. |

## Workflow

1. **Always preview first**: `dry_run=true` (the default). Review the diff.
2. **Then apply**: same call with `dry_run=false`.

### rewrite_symbol (for code symbols)

- `replace_full` swaps an entire symbol definition
- `replace_body` rewrites only the body
- `replace_signature` rewrites only the signature
- `insert_before` adds content before the symbol
- `insert_after` adds content after the symbol
- `add_doc` adds a doc block ahead of the symbol
- `file_path` narrows the match when multiple symbols share a name

Use `deep_dive` before rewriting a symbol. Use `fast_refs` first when the change may affect many callers.

### rename_symbol (for semantic workspace-wide renames)

Renames a symbol across the workspace and updates all call sites. Prefer this over `edit_file` for renaming functions, structs, classes, methods, or variables.

```
rename_symbol(old_name="getUserData", new_name="fetchUser", dry_run=true)
```

Use `scope` to narrow the rename when multiple symbols share a name. Valid values: `"workspace"` (default), `"all"`, or `"file:<path>"` (e.g., `scope="file:src/tools/foo.rs"`). Preview with `dry_run=true`, then apply with `dry_run=false`. Reach for `edit_file` only for text-replacement renames (e.g., renaming a config key, a markdown heading).

### edit_file (for any text)

- `old_text`: text to find (DMP fuzzy matched)
- `new_text`: replacement text
- `occurrence`: `"first"` (default), `"last"`, or `"all"`

## Example: the cost of Read + Edit

Changing a version number in Cargo.toml:

**Read + Edit pattern (5 calls, ~800 tokens):**
1. Edit -- fails ("File has not been read yet")
2. Read Cargo.toml -- waste
3. Grep for the version line -- unnecessary
4. Read again with offset -- more waste
5. Edit -- finally works

**edit_file pattern (2 calls, ~200 tokens):**
1. `edit_file(file_path="Cargo.toml", old_text='version = "6.6.2"', new_text='version = "6.6.3"', dry_run=true)` -- preview
2. Same call with `dry_run=false` -- done

4x fewer tokens. 3 fewer round trips.
