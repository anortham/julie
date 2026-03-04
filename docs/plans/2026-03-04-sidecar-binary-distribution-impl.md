# Sidecar Binary Distribution Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Embed the Python sidecar source into the Julie binary so distributed binaries and `cargo install` work without a source checkout.

**Architecture:** Use `include_dir` to embed `python/embeddings_sidecar/` (excluding tests) at compile time. `sidecar_root_path()` gains a 4-level fallback chain: env override → adjacent to binary → source checkout → extract embedded files to cache. Extraction is versioned via `.embedded-version` marker.

**Tech Stack:** Rust, `include_dir` crate, existing `sidecar_supervisor.rs`

**Design doc:** `docs/plans/2026-03-04-sidecar-binary-distribution-design.md`

---

### Task 1: Add `include_dir` dependency

**Files:**
- Modify: `Cargo.toml:68` (utilities section)

**Step 1: Add the dependency**

In `Cargo.toml`, add `include_dir` to the utilities section (after `anyhow`):

```toml
include_dir = "0.7"
```

**Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: compiles cleanly

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add include_dir dependency for sidecar embedding"
```

---

### Task 2: Add the embedded sidecar directory and extraction function

**Files:**
- Modify: `src/embeddings/sidecar_supervisor.rs:1-8` (imports)
- Modify: `src/embeddings/sidecar_supervisor.rs:71-79` (sidecar_root_path)

This task adds:
1. A `static EMBEDDED_SIDECAR` using `include_dir!`
2. An `extract_embedded_sidecar()` function
3. A `managed_sidecar_source_path()` helper

**Step 1: Write the failing test**

Create `src/tests/core/sidecar_embedding_tests.rs`:

```rust
use std::path::Path;
use tempfile::TempDir;

use julie::embeddings::sidecar_supervisor::INSTALL_MARKER_VERSION;

// --- extract_embedded_sidecar tests ---

#[test]
fn test_extract_embedded_sidecar_writes_all_expected_files() {
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("source");

    julie::embeddings::sidecar_supervisor::extract_embedded_sidecar(&target)
        .expect("extraction should succeed");

    // Core files must exist
    assert!(target.join("pyproject.toml").exists());
    assert!(target.join("sidecar/__init__.py").exists());
    assert!(target.join("sidecar/main.py").exists());
    assert!(target.join("sidecar/runtime.py").exists());
    assert!(target.join("sidecar/protocol.py").exists());

    // Version marker must exist
    let marker = std::fs::read_to_string(target.join(".embedded-version")).unwrap();
    assert_eq!(marker.trim(), INSTALL_MARKER_VERSION);
}

#[test]
fn test_extract_embedded_sidecar_skips_when_version_matches() {
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("source");

    // First extraction
    julie::embeddings::sidecar_supervisor::extract_embedded_sidecar(&target).unwrap();
    let first_mtime = std::fs::metadata(target.join("pyproject.toml"))
        .unwrap()
        .modified()
        .unwrap();

    // Small delay so mtime would differ if re-written
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Second extraction — should skip
    julie::embeddings::sidecar_supervisor::extract_embedded_sidecar(&target).unwrap();
    let second_mtime = std::fs::metadata(target.join("pyproject.toml"))
        .unwrap()
        .modified()
        .unwrap();

    assert_eq!(first_mtime, second_mtime, "files should not be re-written when version matches");
}

#[test]
fn test_extract_embedded_sidecar_re_extracts_on_version_mismatch() {
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("source");

    // First extraction
    julie::embeddings::sidecar_supervisor::extract_embedded_sidecar(&target).unwrap();

    // Tamper with the version marker
    std::fs::write(target.join(".embedded-version"), "stale-version").unwrap();

    // Second extraction — should re-extract
    julie::embeddings::sidecar_supervisor::extract_embedded_sidecar(&target).unwrap();
    let marker = std::fs::read_to_string(target.join(".embedded-version")).unwrap();
    assert_eq!(marker.trim(), INSTALL_MARKER_VERSION);
}

