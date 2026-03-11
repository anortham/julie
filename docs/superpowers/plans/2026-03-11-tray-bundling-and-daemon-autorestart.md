# Tray Server Bundling & Daemon Auto-Restart Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Julie tray app self-contained (bundled server binary) and eliminate the developer restart dance (daemon auto-restarts when its binary is rebuilt).

**Architecture:** Two independent features. Feature 1 uses Tauri's `externalBin` sidecar mechanism to bundle `julie-server` inside the tray DMG/installer, with a CI workflow that builds the server first and copies it into the Tauri build tree. Feature 2 adds a lightweight background task to the daemon that polls its own binary's mtime every 5 seconds and triggers graceful shutdown when a newer binary is detected — the existing `connect` reconnect logic then automatically restarts with the new binary.

**Tech Stack:** Rust, Tauri v2 (externalBin/sidecar), GitHub Actions, tokio (spawn, interval), std::fs::metadata (mtime)

---

## Chunk 1: Bundle Server Binary in Tray App

### File Structure

| Action | Path | Purpose |
|--------|------|---------|
| Create | `tauri-app/src-tauri/binaries/.gitkeep` | Directory for platform-suffixed server binaries (populated at build time) |
| Modify | `tauri-app/src-tauri/tauri.conf.json` | Add `externalBin` config |
| Modify | `tauri-app/src-tauri/Cargo.toml` | Add `tauri-plugin-shell` dependency |
| Modify | `tauri-app/src-tauri/src/main.rs` | Register shell plugin |
| Modify | `tauri-app/src-tauri/src/daemon_client.rs` | Add bundled sidecar path to binary discovery |
| Modify | `.github/workflows/release.yml` | Build server → copy into tray build tree → build tray |
| Create | `scripts/copy-server-for-tray.sh` | Helper to copy local dev build into tray binaries dir |

---

### Task 1: Configure Tauri `externalBin` sidecar

**Files:**
- Create: `tauri-app/src-tauri/binaries/.gitkeep`
- Modify: `tauri-app/src-tauri/tauri.conf.json`
- Modify: `tauri-app/src-tauri/Cargo.toml:~line 8` (dependencies)
- Modify: `tauri-app/src-tauri/src/main.rs`

- [ ] **Step 1: Create the binaries directory with .gitkeep**

```bash
mkdir -p tauri-app/src-tauri/binaries
touch tauri-app/src-tauri/binaries/.gitkeep
```

The actual binaries are build artifacts — only `.gitkeep` is committed.

- [ ] **Step 2: Add `externalBin` to tauri.conf.json**

Tauri v2 requires the binary name (without platform suffix) in the `bundle.externalBin` array. At build time, Tauri looks for `binaries/julie-server-{target-triple}[.exe]`.

```json
{
  "productName": "Julie",
  "version": "4.2.0",
  "identifier": "com.julie.tray",
  "build": {
    "beforeDevCommand": "",
    "beforeBuildCommand": "",
    "frontendDist": "../src"
  },
  "app": {
    "windows": [],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "externalBin": [
      "binaries/julie-server"
    ],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico",
      "icons/icon.png"
    ]
  }
}
```

- [ ] **Step 3: Add `tauri-plugin-shell` dependency to tray Cargo.toml**

The shell plugin provides Tauri's `app.shell().sidecar()` API for spawning bundled binaries. We don't use that API directly (we use `std::process::Command` via `find_julie_binary()`), but including the plugin ensures forward compatibility and is required by some Tauri bundler versions for `externalBin` to work.

Add to `[dependencies]`:
```toml
tauri-plugin-shell = "2"
```

- [ ] **Step 4: Register the shell plugin in main.rs**

In `tauri-app/src-tauri/src/main.rs`, add `.plugin(tauri_plugin_shell::init())` to the Tauri builder chain (alongside the existing `tauri_plugin_autostart`, `tauri_plugin_notification`, etc.).

- [ ] **Step 5: Commit**

```bash
git add tauri-app/src-tauri/binaries/.gitkeep tauri-app/src-tauri/tauri.conf.json \
       tauri-app/src-tauri/Cargo.toml tauri-app/src-tauri/src/main.rs
git commit -m "feat(tray): configure Tauri externalBin sidecar for julie-server"
```

