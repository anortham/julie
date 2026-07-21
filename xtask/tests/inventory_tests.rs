use std::cell::RefCell;
use std::collections::BTreeMap;

use anyhow::Result;
use xtask::inventory::{
    InventoryExecutor, InventoryTarget, render_inventory_report, run_inventory,
};
use xtask::manifest::TestManifest;

#[test]
fn inventory_tests_reports_duplicate_selected_tests() {
    let manifest = sample_manifest();
    let executor = FakeInventoryExecutor::new([
        (
            "cargo nextest list --lib tests::tools::search::line_ -- --skip search_quality",
            "tests::tools::search::line_mode::test_alpha\n\
tests::tools::search::line_mode::test_beta\n",
        ),
        (
            "cargo nextest list --lib tests::tools::search::file_ -- --skip search_quality",
            "tests::tools::search::line_mode::test_beta\n\
tests::tools::search::file_mode::test_gamma\n",
        ),
    ]);

    let report = run_inventory(
        &manifest,
        &InventoryTarget::Bucket("tools-search-line-file".to_string()),
        &executor,
    )
    .unwrap();
    let output = render_inventory_report(&report);

    assert_eq!(
        executor.calls(),
        vec![
            "cargo nextest list --lib tests::tools::search::line_ -- --skip search_quality",
            "cargo nextest list --lib tests::tools::search::file_ -- --skip search_quality",
        ]
    );
    assert!(output.contains("INVENTORY: target bucket tools-search-line-file"));
    assert!(output.contains("INVENTORY: selected tests = 3"));
    assert!(output.contains("INVENTORY: duplicate selected tests = 1"));
    assert!(output.contains("DUPLICATE tests::tools::search::line_mode::test_beta"));
    assert!(output.contains("command = cargo nextest list --lib tests::tools::search::line_"));
    assert!(output.contains("command = cargo nextest list --lib tests::tools::search::file_"));
}

#[test]
fn inventory_tests_reports_non_inventoryable_commands() {
    let manifest = sample_manifest();
    let executor = FakeInventoryExecutor::new([(
        "cargo nextest list --lib tests::cli_tests",
        "tests::cli_tests::test_one\n",
    )]);

    let report = run_inventory(
        &manifest,
        &InventoryTarget::Bucket("cli".to_string()),
        &executor,
    )
    .unwrap();
    let output = render_inventory_report(&report);

    assert_eq!(
        executor.calls(),
        vec!["cargo nextest list --lib tests::cli_tests"]
    );
    assert!(output.contains("INVENTORY: target bucket cli"));
    assert!(output.contains("INVENTORY: selected tests = 1"));
    assert!(output.contains("INVENTORY: non-inventoryable commands = 1"));
    assert!(output.contains("NON-INVENTORYABLE cli: cargo build"));
}

#[test]
fn inventory_tests_tier_target_aggregates_bucket_commands() {
    let manifest = sample_manifest();
    let executor = FakeInventoryExecutor::new([
        (
            "cargo nextest list --lib tests::cli_tests",
            "tests::cli_tests::test_one\n",
        ),
        (
            "cargo nextest list --lib tests::tools::search::line_ -- --skip search_quality",
            "tests::tools::search::line_mode::test_beta\n\
tests::tools::search::line_mode::test_delta\n",
        ),
        (
            "cargo nextest list --lib tests::tools::search::file_ -- --skip search_quality",
            "tests::tools::search::line_mode::test_delta\n\
tests::tools::search::file_mode::test_gamma\n",
        ),
    ]);

    let report = run_inventory(
        &manifest,
        &InventoryTarget::Tier("dev".to_string()),
        &executor,
    )
    .unwrap();
    let output = render_inventory_report(&report);

    assert_eq!(
        executor.calls(),
        vec![
            "cargo nextest list --lib tests::cli_tests",
            "cargo nextest list --lib tests::tools::search::line_ -- --skip search_quality",
            "cargo nextest list --lib tests::tools::search::file_ -- --skip search_quality",
        ]
    );
    assert!(output.contains("INVENTORY: target tier dev"));
    assert!(output.contains("INVENTORY: selected tests = 4"));
    assert!(output.contains("INVENTORY: duplicate selected tests = 1"));
    assert!(output.contains("DUPLICATE tests::tools::search::line_mode::test_delta"));
    assert!(output.contains("NON-INVENTORYABLE cli: cargo build"));
}

fn sample_manifest() -> TestManifest {
    TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
dev = ["cli", "tools-search-line-file"]

[buckets.cli]
expected_seconds = 5
timeout_seconds = 30
commands = [
  "cargo nextest run --lib tests::cli_tests",
  "cargo build",
]

[buckets.tools-search-line-file]
expected_seconds = 10
timeout_seconds = 40
commands = [
  "cargo nextest run --lib tests::tools::search::line_ -- --skip search_quality",
  "cargo nextest run --lib tests::tools::search::file_ -- --skip search_quality",
]
"#,
    )
    .unwrap()
}

struct FakeInventoryExecutor {
    outputs: BTreeMap<String, String>,
    calls: RefCell<Vec<String>>,
}

impl FakeInventoryExecutor {
    fn new<const N: usize>(entries: [(&str, &str); N]) -> Self {
        Self {
            outputs: entries
                .into_iter()
                .map(|(command, output)| (command.to_string(), output.to_string()))
                .collect(),
            calls: RefCell::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl InventoryExecutor for FakeInventoryExecutor {
    fn list(&self, command: &str) -> Result<String> {
        self.calls.borrow_mut().push(command.to_string());
        self.outputs
            .get(command)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected command: {command}"))
    }
}
