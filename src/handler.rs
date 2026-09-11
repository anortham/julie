#[path = "handler/embedding_init.rs"]
pub(crate) mod embedding_init;
#[path = "handler/mcp_adapter.rs"]
pub(crate) mod mcp_adapter;
#[path = "handler/search_telemetry.rs"]
pub(crate) mod search_telemetry;
pub mod session_workspace;
#[path = "handler/tool_context_impl.rs"]
mod tool_context_impl;
#[path = "handler/tool_metrics.rs"]
pub(crate) mod tool_metrics;
#[path = "handler/tool_targets.rs"]
pub(crate) mod tool_targets;
pub(crate) mod tools;
#[path = "handler/workspace_resolution.rs"]
pub(crate) mod workspace_resolution;

use std::collections::HashMap;

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::tool::ToolRouter,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, InitializeRequestParams,
        ListToolsResult, ServerInfo, Tool,
    },
    service::{NotificationContext, RequestContext},
};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::dashboard::state::DashboardEvent;

use self::session_workspace::{PrimaryWorkspaceBinding, SessionWorkspaceState};
use crate::workspace::JulieWorkspace;
use crate::workspace::mutation_gate::{MutationGuard, acquire_gate};
use crate::workspace::startup_hint::WorkspaceStartupHint;
use crate::workspace::startup_hint::WorkspaceStartupSource;
use julie_index::checkout_store::CheckoutStore;
use tokio::sync::RwLock;

use self::tool_metrics::{MetricsTask, run_metrics_writer, source_bytes_for_paths};
use crate::tools::metrics::session::{SessionMetrics, extract_source_paths};
use crate::tools::workspace::commands::ManageWorkspaceOperation;
use crate::tools::workspace::indexing::store_open::open_or_recreate;

pub(crate) struct PrimaryWorkspaceSnapshot {
    pub binding: PrimaryWorkspaceBinding,
    pub store: Arc<CheckoutStore>,
    pub indexing_runtime: Option<crate::tools::workspace::indexing::state::SharedIndexingRuntime>,
}

// Production callers were removed with the WorkspacePool metrics fast-path in
// Phase 3d.2b (the in-process metrics writer resolves the DB via `ws.db`
// directly). Retained as test-only path-layout coverage.
#[cfg(test)]
pub(crate) fn metrics_db_path_for_workspace(
    index_root_override: Option<&std::path::Path>,
    current_workspace_root: &std::path::Path,
    workspace_id: &str,
) -> PathBuf {
    if let Some(override_root) = index_root_override {
        override_root
            .parent()
            .map(|shared_indexes| shared_indexes.join(workspace_id).join("facts.sqlite"))
            .unwrap_or_else(|| {
                current_workspace_root
                    .join(".julie")
                    .join("indexes")
                    .join(workspace_id)
                    .join("facts.sqlite")
            })
    } else {
        current_workspace_root
            .join(".julie")
            .join("indexes")
            .join(workspace_id)
            .join("facts.sqlite")
    }
}

/// Tracks which indexes are ready for search operations
#[derive(Debug)]
pub struct IndexingStatus {
    /// Search system (Tantivy) is ready
    pub search_ready: AtomicBool,
    /// Semantic embeddings are ready
    pub embeddings_ready: AtomicBool,
}

impl IndexingStatus {
    /// Create new indexing status with all indexes not ready
    pub fn new() -> Self {
        Self {
            search_ready: AtomicBool::new(false),
            embeddings_ready: AtomicBool::new(false),
        }
    }
}

impl Default for IndexingStatus {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// JulieServerHandler
// ---------------------------------------------------------------------------

/// Julie's custom handler for MCP messages
///
/// This handler manages the core Julie functionality including:
/// - Code intelligence operations (search, navigation, extraction)
/// - Symbol database management
/// - Cross-language relationship detection
#[derive(Clone)]
pub struct JulieServerHandler {
    /// Session-owned workspace state. This is the mutable source of truth for
    /// startup hint, root tracking, primary binding, and secondary activations.
    pub(crate) session_workspace: Arc<StdRwLock<SessionWorkspaceState>>,
    /// Workspace managing persistent storage
    pub workspace: Arc<RwLock<Option<JulieWorkspace>>>,
    /// Flag to track if workspace has been indexed
    pub is_indexed: Arc<RwLock<bool>>,
    /// Tracks which indexes are ready for search operations
    pub indexing_status: Arc<IndexingStatus>,
    /// Per-session operational metrics (tool call timing, output sizes)
    pub session_metrics: Arc<SessionMetrics>,
    /// Per-workspace embedding pipeline: cancellation flag + task handle.
    /// Keyed by workspace_id so concurrent workspaces don't cancel each other.
    pub(crate) embedding_tasks: Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<
                String,
                (
                    Arc<std::sync::atomic::AtomicBool>,
                    tokio::task::JoinHandle<()>,
                ),
            >,
        >,
    >,
    /// rmcp tool router for handling tool calls
    tool_router: ToolRouter<Self>,
    /// Per-project log for daemon mode (writes to {project}/.julie/logs/).
    /// None in stdio mode (tracing handles project logging directly).
    pub(crate) project_log: Option<Arc<crate::registry::project_log::ProjectLog>>,
    /// Daemon-level database for persistent metrics and workspace registry.
    /// None in stdio mode, Some in daemon mode.
    pub(crate) daemon_db: Option<Arc<crate::registry::database::DaemonDatabase>>,
    /// Workspace ID for the workspace currently stored in `handler.workspace`.
    /// Keep this separate from `current_workspace_id()`, which reads session-owned
    /// mutable state and may diverge during rebinding.
    pub(crate) workspace_id: Arc<StdRwLock<Option<String>>>,
    /// Certification/replay handlers can index external repos without writing
    /// helper files such as `.julieignore` into those repos.
    pub(crate) suppress_workspace_file_writes: Arc<AtomicBool>,
    /// Bounded channel sender for background metrics writes (M03).
    /// A single background task drains this; try_send drops on backpressure
    /// rather than spawning unbounded tasks.
    metrics_tx: tokio::sync::mpsc::Sender<MetricsTask>,
    /// Checkout stores opened for non-primary workspaces, keyed by workspace_id.
    ref_store_cache: Arc<RwLock<HashMap<String, Arc<CheckoutStore>>>>,
    /// Broadcast sender for dashboard live-feed events. None in stdio/test mode.
    dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    /// True when this handler was built by an in-process constructor; gates the
    /// bounded read envelope in `call_tool`.
    pub(crate) in_process: bool,
    /// Embedding provider injected by `new_in_process`. When `Some`, takes
    /// priority over the per-workspace provider.
    injected_embedding_provider:
        Arc<std::sync::RwLock<Option<Arc<dyn crate::embeddings::EmbeddingProvider>>>>,
    /// When true, semantics are explicitly disabled for the current request context.
    pub(crate) semantics_disabled: Arc<std::sync::atomic::AtomicBool>,
    /// Shared semantic runtime managing embedding providers and readiness across checkouts.
    pub(crate) semantic_runtime:
        Arc<std::sync::RwLock<Arc<dyn crate::request_engine::semantic::SemanticRuntime>>>,

    /// Index root override for in-process sessions.  When `Some`, the
    /// non-pool branch of `initialize_workspace_with_force` routes db/tantivy
    /// to the shared directory (`~/.julie/indexes/{ws}/`).
    pub(crate) in_process_index_root: Option<PathBuf>,
    /// Keeps isolated temp roots alive for test-only handlers.
    #[cfg(test)]
    test_temp_guard: Option<Arc<tempfile::TempDir>>,
}

impl JulieServerHandler {
    pub(crate) fn canonicalize_workspace_path(path: PathBuf) -> PathBuf {
        path.canonicalize().unwrap_or(path)
    }

