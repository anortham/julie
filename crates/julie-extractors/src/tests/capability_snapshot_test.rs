//! Phase 5.2 — capability_snapshot() public API regression bar.

use crate::capability_snapshot;

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
