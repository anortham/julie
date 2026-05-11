use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::tree_sitter_certification_data::load_certification_data;
pub use crate::tree_sitter_certification_report::render_tree_sitter_certification_markdown;
use crate::tree_sitter_real_world::{
    TreeSitterRealWorldEvidenceReport, load_tree_sitter_real_world_evidence,
};
use crate::workspace_root;

pub const DEFAULT_TREE_SITTER_CERTIFICATION_REPORT: &str = "docs/LANGUAGE_CERTIFICATION_REPORT.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeSitterCertificationMetadata {
    pub head_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeSitterCertificationReport {
    pub head_sha: String,
    pub registry_row_count: usize,
    pub fixture_count: usize,
    pub historical_matrix_row_count: usize,
    pub raw_verification_report_count: usize,
    pub rows_with_open_gaps: Vec<String>,
    pub rows_without_gap_entries: Vec<String>,
    pub current_rows_missing_from_historical_matrix: Vec<String>,
    pub historical_rows_missing_from_current_registry: Vec<String>,
    pub gap_count_by_capability: BTreeMap<String, usize>,
    pub fixture_totals: FixtureEvidenceCounts,
    pub kind_coverage_totals: KindCoverageCounts,
    pub language_rows: Vec<LanguageCertificationRow>,
    pub open_gaps: Vec<CapabilityGapReport>,
    pub real_world_evidence: Option<RealWorldCertificationSummary>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FixtureEvidenceCounts {
    pub symbols: usize,
    pub relationships: usize,
    pub pending_relationships: usize,
    pub structured_pending_relationships: usize,
    pub identifiers: usize,
    pub types: usize,
    pub parse_diagnostics: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct KindCoverageCounts {
    pub symbols: usize,
    pub relationships: usize,
    pub identifiers: usize,
    pub body_spans: usize,
}

impl KindCoverageCounts {
    pub(crate) fn add_assign(&mut self, other: &KindCoverageCounts) {
        self.symbols += other.symbols;
        self.relationships += other.relationships;
        self.identifiers += other.identifiers;
        self.body_spans += other.body_spans;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageCertificationRow {
    pub language: String,
    pub parser_crate: String,
    pub dependency_status: String,
    pub fixture_count: usize,
    pub evidence: FixtureEvidenceCounts,
    pub kind_coverage: KindCoverageCounts,
    pub gap_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityGapReport {
    pub language: String,
    pub capability: String,
    pub status: String,
    pub required_closure: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealWorldCertificationSummary {
    pub profile: String,
    pub julie_head: String,
    pub verified_repo_count: usize,
    pub skipped_repo_count: usize,
    pub summary_flags: Vec<String>,
    pub rows: Vec<RealWorldCertificationRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealWorldCertificationRow {
    pub repo_name: String,
    pub language: String,
    pub status: String,
    pub file_count: i64,
    pub language_file_count: i64,
    pub symbol_count: i64,
    pub relationship_count: i64,
    pub identifier_count: i64,
    pub type_count: i64,
    pub parse_diagnostic_file_count: i64,
    pub hard_failures: Vec<String>,
}

pub fn run_tree_sitter_certification(
    out: &Path,
    check: bool,
    stdout: &mut dyn Write,
) -> Result<()> {
    let root = workspace_root();
    let metadata = TreeSitterCertificationMetadata {
        head_sha: current_git_head_sha(&root)?,
    };
    let report = build_tree_sitter_certification_report(&root, metadata)?;
    let rendered = render_tree_sitter_certification_markdown(&report);
    let out_path = resolve_output_path(&root, out);

    if check {
        let existing = fs::read_to_string(&out_path).with_context(|| {
            format!(
                "tree-sitter certification report is missing: {}; run `cargo xtask certify tree-sitter --out {}`",
                out_path.display(),
                out.display()
            )
        })?;
        if existing != rendered {
            bail!(
                "tree-sitter certification report is stale: {}; run `cargo xtask certify tree-sitter --out {}`",
                out_path.display(),
                out.display()
            );
        }
        writeln!(
            stdout,
            "tree-sitter certification report is current: {}",
            out_path.display()
        )?;
        return Ok(());
    }

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out_path, rendered)?;
    writeln!(
        stdout,
        "tree-sitter certification wrote {}",
        out_path.display()
    )?;
    Ok(())
}

pub fn build_tree_sitter_certification_report(
    root: &Path,
    metadata: TreeSitterCertificationMetadata,
) -> Result<TreeSitterCertificationReport> {
    let data = load_certification_data(root)?;
    let real_world_evidence =
        load_tree_sitter_real_world_evidence(root)?.map(real_world_summary_from_evidence);
    let current_languages = data
        .languages
        .iter()
        .map(|row| row.language.clone())
        .collect::<BTreeSet<_>>();
    let mut rows_with_open_gaps = BTreeSet::new();
    let mut rows_without_gap_entries = Vec::new();
    let mut gap_count_by_capability = BTreeMap::new();
    let mut fixture_totals = FixtureEvidenceCounts::default();
    let mut kind_coverage_totals = KindCoverageCounts::default();
    let mut language_rows = Vec::new();
    let mut open_gaps = Vec::new();
    let mut fixture_count = 0usize;

    for row in data.languages {
        fixture_count += row.fixture_count;
        fixture_totals.add_assign(&row.evidence);
        kind_coverage_totals.add_assign(&row.kind_coverage);
        if row.gaps.is_empty() {
            rows_without_gap_entries.push(row.language.clone());
        }
        if row.gaps.iter().any(|gap| gap.status == "open") {
            rows_with_open_gaps.insert(row.language.clone());
        }

        for gap in &row.gaps {
            *gap_count_by_capability
                .entry(gap.capability.clone())
                .or_insert(0) += 1;
            if gap.status == "open" {
                open_gaps.push(CapabilityGapReport {
                    language: row.language.clone(),
                    capability: gap.capability.clone(),
                    status: gap.status.clone(),
                    required_closure: gap.required_closure.clone(),
                    evidence: gap.evidence.clone(),
                });
            }
        }

        language_rows.push(LanguageCertificationRow {
            language: row.language,
            parser_crate: row.parser_crate,
            dependency_status: row.dependency_status,
            fixture_count: row.fixture_count,
            evidence: row.evidence,
            kind_coverage: row.kind_coverage,
            gap_count: row.gaps.len(),
        });
    }

    Ok(TreeSitterCertificationReport {
        head_sha: metadata.head_sha,
        registry_row_count: current_languages.len(),
        fixture_count,
        historical_matrix_row_count: data.historical_rows.len(),
        raw_verification_report_count: data.raw_verification_report_count,
        rows_with_open_gaps: rows_with_open_gaps.into_iter().collect(),
        rows_without_gap_entries,
        current_rows_missing_from_historical_matrix: current_languages
            .difference(&data.historical_rows)
            .cloned()
            .collect(),
        historical_rows_missing_from_current_registry: data
            .historical_rows
            .difference(&current_languages)
            .cloned()
            .collect(),
        gap_count_by_capability,
        fixture_totals,
        kind_coverage_totals,
        language_rows,
        open_gaps,
        real_world_evidence,
    })
}

pub(crate) fn current_git_head_sha(root: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .context("failed to run git rev-parse HEAD")?;
    if !output.status.success() {
        bail!("git rev-parse HEAD failed");
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn real_world_summary_from_evidence(
    evidence: TreeSitterRealWorldEvidenceReport,
) -> RealWorldCertificationSummary {
    RealWorldCertificationSummary {
        profile: evidence.profile,
        julie_head: evidence.julie_head,
        verified_repo_count: evidence.verified_repos.len(),
        skipped_repo_count: evidence.skipped_repos.len(),
        summary_flags: evidence.summary_flags,
        rows: evidence
            .verified_repos
            .into_iter()
            .map(|row| RealWorldCertificationRow {
                repo_name: row.repo_name,
                language: row.language,
                status: row.status,
                file_count: row.file_count,
                language_file_count: row.language_file_count,
                symbol_count: row.symbol_count,
                relationship_count: row.relationship_count,
                identifier_count: row.identifier_count,
                type_count: row.type_count,
                parse_diagnostic_file_count: row.parse_diagnostic_file_count,
                hard_failures: row.hard_failures,
            })
            .collect(),
    }
}

fn resolve_output_path(root: &Path, out: &Path) -> PathBuf {
    if out.is_absolute() {
        out.to_path_buf()
    } else {
        root.join(out)
    }
}

impl FixtureEvidenceCounts {
    pub(crate) fn add_assign(&mut self, other: &FixtureEvidenceCounts) {
        self.symbols += other.symbols;
        self.relationships += other.relationships;
        self.pending_relationships += other.pending_relationships;
        self.structured_pending_relationships += other.structured_pending_relationships;
        self.identifiers += other.identifiers;
        self.types += other.types;
        self.parse_diagnostics += other.parse_diagnostics;
    }
}
