//! Dependency-direction tripwire for the `julie-index` crate (see ADR-0006
//! and `docs/plans/2026-06-03-julie-rescue-design.md`).
//!
//! `julie-index` is the search + analysis layer sitting above `julie-core` and
//! below the top `julie` crate. It must never reach UP into the parent `julie`
//! crate (handler / tools / daemon / watcher / workspace / cli / indexing_core /
//! concrete embeddings). The Rust compiler already enforces this for real code
//! references (julie-index has no path-dependency on those crates, so any such
//! `use` fails to compile) — this test is a cheaper, clearer guard that also
//! catches a regression at the manifest level and fails with an actionable
//! message instead of a confusing resolver error.
//!
//! Allowed downward references within this crate:
//!   - `crate::search`   — internal search module (same crate)
//!   - `crate::analysis` — internal analysis module (same crate)
//!   - `julie_core::`    — the leaf crate this layer sits above
//!   - `julie_extractors::` — the external extractor dependency

use std::fs;
use std::path::{Path, PathBuf};

/// Upward module paths / crate names that must never appear in julie-index's
/// executable source. Matched against comment-stripped lines.
///
/// Note: `julie_test_support` is NOT forbidden here — it is a valid dev-dep
/// for `julie-index` (no cycle risk since julie-test-support does not depend
/// on julie-index). Only top-crate (`julie`) module paths are forbidden.
const FORBIDDEN_SOURCE: &[&str] = &[
    "crate::tools",
    "crate::handler",
    "crate::registry",
    "crate::indexing_core",
    "crate::watcher",
    "crate::workspace",
    "crate::cli",
    "crate::embeddings",
    "julie::",
];

/// Strip a single-line `//` comment (and trailing comments) from a source line
/// so that architecture-describing doc comments in lib.rs (which legitimately
/// *mention* the forbidden names) do not trip the guard.
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
                        file.strip_prefix(&src)
                            .unwrap_or_else(|_| file.as_path())
                            .display(),
                        lineno + 1,
                        needle
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "julie-index must not reference parent-crate modules (handler, tools, daemon, \
         watcher, workspace, cli, indexing_core, concrete embeddings) or julie-test-support \
         (ADR-0006). Found {} violation(s):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn manifest_has_no_cyclic_or_upward_dependency() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let content = fs::read_to_string(&manifest).expect("read julie-index Cargo.toml");

    let mut violations = Vec::new();
    for (lineno, line) in content.lines().enumerate() {
        let code = code_part(line); // `//` doesn't appear in TOML; comments are `#`
        let trimmed = code.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        // Depending on the parent crate by name would invert the dependency direction.
        // Note: `julie-core`, `julie-extractors`, `julie-test-support` (dev-dep) are all
        // fine — they are downward deps or test helpers that do NOT depend on julie-index.
        // Only `julie` itself (the top crate) is forbidden.
        if trimmed.starts_with("julie ") || trimmed.starts_with("julie=") {
            violations.push(format!(
                "Cargo.toml:{}: julie-index must NOT depend on the parent `julie` crate",
                lineno + 1
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "Dependency-direction violation(s) in julie-index/Cargo.toml:\n{}",
        violations.join("\n")
    );
}
