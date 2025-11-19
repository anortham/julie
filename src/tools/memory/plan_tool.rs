// PlanTool - MCP interface for mutable development plans (Phase 1.5)

use anyhow::{Result, anyhow};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::handler::JulieServerHandler;

use super::plan::*;

/// Capture git context from the workspace (reuse from checkpoint)
async fn capture_git_context(handler: &JulieServerHandler) -> Option<super::GitContext> {
    use std::process::Stdio;
    use tokio::process::Command;

    let workspace = handler.get_workspace().await.ok()??;
    let workspace_root = workspace.root.clone();

    // Get current branch
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&workspace_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    let branch = String::from_utf8(branch_output.stdout)
        .ok()?
        .trim()
        .to_string();

    // Get current commit
    let commit_output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&workspace_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    let commit = String::from_utf8(commit_output.stdout)
        .ok()?
        .trim()
        .to_string();

    // Check if working directory is dirty
    let status_output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&workspace_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    let dirty = !status_output.stdout.is_empty();

    // Get changed files
    let files_output = Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(&workspace_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    let files_changed = if !files_output.stdout.is_empty() {
        Some(
            String::from_utf8(files_output.stdout)
                .ok()?
                .lines()
                .map(|s| s.to_string())
                .collect(),
        )
    } else {
        None
    };

    Some(super::GitContext {
        branch,
        commit,
        dirty,
        files_changed,
    })
}

#[mcp_tool(
    name = "plan",
    description = "Create and manage development plans stored in .memories/plans/.",
    title = "Manage Development Plans",
    idempotent_hint = false,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = false,
    meta = r#"{"category": "memory", "phase": "1.5"}"#
)]
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

        Ok(CallToolResult::text_content(vec![TextContent::from(
            format!(
                "✅ Plan created: {}\nID: {}\nStatus: {}\n\nPlan saved to: .memories/plans/{}.json",
                plan.title,
                plan.id,
                if should_activate {
                    "active"
                } else {
                    match plan.status {
                        PlanStatus::Active => "active",
                        PlanStatus::Completed => "completed",
                        PlanStatus::Archived => "archived",
                    }
                },
                plan.id
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

        // Format plan as readable text
        let content_preview = plan
            .content
            .as_ref()
            .map(|c| {
                let lines: Vec<&str> = c.lines().take(10).collect();
                let preview = lines.join("\n");
                if c.lines().count() > 10 {
                    format!("{}\n... ({} more lines)", preview, c.lines().count() - 10)
                } else {
                    preview
                }
            })
            .unwrap_or_else(|| "(no content)".to_string());

        Ok(CallToolResult::text_content(vec![TextContent::from(
            format!(
                "📋 Plan: {}\nID: {}\nStatus: {}\n\n{}",
                plan.title,
                plan.id,
                match plan.status {
                    PlanStatus::Active => "active",
                    PlanStatus::Completed => "completed",
                    PlanStatus::Archived => "archived",
                },
                content_preview
            ),
        )]))
    }

    async fn handle_list(&self, workspace_root: &std::path::Path) -> Result<CallToolResult> {
        info!("📋 Listing plans");

        // Parse status filter
        let status_filter = if let Some(ref status_str) = self.status {
            Some(match status_str.to_lowercase().as_str() {
                "active" => PlanStatus::Active,
                "completed" => PlanStatus::Completed,
                "archived" => PlanStatus::Archived,
                _ => return Err(anyhow!("Invalid status: {}", status_str)),
            })
        } else {
            None
        };

        let plans = list_plans(workspace_root, status_filter)?;

        if plans.is_empty() {
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                "No plans found.",
            )]));
        }

        // Format plans list
        let mut output = format!("Found {} plan(s):\n\n", plans.len());
        for plan in plans {
            let status_icon = match plan.status {
                PlanStatus::Active => "🟢",
                PlanStatus::Completed => "✅",
                PlanStatus::Archived => "📦",
            };
            output.push_str(&format!(
                "{} {} ({})\n   ID: {}\n",
                status_icon,
                plan.title,
                match plan.status {
                    PlanStatus::Active => "active",
                    PlanStatus::Completed => "completed",
                    PlanStatus::Archived => "archived",
                },
                plan.id
            ));
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(
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

        Ok(CallToolResult::text_content(vec![TextContent::from(
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

        info!("✏️ Updating plan: {}", id);

        // Parse status if provided
        let status = if let Some(ref status_str) = self.status {
            Some(match status_str.to_lowercase().as_str() {
                "active" => PlanStatus::Active,
                "completed" => PlanStatus::Completed,
                "archived" => PlanStatus::Archived,
                _ => return Err(anyhow!("Invalid status: {}", status_str)),
            })
        } else {
            None
        };

        let updates = PlanUpdates {
            title: None, // Don't allow title changes (changes filename)
            status,
            content: self.content.clone(),
            extra: None,
        };

        let plan = update_plan(workspace_root, id, updates)?;

        Ok(CallToolResult::text_content(vec![TextContent::from(
            format!(
                "✅ Updated plan: {}\nStatus: {}",
                plan.title,
                match plan.status {
                    PlanStatus::Active => "active",
                    PlanStatus::Completed => "completed",
                    PlanStatus::Archived => "archived",
                }
            ),
        )]))
    }

    async fn handle_complete(&self, workspace_root: &std::path::Path) -> Result<CallToolResult> {
        let id = self
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("ID is required for complete action"))?;

        info!("✅ Completing plan: {}", id);

        let plan = complete_plan(workspace_root, id)?;

        Ok(CallToolResult::text_content(vec![TextContent::from(
            format!("✅ Completed plan: {}\nStatus: completed", plan.title),
        )]))
    }
}
