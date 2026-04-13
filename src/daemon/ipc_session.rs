use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result};
use rmcp::ServiceExt;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::dashboard::state::DashboardEvent;
use crate::handler::JulieServerHandler;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

use super::database::DaemonDatabase;
use super::embedding_service::EmbeddingService;
use super::ipc::IpcStream;
use super::watcher_pool::WatcherPool;
use super::workspace_pool::WorkspacePool;

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

/// IPC headers sent by the adapter on connect.
#[derive(Debug)]
pub(crate) struct IpcHeaders {
    pub(crate) workspace: PathBuf,
    pub(crate) workspace_source: Option<WorkspaceStartupSource>,
    /// Adapter binary version (None if old adapter without version support).
    pub(crate) version: Option<String>,
}

impl IpcHeaders {
    pub(crate) fn workspace_startup_hint(&self) -> WorkspaceStartupHint {
        WorkspaceStartupHint {
            path: self.workspace.clone(),
            source: self.workspace_source,
        }
    }
}

pub(crate) struct ParsedIpcHeaders {
    pub(crate) headers: IpcHeaders,
    pub(crate) buffered_bytes: Vec<u8>,
}

pub(crate) struct PrefixedIpcStream {
    prefix: std::io::Cursor<Vec<u8>>,
    stream: IpcStream,
}

impl PrefixedIpcStream {
    pub(crate) fn new(stream: IpcStream, prefix: Vec<u8>) -> Self {
        Self {
            prefix: std::io::Cursor::new(prefix),
            stream,
        }
    }
}

impl AsyncRead for PrefixedIpcStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let position = this.prefix.position() as usize;
        let prefix = this.prefix.get_ref();

        if position < prefix.len() {
            let to_copy = std::cmp::min(prefix.len() - position, buf.remaining());
            buf.put_slice(&prefix[position..position + to_copy]);
            this.prefix.set_position((position + to_copy) as u64);
            return std::task::Poll::Ready(Ok(()));
        }

        std::pin::Pin::new(&mut this.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrefixedIpcStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.get_mut().stream).poll_write(cx, buf)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}

/// Read IPC headers from the adapter.
///
/// The adapter sends:
///   WORKSPACE:/path/to/project\n
///   WORKSPACE_SOURCE:cli\n      (optional, added for startup hint tracking)
///   VERSION:6.5.2\n            (optional, added in v6.5.3)
///
/// We read byte-by-byte to avoid BufReader consuming bytes past the headers,
/// which would break the subsequent MCP JSON-RPC framing.
pub(crate) async fn read_ipc_headers(stream: &mut IpcStream) -> Result<ParsedIpcHeaders> {
    let (first_line, _) = read_header_line(stream).await?;
    let mut headers = parse_ipc_headers_block(&format!("{}\n", first_line))?;
    let mut buffered_bytes = Vec::new();

    loop {
        let (line, raw_line) = read_header_line(stream).await?;
        if line.is_empty() {
            break;
        }

        match parse_startup_header_line(&mut headers, &line)? {
            StartupHeaderLine::Recognized => continue,
            StartupHeaderLine::BlankLineModeOnly => continue,
            StartupHeaderLine::NotStartup => {
                buffered_bytes.extend_from_slice(&raw_line);
                break;
            }
        }
    }

    Ok(ParsedIpcHeaders {
        headers,
        buffered_bytes,
    })
}

pub(crate) fn parse_ipc_headers_block(block: &str) -> Result<IpcHeaders> {
    let mut lines = block.lines();

    let first_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid IPC header: missing WORKSPACE line"))?;
    let path = first_line.strip_prefix("WORKSPACE:").ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid IPC header: expected WORKSPACE:<path>, got: {}",
            first_line
        )
    })?;

    let mut headers = IpcHeaders {
        workspace: PathBuf::from(path),
        workspace_source: None,
        version: None,
    };

    for line in lines {
        if line.is_empty() {
            break;
        }

        match parse_startup_header_line(&mut headers, line)? {
            StartupHeaderLine::Recognized | StartupHeaderLine::BlankLineModeOnly => {}
            StartupHeaderLine::NotStartup => break,
        }
    }

    Ok(headers)
}

enum StartupHeaderLine {
    Recognized,
    BlankLineModeOnly,
    NotStartup,
}

fn parse_startup_header_line(headers: &mut IpcHeaders, line: &str) -> Result<StartupHeaderLine> {
    if let Some(parsed) = line.strip_prefix("WORKSPACE_SOURCE:") {
        headers.workspace_source = Some(
            WorkspaceStartupSource::from_header_value(parsed)
                .ok_or_else(|| anyhow::anyhow!("Invalid WORKSPACE_SOURCE header: {}", parsed))?,
        );
        return Ok(StartupHeaderLine::Recognized);
    }

    if let Some(parsed) = line.strip_prefix("VERSION:") {
        headers.version = Some(parsed.to_string());
        return Ok(StartupHeaderLine::Recognized);
    }

    if line.starts_with("WORKSPACE_") {
        return Ok(StartupHeaderLine::BlankLineModeOnly);
    }

    Ok(StartupHeaderLine::NotStartup)
}

