//! Runtime factory managing workspace-bound handlers; every handler is the writer for its workspace.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::handler::JulieServerHandler;
use crate::paths::RegistryPaths;
use crate::registry::database::DaemonDatabase;
use crate::request_engine::types::{
    RequestContext, RequestFailure, RequestReadiness, SemanticMode, WorkspaceBinding,
};
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeKey {
    pub root: PathBuf,
    pub index_root: PathBuf,
}

pub struct RequestRuntime {
    handler: Arc<JulieServerHandler>,
    binding: Option<WorkspaceBinding>,
}

impl RequestRuntime {
    pub fn new(handler: Arc<JulieServerHandler>, binding: Option<WorkspaceBinding>) -> Self {
        Self { handler, binding }
    }

    pub fn handler(&self) -> &Arc<JulieServerHandler> {
        &self.handler
    }

    pub fn binding(&self) -> Option<&WorkspaceBinding> {
        self.binding.as_ref()
    }

    pub fn readiness(&self) -> RequestReadiness {
        RequestReadiness::ready(SemanticMode::Auto)
    }
}

#[derive(Clone)]
pub struct RuntimeFactory {
    registry_paths: RegistryPaths,
    runtimes: Arc<RwLock<HashMap<RuntimeKey, Arc<RequestRuntime>>>>,
    unbound_runtime: Arc<RwLock<Option<Arc<RequestRuntime>>>>,
    template_handler: Option<Arc<JulieServerHandler>>,
}

impl RuntimeFactory {
    pub fn new(registry_paths: RegistryPaths) -> Self {
        Self {
            registry_paths,
            runtimes: Arc::new(RwLock::new(HashMap::new())),
            unbound_runtime: Arc::new(RwLock::new(None)),
            template_handler: None,
        }
    }

    pub fn with_handler(handler: Arc<JulieServerHandler>, registry_paths: RegistryPaths) -> Self {
        let root = handler.current_workspace_root();
        let index_root = handler.in_process_index_root.clone().unwrap_or_else(|| {
            let id = handler.current_workspace_id().unwrap_or_default();
            registry_paths.workspace_index_dir(&id)
        });
        let key = RuntimeKey {
            root: root.clone(),
            index_root: index_root.clone(),
        };
        let binding = handler.current_workspace_id().map(|id| WorkspaceBinding {
            workspace_id: id,
            root,
            index_root,
        });
        let runtime = Arc::new(RequestRuntime::new(Arc::clone(&handler), binding));
        let mut map = HashMap::new();
        map.insert(key, Arc::clone(&runtime));
        Self {
            registry_paths,
            runtimes: Arc::new(RwLock::new(map)),
            unbound_runtime: Arc::new(RwLock::new(None)),
            template_handler: Some(Arc::clone(&handler)),
        }
    }

    pub fn registry_paths(&self) -> &RegistryPaths {
        &self.registry_paths
    }

    pub fn template_handler(&self) -> Option<&Arc<JulieServerHandler>> {
        self.template_handler.as_ref()
    }