---

### Task 2: Update binary discovery to find the bundled sidecar

**Files:**
- Modify: `tauri-app/src-tauri/src/daemon_client.rs:179-206` (`find_julie_binary`)

The current search order is: `~/.julie/bin/` → adjacent to exe → PATH. We need to add the Tauri sidecar resolution as a fallback. The search order becomes:

1. `~/.julie/bin/julie-server` — user/dev override (highest priority)
2. Adjacent to tray binary — covers Tauri sidecar location on macOS (`Julie.app/Contents/MacOS/`)
3. PATH lookup

The key insight: Tauri's `externalBin` places the sidecar binary **adjacent to the main executable** inside the app bundle. On macOS, that's `Julie.app/Contents/MacOS/julie-server-aarch64-apple-darwin`. On Windows, it's next to the `.exe`. So step 2 ("adjacent to tray app") already covers this — but the binary will have a platform-triple suffix.

- [ ] **Step 1: Write the failing test**

Create a test in `daemon_client.rs` (the tray crate uses inline tests per its existing convention — `updates.rs` already has `mod tests` inline):

```rust
#[cfg(test)]
mod find_binary_tests {
    use super::*;
    use std::fs;

    /// Test the sidecar name lookup in isolation (avoids ~/.julie/bin/ short-circuit).
    /// This directly tests `find_sidecar_adjacent()` rather than the full
    /// `find_julie_binary()` chain, so it works even when ~/.julie/bin/julie-server exists.
    #[test]
    fn test_find_sidecar_adjacent_to_exe() {
        let exe = std::env::current_exe().unwrap();
        let exe_dir = exe.parent().unwrap();

        let suffix = current_target_triple();
        let sidecar_name = if cfg!(windows) {
            format!("julie-server-{}.exe", suffix)
        } else {
            format!("julie-server-{}", suffix)
        };

        let sidecar_path = exe_dir.join(&sidecar_name);
        fs::write(&sidecar_path, b"fake-binary").unwrap();

        // Test the sidecar lookup directly
        let result = find_sidecar_adjacent();
        let path = result.expect("Should find sidecar binary adjacent to exe");
        assert!(
            path.file_name().unwrap().to_str().unwrap().contains("julie-server"),
            "Found path should contain 'julie-server', got: {:?}", path
        );

        // Cleanup
        let _ = fs::remove_file(&sidecar_path);
    }
}
```

**Note:** This test calls `find_sidecar_adjacent()` (a helper extracted in Step 3) rather than `find_julie_binary()`, so it isn't short-circuited by `~/.julie/bin/julie-server` existing on the dev machine.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p julie-tray test_find_binary_adjacent_with_triple_suffix 2>&1 | tail -10`
Expected: FAIL — `current_target_triple()` doesn't exist yet and the search doesn't check suffixed names.

- [ ] **Step 3: Implement the sidecar-aware binary discovery**

Update `find_julie_binary()` in `daemon_client.rs`. Extract the sidecar lookup into a helper so it can be tested independently (without `~/.julie/bin/` short-circuiting):

```rust
/// Return the target triple for the current platform at compile time.
fn current_target_triple() -> &'static str {
    env!("TARGET_TRIPLE")  // Set by build.rs
}

/// Look for the sidecar binary adjacent to this executable.
/// Tries plain name first, then Tauri's target-triple-suffixed name.
#[cfg_attr(test, allow(dead_code))]
fn find_sidecar_adjacent() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;

    let bin_name = if cfg!(windows) { "julie-server.exe" } else { "julie-server" };

    // Try plain name first
    let adjacent = dir.join(bin_name);
    if adjacent.exists() {
        return Some(adjacent);
    }

    // Try Tauri sidecar name (with target triple suffix)
    let sidecar_name = if cfg!(windows) {
        format!("julie-server-{}.exe", current_target_triple())
    } else {
        format!("julie-server-{}", current_target_triple())
    };
    let sidecar = dir.join(&sidecar_name);
    if sidecar.exists() {
        return Some(sidecar);
    }

    None
}

/// Find the `julie-server` binary. Search order:
/// 1. `~/.julie/bin/julie-server` (user/dev override — highest priority)
/// 2. Adjacent to this tray binary (Tauri sidecar location) — tries both
///    plain name and target-triple-suffixed name
/// 3. PATH lookup
pub fn find_julie_binary() -> Option<PathBuf> {
    let bin_name = if cfg!(windows) { "julie-server.exe" } else { "julie-server" };

    // 1. User/dev override location
    if let Ok(home) = julie_home() {
        let installed = home.join("bin").join(bin_name);
        if installed.exists() {
            return Some(installed);
        }
    }

    // 2. Adjacent to tray app (covers Tauri sidecar bundling)
    if let Some(path) = find_sidecar_adjacent() {
        return Some(path);
    }

    // 3. PATH lookup
    which_in_path(bin_name)
}
```

