# Workspace Startup and Roots Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor Julie startup and session binding so explicit `--workspace` and `JULIE_WORKSPACE` stay authoritative, GUI `cwd` is treated as a weak hint, and MCP roots can drive primary plus secondary workspace activation.

**Architecture:** Preserve workspace source metadata from CLI through the adapter handshake, then move primary binding policy into handler-owned session state. Daemon sessions may start unbound when the startup source is `cwd`, and primary-scoped tool requests resolve the active workspace on demand via `roots/list`, with `notifications/roots/list_changed` marking the session dirty for the next request boundary.

**Tech Stack:** Rust, tokio, rmcp 1.2, anyhow, clap, existing WorkspacePool and daemon registry, Julie test suite under `src/tests/`

**Spec:** `docs/superpowers/specs/2026-04-11-workspace-startup-roots-design.md`

**Execution note:** Start from a dedicated worktree before Task 1 if the current tree has unrelated edits. This refactor touches `src/handler.rs` and `src/daemon/mod.rs`, both of which are already large and easy to collide on.

---

## File Map

- `src/workspace/startup_hint.rs` - shared startup hint types (`WorkspaceStartupHint`, `WorkspaceStartupSource`) used by CLI, adapter, daemon, and handler
- `src/cli.rs` - resolve startup hint from CLI, env, and `cwd`
- `src/main.rs` - pass the startup hint into adapter mode
- `src/adapter/mod.rs` - serialize the startup hint into the IPC header
- `src/daemon/ipc_session.rs` - parse IPC headers and bootstrap IPC sessions with startup-hint awareness
- `src/daemon/mod.rs` - accept loop keeps orchestration only and delegates IPC-session details to `ipc_session`
- `src/handler/session_workspace.rs` - pure session-state model for startup hints, root snapshots, primary binding, and dirty-state reconciliation
- `src/handler.rs` - handler constructors, current-primary accessors, roots-aware lifecycle hooks, and tool-router wrappers
- `src/startup.rs` - auto-index checks must use the current primary binding instead of assuming eager startup binding
- `src/tools/workspace/commands/index.rs` - `path: None` must resolve against the current bound primary root, not raw startup `cwd`
- `src/tests/cli_tests.rs` - startup-hint precedence and source tests
- `src/tests/adapter/handshake.rs` - adapter header serialization tests
- `src/tests/daemon/ipc_session.rs` - daemon header parsing and session bootstrap tests
- `src/tests/daemon/session_workspace.rs` - pure session-state tests for binding policy and root reconciliation
- `src/tests/daemon/roots.rs` - end-to-end rmcp tests for `roots/list` resolution and `roots/list_changed`
- `docs/WORKSPACE_ARCHITECTURE.md` - user-facing documentation for the new startup and roots flow

---

### Task 1: Introduce a shared startup hint and preserve it through the adapter handshake

**Files:**
- Create: `src/workspace/startup_hint.rs`
- Modify: `src/workspace/mod.rs:11-12`
- Modify: `src/cli.rs:1-96`
- Modify: `src/main.rs:8-89`
- Modify: `src/adapter/mod.rs:10-129`
- Modify: `src/tests/adapter/mod.rs:1-2`
- Modify: `src/tests/cli_tests.rs:1-114`
- Create: `src/tests/adapter/handshake.rs`

- [ ] **Step 1: Write the failing tests**

Add to `src/tests/cli_tests.rs`:

```rust
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

#[test]
fn test_resolve_workspace_startup_hint_marks_cli_source() {
    let hint = crate::cli::resolve_workspace_startup_hint(Some(PathBuf::from("/tmp")));
    assert_eq!(hint.source, WorkspaceStartupSource::Cli);
    assert!(hint.path.exists());
}

#[test]
fn test_resolve_workspace_startup_hint_marks_cwd_source() {
    let hint = crate::cli::resolve_workspace_startup_hint(None);
    assert_eq!(hint.source, WorkspaceStartupSource::Cwd);
    assert!(hint.path.exists());
}
```

Create `src/tests/adapter/handshake.rs`:

```rust
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::adapter::build_ipc_header;
    use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

    #[test]
    fn test_build_ipc_header_includes_workspace_source() {
        let hint = WorkspaceStartupHint {
            path: PathBuf::from("/tmp/project"),
            source: WorkspaceStartupSource::Env,
        };

        let header = build_ipc_header(&hint);

        assert!(header.contains("WORKSPACE:/tmp/project\n"));
        assert!(header.contains("WORKSPACE_SOURCE:env\n"));
        assert!(header.contains("VERSION:"));
    }
}
```

