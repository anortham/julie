# Watcher `.gitignore` Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the watcher's hardcoded `glob::Pattern` list with the `ignore` crate's `Gitignore` matcher so the file watcher respects `.gitignore` the same way the walker does.

**Architecture:** Build a `Gitignore` matcher at `IncrementalIndexer::new()` from the workspace root's `.gitignore` + `.julieignore` + synthetic Julie patterns. Use `matched_path_or_any_parents()` for directory-aware matching. Keep `BLACKLISTED_DIRECTORIES` as an independent safety net.

**Tech Stack:** `ignore` crate v0.4 (`Gitignore`, `GitignoreBuilder`), already a dependency.

**Spec:** `docs/superpowers/specs/2026-03-18-watcher-gitignore-support-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `src/tools/shared.rs` | **Modify** | Add `.gradle`, `.dart_tool` to `BLACKLISTED_DIRECTORIES` |
| `src/watcher/filtering.rs` | **Rewrite** | All filtering logic: matcher builder, blacklist check, should_index_file, should_process_deletion |
| `src/watcher/events.rs` | **Modify** | Update `process_file_system_event` signature; remove private filter functions |
| `src/watcher/mod.rs` | **Modify** | Change `IncrementalIndexer` field type, update `new()` and `start_watching()` |
| `src/tests/integration/watcher.rs` | **Modify** | Update tests using old `glob::Pattern` API |

---

### Task 0: Add Missing Directories to `BLACKLISTED_DIRECTORIES`

**Files:**
- Modify: `src/tools/shared.rs`

The old hardcoded glob list included `.gradle/` and `.dart_tool/` which aren't in `BLACKLISTED_DIRECTORIES`. Add them so the safety-net layer catches these regardless of `.gitignore`.

- [ ] **Step 1: Add `.gradle` and `.dart_tool` to BLACKLISTED_DIRECTORIES**

In `src/tools/shared.rs`, find the `BLACKLISTED_DIRECTORIES` constant. Add these entries to the "Build and output directories" section (after the existing `.next` and `.nuxt` entries or nearby):

```rust
    ".gradle",      // Gradle build cache (Java, Android)
    ".dart_tool",   // Dart/Flutter build cache
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check 2>&1 | tail -5
```

Expected: Clean compile.

- [ ] **Step 3: Commit**

```bash
git add src/tools/shared.rs
git commit -m "fix(shared): add .gradle and .dart_tool to BLACKLISTED_DIRECTORIES"
```

---

### Task 1: `build_gitignore_matcher` — Core Builder Function

**Files:**
- Modify: `src/watcher/filtering.rs`

This replaces `build_ignore_patterns()`. Builds a `Gitignore` matcher from root `.gitignore` + `.julieignore` + synthetic patterns.

- [ ] **Step 1: Write failing test for build_gitignore_matcher**

Add this test at the bottom of the `mod tests` block in `src/watcher/filtering.rs` (replacing the existing `test_ignore_patterns` test):

```rust
#[test]
fn test_build_gitignore_matcher_with_gitignore_file() {
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".gitignore"),
        "*.log\nbuild/\n!important.log\n",
    )
    .unwrap();

    let gitignore = build_gitignore_matcher(dir.path()).unwrap();

    // *.log should be ignored
    assert!(
        gitignore
            .matched_path_or_any_parents("debug.log", false)
            .is_ignore(),
        "*.log pattern should match"
    );
    // !important.log negation should whitelist
    assert!(
        !gitignore
            .matched_path_or_any_parents("important.log", false)
            .is_ignore(),
        "!important.log negation should whitelist"
    );
    // Files inside build/ should be ignored (directory pattern)
    assert!(
        gitignore
            .matched_path_or_any_parents("build/output.js", false)
            .is_ignore(),
        "Files inside build/ should be ignored"
    );
    // Synthetic .julie/ pattern should always be present
    assert!(
        gitignore
            .matched_path_or_any_parents(".julie/data.db", false)
            .is_ignore(),
        ".julie/ synthetic pattern should be present"
    );
    // Synthetic .memories/ pattern should always be present
    assert!(
        gitignore
            .matched_path_or_any_parents(".memories/checkpoint.md", false)
            .is_ignore(),
        ".memories/ synthetic pattern should be present"
    );
    // Synthetic cmake-build-* pattern should work
    assert!(
        gitignore
            .matched_path_or_any_parents("cmake-build-debug/CMakeCache.txt", false)
            .is_ignore(),
        "cmake-build-* synthetic pattern should be present"
    );
    // Synthetic *.min.js pattern should work
    assert!(
        gitignore
            .matched_path_or_any_parents("dist/app.min.js", false)
            .is_ignore(),
        "*.min.js synthetic pattern should be present"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --lib test_build_gitignore_matcher_with_gitignore_file 2>&1 | tail -10
```

Expected: FAIL — `build_gitignore_matcher` doesn't exist yet.

- [ ] **Step 3: Implement `build_gitignore_matcher`**

Add these new imports to the existing ones in `src/watcher/filtering.rs` (keep the existing `BLACKLISTED_FILENAMES` import):

```rust
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use tracing::warn;
```

Then add the new function (leave `build_ignore_patterns` in place for now — Task 7 removes it):

```rust
/// Build a Gitignore matcher from the workspace's .gitignore, .julieignore,
/// and synthetic Julie-specific patterns.
///
/// Only the root .gitignore is loaded (not nested subdirectory files).
/// Partial parse errors (malformed patterns) are logged and skipped.
/// GlobSet build errors from `builder.build()` are propagated.
pub fn build_gitignore_matcher(workspace_root: &Path) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(workspace_root);

    // Load root .gitignore (log and continue if missing or has parse errors)
    let gitignore_path = workspace_root.join(".gitignore");
    if gitignore_path.is_file() {
        if let Some(err) = builder.add(&gitignore_path) {
            warn!("Partial error reading {}: {}", gitignore_path.display(), err);
        }
    }

    // Load .julieignore if present
    let julieignore_path = workspace_root.join(".julieignore");
    if julieignore_path.is_file() {
        if let Some(err) = builder.add(&julieignore_path) {
            warn!("Partial error reading {}: {}", julieignore_path.display(), err);
        }
    }

    // Synthetic always-ignore patterns:
    // - Julie's own directories (won't be in user's .gitignore)
    // - Build tool directories not in BLACKLISTED_DIRECTORIES (cmake uses wildcard names)
    // - Minified/bundled JS (supported extension, usually gitignored but safety net)
    // Note: .claude/ is in BLACKLISTED_DIRECTORIES so doesn't need a synthetic pattern.
    let synthetics = [
        ".julie/",
        ".memories/",
        "cmake-build-*/",
        "*.min.js",
        "*.bundle.js",
    ];
    for pattern in &synthetics {
        builder
            .add_line(None, pattern)
            .map_err(|e| anyhow::anyhow!("Invalid synthetic pattern '{}': {}", pattern, e))?;
    }

    builder
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build gitignore matcher: {}", e))
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test --lib test_build_gitignore_matcher_with_gitignore_file 2>&1 | tail -10
```

Expected: PASS

- [ ] **Step 5: Write test for missing .gitignore**

```rust
#[test]
fn test_build_gitignore_matcher_no_gitignore_file() {
    let dir = tempfile::tempdir().unwrap();
    // No .gitignore file — should still succeed with synthetic patterns only
    let gitignore = build_gitignore_matcher(dir.path()).unwrap();

    // Synthetic patterns should still work
    assert!(
        gitignore
            .matched_path_or_any_parents(".julie/data.db", false)
            .is_ignore(),
        ".julie/ should be ignored even without .gitignore"
    );
    // Normal files should not be ignored
    assert!(
        !gitignore
            .matched_path_or_any_parents("src/main.rs", false)
            .is_ignore(),
        "Normal files should not be ignored"
    );
}
```

- [ ] **Step 6: Run test to verify it passes**

```bash
cargo test --lib test_build_gitignore_matcher_no_gitignore_file 2>&1 | tail -10
```

Expected: PASS

- [ ] **Step 7: Write test for .julieignore merging**

```rust
#[test]
fn test_build_gitignore_matcher_merges_julieignore() {
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "*.log\n").unwrap();
    fs::write(dir.path().join(".julieignore"), "generated/\n").unwrap();

    let gitignore = build_gitignore_matcher(dir.path()).unwrap();

    // .gitignore pattern works
    assert!(
        gitignore
            .matched_path_or_any_parents("debug.log", false)
            .is_ignore()
    );
    // .julieignore pattern works
    assert!(
        gitignore
            .matched_path_or_any_parents("generated/output.rs", false)
            .is_ignore()
    );
}
```

- [ ] **Step 8: Run and verify**

```bash
cargo test --lib test_build_gitignore_matcher_merges_julieignore 2>&1 | tail -10
```

Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add src/watcher/filtering.rs
git commit -m "feat(watcher): add build_gitignore_matcher using ignore crate"
```

