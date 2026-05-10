use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::tree_sitter_certification::FixtureEvidenceCounts;

pub struct LoadedCertificationData {
    pub languages: Vec<LoadedCapabilityRow>,
    pub historical_rows: BTreeSet<String>,
    pub raw_verification_report_count: usize,
}

pub struct LoadedCapabilityRow {
    pub language: String,
    pub parser_crate: String,
    pub dependency_status: String,
    pub fixture_count: usize,
    pub evidence: FixtureEvidenceCounts,
    pub gaps: Vec<LoadedCapabilityGap>,
}

pub struct LoadedCapabilityGap {
    pub capability: String,
    pub status: String,
    pub required_closure: String,
    pub evidence: String,
}

#[derive(Debug, Deserialize)]
struct CapabilityMatrix {
    languages: Vec<CapabilityRow>,
}

#[derive(Debug, Deserialize)]
struct CapabilityRow {
    language: String,
    parser_crate: String,
    extensions: Vec<String>,
    dependency_status: String,
    target_capabilities: CapabilityFlags,
    capabilities: CapabilityFlags,
    fixtures: Vec<FixtureRow>,
    #[serde(default)]
    capability_gaps: Vec<CapabilityGap>,
}

#[derive(Debug, Deserialize)]
struct CapabilityFlags {
    symbols: bool,
    relationships: bool,
    pending_relationships: bool,
    identifiers: bool,
    types: bool,
}

#[derive(Debug, Deserialize)]
struct FixtureRow {
    name: String,
    source: String,
    expected: String,
}

#[derive(Debug, Deserialize)]
struct CapabilityGap {
    capability: String,
    status: String,
    reason: String,
    required_closure: String,
    evidence: EvidenceRef,
    #[serde(default)]
    #[allow(dead_code)]
    planned_closure_task: Option<String>,
}