- [ ] **Step 2: Run the focused tests to verify RED**

Run: `cargo test --lib test_resolve_workspace_startup_hint_ 2>&1 | tail -10`
Expected: FAIL because `resolve_workspace_startup_hint` and the shared startup-hint types do not exist yet.

Run: `cargo test --lib test_build_ipc_header_includes_workspace_source 2>&1 | tail -10`
Expected: FAIL because `build_ipc_header` and `WORKSPACE_SOURCE` support do not exist yet.

- [ ] **Step 3: Implement the shared startup hint and adapter serialization**

Create `src/workspace/startup_hint.rs`:

```rust
use std::path::PathBuf;

use anyhow::{Result, anyhow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceStartupSource {
    Cli,
    Env,
    Cwd,
}

impl WorkspaceStartupSource {
    pub fn as_header_value(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Env => "env",
            Self::Cwd => "cwd",
        }
    }

    pub fn parse_header_value(value: &str) -> Result<Self> {
        match value {
            "cli" => Ok(Self::Cli),
            "env" => Ok(Self::Env),
            "cwd" => Ok(Self::Cwd),
            other => Err(anyhow!("Unknown workspace source header: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceStartupHint {
    pub path: PathBuf,
    pub source: WorkspaceStartupSource,
}
```

Update `src/workspace/mod.rs`:

```rust
pub mod registry;
pub mod startup_hint;
```

Update `src/cli.rs` so the resolver preserves source instead of returning only `PathBuf`:

```rust
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

fn canonicalize_workspace_path(raw_path: PathBuf) -> PathBuf {
    let path_str = raw_path.to_string_lossy();
    let expanded = shellexpand::tilde(&path_str).to_string();
    let path = PathBuf::from(expanded);
    if path.exists() {
        path.canonicalize().unwrap_or(path)
    } else {
        path
    }
}

pub fn resolve_workspace_startup_hint(cli_workspace: Option<PathBuf>) -> WorkspaceStartupHint {
    if let Some(raw_path) = cli_workspace {
        let path = canonicalize_workspace_path(raw_path);
        return WorkspaceStartupHint {
            path,
            source: WorkspaceStartupSource::Cli,
        };
    }

    if let Ok(path_str) = std::env::var("JULIE_WORKSPACE") {
        let path = canonicalize_workspace_path(PathBuf::from(path_str));
        return WorkspaceStartupHint {
            path,
            source: WorkspaceStartupSource::Env,
        };
    }

    let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    WorkspaceStartupHint {
        path: current.canonicalize().unwrap_or(current),
        source: WorkspaceStartupSource::Cwd,
    }
}
```

Update `src/main.rs` and `src/adapter/mod.rs`:

```rust
use julie::cli::{Cli, Command, resolve_workspace_startup_hint};

let startup_hint = resolve_workspace_startup_hint(cli.workspace);
...
julie::adapter::run_adapter(startup_hint).await?;
```

```rust
use crate::workspace::startup_hint::WorkspaceStartupHint;

pub(crate) fn build_ipc_header(startup_hint: &WorkspaceStartupHint) -> String {
    format!(
        "WORKSPACE:{}\nWORKSPACE_SOURCE:{}\nVERSION:{}\n",
        startup_hint.path.to_string_lossy(),
        startup_hint.source.as_header_value(),
        env!("CARGO_PKG_VERSION"),
    )
}

pub async fn run_adapter(startup_hint: WorkspaceStartupHint) -> Result<()> {
    ...
    let stream = match connect_and_handshake(&paths, &startup_hint).await {
        ...
    };
}

async fn connect_and_handshake(
    paths: &DaemonPaths,
    startup_hint: &WorkspaceStartupHint,
) -> Result<IpcClientStream> {
    ...
    stream.write_all(build_ipc_header(startup_hint).as_bytes()).await?;
    Ok(stream)
}
```

- [ ] **Step 4: Run the focused tests to verify GREEN**

Run: `cargo test --lib test_resolve_workspace_startup_hint_ 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib test_build_ipc_header_includes_workspace_source 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/workspace/startup_hint.rs src/workspace/mod.rs src/cli.rs src/main.rs src/adapter/mod.rs src/tests/cli_tests.rs src/tests/adapter/mod.rs src/tests/adapter/handshake.rs
git commit -m "refactor(startup): preserve workspace source in adapter handshake"
```

---

### Task 2: Extract IPC-session parsing in the daemon and carry startup hints into session bootstrap

**Files:**
- Create: `src/daemon/ipc_session.rs`
- Modify: `src/daemon/mod.rs:745-1018`
- Modify: `src/tests/daemon/mod.rs:1-14`
- Create: `src/tests/daemon/ipc_session.rs`

