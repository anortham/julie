use std::fs;

use julie_test_support::SnapshotFixture;
use tempfile::TempDir;

use crate::tools::deep_dive::deep_dive_query;

const LIB_WITH_BRANCHY_PROCESS: &str = "pub struct Request;\n\npub fn process(input: Request, retry: bool) {\n    if retry {\n        let _ = &input;\n    }\n    if retry { let _ = &input; }\n    for _ in 0..2 { while retry { if retry { break; } } }\n    if retry { let _ = &input; }\n}\n";

fn seeded(lib_rs: &str) -> (TempDir, SnapshotFixture) {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), lib_rs).unwrap();
    let fixture = SnapshotFixture::from_tree(dir.path()).unwrap();
    (dir, fixture)
}

#[test]
fn deep_dive_prints_stored_complexity_for_selected_symbol() {
    let (_dir, fixture) = seeded(LIB_WITH_BRANCHY_PROCESS);
    let snapshot = fixture.snapshot();

    for depth in ["overview", "context", "full"] {
        let output =
            deep_dive_query(&snapshot, "process", Some("src/lib.rs"), depth, 20, 20).unwrap();

        assert!(
            output.contains("complexity: decisions=4 loops=2 nesting=3 params=2 lines=8"),
            "missing stored complexity at {depth} depth:\n{output}"
        );
    }
}

#[test]
fn deep_dive_omits_complexity_line_when_metric_is_absent() {
    let (_dir, fixture) = seeded("pub struct Request;\n");
    let snapshot = fixture.snapshot();

    let output =
        deep_dive_query(&snapshot, "Request", Some("src/lib.rs"), "overview", 20, 20).unwrap();

    assert!(!output.contains("complexity:"));
}
