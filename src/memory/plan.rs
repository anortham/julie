//! Plan CRUD — save, get, list, activate, update, complete.
//!
//! Plans are stored as markdown files with YAML frontmatter at
//! `.memories/plans/{plan-id}.md`. The active plan ID is tracked in
//! `.memories/.active-plan` (plain text).
//!
//! ## File Format
//!
//! ```text
//! ---
//! id: my-plan-id
//! title: My Plan Title
//! status: active
//! created: 2026-03-08T14:15:23.000Z
//! updated: 2026-03-08T14:15:23.000Z
//! tags:
//!   - tag1
//! ---
//!
//! Plan content here as markdown body...
//! ```

use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::Utc;

use super::storage::{get_string, get_string_array, split_frontmatter};
use super::{Plan, PlanInput, PlanUpdate};

// ============================================================================
// Public API
// ============================================================================

/// Slugify a title into a plan ID.
///
/// "My Feature Plan" -> "my-feature-plan"
///
/// Rules:
/// - Lowercase
/// - Replace non-alphanumeric (except hyphens) with hyphens
/// - Collapse consecutive hyphens
/// - Trim leading/trailing hyphens
pub fn slugify(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse consecutive hyphens and trim
    let mut result = String::with_capacity(slug.len());
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push('-');
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }

    // Trim leading/trailing hyphens
    result.trim_matches('-').to_string()
}

/// Save a new plan to `.memories/plans/{id}.md`.
///
/// - If `input.id` is None, generates one from the title via `slugify()`
/// - Creates the plans directory if it doesn't exist
/// - If `input.activate` is true, also writes `.memories/.active-plan`
/// - Default status is "active"
/// Validate a plan ID to prevent path traversal and invalid filenames.
fn validate_plan_id(id: &str) -> Result<()> {
    if id.is_empty() {
        bail!("Plan ID cannot be empty");
    }
    if id.contains("..") || id.contains('/') || id.contains('\\') || id.contains('\0') {
        bail!(
            "Invalid plan ID '{}': must not contain path separators or traversal sequences",
            id
        );
    }
    // Reject IDs that would be problematic filenames on Windows
    if id.contains(':') || id.contains('*') || id.contains('?') || id.contains('"')
        || id.contains('<') || id.contains('>') || id.contains('|')
    {
        bail!("Invalid plan ID '{}': contains characters not allowed in filenames", id);
    }
    Ok(())
}

pub fn save_plan(workspace_root: &Path, input: PlanInput) -> Result<Plan> {
    let id = input.id.unwrap_or_else(|| slugify(&input.title));
    validate_plan_id(&id)?;
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let plan = Plan {
        id: id.clone(),
        title: input.title,
        content: input.content,
        status: "active".to_string(),
        created: now.clone(),
        updated: now,
        tags: input.tags.unwrap_or_default(),
    };

    // Ensure plans directory exists
    let plans_dir = workspace_root.join(".memories").join("plans");
    std::fs::create_dir_all(&plans_dir)
        .with_context(|| format!("Failed to create plans directory: {}", plans_dir.display()))?;

    // Write the plan file
    let file_path = plans_dir.join(format!("{}.md", &plan.id));
    let content = format_plan(&plan);
    std::fs::write(&file_path, &content)
        .with_context(|| format!("Failed to write plan file: {}", file_path.display()))?;

    // Optionally activate
    if input.activate == Some(true) {
        write_active_plan(workspace_root, &plan.id)?;
    }

    Ok(plan)
}

/// Get a single plan by ID.
///
/// Returns `Ok(None)` if the plan file doesn't exist.
pub fn get_plan(workspace_root: &Path, id: &str) -> Result<Option<Plan>> {
    validate_plan_id(id)?;
    let file_path = workspace_root
        .join(".memories")
        .join("plans")
        .join(format!("{}.md", id));

    if !file_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&file_path)
        .with_context(|| format!("Failed to read plan file: {}", file_path.display()))?;

    let plan = parse_plan(&content)?;
    Ok(Some(plan))
}

