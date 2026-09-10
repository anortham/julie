use julie_test_support::SnapshotFixture;

#[path = "../../../tools/metrics/query.rs"]
mod metrics_query;
use metrics_query::{format_metrics_output, query_by_metrics};

fn fixture() -> (tempfile::TempDir, SnapshotFixture) {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir_all(dir.path().join("src")).expect("src");
    std::fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn high_centrality() { low_centrality(); low_centrality(); }\npub fn low_centrality() {}\n",
    )
    .expect("write rust");
    let fixture = SnapshotFixture::from_tree(dir.path()).expect("snapshot");
    (dir, fixture)
}

#[test]
fn query_metrics_orders_by_centrality() {
    let (_dir, fixture) = fixture();
    let results = query_by_metrics(
        fixture.snapshot().graph(),
        "centrality",
        "desc",
        None,
        None,
        Some("function"),
        None,
        None,
        false,
        10,
    )
    .unwrap();
    let output = format_metrics_output(&results, "centrality", "desc");
    assert!(output.contains("Centrality:"), "{output}");
    assert!(
        results
            .iter()
            .any(|result| result.name == "high_centrality" || result.name == "low_centrality"),
        "{results:?}"
    );
}

#[test]
fn query_metrics_kind_filter_excludes_other_kinds() {
    let (_dir, fixture) = fixture();
    let results = query_by_metrics(
        fixture.snapshot().graph(),
        "centrality",
        "desc",
        None,
        None,
        Some("module"),
        None,
        None,
        false,
        10,
    )
    .unwrap();
    assert!(
        results.iter().all(|result| result.kind == "module"),
        "{results:?}"
    );
}
