//! Lean `xtask` must not pull product or eval-only crates as normal deps.
//!
//! Contract (design): `cargo tree -p xtask -e normal --depth 1` must not include
//! `julie`, `rusqlite`, `serde_json`, `tempfile`, or `tokio`. Direct normals are
//! allowlisted to `anyhow`, `serde`, `toml`. `tempfile` may exist as a **dev**-dep only.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Allowed direct normal (`[dependencies]`) package names for lean xtask.
const ALLOWED_DIRECT_NORMALS: &[&str] = &["anyhow", "serde", "toml"];

/// Packages that must never appear as normal deps (direct or depth-1 tree).
const FORBIDDEN_NORMALS: &[&str] = &["julie", "rusqlite", "serde_json", "tempfile", "tokio"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level under repo root")
        .to_path_buf()
}

fn xtask_cargo_toml() -> PathBuf {
    repo_root().join("xtask/Cargo.toml")
}

fn parse_direct_normal_deps(manifest: &str) -> BTreeSet<String> {
    let parsed: toml::Value = toml::from_str(manifest).expect("xtask/Cargo.toml parses as TOML");
    let Some(deps) = parsed.get("dependencies").and_then(|v| v.as_table()) else {
        return BTreeSet::new();
    };
    deps.keys().cloned().collect()
}

fn parse_direct_dev_deps(manifest: &str) -> BTreeSet<String> {
    let parsed: toml::Value = toml::from_str(manifest).expect("xtask/Cargo.toml parses as TOML");
    let Some(deps) = parsed.get("dev-dependencies").and_then(|v| v.as_table()) else {
        return BTreeSet::new();
    };
    deps.keys().cloned().collect()
}

/// Parse `cargo tree -p xtask -e normal --depth 1` package names (children only).
fn parse_cargo_tree_depth1_packages(tree_output: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in tree_output.lines() {
        let trimmed = line.trim_start();
        // Tree edges look like: "├── anyhow v1.0.102" / "└── toml v0.8.23"
        let Some(rest) = trimmed
            .strip_prefix("├── ")
            .or_else(|| trimmed.strip_prefix("└── "))
            .or_else(|| trimmed.strip_prefix("|-- "))
            .or_else(|| trimmed.strip_prefix("`-- "))
        else {
            continue;
        };
        let pkg = rest.split_whitespace().next().unwrap_or("");
        if !pkg.is_empty() {
            names.insert(pkg.to_string());
        }
    }
    names
}

fn run_cargo_tree_xtask_normal_depth1() -> String {
    let output = Command::new("cargo")
        .args(["tree", "-p", "xtask", "-e", "normal", "--depth", "1"])
        .current_dir(repo_root())
        .output()
        .expect("cargo tree must be runnable");
    assert!(
        output.status.success(),
        "cargo tree -p xtask -e normal --depth 1 failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("cargo tree stdout is utf-8")
}

#[test]
fn lean_xtask_normal_deps_match_allowlist() {
    let manifest = std::fs::read_to_string(xtask_cargo_toml()).expect("read xtask/Cargo.toml");
    let direct_normals = parse_direct_normal_deps(&manifest);
    let allowed: BTreeSet<String> = ALLOWED_DIRECT_NORMALS
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    assert_eq!(
        direct_normals, allowed,
        "xtask [dependencies] must equal the lean allowlist {allowed:?}; got {direct_normals:?}"
    );

    for forbidden in FORBIDDEN_NORMALS {
        assert!(
            !direct_normals.contains(*forbidden),
            "forbidden normal dep `{forbidden}` must not appear in xtask [dependencies]"
        );
    }

    // tempfile is allowed as a **dev**-dependency only (tests), never as a normal dep.
    let dev_deps = parse_direct_dev_deps(&manifest);
    assert!(
        !direct_normals.contains("tempfile"),
        "tempfile must not be a normal dependency"
    );
    assert!(
        dev_deps.contains("tempfile"),
        "tempfile should remain available as a dev-dependency for xtask tests"
    );

    let tree = run_cargo_tree_xtask_normal_depth1();
    let tree_packages = parse_cargo_tree_depth1_packages(&tree);

    assert_eq!(
        tree_packages, allowed,
        "cargo tree -p xtask -e normal --depth 1 packages must equal allowlist {allowed:?}; got {tree_packages:?}\nfull tree:\n{tree}"
    );

    for forbidden in FORBIDDEN_NORMALS {
        assert!(
            !tree_packages.contains(*forbidden),
            "forbidden package `{forbidden}` must not appear in cargo tree depth-1 normals; tree:\n{tree}"
        );
        // Also catch a mistaken path-dep line like "julie vX (...)"
        assert!(
            !tree.lines().any(|line| {
                let trimmed = line.trim_start();
                trimmed.contains(&format!("── {forbidden} "))
                    || trimmed.starts_with(&format!("── {forbidden} "))
            }),
            "forbidden package `{forbidden}` must not appear in cargo tree output:\n{tree}"
        );
    }
}