    fn reject_sensitive_cwd_startup_hint(&self) -> Result<()> {
        let startup_hint = self.workspace_startup_hint();
        if !matches!(startup_hint.source, Some(WorkspaceStartupSource::Cwd)) {
            return Ok(());
        }
        crate::workspace::root_safety::reject_sensitive_cwd_workspace_root(&startup_hint.path)
    }

    fn primary_binding_for_root(&self, workspace_root: PathBuf) -> Result<PrimaryWorkspaceBinding> {
        let workspace_root = Self::canonicalize_workspace_path(workspace_root);
        crate::workspace::root_safety::reject_sensitive_workspace_root(&workspace_root)?;
        if !workspace_root.is_dir() {
            return Err(anyhow::anyhow!(
                "workspace root is not a directory: {}",
                workspace_root.display()
            ));
        }
        let workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace_root.to_string_lossy())?;
        Ok(PrimaryWorkspaceBinding {
            workspace_id,
            workspace_root,
        })
    }

    fn activate_primary_binding(&self, binding: &PrimaryWorkspaceBinding) {
        self.bind_current_primary(binding.workspace_id.clone(), binding.workspace_root.clone());
    }

    fn register_workspace_row(
        &self,
        workspace_id: &str,
        workspace_root: &std::path::Path,
    ) -> Result<()> {
        if let Some(db) = self.daemon_db.as_deref() {
            db.upsert_workspace(workspace_id, &workspace_root.to_string_lossy(), "ready")?;
        }
        Ok(())
    }

    /// Bind the startup hint as the primary the first time a primary-scoped
    /// request arrives on a handler that was constructed unbound.
    async fn ensure_primary_workspace_for_request(&self) -> Result<()> {
        if self.require_primary_binding().is_ok() {
            return Ok(());
        }
        self.reject_sensitive_cwd_startup_hint()?;
        let binding = self.primary_binding_for_root(self.workspace_startup_hint().path)?;
        self.register_workspace_row(&binding.workspace_id, &binding.workspace_root)?;
        self.activate_primary_binding(&binding);
        Ok(())
    }

