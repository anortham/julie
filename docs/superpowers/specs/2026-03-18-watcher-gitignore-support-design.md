# Watcher `.gitignore` Support

**Date:** 2026-03-18
**Status:** Approved
**Priority:** P3 (Medium-Low) — architecturally wrong but mitigated by extension filtering

## Problem

Julie has two independent file-filtering systems that disagree about what belongs in the index:

| System | Engine | `.gitignore` | Negation | Nested | Dynamic |
|--------|--------|-------------|----------|--------|---------|
| **Walker** (`build_walker`) | `ignore::WalkBuilder` | Full support | Yes | Yes | Per-project |
| **Watcher** (`build_ignore_patterns`) | Hardcoded `glob::Pattern` list | None | No | No | Static |

The watcher uses ~25 hardcoded glob patterns that approximate common `.gitignore` entries. Any pattern in a project's `.gitignore` that isn't also in this hardcoded list leaks through the watcher's filter.

### What leaks through (examples from Julie's own `.gitignore`)

- `.vscode/`, `.idea/` — IDE configs
- `*.swp`, `*.swo`, `*~` — editor swap files
- `tree-sitter-*/` — extracted archives
- Any custom user-added patterns
- Negation patterns like `!**/test_calls.rs` (impossible with `glob::Pattern`)

### Why it's not worse

Extension filtering (`supported_extensions`) is checked before ignore patterns, blocking ~70% of leaked files. The hardcoded list covers the highest-impact directories (`node_modules`, `target`, `.git`, `__pycache__`). But as Julie supports more extensions (`.json`, `.yaml`, `.toml` are already supported), the surface area grows.

## Solution: Root `.gitignore` + `.julieignore` via `GitignoreBuilder`

Replace `Vec<glob::Pattern>` with the `ignore` crate's `Gitignore` type, built at startup from the workspace root's `.gitignore` and `.julieignore`.

**Scope decision:** Only the root `.gitignore` is loaded (not nested subdirectory `.gitignore` files). The `GitignoreBuilder` scoping semantics for nested files are subtle — patterns with leading `/` may not resolve correctly relative to their subdirectory when composed into a single flat matcher. Root-only covers 95% of value and is definitely correct. Nested support can be added as a follow-up once `GitignoreBuilder`'s multi-file scoping is verified.

### Three Filtering Layers (Matching the Walker)

| Layer | Purpose | Source |
|-------|---------|--------|
| **BLACKLISTED_DIRECTORIES** | Safety net — always skip regardless of `.gitignore` | Hardcoded set (`src/tools/shared.rs`) |
| **Gitignore matcher** | Project-specific rules | Root `.gitignore` + `.julieignore` + Julie synthetic rules |
| **Extension + filename** | Only index supported file types | Existing logic, unchanged |

### Building the Matcher

```
build_gitignore_matcher(workspace_root: &Path) -> Result<Gitignore>
  1. GitignoreBuilder::new(workspace_root)
  2. builder.add(workspace_root.join(".gitignore"))  // root gitignore only
     → Log and continue if file missing or has parse errors (add() returns Option<Error>)
  3. builder.add(workspace_root.join(".julieignore"))  // if exists
     → Log and continue on errors
  4. Add synthetic always-ignore lines via builder.add_line(None, pattern):
     - .julie/
     - .memories/
  5. builder.build()? → Gitignore
     → build() returns Result — propagate GlobSet build errors
```

`Gitignore` is `Clone + Send + Sync` — clones into the spawned event task like the current `Vec<glob::Pattern>`.

Note: `.claude/worktrees/` synthetic pattern is unnecessary — `.claude/` is already in `BLACKLISTED_DIRECTORIES`, which is checked independently.

### Matching at Event Time

The watcher receives **absolute paths** from `notify`. The `Gitignore` matcher expects **relative paths** (its `matched_path_or_any_parents()` asserts `!path.has_root()`).

