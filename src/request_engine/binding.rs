//! Workspace binding resolution, conflict detection, and sensitive root safeguards.

use crate::paths::RegistryPaths;
use crate::request_engine::types::{RequestFailure, WorkspaceBinding};
use julie_core::workspace::registry::generate_workspace_id;
use julie_core::workspace::root_safety::reject_sensitive_workspace_root;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct BindingResolver {
    pub process_workspace: Option<PathBuf>,
    pub standalone: bool,
    pub registry_paths: RegistryPaths,
    pub daemon_db: Option<std::sync::Arc<crate::registry::database::DaemonDatabase>>,
    pub index_base_override: Option<PathBuf>,
}

impl BindingResolver {
    pub fn new(
        process_workspace: Option<PathBuf>,
        standalone: bool,
        registry_paths: RegistryPaths,
    ) -> Self {
        Self {
            process_workspace,
            standalone,
            registry_paths,
            daemon_db: None,
            index_base_override: None,
        }
    }

    pub fn with_daemon_db(
        mut self,
        daemon_db: Option<std::sync::Arc<crate::registry::database::DaemonDatabase>>,
    ) -> Self {
        self.daemon_db = daemon_db;
        self
    }

    pub fn with_index_base_override(mut self, index_base_override: Option<PathBuf>) -> Self {
        self.index_base_override = index_base_override;
        self
    }

    /// Resolves the workspace binding deterministically following precedence:
    /// argument_workspace > envelope_workspace > process_workspace.
    /// Rejects conflicts, sensitive roots, and missing paths without creating storage.
    pub fn resolve(
        &self,
        envelope_workspace: Option<PathBuf>,
        argument_workspace: Option<PathBuf>,
        is_unbound: bool,
    ) -> Result<Option<WorkspaceBinding>, RequestFailure> {
        let candidate_path = match (envelope_workspace, argument_workspace) {
            (Some(env), Some(arg)) => {
                let arg_str = arg.to_string_lossy();
                if arg_str == "primary" || arg_str == "default" {
                    Some(env)
                } else if arg.is_absolute() || arg.exists() {
                    let env_canon = self.canonicalize_or_verbatim(&env);
                    let arg_canon = self.canonicalize_or_verbatim(&arg);
                    if env_canon != arg_canon {
                        return Err(RequestFailure::workspace_conflict(format!(
                            "Conflicting workspace selectors: envelope specifies '{}', but arguments specify '{}'",
                            env.display(),
                            arg.display()
                        )));
                    }
                    Some(env_canon)
                } else {
                    Some(env)
                }
            }
            (Some(env), None) => Some(env),
            (None, Some(arg)) => {
                let arg_str = arg.to_string_lossy();
                if arg_str == "primary" || arg_str == "default" {
                    self.process_workspace.clone()
                } else {
                    Some(arg)
                }
            }
            (None, None) => self.process_workspace.clone(),
        };

        let candidate_path = match candidate_path {
            Some(p) if p.to_string_lossy() == "primary" || p.to_string_lossy() == "default" => {
                self.process_workspace.clone()
            }
            other => other,
        };

        // 2. Unbound or missing handling
        let root_path = match candidate_path {
            Some(path) => path,
            None => {
                if is_unbound {
                    return Ok(None);
                } else {
                    return Err(RequestFailure::workspace_required(
                        "Missing required workspace parameter",
                    ));
                }
            }
        };

        // 3. Validation before ANY directory creation
        let (root_path, known_id) = if !root_path.exists() {
            let candidate_str = root_path.to_string_lossy();
            let from_db = if let Some(ref db) = self.daemon_db {
                db.get_workspace(&candidate_str).ok().flatten()
            } else if let Ok(db) =
                crate::registry::database::DaemonDatabase::open(&self.registry_paths.registry_db())
            {
                db.get_workspace(&candidate_str).ok().flatten()
            } else {
                None
            };
            if let Some(row) = from_db {
                (PathBuf::from(row.path), Some(row.workspace_id))
            } else {
                return Err(RequestFailure::invalid_arguments(format!(
                    "Workspace path does not exist: {}",
                    root_path.display()
                )));
            }
        } else {
            (root_path, None)
        };

        if !root_path.is_dir() {
            return Err(RequestFailure::invalid_arguments(format!(
                "Workspace root is not a directory: {}",
                root_path.display()
            )));
        }

        let canonical_root = root_path.canonicalize().map_err(|e| {
            RequestFailure::invalid_arguments(format!(
                "Failed to canonicalize workspace root {}: {e}",
                root_path.display()
            ))
        })?;

        if let Err(e) = reject_sensitive_workspace_root(&canonical_root) {
            return Err(RequestFailure::sensitive_root(e.to_string()));
        }

        // 4. Derive workspace ID & index root
        let workspace_id = if let Some(id) = known_id {
            id
        } else {
            generate_workspace_id(&canonical_root.to_string_lossy()).map_err(|e| {
                RequestFailure::internal(format!("Failed to generate workspace ID: {e}"))
            })?
        };

        let index_root = if let Some(ref base) = self.index_base_override {
            base.join(&workspace_id)
        } else if self.standalone {
            canonical_root
                .join(".julie")
                .join("indexes")
                .join(&workspace_id)
        } else {
            self.registry_paths.workspace_index_dir(&workspace_id)
        };

        Ok(Some(WorkspaceBinding {
            workspace_id,
            root: canonical_root,
            index_root,
        }))
    }

    fn canonicalize_or_verbatim(&self, path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }
}