- [ ] **Step 1: Write the failing tests**

Create `src/tests/daemon/ipc_session.rs`:

```rust
use std::path::PathBuf;

use crate::daemon::ipc_session::parse_ipc_headers_block;
use crate::workspace::startup_hint::WorkspaceStartupSource;

#[test]
fn test_parse_ipc_headers_block_parses_workspace_source() {
    let headers = parse_ipc_headers_block(
        "WORKSPACE:/tmp/project",
        "WORKSPACE_SOURCE:cwd",
        "VERSION:6.7.0",
    )
    .expect("headers should parse");

    assert_eq!(headers.startup_hint.path, PathBuf::from("/tmp/project"));
    assert_eq!(headers.startup_hint.source, WorkspaceStartupSource::Cwd);
    assert_eq!(headers.version.as_deref(), Some("6.7.0"));
}

#[test]
fn test_parse_ipc_headers_block_rejects_unknown_workspace_source() {
    let error = parse_ipc_headers_block(
        "WORKSPACE:/tmp/project",
        "WORKSPACE_SOURCE:oops",
        "VERSION:6.7.0",
    )
    .expect_err("unknown workspace source should fail");

    assert!(error.to_string().contains("Unknown workspace source header"));
}
```

- [ ] **Step 2: Run the focused tests to verify RED**

Run: `cargo test --lib test_parse_ipc_headers_block_ 2>&1 | tail -10`
Expected: FAIL because `src/daemon/ipc_session.rs` and `parse_ipc_headers_block` do not exist yet.

- [ ] **Step 3: Move header parsing into `src/daemon/ipc_session.rs` and thread startup hints through the daemon boundary**

Create `src/daemon/ipc_session.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result, anyhow};

use crate::daemon::database::DaemonDatabase;
use crate::daemon::embedding_service::EmbeddingService;
use crate::daemon::ipc::IpcStream;
use crate::daemon::watcher_pool::WatcherPool;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::dashboard::state::DashboardEvent;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

pub(crate) struct IpcHeaders {
    pub startup_hint: WorkspaceStartupHint,
    pub version: Option<String>,
}

pub(crate) fn parse_ipc_headers_block(
    workspace_line: &str,
    source_line: &str,
    version_line: &str,
) -> Result<IpcHeaders> {
    let workspace = workspace_line
        .strip_prefix("WORKSPACE:")
        .ok_or_else(|| anyhow!("Invalid IPC header: expected WORKSPACE:<path>"))?;
    let source = source_line
        .strip_prefix("WORKSPACE_SOURCE:")
        .ok_or_else(|| anyhow!("Invalid IPC header: expected WORKSPACE_SOURCE:<source>"))?;
    let version = version_line
        .strip_prefix("VERSION:")
        .map(|value| value.to_string());

    Ok(IpcHeaders {
        startup_hint: WorkspaceStartupHint {
            path: PathBuf::from(workspace),
            source: WorkspaceStartupSource::parse_header_value(source)?,
        },
        version,
    })
}
```

Then update `read_ipc_headers` and `handle_ipc_session` wiring in `src/daemon/mod.rs`:

```rust
mod ipc_session;

use self::ipc_session::{IpcHeaders, handle_ipc_session, read_ipc_headers};

let headers = tokio::time::timeout(Duration::from_secs(5), read_ipc_headers(&mut stream)).await??;
let startup_hint = headers.startup_hint.clone();
info!(workspace = %startup_hint.path.display(), source = ?startup_hint.source, "IPC headers received");

if let Err(e) = handle_ipc_session(
    stream,
    pool,
    &session_id,
    &daemon_db,
    &embedding_service,
    &restart_pending,
    Some(dashboard_tx),
    startup_hint,
    Some(watcher_pool_for_session),
)
.await {
    ...
}
```

Do not change binding policy in this task. This task is transport extraction only: parse the source, keep the accept loop readable, and pass the startup hint into the next layer.

- [ ] **Step 4: Run the focused tests to verify GREEN**

Run: `cargo test --lib test_parse_ipc_headers_block_ 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib tests::integration::daemon_lifecycle::test_ipc_workspace_header_protocol 2>&1 | tail -10`
Expected: FAIL until you update the integration assertion to the new three-line header, then PASS.

- [ ] **Step 5: Commit**

```bash
git add src/daemon/ipc_session.rs src/daemon/mod.rs src/tests/daemon/mod.rs src/tests/daemon/ipc_session.rs src/tests/integration/daemon_lifecycle.rs
git commit -m "refactor(daemon): carry workspace startup hints through ipc sessions"
```

