//! Build script: ensures `ui/dist/` is up-to-date before `cargo build`.
//!
//! The `#[derive(Embed)] #[folder = "ui/dist/"]` macro in `src/ui.rs` requires
//! the folder to physically exist at compile time. On a fresh clone (or CI
//! without a prior `npm run build`), the macro fails with a confusing error.
//!
//! This script:
//! 1. Checks if `ui/dist/index.html` exists
//! 2. If missing OR stale (source files newer than dist), runs `npm run build`
//! 3. Falls back to creating an empty `ui/dist/` with a stub `index.html`
//!    if npm/node isn't available

use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

fn main() {
    // Rerun when Vue source files or package.json change
    println!("cargo:rerun-if-changed=ui/src");
    println!("cargo:rerun-if-changed=ui/package.json");
    println!("cargo:rerun-if-changed=ui/vite.config.ts");

    let dist_index = Path::new("ui/dist/index.html");
    if dist_index.exists() && !is_dist_stale(dist_index) {
        return;
    }

    let reason = if dist_index.exists() { "stale" } else { "missing" };
    eprintln!("ui/dist/ {reason} — building UI...");

    // Try npm
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };

    // Only run npm install if node_modules is missing
    let needs_install = !Path::new("ui/node_modules").exists();
    let install_ok = if needs_install {
        Command::new(npm)
            .args(["install"])
            .current_dir("ui")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    } else {
        true
    };

    if install_ok {
        let build_ok = Command::new(npm)
            .args(["run", "build"])
            .current_dir("ui")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if build_ok {
            eprintln!("UI built successfully.");
            return;
        }
        eprintln!("npm run build failed — creating stub UI.");
    } else {
        eprintln!("npm not available — creating stub UI.");
    }

    // Fallback: create a minimal stub so the Embed macro doesn't fail
    std::fs::create_dir_all("ui/dist").expect("Failed to create ui/dist/");
    std::fs::write(
        "ui/dist/index.html",
        r#"<!DOCTYPE html>
<html>
<head><title>Julie</title></head>
<body style="font-family: system-ui; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #1a1a2e; color: #e0e0e0;">
<div style="text-align: center;">
<h1>Julie Dashboard</h1>
<p>UI assets not built. Run <code>cd ui && npm install && npm run build</code> then rebuild.</p>
</div>
</body>
</html>"#,
    )
    .expect("Failed to write stub index.html");
    eprintln!("Created stub ui/dist/index.html — dashboard will show placeholder.");
}

/// Check if any file in `ui/src/` is newer than the dist output.
fn is_dist_stale(dist_index: &Path) -> bool {
    let dist_mtime = match dist_index.metadata().and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return true, // can't read mtime → treat as stale
    };

    newest_mtime_in(Path::new("ui/src"))
        .map(|src_mtime| src_mtime > dist_mtime)
        .unwrap_or(false) // if we can't walk src, don't force a rebuild
}

/// Recursively find the newest modification time in a directory.
fn newest_mtime_in(dir: &Path) -> Option<SystemTime> {
    let mut newest: Option<SystemTime> = None;
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let mtime = if path.is_dir() {
            newest_mtime_in(&path)
        } else {
            path.metadata().and_then(|m| m.modified()).ok()
        };
        if let Some(t) = mtime {
            newest = Some(newest.map_or(t, |n: SystemTime| n.max(t)));
        }
    }
    newest
}
