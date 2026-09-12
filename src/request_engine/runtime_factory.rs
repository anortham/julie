//! Runtime factory managing workspace-bound handlers; every handler is the writer for its workspace.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::warn;

#[cfg(test)]
use std::sync::Mutex;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(test)]
use tokio::sync::Notify;

use crate::handler::JulieServerHandler;
use crate::paths::RegistryPaths;
use crate::registry::database::DaemonDatabase;
use crate::registry::project_log::{ProjectLog, SERVICE_LOG_PREFIX};
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

struct RuntimeSlot {
    runtime: RwLock<Option<Arc<RequestRuntime>>>,
    last_activity: std::sync::Mutex<Instant>,
}

impl RuntimeSlot {
    fn empty() -> Self {
        Self {
            runtime: RwLock::new(None),
            last_activity: std::sync::Mutex::new(Instant::now()),
        }
    }

    fn loaded(runtime: Arc<RequestRuntime>) -> Self {
        Self {
            runtime: RwLock::new(Some(runtime)),
            last_activity: std::sync::Mutex::new(Instant::now()),
        }
    }
}

#[derive(Clone, Copy)]
struct RuntimeRetirementPolicy {
    max_idle: usize,
    idle_for: Duration,
}

const RUNTIME_RETIREMENT_POLICY: RuntimeRetirementPolicy = RuntimeRetirementPolicy {
    max_idle: 8,
    idle_for: Duration::from_secs(60),
};

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct RetirementProbe {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

#[cfg(test)]
impl RetirementProbe {
    pub(crate) async fn entered(&self) {
        self.entered.notified().await;
    }

    pub(crate) fn release(&self) {
        self.release.notify_one();
    }
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct BoundInitializationProbe {
    entered: Arc<Notify>,
    release: Arc<Notify>,
    attempts: Arc<AtomicUsize>,
}

#[cfg(test)]
impl BoundInitializationProbe {
    pub(crate) async fn entered(&self) {
        self.entered.notified().await;
    }

    pub(crate) fn release(&self) {
        self.release.notify_one();
    }

    pub(crate) fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }
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
    runtimes: Arc<RwLock<HashMap<RuntimeKey, Arc<RuntimeSlot>>>>,
    unbound_runtime: Arc<RwLock<Option<Arc<RequestRuntime>>>>,
    template_handler: Option<Arc<JulieServerHandler>>,
    semantic_runtime:
        Arc<std::sync::RwLock<Arc<dyn crate::request_engine::semantic::SemanticRuntime>>>,
    #[cfg(test)]
    bound_initialization_probe: Arc<Mutex<Option<BoundInitializationProbe>>>,
    #[cfg(test)]
    retirement_policy: Arc<Mutex<RuntimeRetirementPolicy>>,
    #[cfg(test)]
    retirement_probe: Arc<Mutex<Option<RetirementProbe>>>,
}

impl RuntimeFactory {
    pub fn new(registry_paths: RegistryPaths) -> Self {
        let semantic_runtime: Arc<dyn crate::request_engine::semantic::SemanticRuntime> = Arc::new(
            crate::request_engine::semantic::DefaultSemanticRuntime::from_registry_paths(
                registry_paths.clone(),
            ),
        );
        Self {
            registry_paths,
            runtimes: Arc::new(RwLock::new(HashMap::new())),
            unbound_runtime: Arc::new(RwLock::new(None)),
            template_handler: None,
            semantic_runtime: Arc::new(std::sync::RwLock::new(semantic_runtime)),
            #[cfg(test)]
            bound_initialization_probe: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            retirement_policy: Arc::new(Mutex::new(RUNTIME_RETIREMENT_POLICY)),
            #[cfg(test)]
            retirement_probe: Arc::new(Mutex::new(None)),
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
        map.insert(key, Arc::new(RuntimeSlot::loaded(Arc::clone(&runtime))));
        let semantic_runtime = handler.semantic_runtime();
        Self {
            registry_paths,
            runtimes: Arc::new(RwLock::new(map)),
            unbound_runtime: Arc::new(RwLock::new(None)),
            template_handler: Some(Arc::clone(&handler)),
            semantic_runtime: Arc::new(std::sync::RwLock::new(semantic_runtime)),
            #[cfg(test)]
            bound_initialization_probe: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            retirement_policy: Arc::new(Mutex::new(RUNTIME_RETIREMENT_POLICY)),
            #[cfg(test)]
            retirement_probe: Arc::new(Mutex::new(None)),
        }
    }

    pub fn registry_paths(&self) -> &RegistryPaths {
        &self.registry_paths
    }

    pub fn template_handler(&self) -> Option<&Arc<JulieServerHandler>> {
        self.template_handler.as_ref()
    }

    pub fn semantic_runtime(&self) -> Arc<dyn crate::request_engine::semantic::SemanticRuntime> {
        self.semantic_runtime.read().unwrap().clone()
    }

    pub async fn loaded_runtime_count(&self) -> usize {
        let slots = self
            .runtimes
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut runtime_count = 0;
        for slot in slots {
            if slot.runtime.read().await.is_some() {
                runtime_count += 1;
            }
        }
        let has_unbound_runtime = self.unbound_runtime.read().await.is_some();
        runtime_count + usize::from(has_unbound_runtime)
    }

    pub async fn loaded_watcher_count(&self) -> usize {
        let slots = self
            .runtimes
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut count = 0;
        for slot in slots {
            let runtime = slot.runtime.read().await.clone();
            if let Some(runtime) = runtime
                && runtime
                    .handler()
                    .loaded_workspace_file_watcher_running()
                    .await
            {
                count += 1;
            }
        }
        count
    }

    pub fn set_semantic_runtime(
        &self,
        runtime: Arc<dyn crate::request_engine::semantic::SemanticRuntime>,
    ) {
        if let Ok(mut guard) = self.semantic_runtime.write() {
            *guard = Arc::clone(&runtime);
        }
        if let Some(ref template) = self.template_handler {
            template.set_semantic_runtime(runtime);
        }
    }

    #[cfg(test)]
    pub(crate) fn pause_bound_initialization(&self) -> BoundInitializationProbe {
        let probe = BoundInitializationProbe {
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
            attempts: Arc::new(AtomicUsize::new(0)),
        };
        *self.bound_initialization_probe.lock().unwrap() = Some(probe.clone());
        probe
    }

    #[cfg(test)]
    pub(crate) fn set_retirement_policy(&self, max_idle: usize, idle_for: Duration) {
        *self.retirement_policy.lock().unwrap() = RuntimeRetirementPolicy { max_idle, idle_for };
    }

    #[cfg(test)]
    pub(crate) fn pause_retirement(&self) -> RetirementProbe {
        let probe = RetirementProbe {
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        };
        *self.retirement_probe.lock().unwrap() = Some(probe.clone());
        probe
    }

    #[cfg(test)]
    pub(crate) async fn set_slot_activity(&self, binding: &WorkspaceBinding, activity: Instant) {
        let key = RuntimeKey {
            root: binding.root.clone(),
            index_root: binding.index_root.clone(),
        };
        if let Some(slot) = self.runtimes.read().await.get(&key).cloned() {
            *slot.last_activity.lock().unwrap() = activity;
        }
    }

    #[cfg(test)]
    pub(crate) async fn slot_is_loaded(&self, binding: &WorkspaceBinding) -> bool {
        let key = RuntimeKey {
            root: binding.root.clone(),
            index_root: binding.index_root.clone(),
        };
        let slot = self.runtimes.read().await.get(&key).cloned();
        match slot {
            Some(slot) => slot.runtime.read().await.is_some(),
            None => false,
        }
    }

    #[cfg(test)]
    pub(crate) async fn slot_count(&self) -> usize {
        self.runtimes.read().await.len()
    }

    #[cfg(test)]
    async fn wait_for_retirement(&self) {
        let probe = { self.retirement_probe.lock().unwrap().clone() };
        if let Some(probe) = probe {
            probe.entered.notify_one();
            probe.release.notified().await;
        }
    }

    #[cfg(not(test))]
    async fn wait_for_retirement(&self) {}

    fn retirement_policy(&self) -> RuntimeRetirementPolicy {
        #[cfg(test)]
        {
            *self.retirement_policy.lock().unwrap()
        }
        #[cfg(not(test))]
        {
            RUNTIME_RETIREMENT_POLICY
        }
    }

    pub(crate) async fn retire_idle_runtimes(&self, now: Instant) {
        let policy = self.retirement_policy();
        let slots = self
            .runtimes
            .read()
            .await
            .iter()
            .map(|(key, slot)| (key.clone(), Arc::clone(slot)))
            .collect::<Vec<_>>();

        let mut idle = Vec::new();
        for (key, slot) in slots {
            let runtime = slot.runtime.read().await.clone();
            let Some(runtime) = runtime else {
                continue;
            };
            let active = Arc::strong_count(&runtime) > 2
                || runtime.binding().is_some_and(|binding| {
                    runtime
                        .handler()
                        .embedding_tasks
                        .try_lock()
                        .map_or(true, |tasks| tasks.contains_key(&binding.workspace_id))
                });
            if active {
                *slot.last_activity.lock().unwrap() = now;
                continue;
            }
            let last_activity = *slot.last_activity.lock().unwrap();
            idle.push((key, slot, last_activity));
        }
        idle.sort_by_key(|(_, _, last_activity)| *last_activity);
        let excess = idle.len().saturating_sub(policy.max_idle);
        for (index, (_, slot, last_activity)) in idle.into_iter().enumerate() {
            if index >= excess && now.duration_since(last_activity) < policy.idle_for {
                continue;
            }
            self.retire_slot(slot).await;
        }
    }

    async fn retire_slot(&self, slot: Arc<RuntimeSlot>) {
        let mut runtime = slot.runtime.write().await;
        let Some(loaded) = runtime.as_ref() else {
            return;
        };
        if Arc::strong_count(loaded) > 1 {
            *slot.last_activity.lock().unwrap() = Instant::now();
            return;
        }
        if let Some(binding) = loaded.binding() {
            let embedding_tasks = loaded.handler().embedding_tasks.lock().await;
            if embedding_tasks.contains_key(&binding.workspace_id) {
                *slot.last_activity.lock().unwrap() = Instant::now();
                return;
            }
        }
        self.wait_for_retirement().await;
        if let Err(error) = loaded.handler().teardown_loaded_workspace().await {
            warn!("Failed to retire runtime: {error:#}");
            return;
        }
        *runtime = None;
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

                let slot = {
                    let map = self.runtimes.read().await;
                    map.get(&key).cloned()
                };

                let slot = match slot {
                    Some(slot) => slot,
                    None => {
                        let mut map = self.runtimes.write().await;
                        if let Some(slot) = map.get(&key) {
                            Arc::clone(slot)
                        } else {
                            let slot = Arc::new(RuntimeSlot::empty());
                            map.insert(key, Arc::clone(&slot));
                            slot
                        }
                    }
                };

                let runtime = {
                    let mut runtime = slot.runtime.write().await;
                    if runtime.is_none() {
                        context.check_cancelled()?;
                        self.wait_for_bound_initialization(context).await?;
                        let created = self.create_bound_runtime(b, context).await?;
                        *runtime = Some(created);
                    }
                    runtime.as_ref().cloned().expect("runtime initialized")
                };
                *slot.last_activity.lock().unwrap() = Instant::now();

                if runtime.handler().workspace.read().await.is_none() {
                    initialize_recovering_store(runtime.handler(), &b.index_root).await?;
                }
                if !*runtime.handler().is_indexed.read().await {
                    warn_on_repair_failure(
                        &b.index_root,
                        crate::startup::run_primary_workspace_repair(runtime.handler()).await,
                    );
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

    #[cfg(test)]
    async fn wait_for_bound_initialization(
        &self,
        context: &RequestContext,
    ) -> Result<(), RequestFailure> {
        let probe = self.bound_initialization_probe.lock().unwrap().clone();
        if let Some(probe) = probe {
            probe.attempts.fetch_add(1, Ordering::SeqCst);
            probe.entered.notify_one();
            tokio::select! {
                _ = probe.release.notified() => Ok(()),
                _ = context.cancellation.cancelled() => context.check_cancelled(),
            }
        } else {
            Ok(())
        }
    }

    #[cfg(not(test))]
    async fn wait_for_bound_initialization(
        &self,
        _context: &RequestContext,
    ) -> Result<(), RequestFailure> {
        Ok(())
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

        handler.set_semantic_runtime(self.semantic_runtime());
        handler.set_injected_embedding_provider(handler.semantic_runtime().provider());

        initialize_recovering_store(&handler, &binding.index_root).await?;

        warn_on_repair_failure(
            &binding.index_root,
            crate::startup::run_primary_workspace_repair(&handler).await,
        );

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
        let mut handler = JulieServerHandler::new_in_process_with_daemon_db(
            startup_hint,
            None,
            index_root,
            daemon_db,
        )
        .await
        .map_err(|e| RequestFailure::internal(format!("Failed to build unbound handler: {e}")))?;

        handler.set_semantic_runtime(self.semantic_runtime());
        handler.set_injected_embedding_provider(handler.semantic_runtime().provider());
        handler.project_log = Some(Arc::new(ProjectLog::in_dir(
            self.registry_paths.logs_dir(),
            SERVICE_LOG_PREFIX,
        )));

        Ok(Arc::new(RequestRuntime::new(Arc::new(handler), None)))
    }
}

/// Report a repair scan that did not run. The checkout keeps whatever it had,
/// so the operator needs the path and the cause in the message itself: the
/// dashboard error buffer keeps only the message, not the structured fields.
pub(crate) fn warn_on_repair_failure(
    index_root: &std::path::Path,
    outcome: anyhow::Result<Option<crate::startup::PrimaryWorkspaceRepairPlan>>,
) {
    if let Err(err) = outcome {
        warn!(
            "startup repair scan failed for {}: {err:#}",
            index_root.display()
        );
    }
}

pub(crate) async fn initialize_recovering_store(
    handler: &JulieServerHandler,
    index_root: &std::path::Path,
) -> Result<(), RequestFailure> {
    handler
        .initialize_workspace_with_force(None, false)
        .await
        .map_err(|e| RequestFailure::internal(format!("Failed to initialize workspace: {e}")))?;
    if handler.get_workspace().await.ok().flatten().is_none() {
        return Err(RequestFailure::internal(format!(
            "Failed to initialize workspace at {}",
            index_root.display()
        )));
    }
    Ok(())
}
