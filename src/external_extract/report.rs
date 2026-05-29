use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::cli_tools::OutputFormat;
use crate::external_extract::{ExternalExtractArgs, ExternalInfoSchemaState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalExtractStatus {
    Changed,
    Unchanged,
    Ignored,
    Deleted,
    NotFound,
    Scanned,
    Rebuilt,
    Analyzed,
    Failed,
}

impl ExternalExtractStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Unchanged => "unchanged",
            Self::Ignored => "ignored",
            Self::Deleted => "deleted",
            Self::NotFound => "not_found",
            Self::Scanned => "scanned",
            Self::Rebuilt => "rebuilt",
            Self::Analyzed => "analyzed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalExtractError {
    pub code: String,
    pub message: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalExtractReport {
    pub status: ExternalExtractStatus,
    pub operation: String,
    pub workspace_id: Option<String>,
    #[serde(rename = "db_path")]
    pub db: PathBuf,
    pub root: Option<PathBuf>,
    pub julie_version: Option<String>,
    pub schema_version: Option<i32>,
    pub schema_state: Option<ExternalInfoSchemaState>,
    pub extract_contract_version: Option<i32>,
    pub revision: Option<i64>,
    pub analyzed_revision: Option<i64>,
    pub analysis_state: Option<String>,
    pub missing_metadata_keys: Vec<String>,
    pub files_scanned: u64,
    pub files_updated: u64,
    pub files_deleted: u64,
    pub symbols_extracted: u64,
    pub files_total: u64,
    pub symbols_total: u64,
    pub relationships_total: u64,
    pub identifiers_total: u64,
    pub types_total: u64,
    pub type_arguments_total: u64,
    pub errors: Vec<ExternalExtractError>,
}

pub fn format_external_extract_report(
    report: &ExternalExtractReport,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(report)?),
        OutputFormat::Text => Ok(format_external_extract_text(report)),
        OutputFormat::Markdown => Ok(format_external_extract_markdown(report)),
    }
}

pub fn failed_external_extract_report(
    args: &ExternalExtractArgs,
    error: &anyhow::Error,
) -> ExternalExtractReport {
    ExternalExtractReport {
        status: ExternalExtractStatus::Failed,
        operation: args.command.as_str().to_string(),
        workspace_id: args.workspace_id.clone(),
        db: args.db.clone(),
        root: args.root.clone(),
        julie_version: None,
        schema_version: None,
        schema_state: None,
        extract_contract_version: None,
        revision: None,
        analyzed_revision: None,
        analysis_state: None,
        missing_metadata_keys: Vec::new(),
        files_scanned: 0,
        files_updated: 0,
        files_deleted: 0,
        symbols_extracted: 0,
        files_total: 0,
        symbols_total: 0,
        relationships_total: 0,
        identifiers_total: 0,
        types_total: 0,
        type_arguments_total: 0,
        errors: vec![ExternalExtractError {
            code: "external_extract_error".to_string(),
            message: error.to_string(),
            path: None,
        }],
    }
}

fn format_external_extract_text(report: &ExternalExtractReport) -> String {
    let mut fields = vec![
        format!("db_path={}", report.db.display()),
        format!("files_scanned={}", report.files_scanned),
        format!("files_updated={}", report.files_updated),
        format!("files_deleted={}", report.files_deleted),
        format!("symbols_extracted={}", report.symbols_extracted),
        format!("files_total={}", report.files_total),
        format!("symbols_total={}", report.symbols_total),
        format!("relationships_total={}", report.relationships_total),
        format!("identifiers_total={}", report.identifiers_total),
        format!("types_total={}", report.types_total),
        format!("type_arguments_total={}", report.type_arguments_total),
    ];
    if let Some(root) = &report.root {
        fields.push(format!("root={}", root.display()));
    }
    if let Some(workspace_id) = &report.workspace_id {
        fields.push(format!("workspace_id={workspace_id}"));
    }
    if let Some(schema_version) = report.schema_version {
        fields.push(format!("schema_version={schema_version}"));
    }
    if let Some(schema_state) = report.schema_state {
        fields.push(format!(
            "schema_state={}",
            schema_state_as_str(schema_state)
        ));
    }
    if let Some(contract_version) = report.extract_contract_version {
        fields.push(format!("extract_contract_version={contract_version}"));
    }
    if let Some(revision) = report.revision {
        fields.push(format!("revision={revision}"));
    }
    if let Some(analyzed_revision) = report.analyzed_revision {
        fields.push(format!("analyzed_revision={analyzed_revision}"));
    }
    if let Some(analysis_state) = &report.analysis_state {
        fields.push(format!("analysis_state={analysis_state}"));
    }
    if let Some(julie_version) = &report.julie_version {
        fields.push(format!("julie_version={julie_version}"));
    }
    if !report.missing_metadata_keys.is_empty() {
        fields.push(format!(
            "missing_metadata_keys={}",
            report.missing_metadata_keys.join(",")
        ));
    }

    let mut output = format!(
        "extract {}: {} {}",
        report.operation,
        report.status.as_str(),
        fields.join(" ")
    );
    for error in &report.errors {
        output.push('\n');
        output.push_str("error ");
        output.push_str(&error.code);
        output.push_str(": ");
        output.push_str(&error.message);
        if let Some(path) = &error.path {
            output.push_str(" path=");
            output.push_str(&path.display().to_string());
        }
    }
    output
}