---

### Task 3: Add handler-owned session workspace state and make primary identity mutable

**Files:**
- Create: `src/handler/session_workspace.rs`
- Modify: `src/handler.rs:145-337, 535-572, 776-926`
- Modify: `src/tests/daemon/mod.rs:1-15`
- Create: `src/tests/daemon/session_workspace.rs`
- Modify: `src/tests/core/handler.rs:1-80`

- [ ] **Step 1: Write the failing tests**

Create `src/tests/daemon/session_workspace.rs`:

```rust
use std::path::PathBuf;

use crate::handler::session_workspace::{PrimaryWorkspaceBinding, SessionWorkspaceState};
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

#[test]
fn test_session_workspace_state_defers_binding_for_cwd_roots_clients() {
    let mut state = SessionWorkspaceState::new(WorkspaceStartupHint {
        path: PathBuf::from("/"),
        source: WorkspaceStartupSource::Cwd,
    });

    state.set_client_supports_roots(true);

    assert!(state.should_defer_auto_indexing());
    assert!(state.primary_binding().is_none());
}

#[test]
fn test_session_workspace_state_keeps_explicit_primary_binding() {
    let mut state = SessionWorkspaceState::new(WorkspaceStartupHint {
        path: PathBuf::from("/tmp/project"),
        source: WorkspaceStartupSource::Cli,
    });

    state.set_primary_binding(PrimaryWorkspaceBinding {
        workspace_id: "julie_abcd1234".to_string(),
        root: PathBuf::from("/tmp/project"),
    });

    assert!(!state.should_defer_auto_indexing());
    assert_eq!(state.primary_binding().unwrap().workspace_id, "julie_abcd1234");
}
```

Add to `src/tests/core/handler.rs`:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn handler_construction_uses_startup_hint_for_current_root() -> anyhow::Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;
    assert!(handler.current_workspace_root().is_absolute() || handler.current_workspace_root().as_os_str() == ".");
    Ok(())
}
```

- [ ] **Step 2: Run the focused tests to verify RED**

Run: `cargo test --lib test_session_workspace_state_ 2>&1 | tail -10`
Expected: FAIL because `src/handler/session_workspace.rs` and the new state types do not exist yet.

Run: `cargo test --lib handler_construction_uses_startup_hint_for_current_root 2>&1 | tail -10`
Expected: FAIL because `current_workspace_root()` does not exist yet.

- [ ] **Step 3: Implement session workspace state and handler accessors**

Create `src/handler/session_workspace.rs`:

```rust
use std::collections::HashSet;
use std::path::PathBuf;

