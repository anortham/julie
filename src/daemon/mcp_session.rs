use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::SystemTime;

use anyhow::{Context, Result};
use axum::http::request::Parts;
use rmcp::Service;
use rmcp::model::{
    ClientNotification, ClientRequest, ErrorData as McpError, Implementation, ServerCapabilities,
    ServerInfo, ServerResult,
};
use rmcp::service::{NotificationContext, RequestContext, RoleServer};
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::daemon::database::DaemonDatabase;
use crate::daemon::embedding_service::EmbeddingService;
use crate::daemon::lifecycle::{
    DaemonLifecycleController, DisconnectLifecycleAction, IncomingSessionAction, ShutdownCause,
    stale_binary_accept_action, stale_binary_disconnect_action, version_gate_action,
};
use crate::daemon::session::{SessionLifecycleHandle, SessionTracker};
use crate::daemon::watcher_pool::WatcherPool;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::daemon::workspace_session_attachment::WorkspaceSessionAttachment;
use crate::dashboard::state::DashboardEvent;
use crate::handler::JulieServerHandler;
use crate::handler::session_workspace::SessionWorkspaceState;
use crate::workspace::mutation_gate::Registry as MutationGateRegistry;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

pub(crate) const HEADER_JULIE_WORKSPACE: &str = "x-julie-workspace";
pub(crate) const HEADER_JULIE_WORKSPACE_SOURCE: &str = "x-julie-workspace-source";
pub(crate) const HEADER_JULIE_VERSION: &str = "x-julie-version";

pub(crate) fn workspace_ids_to_disconnect(
    startup_workspace_id: &str,
    attached_workspace_ids: Vec<String>,
    startup_workspace_was_attached: bool,
) -> Vec<String> {
    let mut disconnect_ids = attached_workspace_ids;
    if startup_workspace_was_attached && !disconnect_ids.iter().any(|id| id == startup_workspace_id)
    {
        disconnect_ids.push(startup_workspace_id.to_string());
    }
    disconnect_ids.sort();
    disconnect_ids.dedup();
    disconnect_ids
}

#[derive(Clone)]
pub(crate) struct HttpSessionAdmission {
    lifecycle: DaemonLifecycleController,
    startup_binary_mtime: Option<SystemTime>,
    current_binary_mtime: Arc<dyn Fn() -> Option<SystemTime> + Send + Sync>,
    /// Counts every `apply_admission_action` invocation. Used in tests to verify
    /// that the short-circuit in `admit_initialize` prevents the second gate from
    /// running when the first returns `Err`.
    pub(crate) apply_action_call_count: Arc<AtomicUsize>,
}

