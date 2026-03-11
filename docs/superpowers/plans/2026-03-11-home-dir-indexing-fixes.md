# Home Directory Indexing Fixes

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix four bugs discovered from a user's Windows log where Julie indexed their entire home directory, crashed on locked files, watched system directories, and logged noise from ephemeral temp files.

**Architecture:** Four independent fixes targeting: (1) workspace root resolution rejecting `~/.julie/` global config as a workspace marker, (2) non-fatal file errors during discovery, (3) Windows system directory blacklisting, (4) watcher error level tuning for all file event types.

**Tech Stack:** Rust, std::fs, tracing, ignore crate

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `src/tools/workspace/paths.rs` | Add home directory guard to `find_workspace_root()` |
| Modify | `src/tools/workspace/discovery.rs` | Make `should_index_file` and `is_likely_text_file` return `Ok(false)` on file errors instead of propagating |
| Modify | `src/tools/shared.rs` | Add Windows system directories to `BLACKLISTED_DIRECTORIES` |
| Modify | `src/watcher/mod.rs` | Downgrade all 6 file event `error!` calls to `warn!` |
| Modify | `src/tests/core/workspace_init.rs` | Add test for home directory rejection |
| Modify | `src/tests/tools/workspace/discovery.rs` | Add test for graceful locked-file handling |

---

## Chunk 1: Fix find_workspace_root Home Directory Guard

### Task 1: Reject `~/.julie/` as workspace marker

**Files:**
- Modify: `src/tools/workspace/paths.rs:62-117`
- Modify: `src/tests/core/workspace_init.rs`

- [ ] **Step 1: Write a failing test for home directory rejection**

In `src/tests/core/workspace_init.rs`, add a test. The test must set `$HOME` (or `USERPROFILE` on Windows) to the fake home so `julie_home()` resolves to the fake `.julie/` dir. Use `#[serial]` to avoid env var races:

```rust
/// Test: find_workspace_root must not treat ~/.julie/ (global config) as a workspace marker.
///
/// Reproduces the bug where a user's entire home directory was indexed because
/// find_workspace_root() walked up and found ~/.julie/ (the daemon config dir).
#[test]
#[serial]
fn test_find_workspace_root_rejects_home_julie_dir() {
    use crate::tools::workspace::ManageWorkspaceTool;

    let temp = tempfile::tempdir().expect("Failed to create temp dir");

    // Simulate: home_dir/.julie/ exists (global config, NOT a workspace)
    let fake_home = temp.path();
    let global_julie = fake_home.join(".julie");
    fs::create_dir_all(global_julie.join("logs")).expect("create .julie/logs");
    fs::write(global_julie.join("registry.toml"), "").expect("create registry.toml");

    // Simulate: home_dir/projects/myapp/ — the actual working directory (no markers)
    let working_dir = fake_home.join("projects").join("myapp");
    fs::create_dir_all(&working_dir).expect("create working dir");

    // Set HOME to fake_home so julie_home() returns fake_home/.julie
    let original_home = env::var("HOME").ok();
    env::set_var("HOME", fake_home);
    #[cfg(windows)]
    {
        let original_userprofile = env::var("USERPROFILE").ok();
        env::set_var("USERPROFILE", fake_home);
    }

    let tool = ManageWorkspaceTool {
        operation: "test".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let result = tool.find_workspace_root(&working_dir);

    // Restore HOME
    match original_home {
        Some(h) => env::set_var("HOME", h),
        None => env::remove_var("HOME"),
    }
    #[cfg(windows)]
    {
        match original_userprofile {
            Some(h) => env::set_var("USERPROFILE", h),
            None => env::remove_var("USERPROFILE"),
        }
    }

    // Should NOT resolve to fake_home (the "home directory")
    // It should find no valid marker and return working_dir as-is
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_ne!(
        resolved,
        fake_home.to_path_buf(),
        "find_workspace_root must not resolve to a directory whose .julie/ is the global config dir"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_find_workspace_root_rejects_home_julie_dir 2>&1 | tail -20`
