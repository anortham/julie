mod formatting;

use anyhow::{Result, anyhow};
use julie_context::{ToolContext, WorkspaceTarget};
use julie_core::database::StructuralFactQuery;
use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
        let metadata_filters = self.validate()?;
        let database = match workspace_target {
            WorkspaceTarget::Primary => handler.primary_pooled_database().await?,
            WorkspaceTarget::Target(workspace_id) => {
                handler
                    .get_pooled_database_for_workspace(workspace_id)
                    .await?
            }
        };
        let tool = self.clone();
        let rendered = tokio::task::spawn_blocking(move || -> Result<String> {
            let database = database.into_read_snapshot()?;
            tool.execute(&database, metadata_filters)
        })
        .await
        .map_err(|error| anyhow!("patterns query task failed: {error}"))??;
        Ok(CallToolResult::text_content(vec![Content::text(rendered)]))
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
        database: &julie_core::database::SymbolDatabase,
        metadata_filters: Vec<(String, String)>,
    ) -> Result<String> {
        let mut observed = database
            .observed_structural_patterns(self.language.as_deref(), self.path.as_deref())?;
        if let Some(pattern_id) = self.pattern_id.as_deref() {
            observed.retain(|(observed_id, _)| observed_id == pattern_id);
        }

        if self.operation == PatternsOperation::List {
            return formatting::format_list(observed, self.format);
        }

        let matched_pattern_ids = self.matched_pattern_ids(&observed);
        if self.operation == PatternsOperation::Search && matched_pattern_ids.is_empty() {
            return formatting::format_search(Vec::new(), &matched_pattern_ids, self.format);
        }
        let facts = database.search_structural_facts(&StructuralFactQuery {
            pattern_ids: matched_pattern_ids.clone(),
            path_pattern: self.path.clone(),
            language: self.language.clone(),
            metadata_equals: metadata_filters,
            limit: self.effective_limit(),
        })?;
        match self.operation {
            PatternsOperation::List => unreachable!(),
            PatternsOperation::Search => {
                formatting::format_search(facts, &matched_pattern_ids, self.format)
            }
            PatternsOperation::Summary => {
                formatting::format_summary(facts, self.group_by, self.facet.as_deref(), self.format)
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

fn validate_optional_text(name: &str, value: Option<&str>) -> Result<()> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(anyhow!("{name} must not be empty"));
    }
    Ok(())
}

fn default_limit() -> u32 {
    50
}