    fn manage_workspace_request_targets_primary(
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> bool {
        ManageWorkspaceOperation::request_targets_primary(arguments)
    }

    fn tool_request_targets_primary(
        tool_name: &str,
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> bool {
        let workspace_is_primary = arguments
            .and_then(|args| args.get("workspace"))
            .and_then(serde_json::Value::as_str)
            .is_none_or(|workspace| workspace == "primary");

        match tool_name {
            "fast_search" | "fast_refs" | "call_path" | "get_symbols" | "deep_dive"
            | "get_context" | "blast_radius" | "edit_file" => workspace_is_primary,
            "manage_workspace" => Self::manage_workspace_request_targets_primary(arguments),
            _ => false,
        }
    }

    async fn teardown_loaded_workspace(&self) {
        let mut workspace_guard = self.workspace.write().await;
        if let Some(ref mut old_workspace) = *workspace_guard {
            info!("Tearing down loaded workspace before replacement");
            if let Err(e) = old_workspace.stop_file_watching().await {
                warn!("Failed to stop file watching during teardown: {}", e);
            }
        }
        *workspace_guard = None;
        self.set_loaded_workspace_id(None);
    }

    /// Create a new Julie server handler with all components initialized.
    ///
    /// `workspace_root` is the resolved root path for this server session,
    /// determined by the caller (main.rs) via CLI args / env var / cwd.
    pub async fn new(workspace_root: PathBuf) -> Result<Self> {
        info!(
            "Initializing Julie server handler (workspace_root: {:?})",
            workspace_root
        );

        let (metrics_tx, metrics_rx) = tokio::sync::mpsc::channel::<MetricsTask>(512);
        tokio::spawn(run_metrics_writer(metrics_rx));

        let workspace_startup_hint = WorkspaceStartupHint {
            path: workspace_root.clone(),
            source: None,
        };
        let session_workspace = SessionWorkspaceState::new(workspace_startup_hint);

        Ok(Self {
            session_workspace: Arc::new(StdRwLock::new(session_workspace)),
            workspace: Arc::new(RwLock::new(None)),
            is_indexed: Arc::new(RwLock::new(false)),
            indexing_status: Arc::new(IndexingStatus::new()),
            session_metrics: Arc::new(SessionMetrics::new()),
            embedding_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            tool_router: Self::tool_router(),
            project_log: None,
            daemon_db: None,
            workspace_id: Arc::new(StdRwLock::new(None)),
            suppress_workspace_file_writes: Arc::new(AtomicBool::new(false)),
            metrics_tx,
            ref_store_cache: Arc::new(RwLock::new(HashMap::new())),
            dashboard_tx: None,
            in_process: false,
            injected_embedding_provider: Arc::new(std::sync::RwLock::new(None)),
            semantics_disabled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            semantic_runtime: Arc::new(std::sync::RwLock::new(Arc::new(
                crate::request_engine::semantic::NoopSemanticRuntime,
            ))),
            in_process_index_root: None,

            #[cfg(test)]
            test_temp_guard: None,
        })
    }

    /// Create a handler for an in-process follower session backed by a shared workspace.
    ///
    /// Each handler gets its own `session_metrics` and `indexing_status` (per-session),
    /// but the workspace's expensive resources (db, search_index) are shared across
    /// sessions via their inner `Arc<Mutex<...>>` pointers.
    ///
    /// Clone semantics of JulieWorkspace:
    /// - `db: Arc<Mutex<SqliteDB>>` and `search_index: Arc<SearchIndex>` are
    ///   shared (Arc clone). This is the whole point: multiple sessions hit one db.
    /// - `watcher` is `None` in the clone (leader manages the file watcher).
    /// - `embedding_provider` is set to `None`.
    pub async fn new_with_shared_workspace(
        workspace: Arc<JulieWorkspace>,
        workspace_root: PathBuf,
        daemon_db: Option<Arc<crate::registry::database::DaemonDatabase>>,
        workspace_id: Option<String>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    ) -> Result<Self> {
        Self::new_with_shared_workspace_startup_hint(
            workspace,
            WorkspaceStartupHint {
                path: workspace_root,
                source: None,
            },
            daemon_db,
            workspace_id,
            dashboard_tx,
        )
        .await
    }

    pub async fn new_with_shared_workspace_startup_hint(
        workspace: Arc<JulieWorkspace>,
        workspace_startup_hint: WorkspaceStartupHint,
        daemon_db: Option<Arc<crate::registry::database::DaemonDatabase>>,
        workspace_id: Option<String>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    ) -> Result<Self> {
        let workspace_root = workspace_startup_hint.path.clone();
        info!(
            "Creating daemon-mode handler (workspace_root: {:?})",
            workspace_root
        );

        // Clone the workspace out of the Arc. This shares db/search_index via
        // their inner Arcs, which is correct for multi-session sharing.
        let mut ws_clone = (*workspace).clone();
        // Daemon manages embeddings separately (Phase 3).
        ws_clone.embedding_provider = None;

        let already_indexed = ws_clone.store.status().graph.symbols > 0;

        let mut session_workspace = SessionWorkspaceState::new(workspace_startup_hint.clone());
        let initial_workspace_id = workspace_id.clone();
        if let Some(ref id) = workspace_id {
            session_workspace.bind_primary(id.clone(), workspace_root.clone());
        }

        // Create per-project logger for daemon mode
        let project_log = Some(Arc::new(crate::registry::project_log::ProjectLog::new(
            &workspace_root,
        )));

        let (metrics_tx, metrics_rx) = tokio::sync::mpsc::channel::<MetricsTask>(512);
        tokio::spawn(run_metrics_writer(metrics_rx));

        let handler = Self {
            session_workspace: Arc::new(StdRwLock::new(session_workspace)),
            workspace: Arc::new(RwLock::new(Some(ws_clone))),
            is_indexed: Arc::new(RwLock::new(already_indexed)),
            indexing_status: Arc::new(IndexingStatus::new()),
            session_metrics: Arc::new(SessionMetrics::new()),
            embedding_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            tool_router: Self::tool_router(),
            project_log,
            daemon_db,
            workspace_id: Arc::new(StdRwLock::new(workspace_id)),
            suppress_workspace_file_writes: Arc::new(AtomicBool::new(false)),
            metrics_tx,
            ref_store_cache: Arc::new(RwLock::new(HashMap::new())),
            dashboard_tx,
            in_process: false,
            injected_embedding_provider: Arc::new(std::sync::RwLock::new(None)),
            semantics_disabled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            semantic_runtime: Arc::new(std::sync::RwLock::new(Arc::new(
                crate::request_engine::semantic::NoopSemanticRuntime,
            ))),
            in_process_index_root: None,

            #[cfg(test)]
            test_temp_guard: None,
        };

        if let Some(id) = initial_workspace_id {
            handler.register_workspace_row(&id, &workspace_root)?;
        }

        Ok(handler)
    }

    pub async fn new_deferred_daemon_startup_hint(
        workspace_startup_hint: WorkspaceStartupHint,
        daemon_db: Option<Arc<crate::registry::database::DaemonDatabase>>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    ) -> Result<Self> {
        Self::new_deferred_daemon_startup_hint_with_project_log(
            workspace_startup_hint,
            daemon_db,
            dashboard_tx,
            true,
        )
        .await
    }

    pub async fn new_deferred_daemon_startup_hint_without_project_log(
        workspace_startup_hint: WorkspaceStartupHint,
        daemon_db: Option<Arc<crate::registry::database::DaemonDatabase>>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    ) -> Result<Self> {
        Self::new_deferred_daemon_startup_hint_with_project_log(
            workspace_startup_hint,
            daemon_db,
            dashboard_tx,
            false,
        )
        .await
    }

    async fn new_deferred_daemon_startup_hint_with_project_log(
        workspace_startup_hint: WorkspaceStartupHint,
        daemon_db: Option<Arc<crate::registry::database::DaemonDatabase>>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
        enable_project_writes: bool,
    ) -> Result<Self> {
        let workspace_root = workspace_startup_hint.path.clone();
        info!(
            "Creating deferred daemon-mode handler (workspace_root: {:?})",
            workspace_root
        );

        let (metrics_tx, metrics_rx) = tokio::sync::mpsc::channel::<MetricsTask>(512);
        tokio::spawn(run_metrics_writer(metrics_rx));

        Ok(Self {
            session_workspace: Arc::new(StdRwLock::new(SessionWorkspaceState::new(
                workspace_startup_hint,
            ))),
            workspace: Arc::new(RwLock::new(None)),
            is_indexed: Arc::new(RwLock::new(false)),
            indexing_status: Arc::new(IndexingStatus::new()),
            session_metrics: Arc::new(SessionMetrics::new()),
            embedding_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            tool_router: Self::tool_router(),
            project_log: if enable_project_writes {
                Some(Arc::new(crate::registry::project_log::ProjectLog::new(
                    &workspace_root,
                )))
            } else {
                None
            },
            daemon_db,
            workspace_id: Arc::new(StdRwLock::new(None)),
            suppress_workspace_file_writes: Arc::new(AtomicBool::new(!enable_project_writes)),
            metrics_tx,
            ref_store_cache: Arc::new(RwLock::new(HashMap::new())),
            dashboard_tx,
            in_process: false,
            injected_embedding_provider: Arc::new(std::sync::RwLock::new(None)),
            semantics_disabled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            semantic_runtime: Arc::new(std::sync::RwLock::new(Arc::new(
                crate::request_engine::semantic::NoopSemanticRuntime,
            ))),
            in_process_index_root: None,

            #[cfg(test)]
            test_temp_guard: None,
        })
    }

    /// Create a handler for the in-process MCP server path.
    ///
    /// Unlike the daemon constructors, `new_in_process` preserves the full
    /// `startup_hint.source` so that `on_initialized` can correctly defer
    /// auto-indexing when the source is `Cwd` (IDE-injected path) and index
    /// eagerly when the source is `Cli` or `Env`.
    ///
    /// # Parameters
    /// * `startup_hint` — workspace root and source, passed through unchanged.
    /// * `embedding_provider` — optional pre-constructed provider.  When `Some`,
    ///   `embedding_provider()` returns it directly, bypassing both the daemon
    ///   embedding-service and the per-workspace sidecar path. Do **not** call
    ///   `mark_standalone_embedding_skipped` when passing a provider here.
    /// * `index_root` — when `Some`, `initialize_workspace_with_force` routes
    ///   db/tantivy to this directory.  Pass `None` for the traditional
    ///   project-local path.
    pub async fn new_in_process(
        startup_hint: WorkspaceStartupHint,
        embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
        index_root: Option<PathBuf>,
    ) -> Result<Self> {
        Self::new_in_process_with_daemon_db(startup_hint, embedding_provider, index_root, None)
            .await
    }

    pub async fn new_in_process_with_daemon_db(
        startup_hint: WorkspaceStartupHint,
        embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
        index_root: Option<PathBuf>,
        daemon_db: Option<Arc<crate::registry::database::DaemonDatabase>>,
    ) -> Result<Self> {
        // Build on top of the deferred-startup path, which wires up everything
        // except the project-log and sets startup_hint on the session state.
        let mut handler = Self::new_deferred_daemon_startup_hint_with_project_log(
            startup_hint,
            daemon_db,
            /*dashboard_tx=*/ None,
            /*enable_project_writes=*/ true,
        )
        .await?;

        handler.in_process = true;
        handler.injected_embedding_provider = Arc::new(std::sync::RwLock::new(embedding_provider));
        handler.in_process_index_root = index_root;

        Ok(handler)
    }

    /// Test-only convenience: create a handler rooted in an isolated temp dir.
    ///
    /// Uses an isolated temp root so tests do not spray `.julie/indexes` under
    /// the repo checkout.
    #[cfg(test)]
    pub async fn new_for_test() -> Result<Self> {
        let temp_root = Arc::new(
            tempfile::Builder::new()
                .prefix("julie_handler_root_")
                .tempdir()?,
        );
        let mut handler = Self::new(temp_root.path().to_path_buf()).await?;
        handler.test_temp_guard = Some(temp_root);
        Ok(handler)
    }

    fn update_session_workspace<R>(
        &self,
        update: impl FnOnce(&mut SessionWorkspaceState) -> R,
    ) -> R {
        let mut state = self
            .session_workspace
            .write()
            .unwrap_or_else(|p| p.into_inner());
        update(&mut state)
    }

    /// Returns `true` when this handler was built by an in-process constructor.
    ///
    /// Gates the F1 bounded read envelope in `call_tool`: only in-process
    /// handlers get the bounded envelope; daemon/stdio take the existing
    /// path byte-for-byte unchanged.
    pub fn is_in_process(&self) -> bool {
        self.in_process
    }

    pub fn workspace_startup_hint(&self) -> WorkspaceStartupHint {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .startup_hint
            .clone()
    }

    pub fn current_workspace_root(&self) -> PathBuf {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .current_workspace_root()
    }

    pub fn current_workspace_id(&self) -> Option<String> {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .current_workspace_id()
    }

    /// Serialize every writer for `workspace_id` through the in-process mutation gate.
    pub(crate) async fn acquire_mutation_guard(
        &self,
        workspace_id: &str,
    ) -> MutationGuard<'static> {
        acquire_gate(workspace_id).await
    }

    fn require_primary_binding(&self) -> Result<PrimaryWorkspaceBinding> {
        let session_workspace = self
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner());

        session_workspace.primary_binding().ok_or_else(|| {
            anyhow::anyhow!(
                "No workspace initialized. Run manage_workspace(operation=\"index\") first."
            )
        })
    }