/// Typed evidence reference. Mirrors the
/// `crates/julie-extractors/src/tests/capability_matrix.rs::EvidenceRef`
/// shape so the xtask certify command and the in-crate matrix tests parse
/// the same JSON.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EvidenceRef {
    Test {
        #[allow(dead_code)]
        kind: TestKind,
        value: String,
        #[allow(dead_code)]
        command: String,
    },
    Fixture {
        #[allow(dead_code)]
        kind: FixtureKind,
        value: String,
        #[allow(dead_code)]
        command: String,
    },
    Commit {
        #[allow(dead_code)]
        kind: CommitKind,
        value: String,
        #[allow(dead_code)]
        command: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TestKind {
    Test,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum FixtureKind {
    Fixture,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CommitKind {
    Commit,
}

impl EvidenceRef {
    /// Display string used in the certification markdown report and the
    /// `LoadedCapabilityGap::evidence` field. Formatted as "kind:value".
    fn display(&self) -> String {
        match self {
            EvidenceRef::Test { value, .. } => format!("test:{value}"),
            EvidenceRef::Fixture { value, .. } => format!("fixture:{value}"),
            EvidenceRef::Commit { value, .. } => format!("commit:{value}"),
        }
    }
}

pub fn load_certification_data(root: &Path) -> Result<LoadedCertificationData> {
    let matrix = load_matrix(root)?;
    let historical_rows = load_historical_matrix_rows(root)?;
    let raw_verification_report_count = count_raw_verification_reports(root)?;
    let mut seen_languages = BTreeSet::new();
    let mut languages = Vec::new();

    for row in matrix.languages {
        if !seen_languages.insert(row.language.clone()) {
            bail!("duplicate capability matrix row for {}", row.language);
        }
        validate_matrix_row(root, &row)?;

        let mut evidence = FixtureEvidenceCounts::default();
        for fixture in &row.fixtures {
            evidence.add_assign(&load_fixture_evidence(root, fixture, &row.language)?);
        }

        let gaps = row
            .capability_gaps
            .iter()
            .map(|gap| LoadedCapabilityGap {
                capability: gap.capability.clone(),
                status: gap.status.clone(),
                required_closure: gap.required_closure.clone(),
                evidence: gap.evidence.display(),
            })
            .collect();

        languages.push(LoadedCapabilityRow {
            language: row.language,
            parser_crate: row.parser_crate,
            dependency_status: row.dependency_status,
            fixture_count: row.fixtures.len(),
            evidence,
            gaps,
        });
    }

    Ok(LoadedCertificationData {
        languages,
        historical_rows,
        raw_verification_report_count,
    })
}

fn load_matrix(root: &Path) -> Result<CapabilityMatrix> {
    let path = root.join("fixtures/extraction/capabilities.json");
    let json = fs::read_to_string(&path)
        .with_context(|| format!("failed to read capability matrix at {}", path.display()))?;
    serde_json::from_str(&json)
        .with_context(|| format!("failed to parse capability matrix at {}", path.display()))
}

fn validate_matrix_row(root: &Path, row: &CapabilityRow) -> Result<()> {
    if row.parser_crate.trim().is_empty() {
        bail!("{} is missing parser_crate", row.language);
    }
    if row.extensions.is_empty() {
        bail!("{} is missing extension coverage", row.language);
    }
    if !matches!(
        row.dependency_status.as_str(),
        "current" | "upgrade_available" | "git_pinned" | "held"
    ) {
        bail!(
            "{} has unsupported dependency_status {}",
            row.language,
            row.dependency_status
        );
    }
    if row.fixtures.is_empty() {
        bail!("{} must have at least one golden fixture", row.language);
    }

    validate_target_capability(row, "symbols", row.target_capabilities.symbols)?;
    validate_target_capability(row, "relationships", row.target_capabilities.relationships)?;
    validate_target_capability(
        row,
        "pending_relationships",
        row.target_capabilities.pending_relationships,
    )?;
    validate_target_capability(row, "identifiers", row.target_capabilities.identifiers)?;
    validate_target_capability(row, "types", row.target_capabilities.types)?;

    for fixture in &row.fixtures {
        validate_fixture_paths(root, row, fixture)?;
    }
    for gap in &row.capability_gaps {
        validate_gap(root, row, gap)?;
    }

    Ok(())
}

fn validate_fixture_paths(root: &Path, row: &CapabilityRow, fixture: &FixtureRow) -> Result<()> {
    if fixture.name.trim().is_empty() {
        bail!("{} has an unnamed fixture", row.language);
    }
    let source = root.join(&fixture.source);
    if !source.is_file() {
        bail!(
            "{} fixture source does not exist: {}",
            row.language,
            source.display()
        );
    }
    let expected = root.join(&fixture.expected);
    if !expected.is_file() {
        bail!(
            "{} fixture expected output does not exist: {}",
            row.language,
            expected.display()
        );
    }
    Ok(())
}

fn validate_gap(root: &Path, row: &CapabilityRow, gap: &CapabilityGap) -> Result<()> {
    if !matches!(
        gap.capability.as_str(),
        "symbols" | "relationships" | "pending_relationships" | "identifiers" | "types"
    ) {
        bail!(
            "{} has an unknown capability gap: {}",
            row.language,
            gap.capability
        );
    }
    if !matches!(gap.status.as_str(), "open" | "exception") {
        bail!(
            "{} has unsupported gap status {} for {}",
            row.language,
            gap.status,
            gap.capability
        );
    }
    if gap.reason.trim().is_empty() {
        bail!(
            "{} {} gap is missing a reason",
            row.language,
            gap.capability
        );
    }
    if gap.required_closure.trim().is_empty() {
        bail!(
            "{} {} gap is missing required closure text",
            row.language,
            gap.capability
        );
    }
    match &gap.evidence {
        EvidenceRef::Fixture { value, .. } => {
            if !root.join(value).exists() {
                bail!(
                    "{} {} gap evidence path does not exist: {}",
                    row.language,
                    gap.capability,
                    value
                );
            }
        }
        EvidenceRef::Commit { value, .. } => {
            if value.len() != 40 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
                bail!(
                    "{} {} gap commit evidence must be a 40-char hex SHA: {}",
                    row.language,
                    gap.capability,
                    value
                );
            }
            let output = std::process::Command::new("git")
                .args(["cat-file", "-e", value])
                .current_dir(root)
                .output()
                .with_context(|| format!("git cat-file invocation for {value}"))?;
            if !output.status.success() {
                bail!(
                    "{} {} gap commit evidence does not resolve via git cat-file: {}",
                    row.language,
                    gap.capability,
                    value
                );
            }
        }
        EvidenceRef::Test { value, command, .. } => {
            if value.is_empty() {
                bail!(
                    "{} {} gap test evidence has empty value",
                    row.language,
                    gap.capability
                );
            }
            if !command.starts_with("cargo nextest") {
                bail!(
                    "{} {} gap test evidence command must start with `cargo nextest`: {}",
                    row.language,
                    gap.capability,
                    command
                );
            }
        }
    }
    Ok(())
}

fn validate_target_capability(
    row: &CapabilityRow,
    capability: &str,
    target_enabled: bool,
) -> Result<()> {
    let implemented = implemented_capability(row, capability);
    let gap = row
        .capability_gaps
        .iter()
        .find(|gap| gap.capability == capability);

    if !target_enabled {
        if implemented {
            bail!(
                "{} implements {} even though the target marks it non-applicable",
                row.language,
                capability
            );
        }
        if gap.is_none() {
            bail!(
                "{} sets target_capabilities.{} = false but has no matching capability_gaps entry",
                row.language,
                capability
            );
        }
        return Ok(());
    }

    if !implemented && gap.is_none() {
        bail!(
            "{} target capability {} is true but implementation is false and no gap is recorded",
            row.language,
            capability
        );
    }

    Ok(())
}

fn implemented_capability(row: &CapabilityRow, capability: &str) -> bool {
    match capability {
        "symbols" => row.capabilities.symbols,
        "relationships" => row.capabilities.relationships,
        "pending_relationships" => row.capabilities.pending_relationships,
        "identifiers" => row.capabilities.identifiers,
        "types" => row.capabilities.types,
        other => panic!("unknown capability {other}"),
    }
}

fn load_fixture_evidence(
    root: &Path,
    fixture: &FixtureRow,
    language: &str,
) -> Result<FixtureEvidenceCounts> {
    let expected = load_expected_fixture(root, fixture)?;
    assert_fixture_pending_parity(&expected, language, &fixture.name)?;

    Ok(FixtureEvidenceCounts {
        symbols: required_array_len(&expected, "symbols", language, &fixture.name)?,
        relationships: required_array_len(&expected, "relationships", language, &fixture.name)?,
        pending_relationships: required_array_len(
            &expected,
            "pending_relationships",
            language,
            &fixture.name,
        )?,
        structured_pending_relationships: required_array_len(
            &expected,
            "structured_pending_relationships",
            language,
            &fixture.name,
        )?,
        identifiers: required_array_len(&expected, "identifiers", language, &fixture.name)?,
        types: required_array_len(&expected, "types", language, &fixture.name)?,
        parse_diagnostics: required_array_len(
            &expected,
            "parse_diagnostics",
            language,
            &fixture.name,
        )?,
    })
}

fn load_expected_fixture(root: &Path, fixture: &FixtureRow) -> Result<Value> {
    let path = root.join(&fixture.expected);
    let json = fs::read_to_string(&path)
        .with_context(|| format!("failed to read expected fixture at {}", path.display()))?;
    serde_json::from_str(&json)
        .with_context(|| format!("failed to parse expected fixture at {}", path.display()))
}

fn required_array_len(
    expected: &Value,
    field: &str,
    language: &str,
    fixture_name: &str,
) -> Result<usize> {
    expected
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::len)
        .with_context(|| format!("{language} fixture {fixture_name} is missing {field}"))
}

fn assert_fixture_pending_parity(
    expected: &Value,
    language: &str,
    fixture_name: &str,
) -> Result<()> {
    let pending = expected
        .get("pending_relationships")
        .and_then(Value::as_array)
        .with_context(|| {
            format!("{language} fixture {fixture_name} is missing pending_relationships")
        })?;
    let structured_pending = expected
        .get("structured_pending_relationships")
        .and_then(Value::as_array)
        .with_context(|| {
            format!("{language} fixture {fixture_name} is missing structured_pending_relationships")
        })?;
    let degraded = structured_pending
        .iter()
        .map(|item| {
            item.get("pending").cloned().with_context(|| {
                format!(
                    "{language} fixture {fixture_name} has structured pending without pending payload"
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;

    if pending != degraded.as_slice() {
        bail!(
            "{language} fixture {fixture_name} must keep pending_relationships aligned with degraded structured_pending_relationships"
        );
    }

    Ok(())
}

fn load_historical_matrix_rows(root: &Path) -> Result<BTreeSet<String>> {
    let path = root.join("docs/LANGUAGE_VERIFICATION_RESULTS.md");
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read historical matrix at {}", path.display()))?;
    let summary = summary_matrix_section(&contents);
    let mut languages = BTreeSet::new();

    for line in summary.lines() {
        let Some(language) = markdown_table_first_cell(line) else {
            continue;
        };
        if language == "Language" || language.starts_with("---") {
            continue;
        }
        if let Some(normalized) = normalize_language_name(language) {
            languages.insert(normalized);
        }
    }

    Ok(languages)
}

fn summary_matrix_section(contents: &str) -> &str {
    let start = contents
        .find("## Summary Matrix")
        .map(|index| &contents[index..])
        .unwrap_or(contents);
    if let Some(end) = start.find("## Recommended Reference Projects") {
        &start[..end]
    } else {
        start
    }
}

fn markdown_table_first_cell(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return None;
    }
    trimmed.split('|').nth(1).map(str::trim)
}

fn normalize_language_name(language: &str) -> Option<String> {
    let cleaned = language
        .trim()
        .trim_matches('`')
        .trim_matches('*')
        .trim()
        .to_ascii_lowercase();
    let normalized = match cleaned.as_str() {
        "" => return None,
        "c#" => "csharp",
        "c++" => "cpp",
        "vb.net" => "vbnet",
        "typescript" => "typescript",
        "javascript" => "javascript",
        other => other,
    };
    Some(normalized.to_string())
}

fn count_raw_verification_reports(root: &Path) -> Result<usize> {
    let dir = root.join("docs/verification");
    if !dir.exists() {
        return Ok(0);
    }
    let mut count = 0usize;
    for entry in fs::read_dir(&dir)
        .with_context(|| format!("failed to read verification report dir {}", dir.display()))?
    {
        let entry = entry?;
        if entry.path().extension().and_then(|ext| ext.to_str()) == Some("md") {
            count += 1;
        }
    }
    Ok(count)
}
