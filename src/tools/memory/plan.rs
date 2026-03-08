//! MCP tool wrapper for plan management operations.
//!
//! Thin layer that dispatches plan actions to `memory::plan::*` functions.

use anyhow::{bail, Result};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::memory::{PlanInput, PlanUpdate};

#[derive(Debug, Deserialize, JsonSchema)]
/// Manage persistent development plans. Plans track multi-session work
/// and associate checkpoints with goals.
pub struct PlanTool {
    /// Action: "save", "get", "list", "activate", "update", "complete"
    pub action: String,

    /// Plan ID (required for get/activate/update/complete; auto-generated for save)
    #[serde(default)]
    pub id: Option<String>,

    /// Plan title (required for save)
    #[serde(default)]
    pub title: Option<String>,

    /// Plan content (markdown)
    #[serde(default)]
    pub content: Option<String>,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Option<Vec<String>>,

    /// Whether to activate plan after saving (default: false)
    #[serde(default)]
    pub activate: Option<bool>,

    /// Plan status for update action
    #[serde(default)]
    pub status: Option<String>,
}

impl PlanTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("Plan action: {}", self.action);

        let root = &handler.workspace_root;

        match self.action.as_str() {
            "save" => self.handle_save(root),
            "get" => self.handle_get(root),
            "list" => self.handle_list(root),
            "activate" => self.handle_activate(root),
            "update" => self.handle_update(root),
            "complete" => self.handle_complete(root),
            other => bail!("Unknown plan action: '{}'. Expected: save, get, list, activate, update, complete", other),
        }
    }

    fn handle_save(&self, root: &std::path::Path) -> Result<CallToolResult> {
        let title = self
            .title
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("'title' is required for plan save"))?;
        let content = self
            .content
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("'content' is required for plan save"))?;

        let input = PlanInput {
            id: self.id.clone(),
            title: title.clone(),
            content: content.clone(),
            tags: self.tags.clone(),
            activate: self.activate,
        };

        let plan = crate::memory::plan::save_plan(root, input)?;

        let activated = if self.activate == Some(true) {
            " (activated)"
        } else {
            ""
        };

        let output = format!(
            "Plan saved{}\n\
             **ID:** {}\n\
             **Title:** {}\n\
             **Status:** {}",
            activated, plan.id, plan.title, plan.status
        );

        Ok(CallToolResult::text_content(vec![Content::text(output)]))
    }

    fn handle_get(&self, root: &std::path::Path) -> Result<CallToolResult> {
        let id = self
            .id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("'id' is required for plan get"))?;

        let plan = crate::memory::plan::get_plan(root, id)?;

        let output = match plan {
            Some(plan) => format_plan_detail(&plan),
            None => format!("Plan '{}' not found.", id),
        };

        Ok(CallToolResult::text_content(vec![Content::text(output)]))
    }

    fn handle_list(&self, root: &std::path::Path) -> Result<CallToolResult> {
        let status_filter = self.status.as_deref();
        let plans = crate::memory::plan::list_plans(root, status_filter)?;

        let output = if plans.is_empty() {
            let filter_msg = status_filter
                .map(|s| format!(" with status '{}'", s))
                .unwrap_or_default();
            format!("No plans found{}.", filter_msg)
        } else {
            let mut out = format!("## Plans ({} found)\n", plans.len());
            for plan in &plans {
                out.push_str(&format!(
                    "\n- **{}** ({})\n  Status: {} | Tags: {}\n  Created: {}\n",
                    plan.title,
                    plan.id,
                    plan.status,
                    if plan.tags.is_empty() {
                        "none".to_string()
                    } else {
                        plan.tags.join(", ")
                    },
                    plan.created,
                ));
            }
            out
        };

        Ok(CallToolResult::text_content(vec![Content::text(output)]))
    }

    fn handle_activate(&self, root: &std::path::Path) -> Result<CallToolResult> {
        let id = self
            .id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("'id' is required for plan activate"))?;

        crate::memory::plan::activate_plan(root, id)?;

        let output = format!("Plan '{}' activated.", id);
        Ok(CallToolResult::text_content(vec![Content::text(output)]))
    }

    fn handle_update(&self, root: &std::path::Path) -> Result<CallToolResult> {
        let id = self
            .id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("'id' is required for plan update"))?;

        let updates = PlanUpdate {
            title: self.title.clone(),
            content: self.content.clone(),
            status: self.status.clone(),
            tags: self.tags.clone(),
        };

        let plan = crate::memory::plan::update_plan(root, id, updates)?;

        let output = format!(
            "Plan updated\n\
             **ID:** {}\n\
             **Title:** {}\n\
             **Status:** {}",
            plan.id, plan.title, plan.status
        );

        Ok(CallToolResult::text_content(vec![Content::text(output)]))
    }

    fn handle_complete(&self, root: &std::path::Path) -> Result<CallToolResult> {
        let id = self
            .id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("'id' is required for plan complete"))?;

        let plan = crate::memory::plan::complete_plan(root, id)?;

        let output = format!(
            "Plan completed\n\
             **ID:** {}\n\
             **Title:** {}\n\
             **Status:** {}",
            plan.id, plan.title, plan.status
        );

        Ok(CallToolResult::text_content(vec![Content::text(output)]))
    }
}

/// Format a plan as detailed markdown output.
fn format_plan_detail(plan: &crate::memory::Plan) -> String {
    let mut out = format!("## {}\n", plan.title);
    out.push_str(&format!(
        "**ID:** {} | **Status:** {}\n",
        plan.id, plan.status
    ));
    if !plan.tags.is_empty() {
        out.push_str(&format!("**Tags:** {}\n", plan.tags.join(", ")));
    }
    out.push_str(&format!(
        "**Created:** {} | **Updated:** {}\n",
        plan.created, plan.updated
    ));
    out.push('\n');
    out.push_str(&plan.content);
    out.push('\n');
    out
}
