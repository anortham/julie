# Token-Saving Edit Tools: DMP-Powered Editing Without Reading

**Date:** 2026-04-03
**Status:** Draft
**Scope:** Two new MCP tools (edit_file, edit_symbol), get_symbols markdown fix, adoption strategy

## Problem

Every file edit in Claude Code today requires a full Read before Edit. The built-in Edit tool enforces "you must read the file first" at the harness level. For a 300-line file, that's ~1500 tokens burned just to change 5 lines. Across a session with dozens of edits, this is the single largest source of wasted context tokens.

Julie already has deep structural knowledge of every indexed file (symbol boundaries, line ranges, content) via tree-sitter and the file watcher keeps this live. But today Julie is read-only: 8 tools, none with write access. The agent must fall back to built-in Read+Edit for every change.

**Evaluation lens:** Every change must reduce the total tokens an agent needs to complete a task. Julie's core value proposition is intelligence per token.

## Goal

Add two DMP-powered editing tools that let agents edit files without reading them first. Wrap Google's battle-tested diff-match-patch library (already a dependency: `diff-match-patch-rs = "0.5.1"`) with Julie's indexed knowledge to provide safe, token-efficient editing.

Secondary: fix get_symbols to return full section content for markdown files (the data is already indexed but not surfaced through the right line range).

## Non-Goals

