use super::*;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

struct FakeExecutor {
    expectations: Rc<RefCell<HashMap<String, VecDeque<CommandOutcome>>>>,
    calls: Rc<RefCell<Vec<(String, String, Duration)>>>,
}

impl FakeExecutor {
    fn with_outcomes(entries: &[(&str, CommandOutcome)]) -> Self {
        let mut expectations = HashMap::new();
        for (command, outcome) in entries {
            expectations
                .entry((*command).to_string())
                .or_insert_with(VecDeque::new)
                .push_back(outcome.clone());
        }

        Self {
            expectations: Rc::new(RefCell::new(expectations)),
            calls: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls
            .borrow()
            .iter()
            .map(|(_, command, _)| command.clone())
            .collect()
    }
}

impl CommandExecutor for FakeExecutor {
    fn run(&self, bucket: &str, command: &str, timeout: Duration) -> anyhow::Result<CommandResult> {
        self.calls
            .borrow_mut()
            .push((bucket.to_string(), command.to_string(), timeout));

        let outcome = self
            .expectations
            .borrow_mut()
            .get_mut(command)
            .and_then(|queue| queue.pop_front())
            .unwrap_or_else(|| panic!("unexpected command: {command}"));

        Ok(CommandResult {
            outcome,
            captured: String::new(),
        })
    }
}

fn manifest_with_program_buckets() -> TestManifest {
    TestManifest::from_str(
        r#"
[tiers]
fast = ["registry"]
daemon = ["registry"]
workspace-init = ["workspace-init"]
integration = ["integration"]

[buckets.registry]
expected_seconds = 1
timeout_seconds = 2
commands = ["registry cmd"]

[buckets.workspace-init]
expected_seconds = 1
timeout_seconds = 2
commands = ["workspace init cmd"]

[buckets.integration]
expected_seconds = 1
timeout_seconds = 2
commands = ["integration cmd"]

[buckets.system-health]
expected_seconds = 30
timeout_seconds = 120
commands = ["cargo nextest run --lib tests::integration::system_health"]
"#,
    )
    .unwrap()
}

#[test]
fn runner_tests_declared_expected_seconds_prices_system_health_from_manifest() {
    let manifest = TestManifest::load(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_tiers.toml"),
    )
    .expect("load checked-in test_tiers.toml");

    assert!(
        manifest.buckets.contains_key("system-health"),
        "system-health must live in the manifest so pricing is uniform"
    );

    assert_eq!(
        declared_expected_seconds(&manifest, ["system-health"]),
        30,
        "system-health declared expected_seconds must price at 30s"
    );

    let plan = resolve_bucket_plan(&manifest, "system-health")
        .expect("system-health resolves via shared bucket-plan path");
    assert_eq!(plan.expected_seconds, 30);
    assert_eq!(plan.timeout_seconds, 120);
    assert_eq!(
        plan.commands,
        vec!["cargo nextest run --lib tests::integration::system_health".to_string()]
    );
}

#[test]
fn runner_tests_render_manifest_listing_includes_program_tiers() {
    let manifest = manifest_with_program_buckets();

    let listing = render_manifest_listing(&manifest);

    assert!(listing.contains("PROGRAM TIERS"), "{listing}");
    assert!(listing.contains("reliability"), "{listing}");
    assert!(listing.contains("benchmark"), "{listing}");
    assert!(listing.contains("system-health"), "{listing}");
}

#[test]
fn runner_tests_reliability_tier_routes_program_bucket_sequence() {
    let manifest = manifest_with_program_buckets();
    let executor = FakeExecutor::with_outcomes(&[
        (
            "registry cmd",
            CommandOutcome::Passed {
                elapsed: Duration::from_millis(10),
            },
        ),
        (
            "workspace init cmd",
            CommandOutcome::Passed {
                elapsed: Duration::from_millis(15),
            },
        ),
        (
            "integration cmd",
            CommandOutcome::Passed {
                elapsed: Duration::from_millis(20),
            },
        ),
    ]);
    let mut output = Vec::new();

    let summary = run_tier(&manifest, "reliability", 1, false, &executor, &mut output).unwrap();

    assert_eq!(
        summary.bucket_names,
        vec![
            "registry".to_string(),
            "workspace-init".to_string(),
            "integration".to_string(),
        ]
    );
    assert_eq!(
        executor.calls(),
        vec![
            "registry cmd".to_string(),
            "workspace init cmd".to_string(),
            "integration cmd".to_string(),
        ]
    );
}

#[test]
fn runner_tests_benchmark_bucket_runs_system_health_command() {
    let manifest = manifest_with_program_buckets();
    let executor = FakeExecutor::with_outcomes(&[
        (
            "cargo nextest run --no-run --lib",
            CommandOutcome::Passed {
                elapsed: Duration::from_millis(5),
            },
        ),
        (
            "cargo nextest run --lib tests::integration::system_health",
            CommandOutcome::Passed {
                elapsed: Duration::from_millis(25),
            },
        ),
    ]);
    let mut output = Vec::new();

    let summary = run_bucket(&manifest, "system-health", 1, false, &executor, &mut output).unwrap();

    assert_eq!(summary.bucket_names, vec!["system-health".to_string()]);
    assert_eq!(
        executor.calls(),
        vec![
            "cargo nextest run --no-run --lib".to_string(),
            "cargo nextest run --lib tests::integration::system_health".to_string(),
        ]
    );
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("END system-health PASS")
    );
}

#[test]
fn runner_tests_summary_includes_prebuild_elapsed() {
    let manifest = manifest_with_program_buckets();
    let executor = FakeExecutor::with_outcomes(&[
        (
            "cargo nextest run --no-run --lib",
            CommandOutcome::Passed {
                elapsed: Duration::from_millis(47_000),
            },
        ),
        (
            "cargo nextest run --lib tests::integration::system_health",
            CommandOutcome::Passed {
                elapsed: Duration::from_secs(1),
            },
        ),
    ]);
    let mut output = Vec::new();

    let summary = run_tier(&manifest, "benchmark", 1, false, &executor, &mut output).unwrap();

    assert_eq!(
        summary.prebuild_elapsed,
        Duration::from_millis(47_000),
        "prebuild_elapsed should record the prebuild command duration"
    );
    assert_eq!(
        summary.total_elapsed,
        Duration::from_secs(1),
        "total_elapsed must remain warm bucket wall, not include prebuild"
    );

    let rendered = render_summary(&summary);
    assert!(
        rendered.contains("PREBUILD: 47.0s"),
        "rendered summary should show prebuild duration, got:\n{rendered}"
    );
    assert!(
        rendered.contains("warm"),
        "rendered summary should label warm bucket wall, got:\n{rendered}"
    );
    assert!(
        rendered.contains("COLD WALL: 48.0s"),
        "cold wall should be prebuild + warm buckets, got:\n{rendered}"
    );
}