---

### Task 2: `contains_blacklisted_directory` + `is_gitignored` Helpers

**Files:**
- Modify: `src/watcher/filtering.rs`

Extract the safety-net directory check and the gitignore matching into focused helpers.

- [ ] **Step 1: Write failing test for contains_blacklisted_directory**

```rust
#[test]
fn test_contains_blacklisted_directory() {
    use std::path::PathBuf;

    // Path through node_modules — should be blocked
    assert!(contains_blacklisted_directory(&PathBuf::from(
        "/workspace/node_modules/react/index.js"
    )));
    // Path through target — should be blocked
    assert!(contains_blacklisted_directory(&PathBuf::from(
        "/workspace/target/debug/build.rs"
    )));
    // Path through .git — should be blocked
    assert!(contains_blacklisted_directory(&PathBuf::from(
        "/workspace/.git/config"
    )));
    // Normal source path — should NOT be blocked
    assert!(!contains_blacklisted_directory(&PathBuf::from(
        "/workspace/src/main.rs"
    )));
    // Path through .vscode — should be blocked
    assert!(contains_blacklisted_directory(&PathBuf::from(
        "/workspace/.vscode/settings.json"
    )));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --lib test_contains_blacklisted_directory 2>&1 | tail -10
```

Expected: FAIL — function doesn't exist.

