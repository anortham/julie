// Plan System - Mutable development plans (Phase 1.5)
//
// Unlike immutable checkpoints, plans can be updated in-place.
// Storage: `.memories/plans/plan_{slug}.json`
//
// Key behaviors:
// - Stable filenames (plan_add-search.json doesn't change on update)
// - Atomic updates (temp file + rename pattern)
// - Only one plan can be "active" at a time
// - Plans are indexed and searchable like checkpoints

use anyhow::{Result, anyhow};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use super::GitContext;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Memory type identifier for plans
const PLAN_TYPE: &str = "plan";

// ============================================================================
// TYPES & STRUCTURES
// ============================================================================

/// Plan status - only one plan can be active at a time
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanStatus {
    Active,
    Completed,
    Archived,
}

/// Plan-specific data structure
///
/// Plans extend the Memory structure with mutable semantics.
/// They have stable IDs and can be updated in-place.
///
/// # Storage
/// - Location: `.memories/plans/plan_{slug}.json`
/// - Filename: Derived from title (e.g., "Add Search" → "plan_add-search.json")
/// - Updates: Atomic (temp file + rename)
///
/// # Example
/// ```json
/// {
///   "id": "plan_add-search",
///   "timestamp": 1736422822,
///   "type": "plan",
///   "title": "Add Search Feature",
///   "status": "active",
///   "content": "## Tasks\n- [ ] Design API\n- [ ] Implement",
///   "git": { ... }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Unique identifier (format: "plan_{slug}")
    pub id: String,

    /// Last updated timestamp (Unix epoch)
    pub timestamp: i64,

    /// Memory type (always "plan")
    #[serde(rename = "type")]
    pub memory_type: String,

    /// Plan title (human-readable)
    pub title: String,

    /// Current status
    pub status: PlanStatus,

    /// Plan content (markdown)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// Git context at creation/update time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitContext>,

    /// Additional fields (extensibility)
    #[serde(flatten)]
    pub extra: Value,
}

/// Updates to apply to an existing plan
///
/// All fields are optional - only provided fields are updated.
/// Timestamp is always updated automatically.
#[derive(Debug, Clone, Default)]
pub struct PlanUpdates {
    /// New title (changes filename if provided)
    pub title: Option<String>,

    /// New status
    pub status: Option<PlanStatus>,

    /// New content
    pub content: Option<String>,

    /// Extra fields to merge
    pub extra: Option<Value>,
}

// ============================================================================
// CORE FUNCTIONS
// ============================================================================

/// Create a new plan and save it to disk
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `title` - Plan title (used to generate slug)
/// * `content` - Optional markdown content
/// * `git_context` - Optional git context
///
/// # Returns
/// The created Plan with generated ID
///
/// # Storage
/// Creates file at: `.memories/plans/plan_{slug}.json`
/// where {slug} is derived from title (e.g., "Add Search" → "add-search")
///
/// # Errors
/// - Invalid title (empty, too long, invalid characters)
/// - I/O errors creating directory or file
/// - Serialization errors
///
/// # Example
/// ```no_run
/// # use std::path::PathBuf;
/// let plan = create_plan(
///     &PathBuf::from("."),
///     "Add Search Feature".into(),
///     Some("## Tasks\n- [ ] Design API".into()),
///     None
/// )?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn create_plan(
    workspace_root: &Path,
    title: String,
    content: Option<String>,
    git_context: Option<GitContext>,
) -> Result<Plan> {
    // Generate slug from title
    let slug = title_to_slug(&title)?;
    let id = format!("plan_{}", slug);

    // Ensure plans directory exists
    ensure_plans_directory(workspace_root)?;

    // Create plan structure
    let plan = Plan {
        id: id.clone(),
        timestamp: Utc::now().timestamp(),
        memory_type: PLAN_TYPE.to_string(),
        title,
        status: PlanStatus::Active, // New plans default to Active
        content,
        git: git_context,
        extra: serde_json::Value::Object(serde_json::Map::new()),
    };

    // Save to disk atomically
    save_plan_atomic(workspace_root, &plan)?;

    Ok(plan)
}