use crate::workspace::startup_hint::WorkspaceStartupHint;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryWorkspaceBinding {
    pub workspace_id: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SessionWorkspaceState {
    startup_hint: WorkspaceStartupHint,
    client_supports_roots: bool,
    roots_dirty: bool,
    last_roots: Vec<PathBuf>,
    primary_binding: Option<PrimaryWorkspaceBinding>,
    secondary_workspace_ids: HashSet<String>,
}

impl SessionWorkspaceState {
    pub fn new(startup_hint: WorkspaceStartupHint) -> Self {
        Self {
            startup_hint,
            client_supports_roots: false,
            roots_dirty: false,
            last_roots: Vec::new(),
            primary_binding: None,
            secondary_workspace_ids: HashSet::new(),
        }
    }

    pub fn set_client_supports_roots(&mut self, enabled: bool) {
        self.client_supports_roots = enabled;
    }

    pub fn client_supports_roots(&self) -> bool {
        self.client_supports_roots
    }

    pub fn should_defer_auto_indexing(&self) -> bool {
        self.primary_binding.is_none()
            && self.client_supports_roots
            && matches!(self.startup_hint.source, crate::workspace::startup_hint::WorkspaceStartupSource::Cwd)
    }

    pub fn startup_hint(&self) -> &WorkspaceStartupHint {
        &self.startup_hint
    }

    pub fn primary_binding(&self) -> Option<&PrimaryWorkspaceBinding> {
        self.primary_binding.as_ref()
    }

    pub fn set_primary_binding(&mut self, binding: PrimaryWorkspaceBinding) {
        self.primary_binding = Some(binding);
    }

    pub fn mark_roots_dirty(&mut self) {
        self.roots_dirty = true;
    }

    pub fn roots_dirty(&self) -> bool {
        self.roots_dirty
    }
}
```

Update `src/handler.rs` to store and use the state:

```rust
pub mod session_workspace;

use crate::handler::session_workspace::{PrimaryWorkspaceBinding, SessionWorkspaceState};
use crate::workspace::startup_hint::WorkspaceStartupHint;

pub(crate) startup_hint: WorkspaceStartupHint,
pub(crate) session_workspace: Arc<std::sync::RwLock<SessionWorkspaceState>>,
```

Add a daemon-session constructor that accepts an optional eager primary workspace:

```rust
pub async fn new_for_daemon_session(
    startup_hint: WorkspaceStartupHint,
    workspace: Option<Arc<JulieWorkspace>>,
    daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
    workspace_id: Option<String>,
    ...
) -> Result<Self> {
    let state = Arc::new(std::sync::RwLock::new(SessionWorkspaceState::new(startup_hint.clone())));

    if let Some(ref id) = workspace_id {
        state.write().unwrap().set_primary_binding(PrimaryWorkspaceBinding {
            workspace_id: id.clone(),
            root: startup_hint.path.clone(),
        });
    }

    Ok(Self {
        startup_hint,
        session_workspace: state,
        workspace: Arc::new(RwLock::new(workspace.map(|ws| (*ws).clone()))),
        ...
    })
}

pub fn current_workspace_root(&self) -> PathBuf {
    self.session_workspace
        .read()
        .unwrap()
        .primary_binding()
        .map(|binding| binding.root.clone())
        .unwrap_or_else(|| self.startup_hint.path.clone())
}

pub fn current_workspace_id(&self) -> Option<String> {
    self.session_workspace
        .read()
        .unwrap()
        .primary_binding()
        .map(|binding| binding.workspace_id.clone())
}
```

Do not migrate every call site in this task. Only add the state model, the constructor, and the accessors the later tasks will use.

- [ ] **Step 4: Run the focused tests to verify GREEN**

Run: `cargo test --lib test_session_workspace_state_ 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib handler_construction_uses_startup_hint_for_current_root 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/handler/session_workspace.rs src/handler.rs src/tests/daemon/mod.rs src/tests/daemon/session_workspace.rs src/tests/core/handler.rs
git commit -m "refactor(handler): add session workspace state"
```

---

### Task 4: Resolve the primary workspace at request time and defer auto-indexing until the primary is real

**Files:**
- Modify: `src/handler.rs:330-572, 694-820, 978-1402`
- Modify: `src/startup.rs:13-120`
- Modify: `src/tools/workspace/commands/index.rs:45-91`
- Modify: `src/tests/daemon/mod.rs:1-16`
- Create: `src/tests/daemon/roots.rs`

- [ ] **Step 1: Write the failing integration tests**

Create `src/tests/daemon/roots.rs`:

```rust
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation, ListRootsResult, Root};
use rmcp::service::RequestContext;
use rmcp::{ClientHandler, RoleClient, ServiceExt};

use crate::handler::JulieServerHandler;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

#[derive(Clone)]
struct RootsClient {
    roots: Arc<Mutex<Vec<String>>>,
}

impl ClientHandler for RootsClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::builder()
                .enable_roots()
                .enable_roots_list_changed()
                .build(),
            Implementation::new("roots-test-client", "0.1.0"),
        )
    }

    fn list_roots(
        &self,
        _context: RequestContext<RoleClient>,
    ) -> impl Future<Output = Result<ListRootsResult, rmcp::ErrorData>> + Send + '_ {
        let roots = self
            .roots
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .map(Root::new)
            .collect::<Vec<_>>();
        std::future::ready(Ok(ListRootsResult::new(roots)))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_manage_workspace_index_uses_roots_over_cwd_hint() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let bad_cwd_root = temp.path().join("bad-cwd-root");
    let real_root = temp.path().join("real-root");
    std::fs::create_dir_all(real_root.join(".git"))?;
    std::fs::write(real_root.join("lib.rs"), "pub fn roots_marker() {}\n")?;

    let handler = JulieServerHandler::new_for_daemon_session(
        WorkspaceStartupHint {
            path: bad_cwd_root,
            source: WorkspaceStartupSource::Cwd,
        },
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await?;

    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server = handler.clone();
    let server_handle = tokio::spawn(async move {
        server.serve(server_transport).await?.waiting().await?;
        anyhow::Ok(())
    });

    let roots = Arc::new(Mutex::new(vec![format!("file://{}", real_root.display())]));
    let client = RootsClient { roots }.serve(client_transport).await?;

    client
        .call_tool(CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "index", "force": true })
                .as_object()
                .unwrap()
                .clone(),
        ))
        .await?;

    let workspace = handler.get_workspace().await?.expect("workspace should bind");
    assert_eq!(workspace.root.canonicalize()?, real_root.canonicalize()?);

    client.cancel().await?;
    server_handle.await??;
    Ok(())
}
```

- [ ] **Step 2: Run the focused tests to verify RED**

Run: `cargo test --lib test_manage_workspace_index_uses_roots_over_cwd_hint 2>&1 | tail -10`
Expected: FAIL because the handler cannot yet defer binding, request roots, or construct a daemon session without an eager workspace.

- [ ] **Step 3: Implement request-time roots resolution and auto-index deferral**

In `src/handler.rs`, add a roots-aware initialize hook and a resolver that can use `Peer<RoleServer>`:

```rust
use rmcp::{Peer, RoleServer};
use rmcp::service::{NotificationContext, RequestContext};
use rmcp::model::InitializeRequestParams;

