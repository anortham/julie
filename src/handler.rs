#[path = "handler/embedding_init.rs"]
pub(crate) mod embedding_init;
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

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::tool::{ToolCallContext, ToolRouter},
    model::{
        CallToolRequestParams, CallToolResult, Implementation, InitializeRequestParams,
        ListToolsResult, ServerCapabilities, ServerInfo, Tool,
    },
    service::{NotificationContext, Peer, RequestContext},
};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::daemon::session::{SessionLifecycleHandle, SessionLifecyclePhase};
use crate::daemon::workspace_session_attachment::WorkspaceSessionAttachment;
use crate::dashboard::state::DashboardEvent;

use self::session_workspace::{PrimaryWorkspaceBinding, SessionWorkspaceState};
use crate::database::SymbolDatabase;
use crate::search::{SearchIndex, SearchProjection};
use crate::workspace::JulieWorkspace;
use crate::workspace::mutation_gate::{MutationGuard, Registry as MutationGateRegistry};
use crate::workspace::startup_hint::WorkspaceStartupHint;
use crate::workspace::startup_hint::WorkspaceStartupSource;
use tokio::sync::RwLock;

use self::tool_metrics::{MetricsTask, run_metrics_writer};
use crate::tools::metrics::session::{SessionMetrics, extract_source_paths};
use crate::tools::workspace::commands::ManageWorkspaceOperation;

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
        let mut restored_workspace = match self.loaded_workspace_root.clone() {
            Some(workspace_root) => JulieWorkspace::detect_and_load(workspace_root)
                .await?
                .or(self.workspace),
            None => self.workspace,
        };

        // Gate the watcher on `!is_in_process_follower()`, NOT `is_leader()`:
        // stdio/daemon handlers use `LeadershipState::none()` (is_leader()==false)
        // but ARE the sole writer and must restore their watcher. Only an
        // in-process FOLLOWER must skip it (the leader owns writes).
        if handler.daemon_db.is_none() && !handler.is_in_process_follower() {
            if let Some(workspace) = restored_workspace.as_mut() {
                if workspace.config.incremental_updates {
                    workspace.initialize_file_watcher()?;
                    workspace.start_file_watching(true).await?;
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
        handler.publish_session_lifecycle_snapshot(phase, handler.current_workspace_id());
        Ok(())
    }
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

use crate::leadership::LeadershipState;

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
    /// In-memory spillover pages for graph-heavy tool outputs.
    pub(crate) spillover_store: Arc<crate::tools::spillover::store::SpilloverStore>,
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
    /// Set when on_initialized defers auto-indexing until the primary workspace
    /// is resolved from client roots. Consumed by the first successful bind.
    deferred_auto_index_pending: Arc<AtomicBool>,
    /// Single-flight gate for deferred auto-index repair so primary requests can
    /// wait behind an already-started background repair instead of racing it.
    deferred_auto_index_gate: Arc<tokio::sync::Mutex<()>>,
    /// Serializes concurrent roots resolution calls so only one task sends a
    /// ListRoots round-trip to the client at a time. The second waiter re-checks
    /// the binding state after acquiring the gate and short-circuits when the
    /// first caller already bound the workspace.
    roots_resolution_gate: Arc<tokio::sync::Mutex<()>>,
    /// In-flight claim for the F1 background repair spawn (codex pre-merge F-C).
    /// The leader's `call_tool` envelope spawns a non-cancellable repair task
    /// when `deferred_auto_index_pending` is set; without a claim, every
    /// concurrent in-process read would spawn its own task, and on persistent
    /// repair failure each queued task re-runs the (failed) repair in turn.
    /// A `compare_exchange(false → true)` claim ensures at most ONE repair task
    /// is outstanding per release cycle.
    deferred_auto_index_in_flight: Arc<AtomicBool>,
    /// Certification/replay handlers can index external repos without writing
    /// helper files such as `.julieignore` into those repos.
    pub(crate) suppress_workspace_file_writes: Arc<AtomicBool>,
    /// Optional daemon session lifecycle handle. Present when this handler is
    /// serving an IPC session through the daemon.
    session_lifecycle: Option<SessionLifecycleHandle>,
    /// Bounded channel sender for background metrics writes (M03).
    /// A single background task drains this; try_send drops on backpressure
    /// rather than spawning unbounded tasks.
    metrics_tx: tokio::sync::mpsc::Sender<MetricsTask>,
    /// Cache for non-primary workspace DB connections, keyed by workspace_id with
    /// the resolved physical db path so root-anchor changes in stdio do not reuse
    /// stale handles across different `.julie/indexes/...` trees.
    ref_db_cache: Arc<RwLock<HashMap<String, (PathBuf, Arc<std::sync::Mutex<SymbolDatabase>>)>>>,
    /// Broadcast sender for dashboard live-feed events. None in stdio/test mode.
    dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    /// Mutation-gate registry used by workspace writer paths in this handler.
    mutation_gate_registry: Arc<MutationGateRegistry>,
    /// In-process leadership state. Holds the OS-level advisory lock when this
    /// handler is the elected workspace leader (see `new_in_process`). Wrapped
    /// in `Arc` so the `Clone` derive works across sessions.
    pub(crate) leadership: Arc<LeadershipState>,
    /// Embedding provider injected by `new_in_process`. When `Some`, takes
    /// priority over `embedding_service` and the per-workspace provider.
    injected_embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
    /// Index root override for in-process sessions (T8/F2).  When `Some`, the
    /// non-pool branch of `initialize_workspace_with_force` routes db/tantivy
    /// to the daemon shared directory (`~/.julie/indexes/{ws}/`) so storage
    /// and the leader lock share one inode tree.
    pub(crate) in_process_index_root: Option<PathBuf>,
    /// Keeps isolated temp roots alive for test-only handlers.
    #[cfg(test)]
    test_temp_guard: Option<Arc<tempfile::TempDir>>,
}

impl JulieServerHandler {
    pub(crate) fn canonicalize_workspace_path(path: PathBuf) -> PathBuf {
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
        self.rebind_current_primary(binding.workspace_id.clone(), binding.workspace_root.clone());
    }

    fn mark_deferred_auto_index_pending(&self, pending: bool) {
        use std::sync::atomic::Ordering;

        self.deferred_auto_index_pending
            .store(pending, Ordering::Release);
    }

    /// Claim the single in-flight deferred-repair slot (codex pre-merge F-C).
    ///
    /// Returns `true` for exactly ONE caller per release cycle — the caller that
    /// flips the flag `false → true` owns the slot and must spawn the background
    /// repair, then call [`Self::release_deferred_repair_slot`] when the spawned
    /// task finishes. Concurrent callers lose the `compare_exchange` and get
    /// `false`, so they skip spawning. This bounds a persistently-failing repair
    /// to one outstanding task at a time instead of one per concurrent in-process
    /// read.
    pub(crate) fn try_claim_deferred_repair_slot(&self) -> bool {
        use std::sync::atomic::Ordering;

        self.deferred_auto_index_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Release the in-flight deferred-repair slot claimed by
    /// [`Self::try_claim_deferred_repair_slot`]. Call exactly once after the
    /// spawned repair task completes (success or failure), so the next pending
    /// cycle can spawn a fresh task.
    pub(crate) fn release_deferred_repair_slot(&self) {
        use std::sync::atomic::Ordering;

        self.deferred_auto_index_in_flight
            .store(false, Ordering::Release);
    }

    async fn complete_deferred_auto_index_if_needed(&self) -> Result<()> {
        use std::sync::atomic::Ordering;

        // In-process FOLLOWER: skip all writing recovery. The leader owns
        // SQLite/Tantivy writes; followers are pure readers. Running repair
        // here would race the leader (Risk #2, T9/Part A). Daemon/stdio
        // handlers use `LeadershipState::none()` so is_in_process_follower()
        // is false — they take the full repair path below unchanged.
        if self.is_in_process_follower() {
            return Ok(());
        }

        let _deferred_guard = self.deferred_auto_index_gate.lock().await;
        if !self
            .deferred_auto_index_pending
            .swap(false, Ordering::AcqRel)
        {
            return Ok(());
        }

        match crate::startup::run_primary_workspace_repair(self).await {
            Ok(Some(plan)) => {
                let reasons = plan
                    .reasons
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                info!(%reasons, "Completed deferred auto-index repair");
                Ok(())
            }
            Ok(None) => {
                *self.is_indexed.write().await = true;
                info!("Deferred auto-index repair found workspace already indexed");
                Ok(())
            }
            Err(err) => {
                self.mark_deferred_auto_index_pending(true);
                Err(err.context("deferred auto-index repair failed"))
            }
        }
    }

    async fn attach_daemon_primary_binding_if_needed(
        &self,
        binding: &PrimaryWorkspaceBinding,
    ) -> Result<()> {
        self.session_attachment()
            .attach_workspace_once(&binding.workspace_id, binding.workspace_root.clone())
            .await?;
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
        self.reject_sensitive_cwd_startup_hint()?;
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

    async fn ensure_primary_workspace_for_request(
        &self,
        peer: &Peer<RoleServer>,
        complete_deferred_auto_index: bool,
    ) -> Result<()> {
        let existing_binding = match self.require_primary_binding() {
            Ok(binding) => Some(binding),
            Err(err) if self.is_primary_workspace_swap_in_progress() => return Err(err),
            Err(_) => None,
        };
        let prefers_request_roots = self.request_prefers_client_roots();

        if !prefers_request_roots {
            if self.roots_dirty() || existing_binding.is_none() {
                self.reconcile_primary_workspace_to_startup_hint().await?;
                if complete_deferred_auto_index {
                    self.complete_deferred_auto_index_if_needed().await?;
                }
            }
            return Ok(());
        }

        if existing_binding.is_some() && !self.roots_dirty() {
            if complete_deferred_auto_index {
                self.complete_deferred_auto_index_if_needed().await?;
            }
            return Ok(());
        }

        if self.client_supports_workspace_roots() {
            // Serialize concurrent ListRoots round-trips. Without the gate,
            // two tasks (e.g. the `on_initialized` eager probe and the first
            // incoming tool call) can both see an unbound state, both send
            // ListRoots to the client, and then one hangs because the client
            // only handles a single request. The gate ensures only one task
            // performs the round-trip; the second waiter re-checks the
            // binding state after acquiring and short-circuits when it finds
            // the workspace already bound.
            //
            // The gate is held from the re-check through
            // `reconcile_primary_workspace_roots` so the binding write is
            // visible to the next waiter. It is released before the expensive
            // `complete_deferred_auto_index_if_needed` call.
            enum GateOutcome {
                AlreadyBound,
                Reconciled(bool),
                Failed(anyhow::Error),
            }
            let gate_outcome: GateOutcome = {
                let _gate = self.roots_resolution_gate.lock().await;
                // Re-check inside the gate — the previous holder may have
                // already bound the workspace.
                if self.require_primary_binding().is_ok() && !self.roots_dirty() {
                    GateOutcome::AlreadyBound
                } else {
                    match self.list_roots_from_peer(peer).await {
                        Ok(roots) => match self.reconcile_primary_workspace_roots(roots).await {
                            Ok(reconciled) => GateOutcome::Reconciled(reconciled),
                            Err(err) => GateOutcome::Failed(err),
                        },
                        Err(err) => GateOutcome::Failed(err),
                    }
                }
            };

            match gate_outcome {
                GateOutcome::AlreadyBound => {
                    if complete_deferred_auto_index {
                        self.complete_deferred_auto_index_if_needed().await?;
                    }
                    return Ok(());
                }
                GateOutcome::Reconciled(true) => {
                    if complete_deferred_auto_index {
                        self.complete_deferred_auto_index_if_needed().await?;
                    }
                    return Ok(());
                }
                GateOutcome::Reconciled(false) => {
                    self.reconcile_primary_workspace_to_startup_hint().await?;
                    if complete_deferred_auto_index {
                        self.complete_deferred_auto_index_if_needed().await?;
                    }
                    return Ok(());
                }
                GateOutcome::Failed(err) => {
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
                                if complete_deferred_auto_index {
                                    self.complete_deferred_auto_index_if_needed().await?;
                                }
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        self.reject_sensitive_cwd_startup_hint()?;
        let binding = self.primary_binding_for_root(self.workspace_startup_hint().path)?;
        self.attach_daemon_primary_binding_if_needed(&binding)
            .await?;
        self.activate_primary_binding(&binding);
        if complete_deferred_auto_index {
            self.complete_deferred_auto_index_if_needed().await?;
        }
        Ok(())
    }

    fn manage_workspace_primary_index_request(
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> bool {
        ManageWorkspaceOperation::primary_index_request(arguments)
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
            | "get_context" | "blast_radius" | "rename_symbol" | "edit_file" => {
                workspace_is_primary
            }
            "manage_workspace" => Self::manage_workspace_request_targets_primary(arguments),
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
            session_workspace: Arc::new(StdRwLock::new(session_workspace)),
            workspace: Arc::new(RwLock::new(None)),
            is_indexed: Arc::new(RwLock::new(false)),
            indexing_status: Arc::new(IndexingStatus::new()),
            session_metrics: Arc::new(SessionMetrics::new()),
            spillover_store: Arc::new(crate::tools::spillover::store::SpilloverStore::default()),
            embedding_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            tool_router: Self::tool_router(),
            project_log: None,
            daemon_db: None,
            workspace_id: Arc::new(StdRwLock::new(None)),
            embedding_service: None,
            restart_pending: None,
            deferred_auto_index_pending: Arc::new(AtomicBool::new(false)),
            deferred_auto_index_gate: Arc::new(tokio::sync::Mutex::new(())),
            roots_resolution_gate: Arc::new(tokio::sync::Mutex::new(())),
            deferred_auto_index_in_flight: Arc::new(AtomicBool::new(false)),
            suppress_workspace_file_writes: Arc::new(AtomicBool::new(false)),
            session_lifecycle: None,
            metrics_tx,
            ref_db_cache: Arc::new(RwLock::new(HashMap::new())),
            dashboard_tx: None,
            mutation_gate_registry: Arc::clone(MutationGateRegistry::global()),
            leadership: Arc::new(LeadershipState::none()),
            injected_embedding_provider: None,
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
    /// - `db: Arc<Mutex<SqliteDB>>` and `search_index: Arc<Mutex<SearchIndex>>` are
    ///   shared (Arc clone). This is the whole point: multiple sessions hit one db.
    /// - `watcher` is `None` in the clone (leader manages the file watcher).
    /// - `embedding_provider` is set to `None` (shared via EmbeddingService).
    pub async fn new_with_shared_workspace(
        workspace: Arc<JulieWorkspace>,
        workspace_root: PathBuf,
        daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
        workspace_id: Option<String>,
        embedding_service: Option<Arc<crate::daemon::embedding_service::EmbeddingService>>,
        restart_pending: Option<Arc<std::sync::atomic::AtomicBool>>,
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
            embedding_service,
            restart_pending,
            dashboard_tx,
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
        let initial_workspace_id = workspace_id.clone();
        if let Some(ref id) = workspace_id {
            if !prefer_request_roots {
                session_workspace.bind_primary(id.clone(), workspace_root.clone());
            }
        }

        // Create per-project logger for daemon mode
        let project_log = Some(Arc::new(crate::daemon::project_log::ProjectLog::new(
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
            spillover_store: Arc::new(crate::tools::spillover::store::SpilloverStore::default()),
            embedding_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            tool_router: Self::tool_router(),
            project_log,
            daemon_db,
            workspace_id: Arc::new(StdRwLock::new(workspace_id)),
            embedding_service,
            restart_pending,
            deferred_auto_index_pending: Arc::new(AtomicBool::new(false)),
            deferred_auto_index_gate: Arc::new(tokio::sync::Mutex::new(())),
            roots_resolution_gate: Arc::new(tokio::sync::Mutex::new(())),
            deferred_auto_index_in_flight: Arc::new(AtomicBool::new(false)),
            suppress_workspace_file_writes: Arc::new(AtomicBool::new(false)),
            session_lifecycle: None,
            metrics_tx,
            ref_db_cache: Arc::new(RwLock::new(HashMap::new())),
            dashboard_tx,
            mutation_gate_registry: Arc::clone(MutationGateRegistry::global()),
            leadership: Arc::new(LeadershipState::none()),
            injected_embedding_provider: None,
            in_process_index_root: None,
            #[cfg(test)]
            test_temp_guard: None,
        };

        if let Some(id) = initial_workspace_id {
            handler
                .session_attachment()
                .attach_workspace_once(&id, workspace_root)
                .await?;
        }

        Ok(handler)
    }

    pub async fn new_deferred_daemon_startup_hint(
        workspace_startup_hint: WorkspaceStartupHint,
        daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
        embedding_service: Option<Arc<crate::daemon::embedding_service::EmbeddingService>>,
        restart_pending: Option<Arc<std::sync::atomic::AtomicBool>>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    ) -> Result<Self> {
        Self::new_deferred_daemon_startup_hint_with_project_log(
            workspace_startup_hint,
            daemon_db,
            embedding_service,
            restart_pending,
            dashboard_tx,
            true,
        )
        .await
    }

    pub async fn new_deferred_daemon_startup_hint_without_project_log(
        workspace_startup_hint: WorkspaceStartupHint,
        daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
        embedding_service: Option<Arc<crate::daemon::embedding_service::EmbeddingService>>,
        restart_pending: Option<Arc<std::sync::atomic::AtomicBool>>,
        dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    ) -> Result<Self> {
        Self::new_deferred_daemon_startup_hint_with_project_log(
            workspace_startup_hint,
            daemon_db,
            embedding_service,
            restart_pending,
            dashboard_tx,
            false,
        )
        .await
    }

    async fn new_deferred_daemon_startup_hint_with_project_log(
        workspace_startup_hint: WorkspaceStartupHint,
        daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
        embedding_service: Option<Arc<crate::daemon::embedding_service::EmbeddingService>>,
        restart_pending: Option<Arc<std::sync::atomic::AtomicBool>>,
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
            spillover_store: Arc::new(crate::tools::spillover::store::SpilloverStore::default()),
            embedding_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            tool_router: Self::tool_router(),
            project_log: if enable_project_writes {
                Some(Arc::new(crate::daemon::project_log::ProjectLog::new(
                    &workspace_root,
                )))
            } else {
                None
            },
            daemon_db,
            workspace_id: Arc::new(StdRwLock::new(None)),
            embedding_service,
            restart_pending,
            deferred_auto_index_pending: Arc::new(AtomicBool::new(false)),
            deferred_auto_index_gate: Arc::new(tokio::sync::Mutex::new(())),
            roots_resolution_gate: Arc::new(tokio::sync::Mutex::new(())),
            deferred_auto_index_in_flight: Arc::new(AtomicBool::new(false)),
            suppress_workspace_file_writes: Arc::new(AtomicBool::new(!enable_project_writes)),
            session_lifecycle: None,
            metrics_tx,
            ref_db_cache: Arc::new(RwLock::new(HashMap::new())),
            dashboard_tx,
            mutation_gate_registry: Arc::clone(MutationGateRegistry::global()),
            leadership: Arc::new(LeadershipState::none()),
            injected_embedding_provider: None,
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
    /// * `leader` — result of the workspace leader election (T2 primitive).
    ///   Pass `LeadershipState::leader(guard)` when this process won the lock;
    ///   pass `LeadershipState::follower()` when another process won (write-refusing reader);
    ///   pass `LeadershipState::none()` for handlers not in the in-process model.
    /// * `index_root` — when `Some`, `initialize_workspace_with_force` routes
    ///   db/tantivy to this directory so the leader lock and storage share one
    ///   inode tree (T8/F2).  Pass `None` for the traditional project-local path.
    pub async fn new_in_process(
        startup_hint: WorkspaceStartupHint,
        embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
        leader: LeadershipState,
        index_root: Option<PathBuf>,
    ) -> Result<Self> {
        // Build on top of the deferred-startup path, which wires up everything
        // except the project-log and sets startup_hint on the session state.
        let mut handler = Self::new_deferred_daemon_startup_hint_with_project_log(
            startup_hint,
            /*daemon_db=*/ None,
            /*embedding_service=*/ None,
            /*restart_pending=*/ None,
            /*dashboard_tx=*/ None,
            /*enable_project_writes=*/ true,
        )
        .await?;

        // Override the leadership, injected-provider, and index-root fields.
        handler.leadership = Arc::new(leader);
        handler.injected_embedding_provider = embedding_provider;
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

    // Orphaned by the Phase 3d.2b deletion of `src/daemon/mcp_session.rs` (the
    // per-HTTP-session wiring that attached lifecycle handles and drove the
    // serving/closing phase transitions). The in-process server does not yet
    // publish session-lifecycle phases; the `SessionLifecycleHandle` plumbing is
    // torn out in the Phase 3d.3 session-lifecycle/dashboard rewrite.
    #[allow(dead_code)]
    pub(crate) fn attach_session_lifecycle(&mut self, session_lifecycle: SessionLifecycleHandle) {
        let phase = self.current_session_lifecycle_phase();
        session_lifecycle.set_phase(phase);
        session_lifecycle.set_current_workspace(self.current_workspace_id());
        self.session_lifecycle = Some(session_lifecycle);
    }

    fn current_session_lifecycle_phase(&self) -> SessionLifecyclePhase {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .lifecycle_phase()
    }

    fn publish_session_lifecycle_snapshot(
        &self,
        phase: SessionLifecyclePhase,
        current_workspace_id: Option<String>,
    ) {
        if let Some(session_lifecycle) = &self.session_lifecycle {
            session_lifecycle.set_phase(phase);
            session_lifecycle.set_current_workspace(current_workspace_id);
        }
    }

    fn update_session_workspace<R>(
        &self,
        update: impl FnOnce(&mut SessionWorkspaceState) -> R,
    ) -> R {
        let (result, phase, current_workspace_id) = {
            let mut state = self
                .session_workspace
                .write()
                .unwrap_or_else(|p| p.into_inner());
            let result = update(&mut state);
            let phase = state.lifecycle_phase();
            let current_workspace_id = state.current_workspace_id();
            (result, phase, current_workspace_id)
        };
        self.publish_session_lifecycle_snapshot(phase, current_workspace_id);
        result
    }

    // See `attach_session_lifecycle`: orphaned with the daemon mcp_session
    // deletion (3d.2b), removed in the 3d.3 session-lifecycle/dashboard rewrite.
    // `mark_session_serving` stays reachable in test builds via
    // `mark_session_serving_for_test`.
    #[allow(dead_code)]
    pub(crate) fn mark_session_serving(&self) {
        self.update_session_workspace(|state| state.mark_serving());
    }

    #[allow(dead_code)]
    pub(crate) fn mark_session_closing(&self) {
        self.update_session_workspace(|state| state.mark_closing());
    }

    /// Returns `true` when this handler holds the OS-level workspace leader lock.
    ///
    /// The lock is acquired during `new_in_process` via the T2 leader-election
    /// primitive (`DaemonLockGuard::try_acquire`). All existing constructors
    /// (`new`, `new_with_shared_workspace_startup_hint`, etc.) return `false`.
    pub fn is_leader(&self) -> bool {
        self.leadership.is_leader()
    }

    /// Returns `true` when this handler is a read-only follower in an in-process
    /// leader election (created via `new_in_process` with `LeadershipState::follower()`).
    ///
    /// D1 write-mutating operations (index, register, remove, refresh, editing
    /// tools) MUST be refused on followers to prevent cross-process
    /// SQLite/Tantivy data races (T7, Risk #2).
    ///
    /// Unlike `!is_leader()`, this gate does NOT fire for regular pre-3c
    /// constructors (daemon mode, stdio mode) — those use `LeadershipState::none()`
    /// and are not subject to write-refusal gating.
    pub fn is_in_process_follower(&self) -> bool {
        self.leadership.is_follower()
    }

    /// Whether this handler may rebuild a recreated-empty Tantivy projection on
    /// the search-index OPEN/read path (`repair_recreated_open_if_needed`, which
    /// runs `clear_all` + `apply_documents` — a Tantivy WRITE).
    ///
    /// In-process followers (losers) MUST NOT — that would make a non-leader a
    /// Tantivy writer, violating the single-writer invariant (T7, Risk #2) that
    /// the cutover establishes. A follower instead serves the (possibly empty)
    /// opened index read-only and relies on the leader's rebuild becoming visible
    /// via the Tantivy poll-reload (~500ms; the part-(d) freshness-only degrade).
    ///
    /// Leaders and daemon/stdio handlers (`LeadershipState::none()`) repair as
    /// before — `none()` returns `true` here, so the pre-3c paths are unchanged.
    pub(crate) fn may_repair_recreated_projection(&self) -> bool {
        !self.is_in_process_follower()
    }

    /// Returns `true` when this handler is participating in an in-process
    /// leader election (either leader or follower). `false` for all pre-3c
    /// constructors (daemon mode, stdio mode — `LeadershipState::none()`).
    ///
    /// Gates the F1 bounded read envelope in `call_tool`: only in-process
    /// handlers get the bounded envelope; daemon/stdio take the existing
    /// path byte-for-byte unchanged.
    pub fn is_in_process(&self) -> bool {
        self.leadership.is_in_process()
    }

    /// Whether a request-time primary resolution should prefer the client's
    /// `list_roots` over the startup hint.
    ///
    /// **In-process handlers always return `false`** (codex 3c.2 F-A): the
    /// leader lock + index storage are acquired at STARTUP from the
    /// hint-derived `workspace_id`, before any client roots are known. If the
    /// binding then rebound to a different `list_roots` root, the lock/storage
    /// (hint-keyed) and the binding (client-root-keyed) would diverge — two
    /// processes launched from different cwds but reporting the same client
    /// root would each win a *different* lock and maintain divergent index
    /// trees for one logical workspace. Pinning the binding to the canonical
    /// startup hint keeps lock id == storage id == binding id.
    ///
    /// Daemon/stdio (`LeadershipState::none()`) keep the source-driven
    /// behavior unchanged, so multi-root clients still rebind there.
    pub(crate) fn request_prefers_client_roots(&self) -> bool {
        !self.is_in_process()
            && crate::startup::startup_source_prefers_request_roots(
                self.workspace_startup_hint().source,
            )
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

    pub(crate) async fn acquire_mutation_gate<'a>(
        &'a self,
        workspace_id: &'a str,
    ) -> MutationGuard<'a> {
        self.mutation_gate_registry.acquire(workspace_id).await
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

    pub async fn detach_workspace_for_session(&self, workspace_id: &str) -> Result<bool> {
        self.session_attachment()
            .detach_workspace_once(workspace_id)
            .await
    }

    fn session_attachment(&self) -> WorkspaceSessionAttachment {
        WorkspaceSessionAttachment::new(
            self.daemon_db.as_ref().map(Arc::clone),
            Arc::clone(&self.session_workspace),
        )
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

        let attached_workspace_id = workspace_id.clone().filter(|_| mark_attached);
        self.update_session_workspace(move |session_workspace| {
            if let Some(workspace_id) = workspace_id {
                session_workspace.bind_primary(workspace_id, workspace_root);
            }

            session_workspace.complete_primary_swap();
        });
        if let Some(workspace_id) = attached_workspace_id {
            self.session_attachment()
                .mark_workspace_attached(workspace_id);
        }
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

    /// Get the embedding provider, preferring daemon shared service over per-workspace.
    pub(crate) async fn embedding_provider(
        &self,
    ) -> Option<Arc<dyn crate::embeddings::EmbeddingProvider>> {
        // In-process mode: injected provider takes priority.
        if let Some(ref p) = self.injected_embedding_provider {
            return Some(Arc::clone(p));
        }
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
        // In-process mode: workspace pool is gone; every session initializes
        // its workspace directly via `JulieWorkspace::initialize`. The old
        // `use_pooled_rebind = pool.is_some()` gate is collapsed to false.
        let use_pooled_rebind = false;
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

            if let Some(index_root) = &self.in_process_index_root {
                // In-process force reindex (codex pre-merge F-B): the leader's
                // db/tantivy AND the held `leader.lock` all live under the shared
                // `index_root` (~/.julie/indexes/{ws}/). The non-force branch
                // already redirects storage there (T8/F2); the force branch MUST
                // too, or a force reindex (incl. an auto-triggered semantic-engine
                // version bump) would silently rebuild project-local storage while
                // the leader lock sits in the daemon path — breaking the F2 inode
                // coupling and letting a second process "lead" the same workspace.
                //
                // Clear ONLY db/ and tantivy/ under index_root (a full rebuild
                // follows via index_workspace_files force=true). NEVER remove
                // `index_root` wholesale: that would unlink the leader.lock file
                // this process holds on Unix, letting another process acquire a
                // duplicate leader lock.
                for derived in ["db", "tantivy"] {
                    let dir = index_root.join(derived);
                    if dir.exists() {
                        if let Err(e) = std::fs::remove_dir_all(&dir) {
                            warn!(
                                "Failed to clear in-process {} for force reindex at {}: {}",
                                derived,
                                dir.display(),
                                e
                            );
                        }
                    }
                }
                info!(
                    "🗑️ Cleared in-process db+tantivy for force reindex under {} (leader.lock preserved)",
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
            if loaded_workspace_root_changed {
                self.publish_loaded_workspace_swap_intent();
                info!(
                    "Loaded workspace root changed - tearing down old workspace before replacement"
                );
                self.teardown_loaded_workspace(use_pooled_rebind).await;
            }

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

        // Start file watching BEFORE storing workspace (to avoid clone issue).
        // Gate on `!is_in_process_follower()`: stdio/daemon (none()) and the
        // in-process leader watch; only an in-process follower skips (it is a
        // read-only process and must not race the leader's writes).
        if let Err(e) = workspace
            .start_file_watching(!self.is_in_process_follower())
            .await
        {
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
        let snapshot = self
            .primary_workspace_snapshot_from_binding_paths(binding)
            .await
            .ok()?;
        let database = Arc::clone(&snapshot.database);
        let paths = source_file_paths.to_vec();
        tokio::task::spawn_blocking(move || {
            let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
            let db = database
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            db.get_total_file_sizes(&path_refs).ok()
        })
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
        let db_path = self
            .workspace_db_file_path_for(&binding.workspace_id)
            .await?;
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

        let tantivy_path = self
            .workspace_tantivy_dir_for(&binding.workspace_id)
            .await?;
        let search_index = if tantivy_path.join("meta.json").exists() {
            let workspace_id = binding.workspace_id.clone();
            let database_for_projection = Arc::clone(&database);
            let indexing_status = Arc::clone(&self.indexing_status);
            // T7 single-writer gate (codex 3c.3 pre-merge): recreated-open repair
            // is a Tantivy WRITE. In-process followers must skip it — the leader
            // owns the rebuild; the follower picks it up via the poll-reload.
            let may_repair = self.may_repair_recreated_projection();
            Some(
                tokio::task::spawn_blocking(move || {
                    let configs = crate::search::LanguageConfigs::load_embedded();
                    let open_outcome =
                        SearchIndex::open_with_language_configs_outcome(&tantivy_path, &configs)?;
                    let repair_required = open_outcome.repair_required();
                    let index = open_outcome.into_index();

                    if repair_required && may_repair {
                        warn!(
                            "Tantivy index for workspace '{}' at {} was recreated empty during open; rebuilding projection from canonical SQLite state",
                            workspace_id,
                            tantivy_path.display()
                        );

                        let mut db = database_for_projection
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        let projection = SearchProjection::tantivy(workspace_id.clone());
                        projection.repair_recreated_open_if_needed(
                            &mut db,
                            &index,
                            repair_required,
                            Some(&indexing_status.search_ready),
                        )?;
                    } else if repair_required {
                        debug!(
                            "Tantivy index for workspace '{}' at {} was recreated empty during open; in-process follower skipping projection repair (leader owns the rebuild)",
                            workspace_id,
                            tantivy_path.display()
                        );
                    }

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

        return self
            .primary_workspace_snapshot_from_binding_paths(&binding)
            .await;
    }

    pub(crate) async fn primary_database(&self) -> Result<Arc<std::sync::Mutex<SymbolDatabase>>> {
        Ok(self.primary_workspace_snapshot().await?.database)
    }

    /// Pool-aware accessor for the primary workspace database.
    ///
    /// Returns an owned `SymbolDatabase` wrapping a pooled connection (or, in
    /// stdio mode, a fresh owned connection). Multiple sessions or in-flight
    /// requests can hold their own connections concurrently — unlike
    /// [`primary_database`], which returns a shared `Arc<Mutex<>>` that
    /// serializes all callers.
    ///
    /// Prefer this method for new code. Migration from `primary_database`
    /// proceeds incrementally; see Task A2.2c-followup.
    pub(crate) async fn primary_pooled_database(&self) -> Result<SymbolDatabase> {
        let workspace_id = self.require_primary_workspace_identity()?;
        self.get_pooled_database_for_workspace(&workspace_id).await
    }

    pub(crate) async fn primary_pooled_database_and_search_index(
        &self,
    ) -> Result<(SymbolDatabase, Arc<std::sync::Mutex<SearchIndex>>)> {
        let snapshot = self.primary_workspace_snapshot().await?;
        let search_index = snapshot.search_index.ok_or_else(|| {
            anyhow::anyhow!(
                "Search index not initialized. Run manage_workspace(operation=\"index\") first."
            )
        })?;
        let database = self
            .get_pooled_database_for_workspace(&snapshot.binding.workspace_id)
            .await?;

        Ok((database, search_index))
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

        if !attached_matches_target {
            self.session_attachment()
                .attach_workspace_once(workspace_id, workspace_root)
                .await?;
        }

        if already_active {
            return Ok(false);
        }

        Ok(self.mark_workspace_active_internal(workspace_id))
    }

    pub async fn switch_primary_workspace_with_root(
        &self,
        workspace_id: &str,
        workspace_root: PathBuf,
    ) -> Result<bool> {
        let previous_primary_id = self
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .primary_binding()
            .map(|binding| binding.workspace_id);
        let target_workspace_id = workspace_id.to_string();
        let activated = self
            .activate_workspace_with_root(&target_workspace_id, workspace_root.clone())
            .await?;

        self.update_session_workspace(move |session_workspace| {
            let opened_workspace_id = target_workspace_id.clone();
            session_workspace.bind_primary(target_workspace_id, workspace_root);
            if let Some(previous_primary_id) = previous_primary_id {
                if previous_primary_id != opened_workspace_id {
                    session_workspace.mark_workspace_active(previous_primary_id);
                }
            }
        });

        Ok(activated)
    }

    pub(crate) async fn workspace_storage_anchor(&self) -> Result<(PathBuf, Option<PathBuf>)> {
        let loaded_workspace = self.get_workspace().await?;
        let loaded_workspace_id = self.loaded_workspace_id();
        let current_workspace_id = self.current_workspace_id();

        // Deferred startup: no workspace loaded yet (e.g. handler constructed
        // before the primary binding is resolved from client roots). Fall back
        // to the startup hint path so secondary workspace operations (e.g.
        // `manage_workspace open`) can compute storage paths without a bound
        // primary. Storage will live under `{startup_hint}/.julie/indexes/{id}/`.
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
    /// because operations like `manage_workspace(register)` need to compute target
    /// paths before the pool catches up — but actually opening the DB or
    /// search index against a non-pool-resident primary indicates a rebind
    /// that bypassed `attach_daemon_primary_binding_if_needed` (see Findings
    /// #28/#29 in ROOTS_IMPL_REVIEW_NOTES.md).
    ///
    /// Secondary workspaces (workspace_id != current primary) are exempt:
    /// they are accessed lazily via on-disk paths and don't require a pool
    /// entry to function.
    async fn ensure_primary_pool_membership_for(&self, _workspace_id: &str) -> Result<()> {
        Ok(())
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

    pub(crate) async fn workspace_db_file_path_for(&self, workspace_id: &str) -> Result<PathBuf> {
        Ok(self
            .workspace_index_dir_for(workspace_id)
            .await?
            .join("db")
            .join("symbols.db"))
    }

    pub(crate) async fn workspace_tantivy_dir_for(&self, workspace_id: &str) -> Result<PathBuf> {
        Ok(self
            .workspace_index_dir_for(workspace_id)
            .await?
            .join("tantivy"))
    }

    /// Acquire a per-request `SymbolDatabase` backed by a pooled connection.
    ///
    /// **Use this for new handler code.** Each call returns a fresh
    /// `SymbolDatabase` wrapping a `PooledConn` from the workspace's
    /// `WorkspaceConnectionPool` — distinct handlers no longer serialize on a
    /// shared `Arc<Mutex<SymbolDatabase>>`. The connection returns to the pool
    /// when the `SymbolDatabase` is dropped.
    ///
    /// In standalone/in-process mode the implementation opens a fresh owned
    /// `SymbolDatabase` per request — these handlers don't have a cross-session
    /// concurrency problem, so the extra open cost is acceptable.
    pub async fn get_pooled_database_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<SymbolDatabase> {
        // Open a fresh owned SymbolDatabase. Migrations are idempotent so the cost is bounded.
        let db_path = self.workspace_db_file_path_for(workspace_id).await?;
        if !db_path.exists() {
            return Err(anyhow::anyhow!(
                "Database not found for workspace '{}' at {}",
                workspace_id,
                db_path.display()
            ));
        }
        tokio::task::spawn_blocking(move || SymbolDatabase::new(&db_path))
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?
    }

    /// Get the database for a specific workspace by ID.
    ///
    /// In stdio mode: looks in `{project}/.julie/indexes/{workspace_id}/db/symbols.db`.
    /// In daemon mode: looks in `~/.julie/indexes/{workspace_id}/db/symbols.db`
    ///   (sibling of the primary workspace's index dir, not nested under it).
    ///
    /// **Prefer `get_pooled_database_for_workspace` for new code** —
    /// this method returns an Arc<Mutex<...>> that serializes all callers.
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
        // Non-primary workspace indexes are siblings: ~/.julie/indexes/{target_id}/, not nested.
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

        let db_path = self.workspace_db_file_path_for(workspace_id).await?;

        // T7 single-writer gate (codex 3c.3 pre-merge): recreated-open repair is a
        // Tantivy WRITE. In-process followers must skip it — the leader owns the
        // rebuild; the follower serves the (empty) opened index read-only and picks
        // up the leader's rebuild via the poll-reload.
        let may_repair = self.may_repair_recreated_projection();
        let workspace_id = workspace_id.to_string();
        tokio::task::spawn_blocking(move || {
            let configs = crate::search::LanguageConfigs::load_embedded();
            let open_outcome =
                SearchIndex::open_with_language_configs_outcome(&tantivy_path, &configs)?;
            let repair_required = open_outcome.repair_required();
            let index = open_outcome.into_index();

            if repair_required && may_repair {
                warn!(
                    "Tantivy index for workspace '{}' at {} was recreated empty during open; rebuilding projection from canonical SQLite state",
                    workspace_id,
                    tantivy_path.display()
                );

                let mut db = SymbolDatabase::new(&db_path)?;
                let projection = SearchProjection::tantivy(workspace_id.clone());
                projection.repair_recreated_open_if_needed(&mut db, &index, repair_required, None)?;
            } else if repair_required {
                debug!(
                    "Tantivy index for workspace '{}' at {} was recreated empty during open; in-process follower skipping projection repair (leader owns the rebuild)",
                    workspace_id,
                    tantivy_path.display()
                );
            }

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
            + Self::tool_router_spillover_get()
            + Self::tool_router_rename_symbol()
            + Self::tool_router_manage_workspace()
            + Self::tool_router_edit_file()
            + Self::tool_router_rewrite_symbol()
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
/// Pure editing writers (`edit_file`, `rename_symbol`, `rewrite_symbol`) are
/// always exempt — aborting a canonical write mid-transaction would corrupt
/// workspace state.
///
/// For `manage_workspace` the exemption is operation-aware:
/// - **Exempt (mutating / long-running):** Index, Register, Remove, Clean, Refresh, Open.
/// - **Deadline-bounded (read-only):** List, Stats, Health — a hung stats or health
///   query has no write-safety concern, so it must not escape the hang guard.
/// - **Unparseable operation:** not exempt (safely bounded; no mutation risk).
pub(crate) fn is_write_exempt(
    tool_name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
) -> bool {
    if matches!(tool_name, "edit_file" | "rename_symbol" | "rewrite_symbol") {
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
                    | ManageWorkspaceOperation::Register
                    | ManageWorkspaceOperation::Remove
                    | ManageWorkspaceOperation::Clean
                    | ManageWorkspaceOperation::Refresh
                    | ManageWorkspaceOperation::Open
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
        // Compute tool_name and exempt early — both needed before request/context
        // are moved into the in-process bounded future below.
        let tool_name = request.name.as_ref().to_string();
        let exempt = is_write_exempt(&tool_name, request.arguments.as_ref());

        if Self::tool_request_targets_primary(request.name.as_ref(), request.arguments.as_ref()) {
            let complete_deferred_auto_index = !(request.name.as_ref() == "manage_workspace"
                && Self::manage_workspace_primary_index_request(request.arguments.as_ref()));

            // F1: In-process handlers (leader or follower) get a bounded read
            // envelope. Without it, `list_roots_from_peer` (unbounded peer
            // round-trip) or the deferred auto-index running inline on the first
            // request can hang the session indefinitely.
            //
            // Gate strictly on `is_in_process()`: daemon/stdio handlers use
            // `LeadershipState::none()` (is_in_process()==false) and MUST take
            // the existing path byte-for-byte unchanged.
            //
            // Write-exempt tools (edit_file, rename_symbol, rewrite_symbol,
            // manage_workspace mutating ops) fall through to the existing path —
            // aborting a canonical write mid-transaction would corrupt state.
            if self.is_in_process() && !exempt {
                // Leader only: if the deferred auto-index is pending, spawn a
                // NON-CANCELLABLE background task. The timeout below cancels the
                // read envelope but NEVER the spawned repair — that task holds
                // its own `deferred_auto_index_gate` lock and runs to completion.
                // Followers skip (their write gate in complete_deferred_auto_index
                // returns Ok(()) immediately — no write races).
                if self.is_leader() {
                    use std::sync::atomic::Ordering;
                    // Spawn ONLY when a repair is pending AND we win the
                    // single-flight claim (codex pre-merge F-C). Losing the
                    // claim means a repair task is already outstanding — piling
                    // on a duplicate would re-run a persistently-failing repair
                    // once per concurrent read. The spawned task releases the
                    // slot when it finishes so the next pending cycle can retry.
                    if self.deferred_auto_index_pending.load(Ordering::Acquire)
                        && self.try_claim_deferred_repair_slot()
                    {
                        let h = self.clone();
                        tokio::spawn(async move {
                            let _ = h.complete_deferred_auto_index_if_needed().await;
                            h.release_deferred_repair_slot();
                        });
                    }
                }

                // Bound the WHOLE read path (workspace resolution + tool call)
                // in a single timeout. On expiry → bounded McpError, not a hang.
                let deadline = parse_request_timeout(std::env::var(REQUEST_TIMEOUT_ENV).ok());
                let handler = self.clone();
                let bounded_fut = async move {
                    // Pass complete_deferred_auto_index=false: the repair was
                    // spawned above (leader) or skipped by gate (follower).
                    // Running it inline here would re-introduce the hang risk.
                    handler
                        .ensure_primary_workspace_for_request(&context.peer, false)
                        .await
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    handler
                        .tool_router
                        .call(ToolCallContext::new(&handler, request, context))
                        .await
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

            // Non-in-process path OR in-process write path: existing behavior
            // unchanged. Writes are unbounded by design (T3 / dispatch_with_deadline
            // exemption); daemon/stdio take this path for all tools.
            self.ensure_primary_workspace_for_request(&context.peer, complete_deferred_auto_index)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
        }

        let deadline = parse_request_timeout(std::env::var(REQUEST_TIMEOUT_ENV).ok());
        let fut = self
            .tool_router
            .call(ToolCallContext::new(self, request, context));
        dispatch_with_deadline(&tool_name, exempt, fut, deadline).await
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

    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        info!("MCP connection established - client initialized");

        let startup_hint = self.workspace_startup_hint();
        if crate::startup::startup_source_prefers_request_roots(startup_hint.source) {
            self.mark_deferred_auto_index_pending(true);

            if self.client_supports_workspace_roots() {
                info!(
                    startup_source = ?startup_hint.source.unwrap_or(WorkspaceStartupSource::Cwd),
                    "Resolving client roots before auto-indexing"
                );
                let handler = self.clone();
                let peer = context.peer;
                tokio::spawn(async move {
                    if let Err(err) = handler
                        .ensure_primary_workspace_for_request(&peer, true)
                        .await
                    {
                        warn!("Failed to resolve primary workspace from client roots: {err}");
                    }
                });
            } else {
                info!(
                    startup_source = ?startup_hint.source.unwrap_or(WorkspaceStartupSource::Cwd),
                    "Deferring cwd auto-indexing until first primary tool request"
                );
            }
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

    async fn on_roots_list_changed(&self, context: NotificationContext<RoleServer>) {
        self.mark_roots_dirty();

        if self
            .deferred_auto_index_pending
            .load(std::sync::atomic::Ordering::Acquire)
        {
            let handler = self.clone();
            let peer = context.peer;
            tokio::spawn(async move {
                if let Err(err) = handler
                    .ensure_primary_workspace_for_request(&peer, true)
                    .await
                {
                    warn!("Failed to resolve deferred workspace on roots_list_changed: {err}");
                }
            });
        }
    }
}