- [ ] **Step 3: Implement both helpers**

Update the existing `BLACKLISTED_FILENAMES` import in `src/watcher/filtering.rs` to also include `BLACKLISTED_DIRECTORIES`:

```rust
use crate::tools::shared::{BLACKLISTED_DIRECTORIES, BLACKLISTED_FILENAMES};

/// Check if any path component matches a blacklisted directory name.
/// This is the safety-net layer — always filters regardless of .gitignore content.
pub fn contains_blacklisted_directory(path: &Path) -> bool {
    path.components().any(|c| {
        if let std::path::Component::Normal(name) = c {
            if let Some(s) = name.to_str() {
                return BLACKLISTED_DIRECTORIES.contains(&s);
            }
        }
        false
    })
}

/// Check if a path is gitignored, with defensive prefix stripping.
/// Returns false (not ignored) if the path is not under the workspace root.
pub fn is_gitignored(path: &Path, gitignore: &Gitignore, workspace_root: &Path) -> bool {
    let rel_path = match path.strip_prefix(workspace_root) {
        Ok(p) => p,
        Err(_) => return false, // Not under workspace, don't filter
    };
    gitignore
        .matched_path_or_any_parents(rel_path, path.is_dir())
        .is_ignore()
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test --lib test_contains_blacklisted_directory 2>&1 | tail -10
```

Expected: PASS

- [ ] **Step 5: Write test for is_gitignored**

