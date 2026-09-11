use std::cell::RefCell;
use std::time::Duration;

use xtask::cli::Tier;
use xtask::runner::{CommandExecutor, CommandOutcome, run_tier, tier_commands};

const DEV: &[&str] = &[
    "cargo build -p julie --bin julie-server",
    "cargo nextest run --workspace -E 'not (test(/search_quality/) | test(/fixtures::julie_db/) | test(/dogfood/))'",
    "cargo nextest run --lib -p julie --run-ignored only -E 'test(/tests::cli::/)'",
];

const DOGFOOD: &[&str] = &[
    "cargo test --lib -p julie ensure_julie_fixture -- --ignored --nocapture",
    "cargo nextest run --workspace -E 'test(/search_quality/) | test(/fixtures::julie_db/) | test(/dogfood/)'",
];

struct FakeExecutor {
    failing_command: Option<&'static str>,
    calls: RefCell<Vec<String>>,
}

impl FakeExecutor {
    fn passing() -> Self {
        Self {
            failing_command: None,
            calls: RefCell::new(Vec::new()),
        }
    }

    fn failing_at(command: &'static str) -> Self {
        Self {
            failing_command: Some(command),
            calls: RefCell::new(Vec::new()),
        }
    }
}

impl CommandExecutor for FakeExecutor {
    fn run(&self, command: &str) -> anyhow::Result<CommandOutcome> {
        self.calls.borrow_mut().push(command.to_string());
        let elapsed = Duration::from_millis(1500);
        if self.failing_command == Some(command) {
            Ok(CommandOutcome::Failed {
                elapsed,
                exit_code: Some(101),
            })
        } else {
            Ok(CommandOutcome::Passed { elapsed })
        }
    }
}

#[test]
fn tier_tests_dev_is_build_then_workspace_then_ignored_cli() {
    assert_eq!(tier_commands(Tier::Dev), DEV);
}

#[test]
fn tier_tests_dogfood_is_fixture_then_filtered_workspace() {
    assert_eq!(tier_commands(Tier::Dogfood), DOGFOOD);
}

#[test]
fn tier_tests_full_is_dev_then_dogfood() {
    let expected: Vec<&str> = DEV.iter().chain(DOGFOOD).copied().collect();
    assert_eq!(tier_commands(Tier::Full), expected);
}

#[test]
fn tier_tests_passing_run_prints_run_pass_and_summary() {
    let executor = FakeExecutor::passing();
    let mut output = Vec::new();

    run_tier(Tier::Dogfood, &executor, &mut output).unwrap();

    let expected = format!(
        "RUN {0}\nPASS {0} (1.5s)\nRUN {1}\nPASS {1} (1.5s)\nSUMMARY: dogfood 2/2 commands in 3.0s\n",
        DOGFOOD[0], DOGFOOD[1]
    );
    assert_eq!(String::from_utf8(output).unwrap(), expected);
    assert_eq!(*executor.calls.borrow(), DOGFOOD);
}

#[test]
fn tier_tests_failing_command_stops_the_tier() {
    let executor = FakeExecutor::failing_at(DEV[1]);
    let mut output = Vec::new();

    let error = run_tier(Tier::Dev, &executor, &mut output).unwrap_err();

    let expected = format!(
        "RUN {0}\nPASS {0} (1.5s)\nRUN {1}\nFAIL {1} (1.5s)\nSUMMARY: dev 1/3 commands in 3.0s\n",
        DEV[0], DEV[1]
    );
    assert_eq!(String::from_utf8(output).unwrap(), expected);
    assert_eq!(*executor.calls.borrow(), DEV[..2]);
    assert!(error.to_string().contains("FAIL"), "{error}");
}