/// Read a single newline-terminated header line from the IPC stream.
async fn read_header_line(stream: &mut IpcStream) -> Result<(String, Vec<u8>)> {
    let mut line = Vec::new();
    let mut raw = Vec::new();
    let mut buf = [0u8; 1];

    loop {
        stream
            .read_exact(&mut buf)
            .await
            .context("Failed to read IPC header")?;
        raw.push(buf[0]);
        if buf[0] == b'\n' {
            break;
        }
        line.push(buf[0]);

        if line.len() > 4096 {
            anyhow::bail!("IPC header line too long (>4096 bytes)");
        }
    }

    Ok((
        String::from_utf8(line).context("IPC header is not valid UTF-8")?,
        raw,
    ))
}

/// Handle a single IPC session: bootstrap from the startup hint, then serve MCP.
pub(crate) async fn handle_ipc_session(
    stream: impl AsyncRead + AsyncWrite + Unpin + Send + 'static,
    pool: Arc<WorkspacePool>,
    session_id: &str,
    daemon_db: &Option<Arc<DaemonDatabase>>,
    embedding_service: &Arc<EmbeddingService>,
    restart_pending: &Arc<AtomicBool>,
    dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
    workspace_startup_hint: WorkspaceStartupHint,
    watcher_pool: Option<Arc<WatcherPool>>,
) -> Result<()> {
    let workspace_path = workspace_startup_hint.path.clone();

    // Compute workspace ID from path. Use generate_workspace_id() directly
    // (produces e.g. "julie_316c0b08"). Do NOT wrap in another prefix; the
    // indexing pipeline also calls generate_workspace_id() and the IDs must match
    // for daemon.db FK constraints and workspace_db_path() to resolve correctly.
    let path_str = workspace_path.to_string_lossy().to_string();
    let full_workspace_id =
        generate_workspace_id(&path_str).context("Failed to generate workspace ID")?;

    info!(
        session_id = %session_id,
        workspace_id = %full_workspace_id,
        workspace_source = %workspace_startup_hint
            .source
            .map(WorkspaceStartupSource::as_header_value)
            .unwrap_or("legacy-missing"),
        "Getting or initializing workspace from pool"
    );

    let defer_startup_workspace_attach = matches!(
        workspace_startup_hint.source,
        Some(WorkspaceStartupSource::Cwd)
    );

    let session_result: Result<(Vec<String>, Result<(), anyhow::Error>)> = async {
        let handler = if defer_startup_workspace_attach {
            JulieServerHandler::new_deferred_daemon_startup_hint(
                workspace_startup_hint,
                daemon_db.clone(),
                Some(Arc::clone(embedding_service)),
                Some(Arc::clone(restart_pending)),
                dashboard_tx,
                watcher_pool,
                Some(Arc::clone(&pool)),
            )
            .await
        } else {
            let workspace = pool
                .get_or_init(&full_workspace_id, workspace_path.clone())
                .await
                .context("Failed to initialize workspace in pool")?;
            JulieServerHandler::new_with_shared_workspace_startup_hint(
                workspace,
                workspace_startup_hint,
                daemon_db.clone(),
                Some(full_workspace_id.clone()),
                Some(Arc::clone(embedding_service)),
                Some(Arc::clone(restart_pending)),
                dashboard_tx,
                watcher_pool,
                Some(Arc::clone(&pool)),
            )
            .await
        }
        .context("Failed to create handler for IPC session")?;

        let project_log = handler.project_log.clone();

        if let Some(ref log) = project_log {
            log.session_start(session_id);
        }

        let service_result = match handler.clone().serve(stream).await {
            Ok(service) => match service.waiting().await {
                Ok(_reason) => {
                    info!(session_id = %session_id, "MCP session completed normally");
                    Ok(())
                }
                Err(e) => {
                    warn!(session_id = %session_id, "MCP session ended with error: {}", e);
                    Err(anyhow::anyhow!("MCP session error: {}", e))
                }
            },
            Err(e) => {
                warn!(session_id = %session_id, "MCP serve failed: {}", e);
                Err(anyhow::anyhow!("MCP serve failed: {}", e))
            }
        };

        if let Some(ref log) = project_log {
            log.session_end(session_id);
        }

        let attached_workspace_ids = handler.session_attached_workspace_ids().await;
        Ok((attached_workspace_ids, service_result))
    }
    .await;

    let (session_result, attached_workspace_ids) = match session_result {
        Ok((ids, result)) => (result, ids),
        Err(error) => {
            let attached = if defer_startup_workspace_attach {
                Vec::new()
            } else {
                vec![full_workspace_id.clone()]
            };
            (Err(error), attached)
        }
    };

    pool.sync_indexed_from_db(&full_workspace_id).await;

    for workspace_id in workspace_ids_to_disconnect(
        &full_workspace_id,
        attached_workspace_ids,
        !defer_startup_workspace_attach,
    ) {
        pool.disconnect_session(&workspace_id).await;
    }

    session_result
}
