// PlanTool - MCP interface for mutable development plans (Phase 1.5)

use anyhow::{Result, anyhow};
use chrono::Utc;
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::handler::JulieServerHandler;

use super::git::capture_git_context;
use super::plan::*;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PlanTool {
    /// Action: "save", "get", "list", "activate", "update", "complete"
    pub action: PlanAction,
    /// Plan title (required for "save")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Plan ID (required for get, update, activate, complete)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Plan content in markdown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Plan status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Activate after saving (default: true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activate: Option<bool>,
}

/// Actions supported by the plan tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum PlanAction {
    #[serde(rename = "save")]
    Save,
    #[serde(rename = "get")]
    Get,
    #[serde(rename = "list")]
    List,
    #[serde(rename = "activate")]
    Activate,
    #[serde(rename = "update")]
    Update,
    #[serde(rename = "complete")]
    Complete,
}

impl PlanTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Get workspace root
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow!("No workspace available"))?;
        let workspace_root = workspace.root.clone();

        match self.action {
            PlanAction::Save => self.handle_save(&workspace_root, handler).await,
            PlanAction::Get => self.handle_get(&workspace_root).await,
            PlanAction::List => self.handle_list(&workspace_root).await,
            PlanAction::Activate => self.handle_activate(&workspace_root).await,
            PlanAction::Update => self.handle_update(&workspace_root).await,
            PlanAction::Complete => self.handle_complete(&workspace_root).await,
        }
    }

    async fn handle_save(
        &self,
        workspace_root: &std::path::Path,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        let title = self
            .title
            .as_ref()
            .ok_or_else(|| anyhow!("Title is required for save action"))?
            .clone();

        info!("📋 Creating plan: {}", title);

        // Capture git context
        let git_context = capture_git_context(handler).await;

        // Create plan
        let plan = create_plan(workspace_root, title, self.content.clone(), git_context)?;

        // Activate if requested (default: true)
        let should_activate = self.activate.unwrap_or(true);
        if should_activate {
            activate_plan(workspace_root, &plan.id)?;
        }

        let status_str = if should_activate { "active" } else { &plan.status.to_string() };

        Ok(CallToolResult::text_content(vec![Content::text(
            format!(
                "✅ Plan created: {}\nID: {}\nStatus: {}\n\nPlan saved to: .memories/plans/{}.json",
                plan.title, plan.id, status_str, plan.id
            ),
        )]))
    }

    async fn handle_get(&self, workspace_root: &std::path::Path) -> Result<CallToolResult> {
        let id = self
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("ID is required for get action"))?;

        info!("🔍 Getting plan: {}", id);

        let plan = get_plan(workspace_root, id)?;

        let content = plan.content.as_deref().unwrap_or("(no content)");

        Ok(CallToolResult::text_content(vec![Content::text(
            format!(
                "📋 Plan: {}\nID: {}\nStatus: {}\n\n{}",
                plan.title, plan.id, plan.status, content
            ),
        )]))
    }

    async fn handle_list(&self, workspace_root: &std::path::Path) -> Result<CallToolResult> {
        info!("📋 Listing plans");

        // Parse status filter
        let status_filter = if let Some(ref status_str) = self.status {
            Some(parse_status(status_str)?)
        } else {
            None
        };

        let plans = list_plans(workspace_root, status_filter)?;

        if plans.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "No plans found.",
            )]));
        }

        let now = Utc::now().timestamp();

        // Format plans list
        let mut output = format!("Found {} plan(s):\n\n", plans.len());
        for plan in plans {
            let status_icon = match plan.status {
                PlanStatus::Active => "🟢",
                PlanStatus::Completed => "✅",
                PlanStatus::Archived => "📦",
            };
            let age = format_relative_time(now, plan.timestamp);
            output.push_str(&format!(
                "{} {} ({}, {})\n   ID: {}\n",
                status_icon, plan.title, plan.status, age, plan.id
            ));
        }

        Ok(CallToolResult::text_content(vec![Content::text(
            output,
        )]))
    }

    async fn handle_activate(&self, workspace_root: &std::path::Path) -> Result<CallToolResult> {
        let id = self
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("ID is required for activate action"))?;

        info!("🎯 Activating plan: {}", id);

        activate_plan(workspace_root, id)?;

        let plan = get_plan(workspace_root, id)?;

        Ok(CallToolResult::text_content(vec![Content::text(
            format!(
                "✅ Activated plan: {}\nAll other plans have been archived.",
                plan.title
            ),
        )]))
    }

    async fn handle_update(&self, workspace_root: &std::path::Path) -> Result<CallToolResult> {
        let id = self
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("ID is required for update action"))?;

        // Reject title changes — ID is derived from title slug, so changing
        // the title would break the ID→filename mapping
        if self.title.is_some() {
            return Err(anyhow!(
                "Title changes are not supported. The plan ID is derived from the title. \
                 Create a new plan instead."
            ));
        }

        info!("✏️ Updating plan: {}", id);

        // Parse status if provided
        let status = if let Some(ref status_str) = self.status {
            Some(parse_status(status_str)?)
        } else {
            None
        };

        let updates = PlanUpdates {
            title: None,
            status,
            content: self.content.clone(),
            extra: None,
        };

        let plan = update_plan(workspace_root, id, updates)?;

        Ok(CallToolResult::text_content(vec![Content::text(
            format!("✅ Updated plan: {}\nStatus: {}", plan.title, plan.status),
        )]))
    }

    async fn handle_complete(&self, workspace_root: &std::path::Path) -> Result<CallToolResult> {
        let id = self
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("ID is required for complete action"))?;

        info!("✅ Completing plan: {}", id);

        let plan = complete_plan(workspace_root, id)?;

        Ok(CallToolResult::text_content(vec![Content::text(
            format!("✅ Completed plan: {}\nStatus: {}", plan.title, plan.status),
        )]))
    }
}

/// Parse a status string into PlanStatus
fn parse_status(s: &str) -> Result<PlanStatus> {
    match s.to_lowercase().as_str() {
        "active" => Ok(PlanStatus::Active),
        "completed" => Ok(PlanStatus::Completed),
        "archived" => Ok(PlanStatus::Archived),
        _ => Err(anyhow!("Invalid status: {}", s)),
    }
}

/// Format a Unix timestamp as a human-readable relative time (e.g., "2d ago", "3mo ago")
fn format_relative_time(now: i64, timestamp: i64) -> String {
    let delta = now - timestamp;
    if delta < 0 {
        return "just now".to_string();
    }
    let seconds = delta as u64;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    let months = days / 30;

    if seconds < 60 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{}m ago", minutes)
    } else if hours < 24 {
        format!("{}h ago", hours)
    } else if days < 30 {
        format!("{}d ago", days)
    } else {
        format!("{}mo ago", months)
    }
}
