# Tool Error Recovery — Context-Aware Hints

## Problem

Multiple Julie tools return dead-end error messages when they find nothing, giving
the LLM no guidance on what to try next. Observed: a `get_symbols` call with a wrong
file path led to a 4-call recovery sequence (should have been 1).

## Design Principle

Don't guess, don't auto-redirect — just point the LLM at the right next step.
No behavior changes, no database queries in error paths. Just better messages.

## Changes

### 1. get_symbols — context-aware file-not-found (primary.rs, reference.rs)

- **`target` was specified** (symbol-first intent): suggest `deep_dive(symbol="<target>")`
- **No `target`** (file-first intent): suggest `fast_search(query="<filename>", search_target="definitions")`

### 2. get_symbols — tool description (handler.rs)

Added: *"Requires exact file path — use deep_dive(symbol=...) if you don't know the path."*

### 3. fast_refs — no references found (formatting.rs)

Added: suggest `fast_search` to verify the symbol exists.

### 4. fast_search — no results found (mod.rs, line_mode.rs)

- **Definition mode**: suggest switching to `search_target="content"` for line-level search
- **Content mode**: suggest switching to `search_target="definitions"` or broadening filters

### 5. get_context — no relevant symbols (pipeline.rs, formatting.rs)

Added: suggest `fast_search` for exact matches, verify workspace is indexed.
Applied to both readable and compact format paths.

### 6. Standardized "No workspace initialized" errors

All bare "No workspace initialized" / "Search index not initialized" / "Database not
initialized" errors now include: *"Run manage_workspace(operation=\"index\") first."*

Affected files: `deep_dive/mod.rs`, `get_context/pipeline.rs`, `search/text_search.rs`,
`search/line_mode.rs`, `workspace/indexing/processor.rs`.

## Non-goals

- No file guessing or fuzzy matching (the `constants.cs` problem)
- No database queries in error paths
- No behavior changes to any tool
