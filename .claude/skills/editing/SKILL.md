---
name: editing
description: >-
  Use BEFORE making any code or file changes -- whenever you're about to use
  Read+Edit, sed, or any modify-then-write pattern. Routes to Julie's edit_file
  and edit_symbol tools which edit files directly without reading them first.
  Trigger on: fix, change, update, modify, refactor, rename, replace, add,
  remove, move, or any task involving changes to existing files. Even one-line
  changes. Even non-code files.
allowed-tools: mcp__julie__edit_file, mcp__julie__edit_symbol, mcp__julie__get_symbols, mcp__julie__deep_dive, mcp__julie__fast_search
---

# Editing Files with Julie

Julie's edit tools modify files without reading them first. This is the default
path for all file modifications.

## Which tool do I use?

- **Creating a new file?** Use the Write tool. This skill doesn't apply.
- **Changing a symbol (function, struct, class, method)?** Use `deep_dive` to understand it, then `edit_symbol` to change it.
- **Changing arbitrary text in a file?** Use `edit_file` with `old_text` and `new_text`.
- **Need to understand the file first?** Use `get_symbols` (structure) or `deep_dive` (full context), then use `edit_symbol`. Not Read.

## Stop and check

If you catch yourself thinking any of these, you're about to waste tokens:

| Thought | What to do instead |
|---------|-------------------|
| "I need to read the file first" | No. `edit_file` uses DMP fuzzy matching on `old_text`. `edit_symbol` finds symbols by name. Neither needs a Read. |
| "It's just a quick change" | Quick changes are `edit_file`'s sweet spot. `edit_file(old_text=..., new_text=..., dry_run=true)` -- done. |
| "I'm not sure of the exact text to match" | Use `get_symbols` or `deep_dive` to see the code, then `edit_symbol` to change it. Still no Read+Edit. |
| "This isn't a code file" | `edit_file` works on ANY text file: YAML, TOML, Markdown, .gitignore, configs, everything. |
| "The edit is too complex for fuzzy matching" | Try it with `dry_run=true` first. DMP handles whitespace differences, minor mismatches. You'll see the diff before applying. |

## Workflow

1. **Always preview first**: `dry_run=true` (the default). Review the diff.
2. **Then apply**: same call with `dry_run=false`.

### edit_symbol (for code symbols)

- `operation: "replace"` -- swap an entire function/struct/class definition
- `operation: "insert_after"` -- add code after a symbol
- `operation: "insert_before"` -- add code before a symbol

### edit_file (for any text)

- `old_text`: text to find (DMP fuzzy matched)
- `new_text`: replacement text
- `occurrence`: `"first"` (default), `"last"`, or `"all"`

## Example: the cost of Read+Edit

Changing a version number in Cargo.toml:

**Read+Edit pattern (5 calls, ~800 tokens):**
1. Edit -- fails ("File has not been read yet")
2. Read Cargo.toml -- waste
3. Grep for the version line -- unnecessary
4. Read again with offset -- more waste
5. Edit -- finally works

**edit_file pattern (2 calls, ~200 tokens):**
1. `edit_file(file_path="Cargo.toml", old_text='version = "6.6.2"', new_text='version = "6.6.3"', dry_run=true)` -- preview
2. Same call with `dry_run=false` -- done

4x fewer tokens. 3 fewer round trips.