/// Update an existing plan atomically
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `id` - Plan ID (e.g., "plan_add-search")
/// * `updates` - Updates to apply
///
/// # Returns
/// The updated Plan
///
/// # Behavior
/// - Reads existing plan from disk
/// - Applies updates (only non-None fields)
/// - Updates timestamp to current time
/// - Writes atomically (temp file + rename)
///
/// # Errors
/// - Plan not found
/// - I/O errors reading/writing file
/// - Serialization/deserialization errors
///
/// # Example
/// ```no_run
/// # use std::path::PathBuf;
/// # use crate::tools::memory::plan::*;
/// let updates = PlanUpdates {
///     status: Some(PlanStatus::Completed),
///     ..Default::default()
/// };
/// let plan = update_plan(&PathBuf::from("."), "plan_add-search", updates)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn update_plan(workspace_root: &Path, id: &str, updates: PlanUpdates) -> Result<Plan> {
    // Read existing plan
    let mut plan = get_plan(workspace_root, id)?;

    // Apply updates
    if let Some(title) = updates.title {
        plan.title = title;
    }
    if let Some(status) = updates.status {
        plan.status = status;
    }
    if let Some(content) = updates.content {
        plan.content = Some(content);
    }
    if let Some(extra) = updates.extra {
        plan.extra = extra;
    }

    // Update timestamp
    plan.timestamp = Utc::now().timestamp();

    // Save to disk atomically
    save_plan_atomic(workspace_root, &plan)?;

    Ok(plan)
}

/// Get a specific plan by ID
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `id` - Plan ID (e.g., "plan_add-search")
///
/// # Returns
/// The requested Plan
///
/// # Errors
/// - Plan not found
/// - I/O errors reading file
/// - Deserialization errors
pub fn get_plan(workspace_root: &Path, id: &str) -> Result<Plan> {
    let plan_path = get_plan_path(workspace_root, id);

    if !plan_path.exists() {
        return Err(anyhow!("Plan not found: {}", id));
    }

    let content = fs::read_to_string(&plan_path)?;
    let plan: Plan = serde_json::from_str(&content)?;

    Ok(plan)
}

/// List all plans, optionally filtered by status
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `status_filter` - Optional status to filter by
///
/// # Returns
/// Vector of Plans, sorted by timestamp (most recent first)
///
/// # Example
/// ```no_run
/// # use std::path::PathBuf;
/// # use crate::tools::memory::plan::*;
/// // Get all plans
/// let all = list_plans(&PathBuf::from("."), None)?;
///
/// // Get only active plans
/// let active = list_plans(&PathBuf::from("."), Some(PlanStatus::Active))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn list_plans(workspace_root: &Path, status_filter: Option<PlanStatus>) -> Result<Vec<Plan>> {
    let plans_dir = workspace_root.join(".memories").join("plans");

    // Return empty list if directory doesn't exist
    if !plans_dir.exists() {
        return Ok(Vec::new());
    }

    let mut plans = Vec::new();

    // Read all .json files in plans directory
    for entry in fs::read_dir(&plans_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let content = fs::read_to_string(&path)?;
            if let Ok(plan) = serde_json::from_str::<Plan>(&content) {
                // Apply status filter if provided
                if let Some(ref filter_status) = status_filter {
                    if &plan.status == filter_status {
                        plans.push(plan);
                    }
                } else {
                    plans.push(plan);
                }
            }
        }
    }

    // Sort by timestamp (most recent first)
    plans.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(plans)
}