```rust
#[test]
fn test_is_gitignored() {
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "*.log\nbuild/\n").unwrap();

    let gitignore = build_gitignore_matcher(dir.path()).unwrap();

    // Create a real file so is_file()/is_dir() work
    fs::write(dir.path().join("app.log"), "log data").unwrap();
    assert!(is_gitignored(
        &dir.path().join("app.log"),
        &gitignore,
        dir.path()
    ));

    // File inside gitignored directory
    fs::create_dir_all(dir.path().join("build")).unwrap();
    fs::write(dir.path().join("build/output.js"), "code").unwrap();
    assert!(is_gitignored(
        &dir.path().join("build/output.js"),
        &gitignore,
        dir.path()
    ));

    // Normal file should NOT be ignored
    fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    assert!(!is_gitignored(
        &dir.path().join("main.rs"),
        &gitignore,
        dir.path()
    ));

    // Path outside workspace should NOT be ignored (defensive)
    assert!(!is_gitignored(
        Path::new("/some/other/path/file.log"),
        &gitignore,
        dir.path()
    ));
}
```

- [ ] **Step 6: Run and verify**

```bash
cargo test --lib test_is_gitignored 2>&1 | tail -10
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/watcher/filtering.rs
git commit -m "feat(watcher): add contains_blacklisted_directory and is_gitignored helpers"
```

---

### Task 3: New `should_index_file` and `should_process_deletion`

**Files:**
- Modify: `src/watcher/filtering.rs`

Rewrite `should_index_file` with the new three-layer filtering (blacklisted dirs + gitignore + extension). Add `should_process_deletion` for deletion events.

- [ ] **Step 1: Write failing test for new should_index_file**

```rust
#[test]
fn test_should_index_file_with_gitignore() {
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "*.log\nbuild/\n").unwrap();

    let extensions = build_supported_extensions();
    let gitignore = build_gitignore_matcher(dir.path()).unwrap();

    // Normal Rust file should be indexed
    let rs_file = dir.path().join("src/main.rs");
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(&rs_file, "fn main() {}").unwrap();
    assert!(should_index_file(&rs_file, &extensions, &gitignore, dir.path()));

    // File in gitignored directory should NOT be indexed
    fs::create_dir_all(dir.path().join("build")).unwrap();
    let build_file = dir.path().join("build/output.rs");
    fs::write(&build_file, "// generated").unwrap();
    assert!(!should_index_file(
        &build_file, &extensions, &gitignore, dir.path()
    ));

    // File in blacklisted directory should NOT be indexed
    fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
    let nm_file = dir.path().join("node_modules/pkg/index.js");
    fs::write(&nm_file, "module.exports = {}").unwrap();
    assert!(!should_index_file(
        &nm_file, &extensions, &gitignore, dir.path()
    ));

    // Unsupported extension should NOT be indexed
    let txt_file = dir.path().join("readme.txt");
    fs::write(&txt_file, "hello").unwrap();
    assert!(!should_index_file(
        &txt_file, &extensions, &gitignore, dir.path()
    ));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --lib test_should_index_file_with_gitignore 2>&1 | tail -10
```

Expected: FAIL — `should_index_file` signature doesn't match (still takes `&[glob::Pattern]`).

- [ ] **Step 3: Rewrite should_index_file**

Replace the existing `should_index_file` in `src/watcher/filtering.rs` with:

```rust
/// Check if a file should be indexed based on extension, blacklisted directories,
/// and gitignore patterns.
///
/// Three-layer filtering:
/// 1. Blacklisted filenames (lockfiles etc.)
/// 2. Supported extensions
/// 3. BLACKLISTED_DIRECTORIES safety net
/// 4. Gitignore matcher (from .gitignore + .julieignore + synthetic patterns)
pub fn should_index_file(
    path: &Path,
    supported_extensions: &HashSet<String>,
    gitignore: &Gitignore,
    workspace_root: &Path,
) -> bool {
    // Check if it's a file
    if !path.is_file() {
        return false;
    }

    // Skip blacklisted filenames (lockfiles with non-blacklisted extensions)
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        if BLACKLISTED_FILENAMES.contains(&file_name) {
            return false;
        }
    }

    // Check extension
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if !supported_extensions.contains(ext) {
            return false;
        }
    } else {
        return false; // No extension
    }

    // Safety net: always skip blacklisted directories
    if contains_blacklisted_directory(path) {
        return false;
    }

    // Check gitignore patterns
    if is_gitignored(path, gitignore, workspace_root) {
        return false;
    }

    true
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test --lib test_should_index_file_with_gitignore 2>&1 | tail -10
```

Expected: PASS

- [ ] **Step 5: Add should_process_deletion**

```rust
/// Check if a Remove event should be processed.
/// Unlike `should_index_file`, this does NOT check `path.is_file()` because
/// the file no longer exists on disk after deletion.
pub fn should_process_deletion(
    path: &Path,
    supported_extensions: &HashSet<String>,
    gitignore: &Gitignore,
    workspace_root: &Path,
) -> bool {
    // Skip blacklisted filenames
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        if BLACKLISTED_FILENAMES.contains(&file_name) {
            return false;
        }
    }

    // Check extension
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if !supported_extensions.contains(ext) {
            return false;
        }
    } else {
        return false;
    }

    // Safety net: always skip blacklisted directories
    if contains_blacklisted_directory(path) {
        return false;
    }

    // Check gitignore patterns (is_dir=false since file no longer exists)
    let rel_path = match path.strip_prefix(workspace_root) {
        Ok(p) => p,
        Err(_) => return true, // Not under workspace — process it
    };
    if gitignore
        .matched_path_or_any_parents(rel_path, false)
        .is_ignore()
    {
        return false;
    }

    true
}
```

- [ ] **Step 6: Write test for should_process_deletion**

```rust
#[test]
fn test_should_process_deletion_with_gitignore() {
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "build/\n").unwrap();

    let extensions = build_supported_extensions();
    let gitignore = build_gitignore_matcher(dir.path()).unwrap();

    // Deleted .rs file should be processed
    let deleted_rs = dir.path().join("src/deleted.rs");
    assert!(should_process_deletion(
        &deleted_rs, &extensions, &gitignore, dir.path()
    ));

    // Deleted file in gitignored dir should NOT be processed
    let deleted_build = dir.path().join("build/output.rs");
    assert!(!should_process_deletion(
        &deleted_build, &extensions, &gitignore, dir.path()
    ));

    // Deleted file in blacklisted dir should NOT be processed
    let deleted_nm = dir.path().join("node_modules/pkg/index.js");
    assert!(!should_process_deletion(
        &deleted_nm, &extensions, &gitignore, dir.path()
    ));
}
```

- [ ] **Step 7: Run all new filtering tests**

```bash
cargo test --lib test_should_index_file_with_gitignore 2>&1 | tail -10
cargo test --lib test_should_process_deletion_with_gitignore 2>&1 | tail -10
```

Expected: Both PASS

- [ ] **Step 8: Update test_should_index_file_skips_lockfiles**

Update the existing lockfile test to use the new signature:

```rust
#[test]
fn test_should_index_file_skips_lockfiles() {
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    let lockfile = dir.path().join("pnpm-lock.yaml");
    fs::write(&lockfile, "lockfileVersion: '9.0'").unwrap();

    let extensions = build_supported_extensions();
    let gitignore = build_gitignore_matcher(dir.path()).unwrap();

    assert!(
        !should_index_file(&lockfile, &extensions, &gitignore, dir.path()),
        "pnpm-lock.yaml must not be indexed by watcher"
    );
    let lockfile2 = dir.path().join("package-lock.json");
    fs::write(&lockfile2, "{}").unwrap();
    assert!(
        !should_index_file(&lockfile2, &extensions, &gitignore, dir.path()),
        "package-lock.json must not be indexed by watcher"
    );

    fs::remove_file(&lockfile).ok();
    fs::remove_file(&lockfile2).ok();
}
```