async fn initialize(
    &self,
    request: InitializeRequestParams,
    context: RequestContext<RoleServer>,
) -> Result<rmcp::model::InitializeResult, McpError> {
    self.session_workspace
        .write()
        .unwrap()
        .set_client_supports_roots(request.capabilities.roots.is_some());

    if context.peer.peer_info().is_none() {
        context.peer.set_peer_info(request);
    }

    Ok(self.get_info())
}

async fn on_initialized(&self, _context: NotificationContext<RoleServer>) {
    if self.session_workspace.read().unwrap().should_defer_auto_indexing() {
        info!("Deferring auto-indexing until primary workspace is resolved from roots");
        return;
    }

    let handler = self.clone();
    tokio::spawn(async move {
        handler.run_auto_indexing().await;
    });
}

pub(crate) async fn ensure_primary_workspace_for_request(
    &self,
    peer: &Peer<RoleServer>,
) -> Result<()> {
    let snapshot = self.session_workspace.read().unwrap().clone();
    if snapshot.primary_binding().is_some() && !snapshot.roots_dirty() {
        return Ok(());
    }

    let roots = if snapshot.client_supports_roots() {
        self.list_roots_from_peer(peer).await?
    } else {
        Vec::new()
    };

    let chosen_root = roots
        .first()
        .cloned()
        .unwrap_or_else(|| snapshot.startup_hint().path.clone());

    self.bind_primary_workspace(chosen_root, roots).await
}

async fn list_roots_from_peer(&self, peer: &Peer<RoleServer>) -> Result<Vec<PathBuf>> {
    let response = peer.list_roots().await?;
    response
        .roots
        .into_iter()
        .map(|root| {
            let url = url::Url::parse(&root.uri)?;
            url.to_file_path()
                .map_err(|_| anyhow::anyhow!("Non-file root URI is unsupported: {}", root.uri))
        })
        .collect()
}
```

Update primary-scoped tool wrappers so the router injects the request peer before calling `params.call_tool(self)`:

```rust
#[tool(name = "fast_search", ...)]
async fn fast_search(
    &self,
    Parameters(params): Parameters<FastSearchTool>,
    peer: Peer<RoleServer>,
) -> Result<CallToolResult, McpError> {
    if params.workspace.as_deref().unwrap_or("primary") == "primary" {
        self.ensure_primary_workspace_for_request(&peer)
            .await
            .map_err(|e| McpError::internal_error(format!("workspace resolution failed: {e}"), None))?;
    }
    ...
}
```

Apply the same pattern to:

- `fast_refs`
- `get_symbols`
- `deep_dive`
- `get_context`
- `rename_symbol`
- `manage_workspace`
- `edit_file`
- `edit_symbol`

Update default-root helpers so they use the current binding instead of raw startup `cwd`:

```rust
// src/startup.rs
let primary_workspace_id = handler.current_workspace_id().unwrap_or_default();

