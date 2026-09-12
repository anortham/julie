mod formatting;

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use julie_context::{ToolContext, WorkspaceTarget};
use julie_core::glob::matches_glob_pattern;
use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use julie_extractors::StructuralFact;
use julie_facts::FactsReader;
use julie_facts::rows::{StructuralFactQuery, StructuralFactRow};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const OBSERVED_ROW_CAP: usize = 10_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PatternsOperation {
    #[default]
    List,
    Summary,
    Search,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PatternsGroupBy {
    #[default]
    LanguagePatternCapture,
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PatternsFormat {
    #[default]
    Compact,
    Json,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PatternsTool {
    #[serde(default)]
    pub operation: PatternsOperation,
    #[serde(default)]
    pub pattern_id: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default, rename = "where")]
    pub where_filter: Option<String>,
    #[serde(default)]
    pub facet: Option<String>,
    #[serde(default)]
    pub group_by: PatternsGroupBy,
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Required workspace ID or absolute workspace path for MCP calls.
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub format: PatternsFormat,
}

impl Default for PatternsTool {
    fn default() -> Self {
        Self {
            operation: PatternsOperation::List,
            pattern_id: None,
            query: None,
            path: None,
            language: None,
            where_filter: None,
            facet: None,
            group_by: PatternsGroupBy::LanguagePatternCapture,
            limit: default_limit(),
            workspace: None,
            format: PatternsFormat::Compact,
        }
    }
}

impl PatternsTool {
    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        let workspace_target = handler
            .resolve_workspace_target(self.workspace.as_deref())
            .await?;
        self.call_tool_with_target(handler, &workspace_target).await
    }

    pub async fn call_tool_with_target(
        &self,
        handler: &dyn ToolContext,
        workspace_target: &WorkspaceTarget,
    ) -> Result<CallToolResult> {
        self.call_tool_counted(handler, workspace_target)
            .await
            .map(|(result, _)| result)
    }

    pub async fn call_tool_counted(
        &self,
        handler: &dyn ToolContext,
        workspace_target: &WorkspaceTarget,
    ) -> Result<(CallToolResult, u32)> {
        let metadata_filters = self.validate()?;
        let snapshot = handler.snapshot(workspace_target).await?;
        let tool = self.clone();
        let (rendered, count) = tokio::task::spawn_blocking(move || -> Result<(String, u32)> {
            let facts = snapshot.facts()?;
            tool.execute(&facts.reader(), metadata_filters)
        })
        .await
        .map_err(|error| anyhow!("patterns query task failed: {error}"))??;
        Ok((
            CallToolResult::text_content(vec![Content::text(rendered)]),
            count,
        ))
    }

    fn validate(&self) -> Result<Vec<(String, String)>> {
        validate_optional_text("pattern_id", self.pattern_id.as_deref())?;
        validate_optional_text("query", self.query.as_deref())?;
        validate_optional_text("path", self.path.as_deref())?;
        validate_optional_text("language", self.language.as_deref())?;
        validate_optional_text("facet", self.facet.as_deref())?;

        if self.operation == PatternsOperation::Search
            && self.pattern_id.is_none()
            && self.query.is_none()
        {
            return Err(anyhow!("patterns search requires pattern_id or query"));
        }
        if self.operation != PatternsOperation::Search && self.where_filter.is_some() {
            return Err(anyhow!(
                "where filters are only supported for patterns search"
            ));
        }
        if self.operation != PatternsOperation::Summary && self.facet.is_some() {
            return Err(anyhow!("facet is only supported for patterns summary"));
        }
        self.metadata_filters()
    }

    fn metadata_filters(&self) -> Result<Vec<(String, String)>> {
        self.where_filter
            .as_deref()
            .unwrap_or_default()
            .split(';')
            .filter(|part| !part.trim().is_empty())
            .map(|part| {
                let (key, value) = part
                    .split_once('=')
                    .ok_or_else(|| anyhow!("where filters must use key=value"))?;
                let key = key.trim();
                let value = value.trim();
                if key.is_empty() || value.is_empty() {
                    return Err(anyhow!("where filters must use non-empty key=value"));
                }
                Ok((key.to_string(), value.to_string()))
            })
            .collect()
    }

    fn effective_limit(&self) -> usize {
        self.limit.clamp(1, 500) as usize
    }

    fn execute(
        &self,
        reader: &FactsReader<'_>,
        metadata_filters: Vec<(String, String)>,
    ) -> Result<(String, u32)> {
        let mut observed =
            observed_structural_patterns(reader, self.language.as_deref(), self.path.as_deref())?;
        if let Some(pattern_id) = self.pattern_id.as_deref() {
            observed.retain(|(observed_id, _)| observed_id == pattern_id);
        }

        if self.operation == PatternsOperation::List {
            let count = observed.len() as u32;
            return Ok((formatting::format_list(observed, self.format)?, count));
        }

        let matched_pattern_ids = self.matched_pattern_ids(&observed);
        if self.operation == PatternsOperation::Search && matched_pattern_ids.is_empty() {
            return Ok((
                formatting::format_search(Vec::new(), &matched_pattern_ids, self.format)?,
                0,
            ));
        }
        let facts = search_structural_facts(
            reader,
            &StructuralFactQuery {
                pattern_ids: matched_pattern_ids.clone(),
                path_pattern: None,
                language: self.language.clone(),
                limit: self.effective_limit().saturating_mul(10).clamp(100, 5000),
            },
            self.path.as_deref(),
            &metadata_filters,
            self.effective_limit(),
        )?;
        match self.operation {
            PatternsOperation::List => unreachable!(),
            PatternsOperation::Search => {
                let count = facts.len() as u32;
                Ok((
                    formatting::format_search(facts, &matched_pattern_ids, self.format)?,
                    count,
                ))
            }
            PatternsOperation::Summary => {
                let count = facts.len() as u32;
                Ok((
                    formatting::format_summary(
                        facts,
                        self.group_by,
                        self.facet.as_deref(),
                        self.format,
                    )?,
                    count,
                ))
            }
        }
    }

    fn matched_pattern_ids(&self, observed: &[(String, u64)]) -> Vec<String> {
        if let Some(pattern_id) = self.pattern_id.as_ref() {
            return vec![pattern_id.clone()];
        }
        if let Some(query) = self.query.as_ref() {
            let query = query.to_ascii_lowercase();
            return observed
                .iter()
                .filter(|(pattern_id, _)| pattern_id.to_ascii_lowercase().contains(&query))
                .map(|(pattern_id, _)| pattern_id.clone())
                .collect();
        }
        observed
            .iter()
            .map(|(pattern_id, _)| pattern_id.clone())
            .collect()
    }
}

