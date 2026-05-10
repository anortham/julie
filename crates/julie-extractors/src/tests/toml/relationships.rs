//! Phase 3.3 — TOML domain-relationship extraction.
//!
//! - **Cargo.toml** `[dependencies]` / `[dev-dependencies]` /
//!   `[build-dependencies]` (and target-scoped variants) emit
//!   `RelationshipKind::Imports` edges, one per child key.
//! - **pyproject.toml** `[tool.<x>.*]` tables emit
//!   `RelationshipKind::References` edges, one per top-level tool name.
//! - **Arbitrary tables** in non-Cargo / non-pyproject files emit nothing.
//!
//! File-name detection: only paths whose basename is `Cargo.toml` trigger
//! Cargo extraction; only `pyproject.toml` triggers tool-table extraction.

use crate::base::RelationshipKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_toml_cargo_dependencies_emit_relationships() {
    let source = r#"
[package]
name = "myapp"

[dependencies]
serde = "1.0"
tokio = { version = "1", features = ["full"] }
"#;
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("Cargo.toml", source, workspace_root)
        .expect("canonical TOML extraction must succeed");

    let serde_id = &result
        .symbols
        .iter()
        .find(|s| s.name == "serde")
        .expect("serde dependency symbol must exist")
        .id;
    let tokio_id = &result
        .symbols
        .iter()
        .find(|s| s.name == "tokio")
        .expect("tokio dependency symbol must exist")
        .id;

    let serde_edge = result
        .relationships
        .iter()
        .find(|r| &r.to_symbol_id == serde_id)
        .unwrap_or_else(|| {
            panic!(
                "expected Cargo [dependencies] serde Imports edge; got: {:#?}",
                result.relationships
            )
        });
    assert!(
        matches!(serde_edge.kind, RelationshipKind::Imports),
        "serde edge must be Imports, got {:?}",
        serde_edge.kind
    );

    assert!(
        result
            .relationships
            .iter()
            .any(|r| &r.to_symbol_id == tokio_id && matches!(r.kind, RelationshipKind::Imports)),
        "expected Cargo [dependencies] tokio Imports edge"
    );
}

#[test]
fn test_toml_pyproject_tool_tables_emit_relationships() {
    let source = r#"
[project]
name = "myapp"

[tool.ruff]
line-length = 88

[tool.pytest.ini_options]
asyncio_mode = "auto"
"#;
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("pyproject.toml", source, workspace_root)
        .expect("canonical TOML extraction must succeed");

    let ruff_edge = result
        .relationships
        .iter()
        .find(|r| {
            r.metadata
                .as_ref()
                .and_then(|m| m.get("toolName"))
                .and_then(|v| v.as_str())
                == Some("ruff")
        })
        .unwrap_or_else(|| {
            panic!(
                "expected pyproject [tool.ruff] References edge with metadata.toolName=\"ruff\"; got: {:#?}",
                result.relationships
            )
        });
    assert!(
        matches!(ruff_edge.kind, RelationshipKind::References),
        "tool table edge must be References, got {:?}",
        ruff_edge.kind
    );

    assert!(
        result.relationships.iter().any(|r| {
            r.metadata
                .as_ref()
                .and_then(|m| m.get("toolName"))
                .and_then(|v| v.as_str())
                == Some("pytest")
                && matches!(r.kind, RelationshipKind::References)
        }),
        "expected pyproject [tool.pytest.*] References edge with metadata.toolName=\"pytest\""
    );

    // Only one edge per top-level tool name, even though [tool.pytest.ini_options]
    // is a deeper sub-table — the unique-tool dedup must hold.
    let pytest_count = result
        .relationships
        .iter()
        .filter(|r| {
            r.metadata
                .as_ref()
                .and_then(|m| m.get("toolName"))
                .and_then(|v| v.as_str())
                == Some("pytest")
        })
        .count();
    assert_eq!(
        pytest_count, 1,
        "expected exactly one References edge per unique top-level tool name; got {} for `pytest`",
        pytest_count
    );
}

#[test]
fn test_toml_arbitrary_table_emits_no_relationship() {
    let source = r#"
[some.other.table]
key = "value"
"#;
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("config.toml", source, workspace_root)
        .expect("canonical TOML extraction must succeed");

    assert!(
        result.relationships.is_empty(),
        "no relationships should be emitted for arbitrary tables in non-Cargo/non-pyproject files; got: {:#?}",
        result.relationships
    );
}
