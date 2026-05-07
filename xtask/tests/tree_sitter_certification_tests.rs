use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use xtask::cli::{CertifyCommand, CliCommand, parse_cli_command};
use xtask::tree_sitter_certification::{
    DEFAULT_TREE_SITTER_CERTIFICATION_REPORT, TreeSitterCertificationMetadata,
    build_tree_sitter_certification_report, render_tree_sitter_certification_markdown,
};
use xtask::tree_sitter_real_world::{
    DEFAULT_TREE_SITTER_REAL_WORLD_CORPUS, DEFAULT_TREE_SITTER_REAL_WORLD_EVIDENCE,
    DEFAULT_TREE_SITTER_REAL_WORLD_HOME, run_tree_sitter_real_world_with_head,
};

#[test]
fn tree_sitter_certification_tests_parse_default_command() {
    let parsed = parse_cli_command(["xtask", "certify", "tree-sitter"])
        .expect("tree-sitter certification command should parse");

    assert_eq!(
        parsed,
        CliCommand::Certify(CertifyCommand::TreeSitter {
            out: PathBuf::from(DEFAULT_TREE_SITTER_CERTIFICATION_REPORT),
            check: false,
            real_world: false,
            profile: "smoke".to_string(),
            corpus: PathBuf::from(DEFAULT_TREE_SITTER_REAL_WORLD_CORPUS),
            julie_home: PathBuf::from(DEFAULT_TREE_SITTER_REAL_WORLD_HOME),
        })
    );
}

#[test]
fn tree_sitter_certification_tests_parse_check_and_out_flags() {
    let parsed = parse_cli_command([
        "xtask",
        "certify",
        "tree-sitter",
        "--check",
        "--out",
        "artifacts/tree-sitter-certification.md",
    ])
    .expect("tree-sitter certification command should parse flags");

    assert_eq!(
        parsed,
        CliCommand::Certify(CertifyCommand::TreeSitter {
            out: PathBuf::from("artifacts/tree-sitter-certification.md"),
            check: true,
            real_world: false,
            profile: "smoke".to_string(),
            corpus: PathBuf::from(DEFAULT_TREE_SITTER_REAL_WORLD_CORPUS),
            julie_home: PathBuf::from(DEFAULT_TREE_SITTER_REAL_WORLD_HOME),
        })
    );
}

#[test]
fn tree_sitter_certification_tests_parse_real_world_flags() {
    let parsed = parse_cli_command([
        "xtask",
        "certify",
        "tree-sitter",
        "--real-world",
        "--profile",
        "restored",
        "--corpus",
        "fixtures/extraction/custom-real-world.toml",
        "--julie-home",
        "artifacts/tree-sitter-certification/custom-home",
        "--out",
        "docs/custom-real-world-evidence.json",
    ])
    .expect("tree-sitter real-world certification command should parse flags");

    assert_eq!(
        parsed,
        CliCommand::Certify(CertifyCommand::TreeSitter {
            out: PathBuf::from("docs/custom-real-world-evidence.json"),
            check: false,
            real_world: true,
            profile: "restored".to_string(),
            corpus: PathBuf::from("fixtures/extraction/custom-real-world.toml"),
            julie_home: PathBuf::from("artifacts/tree-sitter-certification/custom-home"),
        })
    );
}