Also create/update `tauri-app/src-tauri/build.rs` to pass the target triple:

```rust
fn main() {
    // Pass target triple to the binary for sidecar resolution
    println!("cargo:rustc-env=TARGET_TRIPLE={}", std::env::var("TARGET").unwrap());
    tauri_build::build();
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p julie-tray test_find_binary_adjacent_with_triple_suffix 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tauri-app/src-tauri/src/daemon_client.rs tauri-app/src-tauri/build.rs
git commit -m "feat(tray): find bundled sidecar binary with target-triple suffix"
```

---

### Task 3: Create local dev helper script

**Files:**
- Create: `scripts/copy-server-for-tray.sh`

During development, the tray needs a server binary in `binaries/` to build. This script copies the local debug or release build.

- [ ] **Step 1: Write the script**

```bash
#!/usr/bin/env bash
# Copy the locally-built julie-server into the tray sidecar directory
# so `npx tauri dev` or `npx tauri build` can find it.
#
# Usage: ./scripts/copy-server-for-tray.sh [--release]

set -euo pipefail

PROFILE="debug"
if [[ "${1:-}" == "--release" ]]; then
    PROFILE="release"
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="${REPO_ROOT}/target"
BINARIES_DIR="${REPO_ROOT}/tauri-app/src-tauri/binaries"

# Detect current platform triple
case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)  TRIPLE="aarch64-apple-darwin" ;;
    Darwin-x86_64) TRIPLE="x86_64-apple-darwin" ;;
    Linux-x86_64)  TRIPLE="x86_64-unknown-linux-gnu" ;;
    *)             echo "Unsupported platform: $(uname -s)-$(uname -m)"; exit 1 ;;
esac

SRC="${TARGET_DIR}/${PROFILE}/julie-server"
if [[ ! -f "$SRC" ]]; then
    echo "Server binary not found at ${SRC}"
    if [[ "$PROFILE" == "release" ]]; then
        echo "Run: cargo build --release --bin julie-server"
    else
        echo "Run: cargo build --bin julie-server"
    fi
    exit 1
fi

DEST="${BINARIES_DIR}/julie-server-${TRIPLE}"
mkdir -p "$BINARIES_DIR"
cp "$SRC" "$DEST"
echo "Copied: ${SRC} → ${DEST}"
```

- [ ] **Step 2: Make executable and add .gitignore for binaries**

```bash
chmod +x scripts/copy-server-for-tray.sh
```

Add to `tauri-app/src-tauri/binaries/.gitignore`:
```
*
!.gitkeep
!.gitignore
```

This keeps build artifacts out of git while preserving the directory.

- [ ] **Step 3: Commit**

```bash
git add scripts/copy-server-for-tray.sh tauri-app/src-tauri/binaries/.gitignore
git commit -m "chore(tray): add dev helper to copy server binary for sidecar builds"
```

---

### Task 4: Update release workflow to build server before tray

**Files:**
- Modify: `.github/workflows/release.yml`

The current workflow builds `build` (server) and `build-tray` independently. We need `build-tray` to depend on `build` so it can download the server artifact and place it in the sidecar directory before `npx tauri build`.

- [ ] **Step 1: Make `build-tray` depend on `build`**

Add `needs: [build]` to the `build-tray` job.

- [ ] **Step 2: Add step to download and place server binary**

Insert these steps in the `build-tray` job, **after** "Generate bundle icons" and **before** "Build tray app".

