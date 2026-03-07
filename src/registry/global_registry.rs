//! Global project registry stored at `~/.julie/registry.toml`.
//!
//! Tracks all known projects on the machine — distinct from per-project
//! `workspace_registry.json` which tracks reference workspaces within a project.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::workspace::registry::generate_workspace_id;

/// Global registry of all known projects on this machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRegistry {
    pub version: String,
    pub projects: HashMap<String, ProjectEntry>,
}

/// A single project tracked by the global registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub name: String,
    pub path: PathBuf,
    pub workspace_id: String,
    pub last_indexed: Option<String>,
    pub symbol_count: Option<u64>,
    pub file_count: Option<u64>,
    pub status: ProjectStatus,
}

/// Status of a project in the registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProjectStatus {
    Registered,
    Indexing,
    Ready,
    Stale,
    Error(String),
}

const REGISTRY_VERSION: &str = "1";
const REGISTRY_FILENAME: &str = "registry.toml";

impl GlobalRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            version: REGISTRY_VERSION.to_string(),
            projects: HashMap::new(),
        }
    }

    /// Load registry from file, creating a new one if it doesn't exist.
    pub fn load(julie_home: &Path) -> Result<Self> {
        let path = julie_home.join(REGISTRY_FILENAME);
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read registry file {:?}", path))?;

        if content.trim().is_empty() {
            return Ok(Self::new());
        }

        let registry: GlobalRegistry = toml::from_str(&content)
            .with_context(|| format!("Failed to parse registry file {:?}", path))?;

        Ok(registry)
    }

    /// Persist registry to file atomically (write-to-temp, rename).
    pub fn save(&self, julie_home: &Path) -> Result<()> {
        fs::create_dir_all(julie_home)
            .with_context(|| format!("Failed to create directory {:?}", julie_home))?;

        let target = julie_home.join(REGISTRY_FILENAME);
        let temp = julie_home.join(format!("{}.tmp", REGISTRY_FILENAME));

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize registry to TOML")?;

        fs::write(&temp, &content)
            .with_context(|| format!("Failed to write temp registry file {:?}", temp))?;

        fs::rename(&temp, &target)
            .with_context(|| format!("Failed to rename {:?} -> {:?}", temp, target))?;

        Ok(())
    }

    /// Register a project by path. Returns the workspace ID.
    ///
    /// If the project is already registered (same workspace ID), this is a no-op
    /// and returns the existing workspace ID.
    pub fn register_project(&mut self, project_path: &Path) -> Result<String> {
        let canonical = project_path.canonicalize()
            .with_context(|| format!("Failed to resolve path {:?}", project_path))?;
        let path_str = canonical.to_string_lossy();
        let workspace_id = generate_workspace_id(&path_str)?;

        if self.projects.contains_key(&workspace_id) {
            return Ok(workspace_id);
        }

        let name = canonical
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path_str.to_string());

        let entry = ProjectEntry {
            name,
            path: canonical,
            workspace_id: workspace_id.clone(),
            last_indexed: None,
            symbol_count: None,
            file_count: None,
            status: ProjectStatus::Registered,
        };

        self.projects.insert(workspace_id.clone(), entry);
        Ok(workspace_id)
    }

    /// Remove a project by workspace ID. Returns true if it existed.
    pub fn remove_project(&mut self, workspace_id: &str) -> bool {
        self.projects.remove(workspace_id).is_some()
    }

    /// Get a project by workspace ID.
    pub fn get_project(&self, workspace_id: &str) -> Option<&ProjectEntry> {
        self.projects.get(workspace_id)
    }

    /// Get a mutable project entry by workspace ID.
    pub fn get_project_mut(&mut self, workspace_id: &str) -> Option<&mut ProjectEntry> {
        self.projects.get_mut(workspace_id)
    }

    /// List all projects, sorted by name.
    pub fn list_projects(&self) -> Vec<&ProjectEntry> {
        let mut entries: Vec<_> = self.projects.values().collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }

    /// Update a project's status to Indexing.
    pub fn mark_indexing(&mut self, workspace_id: &str) -> bool {
        if let Some(entry) = self.projects.get_mut(workspace_id) {
            entry.status = ProjectStatus::Indexing;
            true
        } else {
            false
        }
    }

    /// Update a project's status to Ready with index stats.
    pub fn mark_ready(
        &mut self,
        workspace_id: &str,
        symbol_count: u64,
        file_count: u64,
    ) -> bool {
        if let Some(entry) = self.projects.get_mut(workspace_id) {
            entry.status = ProjectStatus::Ready;
            entry.last_indexed = Some(Utc::now().to_rfc3339());
            entry.symbol_count = Some(symbol_count);
            entry.file_count = Some(file_count);
            true
        } else {
            false
        }
    }

    /// Update a project's status to Error.
    pub fn mark_error(&mut self, workspace_id: &str, message: String) -> bool {
        if let Some(entry) = self.projects.get_mut(workspace_id) {
            entry.status = ProjectStatus::Error(message);
            true
        } else {
            false
        }
    }
}

impl Default for GlobalRegistry {
    fn default() -> Self {
        Self::new()
    }
}