impl HttpSessionAdmission {
    pub(crate) fn new(
        lifecycle: DaemonLifecycleController,
        startup_binary_mtime: Option<SystemTime>,
        current_binary_mtime: impl Fn() -> Option<SystemTime> + Send + Sync + 'static,
    ) -> Self {
        Self {
            lifecycle,
            startup_binary_mtime,
            current_binary_mtime: Arc::new(current_binary_mtime),
            apply_action_call_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

fn restart_required_error() -> McpError {
    McpError::internal_error(
        "Julie daemon restart required; reconnect after daemon restarts",
        None,
    )
}

struct HttpSessionRegistration {
    session_id: String,
    session_lifecycle: Option<SessionLifecycleHandle>,
}

#[derive(Clone)]
pub(crate) struct DaemonSessionDependencies {
    pool: Arc<WorkspacePool>,
    daemon_db: Option<Arc<DaemonDatabase>>,
    embedding_service: Arc<EmbeddingService>,
    restart_pending: Arc<AtomicBool>,
    dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    watcher_pool: Option<Arc<WatcherPool>>,
    sessions: Option<Arc<SessionTracker>>,
    http_admission: Option<HttpSessionAdmission>,
    mutation_gate_registry: Arc<MutationGateRegistry>,
}

impl DaemonSessionDependencies {
    pub(crate) fn new(
        pool: Arc<WorkspacePool>,
        daemon_db: Option<Arc<DaemonDatabase>>,
        embedding_service: Arc<EmbeddingService>,
        restart_pending: Arc<AtomicBool>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
        watcher_pool: Option<Arc<WatcherPool>>,
        sessions: Arc<SessionTracker>,
        mutation_gate_registry: Arc<MutationGateRegistry>,
    ) -> Self {
        Self {
            pool,
            daemon_db,
            embedding_service,
            restart_pending,
            dashboard_tx,
            watcher_pool,
            sessions: Some(sessions),
            http_admission: None,
            mutation_gate_registry,
        }
    }

    pub(crate) fn with_http_admission(mut self, admission: HttpSessionAdmission) -> Self {
        self.http_admission = Some(admission);
        self
    }

    fn project_cleanup_attachment(
        &self,
        startup_hint: WorkspaceStartupHint,
    ) -> WorkspaceSessionAttachment {
        WorkspaceSessionAttachment::new(
            None,
            self.daemon_db.clone(),
            self.watcher_pool.clone(),
            Some(Arc::clone(&self.embedding_service)),
            Arc::new(StdRwLock::new(SessionWorkspaceState::new(startup_hint))),
        )
    }

    async fn cleanup_workspace_ids(
        &self,
        startup_workspace_id: &str,
        startup_hint: WorkspaceStartupHint,
        attached_workspace_ids: Vec<String>,
        startup_workspace_was_attached: bool,
        transport_label: &str,
    ) {
        let cleanup_attachment = self.project_cleanup_attachment(startup_hint);
        for workspace_id in workspace_ids_to_disconnect(
            startup_workspace_id,
            attached_workspace_ids,
            startup_workspace_was_attached,
        ) {
            if let Err(error) = cleanup_attachment
                .detach_workspace_resources(&workspace_id)
                .await
            {
                warn!(
                    workspace_id,
                    "{transport_label} workspace session resource detach failed: {error}"
                );
            }
        }
    }
}

pub(crate) struct DaemonMcpSession {
    handler: JulieServerHandler,
    dependencies: Arc<DaemonSessionDependencies>,
    session_id: String,
    startup_workspace_id: String,
    cleanup_startup_hint: WorkspaceStartupHint,
    startup_workspace_was_attached: bool,
    transport_label: &'static str,
}

impl DaemonMcpSession {
    pub(crate) async fn start(
        dependencies: Arc<DaemonSessionDependencies>,
        session_id: impl Into<String>,
        workspace_startup_hint: WorkspaceStartupHint,
        session_lifecycle: Option<SessionLifecycleHandle>,
        transport_label: &'static str,
    ) -> Result<Self> {
        let session_id = session_id.into();
        let workspace_path = workspace_startup_hint.path.clone();
        let cleanup_startup_hint = workspace_startup_hint.clone();
        let full_workspace_id = generate_workspace_id(&workspace_path.to_string_lossy())
            .context("Failed to generate workspace ID")?;

        info!(
            session_id = %session_id,
            workspace_id = %full_workspace_id,
            workspace_source = %workspace_startup_hint
                .source
                .map(WorkspaceStartupSource::as_header_value)
                .unwrap_or("legacy-missing"),
            transport = transport_label,
            "Getting or initializing workspace from pool"
        );

        let defer_startup_workspace_attach = matches!(
            workspace_startup_hint.source,
            Some(WorkspaceStartupSource::Cwd)
        );

        let handler_result = if defer_startup_workspace_attach {
            JulieServerHandler::new_deferred_daemon_startup_hint(
                workspace_startup_hint,
                dependencies.daemon_db.clone(),
                Some(Arc::clone(&dependencies.embedding_service)),
                Some(Arc::clone(&dependencies.restart_pending)),
                dependencies.dashboard_tx.clone(),
                dependencies.watcher_pool.clone(),
                Some(Arc::clone(&dependencies.pool)),
            )
            .await
        } else {
            let workspace = dependencies
                .pool
                .get_or_init(&full_workspace_id, workspace_path.clone())
                .await
                .context("Failed to initialize workspace in pool")?;
            JulieServerHandler::new_with_shared_workspace_startup_hint(
                workspace,
                workspace_startup_hint,
                dependencies.daemon_db.clone(),
                Some(full_workspace_id.clone()),
                Some(Arc::clone(&dependencies.embedding_service)),
                Some(Arc::clone(&dependencies.restart_pending)),
                dependencies.dashboard_tx.clone(),
                dependencies.watcher_pool.clone(),
                Some(Arc::clone(&dependencies.pool)),
            )
            .await
            .context("Failed to create handler for workspace session")
        };

        let mut handler = match handler_result {
            Ok(handler) => handler,
            Err(error) => {
                let attached = if defer_startup_workspace_attach {
                    Vec::new()
                } else {
                    vec![full_workspace_id.clone()]
                };
                dependencies
                    .cleanup_workspace_ids(
                        &full_workspace_id,
                        cleanup_startup_hint,
                        attached,
                        !defer_startup_workspace_attach,
                        transport_label,
                    )
                    .await;
                return Err(error).context("Failed to create daemon MCP session");
            }
        };
        handler.set_mutation_gate_registry(Arc::clone(&dependencies.mutation_gate_registry));

        if let Some(session_lifecycle) = session_lifecycle {
            handler.attach_session_lifecycle(session_lifecycle);
        }

        if let Some(ref log) = handler.project_log {
            log.session_start(&session_id);
        }
        handler.mark_session_serving();

        Ok(Self {
            handler,
            dependencies,
            session_id,
            startup_workspace_id: full_workspace_id,
            cleanup_startup_hint,
            startup_workspace_was_attached: !defer_startup_workspace_attach,
            transport_label,
        })
    }

    pub(crate) fn handler(&self) -> JulieServerHandler {
        self.handler.clone()
    }

    pub(crate) async fn finish(self) {
        self.handler.mark_session_closing();
        if let Some(ref log) = self.handler.project_log {
            log.session_end(&self.session_id);
        }
        let attached_workspace_ids = self.handler.session_attached_workspace_ids().await;
        self.dependencies
            .cleanup_workspace_ids(
                &self.startup_workspace_id,
                self.cleanup_startup_hint,
                attached_workspace_ids,
                self.startup_workspace_was_attached,
                self.transport_label,
            )
            .await;
    }
}

pub(crate) struct HttpJulieService {
    dependencies: Arc<DaemonSessionDependencies>,
    session_registration: tokio::sync::Mutex<Option<HttpSessionRegistration>>,
    session: tokio::sync::Mutex<Option<DaemonMcpSession>>,
}

impl HttpJulieService {
    pub(crate) fn new(dependencies: Arc<DaemonSessionDependencies>) -> Self {
        Self {
            dependencies,
            session_registration: tokio::sync::Mutex::new(None),
            session: tokio::sync::Mutex::new(None),
        }
    }

    fn active_sessions_before_current(&self) -> usize {
        self.dependencies
            .sessions
            .as_ref()
            .map(|sessions| sessions.active_count())
            .unwrap_or(0)
    }

    fn register_session(&self) -> HttpSessionRegistration {
        self.dependencies
            .sessions
            .as_ref()
            .map(|sessions| {
                let session_id = sessions.add_session();
                let lifecycle = sessions.lifecycle_handle(&session_id);
                if let Some(tx) = &self.dependencies.dashboard_tx {
                    let _ = tx.send(DashboardEvent::SessionChange {
                        active_count: sessions.active_count(),
                    });
                }
                HttpSessionRegistration {
                    session_id,
                    session_lifecycle: Some(lifecycle),
                }
            })
            .unwrap_or_else(|| HttpSessionRegistration {
                session_id: uuid::Uuid::new_v4().to_string(),
                session_lifecycle: None,
            })
    }

    fn remove_session_registration(&self, registration: &HttpSessionRegistration) {
        Self::remove_session_registration_for(&self.dependencies, registration);
    }

    fn remove_session_registration_for(
        dependencies: &DaemonSessionDependencies,
        registration: &HttpSessionRegistration,
    ) {
        let remaining = if let Some(sessions) = &dependencies.sessions {
            sessions.remove_session(&registration.session_id);
            let remaining = sessions.active_count();
            if let Some(tx) = &dependencies.dashboard_tx {
                let _ = tx.send(DashboardEvent::SessionChange {
                    active_count: remaining,
                });
            }
            remaining
        } else {
            0
        };
        Self::apply_disconnect_action_for(dependencies, remaining);
    }

    fn apply_disconnect_action_for(dependencies: &DaemonSessionDependencies, remaining: usize) {
        let Some(admission) = &dependencies.http_admission else {
            return;
        };

        let binary_is_stale = admission
            .startup_binary_mtime
            .zip((admission.current_binary_mtime.as_ref())())
            .is_some_and(|(startup_mtime, current_mtime)| current_mtime > startup_mtime);
        match stale_binary_disconnect_action(
            binary_is_stale,
            admission.lifecycle.restart_pending(),
            remaining,
        ) {
            DisconnectLifecycleAction::None => {}
            DisconnectLifecycleAction::MarkRestartPending(reason) => {
                let transition = admission
                    .lifecycle
                    .mark_restart_pending(remaining, ShutdownCause::RestartRequired);
                if transition.first_request {
                    warn!(
                        ?reason,
                        "Binary rebuild detected at HTTP session disconnect."
                    );
                }
            }
            DisconnectLifecycleAction::TriggerShutdown(cause) => {
                admission.lifecycle.mark_restart_pending(remaining, cause);
            }
        }

        if remaining == 0 && admission.lifecycle.restart_pending() {
            // Restart notify is handled by the earlier mark_restart_pending call;
            // the first transition signals the restart channel internally.
            info!("Last HTTP session disconnected; restart already armed via mark_restart_pending");
        }
    }

    fn apply_admission_action(
        &self,
        admission: &HttpSessionAdmission,
        active_sessions: usize,
        action: IncomingSessionAction,
        gate: &str,
        adapter_version: Option<&str>,
    ) -> Result<(), McpError> {
        admission
            .apply_action_call_count
            .fetch_add(1, Ordering::Relaxed);
        match action {
            IncomingSessionAction::Accept => Ok(()),
            IncomingSessionAction::AcceptWithRestartPending(reason) => {
                let transition = admission
                    .lifecycle
                    .mark_restart_pending(active_sessions, ShutdownCause::RestartRequired);
                warn!(
                    ?reason,
                    gate,
                    active_sessions,
                    first_request = transition.first_request,
                    "HTTP session accepted while daemon restart is pending"
                );
                Ok(())
            }
            IncomingSessionAction::ShutdownForRestart(reason) => {
                admission
                    .lifecycle
                    .mark_restart_pending(active_sessions, ShutdownCause::RestartRequired);
                warn!(
                    adapter_version = adapter_version.unwrap_or("<none>"),
                    daemon_version = env!("CARGO_PKG_VERSION"),
                    ?reason,
                    gate,
                    "Rejecting HTTP session and triggering daemon restart"
                );
                // Restart notify is handled by the mark_restart_pending call above;
                // the first transition signals the restart channel internally.
                Err(restart_required_error())
            }
            IncomingSessionAction::RejectForRestart(reason) => {
                let transition = admission
                    .lifecycle
                    .mark_restart_pending(active_sessions, ShutdownCause::RestartRequired);
                warn!(
                    adapter_version = adapter_version.unwrap_or("<none>"),
                    daemon_version = env!("CARGO_PKG_VERSION"),
                    ?reason,
                    gate,
                    first_request = transition.first_request,
                    "Rejecting HTTP session while daemon waits to restart"
                );
                Err(restart_required_error())
            }
        }
    }

    fn admit_initialize(&self, context: &RequestContext<RoleServer>) -> Result<(), McpError> {
        let Some(admission) = &self.dependencies.http_admission else {
            return Ok(());
        };

        let active_sessions = self.active_sessions_before_current();
        let binary_is_stale = admission
            .startup_binary_mtime
            .zip((admission.current_binary_mtime.as_ref())())
            .is_some_and(|(startup_mtime, current_mtime)| current_mtime > startup_mtime);
        self.apply_admission_action(
            admission,
            active_sessions,
            stale_binary_accept_action(
                binary_is_stale,
                active_sessions,
                admission.lifecycle.restart_pending(),
            ),
            "stale-binary",
            None,
        )?;

        let adapter_version = context
            .extensions
            .get::<Parts>()
            .and_then(|parts| header_value(parts, HEADER_JULIE_VERSION));
        self.apply_admission_action(
            admission,
            active_sessions,
            version_gate_action(adapter_version, env!("CARGO_PKG_VERSION"), active_sessions),
            "version",
            adapter_version,
        )
    }

    async fn handler_for_request(
        &self,
        request: &ClientRequest,
        context: &RequestContext<RoleServer>,
    ) -> Result<JulieServerHandler, McpError> {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.as_ref() {
            return Ok(session.handler());
        }
        if !matches!(request, ClientRequest::InitializeRequest(_)) {
            return Err(McpError::invalid_request(
                "HTTP Julie session must initialize before handling requests",
                None,
            ));
        }

        self.admit_initialize(context)?;
        let startup_hint = workspace_startup_hint_from_context(context)?;
        let registration = self.register_session();
        let session = DaemonMcpSession::start(
            Arc::clone(&self.dependencies),
            registration.session_id.clone(),
            startup_hint,
            registration.session_lifecycle.clone(),
            "HTTP",
        )
        .await
        .map_err(|error| {
            self.remove_session_registration(&registration);
            McpError::invalid_params(error.to_string(), None)
        })?;
        let handler = session.handler();
        *self.session_registration.lock().await = Some(registration);
        *guard = Some(session);
        Ok(handler)
    }
}

impl Drop for HttpJulieService {
    // Cleanup invariant:
    //
    // The session must remain in `SessionTracker` until async DELETE cleanup
    // (DB commit, watcher pool detach, etc.) completes so drain accounting
    // stays honest while the daemon is shutting down. To preserve that, we
    // spawn `session.finish()` AND tracker removal together on the Tokio
    // runtime — pre-fix, tracker removal happened synchronously here while
    // `finish()` was awaited later, which could leave the daemon counting an
    // empty session as drained even though work was still in flight.
    //
    // Trade-off: if the runtime is torn down before the spawned task runs to
    // completion (panic, SIGKILL, abrupt drop), both `session.finish()` and
    // tracker removal are lost. In normal shutdown the runtime is kept alive
    // until drain completes, so this holds. The no-runtime branch below runs
    // tracker removal synchronously as a best-effort fallback.
    //
    // Lock contention: `try_lock()` on either Mutex is expected to succeed
    // because Drop runs while the last `Arc<HttpJulieService>` reference is
    // being released. If it fails, log loudly — the missing cleanup means a
    // tracker entry could linger and drain/restart accounting could stall.
    fn drop(&mut self) {
        let dependencies = Arc::clone(&self.dependencies);
        let registration = match self.session_registration.try_lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => {
                warn!(
                    "HTTP Julie session dropped while session_registration lock was held; \
                     registration cleanup skipped — drain accounting may stay stale"
                );
                None
            }
        };
        let session = match self.session.try_lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => {
                warn!(
                    "HTTP Julie session dropped while session lock was held; \
                     async session cleanup skipped"
                );
                None
            }
        };
        if registration.is_none() && session.is_none() {
            return;
        }

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Some(session) = session {
                    session.finish().await;
                }
                if let Some(registration) = registration {
                    HttpJulieService::remove_session_registration_for(&dependencies, &registration);
                }
            });
        } else {
            if let Some(registration) = &registration {
                HttpJulieService::remove_session_registration_for(&dependencies, registration);
            }
            if session.is_some() {
                warn!("HTTP Julie session dropped without a Tokio runtime; async cleanup skipped");
            }
        }
    }
}