/// Activate a plan (deactivates all others)
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `id` - Plan ID to activate
///
/// # Behavior
/// - Sets specified plan status to Active
/// - Sets all other plans to Archived
/// - Updates timestamps for all modified plans
/// - All operations are atomic
///
/// # Errors
/// - Plan not found
/// - I/O errors
///
/// # Example
/// ```no_run
/// # use std::path::PathBuf;
/// activate_plan(&PathBuf::from("."), "plan_add-search")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn activate_plan(workspace_root: &Path, id: &str) -> Result<()> {
    // Get all plans
    let all_plans = list_plans(workspace_root, None)?;

    // Verify the target plan exists
    if !all_plans.iter().any(|p| p.id == id) {
        return Err(anyhow!("Plan not found: {}", id));
    }

    // Update each plan
    for plan in all_plans {
        let new_status = if plan.id == id {
            PlanStatus::Active
        } else {
            PlanStatus::Archived
        };

        // Only update if status changed
        if plan.status != new_status {
            let updates = PlanUpdates {
                status: Some(new_status),
                ..Default::default()
            };
            update_plan(workspace_root, &plan.id, updates)?;
        }
    }

    Ok(())
}

/// Complete a plan (sets status to Completed)
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `id` - Plan ID to complete
///
/// # Returns
/// The updated Plan
///
/// # Errors
/// - Plan not found
/// - I/O errors
pub fn complete_plan(workspace_root: &Path, id: &str) -> Result<Plan> {
    let updates = PlanUpdates {
        status: Some(PlanStatus::Completed),
        ..Default::default()
    };

    update_plan(workspace_root, id, updates)
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Save a plan to disk atomically using temp file + rename pattern
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `plan` - Plan to save
///
/// # Behavior
/// - Writes plan to temporary file
/// - Renames temp file to final location (atomic operation)
/// - Ensures no partial writes if process crashes
///
/// # Errors
/// - Serialization errors
/// - I/O errors writing or renaming file
fn save_plan_atomic(workspace_root: &Path, plan: &Plan) -> Result<()> {
    let plan_path = get_plan_path(workspace_root, &plan.id);
    let temp_path = plan_path.with_extension("tmp");

    // Write to temp file
    let json = serde_json::to_string_pretty(plan)?;
    fs::write(&temp_path, json)?;

    // Atomic rename
    fs::rename(&temp_path, &plan_path)?;

    Ok(())
}

/// Generate a URL-friendly slug from a title
///
/// # Behavior
/// - Converts to lowercase
/// - Replaces spaces and special chars with hyphens
/// - Removes consecutive hyphens
/// - Trims hyphens from ends
///
/// # Examples
/// - "Add Search Feature" → "add-search-feature"
/// - "Fix: Auth Bug" → "fix-auth-bug"
/// - "Database Migration (v2)" → "database-migration-v2"
///
/// # Errors
/// - Empty title after slugification
/// - Slug too long (>100 chars)
fn title_to_slug(title: &str) -> Result<String> {
    // Trim and convert to lowercase
    let slug = title
        .trim()
        .to_lowercase()
        // Replace whitespace and special chars with hyphens
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '-'
            } else {
                // Skip other special characters
                '\0'
            }
        })
        .filter(|&c| c != '\0')
        .collect::<String>();

    // Remove consecutive hyphens
    let slug = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    // Validate
    if slug.is_empty() {
        return Err(anyhow!(
            "Title cannot be empty or contain only special characters"
        ));
    }

    if slug.len() > 100 {
        return Err(anyhow!("Slug too long (max 100 characters): {}", slug));
    }

    Ok(slug)
}

/// Get the file path for a plan ID
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `id` - Plan ID (e.g., "plan_add-search")
///
/// # Returns
/// PathBuf to `.memories/plans/{id}.json`
fn get_plan_path(workspace_root: &Path, id: &str) -> PathBuf {
    workspace_root
        .join(".memories")
        .join("plans")
        .join(format!("{}.json", id))
}

/// Ensure the plans directory exists
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
///
/// # Returns
/// PathBuf to `.memories/plans/` directory
///
/// # Errors
/// - I/O errors creating directory
fn ensure_plans_directory(workspace_root: &Path) -> Result<PathBuf> {
    let plans_dir = workspace_root.join(".memories").join("plans");
    fs::create_dir_all(&plans_dir)?;
    Ok(plans_dir)
}
