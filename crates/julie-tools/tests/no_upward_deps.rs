//! Dependency-direction tripwire for the `julie-tools` crate (Phase 2b;
//! see `docs/plans/2026-06-03-julie-rescue-design.md`).
//!
//! `julie-tools` is the handler-free tool-implementation layer. In the target DAG
//! it sits ABOVE `julie-core`, `julie-index`, `julie-pipeline`, and
//! `julie-context` and BELOW the top `julie` crate (which wires tools to the MCP
//! handler). It must therefore reach DOWN only into its declared workspace
//! dependencies and must never reference the top `julie` crate, the handler
//! runtime, daemon, or session infrastructure.
//!
//! `julie-context::ToolContext` is the explicit seam: all tool entry points accept
//! `&dyn ToolContext` rather than `&JulieServerHandler`; this test verifies that
//! seam is not short-circuited.

use std::fs;
use std::path::{Path, PathBuf};

/// Upward / sibling module paths and crate names that must never appear in
/// julie-tools' executable source. Matched against comment-stripped lines.
const FORBIDDEN_SOURCE: &[&str] = &[
    // Parent `julie` crate, by extern-crate name.
    "julie::",
    // Top-crate handler / daemon / session infrastructure.
    "crate::handler",
    "crate::daemon",
    "crate::session",
    "crate::watcher",
    "crate::external_extract",
    "crate::health",
    // The JulieServerHandler type must not appear — ToolContext is the seam.
    "JulieServerHandler",
    "IsolatedStorageHandler",
];

/// Workspace path-dependencies julie-tools is allowed to declare. Everything
/// else in the `julie-*` family is a sibling or higher crate; depending on it
/// would invert the layering. `julie-extractors` is an external git dependency
/// and `julie-test-support` is a leaf test helper (legal dev-dep, no cycle).
const ALLOWED_WORKSPACE_DEPS: &[&str] = &[
    "julie-core",
    "julie-index",
    "julie-pipeline",
    "julie-context",
    "julie-extractors",
    "julie-test-support",
];

/// Strip a single-line `//` comment from a source line so that
/// architecture-describing doc comments (which legitimately *mention*
/// `julie::` or `crate::handler`) do not trip the guard.
fn code_part(line: &str) -> &str {
    match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

/// Extract the dependency name from a TOML dependency line such as
/// `julie-core = { path = "../julie-core" }`.
fn workspace_dep_name(trimmed: &str) -> Option<&str> {
    if trimmed.starts_with('[') {
        return None;
    }
    let end = trimmed.find([' ', '=', '.'])?;
    let name = &trimmed[..end];
    if name.is_empty() { None } else { Some(name) }
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
        "julie-tools sits below the top `julie` crate and must not reach up into \
         handler/daemon/session infrastructure. Found {} violation(s):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn manifest_has_no_cyclic_or_upward_dependency() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let content = fs::read_to_string(&manifest).expect("read julie-tools Cargo.toml");

    let mut violations = Vec::new();
    for (lineno, line) in content.lines().enumerate() {
        let code = code_part(line);
        let trimmed = code.trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("julie ") || trimmed.starts_with("julie=") {
            violations.push(format!(
                "Cargo.toml:{}: julie-tools must NOT depend on the parent `julie` crate",
                lineno + 1
            ));
            continue;
        }
        if let Some(name) = workspace_dep_name(trimmed) {
            if name.starts_with("julie-") && !ALLOWED_WORKSPACE_DEPS.contains(&name) {
                violations.push(format!(
                    "Cargo.toml:{}: julie-tools must NOT depend on `{}` — only {:?} \
                     are legal downward deps (R7)",
                    lineno + 1,
                    name,
                    ALLOWED_WORKSPACE_DEPS
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Dependency-direction violation(s) in julie-tools/Cargo.toml:\n{}",
        violations.join("\n")
    );
}