    pub fn require_primary_workspace_identity(&self) -> Result<String> {
        Ok(self.require_primary_binding()?.workspace_id)
    }

    pub fn require_primary_workspace_binding(&self) -> Result<PrimaryWorkspaceBinding> {
        self.require_primary_binding()
    }

    pub fn require_primary_workspace_root(&self) -> Result<PathBuf> {
        Ok(self.require_primary_binding()?.workspace_root)
    }

    pub fn loaded_workspace_id(&self) -> Option<String> {
        self.workspace_id
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn set_loaded_workspace_id(&self, workspace_id: Option<String>) {
        *self.workspace_id.write().unwrap_or_else(|p| p.into_inner()) = workspace_id;
    }

    fn bind_current_primary(&self, workspace_id: impl Into<String>, workspace_root: PathBuf) {
        let workspace_id = workspace_id.into();
        self.update_session_workspace(move |session_workspace| {
            session_workspace.bind_primary(workspace_id, workspace_root);
        });
    }

    #[cfg(test)]
    pub fn set_current_primary_binding(
        &self,
        workspace_id: impl Into<String>,
        workspace_root: PathBuf,
    ) {
        self.bind_current_primary(workspace_id, workspace_root);
    }

    #[cfg(test)]
    pub async fn loaded_workspace_file_watcher_running_for_test(&self) -> bool {
        let workspace_guard = self.workspace.read().await;
        workspace_guard
            .as_ref()
            .and_then(|workspace| workspace.watcher.as_ref())
            .is_some_and(|watcher| watcher.is_running())
    }

    #[cfg(test)]
    pub async fn stop_loaded_workspace_file_watching_for_test(&self) -> Result<()> {
        let mut workspace_guard = self.workspace.write().await;
        if let Some(workspace) = workspace_guard.as_mut() {
            workspace.stop_file_watching().await?;
        }
        Ok(())
    }

    /// Get the workspace root path for workspace operations.
    ///
    /// Returns the resolved workspace root that was passed to `new()`.
    /// This replaces the old `current_dir()` fallback, ensuring the handler
    /// always uses the path determined by main.rs (CLI > env var > cwd).
    fn get_workspace_path(&self) -> PathBuf {
        self.current_workspace_root()
    }

    /// Get the active embedding provider from the injected provider or shared semantic runtime.
    pub(crate) async fn embedding_provider(
        &self,
    ) -> Option<Arc<dyn crate::embeddings::EmbeddingProvider>> {
        if self
            .semantics_disabled
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return None;
        }
        // In-process mode: injected provider takes priority.
        if let Ok(guard) = self.injected_embedding_provider.read() {
            if let Some(ref p) = *guard {
                return Some(Arc::clone(p));
            }
        }
        if let Some(provider) = self.semantic_runtime().provider() {
            return Some(provider);
        }
        let ws = self.workspace.read().await;
        ws.as_ref().and_then(|ws| ws.embedding_provider.clone())
    }

    /// Access the shared semantic runtime.
    pub fn semantic_runtime(&self) -> Arc<dyn crate::request_engine::semantic::SemanticRuntime> {
        self.semantic_runtime.read().unwrap().clone()
    }

    /// Update the shared semantic runtime.
    pub fn set_semantic_runtime(
        &self,
        runtime: Arc<dyn crate::request_engine::semantic::SemanticRuntime>,
    ) {
        if let Ok(mut guard) = self.semantic_runtime.write() {
            *guard = runtime;
        }
    }

    /// Acquire the embedding provider through the shared semantic runtime, waiting up to `timeout`.
    pub async fn acquire_embedding_provider(
        &self,
        timeout: Duration,
    ) -> Option<Arc<dyn crate::embeddings::EmbeddingProvider>> {
        if let Some(provider) = self.embedding_provider().await {
            return Some(provider);
        }
        let root = self.current_workspace_root();
        let id = self.current_workspace_id().unwrap_or_else(|| {
            crate::workspace::registry::generate_workspace_id(&root.to_string_lossy())
                .unwrap_or_default()
        });
        let index_root = self
            .in_process_index_root
            .clone()
            .unwrap_or_else(|| crate::paths::RegistryPaths::default().workspace_index_dir(&id));
        let binding = crate::request_engine::types::WorkspaceBinding {
            workspace_id: id,
            root,
            index_root,
        };
        let deadline = tokio::time::Instant::now() + timeout;
        let cancel = tokio_util::sync::CancellationToken::new();
        let readiness = self
            .semantic_runtime()
            .ensure_ready(
                &binding,
                crate::request_engine::semantic::SemanticRequirement::Query,
                crate::request_engine::semantic::SemanticMode::Auto,
                deadline,
                &cancel,
            )
            .await;
        let provider = self.semantic_runtime().provider();
        let runtime_status = match &provider {
            Some(p) => {
                let dev_info = p.device_info();
                let accelerated = p.accelerated().unwrap_or_else(|| dev_info.is_accelerated());
                crate::embeddings::EmbeddingRuntimeStatus {
                    requested_backend: crate::embeddings::EmbeddingBackend::Native,
                    resolved_backend: crate::embeddings::EmbeddingBackend::Native,
                    accelerated,
                    degraded_reason: p.degraded_reason(),
                }
            }
            None => {
                let reason = match &readiness {
                    Ok(crate::request_engine::semantic::SemanticReadiness::Degraded {
                        reason,
                        ..
                    }) => Some(reason.clone()),
                    Ok(crate::request_engine::semantic::SemanticReadiness::Disabled) => {
                        Some("disabled by environment".to_string())
                    }
                    Ok(crate::request_engine::semantic::SemanticReadiness::Starting) => {
                        Some("starting".to_string())
                    }
                    Err(failure) => Some(failure.message.clone()),
                    _ => Some("embedding provider unavailable".to_string()),
                };
                crate::embeddings::EmbeddingRuntimeStatus {
                    requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                    resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                    accelerated: false,
                    degraded_reason: reason,
                }
            }
        };
        let mut ws_guard = self.workspace.write().await;
        if let Some(ref mut ws) = *ws_guard {
            ws.embedding_provider = provider.clone();
            ws.embedding_runtime_status = Some(runtime_status);
            if let Some(ref p) = provider {
                if let Some(ref watcher) = ws.watcher {
                    watcher.update_embedding_provider(Some(Arc::clone(p)));
                }
            }
        }
        provider
    }

    /// Dynamically inject or clear the in-process embedding provider.
    pub fn set_injected_embedding_provider(
        &self,
        provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
    ) {
        if let Ok(mut guard) = self.injected_embedding_provider.write() {
            *guard = provider;
        }
    }

