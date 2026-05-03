use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock as StdRwLock};

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
use crate::daemon::ipc_session::workspace_ids_to_disconnect;
use crate::daemon::session::{SessionLifecycleHandle, SessionTracker};
use crate::daemon::watcher_pool::WatcherPool;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::daemon::workspace_session_attachment::WorkspaceSessionAttachment;
use crate::dashboard::state::DashboardEvent;
use crate::handler::JulieServerHandler;
use crate::handler::session_workspace::SessionWorkspaceState;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

#[allow(dead_code)]
pub(crate) const HEADER_JULIE_WORKSPACE: &str = "x-julie-workspace";
#[allow(dead_code)]
pub(crate) const HEADER_JULIE_WORKSPACE_SOURCE: &str = "x-julie-workspace-source";
#[allow(dead_code)]
pub(crate) const HEADER_JULIE_VERSION: &str = "x-julie-version";

#[derive(Clone)]
pub(crate) struct DaemonSessionDependencies {
    pool: Arc<WorkspacePool>,
    daemon_db: Option<Arc<DaemonDatabase>>,
    embedding_service: Arc<EmbeddingService>,
    restart_pending: Arc<AtomicBool>,
    dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    watcher_pool: Option<Arc<WatcherPool>>,
    #[allow(dead_code)]
    sessions: Option<Arc<SessionTracker>>,
}

impl DaemonSessionDependencies {
    #[allow(dead_code)]
    pub(crate) fn new(
        pool: Arc<WorkspacePool>,
        daemon_db: Option<Arc<DaemonDatabase>>,
        embedding_service: Arc<EmbeddingService>,
        restart_pending: Arc<AtomicBool>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
        watcher_pool: Option<Arc<WatcherPool>>,
        sessions: Arc<SessionTracker>,
    ) -> Self {
        Self {
            pool,
            daemon_db,
            embedding_service,
            restart_pending,
            dashboard_tx,
            watcher_pool,
            sessions: Some(sessions),
        }
    }

    pub(crate) fn without_session_tracker(
        pool: Arc<WorkspacePool>,
        daemon_db: Option<Arc<DaemonDatabase>>,
        embedding_service: Arc<EmbeddingService>,
        restart_pending: Arc<AtomicBool>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
        watcher_pool: Option<Arc<WatcherPool>>,
    ) -> Self {
        Self {
            pool,
            daemon_db,
            embedding_service,
            restart_pending,
            dashboard_tx,
            watcher_pool,
            sessions: None,
        }
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

#[allow(dead_code)]
pub(crate) struct HttpJulieService {
    dependencies: Arc<DaemonSessionDependencies>,
    session_id: String,
    session_lifecycle: Option<SessionLifecycleHandle>,
    session: tokio::sync::Mutex<Option<DaemonMcpSession>>,
}

#[allow(dead_code)]
impl HttpJulieService {
    pub(crate) fn new(dependencies: Arc<DaemonSessionDependencies>) -> Self {
        let (session_id, session_lifecycle) = dependencies
            .sessions
            .as_ref()
            .map(|sessions| {
                let session_id = sessions.add_session();
                let lifecycle = sessions.lifecycle_handle(&session_id);
                if let Some(tx) = &dependencies.dashboard_tx {
                    let _ = tx.send(DashboardEvent::SessionChange {
                        active_count: sessions.active_count(),
                    });
                }
                (session_id, Some(lifecycle))
            })
            .unwrap_or_else(|| (uuid::Uuid::new_v4().to_string(), None));

        Self {
            dependencies,
            session_id,
            session_lifecycle,
            session: tokio::sync::Mutex::new(None),
        }
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

        let startup_hint = workspace_startup_hint_from_context(context)?;
        let session = DaemonMcpSession::start(
            Arc::clone(&self.dependencies),
            self.session_id.clone(),
            startup_hint,
            self.session_lifecycle.clone(),
            "HTTP",
        )
        .await
        .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let handler = session.handler();
        *guard = Some(session);
        Ok(handler)
    }
}

impl Drop for HttpJulieService {
    fn drop(&mut self) {
        if let Some(sessions) = &self.dependencies.sessions {
            sessions.remove_session(&self.session_id);
            if let Some(tx) = &self.dependencies.dashboard_tx {
                let _ = tx.send(DashboardEvent::SessionChange {
                    active_count: sessions.active_count(),
                });
            }
        }

        let session = self
            .session
            .try_lock()
            .ok()
            .and_then(|mut guard| guard.take());
        let Some(session) = session else {
            return;
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                session.finish().await;
            });
        } else {
            warn!("HTTP Julie session dropped without a Tokio runtime; async cleanup skipped");
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