First, add `server_archive_ext` to the matrix so we can compute the exact artifact name:

```yaml
        include:
          - os: macos-latest
            label: macOS
            target: aarch64-apple-darwin
            bundles: dmg
            server_archive_ext: tar.gz
          - os: windows-latest
            label: Windows
            target: x86_64-pc-windows-msvc
            bundles: nsis
            server_archive_ext: zip
          - os: ubuntu-latest
            label: Linux
            target: x86_64-unknown-linux-gnu
            bundles: appimage,deb
            server_archive_ext: tar.gz
```

Then add the download + placement steps. **Critical:** `download-artifact@v4` requires the **exact** artifact name — no globs. The server `build` job uploads with `name: ${{ env.ARCHIVE }}` where `ARCHIVE` is `julie-v${VERSION}-${target}.{ext}`. We compute the same name:

```yaml
      - name: Compute server artifact name
        shell: bash
        run: |
          VERSION=${GITHUB_REF#refs/tags/v}
          echo "SERVER_ARTIFACT=julie-v${VERSION}-${{ matrix.target }}.${{ matrix.server_archive_ext }}" >> $GITHUB_ENV

      - name: Download server binary for bundling
        uses: actions/download-artifact@v4
        with:
          name: ${{ env.SERVER_ARTIFACT }}
          path: server-artifact

      - name: Place server binary as Tauri sidecar
        shell: bash
        run: |
          mkdir -p tauri-app/src-tauri/binaries
          if [ "${{ matrix.os }}" = "windows-latest" ]; then
            cd server-artifact && 7z x *.zip -y && cd ..
            cp server-artifact/julie-server.exe \
               "tauri-app/src-tauri/binaries/julie-server-${{ matrix.target }}.exe"
          else
            tar -xzf server-artifact/*.tar.gz -C server-artifact
            cp server-artifact/julie-server \
               "tauri-app/src-tauri/binaries/julie-server-${{ matrix.target }}"
          fi
```

The tray and server targets are identical (`aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`), so the artifact name computation works directly.

- [ ] **Step 3: Verify locally that `npx tauri build` picks up the sidecar**

```bash
# Build server
cargo build --release --bin julie-server
# Copy into sidecar dir
./scripts/copy-server-for-tray.sh --release
# Build tray (will fail if sidecar not found)
cd tauri-app && npx tauri build --bundles dmg
```

Check the DMG contents to confirm `julie-server-aarch64-apple-darwin` is inside the app bundle.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: bundle julie-server binary inside tray app installers"
```

---

### Task 5: Fix the quick bugs from the GPT review

**Files:**
- Modify: `tauri-app/src-tauri/src/updates.rs:22-23` (repo URL)
- Modify: `tauri-app/src-tauri/Cargo.toml:3` (version)
- Modify: `tauri-app/src-tauri/tauri.conf.json:3` (version)
- Modify: `ui/src/views/Dashboard.vue:~359` (diagnostics label)

- [ ] **Step 1: Fix updater repo URL**

In `updates.rs`, change:
```rust
const GITHUB_OWNER: &str = "anthropics";
const GITHUB_REPO: &str = "julie";
```
to:
```rust
const GITHUB_OWNER: &str = "anortham";
const GITHUB_REPO: &str = "julie";
```

- [ ] **Step 2: Sync versions to 4.2.1**

In `tauri-app/src-tauri/Cargo.toml`, change `version = "4.2.0"` → `version = "4.2.1"`
In `tauri-app/src-tauri/tauri.conf.json`, change `"version": "4.2.0"` → `"version": "4.2.1"`

- [ ] **Step 3: Fix dashboard diagnostics button label**

In `ui/src/views/Dashboard.vue:~359`, change:
```
{{ exportingDiagnostics ? 'Exporting...' : 'Export Diagnostic Bundle' }}
```
to:
```
{{ exportingDiagnostics ? 'Exporting...' : 'Export Diagnostic Report' }}
```

The dashboard endpoint exports JSON (a report), not a zip bundle. The tray app exports the actual bundle.

- [ ] **Step 4: Commit**

```bash
git add tauri-app/src-tauri/src/updates.rs tauri-app/src-tauri/Cargo.toml \
       tauri-app/src-tauri/tauri.conf.json ui/src/views/Dashboard.vue
