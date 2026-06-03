//! Dependency-direction tripwire for the `julie-core` leaf crate (see ADR-0006
//! and `docs/plans/2026-06-03-julie-rescue-design.md`).
//!
//! `julie-core` is the bottom leaf of the workspace. It must never reach UP into
//! the parent `julie` crate (handler / tools / daemon / indexing_core / watcher /
//! analysis / search) and must never depend on `julie-test-support` (which would
//! re-create the dev-dependency cycle that ADR-0006 eliminated). The Rust
//! compiler already enforces this for real code references (julie-core has no
//! path-dependency on those crates, so any such `use` fails to compile) — this
//! test is a cheaper, clearer guard that also catches a regression at the
//! manifest level and fails with an actionable message instead of a confusing
//! resolver error.

use std::fs;
use std::path::{Path, PathBuf};

/// Upward module paths / crate names that must never appear in julie-core's
/// executable source. Matched against comment-stripped lines.
const FORBIDDEN_SOURCE: &[&str] = &[
    "crate::handler",
    "crate::tools",
    "crate::daemon",
    "crate::indexing_core",
    "crate::watcher",
    "crate::analysis",
    "crate::search",
    "crate::workspace",
    "crate::external_extract",
    "crate::health",
    "julie_test_support",
    "julie::",
];

/// Strip a single-line `//` comment (and trailing comments) from a source line so
/// that the architecture-describing doc comments in lib.rs / paths.rs (which
/// legitimately *mention* `julie::` and `crate::handler`) do not trip the guard.
/// Only the code portion before the first `//` is inspected.
fn code_part(line: &str) -> &str {
    match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_upward_source_references() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src, &mut files);
    assert!(
        !files.is_empty(),
        "tripwire found no .rs files under {} — path wrong?",
        src.display()
    );

    let mut violations = Vec::new();
    for file in &files {
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for (lineno, line) in content.lines().enumerate() {
            let code = code_part(line);
            for needle in FORBIDDEN_SOURCE {
                if code.contains(needle) {
                    violations.push(format!(
                        "{}:{}: forbidden upward reference `{}`",
                        file.strip_prefix(&src).unwrap_or(file).display(),
                        lineno + 1,
                        needle
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "julie-core is the bottom leaf crate and must not reference parent-crate \
         modules or julie-test-support (ADR-0006). Found {} violation(s):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn manifest_has_no_cyclic_or_upward_dependency() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let content = fs::read_to_string(&manifest).expect("read julie-core Cargo.toml");

    let mut violations = Vec::new();
    for (lineno, line) in content.lines().enumerate() {
        let code = code_part(line); // ignore `# ...` is TOML, but `//` never appears; comments are `#`
        let trimmed = code.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        // Re-creating the ADR-0006 cycle: julie-core must not depend on julie-test-support.
        if trimmed.contains("julie-test-support") {
            violations.push(format!(
                "Cargo.toml:{}: julie-core must NOT depend on julie-test-support \
                 (re-creates the dev-dep cycle ADR-0006 removed)",
                lineno + 1
            ));
        }
        // Depending on the parent crate by name would invert the dependency direction.
        if trimmed.starts_with("julie ") || trimmed.starts_with("julie=") {
            violations.push(format!(
                "Cargo.toml:{}: julie-core must NOT depend on the parent `julie` crate",
                lineno + 1
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "Dependency-direction violation(s) in julie-core/Cargo.toml:\n{}",
        violations.join("\n")
    );
}