#[test]
fn test_extract_does_not_include_test_files() {
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("source");

    julie::embeddings::sidecar_supervisor::extract_embedded_sidecar(&target).unwrap();

    // Test directory should NOT be extracted
    assert!(!target.join("tests").exists(), "test files should not be embedded");
}
```

Register the module in `src/tests/core/mod.rs` — add:

```rust
mod sidecar_embedding_tests;
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib sidecar_embedding 2>&1 | tail -10`
Expected: FAIL — `extract_embedded_sidecar` doesn't exist yet

**Step 3: Write the implementation**

In `src/embeddings/sidecar_supervisor.rs`, add the import at the top (after existing imports):

```rust
use include_dir::{Dir, include_dir};
```

Add the embedded directory static (after the constants, before `SidecarLaunchConfig`):

```rust
static EMBEDDED_SIDECAR: Dir = include_dir!("$CARGO_MANIFEST_DIR/python/embeddings_sidecar");
```

Add the extraction function (after `managed_cache_base_dir`, before `managed_venv_python_path`):

```rust
/// Path where embedded sidecar source is extracted in the cache dir.
fn managed_sidecar_source_path() -> PathBuf {
    managed_cache_base_dir()
        .join("embeddings")
        .join("sidecar")
        .join("source")
}

const EMBEDDED_VERSION_MARKER: &str = ".embedded-version";

/// Extract the embedded sidecar Python source to `target_dir` if needed.
///
/// Skips extraction when the version marker already matches `INSTALL_MARKER_VERSION`.
pub fn extract_embedded_sidecar(target_dir: &Path) -> Result<()> {
    let marker_path = target_dir.join(EMBEDDED_VERSION_MARKER);
    if marker_path.exists() {
        if let Ok(existing) = std::fs::read_to_string(&marker_path) {
            if existing.trim() == INSTALL_MARKER_VERSION {
                return Ok(());
            }
        }
    }

    // Extract all files (skip directories named "tests" and __pycache__)
    extract_dir_recursive(&EMBEDDED_SIDECAR, target_dir)?;

    std::fs::write(&marker_path, INSTALL_MARKER_VERSION).with_context(|| {
        format!(
            "failed to write embedded sidecar version marker '{}'",
            marker_path.display()
        )
    })?;

    Ok(())
}

fn extract_dir_recursive(dir: &Dir, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;

    for file in dir.files() {
        let file_path = target.join(file.path().file_name().unwrap());
        std::fs::write(&file_path, file.contents()).with_context(|| {
            format!("failed to extract embedded file '{}'", file_path.display())
        })?;
    }

    for subdir in dir.dirs() {
        let name = subdir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy();
        // Skip test and cache directories — not needed at runtime
        if name == "tests" || name == "__pycache__" {
            continue;
        }
        extract_dir_recursive(subdir, &target.join(name.as_ref()))?;
    }

    Ok(())
}
```

Update the public exports in `src/embeddings/mod.rs:140-142` to also export `extract_embedded_sidecar`:

```rust
pub use sidecar_supervisor::{
    SidecarLaunchConfig, build_sidecar_launch_config, extract_embedded_sidecar,
    managed_venv_path, sidecar_root_path,
};
```

**Step 4: Run test to verify it passes**

Run: `cargo test --lib sidecar_embedding 2>&1 | tail -10`
Expected: 4 tests PASS

**Step 5: Commit**

```bash
git add src/embeddings/sidecar_supervisor.rs src/embeddings/mod.rs src/tests/core/sidecar_embedding_tests.rs src/tests/core/mod.rs
git commit -m "feat: embed python sidecar source and add extraction function"
```

---

### Task 3: Update `sidecar_root_path()` with fallback chain

**Files:**
- Modify: `src/embeddings/sidecar_supervisor.rs:71-79` (sidecar_root_path)
- Modify: `src/embeddings/sidecar_supervisor.rs:30-31` (caller in build_sidecar_launch_config)

**Step 1: Write the failing test**

Add to `src/tests/core/sidecar_embedding_tests.rs`:

```rust
#[test]
fn test_sidecar_root_path_env_override_wins() {
    // When JULIE_EMBEDDING_SIDECAR_ROOT is set, it always wins
    let tmp = TempDir::new().unwrap();
    let override_path = tmp.path().join("custom_sidecar");
    std::fs::create_dir_all(&override_path).unwrap();

    // Temporarily set the env var
    std::env::set_var("JULIE_EMBEDDING_SIDECAR_ROOT", &override_path);
    let result = julie::embeddings::sidecar_supervisor::sidecar_root_path();
    std::env::remove_var("JULIE_EMBEDDING_SIDECAR_ROOT");

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), override_path);
}