fn format_external_extract_markdown(report: &ExternalExtractReport) -> String {
    let mut output = String::from("# External Extract\n\n| Field | Value |\n|---|---|\n");
    push_markdown_row(&mut output, "Operation", &report.operation);
    push_markdown_row(&mut output, "Status", report.status.as_str());
    push_markdown_row(&mut output, "DB Path", &report.db.display().to_string());
    push_optional_markdown_row(
        &mut output,
        "Root",
        report.root.as_ref().map(|root| root.display().to_string()),
    );
    push_optional_markdown_row(&mut output, "Workspace ID", report.workspace_id.clone());
    push_optional_markdown_row(
        &mut output,
        "Schema Version",
        report.schema_version.map(|value| value.to_string()),
    );
    push_optional_markdown_row(
        &mut output,
        "Schema State",
        report
            .schema_state
            .map(|value| schema_state_as_str(value).to_string()),
    );
    push_optional_markdown_row(
        &mut output,
        "Extract Contract Version",
        report
            .extract_contract_version
            .map(|value| value.to_string()),
    );
    push_optional_markdown_row(
        &mut output,
        "Revision",
        report.revision.map(|value| value.to_string()),
    );
    push_optional_markdown_row(
        &mut output,
        "Analyzed Revision",
        report.analyzed_revision.map(|value| value.to_string()),
    );
    push_optional_markdown_row(&mut output, "Analysis State", report.analysis_state.clone());
    push_optional_markdown_row(&mut output, "Julie Version", report.julie_version.clone());
    if !report.missing_metadata_keys.is_empty() {
        push_markdown_row(
            &mut output,
            "Missing Metadata Keys",
            &report.missing_metadata_keys.join(", "),
        );
    }
    push_markdown_row(
        &mut output,
        "Files Scanned",
        &report.files_scanned.to_string(),
    );
    push_markdown_row(
        &mut output,
        "Files Updated",
        &report.files_updated.to_string(),
    );
    push_markdown_row(
        &mut output,
        "Files Deleted",
        &report.files_deleted.to_string(),
    );
    push_markdown_row(
        &mut output,
        "Symbols Extracted",
        &report.symbols_extracted.to_string(),
    );
    push_markdown_row(&mut output, "Files Total", &report.files_total.to_string());
    push_markdown_row(
        &mut output,
        "Symbols Total",
        &report.symbols_total.to_string(),
    );
    push_markdown_row(
        &mut output,
        "Relationships Total",
        &report.relationships_total.to_string(),
    );
    push_markdown_row(
        &mut output,
        "Identifiers Total",
        &report.identifiers_total.to_string(),
    );
    push_markdown_row(&mut output, "Types Total", &report.types_total.to_string());
    push_markdown_row(
        &mut output,
        "Type Arguments Total",
        &report.type_arguments_total.to_string(),
    );

    if !report.errors.is_empty() {
        output.push_str("\n## Errors\n\n| Code | Message | Path |\n|---|---|---|\n");
        for error in &report.errors {
            push_markdown_error_row(&mut output, error);
        }
    }

    output
}

fn push_optional_markdown_row(output: &mut String, label: &str, value: Option<String>) {
    if let Some(value) = value {
        push_markdown_row(output, label, &value);
    }
}

fn push_markdown_row(output: &mut String, label: &str, value: &str) {
    output.push_str("| ");
    output.push_str(&escape_markdown_cell(label));
    output.push_str(" | ");
    output.push_str(&escape_markdown_cell(value));
    output.push_str(" |\n");
}

fn push_markdown_error_row(output: &mut String, error: &ExternalExtractError) {
    output.push_str("| ");
    output.push_str(&escape_markdown_cell(&error.code));
    output.push_str(" | ");
    output.push_str(&escape_markdown_cell(&error.message));
    output.push_str(" | ");
    let path = error
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    output.push_str(&escape_markdown_cell(&path));
    output.push_str(" |\n");
}

fn escape_markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', "<br>")
}

fn schema_state_as_str(state: ExternalInfoSchemaState) -> &'static str {
    match state {
        ExternalInfoSchemaState::Missing => "missing",
        ExternalInfoSchemaState::Older => "older",
        ExternalInfoSchemaState::Current => "current",
        ExternalInfoSchemaState::Newer => "newer",
    }
}