git commit -m "fix(tray): correct updater repo URL, sync versions, fix diagnostics label"
```

---

## Chunk 2: Daemon Auto-Restart on Binary Change

**Relationship to existing code:** `src/daemon.rs:505` already has `is_binary_newer_than_daemon()`, which the `connect` command uses client-side to detect rebuilds on new connections. The new binary monitor is **daemon-side** — it runs inside the daemon process itself, polling every 5s regardless of client connections. The two mechanisms are complementary (belt and suspenders): the daemon monitor catches rebuilds immediately, while the existing `connect`-side check catches the edge case where the daemon is already running but the monitor hasn't detected the change yet (within its poll window). Neither needs modification of the other.

### File Structure

| Action | Path | Purpose |
|--------|------|---------|
| Create | `src/binary_monitor.rs` | Background task that polls binary mtime |
| Modify | `src/lib.rs` | Register new module |
| Modify | `src/daemon.rs:228-286` | Spawn monitor task during daemon startup |
| Create | `src/tests/binary_monitor_tests.rs` | Tests for the monitor |
| Modify | `src/tests/mod.rs` | Register test module |

---

### Task 6: Write the binary monitor module

**Files:**
- Create: `src/binary_monitor.rs`
- Modify: `src/lib.rs`
- Create: `src/tests/binary_monitor_tests.rs`
- Modify: `src/tests/mod.rs`

The monitor is a simple background task: every N seconds, stat the executable and compare mtime against the daemon's start time. If the binary is newer, cancel the `CancellationToken` to trigger graceful shutdown. The existing `connect` reconnect logic (`connect.rs:373-399`) then handles restarting with the new binary.

Design decisions:
- **Polling, not `notify` watching**: The daemon's own binary rarely changes (only on rebuild), so 5s polling is fine and avoids the complexity of inotify/FSEvents for a single file. The `notify` crate is available but overkill here.
- **Uses `CancellationToken`**: Already wired into the shutdown sequence at `server.rs:274`. Cancelling it stops MCP sessions, file watchers, and the HTTP server — exactly what we want.
- **Disabled via env var**: `JULIE_NO_BINARY_WATCH=1` skips the monitor. Useful if polling causes issues on NFS or exotic filesystems.

- [ ] **Step 1: Write the failing test**

Create `src/tests/binary_monitor_tests.rs`:

```rust
use std::path::PathBuf;
use std::time::{Instant, SystemTime, Duration};
use tokio_util::sync::CancellationToken;

/// Test that the monitor detects a binary newer than start_time
#[test]
fn test_binary_is_newer_detection() {
    // Create a temp file to act as our "binary"
    let dir = tempfile::tempdir().unwrap();
    let fake_binary = dir.path().join("julie-server");
    std::fs::write(&fake_binary, b"v1").unwrap();

    // Start time is "now" — binary mtime should be <= start_time
    let start_time = SystemTime::now();

    assert!(
        !crate::binary_monitor::is_binary_newer(&fake_binary, start_time),
        "Binary written before start_time should not be detected as newer"
    );

    // Sleep briefly, then "rebuild" the binary
    std::thread::sleep(Duration::from_millis(100));
    std::fs::write(&fake_binary, b"v2").unwrap();

    assert!(
        crate::binary_monitor::is_binary_newer(&fake_binary, start_time),
        "Binary written after start_time should be detected as newer"
    );
}

/// Test that the monitor triggers cancellation when binary changes
#[tokio::test]
async fn test_monitor_cancels_on_binary_change() {
    let dir = tempfile::tempdir().unwrap();
    let fake_binary = dir.path().join("julie-server");
    std::fs::write(&fake_binary, b"v1").unwrap();

    let ct = CancellationToken::new();
    let ct_clone = ct.clone();
    let binary_path = fake_binary.clone();

    // Start monitor with a fast poll interval for testing
    let handle = tokio::spawn(async move {
        crate::binary_monitor::run_monitor(
            binary_path,
            ct_clone,
            Duration::from_millis(50),
        ).await;
    });

    // Give monitor time to start and do its first (no-change) check
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!ct.is_cancelled(), "Should not be cancelled yet");

    // "Rebuild" the binary — sleep 500ms first to ensure mtime is clearly
    // after start_time (some filesystems have 1s mtime granularity)
    tokio::time::sleep(Duration::from_millis(500)).await;
    std::fs::write(&fake_binary, b"v2").unwrap();

    // Wait for monitor to detect and cancel (up to 500ms for poll + scheduling jitter)
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(ct.is_cancelled(), "Should be cancelled after binary change");

    let _ = handle.await;
}

