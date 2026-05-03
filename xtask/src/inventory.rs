use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::manifest::TestManifest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryTarget {
    Tier(String),
    Bucket(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryReport {
    pub target: InventoryTarget,
    pub selected_tests: BTreeSet<String>,
    pub duplicates: Vec<DuplicateSelection>,
    pub non_inventoryable: Vec<NonInventoryableCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateSelection {
    pub test_name: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonInventoryableCommand {
    pub bucket_name: String,
    pub command: String,
}

pub trait InventoryExecutor {
    fn list(&self, command: &str) -> Result<String>;
}

pub struct ProcessInventoryExecutor;

impl InventoryExecutor for ProcessInventoryExecutor {
    fn list(&self, command: &str) -> Result<String> {
        let output = shell_command(command)
            .output()
            .with_context(|| format!("failed to run inventory command `{command}`"))?;

        if !output.status.success() {
            bail!(
                "inventory command `{command}` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

pub fn run_inventory<E: InventoryExecutor>(
    manifest: &TestManifest,
    target: &InventoryTarget,
    executor: &E,
) -> Result<InventoryReport> {
    let bucket_names = bucket_names_for_target(manifest, target)?;
    let mut selected_tests = BTreeSet::new();
    let mut selected_by_test: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut non_inventoryable = Vec::new();

    for bucket_name in bucket_names {
        let Some(bucket) = manifest.buckets.get(&bucket_name) else {
            bail!("inventory target references missing bucket `{bucket_name}`");
        };

        for raw_command in &bucket.commands {
            let Some(list_command) = inventory_command_for(raw_command) else {
                non_inventoryable.push(NonInventoryableCommand {
                    bucket_name: bucket_name.clone(),
                    command: raw_command.clone(),
                });
                continue;
            };

            let output = executor.list(&list_command)?;
            for test_name in parse_nextest_list_output(&output) {
                selected_tests.insert(test_name.clone());
                selected_by_test
                    .entry(test_name)
                    .or_default()
                    .push(list_command.clone());
            }
        }
    }

    let duplicates = selected_by_test
        .into_iter()
        .filter_map(|(test_name, commands)| {
            if commands.len() > 1 {
                Some(DuplicateSelection {
                    test_name,
                    commands,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(InventoryReport {
        target: target.clone(),
        selected_tests,
        duplicates,
        non_inventoryable,
    })
}

pub fn render_inventory_report(report: &InventoryReport) -> String {
    let mut output = format!("INVENTORY: target {}\n", render_target(&report.target));
    output.push_str(&format!(
        "INVENTORY: selected tests = {}\n",
        report.selected_tests.len()
    ));
    output.push_str(&format!(
        "INVENTORY: duplicate selected tests = {}\n",
        report.duplicates.len()
    ));

    for duplicate in &report.duplicates {
        output.push_str(&format!("DUPLICATE {}\n", duplicate.test_name));
        for command in &duplicate.commands {
            output.push_str(&format!("  command = {command}\n"));
        }
    }

    output.push_str(&format!(
        "INVENTORY: non-inventoryable commands = {}\n",
        report.non_inventoryable.len()
    ));
    for command in &report.non_inventoryable {
        output.push_str(&format!(
            "NON-INVENTORYABLE {}: {}\n",
            command.bucket_name, command.command
        ));
    }

    output
}

fn bucket_names_for_target(
    manifest: &TestManifest,
    target: &InventoryTarget,
) -> Result<Vec<String>> {
    match target {
        InventoryTarget::Tier(name) => manifest
            .tiers
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("unknown inventory tier `{name}`")),
        InventoryTarget::Bucket(name) => {
            if manifest.buckets.contains_key(name) {
                Ok(vec![name.clone()])
            } else {
                Err(anyhow!("unknown inventory bucket `{name}`"))
            }
        }
    }
}

fn inventory_command_for(command: &str) -> Option<String> {
    command
        .strip_prefix("cargo nextest run")
        .map(|rest| format!("cargo nextest list{rest}"))
}

fn parse_nextest_list_output(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("Finished "))
        .filter(|line| !line.starts_with("Listing "))
        .filter(|line| !line.starts_with("warning:"))
        .map(ToOwned::to_owned)
        .collect()
}

fn render_target(target: &InventoryTarget) -> String {
    match target {
        InventoryTarget::Tier(name) => format!("tier {name}"),
        InventoryTarget::Bucket(name) => format!("bucket {name}"),
    }
}

#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    let mut shell = Command::new("sh");
    shell.arg("-c").arg(command);
    shell
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut shell = Command::new("cmd");
    shell.arg("/C").arg(command);
    shell
}
