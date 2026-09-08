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
        let argument_workspace = match argument_workspace {
            Some(ref p) if p.to_string_lossy() == "resume" || p.to_string_lossy() == "rollback" => {
                None
            }
            other => other,
        };

        let (root_path, known_id) = match (envelope_workspace, argument_workspace) {
            (Some(env), Some(arg)) => {
                let env_str = env.to_string_lossy();
                let arg_str = arg.to_string_lossy();
                if arg_str == "primary" || arg_str == "default" {
                    let target = if env_str == "primary" || env_str == "default" {
                        self.process_workspace.as_ref().ok_or_else(|| {
                            RequestFailure::workspace_required(
                                "Missing required workspace parameter",
                            )
                        })?
                    } else {
                        &env
                    };
                    self.resolve_target_path(target)?
                } else if env_str == "primary" || env_str == "default" {
                    self.resolve_target_path(&arg)?
                } else if env == arg {
                    self.resolve_target_path(&arg)?
                } else if (arg.is_absolute() || arg.exists()) && (env.is_absolute() || env.exists())
                {
                    let (env_root, env_id) = self.resolve_target_path(&env)?;
                    let (arg_root, arg_id) = self.resolve_target_path(&arg)?;
                    if env_root != arg_root {
                        return Err(RequestFailure::workspace_conflict(format!(
                            "Conflicting workspace selectors: envelope specifies '{}', but arguments specify '{}'",
                            env.display(),
                            arg.display()
                        )));
                    }
                    (arg_root, arg_id.or(env_id))
                } else {
                    self.resolve_target_path(&env)?
                }
            }
            (Some(env), None) => {
                let env_str = env.to_string_lossy();
                if env_str == "primary" || env_str == "default" {
                    let p = self.process_workspace.as_ref().ok_or_else(|| {
                        RequestFailure::workspace_required("Missing required workspace parameter")
                    })?;
                    self.resolve_target_path(p)?
                } else {
                    self.resolve_target_path(&env)?
                }
            }
            (None, Some(arg)) => {
                let arg_str = arg.to_string_lossy();
                if arg_str == "primary" || arg_str == "default" {
                    let p = self.process_workspace.as_ref().ok_or_else(|| {
                        RequestFailure::workspace_required("Missing required workspace parameter")
                    })?;
                    self.resolve_target_path(p)?
                } else {
                    self.resolve_target_path(&arg)?
                }
            }
            (None, None) => {
                if let Some(ref p) = self.process_workspace {
                    let p_str = p.to_string_lossy();
                    if p_str == "primary" || p_str == "default" {
                        if is_unbound {
                            return Ok(None);
                        } else {
                            return Err(RequestFailure::workspace_required(
                                "Missing required workspace parameter",
                            ));
                        }
                    }
                    self.resolve_target_path(p)?
                } else if is_unbound {
                    return Ok(None);
                } else {
                    return Err(RequestFailure::workspace_required(
                        "Missing required workspace parameter",
                    ));
                }
            }
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

    fn resolve_target_path(
        &self,
        candidate: &Path,
    ) -> Result<(PathBuf, Option<String>), RequestFailure> {
        let candidate_str = candidate.to_string_lossy();
        let from_db = if let Some(ref db) = self.daemon_db {
            db.get_workspace(&candidate_str)
                .ok()
                .flatten()
                .or_else(|| db.get_workspace_by_path(&candidate_str).ok().flatten())
        } else if let Ok(db) =
            crate::registry::database::DaemonDatabase::open(&self.registry_paths.registry_db())
        {
            db.get_workspace(&candidate_str)
                .ok()
                .flatten()
                .or_else(|| db.get_workspace_by_path(&candidate_str).ok().flatten())
        } else {
            None
        };

        if let Some(row) = from_db {
            let p = PathBuf::from(row.path);
            if !p.exists() {
                return Err(RequestFailure::workspace_missing(format!(
                    "Workspace path does not exist: {}",
                    p.display()
                )));
            }
            let canon = self.canonicalize_or_verbatim(&p);
            Ok((canon, Some(row.workspace_id)))
        } else if candidate.exists() {
            let canon = self.canonicalize_or_verbatim(candidate);
            Ok((canon, None))
        } else {
            Err(RequestFailure::workspace_missing(format!(
                "Workspace path does not exist: {}",
                candidate.display()
            )))
        }
    }

    fn canonicalize_or_verbatim(&self, path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }
}