/// List all plans, optionally filtered by status.
///
/// Returns an empty vec if the plans directory doesn't exist.
pub fn list_plans(workspace_root: &Path, status_filter: Option<&str>) -> Result<Vec<Plan>> {
    let plans_dir = workspace_root.join(".memories").join("plans");

    if !plans_dir.exists() {
        return Ok(Vec::new());
    }

    let mut plans = Vec::new();

    for entry in std::fs::read_dir(&plans_dir)
        .with_context(|| format!("Failed to read plans directory: {}", plans_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        // Only process .md files (skip .active-plan and other non-md files)
        match path.extension().and_then(|e| e.to_str()) {
            Some("md") => {}
            _ => continue,
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read plan file: {}", path.display()))?;

        match parse_plan(&content) {
            Ok(plan) => {
                if let Some(filter) = status_filter {
                    if plan.status != filter {
                        continue;
                    }
                }
                plans.push(plan);
            }
            Err(e) => {
                // Log and skip malformed plan files rather than failing entirely
                tracing::warn!("Skipping malformed plan file {}: {}", path.display(), e);
            }
        }
    }

    // Sort by created timestamp for consistent ordering
    plans.sort_by(|a, b| a.created.cmp(&b.created));

    Ok(plans)
}

/// Activate a plan by writing its ID to `.memories/.active-plan`.
///
/// The plan must exist; returns an error if it doesn't.
pub fn activate_plan(workspace_root: &Path, id: &str) -> Result<()> {
    validate_plan_id(id)?;
    // Verify the plan exists
    let plan_file = workspace_root
        .join(".memories")
        .join("plans")
        .join(format!("{}.md", id));

    if !plan_file.exists() {
        bail!("Plan '{}' not found", id);
    }

    write_active_plan(workspace_root, id)
}

/// Update an existing plan's fields.
///
/// Only `Some` fields in the update are applied. The `updated` timestamp
/// is always refreshed. Returns the updated plan.
pub fn update_plan(workspace_root: &Path, id: &str, updates: PlanUpdate) -> Result<Plan> {
    validate_plan_id(id)?;
    let mut plan = get_plan(workspace_root, id)?
        .with_context(|| format!("Plan '{}' not found", id))?;

    // Apply updates
    if let Some(title) = updates.title {
        plan.title = title;
    }
    if let Some(content) = updates.content {
        plan.content = content;
    }
    if let Some(status) = updates.status {
        plan.status = status;
    }
    if let Some(tags) = updates.tags {
        plan.tags = tags;
    }

    // Refresh updated timestamp
    plan.updated = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // Write back to disk
    let file_path = workspace_root
        .join(".memories")
        .join("plans")
        .join(format!("{}.md", id));
    let content = format_plan(&plan);
    std::fs::write(&file_path, &content)
        .with_context(|| format!("Failed to write plan file: {}", file_path.display()))?;

    Ok(plan)
}

/// Complete a plan (set status to "completed").
///
/// Shorthand for `update_plan()` with status = "completed".
pub fn complete_plan(workspace_root: &Path, id: &str) -> Result<Plan> {
    let plan = update_plan(
        workspace_root,
        id,
        PlanUpdate {
            status: Some("completed".to_string()),
            ..Default::default()
        },
    )?;

    // If this was the active plan, clear .active-plan so recall doesn't show
    // a completed plan as active
    let active_path = workspace_root.join(".memories").join(".active-plan");
    if let Ok(active_id) = std::fs::read_to_string(&active_path) {
        if active_id.trim() == id {
            let _ = std::fs::remove_file(&active_path);
        }
    }

    Ok(plan)
}

/// Get the currently active plan.
///
/// Reads `.memories/.active-plan` for the plan ID, then loads the plan.
/// Returns `Ok(None)` if:
/// - No `.active-plan` file exists
/// - The file is empty
/// - The referenced plan file doesn't exist
pub fn get_active_plan(workspace_root: &Path) -> Result<Option<Plan>> {
    let active_path = workspace_root.join(".memories").join(".active-plan");

    let id = match std::fs::read_to_string(&active_path) {
        Ok(content) => {
            let trimmed = content.trim().to_string();
            if trimmed.is_empty() {
                return Ok(None);
            }
            trimmed
        }
        Err(_) => return Ok(None),
    };

    get_plan(workspace_root, &id)
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Write a plan ID to `.memories/.active-plan`.
fn write_active_plan(workspace_root: &Path, id: &str) -> Result<()> {
    let memories_dir = workspace_root.join(".memories");
    std::fs::create_dir_all(&memories_dir)
        .with_context(|| format!("Failed to create memories directory: {}", memories_dir.display()))?;

    let active_path = memories_dir.join(".active-plan");
    std::fs::write(&active_path, id)
        .with_context(|| format!("Failed to write .active-plan: {}", active_path.display()))?;

    Ok(())
}

/// Format a plan as YAML frontmatter + markdown body.
fn format_plan(plan: &Plan) -> String {
    let mut frontmatter = serde_yaml::Mapping::new();

    frontmatter.insert(
        serde_yaml::Value::String("id".to_string()),
        serde_yaml::Value::String(plan.id.clone()),
    );
    frontmatter.insert(
        serde_yaml::Value::String("title".to_string()),
        serde_yaml::Value::String(plan.title.clone()),
    );
    frontmatter.insert(
        serde_yaml::Value::String("status".to_string()),
        serde_yaml::Value::String(plan.status.clone()),
    );
    frontmatter.insert(
        serde_yaml::Value::String("created".to_string()),
        serde_yaml::Value::String(plan.created.clone()),
    );
    frontmatter.insert(
        serde_yaml::Value::String("updated".to_string()),
        serde_yaml::Value::String(plan.updated.clone()),
    );

    if !plan.tags.is_empty() {
        let tag_values: Vec<serde_yaml::Value> = plan
            .tags
            .iter()
            .map(|t| serde_yaml::Value::String(t.clone()))
            .collect();
        frontmatter.insert(
            serde_yaml::Value::String("tags".to_string()),
            serde_yaml::Value::Sequence(tag_values),
        );
    }

    let yaml = serde_yaml::to_string(&frontmatter).unwrap_or_default();
    let yaml = yaml.trim();

    format!("---\n{}\n---\n\n{}\n", yaml, plan.content)
}

/// Parse a plan from a YAML frontmatter markdown file.
fn parse_plan(content: &str) -> Result<Plan> {
    // Strip BOM and normalize line endings
    let normalized = content
        .trim_start_matches('\u{FEFF}')
        .replace("\r\n", "\n");

    let (yaml_content, body) = split_frontmatter(&normalized)
        .context("Invalid plan file: no YAML frontmatter found")?;

    let frontmatter: serde_yaml::Mapping = serde_yaml::from_str(yaml_content)
        .context("Invalid plan file: YAML parsing failed")?;

    let id = get_string(&frontmatter, "id")
        .context("Missing required field: id")?;
    let title = get_string(&frontmatter, "title")
        .context("Missing required field: title")?;
    let status = get_string(&frontmatter, "status")
        .unwrap_or_else(|| "active".to_string());
    let created = get_string(&frontmatter, "created")
        .unwrap_or_default();
    let updated = get_string(&frontmatter, "updated")
        .unwrap_or_default();

    let tags = get_string_array(&frontmatter, "tags").unwrap_or_default();

    Ok(Plan {
        id,
        title,
        content: body.to_string(),
        status,
        created,
        updated,
        tags,
    })
}

