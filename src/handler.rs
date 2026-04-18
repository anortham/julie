pub mod session_workspace;

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::tool::{ToolCallContext, ToolRouter},
    handler::server::wrapper::Parameters,
    model::{
        CallToolRequestParams, CallToolResult, Implementation, InitializeRequestParams,
        ListToolsResult, ServerCapabilities, ServerInfo, Tool,
    },
    service::{NotificationContext, Peer, RequestContext},
    tool, tool_router,
};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock as StdRwLock};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::daemon::session::{SessionLifecycleHandle, SessionLifecyclePhase};
use crate::dashboard::state::DashboardEvent;

use self::session_workspace::{PrimaryWorkspaceBinding, SessionWorkspaceState};
use crate::database::SymbolDatabase;
use crate::search::SearchIndex;
use crate::workspace::JulieWorkspace;
use crate::workspace::startup_hint::WorkspaceStartupHint;
use crate::workspace::startup_hint::WorkspaceStartupSource;
use tokio::sync::RwLock;

// Import tool parameter types
use crate::tools::metrics::session::{
    SessionMetrics, ToolCallReport, ToolKind, extract_source_paths,
};
use crate::tools::{
    DeepDiveTool, FastRefsTool, FastSearchTool, GetContextTool, GetSymbolsTool,
    ManageWorkspaceTool, RenameSymbolTool,
};

/// Data for a single metrics write, sent via bounded channel to the background writer.
/// Avoids spawning a new task per tool call (M03).
struct MetricsTask {
    workspace: Arc<RwLock<Option<JulieWorkspace>>>,
    workspace_pool: Option<Arc<crate::daemon::workspace_pool::WorkspacePool>>,
    current_workspace_root: PathBuf,
    session_metrics: Arc<SessionMetrics>,
    session_id: String,
    tool_name: String,
    duration_ms: f64,
    result_count: Option<u32>,
    source_file_paths: Vec<String>,
    output_bytes: u64,
    metadata_str: Option<String>,
    daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
    workspace_id: Option<String>,
}

pub(crate) struct PrimaryWorkspaceSnapshot {
    pub binding: PrimaryWorkspaceBinding,
    pub database: Arc<std::sync::Mutex<SymbolDatabase>>,
    pub search_index: Option<Arc<std::sync::Mutex<SearchIndex>>>,
    pub indexing_runtime: Option<crate::tools::workspace::indexing::state::SharedIndexingRuntime>,
}

#[derive(Clone)]
struct PrimarySwapRollback {
    workspace: Option<JulieWorkspace>,
    loaded_workspace_id: Option<String>,
    loaded_workspace_root: Option<PathBuf>,
    session_workspace: SessionWorkspaceState,
}

impl PrimarySwapRollback {
    async fn capture(handler: &JulieServerHandler) -> Self {
        let workspace = handler.workspace.read().await.clone();
        let loaded_workspace_root = workspace.as_ref().map(|workspace| workspace.root.clone());
        let loaded_workspace_id = handler
            .workspace_id
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let session_workspace = handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();

        Self {
            workspace,
            loaded_workspace_id,
            loaded_workspace_root,
            session_workspace,
        }
    }

    async fn restore(self, handler: &JulieServerHandler) -> Result<()> {
        let mut restored_workspace = match (
            self.loaded_workspace_id.as_deref(),
            self.loaded_workspace_root.clone(),
        ) {
            (Some(workspace_id), Some(workspace_root))
                if handler.workspace_pool.is_some() && handler.daemon_db.is_some() =>
            {
                Some(
                    handler
                        .acquire_pooled_workspace_for_rebind(workspace_id, workspace_root)
                        .await?,
                )
            }
            (_, Some(workspace_root)) => JulieWorkspace::detect_and_load(workspace_root)
                .await?
                .or(self.workspace),
            (_, None) => self.workspace,
        };

        if handler.workspace_pool.is_none() && handler.daemon_db.is_none() {
            if let Some(workspace) = restored_workspace.as_mut() {
                if workspace.config.incremental_updates {
                    workspace.initialize_file_watcher()?;
                    workspace.start_file_watching().await?;
                }
            }
        }

        *handler.workspace.write().await = restored_workspace;
        handler.set_loaded_workspace_id(self.loaded_workspace_id);
        let phase = {
            let mut session_workspace = handler
                .session_workspace
                .write()
                .unwrap_or_else(|p| p.into_inner());
            *session_workspace = self.session_workspace;
            session_workspace.lifecycle_phase()
        };
        handler.publish_session_lifecycle_phase(phase);
        Ok(())
    }
}

pub(crate) fn metrics_db_path_for_workspace(
    index_root_override: Option<&std::path::Path>,
    current_workspace_root: &std::path::Path,
    workspace_id: &str,
) -> PathBuf {
    if let Some(override_root) = index_root_override {
        override_root
            .parent()
            .map(|shared_indexes| {
                shared_indexes
                    .join(workspace_id)
                    .join("db")
                    .join("symbols.db")
            })
            .unwrap_or_else(|| {
                current_workspace_root
                    .join(".julie")
                    .join("indexes")
                    .join(workspace_id)
                    .join("db")
                    .join("symbols.db")
            })
    } else {
        current_workspace_root
            .join(".julie")
            .join("indexes")
            .join(workspace_id)
            .join("db")
            .join("symbols.db")
    }
}