impl Service<RoleServer> for HttpJulieService {
    async fn handle_request(
        &self,
        request: ClientRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<ServerResult, McpError> {
        let handler = self.handler_for_request(&request, &context).await?;
        handler.handle_request(request, context).await
    }

    async fn handle_notification(
        &self,
        notification: ClientNotification,
        context: NotificationContext<RoleServer>,
    ) -> Result<(), McpError> {
        let handler = {
            let guard = self.session.lock().await;
            guard.as_ref().map(DaemonMcpSession::handler)
        };
        let Some(handler) = handler else {
            return Err(McpError::invalid_request(
                "HTTP Julie session received a notification before initialize",
                None,
            ));
        };
        handler.handle_notification(notification, context).await
    }

    fn get_info(&self) -> ServerInfo {
        let server_info = Implementation::new("Julie", env!("CARGO_PKG_VERSION"))
            .with_title("Julie - Code Intelligence Server");
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(server_info)
    }
}

#[allow(dead_code)]
fn workspace_startup_hint_from_context(
    context: &RequestContext<RoleServer>,
) -> Result<WorkspaceStartupHint, McpError> {
    let parts = context.extensions.get::<Parts>().ok_or_else(|| {
        McpError::invalid_params("HTTP Julie session is missing request metadata", None)
    })?;
    workspace_startup_hint_from_parts(parts)
}

#[allow(dead_code)]
fn workspace_startup_hint_from_parts(parts: &Parts) -> Result<WorkspaceStartupHint, McpError> {
    let workspace = header_value(parts, HEADER_JULIE_WORKSPACE).ok_or_else(|| {
        McpError::invalid_params(
            format!("Missing required {HEADER_JULIE_WORKSPACE} header"),
            None,
        )
    })?;
    if workspace.trim().is_empty() {
        return Err(McpError::invalid_params(
            format!("{HEADER_JULIE_WORKSPACE} header must not be empty"),
            None,
        ));
    }

    let source = header_value(parts, HEADER_JULIE_WORKSPACE_SOURCE)
        .map(|value| {
            WorkspaceStartupSource::from_header_value(value).ok_or_else(|| {
                McpError::invalid_params(
                    format!("Invalid {HEADER_JULIE_WORKSPACE_SOURCE} header: {value}"),
                    None,
                )
            })
        })
        .transpose()?;

    Ok(WorkspaceStartupHint {
        path: PathBuf::from(workspace),
        source,
    })
}

#[allow(dead_code)]
fn header_value<'a>(parts: &'a Parts, name: &str) -> Option<&'a str> {
    parts.headers.get(name)?.to_str().ok()
}