- [ ] **Step 9: Run and verify**

```bash
cargo test --lib test_should_index_file_skips_lockfiles 2>&1 | tail -10
```

Expected: PASS

- [ ] **Step 10: Commit**

```bash
git add src/watcher/filtering.rs
git commit -m "feat(watcher): rewrite should_index_file with gitignore + blacklist layers"
```

---

### Task 4: Update `events.rs` — Use New Filtering Functions

**Files:**
- Modify: `src/watcher/events.rs`

Remove the private `should_index_file`, `should_process_deletion` from events.rs. Update `process_file_system_event` to accept `&Gitignore` + `&Path` (workspace root) and call `filtering::should_index_file` / `filtering::should_process_deletion`.

- [ ] **Step 1: Rewrite events.rs**

Replace the full contents of `src/watcher/events.rs`:

```rust
use crate::watcher::filtering;
use crate::watcher::types::{FileChangeEvent, FileChangeType};
use anyhow::Result;
use ignore::gitignore::Gitignore;
use notify::{Event, EventKind};
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::time::SystemTime;
use tokio::sync::Mutex as TokioMutex;
use tracing::debug;

/// Process a file system event and queue any relevant changes
pub async fn process_file_system_event(
    supported_extensions: &HashSet<String>,
    gitignore: &Gitignore,
    workspace_root: &Path,
    index_queue: std::sync::Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
    event: Event,
) -> Result<()> {
    debug!("Processing file system event: {:?}", event);

    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                if filtering::should_index_file(
                    &path,
                    supported_extensions,
                    gitignore,
                    workspace_root,
                ) {
                    let change_event = FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Created,
                        timestamp: SystemTime::now(),
                    };
                    queue_file_change(index_queue.clone(), change_event).await;
                }
            }
        }
        EventKind::Modify(_) => {
            for path in event.paths {
                if filtering::should_index_file(
                    &path,
                    supported_extensions,
                    gitignore,
                    workspace_root,
                ) {
                    let change_event = FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Modified,
                        timestamp: SystemTime::now(),
                    };
                    queue_file_change(index_queue.clone(), change_event).await;
                }
            }
        }
        EventKind::Remove(_) => {
            for path in event.paths {
                if filtering::should_process_deletion(
                    &path,
                    supported_extensions,
                    gitignore,
                    workspace_root,
                ) {
                    let change_event = FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Deleted,
                        timestamp: SystemTime::now(),
                    };
                    queue_file_change(index_queue.clone(), change_event).await;
                }
            }
        }
        _ => {
            debug!("Ignoring event kind: {:?}", event.kind);
        }
    }

    Ok(())
}

/// Queue a file change event for processing
async fn queue_file_change(
    index_queue: std::sync::Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
    event: FileChangeEvent,
) {
    debug!("Queueing file change: {:?}", event);
    let mut queue = index_queue.lock().await;
    queue.push_back(event);
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check 2>&1 | tail -20
```

Expected: Compile errors in `mod.rs` (still passing old types) — that's expected, we fix it in Task 5.

