//! Integration test that proves julie-extractors is usable from a downstream
//! Rust crate via a path dependency. This is the Pillar 3 gate — see plan
//! Task 5.7 for context.
//!
//! `cargo package --list` only enumerates files; it does not prove the crate
//! is consumable. Real `cargo package -p julie-extractors --allow-dirty`
//! fails because four inherent git dependencies (tree-sitter-qmljs,
//! tree-sitter-razor, tree-sitter-powershell, tree-sitter-vb-dotnet) have
//! no crates.io versions. The actual Pillar 3 contract is
//! "consumable as a Rust path/git dependency". This test spawns a tempdir
//! consumer crate, path-deps julie-extractors, and runs a program calling
//! both extract_canonical and capability_snapshot.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn julie_extractors_works_as_path_dependency_in_downstream_crate() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let extractors_path = PathBuf::from(manifest_dir);
    assert!(
        extractors_path.join("Cargo.toml").exists(),
        "expected extractors Cargo.toml at {:?}",
        extractors_path
    );

    let tempdir = tempfile::tempdir().expect("create tempdir");
    let consumer = tempdir.path();

    let extractors_abs = extractors_path
        .canonicalize()
        .expect("canonicalize extractors path");
    let extractors_abs_str = extractors_abs.to_string_lossy().replace('\\', "/");

    fs::write(
        consumer.join("Cargo.toml"),
        format!(
            r#"[package]
name = "julie_extractors_downstream_smoke"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
julie-extractors = {{ path = "{extractors_abs_str}" }}
anyhow = "1.0"
"#
        ),
    )
    .expect("write consumer Cargo.toml");

    fs::create_dir_all(consumer.join("src")).expect("create src/");
    fs::write(
        consumer.join("src/main.rs"),
        r#"use std::path::Path;

fn main() -> anyhow::Result<()> {
    let source = "fn main() { println!(\"hi\"); }";
    let result = julie_extractors::extract_canonical("hello.rs", source, Path::new("."))?;
    assert!(!result.symbols.is_empty(), "expected at least one symbol");

    let snap = julie_extractors::capability_snapshot();
    let rust = snap.get("rust").expect("rust language row");
    assert!(rust.target_capabilities.symbols);

    let _version: &str = julie_extractors::EXTRACTION_CONTRACT_VERSION;
    Ok(())
}
"#,
    )
    .expect("write consumer main.rs");

    let target_dir = consumer.join("target");

    let status = Command::new(env!("CARGO"))
        .args(["build", "--manifest-path"])
        .arg(consumer.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir)
        .status()
        .expect("spawn cargo build");
    assert!(status.success(), "downstream consumer crate failed to build");

    let run_status = Command::new(env!("CARGO"))
        .args(["run", "--manifest-path"])
        .arg(consumer.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir)
        .status()
        .expect("spawn cargo run");
    assert!(
        run_status.success(),
        "downstream consumer crate failed to run"
    );
}
