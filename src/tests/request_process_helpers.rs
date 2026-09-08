//! Real-wire child process test fixture and helpers for Julie subprocess acceptance.
//!
//! Provides `ProcessFixture` for testing compiled `julie-server` binaries across
//! stdio MCP (JSON-RPC) and CLI transports with complete workspace and storage isolation.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::tests::helpers::workspace::make_isolated_workspace_root;

// ============================================================================
// 1. Binary Resolution
// ============================================================================

/// Resolve the path to the compiled `julie-server` binary.
///
/// Priority:
/// 1. `JULIE_TEST_BIN` environment variable (explicit override)
/// 2. `CARGO_BIN_EXE_julie-server` (set by Cargo during integration tests)
/// 3. `<manifest_dir>/target/debug/julie-server`
/// 4. `<manifest_dir>/target/release/julie-server`
pub fn resolve_julie_binary() -> PathBuf {
    if let Ok(bin) = std::env::var("JULIE_TEST_BIN") {
        let path = PathBuf::from(bin);
        if path.exists() {
            return path;
        }
    }

    if let Some(bin) = option_env!("CARGO_BIN_EXE_julie-server") {
        let path = PathBuf::from(bin);
        if path.exists() {
            return path;
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let debug_bin = manifest_dir.join(format!(
        "target/debug/julie-server{}",
        std::env::consts::EXE_SUFFIX
    ));
    if debug_bin.exists() {
        return debug_bin;
    }

    let release_bin = manifest_dir.join(format!(
        "target/release/julie-server{}",
        std::env::consts::EXE_SUFFIX
    ));
    if release_bin.exists() {
        return release_bin;
    }

    panic!(
        "julie-server binary not found! Please build it first with: \
         cargo build -p julie --bin julie-server \
         or set JULIE_TEST_BIN=/path/to/julie-server"
    );
}

// ============================================================================
// 2. Temporary Workspace Setup & Seeds
// ============================================================================

/// Populate isolated workspace root with seed files for exploration and refactoring.
pub fn seed_workspace_files(root: &Path) {
    // 1. Cargo.toml project marker
    let cargo_toml = r#"[package]
name = "request-probe-workspace"
version = "0.1.0"
edition = "2021"
"#;
    std::fs::write(root.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");

    // 2. Source tree
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");

    let lib_rs = r#"//! Request probe seed library for transport parity testing.

pub struct ProbeConfig {
    pub retries: u32,
    pub verbose: bool,
}

impl ProbeConfig {
    pub fn new(retries: u32) -> Self {
        Self {
            retries,
            verbose: false,
        }
    }

    pub fn is_verbose(&self) -> bool {
        self.verbose
    }
}

pub fn request_probe() -> String {
    "probe_active".to_string()
}

pub fn mid_step() -> String {
    request_probe()
}

pub fn entry_point() -> String {
    mid_step()
}
"#;
    std::fs::write(src_dir.join("lib.rs"), lib_rs).expect("write src/lib.rs");

    let main_rs = r#"//! Primary workspace main entry point

/// Calculate the sum of two numbers
pub fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

/// Primary workspace marker function
pub fn primary_marker_function() {
    println!("PRIMARY_WORKSPACE_MARKER");
}

fn main() {
    let result = calculate_sum(5, 3);
    println!("Sum: {}", result);
    primary_marker_function();
}
"#;
    std::fs::write(src_dir.join("main.rs"), main_rs).expect("write src/main.rs");

    let calculator_rs = r#"//! Secondary source file for editing and refactoring tests.

pub fn add(x: i32, y: i32) -> i32 {
    x + y
}

pub fn subtract(x: i32, y: i32) -> i32 {
    x - y
}
"#;
    std::fs::write(src_dir.join("calculator.rs"), calculator_rs).expect("write src/calculator.rs");
}

// ============================================================================
// 3. CLI Subprocess Invocation Output
// ============================================================================

/// Result of executing a CLI subcommand or generic tool invocation.
#[derive(Debug, Clone)]
pub struct CliOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CliOutput {
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    pub fn stdout_json(&self) -> Value {
        serde_json::from_str(&self.stdout).unwrap_or_else(|err| {
            panic!(
                "CLI stdout was not valid JSON: {:?}. Error: {err}.\nStderr: {}",
                self.stdout, self.stderr
            );
        })
    }
}

// ============================================================================
// 4. ProcessFixture Harness
// ============================================================================

/// Manages a spawned `julie-server` subprocess with isolated home and workspace.
pub struct ProcessFixture {
    pub binary_path: PathBuf,
    pub workspace_root: PathBuf,
    pub temp_repo: Arc<TempDir>,
    pub temp_home: Arc<TempDir>,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout_lines: Option<Lines<BufReader<ChildStdout>>>,
    stderr_task: Option<tokio::task::JoinHandle<()>>,
    stderr_buffer: Arc<std::sync::Mutex<Vec<String>>>,
    unsolicited_messages: Vec<Value>,
    next_request_id: AtomicU64,
}

impl ProcessFixture {
    /// Create a fixture resolving the compiled binary from the environment.
    pub async fn from_env() -> Self {
        let binary = resolve_julie_binary();
        Self::new(binary).await
    }

    /// Create a fixture with an explicit binary path.
    pub async fn new(binary_path: PathBuf) -> Self {
        let tmp_base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp");
                let _ = std::fs::create_dir_all(&fallback);
                fallback
            });
        let temp_repo = tempfile::tempdir_in(&tmp_base).expect("create temp repo dir");
        let workspace_root = make_isolated_workspace_root(temp_repo.path(), "request_probe");
        seed_workspace_files(&workspace_root);

        let temp_home = tempfile::tempdir_in(&tmp_base).expect("create temp home dir");

        let mut fixture = Self {
            binary_path,
            workspace_root,
            temp_repo: Arc::new(temp_repo),
            temp_home: Arc::new(temp_home),
            child: None,
            stdin: None,
            stdout_lines: None,
            stderr_task: None,
            stderr_buffer: Arc::new(std::sync::Mutex::new(Vec::new())),
            unsolicited_messages: Vec::new(),
            next_request_id: AtomicU64::new(1),
        };

        fixture.spawn_mcp_server().await;
        fixture
    }

    /// Spawn `julie-server` in stdio MCP mode.
    async fn spawn_mcp_server(&mut self) {
        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("--workspace").arg(&self.workspace_root);
        cmd.env("JULIE_HOME", self.temp_home.path());
        cmd.env("TMPDIR", self.temp_home.path());
        cmd.env("JULIE_EMBEDDING_PROVIDER", "none");
        cmd.current_dir(&self.workspace_root);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().expect("spawn julie-server MCP stdio process");
        let stdin = child.stdin.take().expect("capture child stdin");
        let stdout = child.stdout.take().expect("capture child stdout");
        let stderr = child.stderr.take().expect("capture child stderr");

        let stdout_lines = BufReader::new(stdout).lines();

        let stderr_buf = Arc::clone(&self.stderr_buffer);
        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut buf = stderr_buf.lock().unwrap();
                if buf.len() < 1000 {
                    buf.push(line);
                }
            }
        });

        self.child = Some(child);
        self.stdin = Some(stdin);
        self.stdout_lines = Some(stdout_lines);
        self.stderr_task = Some(stderr_task);
    }

    pub fn root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn root_str(&self) -> &str {
        self.workspace_root.to_str().expect("valid utf-8 root path")
    }

    pub fn home(&self) -> &Path {
        self.temp_home.path()
    }

    pub fn write_file(&self, relative_path: &str, content: &str) {
        let full_path = self.workspace_root.join(relative_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        std::fs::write(&full_path, content).expect("write file");
    }

    pub fn read_file(&self, relative_path: &str) -> String {
        let full_path = self.workspace_root.join(relative_path);
        std::fs::read_to_string(&full_path).expect("read file")
    }

    pub fn last_stderr_lines(&self, n: usize) -> Vec<String> {
        let buf = self.stderr_buffer.lock().unwrap();
        buf.iter()
            .rev()
            .take(n)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    // ------------------------------------------------------------------------
    // Transport A: MCP JSON-RPC Stdio Calls
    // ------------------------------------------------------------------------

    /// Send a JSON-RPC request to stdio MCP and wait for matching response ID.
    pub async fn rpc(&mut self, mut request: Value) -> Value {
        if request.get("jsonrpc").is_none() {
            if let Some(obj) = request.as_object_mut() {
                obj.insert("jsonrpc".to_string(), json!("2.0"));
            }
        }

        let req_id = match request.get("id") {
            Some(id) => id.clone(),
            None => {
                let id = json!(self.next_request_id.fetch_add(1, Ordering::SeqCst));
                if let Some(obj) = request.as_object_mut() {
                    obj.insert("id".to_string(), id.clone());
                }
                id
            }
        };

        if let Some(pos) = self
            .unsolicited_messages
            .iter()
            .position(|m| m.get("id") == Some(&req_id))
        {
            return self.unsolicited_messages.remove(pos);
        }

        let payload = serde_json::to_string(&request).expect("serialize JSON-RPC request");
        let stdin = self.stdin.as_mut().expect("child stdin available");
        stdin
            .write_all(payload.as_bytes())
            .await
            .expect("write request to stdin");
        stdin
            .write_all(b"\n")
            .await
            .expect("write newline to stdin");
        stdin.flush().await.expect("flush stdin");

        let stdout_lines = self.stdout_lines.as_mut().expect("child stdout available");
        let stderr_tail_fn = {
            let buf = Arc::clone(&self.stderr_buffer);
            move || {
                let lines = buf.lock().unwrap();
                lines
                    .iter()
                    .rev()
                    .take(20)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        };

        let wait_future = async {
            loop {
                let line = match stdout_lines.next_line().await {
                    Ok(Some(l)) => l,
                    Ok(None) => {
                        panic!(
                            "julie-server closed stdout unexpectedly (EOF).\nStderr tail:\n{}",
                            stderr_tail_fn()
                        );
                    }
                    Err(e) => {
                        panic!(
                            "I/O error reading child stdout: {e}.\nStderr tail:\n{}",
                            stderr_tail_fn()
                        );
                    }
                };

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let parsed: Value = serde_json::from_str(trimmed).unwrap_or_else(|err| {
                    panic!(
                        "Child process emitted non-JSON line on stdout: {:?}. Error: {err}.\nStderr tail:\n{}",
                        trimmed,
                        stderr_tail_fn()
                    );
                });

                if parsed.get("id") == Some(&req_id) {
                    return parsed;
                }

                self.unsolicited_messages.push(parsed);
            }
        };

        tokio::time::timeout(Duration::from_secs(10), wait_future)
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "RPC call timed out after 10s waiting for response ID {req_id}.\nStderr tail:\n{}",
                    stderr_tail_fn()
                );
            })
    }

    /// Send a one-way JSON-RPC notification to stdio MCP without waiting for a response.
    pub async fn notify(&mut self, mut notification: Value) {
        if notification.get("jsonrpc").is_none() {
            if let Some(obj) = notification.as_object_mut() {
                obj.insert("jsonrpc".to_string(), json!("2.0"));
            }
        }
        let payload = serde_json::to_string(&notification).expect("serialize notification");
        let stdin = self.stdin.as_mut().expect("child stdin available");
        stdin
            .write_all(payload.as_bytes())
            .await
            .expect("write notification to stdin");
        stdin
            .write_all(b"\n")
            .await
            .expect("write newline to stdin");
        stdin.flush().await.expect("flush stdin");
    }

    // ------------------------------------------------------------------------
    // Transport B: CLI Invocation Helpers
    // ------------------------------------------------------------------------

    /// Invoke a CLI command against the fixture's workspace and home.
    pub async fn cli(&self, args: &[&str]) -> CliOutput {
        self.cli_with_stdin(args, None).await
    }

    /// Invoke a CLI command supplying optional stdin bytes.
    pub async fn cli_with_stdin(&self, args: &[&str], stdin_data: Option<&[u8]>) -> CliOutput {
        let mut cmd = Command::new(&self.binary_path);
        cmd.args(args);
        cmd.env("JULIE_HOME", self.temp_home.path());
        cmd.env("TMPDIR", self.temp_home.path());
        cmd.env("JULIE_EMBEDDING_PROVIDER", "none");
        cmd.current_dir(&self.workspace_root);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        if stdin_data.is_some() {
            cmd.stdin(std::process::Stdio::piped());
        } else {
            cmd.stdin(std::process::Stdio::null());
        }

        let result = tokio::time::timeout(Duration::from_secs(15), async {
            let mut child = cmd.spawn().expect("spawn CLI command");
            if let Some(data) = stdin_data {
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(data).await.expect("write stdin");
                    stdin.flush().await.expect("flush stdin");
                }
            }
            child
                .wait_with_output()
                .await
                .expect("wait for CLI process")
        })
        .await
        .unwrap_or_else(|_| {
            panic!("CLI command '{args:?}' timed out after 15s");
        });

        CliOutput {
            exit_code: result.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&result.stdout).to_string(),
            stderr: String::from_utf8_lossy(&result.stderr).to_string(),
        }
    }

    /// Invoke a CLI command expecting a JSON envelope, returning `(exit_code, stdout_json)`.
    pub async fn cli_json(&self, args: &[&str]) -> (i32, Value) {
        let out = self.cli(args).await;
        let json = out.stdout_json();
        (out.exit_code, json)
    }

    /// Invoke generic `tool <name>` with JSON parameters.
    pub async fn cli_tool(&self, name: &str, params: &Value) -> CliOutput {
        let params_str = serde_json::to_string(params).expect("serialize params");
        self.cli(&[
            "tool",
            name,
            "--params",
            &params_str,
            "--workspace",
            self.root_str(),
            "--standalone",
            "--json",
        ])
        .await
    }

    /// Invoke a named subcommand (e.g. `search`, `symbols`).
    pub async fn cli_subcommand(&self, subcommand: &str, args: &[&str]) -> CliOutput {
        let mut full_args = vec![subcommand];
        full_args.extend_from_slice(args);
        full_args.extend_from_slice(&["--workspace", self.root_str(), "--standalone", "--json"]);
        self.cli(&full_args).await
    }

    // ------------------------------------------------------------------------
    // Process Cleanup & Teardown
    // ------------------------------------------------------------------------

    /// Gracefully shutdown the stdio MCP process.
    pub async fn shutdown(&mut self) {
        self.stdin.take();

        if let Some(mut child) = self.child.take() {
            let wait_result = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
            match wait_result {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => eprintln!("Warning: error waiting for child: {e}"),
                Err(_) => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                }
            }
        }

        if let Some(task) = self.stderr_task.take() {
            task.abort();
        }
    }
}

