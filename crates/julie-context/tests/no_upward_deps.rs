//! Dependency-direction tripwire for the `julie-context` crate (Phase 2 PR 2b;
//! see `docs/plans/2026-06-04-julie-phase2-crate-split-plan.md` and ADR-0006).
//!
//! `julie-context` holds the `ToolContext` facade trait, `SpilloverStore`, and
//! `WorkspaceTarget`. In the target DAG it sits ABOVE `julie-core` and
//! `julie-index` and BELOW `julie-tools` / `julie-runtime` / the top `julie`
//! crate. It is a SIBLING of `julie-pipeline` (neither depends on the other).
//! It must therefore reach DOWN only into `julie-core` and `julie-index` and
//! must never reference any higher or sibling crate (`julie-pipeline`,
//! `julie-tools`, `julie-runtime`, the parent `julie`).
//!
//! The Rust compiler already enforces this for real code references (context has
//! no path-dependency on those crates, so any such `use` fails to compile) — this
//! test is a cheaper, clearer guard that ALSO catches a regression at the manifest
//! level (an accidental upward path-dep) and fails with an actionable message
//! instead of a confusing resolver error (R7).

use std::fs;
use std::path::{Path, PathBuf};

/// Upward / sibling module paths and crate names that must never appear in
/// julie-context's executable source. Matched against comment-stripped lines.
const FORBIDDEN_SOURCE: &[&str] = &[
    // Parent `julie` crate, by extern-crate name.
    "julie::",
    // Sibling / higher workspace crates, by extern-crate name.
    "julie_pipeline",
    "julie_tools",
    "julie_runtime",
    // Top-crate / runtime modules (belt-and-suspenders; these resolve to nothing
    // inside julie-context and would be compile errors, but the explicit needle
    // yields an actionable message if someone wires up a path-dep first).
    // NOTE: "crate::workspace::" (with trailing ::) to avoid matching
    // the legitimate in-crate module "crate::workspace_target".
    "crate::handler",
    "crate::registry",
    "crate::watcher",
    "crate::workspace::",
    "crate::external_extract",
    "crate::health",
    "crate::tools",
];

/// Workspace path-dependencies julie-context is allowed to declare. Everything
/// else in the `julie-*` family is a sibling or higher crate; depending on it
/// would invert the layering. `julie-test-support` is a leaf test helper that
/// depends only on julie-core (legal dev-dep, no cycle).
const ALLOWED_WORKSPACE_DEPS: &[&str] = &["julie-core", "julie-index", "julie-test-support"];

/// Strip a single-line `//` comment (and trailing comments) from a source line so
/// that architecture-describing doc comments (which legitimately *mention*
/// `julie::` or `crate::handler`) do not trip the guard. Only the code portion
/// before the first `//` is inspected.
fn code_part(line: &str) -> &str {
    match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

/// Extract the dependency name from a TOML dependency line such as
/// `julie-core = { path = "../julie-core" }` or `julie-core.workspace = true`.
/// Returns `None` for section headers and non-key lines.
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
                        "{}:{}: forbidden upward/sibling reference `{}`",
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
        "julie-context sits below tools/runtime/julie and beside julie-pipeline; \
         it must reach down only into julie-core / julie-index. Found {} \
         violation(s):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn manifest_has_no_cyclic_or_upward_dependency() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let content = fs::read_to_string(&manifest).expect("read julie-context Cargo.toml");

    let mut violations = Vec::new();
    for (lineno, line) in content.lines().enumerate() {
        let code = code_part(line); // TOML comments are `#`; `//` never appears.
        let trimmed = code.trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        // Depending on the parent crate by name would invert the dependency
        // direction. (`julie-core`/`julie-index` start with `julie-`, so the
        // space/`=` guard distinguishes the bare top crate.)
        if trimmed.starts_with("julie ") || trimmed.starts_with("julie=") {
            violations.push(format!(
                "Cargo.toml:{}: julie-context must NOT depend on the parent `julie` crate",
                lineno + 1
            ));
            continue;
        }
        // Any `julie-*` dependency outside the allowlist is a sibling or higher
        // crate (julie-pipeline / julie-tools / julie-runtime, or a future crate
        // above context). Forbidding by allowlist — rather than naming each
        // higher sibling — means an accidental upward path-dep cannot slip past
        // even before that crate exists (R7).
        if let Some(name) = workspace_dep_name(trimmed) {
            if name.starts_with("julie-") && !ALLOWED_WORKSPACE_DEPS.contains(&name) {
                violations.push(format!(
                    "Cargo.toml:{}: julie-context must NOT depend on `{}` — only {:?} \
                     are legal downward deps; everything else in the julie-* family is a \
                     sibling or higher crate (R7)",
                    lineno + 1,
                    name,
                    ALLOWED_WORKSPACE_DEPS
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Dependency-direction violation(s) in julie-context/Cargo.toml:\n{}",
        violations.join("\n")
    );
}