// src/tools/workspace/commands/index.rs
let original_path = match path {
    Some(ref p) => PathBuf::from(shellexpand::tilde(p).to_string()),
    None => handler.current_workspace_root(),
};
```

- [ ] **Step 4: Run the focused tests to verify GREEN**

Run: `cargo test --lib test_manage_workspace_index_uses_roots_over_cwd_hint 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib tests::core::handler::test_auto_index_write_lock_prevents_double_spawn 2>&1 | tail -10`
Expected: PASS. This guards against breaking the existing `on_initialized` concurrency fix while you add the defer path.

- [ ] **Step 5: Commit**

```bash
git add src/handler.rs src/startup.rs src/tools/workspace/commands/index.rs src/tests/daemon/mod.rs src/tests/daemon/roots.rs
git commit -m "feat(mcp): resolve primary workspace from client roots"
```

---

### Task 5: Handle `roots/list_changed`, keep secondary roots active, and document the new model

**Files:**
- Modify: `src/handler/session_workspace.rs`
- Modify: `src/handler.rs:776-926, 1350-1402`
- Modify: `src/tests/daemon/roots.rs`
- Modify: `docs/WORKSPACE_ARCHITECTURE.md`

- [ ] **Step 1: Write the failing roots-refresh tests**

Extend `src/tests/daemon/roots.rs`:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn test_roots_list_changed_marks_session_dirty_until_next_request() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root_a = temp.path().join("root-a");
    let root_b = temp.path().join("root-b");
    std::fs::create_dir_all(root_a.join(".git"))?;
    std::fs::create_dir_all(root_b.join(".git"))?;
    std::fs::write(root_a.join("lib.rs"), "pub fn root_a_marker() {}\n")?;
    std::fs::write(root_b.join("lib.rs"), "pub fn root_b_marker() {}\n")?;

    let handler = JulieServerHandler::new_for_daemon_session(
        WorkspaceStartupHint {
            path: PathBuf::from("/"),
            source: WorkspaceStartupSource::Cwd,
        },
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await?;

    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server = handler.clone();
    let server_handle = tokio::spawn(async move {
        server.serve(server_transport).await?.waiting().await?;
        anyhow::Ok(())
    });

    let roots = Arc::new(Mutex::new(vec![format!("file://{}", root_a.display())]));
    let client = RootsClient { roots: Arc::clone(&roots) }.serve(client_transport).await?;

    client
        .call_tool(CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "index", "force": true })
                .as_object()
                .unwrap()
                .clone(),
        ))
        .await?;

    roots.lock().unwrap().clear();
    roots.lock().unwrap().push(format!("file://{}", root_b.display()));
    client.notify_roots_list_changed().await?;

    assert!(handler.session_workspace.read().unwrap().roots_dirty());

    client
        .call_tool(CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "index", "force": true })
                .as_object()
                .unwrap()
                .clone(),
        ))
        .await?;

    let workspace = handler.get_workspace().await?.expect("workspace should rebind");
    assert_eq!(workspace.root.canonicalize()?, root_b.canonicalize()?);

    client.cancel().await?;
    server_handle.await??;
    Ok(())
}
```

- [ ] **Step 2: Run the focused tests to verify RED**

Run: `cargo test --lib test_roots_list_changed_marks_session_dirty_until_next_request 2>&1 | tail -10`
Expected: FAIL because `on_roots_list_changed`, dirty-state tracking, and request-bound refresh are not wired yet.

- [ ] **Step 3: Implement dirty-state handling, secondary-root activation, and docs**

Update `src/handler/session_workspace.rs` so the state can reconcile root snapshots:

```rust
pub fn apply_root_snapshot(
    &mut self,
    primary: PrimaryWorkspaceBinding,
    secondary_workspace_ids: HashSet<String>,
    roots: Vec<PathBuf>,
) {
    self.primary_binding = Some(primary);
    self.secondary_workspace_ids = secondary_workspace_ids;
    self.last_roots = roots;
    self.roots_dirty = false;
}

pub fn roots_dirty(&self) -> bool {
    self.roots_dirty
}

pub fn secondary_workspace_ids(&self) -> &HashSet<String> {
    &self.secondary_workspace_ids
}
```

Update `src/handler.rs`:

```rust
async fn on_roots_list_changed(&self, _context: NotificationContext<RoleServer>) {
    self.session_workspace.write().unwrap().mark_roots_dirty();
}

async fn bind_primary_workspace(
    &self,
    primary_root: PathBuf,
    all_roots: Vec<PathBuf>,
) -> Result<()> {
    let resolved_roots = all_roots.clone();
    let primary_root = primary_root.canonicalize().unwrap_or(primary_root);
    let primary_id = crate::workspace::registry::generate_workspace_id(&primary_root.to_string_lossy())?;

    let workspace = if let Some(pool) = &self.workspace_pool {
        pool.get_or_init(&primary_id, primary_root.clone()).await?
    } else {
        Arc::new(crate::workspace::JulieWorkspace::initialize(primary_root.clone()).await?)
    };

    {
        let mut guard = self.workspace.write().await;
        *guard = Some((*workspace).clone());
    }

    let mut secondary_ids = HashSet::new();
    for root in all_roots.into_iter().skip(1) {
        let canonical = root.canonicalize().unwrap_or(root);
        let workspace_id = crate::workspace::registry::generate_workspace_id(&canonical.to_string_lossy())?;
        if let Some(pool) = &self.workspace_pool {
            self.activate_workspace_with_root(&workspace_id, canonical.clone()).await?;
        }
        secondary_ids.insert(workspace_id);
    }

    self.session_workspace.write().unwrap().apply_root_snapshot(
        PrimaryWorkspaceBinding {
            workspace_id: primary_id,
            root: primary_root,
        },
        secondary_ids,
        resolved_roots,
    );
    Ok(())
}
```