impl Drop for ProcessFixture {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
        if let Some(task) = self.stderr_task.take() {
            task.abort();
        }
    }
}

// ============================================================================
// 5. Parity Normalization Helpers
// ============================================================================

/// Extract the normalized tool result payload from a CLI JSON envelope.
pub fn extract_cli_result(envelope: &Value) -> Value {
    let mut result = envelope["reply"]["result"].clone();
    if let Some(obj) = result.as_object_mut() {
        obj.remove("duration_ms");
    }
    result
}

/// Extract the normalized tool result payload from an MCP JSON-RPC response.
pub fn extract_mcp_result(response: &Value) -> Value {
    let mut result = response["result"].clone();
    if let Some(obj) = result.as_object_mut() {
        // Modern MCP 2026-07-28 includes resultType: "complete" on the wire
        obj.remove("resultType");
    }
    result
}

/// Assert content and error parity between CLI envelope and MCP response.
pub fn assert_transport_parity(cli_envelope: &Value, mcp_response: &Value) {
    let cli_res = extract_cli_result(cli_envelope);
    let mcp_res = extract_mcp_result(mcp_response);

    let cli_err = cli_res
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mcp_err = mcp_res
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert_eq!(cli_err, mcp_err, "isError mismatch between CLI and MCP");

    let cli_content = cli_res.get("content").unwrap_or(&Value::Null);
    let mcp_content = mcp_res.get("content").unwrap_or(&Value::Null);
    assert_eq!(
        cli_content, mcp_content,
        "content mismatch between CLI and MCP"
    );

    let cli_structured = cli_res.get("structuredContent");
    let mcp_structured = mcp_res.get("structuredContent");
    assert_eq!(
        cli_structured, mcp_structured,
        "structuredContent mismatch between CLI and MCP"
    );
}