    /// Get the per-workspace embedding runtime status.
    pub(crate) async fn embedding_runtime_status(
        &self,
    ) -> Option<crate::embeddings::EmbeddingRuntimeStatus> {
        let ws = self.workspace.read().await;
        ws.as_ref()
            .and_then(|ws| ws.embedding_runtime_status.clone())
    }

    /// Mark embedding initialization as skipped for standalone (single-shot CLI) mode.
    ///
    /// Standalone mode is a single-shot CLI invocation. Launching the Python
    /// embedding sidecar to serve one query costs 8-10s and is torn down
    /// immediately after. The keyword search path is the correct degraded mode.
    ///
    /// Setting `embedding_runtime_status` to `Some(...)` satisfies the guard
    /// in `maybe_initialize_embeddings_for_nl_definitions`:
    ///   `if workspace.embedding_runtime_status.is_none() { ... probe ... }`
    /// so the 8-10s sidecar probe is never entered for NL queries.
    #[allow(dead_code)]
    pub(crate) async fn mark_standalone_embedding_skipped(&self) {
        let mut ws = self.workspace.write().await;
        if let Some(workspace) = ws.as_mut() {
            if workspace.embedding_runtime_status.is_none() {
                workspace.embedding_runtime_status =
                    Some(crate::embeddings::EmbeddingRuntimeStatus {
                        requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                        resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                        accelerated: false,
                        degraded_reason: Some(
                            "standalone mode: embedding sidecar skipped".to_string(),
                        ),
                    });
            }
        }
    }

    /// Initialize or load workspace and update components to use persistent storage
    pub async fn initialize_workspace(&self, workspace_path: Option<String>) -> Result<()> {
        self.initialize_workspace_with_force(workspace_path, false)
            .await
    }

    /// Initialize or load workspace with optional force reinitialization
    pub async fn initialize_workspace_with_force(
        &self,
        workspace_path: Option<String>,
        force: bool,
    ) -> Result<()> {
        debug!(
            "🔍 DEBUG: initialize_workspace_with_force called with workspace_path: {:?}, force: {}",
            workspace_path, force
        );
        let target_path = match workspace_path {
            Some(path) => {
                let expanded_path = shellexpand::tilde(&path).to_string();
                std::path::PathBuf::from(expanded_path)
            }
            None => self.get_workspace_path(),
        };

        info!("Initializing workspace at: {}", target_path.display());
        debug!(
            "🔍 DEBUG: target_path resolved to: {}",
            target_path.display()
        );

        let target_canonical = target_path
            .canonicalize()
            .unwrap_or_else(|_| target_path.clone());
        crate::workspace::root_safety::reject_sensitive_workspace_root(&target_canonical)?;

        // Handle force reinitialization vs normal initialization
        let workspace_result: Result<JulieWorkspace> = if force {
            info!("🔄 Force reinitialization requested - clearing derived data only");

            self.teardown_loaded_workspace().await;

            if let Some(index_root) = &self.in_process_index_root {
                if index_root.exists() {
                    if let Err(e) = std::fs::remove_dir_all(index_root) {
                        warn!(
                            "Failed to clear in-process index root for force reindex at {}: {}",
                            index_root.display(),
                            e
                        );
                    }
                }
                info!(
                    "🗑️ Cleared in-process index root for force reindex at {}",
                    index_root.display()
                );
                JulieWorkspace::initialize_with_index_root(target_path.clone(), index_root.clone())
                    .await
            } else {
                // For force reindex, we only clear derived data, NOT the database (source of truth)
                let julie_dir = target_path.join(".julie");
                if julie_dir.exists() {
                    info!(
                        "🗑️ Clearing search index and cache for force reindex (preserving database)"
                    );

                    // 🔴 CRITICAL FIX: Only clear the PRIMARY workspace's index, NOT all workspaces!
                    // Non-primary workspace indexes must be preserved during force reindex.

                    // Determine the primary workspace ID so we only clear its directory
                    use crate::workspace::registry::generate_workspace_id;
                    let workspace_path_str = target_path.to_string_lossy().to_string();

                    let primary_workspace_index_dir = match generate_workspace_id(
                        &workspace_path_str,
                    ) {
                        Ok(workspace_id) => Some(julie_dir.join("indexes").join(workspace_id)),
                        Err(e) => {
                            warn!(
                                "Failed to generate workspace ID: {} - will skip index clearing",
                                e
                            );
                            None
                        }
                    };

                    // Clear primary workspace's index directory (NOT the entire indexes/ directory)
                    if let Some(primary_index_dir) = primary_workspace_index_dir {
                        if primary_index_dir.exists() {
                            if let Err(e) = std::fs::remove_dir_all(&primary_index_dir) {
                                warn!(
                                    "Failed to clear primary workspace index {}: {}",
                                    primary_index_dir.display(),
                                    e
                                );
                            } else {
                                info!(
                                    "✅ Cleared primary workspace index: {}",
                                    primary_index_dir.display()
                                );
                                info!(
                                    "✅ Non-primary workspace indexes preserved (workspace isolation maintained)"
                                );
                            }
                        }
                    }

                    // Clear shared cache (applies to all workspaces, can be rebuilt)
                    let cache_path = julie_dir.join("cache");
                    if cache_path.exists() {
                        if let Err(e) = std::fs::remove_dir_all(&cache_path) {
                            warn!("Failed to clear cache {}: {}", cache_path.display(), e);
                        } else {
                            info!("Cleared shared cache: {}", cache_path.display());
                        }
                    }

                    // Database directory is explicitly preserved for incremental updates
                    let db_path = julie_dir.join("db");
                    if db_path.exists() {
                        info!(
                            "✅ Database preserved at: {} (contains source of truth)",
                            db_path.display()
                        );
                    }
                }

                // Initialize workspace (will reuse existing database if present)
                JulieWorkspace::initialize(target_path).await
            }
        } else {
            if let Some(index_root) = &self.in_process_index_root {
                // In-process mode (T8/F2): redirect db/tantivy to the shared daemon
                // index directory so the leader lock and workspace storage share one
                // inode tree (`~/.julie/indexes/{ws}/`).  This is the F2 hard gate —
                // without this branch, `initialize` would create project-local storage
                // while the leader lock sits in the daemon path, letting two processes
                // silently "lead" the same workspace.
                JulieWorkspace::initialize_with_index_root(target_path, index_root.clone()).await
            } else {
                // Try to load existing workspace first
                match JulieWorkspace::detect_and_load(target_path.clone()).await? {
                    Some(existing_workspace) => {
                        info!("Loaded existing workspace");
                        Ok(existing_workspace)
                    }
                    None => {
                        info!("Creating new workspace");
                        JulieWorkspace::initialize(target_path).await
                    }
                }
            }
        };

        let mut workspace: JulieWorkspace = workspace_result?;

        // Start file watching BEFORE storing workspace (to avoid clone issue).
        if let Err(e) = workspace.start_file_watching(true).await {
            warn!("Failed to start file watching: {}", e);
        }

        let workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace.root.to_string_lossy())
                .ok();
        let workspace_root = workspace.root.clone();
        *self.workspace.write().await = Some(workspace);
        self.set_loaded_workspace_id(workspace_id.clone());
        if let Some(workspace_id) = workspace_id {
            self.bind_current_primary(workspace_id.clone(), workspace_root.clone());
            if let Err(error) = self.register_workspace_row(&workspace_id, &workspace_root) {
                warn!(
                    workspace_id = %workspace_id,
                    "Failed to register loaded primary workspace in registry: {error}"
                );
            }
        }

