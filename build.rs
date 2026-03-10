//! Build script: ensures `ui/dist/` exists before `cargo build`.
//!
//! The `#[derive(Embed)] #[folder = "ui/dist/"]` macro in `src/ui.rs` requires
//! the folder to physically exist at compile time. On a fresh clone (or CI
//! without a prior `npm run build`), the macro fails with a confusing error.
//!
//! This script:
//! 1. Checks if `ui/dist/index.html` exists
//! 2. If missing, runs `npm install && npm run build` in `ui/`
//! 3. Falls back to creating an empty `ui/dist/` with a stub `index.html`
//!    if npm/node isn't available

use std::path::Path;
use std::process::Command;

fn main() {
    // Only rerun if the dist directory or package.json changes
    println!("cargo:rerun-if-changed=ui/dist/index.html");
    println!("cargo:rerun-if-changed=ui/package.json");

    let dist_index = Path::new("ui/dist/index.html");
    if dist_index.exists() {
        return;
    }

    eprintln!("ui/dist/ not found — building UI...");

    // Try npm
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };

    let install_ok = Command::new(npm)
        .args(["install"])
        .current_dir("ui")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

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