Expected: FAIL — currently `find_workspace_root` WILL resolve to `fake_home` because it finds `.julie/` there and has no home-dir guard.

- [ ] **Step 3: Implement the fix in `find_workspace_root()`**

In `src/tools/workspace/paths.rs`:

First, add the import at the top of the file (after existing imports):
```rust
use crate::daemon::julie_home;
```

Then add a helper function to check if a `.julie` path is the global config dir. Insert this just before `find_workspace_root`:

```rust
    /// Returns true if the given `.julie` directory path matches the global config dir (~/.julie/).
    fn is_global_julie_dir(julie_dir_path: &Path, global_julie_home: &Option<PathBuf>) -> bool {
        global_julie_home.as_ref().map_or(false, |home| {
            julie_dir_path
                .canonicalize()
                .unwrap_or_else(|_| julie_dir_path.to_path_buf())
                == home
                    .canonicalize()
                    .unwrap_or_else(|_| home.clone())
        })
    }
```

Then modify `find_workspace_root()` with these surgical changes:

**Change 1:** After the `workspace_markers` array (line 70), add:
```rust
    // Determine the global julie home path (e.g., ~/.julie) to avoid treating it as a workspace.
    // If we can't determine it, we'll skip this guard (better to over-match than crash).
    let global_julie_home = julie_home().ok();
```

**Change 2:** In the start_path `.julie` check (lines 76-85), wrap the early return with a guard:
```rust
    let julie_dir = start_path.join(".julie");
    if julie_dir.exists() && julie_dir.is_dir() {
        if Self::is_global_julie_dir(&julie_dir, &global_julie_home) {
            debug!(
                "Skipping global ~/.julie at {}, not a workspace marker",
                start_path.display()
            );
        } else {
            debug!(
                "Found .julie directory at provided path: {}",
                start_path.display()
            );
            info!(
                "🎯 Found .julie directory at provided path: {}",
                start_path.display()
            );
            return Ok(start_path.to_path_buf());
        }
    }
```

**Change 3:** In the walk-up loop, after `if marker_path.exists() {` (line 95), add a guard for `.julie`:
```rust
                if marker_path.exists() {
                    // Guard: skip .julie if it's the global config dir (~/.julie/)
                    if *marker == ".julie"
                        && Self::is_global_julie_dir(&marker_path, &global_julie_home)
                    {
                        debug!(
                            "Skipping global ~/.julie at {}, not a workspace marker",
                            current_path.display()
                        );
                        continue;
                    }
                    info!(
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_find_workspace_root_rejects_home_julie_dir 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Run the existing workspace_init tests to check for regressions**

Run: `cargo test --lib tests::core::workspace_init 2>&1 | tail -20`
Expected: All existing tests pass

- [ ] **Step 6: Commit**

```bash
git add src/tools/workspace/paths.rs src/tests/core/workspace_init.rs
git commit -m "fix: reject ~/.julie/ global config dir as workspace marker