fn matches_path(row: &StructuralFactRow, path_pattern: Option<&str>) -> bool {
    path_pattern.is_none_or(|pattern| matches_glob_pattern(&row.path, pattern))
}

fn matches_metadata(row: &StructuralFactRow, filters: &[(String, String)]) -> bool {
    filters.iter().all(|(key, value)| {
        row.metadata
            .as_ref()
            .and_then(|metadata| metadata.get(key))
            .and_then(Value::as_str)
            == Some(value.as_str())
    })
}

fn observed_structural_patterns(
    reader: &FactsReader<'_>,
    language: Option<&str>,
    path_pattern: Option<&str>,
) -> Result<Vec<(String, u64)>> {
    let rows = reader.structural_facts(&StructuralFactQuery {
        language: language.map(str::to_string),
        limit: OBSERVED_ROW_CAP,
        ..StructuralFactQuery::default()
    })?;
    let mut counts = BTreeMap::<String, u64>::new();
    for row in rows.iter().filter(|row| matches_path(row, path_pattern)) {
        *counts.entry(row.pattern_id.clone()).or_default() += 1;
    }
    let mut observed = counts.into_iter().collect::<Vec<_>>();
    observed.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    Ok(observed)
}

// ponytail: path glob and metadata filters run after the capped SQL fetch;
// push them into the julie-facts query if a workspace overflows the cap.
fn search_structural_facts(
    reader: &FactsReader<'_>,
    query: &StructuralFactQuery,
    path_pattern: Option<&str>,
    metadata_filters: &[(String, String)],
    limit: usize,
) -> Result<Vec<StructuralFact>> {
    let mut rows = reader.structural_facts(query)?;
    rows.sort_by(|left, right| {
        (
            &left.pattern_id,
            &left.path,
            left.span.start_byte,
            left.ordinal,
        )
            .cmp(&(
                &right.pattern_id,
                &right.path,
                right.span.start_byte,
                right.ordinal,
            ))
    });
    Ok(rows
        .into_iter()
        .filter(|row| matches_path(row, path_pattern) && matches_metadata(row, metadata_filters))
        .take(limit)
        .map(structural_fact)
        .collect())
}

fn structural_fact(row: StructuralFactRow) -> StructuralFact {
    StructuralFact {
        id: format!("{}:{}", row.blob_hash, row.ordinal),
        containing_symbol_id: row
            .containing_ordinal
            .map(|ordinal| format!("{}:{ordinal}", row.blob_hash)),
        file_path: row.path,
        language: row.language,
        pattern_id: row.pattern_id,
        capture_name: row.capture_name,
        node_kind: row.node_kind,
        start_line: row.span.start_line,
        start_column: row.span.start_col,
        end_line: row.span.end_line,
        end_column: row.span.end_col,
        start_byte: row.span.start_byte,
        end_byte: row.span.end_byte,
        confidence: row.confidence,
        metadata: row.metadata,
    }
}

fn validate_optional_text(name: &str, value: Option<&str>) -> Result<()> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(anyhow!("{name} must not be empty"));
    }
    Ok(())
}

fn default_limit() -> u32 {
    50
}