- Replacing the built-in Write tool (creating new files doesn't waste tokens)
- Multi-file batch edits (Phase 2)
- Blocking built-in Edit via hooks (Phase 2, after confidence is established)
- Julie-owned read_file replacement (Phase 2)
- Regex-based pattern matching (DMP's fuzzy matching is sufficient)

## Prior Art

### Serena (~/source/serena)

Serena provides symbol-aware editing tools (`replace_symbol_body`, `insert_after_symbol`, `insert_before_symbol`) backed by LSP. Also provides `replace_content` with a regex mode that lets agents match `"beginning.*?end"` to replace large sections without reading. Adoption is driven by system prompt persuasion, tool descriptions that position Serena tools as "call this FIRST", and context-specific exclusion of built-in tools.

### Julie v1.x-v2.1 (removed in v2.2.0)

Julie previously had DMP-powered tools: `edit_lines` (line-level editing), `edit_symbol` (symbol body replacement), `fuzzy_replace` (fuzzy find-and-replace). These were supported by:
- `EditingTransaction` / `MultiFileTransaction` for atomic writes (temp file + rename)
- Golden master test suite (SOURCE/CONTROL fixture files, byte-for-byte verification)
- Security tests (path traversal: absolute, relative, symlink)
- Balance validation (bracket/paren check before committing)

Removed in v2.2.0 during tool consolidation (14 to 9 tools). The token-saving value wasn't recognized at the time. The `diff-match-patch-rs` dependency was never removed from Cargo.toml.

### Key Lesson from Old Implementation

The old `fuzzy_replace` was slow (~7s for 20KB files) because it layered Levenshtein distance validation on top of DMP matching. This was over-engineered. DMP's native `patch_apply` already has fuzzy matching with configurable thresholds. The new implementation should use DMP directly without the Levenshtein layer.

## Design

### Tool 1: `edit_file` -- DMP Fuzzy Replace

General-purpose editing for any file. The agent provides old_text (what to find) and new_text (what to replace with). DMP handles fuzzy matching, so old_text doesn't need to be exact.

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `file_path` | string | required | Relative to workspace root |
| `old_text` | string | required | Text to find (DMP fuzzy-matched) |
| `new_text` | string | required | Replacement text |
| `dry_run` | bool | true | Preview diff without applying |
| `occurrence` | string | "first" | "first", "last", or "all" |

**Internal flow:**

1. Resolve and validate file_path (security: no traversal outside workspace)
2. Read file content internally (not costing agent context tokens)
3. DMP `match_main` to find best location for old_text
4. Handle occurrence parameter (first match, last match, or all)
5. DMP `patch_make` + `patch_apply` to create and apply the replacement
6. Balance validation (code files only, skip for markdown/yaml/json/toml): parse the modified file's bracket/paren structure and reject if any `{}[]()` are unmatched (e.g., an open brace with no close). This catches truncated or corrupted edits. Legitimate structural changes (adding a new function with its own balanced braces) pass because the check is "are all brackets matched," not "did the count change."
7. If dry_run: return unified diff preview (--- before / +++ after format) scoped to the changed region with 3 lines of context
8. If not dry_run: atomic write via EditingTransaction (temp + rename). The file watcher picks up the filesystem event naturally; no explicit notification needed.
9. Return confirmation with file path and line numbers affected

**Tool description (for MCP, under 2k chars):**

```
Edit a file without reading it first. Provide old_text (fuzzy-matched) and
new_text. Saves the full Read step that the built-in Edit tool requires.
Use occurrence to control which match: "first" (default), "last", or "all".
Always dry_run=true first to preview, then dry_run=false to apply.
```

**Token savings:** Eliminates the mandatory Read step. For a 300-line file, that's ~1500 tokens saved per edit. The agent only needs to provide old_text (which it already knows from prior search/get_symbols output) and new_text.

### Tool 2: `edit_symbol` -- Symbol-Aware Editing

Code-aware editing that leverages Julie's indexed symbol boundaries. The agent references a symbol by name; Julie knows exactly where it lives.

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `symbol` | string | required | Symbol name (supports qualified: `MyClass::method`) |
| `operation` | string | required | "replace", "insert_after", "insert_before" |
| `content` | string | required | New code/text to insert or replace with |
| `file_path` | string | optional | Disambiguate when multiple symbols share a name |
| `dry_run` | bool | true | Preview diff without applying |

**Internal flow:**

1. Look up symbol in the index (name, optional file_path for disambiguation)
2. If multiple matches and no file_path: return error listing matches with file locations
3. Read the symbol's file content internally
4. Based on operation:
   - `replace`: Replace the symbol's entire definition (signature + body, from start_line to end_line inclusive) with content. The agent provides the complete new definition. DMP patches the replacement in.
   - `insert_after`: Insert content on a new line after the symbol's end_line
   - `insert_before`: Insert content on a new line before the symbol's start_line
5. Balance validation (code files only, same matched-bracket check as edit_file)
6. If dry_run: return unified diff preview scoped to the changed region
7. If not dry_run: atomic write via EditingTransaction. File watcher picks up the change naturally.
8. Return confirmation with file path and affected line range

**Tool description (for MCP):**

```
Edit a symbol by name without reading the file. Operations: replace (swap body),
insert_after, insert_before. The symbol is looked up from Julie's index.
Combine with deep_dive or get_symbols for zero-read editing workflows.
Always dry_run=true first to preview, then dry_run=false to apply.
```

**Token savings:** Even more than edit_file, because the agent doesn't need to specify old_text at all. It references the symbol by name (already known from deep_dive/get_symbols), and Julie handles the rest.

### Bonus: get_symbols Markdown Section Fix

**Problem:** The markdown extractor creates symbols for section headings but sets the symbol's line range to just the heading node (e.g., `## Quick Reference` on line 30). The section content (paragraphs, lists, code blocks) is stored as `doc_comment` for RAG embedding but not reflected in the line range.

**Fix:** In `extract_heading()`, use the parent section node's line range instead of the heading node's line range. This way `get_symbols(file, target="Quick Reference", mode="minimal")` returns the full section content, not just the heading text.

**Impact:** Enables structured reading of markdown files (plans, docs, configs) through existing infrastructure. No new tool needed.

**Risk:** Changes what line ranges mean for markdown symbols throughout the system. Search results, deep_dive, fast_refs all use these ranges. Need to verify that wider ranges don't cause unexpected behavior in those tools.

## Adoption Strategy

Four layers, from always-on to task-specific:

### Layer 1: Tool Descriptions

Emphasize "without reading it first" in the MCP tool descriptions. This is the hook that makes agents prefer these tools over built-in Edit. The descriptions above are designed for this.

### Layer 2: SessionStart Hook

Add to Julie's SessionStart hook instructions:

```
6. **Edit without reading**: Use edit_file or edit_symbol instead of
   Read + Edit. They don't require reading the file first.
   edit_file: fuzzy find-and-replace (any file).
   edit_symbol: edit by symbol name (code files).
   Always dry_run=true first, then dry_run=false to apply.
```

Survives context compaction. Injected into every session.

### Layer 3: PreToolUse Hook (Soft Nudge)

A hook that fires when the agent calls the built-in `Edit` tool:

```json
{
  "event": "PreToolUse",
  "matcher": "Edit",
  "command": "echo 'Tip: mcp__julie__edit_file and edit_symbol edit without reading first. Consider using them to save tokens.'"
}
```

Nudge, not block. The agent can still use Edit. In Phase 2, this could escalate to blocking once the tools are proven reliable.

### Layer 4: Editing Skill

A new `editing` skill added to the Julie plugin:

```
Workflow:
1. Use get_symbols or deep_dive to understand the target
2. Use edit_symbol for code changes (by symbol name)
3. Use edit_file for non-code or arbitrary text changes
4. Always dry_run=true first, review the diff, then dry_run=false
5. Only fall back to Read + Edit if Julie's tools can't handle the case
```

Update existing skills (explore-area, impact-analysis, etc.) to reference edit_file/edit_symbol in their allowed-tools lists.

## Testing Strategy

### Golden Master (SOURCE/CONTROL) Pattern

Deterministic, byte-for-byte verification using fixture files.

```
fixtures/editing/
  sources/                        # INPUT files
    rust_module.rs                # Functions, structs, impls
    python_class.py               # Class with methods
    typescript_component.tsx      # React component
    markdown_plan.md              # Sections, headings, lists
    config.yaml                   # Nested keys
    unicode_sample.rs             # Multi-byte chars, emoji
    crlf_sample.py                # Windows line endings

  controls/                       # EXPECTED OUTPUT (golden masters)
    edit-file/
      rust_exact_replace.rs       # Exact text match
      rust_fuzzy_replace.rs       # Whitespace-tolerant match
      markdown_section_edit.md    # Non-code file edit
      yaml_key_edit.yaml          # Config file edit
      unicode_replace.rs          # Multi-byte safety
      crlf_preserved.py           # Line ending preservation
      multiple_occurrences.rs     # occurrence="all"

    edit-symbol/
      rust_replace_fn_body.rs     # Replace function body
      python_insert_after_class.py  # Insert after class
      ts_insert_before_fn.tsx     # Insert before function
      rust_qualified_replace.rs   # MyClass::method replace
```

### Test Categories

**1. edit_file functional:**
- Exact text match replacement
- Fuzzy match (minor whitespace/typo differences)
- Multiple occurrences (first, last, all)
- No match found: graceful error with helpful message
- Unicode/multi-byte characters
- CRLF preservation
- Non-code files (markdown, yaml, json, toml)

**2. edit_symbol functional:**
- Replace function body
- Replace method within class/impl (qualified names)
- Insert after struct/class
- Insert before function
- Ambiguous symbol (multiple matches, no file_path): error listing locations
- Symbol not found: graceful error

**3. Balance validation:**
- Edit that breaks bracket balance: rejected with clear error
- Edit that legitimately changes structure (adding a block): accepted
- Non-code files: balance check skipped

**4. Dry run:**
- dry_run=true: diff preview returned, file unchanged on disk
- dry_run=false: change applied, file modified

**5. Security (3 per tool, non-negotiable):**
- Absolute path outside workspace (`/etc/passwd`)
- Relative traversal (`../../../../etc/passwd`)
- Symlink to outside workspace (unix only)

**6. Transaction atomicity:**
- Temp file cleanup on failure
- Read-only file: pre-flight validation error
- Verify no partial writes on crash

**7. get_symbols markdown fix:**
- Structure mode shows section with full line range
- Minimal mode with target returns section content
- Nested sections (h2 within h1) preserve hierarchy
- Frontmatter sections unaffected

### xtask Integration

Add `editing` test bucket to the xtask runner. Golden master tests are fast (file I/O + small tool operations), so they belong in the `dev` tier.

## File Organization

New files (estimated):

```
src/tools/editing/
  mod.rs              # Public API, re-exports
  edit_file.rs        # edit_file tool implementation
  edit_symbol.rs      # edit_symbol tool implementation
  transaction.rs      # EditingTransaction (atomic temp+rename)
  validation.rs       # Balance validation, security checks

src/tests/tools/editing/
  mod.rs              # Test module root
  edit_file.rs        # edit_file golden master tests
  edit_symbol.rs      # edit_symbol golden master tests
  security.rs         # Path traversal tests
  validation.rs       # Balance check, dry run tests
  markdown_fix.rs     # get_symbols markdown section tests
```

All files under 500 lines per project standards.

## Phase 2 Horizon (Not Designed, Just Noted)

- **Julie-owned `read_file`**: structure-aware reading for any file type
- **Batch edits**: multiple patches in one edit_file call
- **PreToolUse escalation**: move from nudge to block on built-in Edit
- **Token savings metrics**: query_metrics tracks "tokens saved by edit_file vs hypothetical Read+Edit"
- **Multi-file symbol edits**: edit the same symbol pattern across multiple files

## Risk Assessment

**DMP fuzzy matching too aggressive:** Could match the wrong location. Mitigated by dry_run=true default and balance validation. The agent reviews the diff before applying.

**Stale symbol boundaries:** File changed since last index. Mitigated by file watcher keeping index live. Edge case: rapid edits before watcher catches up. Mitigated by re-reading file content at edit time and using DMP fuzzy matching (tolerates minor shifts).

**Adoption failure:** Agents keep using built-in Edit despite the tools being available. Mitigated by the four-layer adoption strategy. The PreToolUse hook is the backstop.

**Markdown line range change:** Widening section line ranges could affect search result display, deep_dive, fast_refs. Needs verification across all consumers of markdown symbol data. Low risk since markdown symbols are already treated differently (SymbolKind::Module, no signatures).