- [ ] **Step 3: Commit (WIP — won't compile until Task 5)**

Don't commit yet — wait until mod.rs is updated in Task 5.

---

### Task 5: Update `IncrementalIndexer` in `mod.rs`

**Files:**
- Modify: `src/watcher/mod.rs`

Change the `ignore_patterns` field to `gitignore: Gitignore`, update constructor and `start_watching`.

- [ ] **Step 1: Update field type on IncrementalIndexer**

In `src/watcher/mod.rs`, add import and change the struct field:

```rust
// Add to imports at top of file:
use ignore::gitignore::Gitignore;

// In struct IncrementalIndexer, replace:
//   ignore_patterns: Vec<glob::Pattern>,
// With:
    gitignore: Gitignore,
```

- [ ] **Step 2: Update `new()` constructor**

In `IncrementalIndexer::new()`, replace:
```rust
let ignore_patterns = filtering::build_ignore_patterns()?;
```
With:
```rust
let gitignore = filtering::build_gitignore_matcher(&workspace_root)?;
```

And in the `Ok(Self { ... })` block, replace `ignore_patterns,` with `gitignore,`.

- [ ] **Step 3: Update `start_watching()` event processing setup**

In `start_watching()`, replace:
```rust
let ignore_patterns = self.ignore_patterns.clone();
```
With:
```rust
let gitignore = self.gitignore.clone();
let workspace_root_for_events = self.workspace_root.clone();
```

And update the `process_file_system_event` call:
```rust
if let Err(e) = events::process_file_system_event(
    &supported_extensions,
    &gitignore,
    &workspace_root_for_events,
    index_queue.clone(),
    event,
)
```

- [ ] **Step 4: Update inline tests in mod.rs**

Replace the `test_ignore_patterns` test in the `mod tests` block at the bottom of `mod.rs`:

```rust
#[test]
fn test_gitignore_matcher() {
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "*.log\nvendor/\n").unwrap();

    let gitignore = filtering::build_gitignore_matcher(dir.path()).unwrap();
    assert!(
        gitignore
            .matched_path_or_any_parents("debug.log", false)
            .is_ignore()
    );
}
```

- [ ] **Step 5: Verify it compiles and tests pass**

```bash
cargo check 2>&1 | tail -10
```

Expected: Clean compilation.

```bash
cargo test --lib test_gitignore_matcher 2>&1 | tail -10
```

Expected: PASS

- [ ] **Step 6: Commit events.rs + mod.rs together**

```bash
git add src/watcher/events.rs src/watcher/mod.rs
git commit -m "feat(watcher): switch IncrementalIndexer from glob::Pattern to Gitignore matcher"
```

---

### Task 6: Update Integration Tests

**Files:**
- Modify: `src/tests/integration/watcher.rs`

Update integration tests that reference the old `build_ignore_patterns` or `glob::Pattern` API.

- [ ] **Step 1: Update test_ignore_patterns**

In `src/tests/integration/watcher.rs`, replace `test_ignore_patterns` (around line 41):

```rust
#[test]
fn test_ignore_patterns() {
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "**/node_modules/**\n*.log\n").unwrap();

    let gitignore = filtering::build_gitignore_matcher(dir.path()).unwrap();

    // Test that gitignore patterns work
    assert!(
        gitignore
            .matched_path_or_any_parents("src/node_modules/package.json", false)
            .is_ignore()
    );
    assert!(
        gitignore
            .matched_path_or_any_parents("frontend/node_modules/react/index.js", false)
            .is_ignore()
    );
}
```

- [ ] **Step 2: Update test_remove_event_queued_for_deleted_file**

In `src/tests/integration/watcher.rs`, update the test around line 206. Replace `ignore_patterns` setup and `process_file_system_event` call:

```rust
#[tokio::test]
async fn test_remove_event_queued_for_deleted_file() {
    use crate::watcher::events::process_file_system_event;
    use notify::{Event, EventKind, event::RemoveKind};
    use std::collections::{HashSet, VecDeque};
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    let temp_dir = tempfile::tempdir().unwrap();
    let test_file = temp_dir.path().join("deleted.rs");

    // Create then delete — simulating a real file deletion
    fs::write(&test_file, "fn gone() {}").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();
    fs::remove_file(&test_file).unwrap();
    assert!(!test_file.exists(), "File should be gone");

    let mut extensions = HashSet::new();
    extensions.insert("rs".to_string());
    let gitignore = filtering::build_gitignore_matcher(temp_dir.path()).unwrap();
    let queue: Arc<TokioMutex<VecDeque<crate::watcher::types::FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));

    let event = Event {
        kind: EventKind::Remove(RemoveKind::File),
        paths: vec![absolute_path],
        attrs: Default::default(),
    };

    process_file_system_event(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        event,
    )
    .await
    .expect("Event processing should succeed");

    let queue_lock = queue.lock().await;
    assert_eq!(
        queue_lock.len(),
        1,
        "Remove event should be queued even though file no longer exists"
    );
    assert!(
        matches!(
            queue_lock[0].change_type,
            crate::watcher::types::FileChangeType::Deleted
        ),
        "Queued event should be a Deleted type"
    );
}
```

- [ ] **Step 3: Run integration tests**

```bash
cargo test --lib tests::integration::watcher 2>&1 | tail -15
```

Expected: All watcher integration tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src/tests/integration/watcher.rs
git commit -m "test(watcher): update integration tests for gitignore-based filtering"
```

---

### Task 7: Cleanup + Remove Old Glob Code

**Files:**
- Modify: `src/watcher/filtering.rs`

Remove `build_ignore_patterns()` and its tests (now dead code). Remove old glob-based tests that no longer apply. Verify no remaining `glob::Pattern` references in the watcher module.

- [ ] **Step 1: Remove `build_ignore_patterns` function**

Delete the entire `build_ignore_patterns()` function from `src/watcher/filtering.rs`.

- [ ] **Step 2: Remove old glob-based tests**

Remove these tests from the `mod tests` block in `filtering.rs`:
- `test_ignore_patterns` (the old one testing `build_ignore_patterns`)
- `test_dotnet_build_artifacts_ignored` (tests the old glob patterns)
- `test_additional_build_artifacts_ignored` (tests the old glob patterns)

These tested the hardcoded glob patterns which no longer exist. The new tests in Tasks 1-3 cover the gitignore-based equivalent.

- [ ] **Step 3: Remove `use glob` if no longer needed**

Check if `glob` crate is still used anywhere in the watcher module. If not, the `glob` dependency may still be needed by other parts of the codebase (search query matching), so don't remove it from `Cargo.toml` — just clean up unused imports in watcher files.

```bash
grep -rn "glob::" src/watcher/ 2>/dev/null
```

Remove any remaining `glob` imports from watcher files.

- [ ] **Step 4: Verify everything compiles and passes**

```bash
cargo check 2>&1 | tail -10
```

Expected: Clean compilation, no warnings about dead code in watcher module.

- [ ] **Step 5: Commit**

```bash
git add src/watcher/filtering.rs
git commit -m "refactor(watcher): remove old build_ignore_patterns and glob-based tests"
```

---

### Task 8: Run Full Dev Test Suite + Final Verification

**Files:** None (verification only)

- [ ] **Step 1: Run cargo xtask test dev**

```bash
cargo xtask test dev 2>&1 | tail -30
```

Expected: All buckets pass (except known pre-existing `core-embeddings` failure).

- [ ] **Step 2: Verify no remaining glob::Pattern in watcher module**

```bash
grep -rn "glob::Pattern" src/watcher/
```

Expected: No matches.

- [ ] **Step 3: Verify the old `build_ignore_patterns` is fully removed**

```bash
grep -rn "build_ignore_patterns" src/
```

Expected: No matches (function fully removed from all files).

- [ ] **Step 4: Update TODO.md**

Mark the `.gitignore` watcher item as done:

```markdown
- [x] **Watcher doesn't respect `.gitignore`** — fixed: replaced hardcoded `glob::Pattern` list with `ignore` crate's `Gitignore` matcher built from root `.gitignore` + `.julieignore` + synthetic patterns. Uses `matched_path_or_any_parents()` for directory-aware matching. `BLACKLISTED_DIRECTORIES` kept as independent safety net. (`src/watcher/filtering.rs`, `src/watcher/events.rs`, `src/watcher/mod.rs`)
```

- [ ] **Step 5: Final commit**

```bash
git add TODO.md
git commit -m "docs: mark watcher .gitignore support as complete in TODO.md"
```