        info!("Workspace initialization complete");
        Ok(())
    }

    /// Get workspace if initialized
    pub async fn get_workspace(&self) -> Result<Option<JulieWorkspace>> {
        let workspace_guard = self.workspace.read().await;
        Ok(workspace_guard.clone())
    }

    /// Ensure workspace is initialized for operations that require it
    pub async fn ensure_workspace(&self) -> Result<()> {
        if self.workspace.read().await.is_some() {
            return Ok(());
        }
        // Atomically claim the initialization slot to prevent concurrent double-init.
        // Mirrors the pattern in on_initialized: only the caller that transitions
        // is_indexed false→true proceeds with initialization.
        {
            let mut indexed = self.is_indexed.write().await;
            if *indexed {
                return Ok(());
            }
            *indexed = true;
        }
        self.initialize_workspace(None).await
    }

    /// Backfill vector_count and embedding_model in daemon.db if missing.
    /// Handles workspaces embedded before the daemon tracked these stats.
    async fn backfill_vector_count(&self) {
        let current_workspace_id = self.current_workspace_id();
        let (Some(db), Some(ws_id)) = (&self.daemon_db, current_workspace_id.as_deref()) else {
            return;
        };
        let row = match db.get_workspace(ws_id) {
            Ok(Some(row)) => row,
            _ => return,
        };
        let needs_vectors = row.vector_count.is_none();
        let needs_model = row.embedding_model.is_none();
        if !needs_vectors && !needs_model {
            return;
        }

        if needs_vectors {
            let ws_guard = self.workspace.read().await;
            if let Some(ws) = ws_guard.as_ref() {
                {
                    let store = &ws.store;
                    let count = store.status().vector_count as i64;
                    if count > 0 {
                        let _ = db.update_vector_count(ws_id, count);
                        info!(workspace_id = %ws_id, count, "Backfilled vector_count");
                    }
                }
            }
        }

        if needs_model {
            if let Some(provider) = self.embedding_provider().await {
                let model = provider.device_info().model_name;
                let _ = db.update_embedding_model(ws_id, &model);
                info!(workspace_id = %ws_id, model, "Backfilled embedding_model");
            }
        }
    }

    /// Build a metrics binding from a pre-resolved `WorkspaceTarget`.
    ///
    /// Tool wrappers call `resolve_workspace_filter` once and pass the typed
    /// result here, replacing the older string-compared
    /// `metrics_workspace_binding_for_workspace_param` (which both duplicated
    /// resolution and silently fell back to the current workspace root on lookup
    /// failure). Now the caller owns resolution; this helper only translates an
    /// already-resolved target into a `PrimaryWorkspaceBinding` snapshot for
    /// attribution.
    pub(crate) async fn metrics_workspace_binding_for_target(
        &self,
        target: &crate::tools::navigation::resolution::WorkspaceTarget,
    ) -> Option<PrimaryWorkspaceBinding> {
        use crate::tools::navigation::resolution::WorkspaceTarget;
        match target {
            WorkspaceTarget::Primary => self.require_primary_workspace_binding().ok(),
            WorkspaceTarget::Target(workspace_id) => {
                let workspace_root = self
                    .get_workspace_root_for_target(workspace_id)
                    .await
                    .unwrap_or_else(|_| self.current_workspace_root());
                Some(PrimaryWorkspaceBinding {
                    workspace_id: workspace_id.clone(),
                    workspace_root,
                })
            }
        }
    }

    pub(crate) async fn metrics_source_bytes_for_binding(
        &self,
        binding: Option<&PrimaryWorkspaceBinding>,
        source_file_paths: &[String],
    ) -> Option<u64> {
        if source_file_paths.is_empty() {
            return None;
        }
        let binding = binding?;
        let store = self
            .checkout_store_for_workspace(&binding.workspace_id, &binding.workspace_root)
            .await
            .ok()?;
        let root = binding.workspace_root.clone();
        let paths = source_file_paths.to_vec();
        tokio::task::spawn_blocking(move || source_bytes_for_paths(&store, &root, &paths))
            .await
            .ok()
            .flatten()
    }

    /// Extract output byte count from a CallToolResult.
    pub(crate) fn output_bytes_from_result(result: &CallToolResult) -> u64 {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .map(|t| t.text.len() as u64)
            .sum()
    }

    /// Extract file paths from a CallToolResult's text content.
    pub(crate) fn extract_paths_from_result(result: &CallToolResult) -> Vec<String> {
        let text: String = result
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        extract_source_paths(&text)
    }

    /// Run auto-indexing in background (called after MCP handshake)
    async fn run_auto_indexing(&self) {
        use crate::startup::run_primary_workspace_repair;

        // Concurrent catch-up calls serialize through mutation_gate::acquire_gate(workspace_id)
        // inside run_primary_workspace_repair. Redundant invocations are cheap because
        // filter_changed_files short-circuits when nothing has changed.
        info!("🔍 Starting background auto-indexing check...");

        match run_primary_workspace_repair(self).await {
            Ok(Some(plan)) => {
                let reasons = plan
                    .reasons
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                info!(%reasons, "✅ Background auto-indexing completed startup repair");
            }
            Ok(None) => {
                info!("✅ Workspace already indexed - skipping auto-indexing");
            }
            Err(e) => {
                warn!("⚠️ Failed to check indexing status: {}", e);
            }
        }
    }

    // ========== Workspace Access Helpers ==========

    async fn primary_workspace_snapshot_from_loaded_workspace(
        &self,
        binding: &PrimaryWorkspaceBinding,
    ) -> Result<Option<PrimaryWorkspaceSnapshot>> {
        let workspace = self.get_workspace().await?;
        let loaded_workspace_id_after = self.loaded_workspace_id();
        let workspace = workspace.ok_or_else(|| {
            anyhow::anyhow!(
                "No workspace initialized. Run manage_workspace(operation=\"index\") first."
            )
        })?;
        if loaded_workspace_id_after.as_deref() != Some(binding.workspace_id.as_str())
            || workspace.root != binding.workspace_root
        {
            return Ok(None);
        }

        Ok(Some(PrimaryWorkspaceSnapshot {
            binding: binding.clone(),
            store: Arc::clone(&workspace.store),
            indexing_runtime: Some(Arc::clone(&workspace.indexing_runtime)),
        }))
    }

    async fn primary_workspace_snapshot_from_binding_paths(
        &self,
        binding: &PrimaryWorkspaceBinding,
    ) -> Result<PrimaryWorkspaceSnapshot> {
        let store = self
            .checkout_store_for_workspace(&binding.workspace_id, &binding.workspace_root)
            .await?;
        Ok(PrimaryWorkspaceSnapshot {
            binding: binding.clone(),
            store,
            indexing_runtime: None,
        })
    }

    pub(crate) async fn primary_workspace_snapshot(&self) -> Result<PrimaryWorkspaceSnapshot> {
        let binding = self.require_primary_workspace_binding()?;
        let prefers_loaded_workspace =
            self.loaded_workspace_id().as_deref() == Some(binding.workspace_id.as_str());

        if prefers_loaded_workspace {
            if let Some(snapshot) = self
                .primary_workspace_snapshot_from_loaded_workspace(&binding)
                .await?
            {
                return Ok(snapshot);
            }
        }

        self.primary_workspace_snapshot_from_binding_paths(&binding)
            .await
    }

    /// Active workspace IDs for this session, sorted for stable output.
    pub async fn active_workspace_ids(&self) -> Vec<String> {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .active_workspace_ids()
    }

    /// Check whether a workspace ID is currently active for this session.
    pub async fn is_workspace_active(&self, workspace_id: &str) -> bool {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .is_workspace_active(workspace_id)
    }

    /// Add a workspace ID to this session's active set. Returns `true` when it was new.
    pub fn mark_workspace_active(&self, workspace_id: &str) -> bool {
        self.update_session_workspace(|state| state.mark_workspace_active(workspace_id))
    }

    pub(crate) async fn workspace_storage_anchor(&self) -> Result<(PathBuf, Option<PathBuf>)> {
        let loaded_workspace = self.get_workspace().await?;
        let loaded_workspace_id = self.loaded_workspace_id();
        let current_workspace_id = self.current_workspace_id();

        // No workspace loaded yet: storage for secondary workspace operations
        // lives under `{startup_hint}/.julie/indexes/{id}/`.
        let Some(loaded_workspace) = loaded_workspace else {
            let hint_path = self.workspace_startup_hint().path;
            return Ok((hint_path, None));
        };

        if let Some(ref current_id) = current_workspace_id {
            if loaded_workspace_id.as_deref() != Some(current_id.as_str()) {
                return Ok((self.current_workspace_root(), None));
            }
        }

        if loaded_workspace_id.is_none() {
            return Err(anyhow::anyhow!("Loaded workspace has no workspace id"));
        }

        Ok((
            loaded_workspace.root.clone(),
            loaded_workspace.index_root_override.clone(),
        ))
    }

    pub(crate) async fn workspace_index_dir_for(&self, workspace_id: &str) -> Result<PathBuf> {
        // In-process / daemon-shared mode: the deleted `WorkspacePool` used to
        // resolve every workspace's storage as a sibling under the shared
        // `~/.julie/indexes/` root — even during a rebind window where the loaded
        // workspace hasn't caught up to the current primary binding. With the
        // pool gone, `in_process_index_root` (`<shared_indexes>/{primary_id}`) is
        // the authoritative shared-root source. Consult it FIRST so rebound and
        // secondary workspaces keep the shared anchor instead of falling back to
        // the current workspace's project-local `.julie` tree. This mirrors the
        // `anchor_override.parent().join(workspace_id)` resolution below, which
        // already assumes the override is `<shared_indexes>/{primary_id}`.
        if let Some(index_root) = &self.in_process_index_root {
            if let Some(shared_indexes) = index_root.parent() {
                return Ok(shared_indexes.join(workspace_id));
            }
        }
        let (anchor_root, anchor_override) = self.workspace_storage_anchor().await?;
        Ok(if let Some(ref override_root) = anchor_override {
            override_root
                .parent()
                .map(|shared_indexes| shared_indexes.join(workspace_id))
                .unwrap_or_else(|| {
                    anchor_root
                        .join(".julie")
                        .join("indexes")
                        .join(workspace_id)
                })
        } else {
            anchor_root
                .join(".julie")
                .join("indexes")
                .join(workspace_id)
        })
    }

    pub(crate) async fn workspace_tantivy_dir_for(&self, workspace_id: &str) -> Result<PathBuf> {
        Ok(self
            .workspace_index_dir_for(workspace_id)
            .await?
            .join("tantivy"))
    }

    /// The checkout store for `workspace_id`: the loaded primary's own store, or
    /// one opened from `indexes/{workspace_id}/` and cached for the session.
    /// Opens through `open_or_recreate`, so a facts version mismatch rebuilds the
    /// directory and every other open failure propagates. The empty store left by
    /// a rebuild is never cached: the checkout's own handler reindexes it, and a
    /// cached empty store would hide that work for the rest of the session.
    pub(crate) async fn checkout_store_for_workspace(
        &self,
        workspace_id: &str,
        workspace_root: &Path,
    ) -> Result<Arc<CheckoutStore>> {
        if self.loaded_workspace_id().as_deref() == Some(workspace_id) {
            if let Some(ws) = self.get_workspace().await? {
                self.ref_store_cache.write().await.remove(workspace_id);
                return Ok(ws.store);
            }
        }
        if let Some(store) = self.ref_store_cache.read().await.get(workspace_id) {
            return Ok(Arc::clone(store));
        }
        let store_dir = self.workspace_index_dir_for(workspace_id).await?;
        let root = workspace_root.to_path_buf();
        let opened =
            tokio::task::spawn_blocking(move || open_or_recreate(&store_dir, &root)).await??;
        let store = Arc::new(opened.store);
        if !opened.rebuilt {
            self.ref_store_cache
                .write()
                .await
                .insert(workspace_id.to_string(), Arc::clone(&store));
        }
        Ok(store)
    }

    pub(crate) async fn invalidate_checkout_store(&self, workspace_id: &str) {
        self.ref_store_cache.write().await.remove(workspace_id);
    }

    /// Get the root path on disk for a specific workspace by ID.
    ///
    /// Looks up the workspace entry in the primary workspace's
    /// registry and returns `WorkspaceEntry.original_path`.
    pub async fn get_workspace_root_for_target(&self, workspace_id: &str) -> Result<PathBuf> {
        // Daemon mode: look up in DaemonDatabase
        if let Some(ref db) = self.daemon_db {
            let row = db
                .get_workspace(workspace_id)?
                .ok_or_else(|| anyhow::anyhow!("Workspace '{}' not found", workspace_id))?;
            return Ok(PathBuf::from(row.path));
        }

        let primary = self.require_primary_binding()?;

        // Stdio mode: a rebound current primary may be queried through the non-primary path,
        // and secondary workspaces resolve through workspace_registry.json rooted at the
        // current primary workspace.
        if primary.workspace_id == workspace_id {
            Ok(primary.workspace_root)
        } else {
            let registry_path = primary
                .workspace_root
                .join(".julie")
                .join("workspace_registry.json");
            if registry_path.exists() {
                let registry_text = std::fs::read_to_string(&registry_path)?;
                let registry: crate::workspace::registry::WorkspaceRegistry =
                    serde_json::from_str(&registry_text)?;
                if let Some(entry) = registry.known_workspaces.get(workspace_id) {
                    return Ok(PathBuf::from(&entry.original_path));
                }
            }

            Err(anyhow::anyhow!(
                "Workspace '{}' not found in current primary workspace registry",
                workspace_id
            ))
        }
    }

    /// Returns the agent instructions embedded at compile time.
    ///
    /// `JULIE_AGENT_INSTRUCTIONS.md` is product metadata that ships with Julie,
    /// not something found in user workspaces. Embedding via `include_str!`
    /// guarantees instructions are always available regardless of deployment.
    fn load_agent_instructions(&self) -> Option<String> {
        Some(include_str!("../JULIE_AGENT_INSTRUCTIONS.md").to_string())
    }
}