find_workspace_root() walked up the directory tree and treated the global
~/.julie/ config directory as a workspace marker, causing the user's entire
home directory to be indexed. Now compares against julie_home() and skips
the global config directory."
```

---

## Chunk 2: Make File Discovery Errors Non-Fatal

### Task 2: Gracefully skip unreadable files during discovery

**Files:**
- Modify: `src/tools/workspace/discovery.rs:88-165`
- Modify: `src/tests/tools/workspace/discovery.rs`

- [ ] **Step 1: Write a failing test for graceful locked-file handling**

In `src/tests/tools/workspace/discovery.rs`, add the necessary imports at the top of the file (after existing imports):
```rust
use crate::tools::shared::BLACKLISTED_EXTENSIONS;
use std::collections::HashSet;
```

Then add the test:

```rust
/// Test: should_index_file returns Ok(false) for unreadable files instead of Err
///
/// Reproduces the bug where a Dropbox-locked file (os error 32 on Windows) caused
/// the entire indexing operation to fail.
#[test]
fn test_should_index_file_skips_unreadable_files() {
    let tool = create_tool();
    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    let max_file_size = 1024 * 1024;

    // A file path that doesn't exist — simulates an inaccessible/locked file
    // Use no extension so it hits both metadata check AND is_likely_text_file
    let nonexistent = PathBuf::from("/tmp/julie_test_nonexistent_file_no_extension");
    let result = tool.should_index_file(&nonexistent, &blacklisted_exts, max_file_size, false);

    // Should return Ok(false), NOT Err
    assert!(
        result.is_ok(),
        "should_index_file should not error on unreadable files: {:?}",
        result.err()
    );
    assert!(
        !result.unwrap(),
        "should_index_file should return false for unreadable files"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_should_index_file_skips_unreadable_files 2>&1 | tail -20`
Expected: FAIL — `should_index_file` currently propagates the metadata error via `?`.

- [ ] **Step 3: Implement the fix**

In `src/tools/workspace/discovery.rs`, make two surgical edits:

**Edit 1:** In `should_index_file` (line 115-116), replace the `fs::metadata` call:

Old:
```rust
        let metadata = fs::metadata(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to get metadata for {:?}: {}", file_path, e))?;
```

New:
```rust
        // Skip files we can't stat (locked by another process, permissions, etc.)
        let metadata = match fs::metadata(file_path) {
            Ok(m) => m,
            Err(e) => {
                debug!("⏭️  Skipping inaccessible file {:?}: {}", file_path, e);
                return Ok(false);
            }
        };
```

**Edit 2:** In `is_likely_text_file` (lines 139-145), replace the `File::open` and `read` calls:

Old:
```rust
        let mut file = fs::File::open(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to open file {:?}: {}", file_path, e))?;

        let mut buffer = [0; 512];
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;
```

New:
```rust
        let mut file = match fs::File::open(file_path) {
            Ok(f) => f,
            Err(e) => {
                debug!("⏭️  Skipping inaccessible file {:?}: {}", file_path, e);
                return Ok(false);
            }
        };

        let mut buffer = [0; 512];
        let bytes_read = match file.read(&mut buffer) {
            Ok(n) => n,
            Err(e) => {
                debug!("⏭️  Skipping unreadable file {:?}: {}", file_path, e);
                return Ok(false);
            }
        };
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_should_index_file_skips_unreadable_files 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Run discovery tests for regressions**

Run: `cargo test --lib tests::tools::workspace::discovery 2>&1 | tail -20`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add src/tools/workspace/discovery.rs src/tests/tools/workspace/discovery.rs
git commit -m "fix: skip unreadable files during discovery instead of aborting

should_index_file() and is_likely_text_file() now return Ok(false) on I/O
errors (locked files, permission denied) instead of propagating Err. This
prevents a single locked Dropbox temp file from killing the entire indexing
operation."
```

---

## Chunk 3: Blacklist Windows System Directories

### Task 3: Add Windows system directory patterns to BLACKLISTED_DIRECTORIES

**Files:**
- Modify: `src/tools/shared.rs:43-93`
- Modify: `src/tests/tools/workspace/discovery.rs`

Note: `Library` is intentionally excluded from this change — it's used by Unity projects as a legitimate (though generated) directory, and by some other project types as real source. The home directory guard from Chunk 1 is the proper fix; this blacklist is defense-in-depth for Windows-specific system directories that are unambiguously non-code.

- [ ] **Step 1: Write a test for the new blacklist entries**

In `src/tests/tools/workspace/discovery.rs`, add a test verifying the new entries exist (following the existing `test_claude_dir_in_blacklist` pattern):

```rust
/// Test: Windows system directories are in BLACKLISTED_DIRECTORIES
#[test]
fn test_windows_system_dirs_in_blacklist() {
    assert!(
        BLACKLISTED_DIRECTORIES.contains(&"AppData"),
        "AppData should be in BLACKLISTED_DIRECTORIES"
    );
    assert!(
        BLACKLISTED_DIRECTORIES.contains(&"Application Data"),
        "Application Data should be in BLACKLISTED_DIRECTORIES"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_windows_system_dirs_in_blacklist 2>&1 | tail -20`
Expected: FAIL — entries don't exist yet.

- [ ] **Step 3: Add the entries to BLACKLISTED_DIRECTORIES**

In `src/tools/shared.rs`, add after the "Temporary and cache" section (after line ~82):

```rust
    // Windows system/app data directories (defense-in-depth for home dir indexing)
    "AppData",           // Windows: AppData\Local, AppData\Roaming, AppData\LocalLow
    "Application Data",  // Windows: legacy name for AppData
```

Also add the missing import in the test file if `BLACKLISTED_DIRECTORIES` isn't already imported (it was added in Chunk 2's Step 1).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_windows_system_dirs_in_blacklist 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Run fast tier tests for regressions**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add src/tools/shared.rs src/tests/tools/workspace/discovery.rs
git commit -m "fix: blacklist Windows AppData directories

Adds AppData and Application Data to BLACKLISTED_DIRECTORIES as
defense-in-depth. Prevents indexing Windows system files even if workspace
root resolution accidentally includes a home directory."
```

---

## Chunk 4: Downgrade Watcher File Event Errors

### Task 4: Change all file event error! calls to warn! in watcher

**Files:**
- Modify: `src/watcher/mod.rs` — lines 237, 281, 304, 349, 382, 404

There are 6 `error!` calls for file event failures in the watcher — covering file changes, deletions, and renames in both `start_watching()` and `process_pending_changes()`. All of these are non-fatal (the watcher loop continues), and all are subject to the same ephemeral file race condition on Windows. All 6 should be downgraded.

- [ ] **Step 1: Change all 6 error! calls to warn!**

In `src/watcher/mod.rs`, change each of these lines:

Line 237: `error!("Failed to handle file change: {}", e);` → `warn!("Failed to handle file change: {}", e);`
Line 281: `error!("Failed to handle file deletion: {}", e);` → `warn!("Failed to handle file deletion: {}", e);`
Line 304: `error!("Failed to handle file rename: {}", e);` → `warn!("Failed to handle file rename: {}", e);`
Line 349: `error!("Failed to handle file change: {}", e);` → `warn!("Failed to handle file change: {}", e);`
Line 382: `error!("Failed to handle file deletion: {}", e);` → `warn!("Failed to handle file deletion: {}", e);`
Line 404: `error!("Failed to handle file rename: {}", e);` → `warn!("Failed to handle file rename: {}", e);`

Rationale: All 6 are non-fatal — the watcher continues processing the next event. Ephemeral files (PowerShell policy tests, Dropbox sync temps, antivirus quarantine) on Windows disappear between detection and read, triggering any of these paths. `warn!` is the correct level for expected, recoverable failures.

- [ ] **Step 2: Verify `warn` is in the tracing imports**

Check the existing `use tracing::{...}` import at the top of `src/watcher/mod.rs` includes `warn`. If not, add it.

- [ ] **Step 3: Run fast tier tests**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/watcher/mod.rs
git commit -m "fix: downgrade all watcher file-event failures from error to warn

All 6 error! calls for file change/deletion/rename failures in both
start_watching() and process_pending_changes() are now warn!. These are
non-fatal (watcher continues), and ephemeral files on Windows trigger
them routinely. Reduces log noise."
```

---

## Final Verification

- [ ] **Run full fast tier to confirm no regressions**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: All tests pass

- [ ] **Verify the fix addresses the original log scenario**

The four fixes together prevent the cascade:
1. `find_workspace_root` would no longer resolve to `C:\Users\kalan` (rejects `~/.julie/`)
2. Even if it did, discovery wouldn't abort on the Dropbox locked file
3. Even if it scanned, `AppData` directories would be skipped by the blacklist
4. Ephemeral temp file failures would log as warnings, not errors