/// Test that the monitor respects cancellation (doesn't hang)
#[tokio::test]
async fn test_monitor_stops_on_external_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let fake_binary = dir.path().join("julie-server");
    std::fs::write(&fake_binary, b"v1").unwrap();

    let ct = CancellationToken::new();
    let ct_clone = ct.clone();
    let binary_path = fake_binary.clone();

    let handle = tokio::spawn(async move {
        crate::binary_monitor::run_monitor(
            binary_path,
            ct_clone,
            Duration::from_millis(50),
        ).await;
    });

    // Cancel externally (simulating Ctrl+C shutdown)
    tokio::time::sleep(Duration::from_millis(100)).await;
    ct.cancel();

    // Monitor should exit promptly
    let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
    assert!(result.is_ok(), "Monitor should exit within 1s of cancellation");
}
```

Register in `src/tests/mod.rs`:
```rust
mod binary_monitor_tests;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib binary_monitor_tests 2>&1 | tail -10`
Expected: FAIL — `crate::binary_monitor` module doesn't exist.

- [ ] **Step 3: Implement the binary monitor**

Create `src/binary_monitor.rs`:

```rust
//! Background task that monitors the daemon's own binary for changes.
//!
//! When a rebuild is detected (binary mtime > daemon start time), the
//! monitor triggers graceful shutdown via `CancellationToken`. The
//! `connect` command's reconnect logic then restarts with the new binary.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Check whether the binary at `path` has been modified after `start_time`.
pub fn is_binary_newer(path: &Path, start_time: SystemTime) -> bool {
    match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(mtime) => mtime > start_time,
        Err(e) => {
            debug!("Could not stat binary {:?}: {}", path, e);
            false
        }
    }
}

/// Poll the binary's mtime and cancel the token when a newer build is detected.
///
/// This function runs until either:
/// - A newer binary is detected (cancels `ct` and returns)
/// - The `ct` is cancelled externally (returns without action)
pub async fn run_monitor(
    binary_path: PathBuf,
    ct: CancellationToken,
    poll_interval: Duration,
) {
    let start_time = SystemTime::now();
    info!(
        "Binary monitor started: watching {:?} every {:?}",
        binary_path, poll_interval
    );

    let mut interval = tokio::time::interval(poll_interval);
    interval.tick().await; // First tick is immediate — skip it

    loop {
        tokio::select! {
            _ = ct.cancelled() => {
                debug!("Binary monitor: shutdown requested, exiting");
                return;
            }
            _ = interval.tick() => {
                if is_binary_newer(&binary_path, start_time) {
                    info!("Binary change detected — initiating graceful shutdown for restart");
                    ct.cancel();
                    return;
                }
            }
        }
    }
}

/// Spawn the binary monitor as a background task.
///
/// Returns `None` if:
/// - `JULIE_NO_BINARY_WATCH=1` is set
/// - The current executable path cannot be determined
pub fn spawn(ct: CancellationToken) -> Option<tokio::task::JoinHandle<()>> {
    if std::env::var("JULIE_NO_BINARY_WATCH").unwrap_or_default() == "1" {
        info!("Binary monitor disabled via JULIE_NO_BINARY_WATCH=1");
        return None;
    }

    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            warn!("Cannot monitor binary for changes: {}", e);
            return None;
        }
    };

    let ct_clone = ct.clone();
    Some(tokio::spawn(async move {
        run_monitor(exe_path, ct_clone, DEFAULT_POLL_INTERVAL).await;
    }))
}
```

Register in `src/lib.rs`:
```rust
pub mod binary_monitor;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib binary_monitor_tests 2>&1 | tail -10`
Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
git add src/binary_monitor.rs src/lib.rs src/tests/binary_monitor_tests.rs src/tests/mod.rs
git commit -m "feat: add binary monitor that detects rebuilds for daemon auto-restart"
```