    pub async fn acquire(
        &self,
        binding: Option<&WorkspaceBinding>,
        context: &RequestContext,
    ) -> Result<Arc<RequestRuntime>, RequestFailure> {
        context.check_cancelled()?;

        match binding {
            Some(b) => {
                let key = RuntimeKey {
                    root: b.root.clone(),
                    index_root: b.index_root.clone(),
                };

                // Fast-path read lock
                let runtime = {
                    let map = self.runtimes.read().await;
                    map.get(&key).cloned()
                };

                let runtime = match runtime {
                    Some(r) => r,
                    None => {
                        let mut map = self.runtimes.write().await;
                        if let Some(r) = map.get(&key) {
                            Arc::clone(r)
                        } else {
                            let runtime = self.create_bound_runtime(b, context).await?;
                            map.insert(key, Arc::clone(&runtime));
                            runtime
                        }
                    }
                };

                if runtime.handler().workspace.read().await.is_none() {
                    runtime
                        .handler()
                        .initialize_workspace_with_force(None, false)
                        .await
                        .map_err(|e| {
                            RequestFailure::internal(format!("Failed to initialize workspace: {e}"))
                        })?;
                }
                if !*runtime.handler().is_indexed.read().await {
                    crate::startup::run_primary_workspace_repair(runtime.handler())
                        .await
                        .ok();
                }

                Ok(runtime)
            }
            None => {
                // Unbound runtime acquisition
                {
                    let unbound = self.unbound_runtime.read().await;
                    if let Some(runtime) = unbound.as_ref() {
                        return Ok(Arc::clone(runtime));
                    }
                }

                let mut unbound = self.unbound_runtime.write().await;
                if let Some(runtime) = unbound.as_ref() {
                    return Ok(Arc::clone(runtime));
                }

                let runtime = self.create_unbound_runtime().await?;
                *unbound = Some(Arc::clone(&runtime));
                Ok(runtime)
            }
        }
    }

    async fn create_bound_runtime(
        &self,
        binding: &WorkspaceBinding,
        context: &RequestContext,
    ) -> Result<Arc<RequestRuntime>, RequestFailure> {
        context.check_cancelled()?;

        std::fs::create_dir_all(&binding.index_root).map_err(|e| {
            RequestFailure::internal(format!(
                "Failed to create workspace index directory {}: {e}",
                binding.index_root.display()
            ))
        })?;

        let startup_hint = WorkspaceStartupHint {
            path: binding.root.clone(),
            source: Some(WorkspaceStartupSource::Cli),
        };

        let daemon_db = self
            .template_handler
            .as_ref()
            .and_then(|th| th.daemon_db.clone())
            .or_else(|| {
                DaemonDatabase::open(&self.registry_paths.registry_db())
                    .ok()
                    .map(Arc::new)
            });

        let mut handler = JulieServerHandler::new_in_process_with_daemon_db(
            startup_hint,
            None,
            Some(binding.index_root.clone()),
            daemon_db,
        )
        .await
        .map_err(|e| RequestFailure::internal(format!("Failed to build handler: {e}")))?;

        if let Some(ref th) = self.template_handler {
            handler.session_metrics = Arc::clone(&th.session_metrics);
        }

        handler
            .initialize_workspace_with_force(None, false)
            .await
            .map_err(|e| {
                RequestFailure::internal(format!("Failed to initialize workspace: {e}"))
            })?;

        crate::startup::run_primary_workspace_repair(&handler)
            .await
            .ok();

        Ok(Arc::new(RequestRuntime::new(
            Arc::new(handler),
            Some(binding.clone()),
        )))
    }

    async fn create_unbound_runtime(&self) -> Result<Arc<RequestRuntime>, RequestFailure> {
        let temp_root = tempfile::tempdir().map_err(|e| {
            RequestFailure::internal(format!(
                "Failed to create temporary directory for unbound runtime: {e}"
            ))
        })?;
        let root = temp_root.path().to_path_buf();
        let index_root = crate::workspace::registry::generate_workspace_id(&root.to_string_lossy())
            .ok()
            .map(|id| self.registry_paths.workspace_index_dir(&id));
        let startup_hint = WorkspaceStartupHint {
            path: root,
            source: Some(WorkspaceStartupSource::Cli),
        };
        let daemon_db = DaemonDatabase::open(&self.registry_paths.registry_db())
            .ok()
            .map(Arc::new);
        let handler = JulieServerHandler::new_in_process_with_daemon_db(
            startup_hint,
            None,
            index_root,
            daemon_db,
        )
        .await
        .map_err(|e| RequestFailure::internal(format!("Failed to build unbound handler: {e}")))?;

        Ok(Arc::new(RequestRuntime::new(Arc::new(handler), None)))
    }
}