/// Tool router composition for `JulieServerHandler`.
///
/// Each MCP tool lives in its own file under `src/handler/tools/<name>.rs`
/// with a dedicated `#[tool_router(router = tool_router_<name>)]` impl
/// block. This composer adds them together so callers can keep using
/// `Self::tool_router()`.
impl JulieServerHandler {
    pub fn tool_router() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        Self::tool_router_fast_search()
            + Self::tool_router_fast_refs()
            + Self::tool_router_call_path()
            + Self::tool_router_get_symbols()
            + Self::tool_router_deep_dive()
            + Self::tool_router_get_context()
            + Self::tool_router_blast_radius()
            + Self::tool_router_manage_workspace()
            + Self::tool_router_patterns()
            + Self::tool_router_edit_file()
    }
}

// ---------------------------------------------------------------------------
// Per-request deadline / hang guard
// ---------------------------------------------------------------------------

/// Default per-request deadline for read/query tools when the env var is unset.
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 120;

/// Override with this env var. Set to `"0"` to disable the deadline entirely.
const REQUEST_TIMEOUT_ENV: &str = "JULIE_INPROCESS_REQUEST_TIMEOUT_SECS";

/// Parse the per-request deadline from an optional raw env-var string.
///
/// - `Some("0")` → `None` (disabled; the tool is awaited without any ceiling).
/// - `None | Some("<invalid>")` → `Some(DEFAULT_REQUEST_TIMEOUT_SECS s)`.
/// - `Some("N")` (N > 0) → `Some(Duration::from_secs(N))`.
pub(crate) fn parse_request_timeout(raw: Option<String>) -> Option<Duration> {
    const DEFAULT: Duration = Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS);
    match raw.as_deref().map(str::trim) {
        Some("0") => None,
        Some(s) => match s.parse::<u64>() {
            Ok(0) => None,
            Ok(n) => Some(Duration::from_secs(n)),
            Err(_) => Some(DEFAULT),
        },
        None => Some(DEFAULT),
    }
}