---

### Task 7: Integrate monitor into daemon startup

**Files:**
- Modify: `src/daemon.rs:228-286` (`daemon_start`)
- Modify: `src/server.rs:64-73` (`start_server` signature)
- Modify: `src/tests/server_tests.rs:315-321` (update call site)
- Modify: `src/tests/binary_monitor_tests.rs` (add `spawn` tests)

- [ ] **Step 1: Write a test that verifies daemon startup spawns the monitor**

This is an integration-level concern. The unit tests in Task 6 already verify the monitor's behavior. Here we just need to verify the wiring. Add to `binary_monitor_tests.rs`:

```rust
use serial_test::serial;

/// Verify spawn() returns a handle when JULIE_NO_BINARY_WATCH is unset.
/// Uses #[serial] because it mutates process-global env vars.
#[tokio::test]
#[serial]
async fn test_spawn_returns_handle() {
    // Ensure env var is not set
    std::env::remove_var("JULIE_NO_BINARY_WATCH");

    let ct = CancellationToken::new();
    let handle = crate::binary_monitor::spawn(ct.clone());
    assert!(handle.is_some(), "spawn() should return a JoinHandle");

    // Clean up: cancel so the spawned task exits
    ct.cancel();
    if let Some(h) = handle {
        let _ = tokio::time::timeout(Duration::from_secs(1), h).await;
    }
}

/// Verify spawn() returns None when disabled.
/// Uses #[serial] because it mutates process-global env vars.
#[tokio::test]
#[serial]
async fn test_spawn_disabled_via_env() {
    std::env::set_var("JULIE_NO_BINARY_WATCH", "1");
    let ct = CancellationToken::new();
    let handle = crate::binary_monitor::spawn(ct);
    assert!(handle.is_none(), "spawn() should return None when disabled");
    std::env::remove_var("JULIE_NO_BINARY_WATCH");
}
```

**Note:** `serial_test = "3.2"` is already in the workspace `Cargo.toml` dev-dependencies. The `#[serial]` attribute ensures these tests don't race with each other when mutating `JULIE_NO_BINARY_WATCH`.

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib binary_monitor_tests 2>&1 | tail -10`
Expected: PASS (5 tests)

- [ ] **Step 3: Wire the monitor into `daemon_start`**

In `src/daemon.rs`, modify `daemon_start()`. The monitor needs to share the same `CancellationToken` that the server uses. Currently, the token is created inside `start_server()`. We need to either:
- (a) Create the token in `daemon_start` and pass it in, or
- (b) Return the token from `start_server` before awaiting shutdown

Option (a) is cleaner. Modify `daemon_start` to create the `CancellationToken` and pass it to both the monitor and the server.

In `daemon.rs`, after `lock_and_write_pid_file` and before `start_server`:

```rust
    // Create cancellation token shared between server and binary monitor.
    // When either side cancels (signal handler OR binary change), both shut down.
    let cancellation_token = CancellationToken::new();

    // Start binary monitor — detects rebuilds and triggers graceful restart
    let _binary_monitor = crate::binary_monitor::spawn(cancellation_token.clone());

    // Start the HTTP server — runs until shutdown signal or binary change
    let server_result = crate::server::start_server(
        port,
        workspace_root,
        shutdown_signal(),
        registry,
        home.clone(),
        cancellation_token,  // NEW: pass in instead of creating internally
    )
    .await;