#[test]
fn test_sidecar_root_path_falls_back_to_extraction() {
    // When no env var and no source checkout adjacent to binary,
    // it should fall back to extracting embedded files.
    // We can't easily test the full chain without mocking current_exe,
    // but we CAN test that extraction produces a valid sidecar root.
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("source");

    julie::embeddings::sidecar_supervisor::extract_embedded_sidecar(&target).unwrap();

    // The extracted dir should be a valid sidecar root
    assert!(target.join("pyproject.toml").exists());
    assert!(target.join("sidecar/main.py").exists());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib sidecar_embedding 2>&1 | tail -10`
Expected: FAIL — `sidecar_root_path()` returns `PathBuf`, not `Result<PathBuf>`

**Step 3: Update `sidecar_root_path()` with the full fallback chain**

Replace the current `sidecar_root_path()` (lines 71-79):

```rust
pub fn sidecar_root_path() -> Result<PathBuf> {
    // Priority 1: Env var override (dev/advanced users)
    if let Some(root_override) = std::env::var_os(SIDECAR_ROOT_ENV) {
        return Ok(PathBuf::from(root_override));
    }

    // Priority 2: Adjacent to binary (for packagers: Homebrew, scoop, distro packages)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let adjacent = exe_dir.join("python").join("embeddings_sidecar");
            if adjacent.join("pyproject.toml").exists() {
                return Ok(adjacent);
            }
        }
    }

    // Priority 3: Source checkout (dev mode — only works when running from source)
    let source_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("python")
        .join("embeddings_sidecar");
    if source_dir.join("pyproject.toml").exists() {
        return Ok(source_dir);
    }

    // Priority 4: Extract embedded files to cache directory
    let extracted = managed_sidecar_source_path();
    extract_embedded_sidecar(&extracted)?;
    Ok(extracted)
}
```

**Step 4: Update the caller to use `?`**

In `build_sidecar_launch_config()` (line 31), change:

```rust
let sidecar_root = sidecar_root_path();
```

to:

```rust
let sidecar_root = sidecar_root_path()?;
```

**Step 5: Run tests to verify they pass**

Run: `cargo test --lib sidecar_embedding 2>&1 | tail -10`
Expected: all tests PASS

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: full fast tier passes (no regressions)

**Step 6: Commit**

```bash
git add src/embeddings/sidecar_supervisor.rs
git commit -m "feat: sidecar_root_path fallback chain with embedded extraction"
```

---

### Task 4: Run full fast tier and fix any issues

**Step 1: Run fast tier**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -10`
Expected: all pass

**Step 2: Check the inline tests still pass**

The existing inline `mod tests` in `sidecar_supervisor.rs` (line 547) should still pass since we only changed the signature of `sidecar_root_path`. But if any test calls it directly, it will need `?` or `.unwrap()`.

Run: `cargo test --lib test_install_marker 2>&1 | tail -5`
Expected: PASS

**Step 3: Commit if any fixes were needed**

```bash
git add -u
git commit -m "fix: update tests for sidecar_root_path Result return type"
```

---

### Task 5: Manual verification

This task cannot be automated — it verifies the actual binary distribution scenario.

**Step 1: Build a release binary**

Run: `cargo build --release 2>&1 | tail -3`

**Step 2: Copy the binary to an isolated location**

```bash
cp target/release/julie-server /tmp/julie-test-binary
```

**Step 3: Run sidecar root resolution from the isolated binary**

The binary at `/tmp/julie-test-binary` has no source checkout. Verify it falls through to extraction:
- No `JULIE_EMBEDDING_SIDECAR_ROOT` env var set
- No `python/embeddings_sidecar/` adjacent to `/tmp/julie-test-binary`
- `CARGO_MANIFEST_DIR` path exists on this machine (since we built here) — but on a real user's machine it wouldn't

To fully test, temporarily rename the source dir:

```bash
mv python/embeddings_sidecar python/embeddings_sidecar.bak
# Run /tmp/julie-test-binary — embedding initialization should extract to cache
# Check: ls ~/.cache/julie/embeddings/sidecar/source/
mv python/embeddings_sidecar.bak python/embeddings_sidecar
```

**Step 4: Final commit with any polish**

```bash
git add -u
git commit -m "feat: complete sidecar binary distribution support"
```