/// Single background task that drains the metrics channel and writes to SQLite.
async fn run_metrics_writer(mut rx: tokio::sync::mpsc::Receiver<MetricsTask>) {
    while let Some(task) = rx.recv().await {
        // Compute source_bytes from the workspace DB, then use it for both writes.
        let mut source_bytes: Option<u64> = None;
        let mut resolved_workspace = task.workspace.read().await.clone();
        if resolved_workspace.is_none() {
            if let (Some(pool), Some(workspace_id)) =
                (&task.workspace_pool, task.workspace_id.as_ref())
            {
                resolved_workspace = pool.get(workspace_id).await.map(|ws| (*ws).clone());
            }
        }

        if let Some(ws) = resolved_workspace.as_ref() {
            if let Some(ref workspace_id) = task.workspace_id {
                let db_path = metrics_db_path_for_workspace(
                    ws.index_root_override.as_deref(),
                    &task.current_workspace_root,
                    workspace_id,
                );
                if db_path.exists() {
                    if let Ok(db) = SymbolDatabase::new(db_path) {
                        source_bytes = if !task.source_file_paths.is_empty() {
                            let path_refs: Vec<&str> =
                                task.source_file_paths.iter().map(|s| s.as_str()).collect();
                            db.get_total_file_sizes(&path_refs).ok()
                        } else {
                            None
                        };
                        if let Some(sb) = source_bytes {
                            task.session_metrics
                                .total_source_bytes
                                .fetch_add(sb, std::sync::atomic::Ordering::Relaxed);
                        }
                        let _ = db.insert_tool_call(
                            &task.session_id,
                            &task.tool_name,
                            task.duration_ms,
                            task.result_count,
                            source_bytes,
                            Some(task.output_bytes),
                            true,
                            task.metadata_str.as_deref(),
                        );
                    }
                }
            } else if let Some(db_arc) = &ws.db {
                if let Ok(db) = db_arc.lock() {
                    source_bytes = if !task.source_file_paths.is_empty() {
                        let path_refs: Vec<&str> =
                            task.source_file_paths.iter().map(|s| s.as_str()).collect();
                        db.get_total_file_sizes(&path_refs).ok()
                    } else {
                        None
                    };
                    if let Some(sb) = source_bytes {
                        task.session_metrics
                            .total_source_bytes
                            .fetch_add(sb, std::sync::atomic::Ordering::Relaxed);
                    }
                    let _ = db.insert_tool_call(
                        &task.session_id,
                        &task.tool_name,
                        task.duration_ms,
                        task.result_count,
                        source_bytes,
                        Some(task.output_bytes),
                        true,
                        task.metadata_str.as_deref(),
                    );
                }
            }
        } else if let Some(ref workspace_id) = task.workspace_id {
            let db_path =
                metrics_db_path_for_workspace(None, &task.current_workspace_root, workspace_id);
            if db_path.exists() {
                if let Ok(db) = SymbolDatabase::new(db_path) {
                    source_bytes = if !task.source_file_paths.is_empty() {
                        let path_refs: Vec<&str> =
                            task.source_file_paths.iter().map(|s| s.as_str()).collect();
                        db.get_total_file_sizes(&path_refs).ok()
                    } else {
                        None
                    };
                    if let Some(sb) = source_bytes {
                        task.session_metrics
                            .total_source_bytes
                            .fetch_add(sb, std::sync::atomic::Ordering::Relaxed);
                    }
                    let _ = db.insert_tool_call(
                        &task.session_id,
                        &task.tool_name,
                        task.duration_ms,
                        task.result_count,
                        source_bytes,
                        Some(task.output_bytes),
                        true,
                        task.metadata_str.as_deref(),
                    );
                }
            }
        }

        if let Some(daemon_db) = task.daemon_db {
            let workspace_id = task.workspace_id.unwrap_or_default();
            let session_id = task.session_id;
            let tool_name = task.tool_name;
            let duration_ms = task.duration_ms;
            let result_count = task.result_count;
            let output_bytes = task.output_bytes;
            let metadata_str = task.metadata_str;
            tokio::task::spawn_blocking(move || {
                if let Err(e) = daemon_db.insert_tool_call(
                    &workspace_id,
                    &session_id,
                    &tool_name,
                    duration_ms,
                    result_count,
                    source_bytes,
                    Some(output_bytes),
                    true,
                    metadata_str.as_deref(),
                ) {
                    warn!("Failed to write tool call to daemon.db: {}", e);
                }
            });
        }
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

/// Julie's custom handler for MCP messages
///
/// This handler manages the core Julie functionality including:
/// - Code intelligence operations (search, navigation, extraction)
/// - Symbol database management
/// - Cross-language relationship detection
#[derive(Clone)]
pub struct JulieServerHandler {
    /// Original workspace root captured at handler construction. Production
    /// code should read `current_workspace_root()` instead, which reflects
    /// rebind state. This field is retained for tests that need to compare
    /// against the pre-rebind root.
    #[allow(dead_code)]
    pub(crate) workspace_root: PathBuf,
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
    pub(crate) project_log: Option<Arc<crate::daemon::project_log::ProjectLog>>,
    /// Daemon-level database for persistent metrics and workspace registry.
    /// None in stdio mode, Some in daemon mode.
    pub(crate) daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
    /// Workspace ID for the workspace currently stored in `handler.workspace`.
    /// Keep this separate from `current_workspace_id()`, which reads session-owned
    /// mutable state and may diverge during rebinding.
    pub(crate) workspace_id: Arc<StdRwLock<Option<String>>>,
    /// Shared embedding service for daemon mode. None in stdio mode.
    pub(crate) embedding_service: Option<Arc<crate::daemon::embedding_service::EmbeddingService>>,
    /// True when the daemon detects its binary has been rebuilt.
    /// Surfaced in `manage_workspace health`. None in stdio mode.
    pub(crate) restart_pending: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// Fix G: prevents concurrent catch-up auto-indexing scans.
    /// CAS'd to true by the first `run_auto_indexing` call; cleared on exit.
    catchup_in_progress: Arc<AtomicBool>,
    /// Set when on_initialized defers auto-indexing until the primary workspace
    /// is resolved from client roots. Consumed by the first successful bind.
    deferred_auto_index_pending: Arc<AtomicBool>,
    /// Optional daemon session lifecycle handle. Present when this handler is
    /// serving an IPC session through the daemon.
    session_lifecycle: Option<SessionLifecycleHandle>,
    /// Fix C part c: shared watcher pool for pausing reference workspace watchers
    /// during force reindex. None in stdio mode.
    pub(crate) watcher_pool: Option<Arc<crate::daemon::watcher_pool::WatcherPool>>,
    /// Bounded channel sender for background metrics writes (M03).
    /// A single background task drains this; try_send drops on backpressure
    /// rather than spawning unbounded tasks.
    metrics_tx: tokio::sync::mpsc::Sender<MetricsTask>,
    /// Cache for reference workspace DB connections, keyed by workspace_id with
    /// the resolved physical db path so root-anchor changes in stdio do not reuse
    /// stale handles across different `.julie/indexes/...` trees.
    ref_db_cache: Arc<RwLock<HashMap<String, (PathBuf, Arc<std::sync::Mutex<SymbolDatabase>>)>>>,
    /// Shared daemon workspace pool for explicit workspace activation.
    pub(crate) workspace_pool: Option<Arc<crate::daemon::workspace_pool::WorkspacePool>>,
    /// Broadcast sender for dashboard live-feed events. None in stdio/test mode.
    dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    /// Keeps isolated temp roots alive for test-only handlers.
    #[cfg(test)]
    test_temp_guard: Option<Arc<tempfile::TempDir>>,
}

impl JulieServerHandler {
    fn canonicalize_workspace_path(path: PathBuf) -> PathBuf {
        path.canonicalize().unwrap_or(path)
    }

    fn decode_root_uri_path(uri_path: &str) -> Option<String> {
        let mut decoded = Vec::with_capacity(uri_path.len());
        let bytes = uri_path.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                if i + 2 >= bytes.len() {
                    return None;
                }
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                decoded.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            } else {
                decoded.push(bytes[i]);
                i += 1;
            }
        }

        String::from_utf8(decoded).ok()
    }

    fn workspace_path_from_root_uri(uri: &str) -> Option<PathBuf> {
        let uri_path = uri.strip_prefix("file://")?;
        let mut decoded = Self::decode_root_uri_path(uri_path)?;

        if let Some(rest) = decoded.strip_prefix("localhost/") {
            decoded = format!("/{}", rest);
        } else if decoded == "localhost" {
            decoded = "/".to_string();
        }

        #[cfg(windows)]
        let decoded = if decoded.starts_with('/') && decoded.as_bytes().get(2) == Some(&b':') {
            decoded[1..].to_string()
        } else if !decoded.starts_with('/') && decoded.contains('/') {
            format!(r"\\{}", decoded.replace('/', r"\"))
        } else {
            decoded
        };

        Some(PathBuf::from(decoded))
    }

    #[cfg(test)]
    pub(crate) fn workspace_path_from_root_uri_for_test(uri: &str) -> Option<PathBuf> {
        Self::workspace_path_from_root_uri(uri)
    }

    fn client_supports_workspace_roots(&self) -> bool {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .client_supports_workspace_roots
    }

    fn record_client_roots_capability(&self, supported: bool) {
        self.session_workspace
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .client_supports_workspace_roots = supported;
    }

    fn record_roots_snapshot(&self, roots: &[PathBuf]) {
        let mut state = self
            .session_workspace
            .write()
            .unwrap_or_else(|p| p.into_inner());
        state.last_roots_snapshot = Some(roots.to_vec());
    }

    fn last_roots_snapshot(&self) -> Option<Vec<PathBuf>> {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .last_roots_snapshot
            .clone()
    }

    fn roots_dirty(&self) -> bool {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty()
    }

    fn mark_roots_dirty(&self) {
        self.session_workspace
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .mark_roots_dirty();
    }

    async fn list_roots_from_peer(&self, peer: &Peer<RoleServer>) -> Result<Vec<PathBuf>> {
        let roots = peer
            .list_roots()
            .await
            .map_err(|err| anyhow::anyhow!("roots/list failed: {err}"))?
            .roots
            .into_iter()
            .filter_map(|root| {
                let path = Self::workspace_path_from_root_uri(&root.uri);
                if path.is_none() {
                    warn!(uri = %root.uri, "Ignoring unsupported root URI");
                }
                path
            })
            .map(Self::canonicalize_workspace_path)
            .collect::<Vec<_>>();

        self.record_roots_snapshot(&roots);
        Ok(roots)
    }

    fn primary_binding_for_root(&self, workspace_root: PathBuf) -> Result<PrimaryWorkspaceBinding> {
        let workspace_root = Self::canonicalize_workspace_path(workspace_root);
        let workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace_root.to_string_lossy())?;
        Ok(PrimaryWorkspaceBinding {
            workspace_id,
            workspace_root,
        })
    }

    fn activate_primary_binding(&self, binding: &PrimaryWorkspaceBinding) {
        self.rebind_current_primary(binding.workspace_id.clone(), binding.workspace_root.clone());
    }

    fn mark_deferred_auto_index_pending(&self, pending: bool) {
        use std::sync::atomic::Ordering;

        self.deferred_auto_index_pending
            .store(pending, Ordering::Release);
    }

    fn resume_deferred_auto_index_if_needed(&self) {
        use std::sync::atomic::Ordering;

        if !self
            .deferred_auto_index_pending
            .swap(false, Ordering::AcqRel)
        {
            return;
        }

        let handler = self.clone();
        tokio::spawn(async move {
            handler.run_auto_indexing().await;
        });
    }

    async fn attach_daemon_primary_binding_if_needed(
        &self,
        binding: &PrimaryWorkspaceBinding,
    ) -> Result<()> {
        let Some(pool) = &self.workspace_pool else {
            return Ok(());
        };

        if self
            .was_workspace_attached_in_session(&binding.workspace_id)
            .await
        {
            return Ok(());
        }

        pool.get_or_init(&binding.workspace_id, binding.workspace_root.clone())
            .await?;
        self.session_workspace
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .mark_workspace_attached(binding.workspace_id.clone());
        Ok(())
    }

    async fn reconcile_primary_workspace_roots(&self, roots: Vec<PathBuf>) -> Result<bool> {
        let Some(primary_root) = roots.first().cloned() else {
            return Ok(false);
        };

        let primary_binding = self.primary_binding_for_root(primary_root)?;
        self.attach_daemon_primary_binding_if_needed(&primary_binding)
            .await?;

        let mut secondary_workspace_ids = {
            let state = self
                .session_workspace
                .read()
                .unwrap_or_else(|p| p.into_inner());
            let current_primary_id = state.current_workspace_id();
            state
                .active_workspace_ids()
                .into_iter()
                .filter(|workspace_id| Some(workspace_id.as_str()) != current_primary_id.as_deref())
                .collect::<HashSet<_>>()
        };
        for root in roots.iter().skip(1).cloned() {
            let binding = self.primary_binding_for_root(root)?;
            self.activate_workspace_with_root(
                &binding.workspace_id,
                binding.workspace_root.clone(),
            )
            .await?;
            secondary_workspace_ids.insert(binding.workspace_id);
        }

        self.update_session_workspace(move |state| {
            state.apply_root_snapshot(primary_binding, secondary_workspace_ids, roots);
        });
        Ok(true)
    }

    async fn reconcile_primary_workspace_to_startup_hint(&self) -> Result<()> {
        let startup_binding = self.primary_binding_for_root(self.workspace_startup_hint().path)?;
        self.attach_daemon_primary_binding_if_needed(&startup_binding)
            .await?;
        let secondary_workspace_ids = {
            let state = self
                .session_workspace
                .read()
                .unwrap_or_else(|p| p.into_inner());
            let current_primary_id = state.current_workspace_id();
            state
                .active_workspace_ids()
                .into_iter()
                .filter(|workspace_id| Some(workspace_id.as_str()) != current_primary_id.as_deref())
                .collect()
        };
        self.update_session_workspace(move |state| {
            state.apply_root_snapshot(startup_binding, secondary_workspace_ids, Vec::new());
        });
        Ok(())
    }

    async fn ensure_primary_workspace_for_request(&self, peer: &Peer<RoleServer>) -> Result<()> {
        let existing_binding = match self.require_primary_binding() {
            Ok(binding) => Some(binding),
            Err(err) if self.is_primary_workspace_swap_in_progress() => return Err(err),
            Err(_) => None,
        };
        let prefers_request_roots = crate::startup::startup_source_prefers_request_roots(
            self.workspace_startup_hint().source,
        );

        if !prefers_request_roots {
            if self.roots_dirty() || existing_binding.is_none() {
                self.reconcile_primary_workspace_to_startup_hint().await?;
                self.resume_deferred_auto_index_if_needed();
            }
            return Ok(());
        }

        if existing_binding.is_some() && !self.roots_dirty() {
            return Ok(());
        }

        if self.client_supports_workspace_roots() {
            match self.list_roots_from_peer(peer).await {
                Ok(roots) => {
                    if self.reconcile_primary_workspace_roots(roots).await? {
                        self.resume_deferred_auto_index_if_needed();
                        return Ok(());
                    }

                    self.reconcile_primary_workspace_to_startup_hint().await?;
                    self.resume_deferred_auto_index_if_needed();
                    return Ok(());
                }
                Err(err) => {
                    warn!(
                        "Failed to query client roots during request-time primary resolution: {err}"
                    );
                    if self.roots_dirty() {
                        return Err(err);
                    }

                    if existing_binding.is_none() {
                        if let Some(roots) =
                            self.last_roots_snapshot().filter(|roots| !roots.is_empty())
                        {
                            if self.reconcile_primary_workspace_roots(roots).await? {
                                self.resume_deferred_auto_index_if_needed();
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        let binding = self.primary_binding_for_root(self.workspace_startup_hint().path)?;
        self.attach_daemon_primary_binding_if_needed(&binding)
            .await?;
        self.activate_primary_binding(&binding);
        self.resume_deferred_auto_index_if_needed();
        Ok(())
    }

    fn manage_workspace_request_targets_primary(
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> bool {
        let Some(arguments) = arguments else {
            return false;
        };

        let operation = arguments
            .get("operation")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        match operation {
            // `add` is intentionally excluded: it needs a primary to pair
            // against, but we refuse to silently bind the startup-hint/CWD as
            // primary on the user's behalf. The tool body hard-fails with an
            // actionable message that points at `open` or client roots.
            // See Finding #2 in docs/ROOTS_IMPL_REVIEW_NOTES.md.
            "list" | "remove" | "health" => true,
            "stats" => arguments
                .get("workspace_id")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|workspace_id| workspace_id == "primary"),
            "index" => arguments.get("path").is_none_or(serde_json::Value::is_null),
            _ => false,
        }
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
            | "get_context" | "rename_symbol" => workspace_is_primary,
            "manage_workspace" => Self::manage_workspace_request_targets_primary(arguments),
            "edit_file" => true,
            "rewrite_symbol" => workspace_is_primary,
            _ => false,
        }
    }

    async fn teardown_loaded_workspace(&self, release_shared_only: bool) {
        let mut workspace_guard = self.workspace.write().await;
        if let Some(ref mut old_workspace) = *workspace_guard {
            if release_shared_only {
                info!("Releasing pooled loaded workspace reference before replacement");
            } else {
                info!("Tearing down loaded workspace before replacement");
            }

            if release_shared_only {
                *workspace_guard = None;
                self.set_loaded_workspace_id(None);
                return;
            }

            if let Err(e) = old_workspace.stop_file_watching().await {
                warn!("Failed to stop file watching during teardown: {}", e);
            }

            if let Some(ref search_index) = old_workspace.search_index {
                match search_index.lock() {
                    Ok(idx) => {
                        if let Err(e) = idx.shutdown() {
                            warn!("Failed to shut down search index: {}", e);
                        } else {
                            info!("Old search index shut down, file lock released");
                        }
                    }
                    Err(poisoned) => {
                        let idx = poisoned.into_inner();
                        let _ = idx.shutdown();
                        warn!("Recovered from poisoned search index mutex during teardown");
                    }
                }
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
            workspace_root,
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
            embedding_service: None,
            restart_pending: None,
            catchup_in_progress: Arc::new(AtomicBool::new(false)),
            deferred_auto_index_pending: Arc::new(AtomicBool::new(false)),
            session_lifecycle: None,
            watcher_pool: None,
            metrics_tx,
            ref_db_cache: Arc::new(RwLock::new(HashMap::new())),
            workspace_pool: None,
            dashboard_tx: None,
            #[cfg(test)]
            test_temp_guard: None,
        })
    }

    /// Create a handler for daemon mode, backed by a shared workspace from WorkspacePool.
    ///
    /// Each handler gets its own `session_metrics` and `indexing_status` (per-session),
    /// but the workspace's expensive resources (db, search_index) are shared across
    /// sessions via their inner `Arc<Mutex<...>>` pointers.
    ///
    /// Clone semantics of JulieWorkspace:
    /// - `db: Arc<Mutex<SqliteDB>>` and `search_index: Arc<Mutex<SearchIndex>>` are
    ///   shared (Arc clone). This is the whole point: multiple sessions hit one db.
    /// - `watcher` is `None` in the clone (daemon manages file watchers separately).
    /// - `embedding_provider` is set to `None` (Phase 3 handles shared embeddings).
    pub async fn new_with_shared_workspace(
        workspace: Arc<JulieWorkspace>,
        workspace_root: PathBuf,
        daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
        workspace_id: Option<String>,
        embedding_service: Option<Arc<crate::daemon::embedding_service::EmbeddingService>>,
        restart_pending: Option<Arc<std::sync::atomic::AtomicBool>>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
        watcher_pool: Option<Arc<crate::daemon::watcher_pool::WatcherPool>>,
        workspace_pool: Option<Arc<crate::daemon::workspace_pool::WorkspacePool>>,
    ) -> Result<Self> {
        Self::new_with_shared_workspace_startup_hint(
            workspace,
            WorkspaceStartupHint {
                path: workspace_root,
                source: None,
            },
            daemon_db,
            workspace_id,
            embedding_service,
            restart_pending,
            dashboard_tx,
            watcher_pool,
            workspace_pool,
        )
        .await
    }

    pub async fn new_with_shared_workspace_startup_hint(
        workspace: Arc<JulieWorkspace>,
        workspace_startup_hint: WorkspaceStartupHint,
        daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
        workspace_id: Option<String>,
        embedding_service: Option<Arc<crate::daemon::embedding_service::EmbeddingService>>,
        restart_pending: Option<Arc<std::sync::atomic::AtomicBool>>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
        watcher_pool: Option<Arc<crate::daemon::watcher_pool::WatcherPool>>,
        workspace_pool: Option<Arc<crate::daemon::workspace_pool::WorkspacePool>>,
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

        let already_indexed = if let Some(ref db_arc) = ws_clone.db {
            let db = db_arc.lock().unwrap_or_else(|p| p.into_inner());
            let count = db.count_symbols_for_workspace().unwrap_or(0);
            count > 0
        } else {
            false
        };

        let prefer_request_roots =
            crate::startup::startup_source_prefers_request_roots(workspace_startup_hint.source);
        let mut session_workspace = SessionWorkspaceState::new(workspace_startup_hint.clone());
        if let Some(ref id) = workspace_id {
            if !prefer_request_roots {
                session_workspace.bind_primary(id.clone(), workspace_root.clone());
            }
            session_workspace.mark_workspace_attached(id.clone());
        }

        // Create per-project logger for daemon mode
        let project_log = Some(Arc::new(crate::daemon::project_log::ProjectLog::new(
            &workspace_root,
        )));

        let (metrics_tx, metrics_rx) = tokio::sync::mpsc::channel::<MetricsTask>(512);
        tokio::spawn(run_metrics_writer(metrics_rx));

        Ok(Self {
            workspace_root,
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
            embedding_service,
            restart_pending,
            catchup_in_progress: Arc::new(AtomicBool::new(false)),
            deferred_auto_index_pending: Arc::new(AtomicBool::new(false)),
            session_lifecycle: None,
            watcher_pool,
            metrics_tx,
            ref_db_cache: Arc::new(RwLock::new(HashMap::new())),
            workspace_pool,
            dashboard_tx,
            #[cfg(test)]
            test_temp_guard: None,
        })
    }

    pub async fn new_deferred_daemon_startup_hint(
        workspace_startup_hint: WorkspaceStartupHint,
        daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
        embedding_service: Option<Arc<crate::daemon::embedding_service::EmbeddingService>>,
        restart_pending: Option<Arc<std::sync::atomic::AtomicBool>>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
        watcher_pool: Option<Arc<crate::daemon::watcher_pool::WatcherPool>>,
        workspace_pool: Option<Arc<crate::daemon::workspace_pool::WorkspacePool>>,
    ) -> Result<Self> {
        let workspace_root = workspace_startup_hint.path.clone();
        info!(
            "Creating deferred daemon-mode handler (workspace_root: {:?})",
            workspace_root
        );

        let (metrics_tx, metrics_rx) = tokio::sync::mpsc::channel::<MetricsTask>(512);
        tokio::spawn(run_metrics_writer(metrics_rx));

        Ok(Self {
            workspace_root: workspace_root.clone(),
            session_workspace: Arc::new(StdRwLock::new(SessionWorkspaceState::new(
                workspace_startup_hint,
            ))),
            workspace: Arc::new(RwLock::new(None)),
            is_indexed: Arc::new(RwLock::new(false)),
            indexing_status: Arc::new(IndexingStatus::new()),
            session_metrics: Arc::new(SessionMetrics::new()),
            embedding_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            tool_router: Self::tool_router(),
            project_log: Some(Arc::new(crate::daemon::project_log::ProjectLog::new(
                &workspace_root,
            ))),
            daemon_db,
            workspace_id: Arc::new(StdRwLock::new(None)),
            embedding_service,
            restart_pending,
            catchup_in_progress: Arc::new(AtomicBool::new(false)),
            deferred_auto_index_pending: Arc::new(AtomicBool::new(false)),
            session_lifecycle: None,
            watcher_pool,
            metrics_tx,
            ref_db_cache: Arc::new(RwLock::new(HashMap::new())),
            workspace_pool,
            dashboard_tx,
            #[cfg(test)]
            test_temp_guard: None,
        })
    }

    /// Test-only convenience: create a handler rooted in an isolated temp dir.
    ///
    /// Using `current_dir()` here lets tests spray `.julie/indexes` under the
    /// repo when they forget to bind a temp workspace first. Keep the anchor in
    /// temp storage so path=None stays isolated by default.
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

    pub(crate) fn attach_session_lifecycle(&mut self, session_lifecycle: SessionLifecycleHandle) {
        let phase = self.current_session_lifecycle_phase();
        session_lifecycle.set_phase(phase);
        self.session_lifecycle = Some(session_lifecycle);
    }

    fn current_session_lifecycle_phase(&self) -> SessionLifecyclePhase {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .lifecycle_phase()
    }

    fn publish_session_lifecycle_phase(&self, phase: SessionLifecyclePhase) {
        if let Some(session_lifecycle) = &self.session_lifecycle {
            session_lifecycle.set_phase(phase);
        }
    }

    fn update_session_workspace<R>(
        &self,
        update: impl FnOnce(&mut SessionWorkspaceState) -> R,
    ) -> R {
        let (result, phase) = {
            let mut state = self
                .session_workspace
                .write()
                .unwrap_or_else(|p| p.into_inner());
            let result = update(&mut state);
            let phase = state.lifecycle_phase();
            (result, phase)
        };
        self.publish_session_lifecycle_phase(phase);
        result
    }

    pub(crate) fn mark_session_serving(&self) {
        self.update_session_workspace(|state| state.mark_serving());
    }

    pub(crate) fn mark_session_closing(&self) {
        self.update_session_workspace(|state| state.mark_closing());
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

    pub fn is_primary_workspace_swap_in_progress(&self) -> bool {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .primary_swap_in_progress()
    }

    fn require_primary_binding(&self) -> Result<PrimaryWorkspaceBinding> {
        let session_workspace = self
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner());

        if session_workspace.primary_swap_in_progress() {
            return Err(anyhow::anyhow!(
                "Primary workspace identity unavailable during swap"
            ));
        }

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
        if self.is_primary_workspace_swap_in_progress() {
            return None;
        }

        self.workspace_id
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn set_loaded_workspace_id(&self, workspace_id: Option<String>) {
        *self.workspace_id.write().unwrap_or_else(|p| p.into_inner()) = workspace_id;
    }

    pub async fn attached_workspace_id(&self) -> Option<String> {
        let state = self
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(current_id) = state.current_workspace_id() {
            if state.was_workspace_attached_in_session(&current_id) {
                return Some(current_id);
            }
        }

        let loaded_id = self.loaded_workspace_id()?;
        if state.was_workspace_attached_in_session(&loaded_id) {
            Some(loaded_id)
        } else {
            None
        }
    }

    /// Returns whether the workspace was attached at any point during this
    /// session. This is session-lifetime bookkeeping for pool/session-count
    /// cleanup, not a guarantee about the currently loaded workspace.
    pub async fn was_workspace_attached_in_session(&self, workspace_id: &str) -> bool {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .was_workspace_attached_in_session(workspace_id)
    }

    fn rebind_current_primary(&self, workspace_id: impl Into<String>, workspace_root: PathBuf) {
        let workspace_id = workspace_id.into();
        self.update_session_workspace(move |session_workspace| {
            session_workspace.bind_primary(workspace_id, workspace_root);
        });
    }

    fn publish_loaded_workspace_swap_intent(&self) {
        self.update_session_workspace(|session_workspace| {
            session_workspace.begin_primary_swap();
            session_workspace.clear_primary_binding();
        });
        self.set_loaded_workspace_id(None);
    }

    async fn publish_loaded_workspace_swap(
        &self,
        workspace: JulieWorkspace,
        workspace_id: Option<String>,
        mark_attached: bool,
    ) {
        let workspace_root = workspace.root.clone();
        let mut workspace_guard = self.workspace.write().await;
        *workspace_guard = Some(workspace);

        *self.workspace_id.write().unwrap_or_else(|p| p.into_inner()) = workspace_id.clone();

        self.update_session_workspace(move |session_workspace| {
            if let Some(workspace_id) = workspace_id {
                session_workspace.bind_primary(workspace_id.clone(), workspace_root);
                if mark_attached {
                    session_workspace.mark_workspace_attached(workspace_id);
                }
            }

            session_workspace.complete_primary_swap();
        });
    }

    async fn acquire_pooled_workspace_for_rebind(
        &self,
        workspace_id: &str,
        workspace_root: PathBuf,
    ) -> Result<JulieWorkspace> {
        let pool = self.workspace_pool.as_ref().expect("pool checked above");
        let pooled_workspace = if self.was_workspace_attached_in_session(workspace_id).await {
            pool.get(workspace_id).await.ok_or_else(|| {
                anyhow::anyhow!(
                    "Workspace '{}' was marked attached but is missing from the workspace pool",
                    workspace_id
                )
            })?
        } else {
            pool.get_or_init(workspace_id, workspace_root).await?
        };

        let mut workspace = (*pooled_workspace).clone();
        workspace.embedding_provider = None;
        Ok(workspace)
    }

    #[cfg(test)]
    pub fn set_current_primary_binding(
        &self,
        workspace_id: impl Into<String>,
        workspace_root: PathBuf,
    ) {
        self.rebind_current_primary(workspace_id, workspace_root);
    }

    #[cfg(test)]
    pub fn set_client_supports_workspace_roots_for_test(&self, supported: bool) {
        self.record_client_roots_capability(supported);
    }

    #[cfg(test)]
    pub fn publish_loaded_workspace_swap_intent_for_test(&self) {
        self.publish_loaded_workspace_swap_intent();
    }

    #[cfg(test)]
    pub fn session_lifecycle_phase_for_test(&self) -> SessionLifecyclePhase {
        self.current_session_lifecycle_phase()
    }

    #[cfg(test)]
    pub fn mark_session_serving_for_test(&self) {
        self.mark_session_serving();
    }

    #[cfg(test)]
    pub async fn publish_loaded_workspace_swap_teardown_gap_for_test(&self) {
        self.publish_loaded_workspace_swap_intent();
        self.teardown_loaded_workspace(false).await;
    }

    #[cfg(test)]
    pub async fn loaded_workspace_file_watcher_running_for_test(&self) -> bool {
        let workspace_guard = self.workspace.read().await;
        workspace_guard
            .as_ref()
            .and_then(|workspace| workspace.watcher.as_ref())
            .is_some_and(|watcher| watcher.is_running_for_test())
    }

    /// Get the workspace root path for workspace operations.
    ///
    /// Returns the resolved workspace root that was passed to `new()`.
    /// This replaces the old `current_dir()` fallback, ensuring the handler
    /// always uses the path determined by main.rs (CLI > env var > cwd).
    fn get_workspace_path(&self) -> PathBuf {
        self.current_workspace_root()
    }

    /// Get the embedding provider, preferring daemon shared service over per-workspace.
    pub(crate) async fn embedding_provider(
        &self,
    ) -> Option<Arc<dyn crate::embeddings::EmbeddingProvider>> {
        // Daemon mode: use shared service
        if let Some(ref service) = self.embedding_service {
            return service.provider();
        }
        // Stdio mode: use per-workspace provider
        let ws = self.workspace.read().await;
        ws.as_ref().and_then(|ws| ws.embedding_provider.clone())
    }

    /// Get embedding runtime status, preferring daemon shared service.
    pub(crate) async fn embedding_runtime_status(
        &self,
    ) -> Option<crate::embeddings::EmbeddingRuntimeStatus> {
        if let Some(ref service) = self.embedding_service {
            return service.runtime_status();
        }
        let ws = self.workspace.read().await;
        ws.as_ref()
            .and_then(|ws| ws.embedding_runtime_status.clone())
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
        let loaded_workspace_root_changed = {
            let workspace_guard = self.workspace.read().await;
            workspace_guard.as_ref().is_some_and(|workspace| {
                workspace
                    .root
                    .canonicalize()
                    .unwrap_or_else(|_| workspace.root.clone())
                    != target_canonical
            })
        };
        let target_workspace_id =
            crate::workspace::registry::generate_workspace_id(&target_canonical.to_string_lossy())
                .ok();
        // In daemon mode, the primary workspace MUST be sourced from the
        // shared `WorkspacePool` so its index ends up under the daemon-shared
        // `~/.julie/indexes/` path and stays in the pool's membership set.
        // The old gate also required `(loaded_workspace_root_changed || force)`,
        // which meant a deferred session's first non-force primary init (what
        // `run_auto_indexing` does on the first request) fell through to the
        // project-local `JulieWorkspace::initialize` / `detect_and_load`
        // branch, leaving the pool empty while session state later marked the
        // workspace as attached. That pre-staged Finding #38's guard to trip
        // on every subsequent primary-scoped tool call.
        let use_pooled_rebind = self.workspace_pool.is_some()
            && self.daemon_db.is_some()
            && target_workspace_id.is_some();
        let rollback = if loaded_workspace_root_changed {
            Some(PrimarySwapRollback::capture(self).await)
        } else {
            None
        };

        // Handle force reinitialization vs normal initialization
        let workspace_result: Result<JulieWorkspace> = if force {
            info!("🔄 Force reinitialization requested - clearing derived data only");

            if loaded_workspace_root_changed {
                self.publish_loaded_workspace_swap_intent();
            }

            self.teardown_loaded_workspace(use_pooled_rebind).await;

            if use_pooled_rebind {
                let workspace_id = target_workspace_id.as_ref().expect("id checked above");
                Ok(self
                    .acquire_pooled_workspace_for_rebind(workspace_id, target_canonical.clone())
                    .await?)
            } else {
                // For force reindex, we only clear derived data, NOT the database (source of truth)
                let julie_dir = target_path.join(".julie");
                if julie_dir.exists() {
                    info!(
                        "🗑️ Clearing search index and cache for force reindex (preserving database)"
                    );

                    // 🔴 CRITICAL FIX: Only clear the PRIMARY workspace's index, NOT all workspaces!
                    // Reference workspaces must be preserved during force reindex

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
                                    "✅ Reference workspaces preserved (workspace isolation maintained)"
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
            if loaded_workspace_root_changed {
                self.publish_loaded_workspace_swap_intent();
                info!(
                    "Loaded workspace root changed - tearing down old workspace before replacement"
                );
                self.teardown_loaded_workspace(use_pooled_rebind).await;
            }

            if use_pooled_rebind {
                let workspace_id = target_workspace_id.as_ref().expect("id checked above");
                Ok(self
                    .acquire_pooled_workspace_for_rebind(workspace_id, target_canonical.clone())
                    .await?)
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

        let mut workspace: JulieWorkspace = match workspace_result {
            Ok(workspace) => workspace,
            Err(err) => {
                if let Some(rollback) = rollback {
                    if let Err(restore_err) = rollback.restore(self).await {
                        return Err(
                            err.context(format!("primary swap rollback failed: {restore_err:#}"))
                        );
                    }
                }
                return Err(err);
            }
        };

        // Start file watching BEFORE storing workspace (to avoid clone issue)
        if let Err(e) = workspace.start_file_watching().await {
            warn!("Failed to start file watching: {}", e);
        }

        let workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace.root.to_string_lossy())
                .ok();
        // `mark_attached` must reflect "this workspace came from the pool",
        // not "a pool exists". The old `self.workspace_pool.is_some()` form
        // silently lied when `use_pooled_rebind` was false — the workspace
        // went through `JulieWorkspace::initialize` (project-local path) but
        // session state still marked the id as attached, wedging later
        // primary-scoped calls with Finding #38's guard.
        self.publish_loaded_workspace_swap(workspace, workspace_id, use_pooled_rebind)
            .await;

        info!("Workspace initialization complete");
        Ok(())
    }

    /// Get workspace if initialized
    pub async fn get_workspace(&self) -> Result<Option<JulieWorkspace>> {
        let workspace_guard = self.workspace.read().await;
        Ok(workspace_guard.clone())
    }

    /// Pause the file watcher's event dispatch during catch-up indexing (Fix C part a).
    pub async fn pause_watcher(&self) {
        let guard = self.workspace.read().await;
        if let Some(ref ws) = *guard {
            ws.pause_file_watching();
        }
    }

    /// Resume the file watcher after catch-up indexing completes (Fix C part a).
    pub async fn resume_watcher(&self) {
        let guard = self.workspace.read().await;
        if let Some(ref ws) = *guard {
            ws.resume_file_watching();
        }
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

        // Backfill vector count from workspace's symbols.db
        if needs_vectors {
            let ws_guard = self.workspace.read().await;
            if let Some(ws) = ws_guard.as_ref() {
                if let Some(ref db_arc) = ws.db {
                    let count = {
                        let sym_db = db_arc.lock().unwrap_or_else(|p| p.into_inner());
                        sym_db.embedding_count().unwrap_or(0)
                    };
                    if count > 0 {
                        let _ = db.update_vector_count(ws_id, count);
                        info!(workspace_id = %ws_id, count, "Backfilled vector_count");
                    }
                }
            }
        }

        // Backfill embedding model from the shared embedding service
        if needs_model {
            if let Some(ref svc) = self.embedding_service {
                if let Some(provider) = svc.provider() {
                    let model = provider.device_info().model_name;
                    let _ = db.update_embedding_model(ws_id, &model);
                    info!(workspace_id = %ws_id, model, "Backfilled embedding_model");
                }
            }
        }
    }

    /// Record a completed tool call. Bumps in-memory atomics synchronously,
    /// then spawns async task for source_bytes lookup + SQLite write.
    pub(crate) fn record_tool_call(
        &self,
        tool_name: &str,
        duration: std::time::Duration,
        report: &ToolCallReport,
        workspace_snapshot: Option<&PrimaryWorkspaceBinding>,
    ) {
        let duration_us = duration.as_micros() as u64;
        let output_bytes = report.output_bytes;
        let workspace_id = workspace_snapshot
            .map(|binding| binding.workspace_id.clone())
            .or_else(|| self.current_workspace_id());
        let workspace_root = workspace_snapshot
            .map(|binding| binding.workspace_root.clone())
            .unwrap_or_else(|| self.current_workspace_root());

        // Bump in-memory atomics synchronously (source_bytes=0 for now, updated async)
        if let Some(kind) = ToolKind::from_name(tool_name) {
            self.session_metrics
                .record(kind, duration_us, 0, output_bytes);
        }

        // Write to per-project log (daemon mode only)
        if let Some(ref log) = self.project_log {
            log.tool_call(tool_name, duration.as_secs_f64() * 1000.0, output_bytes);
        }

        // Emit live-feed event to dashboard SSE subscribers (if any).
        if let Some(ref tx) = self.dashboard_tx {
            let _ = tx.send(DashboardEvent::ToolCall {
                tool_name: tool_name.to_string(),
                workspace: workspace_id.clone().unwrap_or_default(),
                duration_ms: duration.as_secs_f64() * 1000.0,
            });
        }

        // Offload source-bytes lookup + SQLite writes to the bounded background channel.
        // try_send drops the record on backpressure rather than spawning unbounded tasks.
        let metadata = report.metadata.to_string();
        let _ = self.metrics_tx.try_send(MetricsTask {
            workspace: self.workspace.clone(),
            workspace_pool: self.workspace_pool.clone(),
            current_workspace_root: workspace_root,
            session_metrics: self.session_metrics.clone(),
            session_id: self.session_metrics.session_id.clone(),
            tool_name: tool_name.to_string(),
            duration_ms: duration.as_secs_f64() * 1000.0,
            result_count: report.result_count,
            source_file_paths: report.source_file_paths.clone(),
            output_bytes,
            metadata_str: if metadata == "null" {
                None
            } else {
                Some(metadata)
            },
            daemon_db: self.daemon_db.clone(),
            workspace_id,
        });
    }

    /// Extract output byte count from a CallToolResult.
    fn output_bytes_from_result(result: &CallToolResult) -> u64 {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .map(|t| t.text.len() as u64)
            .sum()
    }

    /// Extract file paths from a CallToolResult's text content.
    fn extract_paths_from_result(result: &CallToolResult) -> Vec<String> {
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
        use std::sync::atomic::Ordering;

        // Fix G: prevent concurrent catch-up scans (e.g. two sessions connecting simultaneously).
        // Only the first caller proceeds; the second sees the flag set and bails out.
        if self
            .catchup_in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            info!("Catch-up auto-indexing already in progress for this workspace, skipping");
            return;
        }

        info!("🔍 Starting background auto-indexing check...");

        match run_primary_workspace_repair(self).await {
            Ok(Some(plan)) => {
                let reasons = plan
                    .reasons
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                info!(%reasons, "📚 Workspace needs indexing - starting auto-indexing");

                let index_tool = ManageWorkspaceTool {
                    operation: "index".to_string(),
                    path: None, // Use default workspace path
                    name: None,
                    workspace_id: None,
                    force: Some(false),
                    detailed: None,
                };

                if let Err(e) = index_tool.call_tool_with_options(self, false).await {
                    warn!(
                        "⚠️ Background auto-indexing failed: {} (use manage_workspace tool to retry)",
                        e
                    );
                } else {
                    info!("✅ Background auto-indexing completed successfully");
                }
            }
            Ok(None) => {
                info!("✅ Workspace already indexed - skipping auto-indexing");
            }
            Err(e) => {
                warn!("⚠️ Failed to check indexing status: {}", e);
            }
        }

        // Fix G: release the dedup flag so the next catch-up can proceed.
        self.catchup_in_progress.store(false, Ordering::Release);
    }

    // ========== Workspace Access Helpers ==========

    fn primary_workspace_db_path_from_binding(&self, binding: &PrimaryWorkspaceBinding) -> PathBuf {
        binding
            .workspace_root
            .join(".julie")
            .join("indexes")
            .join(&binding.workspace_id)
            .join("db")
            .join("symbols.db")
    }

    fn primary_workspace_tantivy_path_from_binding(
        &self,
        binding: &PrimaryWorkspaceBinding,
    ) -> PathBuf {
        binding
            .workspace_root
            .join(".julie")
            .join("indexes")
            .join(&binding.workspace_id)
            .join("tantivy")
    }

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
        let database = workspace.db.as_ref().cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "Database not available. Run manage_workspace(operation=\"index\") first."
            )
        })?;

        Ok(Some(PrimaryWorkspaceSnapshot {
            binding: binding.clone(),
            database,
            search_index: workspace.search_index.as_ref().cloned(),
            indexing_runtime: Some(Arc::clone(&workspace.indexing_runtime)),
        }))
    }

    async fn primary_workspace_snapshot_from_pool(
        &self,
        binding: &PrimaryWorkspaceBinding,
    ) -> Result<Option<PrimaryWorkspaceSnapshot>> {
        let Some(pool) = &self.workspace_pool else {
            return Ok(None);
        };
        let Some(workspace) = pool.get(&binding.workspace_id).await else {
            return Ok(None);
        };
        let database = workspace.db.as_ref().cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "Database not available. Run manage_workspace(operation=\"index\") first."
            )
        })?;

        Ok(Some(PrimaryWorkspaceSnapshot {
            binding: binding.clone(),
            database,
            search_index: workspace.search_index.as_ref().cloned(),
            indexing_runtime: Some(Arc::clone(&workspace.indexing_runtime)),
        }))
    }

    async fn primary_workspace_snapshot_from_binding_paths(
        &self,
        binding: &PrimaryWorkspaceBinding,
    ) -> Result<PrimaryWorkspaceSnapshot> {
        let db_path = self.primary_workspace_db_path_from_binding(binding);
        if !db_path.exists() {
            return Err(anyhow::anyhow!(
                "Database not found for workspace '{}' at {}",
                binding.workspace_id,
                db_path.display()
            ));
        }

        let database = {
            let cache = self.ref_db_cache.read().await;
            cache
                .get(&binding.workspace_id)
                .filter(|(cached_path, _)| *cached_path == db_path)
                .map(|(_, db)| Arc::clone(db))
        };

        let database = if let Some(database) = database {
            database
        } else {
            let db_path_for_open = db_path.clone();
            let database = tokio::task::spawn_blocking(move || {
                let db = SymbolDatabase::new(&db_path_for_open)?;
                Ok::<_, anyhow::Error>(Arc::new(std::sync::Mutex::new(db)))
            })
            .await??;

            let mut cache = self.ref_db_cache.write().await;
            cache.insert(
                binding.workspace_id.clone(),
                (db_path.clone(), Arc::clone(&database)),
            );
            database
        };

        let tantivy_path = self.primary_workspace_tantivy_path_from_binding(binding);
        let search_index = if tantivy_path.join("meta.json").exists() {
            Some(
                tokio::task::spawn_blocking(move || {
                    let configs = crate::search::LanguageConfigs::load_embedded();
                    let index = SearchIndex::open_with_language_configs(&tantivy_path, &configs)?;
                    Ok::<_, anyhow::Error>(Arc::new(std::sync::Mutex::new(index)))
                })
                .await??,
            )
        } else {
            None
        };

        Ok(PrimaryWorkspaceSnapshot {
            binding: binding.clone(),
            database,
            search_index,
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

        if let Some(snapshot) = self.primary_workspace_snapshot_from_pool(&binding).await? {
            return Ok(snapshot);
        }

        return if self.workspace_pool.is_some() {
            Err(anyhow::anyhow!(
                "Primary workspace '{}' is not attached in the daemon workspace pool",
                binding.workspace_id
            ))
        } else {
            self.primary_workspace_snapshot_from_binding_paths(&binding)
                .await
        };
    }

    pub(crate) async fn primary_database(&self) -> Result<Arc<std::sync::Mutex<SymbolDatabase>>> {
        Ok(self.primary_workspace_snapshot().await?.database)
    }

    pub(crate) async fn primary_database_and_search_index(
        &self,
    ) -> Result<(
        Arc<std::sync::Mutex<SymbolDatabase>>,
        Arc<std::sync::Mutex<SearchIndex>>,
    )> {
        let snapshot = self.primary_workspace_snapshot().await?;
        let search_index = snapshot.search_index.ok_or_else(|| {
            anyhow::anyhow!(
                "Search index not initialized. Run manage_workspace(operation=\"index\") first."
            )
        })?;

        Ok((snapshot.database, search_index))
    }

    /// Active workspace IDs for this session, sorted for stable output.
    pub async fn active_workspace_ids(&self) -> Vec<String> {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .active_workspace_ids()
    }

    pub async fn session_attached_workspace_ids(&self) -> Vec<String> {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .session_attached_workspace_ids()
    }

    /// Check whether a workspace ID is currently active for this session.
    pub async fn is_workspace_active(&self, workspace_id: &str) -> bool {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .is_workspace_active(workspace_id)
    }

    fn mark_workspace_active_internal(&self, workspace_id: &str) -> bool {
        let mut guard = self
            .session_workspace
            .write()
            .unwrap_or_else(|p| p.into_inner());
        guard.mark_workspace_active(workspace_id)
    }

    /// Add a workspace ID to this session's active set.
    #[cfg(test)]
    pub async fn mark_workspace_active(&self, workspace_id: &str) {
        self.mark_workspace_active_internal(workspace_id);
    }

    /// Activate a workspace for this session. Returns `true` if this was a new activation.
    #[cfg(test)]
    pub async fn activate_workspace(&self, workspace_id: &str) -> bool {
        self.mark_workspace_active_internal(workspace_id)
    }

    /// Load a workspace through the daemon pool, then mark it active for this session.
    pub async fn activate_workspace_with_root(
        &self,
        workspace_id: &str,
        workspace_root: PathBuf,
    ) -> Result<bool> {
        let attached_matches_target = self.was_workspace_attached_in_session(workspace_id).await;
        let already_active = self.is_workspace_active(workspace_id).await;

        if let Some(pool) = &self.workspace_pool {
            if !attached_matches_target {
                pool.get_or_init(workspace_id, workspace_root).await?;
                pool.sync_indexed_from_db(workspace_id).await;
                self.session_workspace
                    .write()
                    .unwrap_or_else(|p| p.into_inner())
                    .mark_workspace_attached(workspace_id.to_string());
            }
        }

        if already_active {
            return Ok(false);
        }

        Ok(self.mark_workspace_active_internal(workspace_id))
    }

    pub(crate) async fn workspace_storage_anchor(&self) -> Result<(PathBuf, Option<PathBuf>)> {
        let loaded_workspace = self.get_workspace().await?;
        let loaded_workspace_id = self.loaded_workspace_id();
        let current_workspace_id = self.current_workspace_id();

        if let Some(pool) = &self.workspace_pool {
            if let Some(current_id) = current_workspace_id.as_ref() {
                if let Some(anchor_workspace) = pool.get(current_id).await {
                    return Ok((
                        anchor_workspace.root.clone(),
                        anchor_workspace.index_root_override.clone(),
                    ));
                }

                // Path computation stays lenient when the rebound primary isn't
                // pool-resident yet — operations like manage_workspace(add) and
                // refresh routing need to compute target paths even before the
                // pool catches up. Strict pool-membership enforcement happens in
                // the connection-opening helpers (get_database_for_workspace and
                // get_search_index_for_workspace) where it actually matters.
                if loaded_workspace_id.as_deref() != Some(current_id.as_str()) {
                    return Ok((
                        self.current_workspace_root(),
                        loaded_workspace
                            .as_ref()
                            .and_then(|workspace| workspace.index_root_override.clone()),
                    ));
                }

                return Err(anyhow::anyhow!(
                    "Current primary workspace '{}' is not attached in the daemon workspace pool",
                    current_id
                ));
            }

            if let Some(loaded_id) = loaded_workspace_id.as_ref() {
                if let Some(anchor_workspace) = pool.get(loaded_id).await {
                    return Ok((
                        anchor_workspace.root.clone(),
                        anchor_workspace.index_root_override.clone(),
                    ));
                }

                return Err(anyhow::anyhow!(
                    "Current primary workspace '{}' is not attached in the daemon workspace pool",
                    loaded_id
                ));
            }

            if let Some(loaded_workspace) = loaded_workspace.as_ref() {
                return Ok((
                    self.current_workspace_root(),
                    loaded_workspace.index_root_override.clone(),
                ));
            }

            return Err(anyhow::anyhow!("Primary workspace not initialized"));
        }

        let loaded_workspace =
            loaded_workspace.ok_or_else(|| anyhow::anyhow!("Primary workspace not initialized"))?;

        if let Some(ref current_id) = current_workspace_id {
            if loaded_workspace_id.as_deref() != Some(current_id.as_str()) {
                return Ok((self.current_workspace_root(), None));
            }
        }

        if loaded_workspace_id.is_none() {
            return Err(anyhow::anyhow!(
                "Primary workspace identity unavailable during swap"
            ));
        }

        Ok((
            loaded_workspace.root.clone(),
            loaded_workspace.index_root_override.clone(),
        ))
    }

    /// Daemon-mode invariant guard: when accessing the *current primary*
    /// workspace's storage, the workspace must be attached in the workspace
    /// pool. Path computation stays lenient (see `workspace_storage_anchor`)
    /// because operations like `manage_workspace(add)` need to compute target
    /// paths before the pool catches up — but actually opening the DB or
    /// search index against a non-pool-resident primary indicates a rebind
    /// that bypassed `attach_daemon_primary_binding_if_needed` (see Findings
    /// #28/#29 in ROOTS_IMPL_REVIEW_NOTES.md).
    ///
    /// Secondary workspaces (workspace_id != current primary) are exempt:
    /// they are accessed lazily via on-disk paths and don't require a pool
    /// entry to function.
    async fn ensure_primary_pool_membership_for(&self, workspace_id: &str) -> Result<()> {
        let Some(pool) = self.workspace_pool.as_ref() else {
            return Ok(());
        };
        let Some(current_id) = self.current_workspace_id() else {
            return Ok(());
        };
        if current_id != workspace_id {
            return Ok(());
        }
        if pool.get(workspace_id).await.is_some() {
            return Ok(());
        }
        Err(anyhow::anyhow!(
            "Current primary workspace '{}' is not attached in the daemon workspace pool",
            workspace_id
        ))
    }

    pub(crate) async fn workspace_db_file_path_for(&self, workspace_id: &str) -> Result<PathBuf> {
        let (anchor_root, anchor_override) = self.workspace_storage_anchor().await?;
        Ok(if let Some(ref override_root) = anchor_override {
            override_root
                .parent()
                .map(|shared_indexes| {
                    shared_indexes
                        .join(workspace_id)
                        .join("db")
                        .join("symbols.db")
                })
                .unwrap_or_else(|| {
                    anchor_root
                        .join(".julie")
                        .join("indexes")
                        .join(workspace_id)
                        .join("db")
                        .join("symbols.db")
                })
        } else {
            anchor_root
                .join(".julie")
                .join("indexes")
                .join(workspace_id)
                .join("db")
                .join("symbols.db")
        })
    }

    pub(crate) async fn workspace_tantivy_dir_for(&self, workspace_id: &str) -> Result<PathBuf> {
        let (anchor_root, anchor_override) = self.workspace_storage_anchor().await?;
        Ok(if let Some(ref override_root) = anchor_override {
            override_root
                .parent()
                .map(|shared_indexes| shared_indexes.join(workspace_id).join("tantivy"))
                .unwrap_or_else(|| {
                    anchor_root
                        .join(".julie")
                        .join("indexes")
                        .join(workspace_id)
                        .join("tantivy")
                })
        } else {
            anchor_root
                .join(".julie")
                .join("indexes")
                .join(workspace_id)
                .join("tantivy")
        })
    }

    /// Get the database for a specific workspace by ID.
    ///
    /// In stdio mode: looks in `{project}/.julie/indexes/{workspace_id}/db/symbols.db`.
    /// In daemon mode: looks in `~/.julie/indexes/{workspace_id}/db/symbols.db`
    ///   (sibling of the primary workspace's index dir, not nested under it).
    pub async fn get_database_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Arc<std::sync::Mutex<SymbolDatabase>>> {
        self.ensure_primary_pool_membership_for(workspace_id)
            .await?;
        let db_path = self.workspace_db_file_path_for(workspace_id).await?;

        // Fast path: return cached connection for this session (M22).
        {
            let cache = self.ref_db_cache.read().await;
            if let Some((cached_path, db)) = cache.get(workspace_id) {
                if *cached_path == db_path {
                    return Ok(Arc::clone(db));
                }
            }
        }

        // In daemon mode, index_root_override points to ~/.julie/indexes/{primary_id}.
        // Reference workspaces are siblings: ~/.julie/indexes/{ref_id}/, not nested.
        if !db_path.exists() {
            return Err(anyhow::anyhow!(
                "Database not found for workspace '{}' at {}",
                workspace_id,
                db_path.display()
            ));
        }

        let db_path_for_open = db_path.clone();
        let db = tokio::task::spawn_blocking(move || {
            let db = SymbolDatabase::new(&db_path_for_open)?;
            Ok::<_, anyhow::Error>(Arc::new(std::sync::Mutex::new(db)))
        })
        .await??;

        // Populate cache for subsequent calls within this session.
        {
            let mut cache = self.ref_db_cache.write().await;
            cache.insert(workspace_id.to_string(), (db_path.clone(), Arc::clone(&db)));
        }

        Ok(db)
    }

    /// Get the search index for a specific workspace by ID.
    ///
    /// In stdio mode: looks in `{project}/.julie/indexes/{workspace_id}/tantivy/`.
    /// In daemon mode: looks in `~/.julie/indexes/{workspace_id}/tantivy/`.
    /// Returns `Ok(None)` if the index directory doesn't exist yet.
    pub async fn get_search_index_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Option<Arc<std::sync::Mutex<SearchIndex>>>> {
        self.ensure_primary_pool_membership_for(workspace_id)
            .await?;
        let tantivy_path = self.workspace_tantivy_dir_for(workspace_id).await?;
        if !tantivy_path.join("meta.json").exists() {
            return Ok(None);
        }

        tokio::task::spawn_blocking(move || {
            let configs = crate::search::LanguageConfigs::load_embedded();
            let index = SearchIndex::open_with_language_configs(&tantivy_path, &configs)?;
            Ok(Some(Arc::new(std::sync::Mutex::new(index))))
        })
        .await?
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
        // and reference workspaces resolve through workspace_registry.json rooted at the
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
                if let Some(entry) = registry.reference_workspaces.get(workspace_id) {
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

/// Tool router implementation - defines all available tools
#[tool_router]
impl JulieServerHandler {
    pub fn new_router() -> Self {
        // This is used by rmcp to create the tool router
        // We need to provide a way to construct with proper state
        panic!("Use JulieServerHandler::new(workspace_root) instead")
    }

    // ========== Search & Navigation Tools ==========

    #[tool(
        name = "fast_search",
        description = "Search code using text search with code-aware tokenization. Supports multi-word queries with AND/OR logic.",
        annotations(
            title = "Fast Code Search",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn fast_search(
        &self,
        Parameters(params): Parameters<FastSearchTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("⚡ Fast search: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = serde_json::json!({
            "query": params.query,
            "target": params.search_target,
        });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("fast_search failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "fast_search",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }

    #[tool(
        name = "fast_refs",
        description = "Find all references to a symbol across the codebase.",
        annotations(
            title = "Find References",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn fast_refs(
        &self,
        Parameters(params): Parameters<FastRefsTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("⚡ Fast find references: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = serde_json::json!({ "symbol": params.symbol });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("fast_refs failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "fast_refs",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }

    #[tool(
        name = "call_path",
        description = "Find one shortest call-graph path between two symbols (traverses calls, instantiations, and overrides only). Returns a compact hop list when a path exists, or found=false with a short diagnostic when it does not.",
        annotations(
            title = "Call Path",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn call_path(
        &self,
        Parameters(params): Parameters<crate::tools::navigation::CallPathTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("🧭 call_path: {} -> {}", params.from, params.to);
        let start = std::time::Instant::now();
        let workspace_snapshot = if params.workspace.as_deref().unwrap_or("primary") == "primary" {
            self.require_primary_workspace_binding().ok()
        } else {
            None
        };
        let metadata = serde_json::json!({
            "from": params.from,
            "to": params.to,
            "max_hops": params.max_hops,
            "workspace": params.workspace,
        });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("call_path failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "call_path",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }

    #[tool(
        name = "get_symbols",
        description = "Get symbols (functions, classes, etc.) from a file without reading full content. Requires exact file path — use deep_dive(symbol=...) if you don't know the path.",
        annotations(
            title = "Get File Symbols",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_symbols(
        &self,
        Parameters(params): Parameters<GetSymbolsTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("📋 Get symbols for file: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = serde_json::json!({
            "file": params.file_path,
            "mode": params.mode,
            "target": params.target,
        });
        let source_file_paths = vec![params.file_path.clone()];
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("get_symbols failed: {}", e), None))?;
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes: Self::output_bytes_from_result(&result),
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "get_symbols",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }

    #[tool(
        name = "deep_dive",
        description = "Investigate a symbol with progressive depth. Returns definition, references, children, and type info in a single call — tailored to the symbol's kind.\n\n**Always use BEFORE modifying or extending a symbol.** Replaces the common chain of fast_search → get_symbols → fast_refs → Read with a single call.",
        annotations(
            title = "Deep Dive Symbol Investigation",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn deep_dive(
        &self,
        Parameters(params): Parameters<DeepDiveTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("🔍 Deep dive: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = serde_json::json!({
            "symbol": params.symbol,
            "depth": params.depth,
        });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("deep_dive failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "deep_dive",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }

    // ========== Context Tools ==========

    #[tool(
        name = "get_context",
        description = "Get token-budgeted context for a concept or task. Returns relevant code subgraph with pivots (full code) and neighbors (signatures). Use at the start of a task for orientation.",
        annotations(
            title = "Get Context",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_context(
        &self,
        Parameters(params): Parameters<GetContextTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("📦 Get context: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = serde_json::json!({ "query": params.query });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("get_context failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "get_context",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }

    // ========== Refactoring Tools ==========

    #[tool(
        name = "rename_symbol",
        description = "Rename a symbol across the entire codebase with workspace-wide updates.",
        annotations(
            title = "Rename Symbol",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn rename_symbol(
        &self,
        Parameters(params): Parameters<RenameSymbolTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("✏️ Rename symbol: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = serde_json::json!({
            "old": params.old_name,
            "new": params.new_name,
            "dry_run": params.dry_run,
        });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("rename_symbol failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "rename_symbol",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }

    // ========== Workspace Management ==========

    #[tool(
        name = "manage_workspace",
        description = "Manage workspaces: index, open, register metadata, remove, list, refresh, stats, and health-check. For cross-workspace work, call open first, then pass the workspace_id to other tools.",
        annotations(
            title = "Manage Workspace",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn manage_workspace(
        &self,
        Parameters(params): Parameters<ManageWorkspaceTool>,
    ) -> Result<CallToolResult, McpError> {
        info!("🏗️ Managing workspace: {}", params.operation);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = serde_json::json!({ "operation": params.operation });
        let result = params.call_tool(self).await.map_err(|e| {
            McpError::internal_error(format!("manage_workspace failed: {}", e), None)
        })?;
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes: Self::output_bytes_from_result(&result),
            metadata,
            source_file_paths: Vec::new(),
        };
        self.record_tool_call(
            "manage_workspace",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }

    // ========== Editing Tools ==========

    #[tool(
        name = "edit_file",
        description = "Edit a file without reading it first. Provide old_text (fuzzy-matched via diff-match-patch) and new_text. Saves the full Read step that the built-in Edit tool requires. Use occurrence to control which match: \"first\" (default), \"last\", or \"all\". Always dry_run=true first to preview, then dry_run=false to apply.",
        annotations(
            title = "Edit File",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn edit_file(
        &self,
        Parameters(params): Parameters<crate::tools::editing::edit_file::EditFileTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!(
            "✏️ edit_file: {} (dry_run={})",
            params.file_path, params.dry_run
        );
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = serde_json::json!({
            "file": params.file_path,
            "occurrence": params.occurrence,
            "dry_run": params.dry_run,
        });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("edit_file failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "edit_file",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }

    #[tool(
        name = "rewrite_symbol",
        description = "Rewrite a symbol by name without reading the file first. Operations: replace_full, replace_body, replace_signature, insert_after, insert_before, add_doc. Julie resolves the symbol from the index, reparses the live file, and rewrites the live symbol span or a node-derived subspan. Always dry_run=true first to preview changes.",
        annotations(
            title = "Rewrite Symbol",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn rewrite_symbol(
        &self,
        Parameters(params): Parameters<crate::tools::editing::rewrite_symbol::RewriteSymbolTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!(
            "✏️ rewrite_symbol: {} {} (dry_run={})",
            params.operation, params.symbol, params.dry_run
        );
        let start = std::time::Instant::now();
        let workspace_snapshot = if params.workspace.as_deref().unwrap_or("primary") == "primary" {
            self.require_primary_workspace_binding().ok()
        } else {
            None
        };
        let metadata = serde_json::json!({
            "symbol": params.symbol,
            "operation": params.operation,
            "dry_run": params.dry_run,
            "workspace": params.workspace,
        });
        let result = params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("rewrite_symbol failed: {}", e), None))?;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "rewrite_symbol",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}

/// ServerHandler implementation
impl ServerHandler for JulieServerHandler {
    fn get_info(&self) -> ServerInfo {
        let server_info = Implementation::new("Julie", env!("CARGO_PKG_VERSION"))
            .with_title("Julie - Code Intelligence Server");

        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(server_info);

        if let Some(instructions) = self.load_agent_instructions() {
            info = info.with_instructions(instructions);
        }

        info
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ServerInfo, McpError> {
        self.record_client_roots_capability(request.capabilities.roots.is_some());
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }
        Ok(self.get_info())
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        if Self::tool_request_targets_primary(request.name.as_ref(), request.arguments.as_ref()) {
            self.ensure_primary_workspace_for_request(&context.peer)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
        }

        self.tool_router
            .call(ToolCallContext::new(self, request, context))
            .await
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

        let startup_hint = self.workspace_startup_hint();
        if crate::startup::startup_source_prefers_request_roots(startup_hint.source)
            && self.client_supports_workspace_roots()
        {
            self.mark_deferred_auto_index_pending(true);
            info!(
                startup_source = ?startup_hint.source.unwrap_or(WorkspaceStartupSource::Cwd),
                "Deferring auto-indexing until the first primary-scoped request resolves client roots"
            );
            return;
        }

        self.mark_deferred_auto_index_pending(false);

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

    async fn on_roots_list_changed(&self, _context: NotificationContext<RoleServer>) {
        self.mark_roots_dirty();
    }
}