```

This requires modifying `start_server`'s signature to accept an external `CancellationToken` instead of creating its own. In `src/server.rs:64-73`:

Change `src/server.rs:64-73` from:
```rust
pub async fn start_server(
    port: u16,
    workspace_root: PathBuf,
    shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
    registry: GlobalRegistry,
    julie_home: PathBuf,
) -> Result<()> {
    let cancellation_token = CancellationToken::new();  // REMOVE this line
    let ct_for_shutdown = cancellation_token.clone();
```

To:
```rust
pub async fn start_server(
    port: u16,
    workspace_root: PathBuf,
    shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
    registry: GlobalRegistry,
    julie_home: PathBuf,
    cancellation_token: CancellationToken,  // NEW: accept from caller
) -> Result<()> {
    let ct_for_shutdown = cancellation_token.clone();
```

**Key:** The old `let cancellation_token = CancellationToken::new();` at line 73 must be **deleted**, not kept alongside the parameter. The token is now created in `daemon_start()` and passed in.

**Also update `src/tests/server_tests.rs:315-321`** — this is the only other call site:

Change from:
```rust
    let result = crate::server::start_server(
        port,
        workspace_root,
        std::future::pending(),
        GlobalRegistry::new(),
        temp_dir.path().to_path_buf(),
    )
```

To:
```rust
    let result = crate::server::start_server(
        port,
        workspace_root,
        std::future::pending(),
        GlobalRegistry::new(),
        temp_dir.path().to_path_buf(),
        CancellationToken::new(),
    )
```

Add `use tokio_util::sync::CancellationToken;` to the test file's imports if not already present.

Also update the shutdown signal handling to incorporate cancellation. The `shutdown_signal` future currently only listens for OS signals. We need the server to shut down on **either** OS signal **or** token cancellation. Modify the `with_graceful_shutdown` argument:

```rust
    let shutdown_ct = cancellation_token.clone();
    let combined_shutdown = async move {
        tokio::select! {
            _ = shutdown_signal => {},
            _ = shutdown_ct.cancelled() => {},
        }
    };

    let result = axum::serve(listener, app)
        .with_graceful_shutdown(combined_shutdown)
        .await
        .context("HTTP server error");
```

**Why the combined shutdown is needed:** Currently `ct.cancel()` is called at `server.rs:274` *after* the server stops (cleanup). We need the inverse: the binary monitor calls `cancel()`, and the server sees the cancellation as a shutdown trigger. The `tokio::select!` combining both signals handles this — whichever fires first (OS signal or binary monitor) shuts down the server.

- [ ] **Step 4: Run tests**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: PASS — all existing tests + new binary_monitor tests

- [ ] **Step 5: Commit**

```bash
git add src/daemon.rs src/server.rs src/tests/server_tests.rs src/tests/binary_monitor_tests.rs
git commit -m "feat: wire binary monitor into daemon startup for auto-restart on rebuild"
```

---

### Task 8: Increase startup health timeout

**Files:**
- Modify: `tauri-app/src-tauri/src/daemon_client.rs:26`
- Modify: `src/connect.rs:22`

The current backoff `[50, 100, 200, 400, 800, 1600, 2000]` sums to ~5.15s. With the daemon now potentially doing more work on startup (loading Tantivy indexes), extend to ~15s.

- [ ] **Step 1: Update both backoff constants**

In `tauri-app/src-tauri/src/daemon_client.rs:26`:
```rust
const HEALTH_BACKOFF_MS: &[u64] = &[100, 200, 400, 800, 1000, 2000, 2000, 2000, 2000, 2000, 2000];
```
Sum: 14,500ms (~14.5s)

In `src/connect.rs:22`:
```rust
pub(crate) const BACKOFF_MS: &[u64] = &[100, 200, 400, 800, 1000, 2000, 2000, 2000, 2000, 2000, 2000];
```

These should stay in sync.

- [ ] **Step 2: Commit**

```bash
git add tauri-app/src-tauri/src/daemon_client.rs src/connect.rs
git commit -m "fix: increase startup health check timeout from 5s to 15s"
```

---

## Summary

| Task | What | Feature |
|------|------|---------|
| 1 | Configure Tauri externalBin sidecar | Bundling |
| 2 | Sidecar-aware binary discovery | Bundling |
| 3 | Local dev helper script | Bundling |
| 4 | Release workflow: build server before tray | Bundling |
| 5 | Fix GPT review bugs (URL, versions, label) | Cleanup |
| 6 | Binary monitor module + tests | Auto-restart |
| 7 | Wire monitor into daemon startup | Auto-restart |
| 8 | Increase health timeout | Reliability |

**Tasks 1-5** are Chunk 1 (bundling). **Tasks 6-8** are Chunk 2 (auto-restart). The chunks are independent and can be worked on in parallel if desired.
