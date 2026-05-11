//! Capability snapshot public API regression bar.

use crate::{CapabilitySnapshot, capability_snapshot};

#[test]
fn test_capability_snapshot_loads_all_languages() {
    let snap = capability_snapshot();
    assert_eq!(snap.languages().count(), 36);
    assert!(snap.get("rust").is_some());
    assert!(snap.get("vbnet").is_some());
}

#[test]
fn test_capability_snapshot_get_returns_none_for_unknown() {
    assert!(capability_snapshot().get("klingon").is_none());
}

#[test]
fn test_capability_snapshot_uses_oncelock_not_build_script() {
    // A build script would live at CARGO_MANIFEST_DIR/build.rs. The
    // Pillar-3 architecture decision is to load capabilities.json via
    // include_str! at compile time, not via a build script that emits
    // generated Rust. Locking this is cheaper than calling `cargo
    // metadata` from inside the test (which is sensitive to cwd and
    // subprocess setup); a missing build.rs is exactly the invariant
    // we want to assert.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let build_rs = std::path::Path::new(manifest_dir).join("build.rs");
    assert!(
        !build_rs.exists(),
        "julie-extractors must not ship a build.rs ({}); the Pillar-3 design \
         uses include_str! in src/capability_snapshot.rs instead",
        build_rs.display()
    );
}

#[test]
fn test_capability_snapshot_deserializes_mixed_legacy_and_kind_coverage_rows() {
    let json = r#"
{
  "languages": [
    {
      "language": "legacy",
      "parser_crate": "tree-sitter-legacy",
      "extensions": ["legacy"],
      "dependency_status": "current",
      "target_capabilities": {
        "symbols": true,
        "relationships": false,
        "pending_relationships": false,
        "identifiers": true,
        "types": false
      },
      "capabilities": {
        "symbols": true,
        "relationships": false,
        "pending_relationships": false,
        "identifiers": true,
        "types": false
      },
      "fixtures": []
    },
    {
      "language": "new",
      "parser_crate": "tree-sitter-new",
      "extensions": ["new"],
      "dependency_status": "current",
      "target_capabilities": {
        "symbols": true,
        "relationships": true,
        "pending_relationships": true,
        "identifiers": true,
        "types": false
      },
      "capabilities": {
        "symbols": true,
        "relationships": true,
        "pending_relationships": true,
        "identifiers": true,
        "types": false
      },
      "kind_coverage": {
        "symbols": {
          "supported": ["function"],
          "not_applicable": ["event"],
          "open_gaps": []
        },
        "relationships": {
          "supported": ["calls"],
          "not_applicable": [],
          "open_gaps": []
        },
        "identifiers": {
          "supported": ["call"],
          "not_applicable": [],
          "open_gaps": []
        }
      },
      "fixtures": []
    }
  ]
}
"#;

    let snap = CapabilitySnapshot::from_json_str(json).expect("mixed snapshot should parse");

    let legacy = snap.get("legacy").expect("legacy row should be indexed");
    assert!(legacy.capabilities.symbols);
    assert!(legacy.kind_coverage.symbols.supported.is_empty());

    let new = snap.get("new").expect("new row should be indexed");
    assert_eq!(new.kind_coverage.symbols.supported, vec!["function"]);
    assert_eq!(new.kind_coverage.relationships.supported, vec!["calls"]);
    assert_eq!(new.kind_coverage.identifiers.supported, vec!["call"]);
}