```rust
// Strip workspace root to get relative path
let rel_path = path.strip_prefix(&workspace_root).unwrap_or(path);

// Use matched_path_or_any_parents(), NOT matched()
// matched() only checks the exact path — misses files INSIDE gitignored directories.
// e.g., pattern "build/" becomes glob "**/build" with is_only_dir=true.
// matched("build/src/file.rs", is_dir=false) → None (no match!)
// matched_path_or_any_parents("build/src/file.rs") → checks "build" with is_dir=true → Ignore
gitignore.matched_path_or_any_parents(&rel_path, rel_path.is_dir()).is_ignore()
```

Negation works automatically — `matched_path_or_any_parents()` returns `Whitelist` for negated patterns, which `is_ignore()` correctly returns `false` for.

### Deletion Events

`should_process_deletion` differs from `should_index_file` — it skips the `path.is_file()` check because the file no longer exists on disk. For the gitignore check, deleted files are matched with `is_dir: false` (the path no longer exists, so `is_dir()` returns `false`). This is correct: `matched_path_or_any_parents()` internally checks parent components with `is_dir: true`, so directory-only patterns like `build/` still match deleted files inside `build/`.

## File Changes

### `src/watcher/filtering.rs` (main change site)

- **Remove** `build_ignore_patterns()` and its `glob::Pattern` list
- **Add** `build_gitignore_matcher(workspace_root: &Path) -> Result<Gitignore>` — loads root `.gitignore` + `.julieignore` + synthetic patterns, logs and continues on partial parse errors from `add()`, propagates `build()` errors
- **Add** `contains_blacklisted_directory(path: &Path) -> bool` — checks path components against `BLACKLISTED_DIRECTORIES`
- **Consolidate** `should_index_file` — single public version taking `(&Path, &HashSet<String>, &Gitignore, &Path)` where the last `&Path` is the workspace root for prefix stripping. Used by both `events.rs` and tests
- **Add** `should_process_deletion` — same as `should_index_file` but skips `is_file()` check
- **Update** tests for new gitignore-aware API

### `src/watcher/events.rs`

- **Remove** private `should_index_file` and `should_process_deletion` (consolidated into `filtering.rs`)
- **Update** `process_file_system_event` signature: `&Gitignore` + `&Path` (workspace root) instead of `&[glob::Pattern]`

### `src/watcher/mod.rs`

- **Change** `IncrementalIndexer.ignore_patterns: Vec<glob::Pattern>` → `gitignore: Gitignore`
- **Update** `new()`: call `build_gitignore_matcher(&workspace_root)` instead of `build_ignore_patterns()`
- **Update** `start_watching()`: clone `self.gitignore` and `self.workspace_root` instead of `self.ignore_patterns`; pass workspace root to `process_file_system_event`

### Unchanged

- `src/watcher/types.rs`
- `src/utils/walk.rs` (walker already correct)

## Test Plan

1. Root `.gitignore` patterns respected by matcher (e.g., `.vscode/` blocked)
2. Files inside gitignored directories blocked (e.g., `build/src/file.rs` when `build/` is ignored)
3. `.julieignore` patterns merged in
4. Negation patterns work (`test_*.rs` ignored but `!**/test_calls.rs` allowed)
5. Synthetic Julie patterns always present (`.julie/`, `.memories/`)
6. `BLACKLISTED_DIRECTORIES` check works independently of `.gitignore`
7. Missing `.gitignore` file handled gracefully (no error, empty matcher)
8. Malformed patterns in `.gitignore` logged and skipped (partial errors from `add()`)
9. Deletion events correctly filtered (no `is_file()` check, gitignore still applies)
10. Absolute-to-relative path stripping works for paths under workspace root
11. Existing `test_should_index_file_skips_lockfiles` updated for new signature

## Not In Scope

- **Nested `.gitignore` files** — scoping semantics of `GitignoreBuilder` for multi-file composition need verification. Follow-up task.
- **Reacting to `.gitignore` changes at runtime** — restart required (same as walker)
- **Global gitignore** (`~/.config/git/ignore`) — nice-to-have for later