/// Returns `true` when a tool call must not be bounded by the per-request deadline.
///
/// `edit_file` is always exempt — aborting a canonical write mid-transaction would corrupt
/// workspace state.
///
/// For `manage_workspace` the exemption is operation-aware:
/// - **Exempt (mutating / long-running):** Index, Register, Remove, Clean, Refresh, Open.
/// - **Deadline-bounded (read-only):** List, Stats, Health, Dashboard — these
///   have no write-safety concern, so they must not escape the hang guard.
/// - **Unparseable operation:** not exempt (safely bounded; no mutation risk).
pub(crate) fn is_write_exempt(
    tool_name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
) -> bool {
    if tool_name == "edit_file" {
        return true;
    }
    if tool_name == "manage_workspace" {
        let op = arguments
            .and_then(|m| m.get("operation"))
            .and_then(|v| v.as_str())
            .and_then(|s| ManageWorkspaceOperation::parse(s).ok());
        return matches!(
            op,
            Some(
                ManageWorkspaceOperation::Index
                    | ManageWorkspaceOperation::Remove
                    | ManageWorkspaceOperation::Refresh
                    | ManageWorkspaceOperation::Open
                    | ManageWorkspaceOperation::Rebuild
            )
        );
    }
    false
}

/// Dispatch a tool call with an optional per-request deadline.
///
/// `exempt` must be computed by [`is_write_exempt`] before `request` is moved
/// into the `ToolCallContext`. When `true`, the future is awaited unboundedly.
/// All other tools are bounded by `deadline` when it is `Some`.
///
/// On expiry, returns `Err(McpError)` naming the tool and the elapsed ceiling
/// so the caller gets a JSON-RPC error rather than a session hang.
#[allow(dead_code)]
pub(crate) async fn dispatch_with_deadline(
    tool_name: &str,
    exempt: bool,
    fut: impl std::future::Future<Output = Result<CallToolResult, McpError>>,
    deadline: Option<Duration>,
) -> Result<CallToolResult, McpError> {
    if exempt || deadline.is_none() {
        return fut.await;
    }
    let d = deadline.unwrap();
    match tokio::time::timeout(d, fut).await {
        Ok(result) => result,
        Err(_elapsed) => Err(McpError::internal_error(
            format!(
                "tool '{}' did not respond within {}s; \
                 request timed out to prevent session hang",
                tool_name,
                d.as_secs()
            ),
            None,
        )),
    }
}

impl JulieServerHandler {
    pub fn request_engine(&self) -> crate::request_engine::RequestEngine {
        let registry_paths = crate::paths::RegistryPaths::try_new().unwrap_or_else(|_| {
            crate::paths::RegistryPaths::with_home(self.current_workspace_root().join(".julie"))
        });
        let index_base_override = self
            .in_process_index_root
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let resolver = crate::request_engine::BindingResolver::new(
            Some(self.current_workspace_root()),
            self.in_process_index_root.is_some(),
            registry_paths.clone(),
        )
        .with_daemon_db(self.daemon_db.clone())
        .with_index_base_override(index_base_override);
        let runtimes = std::sync::Arc::new(crate::request_engine::RuntimeFactory::with_handler(
            std::sync::Arc::new(self.clone()),
            registry_paths,
        ));
        crate::request_engine::RequestEngine::new(resolver, runtimes)
    }
}

/// ServerHandler implementation
impl ServerHandler for JulieServerHandler {
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        std::borrow::Cow::Borrowed(mcp_adapter::JULIE_PROTOCOL_VERSIONS)
    }

    fn get_info(&self) -> ServerInfo {
        mcp_adapter::get_server_info(self.load_agent_instructions())
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ServerInfo, McpError> {
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }
        Ok(self.get_info())
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let tool_name = request.name.as_ref().to_string();
        let exempt = is_write_exempt(&tool_name, request.arguments.as_ref());

        if Self::tool_request_targets_primary(request.name.as_ref(), request.arguments.as_ref()) {
            if self.is_in_process() && !exempt {
                let deadline = parse_request_timeout(std::env::var(REQUEST_TIMEOUT_ENV).ok());
                let handler = self.clone();
                let bounded_fut = async move {
                    handler
                        .ensure_primary_workspace_for_request()
                        .await
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    let adapter = mcp_adapter::McpAdapter::new(
                        std::sync::Arc::new(handler.request_engine()),
                        Some(handler.current_workspace_root()),
                    )
                    .with_instructions(handler.load_agent_instructions());
                    adapter.call_tool(request, context).await
                };

                return match deadline {
                    Some(d) => match tokio::time::timeout(d, bounded_fut).await {
                        Ok(result) => result,
                        Err(_elapsed) => Err(McpError::internal_error(
                            format!(
                                "in-process workspace not ready within {s}s; \
                                 indexing in progress — retry shortly \
                                 (tool: '{tool_name}')",
                                s = d.as_secs()
                            ),
                            None,
                        )),
                    },
                    None => bounded_fut.await,
                };
            }

            self.ensure_primary_workspace_for_request()
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
        }

        let adapter = mcp_adapter::McpAdapter::new(
            std::sync::Arc::new(self.request_engine()),
            Some(self.current_workspace_root()),
        )
        .with_instructions(self.load_agent_instructions());
        adapter.call_tool(request, context).await
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.tool_router.list_all()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router.get(name).cloned()
    }

    async fn on_initialized(&self, _context: NotificationContext<RoleServer>) {
        info!("MCP connection established - client initialized");

        // Atomically claim the indexing slot. Two concurrent on_initialized calls on
        // a shared handler clone would both see is_indexed=false with a read lock;
        // upgrading to a write lock serializes them so only one proceeds.
        // Fix E: capture the outcome as a bool and drop the write lock BEFORE any .await.
        // Holding a Tokio RwLock write guard across .await blocks all readers.
        let already_indexed = {
            let mut indexed = self.is_indexed.write().await;
            if *indexed {
                true
            } else {
                *indexed = true;
                false
            }
        };
        // Write lock released here.

        if already_indexed {
            info!("Workspace already indexed, running staleness check");
            self.backfill_vector_count().await;
            // Still check for stale files in the background. The index
            // may be outdated if files changed while the daemon was down.
            let handler = self.clone();
            tokio::spawn(async move {
                handler.run_auto_indexing().await;
            });
            return;
        }

        // Run auto-indexing in background task
        let handler = self.clone();
        tokio::spawn(async move {
            handler.run_auto_indexing().await;
        });
    }
}