#[test]
fn tree_sitter_certification_tests_report_surfaces_current_contract_and_historical_gaps() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_minimal_repo(root, true);
    write_real_world_evidence(root);

    let report = build_tree_sitter_certification_report(
        root,
        TreeSitterCertificationMetadata {
            head_sha: "abc123".to_string(),
        },
    )
    .expect("report should build from complete evidence");

    assert_eq!(report.head_sha, "abc123");
    assert_eq!(report.registry_row_count, 3);
    assert_eq!(report.historical_matrix_row_count, 1);
    assert_eq!(report.raw_verification_report_count, 1);
    assert_eq!(report.rows_with_open_gaps, vec!["vbnet"]);
    assert_eq!(report.rows_without_gap_entries, vec!["rust", "tsx"]);
    assert_eq!(
        report.current_rows_missing_from_historical_matrix,
        vec!["tsx", "vbnet"]
    );
    assert_eq!(
        report.gap_count_by_capability.get("pending_relationships"),
        Some(&1)
    );
    assert_eq!(
        report
            .real_world_evidence
            .as_ref()
            .expect("real-world evidence should load")
            .verified_repo_count,
        1
    );
    assert_eq!(
        report
            .real_world_evidence
            .as_ref()
            .expect("real-world evidence should load")
            .skipped_repo_count,
        1
    );

    let markdown = render_tree_sitter_certification_markdown(&report);
    assert!(markdown.contains("# Tree-Sitter Certification Report"));
    assert!(markdown.contains("Current HEAD: `abc123`"));
    assert!(markdown.contains("Registry rows: `3`"));
    assert!(markdown.contains("Historical matrix rows: `1`"));
    assert!(markdown.contains("`tsx`, `vbnet`"));
    assert!(markdown.contains("| `vbnet` | `pending_relationships` | `open` |"));
    assert!(markdown.contains("## Current Real-World OSS Evidence"));
    assert!(markdown.contains("| `ready-rust` | `rust` | `pass` |"));
}

#[test]
fn tree_sitter_certification_tests_missing_gap_evidence_is_a_hard_failure() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_minimal_repo(root, false);

    let error = build_tree_sitter_certification_report(
        root,
        TreeSitterCertificationMetadata {
            head_sha: "abc123".to_string(),
        },
    )
    .expect_err("missing evidence must fail certification generation");

    assert!(
        error
            .to_string()
            .contains("vbnet pending_relationships gap evidence path does not exist"),
        "got: {error}"
    );
}

#[test]
fn tree_sitter_real_world_tests_indexes_repo_without_writing_project_julie_dir() {
    let temp = TempDir::new().unwrap();
    let source_root = temp.path().join("source");
    let repo_root = source_root.join("ready-rust");
    write_file_at(
        &repo_root.join("Cargo.toml"),
        "[package]\nname = \"ready-rust\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    write_file_at(
        &repo_root.join("src/lib.rs"),
        "pub fn exported() -> i32 { 42 }\n",
    );

    let corpus_path = temp.path().join("real-world.toml");
    write_file_at(
        &corpus_path,
        &format!(
            r#"
roots = ["{}"]

[profiles.smoke]
repos = ["ready-rust"]

[[repos]]
name = "ready-rust"
language = "rust"
profile_tags = ["smoke"]
min_files = 1
min_symbols = 1
"#,
            source_root.display()
        ),
    );

    let julie_home = temp.path().join("isolated-julie-home");
    let out_path = temp.path().join("evidence.json");
    let report = run_tree_sitter_real_world_with_head(
        "abc123".to_string(),
        &julie_home,
        &corpus_path,
        "smoke",
        &out_path,
    )
    .expect("real-world runner should index a tiny repo");

    assert_eq!(report.verified_repos.len(), 1);
    assert_eq!(report.verified_repos[0].status, "pass");
    assert!(report.verified_repos[0].symbol_count >= 1);
    assert!(
        !repo_root.join(".julie").exists(),
        "real-world certification must not write project-local .julie data"
    );
    assert!(
        !repo_root.join(".julieignore").exists(),
        "real-world certification must not write project-local .julieignore data"
    );
    assert!(out_path.is_file(), "runner should write JSON evidence");
}