Update `docs/WORKSPACE_ARCHITECTURE.md` with one new section that explains:

```markdown
## Startup Hint and MCP Roots

- Adapter sessions send both `WORKSPACE` and `WORKSPACE_SOURCE` to the daemon.
- `--workspace` and `JULIE_WORKSPACE` remain authoritative.
- `cwd` is a weak startup hint only.
- When the client advertises roots, Julie resolves the primary workspace on the first primary-scoped request.
- Additional roots are activated as explicit secondary workspaces for the session.
- `notifications/roots/list_changed` marks the session dirty; rebinding happens on the next request boundary, not mid-tool-call.
```

- [ ] **Step 4: Run the focused tests to verify GREEN**

Run: `cargo test --lib test_roots_list_changed_marks_session_dirty_until_next_request 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib test_manage_workspace_index_uses_roots_over_cwd_hint 2>&1 | tail -10`
Expected: PASS. Do not regress the first roots-binding case while adding refresh behavior.

- [ ] **Step 5: Commit**

```bash
git add src/handler/session_workspace.rs src/handler.rs src/tests/daemon/roots.rs docs/WORKSPACE_ARCHITECTURE.md
git commit -m "feat(mcp): refresh session roots on list change"
```

---

### Task 6: Run the full verification pass for startup and session behavior

**Files:**
- Modify: none
- Test: `src/tests/cli_tests.rs`, `src/tests/adapter/handshake.rs`, `src/tests/daemon/ipc_session.rs`, `src/tests/daemon/session_workspace.rs`, `src/tests/daemon/roots.rs`

- [ ] **Step 1: Run the focused roots and startup regression tests**

Run: `cargo test --lib test_resolve_workspace_startup_hint_ 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib test_build_ipc_header_includes_workspace_source 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib test_parse_ipc_headers_block_ 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib test_manage_workspace_index_uses_roots_over_cwd_hint 2>&1 | tail -10`
Expected: PASS.

Run: `cargo test --lib test_roots_list_changed_marks_session_dirty_until_next_request 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 2: Run the default local regression tier**

Run: `cargo xtask test dev`
Expected: PASS.

- [ ] **Step 3: Run the system tier because daemon startup and session lifecycle changed**

Run: `cargo xtask test system`
Expected: PASS.

- [ ] **Step 4: Commit the final verification-only changes if any test fixes were needed**

```bash
git add docs/WORKSPACE_ARCHITECTURE.md src/cli.rs src/main.rs src/adapter/mod.rs src/daemon/ipc_session.rs src/daemon/mod.rs src/handler/session_workspace.rs src/handler.rs src/startup.rs src/tools/workspace/commands/index.rs src/tests/cli_tests.rs src/tests/adapter/mod.rs src/tests/adapter/handshake.rs src/tests/daemon/mod.rs src/tests/daemon/ipc_session.rs src/tests/daemon/session_workspace.rs src/tests/daemon/roots.rs src/tests/integration/daemon_lifecycle.rs
git commit -m "refactor(startup): support roots-aware workspace binding"
```

---

## Self-Review

### Spec Coverage

- Startup hint and source preservation: Task 1 and Task 2.
- Precedence order and explicit override preservation: Task 1 and Task 4.
- Unbound `cwd` sessions: Task 3.
- Request-time `roots/list` resolution: Task 4.
- Multi-root secondary activation: Task 5.
- `roots/list_changed` handling: Task 5.
- Stable default tool semantics: Task 4 and Task 5 preserve `primary` as the implicit scope.
- Docs update: Task 5.

### Placeholder Scan

- No `TODO`, `TBD`, or “similar to Task N” shortcuts remain.
- Every code-writing step includes concrete file paths and code snippets.
- Every verification step uses an exact command and an expected result.

### Type Consistency

- Shared startup metadata is always `WorkspaceStartupHint` plus `WorkspaceStartupSource`.
- Mutable primary identity flows through `PrimaryWorkspaceBinding` and `SessionWorkspaceState`.
- Request-time resolver entry point is `ensure_primary_workspace_for_request(&Peer<RoleServer>)`.
- Dirty-state flow uses `mark_roots_dirty()`, `roots_dirty()`, and `apply_root_snapshot(...)` consistently.