fn write_minimal_repo(root: &Path, include_gap_evidence: bool) {
    write_file(
        root,
        "fixtures/extraction/capabilities.json",
        r#"
{
  "languages": [
    {
      "language": "rust",
      "parser_crate": "tree-sitter-rust",
      "extensions": ["rs"],
      "dependency_status": "current",
      "capabilities": {
        "symbols": true,
        "relationships": true,
        "pending_relationships": true,
        "identifiers": true,
        "types": true
      },
      "target_capabilities": {
        "symbols": true,
        "relationships": true,
        "pending_relationships": true,
        "identifiers": true,
        "types": true
      },
      "fixtures": [
        {
          "name": "basic",
          "source": "fixtures/extraction/rust/basic/source.rs",
          "expected": "fixtures/extraction/rust/basic/expected.json"
        }
      ]
    },
    {
      "language": "tsx",
      "parser_crate": "tree-sitter-typescript",
      "extensions": ["tsx"],
      "dependency_status": "current",
      "capabilities": {
        "symbols": true,
        "relationships": true,
        "pending_relationships": true,
        "identifiers": true,
        "types": true
      },
      "target_capabilities": {
        "symbols": true,
        "relationships": true,
        "pending_relationships": true,
        "identifiers": true,
        "types": true
      },
      "fixtures": [
        {
          "name": "basic",
          "source": "fixtures/extraction/tsx/basic/source.tsx",
          "expected": "fixtures/extraction/tsx/basic/expected.json"
        }
      ]
    },
    {
      "language": "vbnet",
      "parser_crate": "tree-sitter-vbnet",
      "extensions": ["vb"],
      "dependency_status": "current",
      "capabilities": {
        "symbols": true,
        "relationships": true,
        "pending_relationships": true,
        "identifiers": true,
        "types": true
      },
      "target_capabilities": {
        "symbols": true,
        "relationships": true,
        "pending_relationships": true,
        "identifiers": true,
        "types": true
      },
      "fixtures": [
        {
          "name": "basic",
          "source": "fixtures/extraction/vbnet/basic/source.vb",
          "expected": "fixtures/extraction/vbnet/basic/expected.json"
        }
      ],
      "capability_gaps": [
        {
          "capability": "pending_relationships",
          "status": "open",
          "reason": "fixture does not prove pending output",
          "required_closure": "add pending fixture evidence",
          "evidence": "docs/findings/COMPILED-FINDINGS.md"
        }
      ]
    }
  ]
}
"#,
    );
    write_file(
        root,
        "docs/LANGUAGE_VERIFICATION_RESULTS.md",
        r#"
# Language Verification Results

| Language | Reference Project | 1. Symbols | Date |
|----------|------------------|-----------|------|
| Rust | Julie | PASS | 2026-03-18 |
"#,
    );
    write_file(root, "docs/verification/rust_julie.md", "# Rust report\n");
    write_file(
        root,
        "fixtures/extraction/rust/basic/source.rs",
        "fn main() {}\n",
    );
    write_file(
        root,
        "fixtures/extraction/tsx/basic/source.tsx",
        "export const App = () => <div />;\n",
    );
    write_file(
        root,
        "fixtures/extraction/vbnet/basic/source.vb",
        "Module Program\nEnd Module\n",
    );
    for path in [
        "fixtures/extraction/rust/basic/expected.json",
        "fixtures/extraction/tsx/basic/expected.json",
        "fixtures/extraction/vbnet/basic/expected.json",
    ] {
        write_file(
            root,
            path,
            r#"{
  "symbols": [{"name": "sample"}],
  "relationships": [],
  "pending_relationships": [],
  "structured_pending_relationships": [],
  "identifiers": [],
  "types": [],
  "parse_diagnostics": []
}
"#,
        );
    }
    if include_gap_evidence {
        write_file(root, "docs/findings/COMPILED-FINDINGS.md", "# findings\n");
    }
}

fn write_real_world_evidence(root: &Path) {
    write_file(
        root,
        DEFAULT_TREE_SITTER_REAL_WORLD_EVIDENCE,
        r#"
{
  "profile": "smoke",
  "julie_head": "abc123",
  "corpus_path": "fixtures/extraction/tree-sitter-real-world-corpus.toml",
  "verified_repos": [
    {
      "repo_name": "ready-rust",
      "language": "rust",
      "display_path": "~/source/ready-rust",
      "repo_head": "def456",
      "workspace_id": "ready_rust_12345678",
      "file_count": 1,
      "language_file_count": 1,
      "symbol_count": 3,
      "relationship_count": 2,
      "identifier_count": 4,
      "type_count": 1,
      "parse_diagnostic_file_count": 0,
      "status": "pass",
      "hard_failures": []
    }
  ],
  "skipped_repos": [
    {
      "repo_name": "missing-python",
      "language": "python",
      "reason": "repo not found under configured roots"
    }
  ],
  "summary_flags": []
}
"#,
    );
}

fn write_file(root: &Path, relative_path: &str, contents: &str) {
    write_file_at(&root.join(relative_path), contents);
}

fn write_file_at(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}
