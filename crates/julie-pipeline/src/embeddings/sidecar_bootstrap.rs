//! Sidecar venv creation, Python detection, and package installation.
//!
//! This module handles the entire bootstrap lifecycle for the managed sidecar
//! environment: finding a compatible Python interpreter, creating a virtual
//! environment (preferring `uv` over `python -m venv`), and installing the
//! sidecar package via pip.

use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use tracing::warn;

use super::sidecar_supervisor::{RUNTIME_EDITABLE_REQUIREMENT, SUPPORTED_PYTHON_MINORS};

const SIDECAR_BOOTSTRAP_PYTHON_ENV: &str = "JULIE_EMBEDDING_SIDECAR_BOOTSTRAP_PYTHON";
const INSTALL_MARKER: &str = ".julie-sidecar-install-root";
const BOOTSTRAP_LOCK: &str = ".julie-sidecar-bootstrap.lock";

pub(super) struct SidecarBootstrapLock {
    file: File,
}

impl Drop for SidecarBootstrapLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub(super) fn acquire_bootstrap_lock(venv_path: &Path) -> Result<SidecarBootstrapLock> {
    let lock_dir = venv_path.parent().unwrap_or(venv_path);
    std::fs::create_dir_all(lock_dir).with_context(|| {
        format!(
            "sidecar bootstrap failed to create lock directory '{}'",
            lock_dir.display()
        )
    })?;

    let lock_path = lock_dir.join(BOOTSTRAP_LOCK);
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| {
            format!(
                "sidecar bootstrap failed to open lock '{}'",
                lock_path.display()
            )
        })?;
    file.lock_exclusive()
        .with_context(|| format!("sidecar bootstrap failed to lock '{}'", lock_path.display()))?;

    Ok(SidecarBootstrapLock { file })
}

/// On Windows, prevent spawned processes from opening visible console windows.
#[cfg(windows)]
fn suppress_console_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn suppress_console_window(_cmd: &mut Command) {
    // No-op on Unix — child processes inherit the parent's terminal.
}

/// Detect whether NVIDIA CUDA is available by probing for nvidia-smi.
/// This is a build-time check (run during venv creation), not a runtime check.
/// The Python sidecar handles runtime device selection via torch.cuda.is_available().
pub fn detect_nvidia_cuda() -> bool {
    let mut cmd = Command::new("nvidia-smi");
    cmd.arg("--query-gpu=driver_version")
        .arg("--format=csv,noheader")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    suppress_console_window(&mut cmd);
    cmd.status().is_ok_and(|s| s.success())
}

/// PyTorch CUDA wheel index URL. Uses CUDA 12.4 which supports
/// Ampere (RTX 30xx, A-series) and newer architectures.
pub fn cuda_torch_index_url() -> &'static str {
    "https://download.pytorch.org/whl/cu124"
}

/// Detect whether an AMD ROCm stack is present by probing for `rocminfo`
/// (which enumerates GPUs) and falling back to the conventional `/opt/rocm`
/// install prefix. Build-time check (run during venv creation); the Python
/// sidecar still makes the final runtime device decision. ROCm's HIP layer
/// reports through `torch.cuda.is_available()`, so runtime selection is shared
/// with the CUDA path.
pub fn detect_amd_rocm() -> bool {
    let mut cmd = Command::new("rocminfo");
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    suppress_console_window(&mut cmd);
    if cmd.status().is_ok_and(|s| s.success()) {
        return true;
    }
    // rocminfo absent or no GPU enumerated — fall back to the install prefix.
    Path::new("/opt/rocm").exists()
}

/// PyTorch ROCm wheel index URL. ROCm 6.4 pairs with the current stable torch
/// line and supports RDNA/CDNA GPUs on Linux. Bump this alongside the torch
/// version the same way [`cuda_torch_index_url`] tracks CUDA releases; a
/// version mismatch degrades gracefully to CPU torch (see
/// [`reinstall_torch_from_index`]).
pub fn rocm_torch_index_url() -> &'static str {
    "https://download.pytorch.org/whl/rocm6.4"
}

/// Detect whether an Intel GPU compute stack is present by probing for
/// `xpu-smi` (Intel's GPU System Management Interface, analogous to
/// `nvidia-smi`). We deliberately gate on `xpu-smi` rather than the mere
/// presence of an Intel render node: ubiquitous older integrated GPUs are not
/// XPU-capable, and a false positive would swap working CPU torch for an XPU
/// build that can't initialize. Build-time check; the sidecar confirms via
/// `torch.xpu.is_available()` at runtime.
pub fn detect_intel_xpu() -> bool {
    let mut cmd = Command::new("xpu-smi");
    cmd.arg("discovery")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    suppress_console_window(&mut cmd);
    cmd.status().is_ok_and(|s| s.success())
}

/// PyTorch Intel GPU (XPU) wheel index URL. Intel GPU support is upstreamed
/// into stable torch (2.5+), so no `intel-extension-for-pytorch` is required;
/// the wheels live at the unversioned `/whl/xpu` path.
pub fn xpu_torch_index_url() -> &'static str {
    "https://download.pytorch.org/whl/xpu"
}

/// Read the currently installed torch version from the venv so we can
/// pin the same version when swapping for the CUDA variant.
fn read_installed_torch_version(venv_python: &Path) -> Option<String> {
    let mut cmd = Command::new(venv_python);
    cmd.arg("-c")
        .arg("import torch; v=torch.__version__; print(v.split('+')[0])")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    suppress_console_window(&mut cmd);
    let output = cmd.output().ok()?;
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !version.is_empty() {
            return Some(version);
        }
    }
    None
}

/// Try creating the sidecar venv with `uv venv --python 3.X`.
///
/// Returns `Some(Ok(()))` on success, `Some(Err(_))` if uv ran but the venv
/// is still missing, or `None` if no compatible Python could be located or
/// installed (caller should fall back to `python -m venv`).
fn try_uv_venv(venv_path: &Path) -> Option<Result<()>> {
    // Try each supported version — uv will find both its own managed
    // installs and system interpreters.
    for minor in SUPPORTED_PYTHON_MINORS {
        let mut cmd = Command::new("uv");
        cmd.arg("venv")
            .arg("--python")
            .arg(format!("3.{minor}"))
            .arg(venv_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        suppress_console_window(&mut cmd);
        match cmd.output() {
            Ok(out) if out.status.success() => return Some(Ok(())),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                tracing::debug!(
                    "uv venv --python 3.{minor} failed (exit {}): {stderr}",
                    out.status
                );
            }
            Err(e) => tracing::debug!("uv venv --python 3.{minor} could not be run: {e}"),
        }
    }

    // No compatible Python available — auto-install the preferred version.
    let preferred = SUPPORTED_PYTHON_MINORS[0]; // 3.12
    tracing::info!("No Python 3.10-3.13 found — installing Python 3.{preferred} via uv");
    let mut install_cmd = Command::new("uv");
    install_cmd
        .arg("python")
        .arg("install")
        .arg(format!("3.{preferred}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    suppress_console_window(&mut install_cmd);
    let install_ok = install_cmd.status().map(|s| s.success()).unwrap_or(false);

    if !install_ok {
        warn!("uv python install 3.{preferred} failed");
        return None;
    }

    // Retry venv creation with the freshly installed interpreter.
    let mut retry_cmd = Command::new("uv");
    retry_cmd
        .arg("venv")
        .arg("--python")
        .arg(format!("3.{preferred}"))
        .arg(venv_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    suppress_console_window(&mut retry_cmd);
    let status = retry_cmd.status();
    if matches!(status, Ok(s) if s.success()) {
        return Some(Ok(()));
    }

    warn!("uv venv --python 3.{preferred} failed even after installing");
    None
}

pub(super) fn ensure_venv_exists(venv_path: &Path) -> Result<()> {
    let venv_python = managed_venv_python_path(venv_path);
    if venv_python.exists() {
        // Verify the Python version is supported by PyTorch. If the venv
        // was created with a too-new Python (e.g. 3.14), nuke it and
        // recreate with a supported version.
        if let Some((_major, minor)) = python_version(&venv_python) {
            if SUPPORTED_PYTHON_MINORS.contains(&minor) {
                return Ok(());
            }
            warn!(
                "Managed sidecar venv has Python 3.{minor} which lacks \
                 PyTorch wheels (need 3.10-3.13). Recreating venv..."
            );
            std::fs::remove_dir_all(venv_path).with_context(|| {
                format!(
                    "sidecar bootstrap failed to remove stale venv at '{}'",
                    venv_path.display()
                )
            })?;
        } else {
            return Ok(()); // Can't determine version — keep existing venv
        }
    }

    if let Some(parent) = venv_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("sidecar bootstrap failed to create '{}'", parent.display())
        })?;
    }

    // Prefer `uv venv` over `python -m venv`. uv's standalone/portable
    // Python builds have a placeholder sys.base_prefix ('/install') that
    // doesn't exist on disk — `python -m venv` copies that broken prefix
    // into the new venv, producing a non-functional environment.
    // `uv venv --python` handles this correctly because uv knows the real
    // layout of its own managed interpreters.
    if command_exists(OsStr::new("uv")) {
        if let Some(result) = try_uv_venv(venv_path) {
            return result;
        }
    }

    let bootstrap_python = detect_bootstrap_python_interpreter()?;

    let mut create_cmd = bootstrap_python.command();
    create_cmd.arg("-m").arg("venv").arg(venv_path);
    run_command(
        &mut create_cmd,
        "sidecar bootstrap failed to create managed venv",
    )
}

fn detect_bootstrap_python_interpreter() -> Result<PythonCommand> {
    if let Some(override_value) = std::env::var_os(SIDECAR_BOOTSTRAP_PYTHON_ENV) {
        if !override_value.is_empty() {
            return Ok(PythonCommand::from_program(override_value));
        }
    }

    // Try uv-managed Python installations first. uv maintains its own
    // Python cache and can locate versions that aren't on system PATH.
    // This is critical on Windows where the py launcher defaults to
    // the latest installed Python (which may be too new for PyTorch).
    for minor in SUPPORTED_PYTHON_MINORS {
        if let Some(path) = uv_python_find(minor) {
            return Ok(PythonCommand::from_program(path));
        }
    }

    // Fall back to system Python candidates, but verify the version is
    // supported. Without this check, `py` on Windows picks 3.14+ which
    // has no PyTorch wheels.
    for candidate in python_interpreter_candidates() {
        if !candidate.exists() {
            continue;
        }
        if let Some((_major, minor)) = candidate.version() {
            if SUPPORTED_PYTHON_MINORS.contains(&minor) {
                return Ok(candidate);
            }
        } else {
            // Can't determine version — use it anyway as a last resort
            return Ok(candidate);
        }
    }

    // Auto-install is handled by try_uv_venv() in ensure_venv_exists.
    // This path is only reached when uv is unavailable or the caller
    // bypassed the uv venv flow (e.g. JULIE_SIDECAR_PROGRAM override).
    bail!(
        "sidecar bootstrap failed: no Python 3.10-3.13 interpreter found \
         (PyTorch does not support newer versions yet). \
         Install Python 3.12 or run: uv python install 3.12"
    )
}

/// A candidate Python invocation: a program plus any leading arguments needed
/// to select a specific interpreter. On Windows the launcher selects a version
/// via `py -3.12`, which is an argument, not a distinct binary — so a bare
/// program name is insufficient.
#[derive(Clone, Debug)]
pub struct PythonCommand {
    program: OsString,
    prefix_args: Vec<OsString>,
}

impl PythonCommand {
    fn plain(program: &str) -> Self {
        Self {
            program: OsString::from(program),
            prefix_args: Vec::new(),
        }
    }

    fn versioned(program: &str, version_arg: &str) -> Self {
        Self {
            program: OsString::from(program),
            prefix_args: vec![OsString::from(version_arg)],
        }
    }

    fn from_program(program: OsString) -> Self {
        Self {
            program,
            prefix_args: Vec::new(),
        }
    }

    pub fn program(&self) -> &OsStr {
        &self.program
    }

    pub fn prefix_args(&self) -> &[OsString] {
        &self.prefix_args
    }

    /// Build a [`Command`] seeded with the program and its version-selecting
    /// prefix args, ready for the caller to append further arguments.
    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix_args);
        cmd
    }

    /// Whether this interpreter responds successfully to `--version`.
    fn exists(&self) -> bool {
        let mut cmd = self.command();
        cmd.arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        suppress_console_window(&mut cmd);
        cmd.status().is_ok_and(|status| status.success())
    }

    /// Parse the interpreter's `(major, minor)` version.
    fn version(&self) -> Option<(u32, u32)> {
        super::sidecar_supervisor::python_version_from_command(&self.program, &self.prefix_args)
    }
}

fn python_interpreter_candidates() -> Vec<PythonCommand> {
    python_interpreter_candidates_for(cfg!(target_os = "windows"))
}

/// Build the ordered interpreter probe list for a target platform. Split out
/// from [`python_interpreter_candidates`] so both branches are unit-testable
/// regardless of the host OS.
pub fn python_interpreter_candidates_for(target_windows: bool) -> Vec<PythonCommand> {
    // Prefer 3.12 for best cross-platform compatibility (macOS, Windows, Linux)
    // with PyTorch and sentence-transformers. Fall back to newer/older versions.
    if target_windows {
        // The Windows launcher (`py`) is the canonical way to select a specific
        // version. `py -3.12` requests exactly that interpreter; bare
        // `python3.12` binaries usually don't exist on Windows. Probe each
        // supported minor through the launcher first, then fall back to plain
        // names for non-launcher setups.
        let mut candidates: Vec<PythonCommand> = SUPPORTED_PYTHON_MINORS
            .iter()
            .map(|minor| PythonCommand::versioned("py", &format!("-3.{minor}")))
            .collect();
        candidates.extend([
            PythonCommand::plain("py"),
            PythonCommand::plain("python3.12"),
            PythonCommand::plain("python3.13"),
            PythonCommand::plain("python3.11"),
            PythonCommand::plain("python"),
            PythonCommand::plain("python3"),
        ]);
        candidates
    } else {
        vec![
            PythonCommand::plain("python3.12"),
            PythonCommand::plain("python3.13"),
            PythonCommand::plain("python3.11"),
            PythonCommand::plain("python3"),
            PythonCommand::plain("python"),
        ]
    }
}

fn command_exists(program: &OsStr) -> bool {
    let mut cmd = Command::new(program);
    cmd.arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    suppress_console_window(&mut cmd);
    cmd.status().is_ok_and(|status| status.success())
}

/// Ask `uv python find 3.{minor}` for a Python interpreter path.
/// Returns `None` if uv isn't installed or doesn't have that version.
fn uv_python_find(minor: u32) -> Option<OsString> {
    let mut cmd = Command::new("uv");
    cmd.arg("python")
        .arg("find")
        .arg(format!("3.{minor}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    suppress_console_window(&mut cmd);
    let output = cmd.output().ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() || !Path::new(&path).exists() {
        return None;
    }

    Some(OsString::from(path))
}

/// Parse `(major, minor)` from the `--version` output of a Python executable.
fn python_version(python_path: &Path) -> Option<(u32, u32)> {
    super::sidecar_supervisor::python_version_from_program(python_path.as_os_str())
}

pub(super) fn managed_venv_python_path(venv_path: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        venv_path.join("Scripts").join("python.exe")
    } else {
        venv_path.join("bin").join("python")
    }
}

pub(super) fn ensure_sidecar_package_installed(
    venv_python: &Path,
    venv_path: &Path,
    sidecar_root: &Path,
) -> Result<()> {
    if !sidecar_root.exists() {
        bail!(
            "sidecar bootstrap failed: sidecar root '{}' does not exist",
            sidecar_root.display()
        );
    }

    let pyproject_path = sidecar_root.join("pyproject.toml");
    if !pyproject_path.exists() {
        bail!(
            "sidecar bootstrap failed: '{}' is missing pyproject.toml",
            sidecar_root.display()
        );
    }

    let marker_path = venv_path.join(INSTALL_MARKER);
    let expected_marker = super::sidecar_supervisor::install_marker_value(sidecar_root);
    if marker_path.exists()
        && std::fs::read_to_string(&marker_path)
            .ok()
            .as_deref()
            .is_some_and(|value| value == expected_marker)
    {
        return Ok(());
    }

    // Install the sidecar package and all deps. On Windows, pyproject.toml
    // includes torch-directml which provides GPU acceleration via DirectX 12
    // for NVIDIA, AMD, and Intel GPUs — no CUDA download required.
    let (mut cmd, _) = pip_install_command(venv_python);
    cmd.arg("--editable")
        .arg(RUNTIME_EDITABLE_REQUIREMENT)
        .current_dir(sidecar_root);
    run_command(
        &mut cmd,
        "sidecar bootstrap failed to install managed sidecar package",
    )?;

    // After the base install (which pulls torch+cpu from PyPI), swap torch for
    // an accelerated build when a supported GPU stack is detected. All other
    // deps (sentence-transformers, etc.) stay on the PyPI versions resolved
    // above; only torch is reinstalled from the vendor wheel index.
    if let Some((index_url, label)) = detect_gpu_torch_index() {
        reinstall_torch_from_index(venv_python, index_url, label);
    }

    std::fs::write(&marker_path, expected_marker).with_context(|| {
        format!(
            "sidecar bootstrap failed to update install marker '{}'",
            marker_path.display()
        )
    })?;

    Ok(())
}

/// Select the accelerated torch wheel index for the detected GPU stack, or
/// `None` to keep the CPU torch from the base install.
///
/// Returns `(index_url, human_label)`. Detection runs in priority order and
/// stops at the first hit, so a machine is only ever matched to one backend.
///
/// Platform policy:
/// - **Windows**: torch-directml (declared in pyproject.toml) already covers
///   NVIDIA/AMD/Intel via DirectX 12, so only the CUDA index is worth swapping
///   in for the dedicated-NVIDIA path. ROCm/XPU have no stable Windows wheels.
/// - **Linux**: NVIDIA (CUDA) → AMD (ROCm) → Intel (XPU).
/// - **macOS**: MPS is built into the PyPI torch wheel, so no swap is needed.
fn detect_gpu_torch_index() -> Option<(&'static str, &'static str)> {
    if cfg!(target_os = "windows") {
        if detect_nvidia_cuda() {
            return Some((cuda_torch_index_url(), "CUDA"));
        }
        return None;
    }

    if cfg!(target_os = "linux") {
        if detect_nvidia_cuda() {
            return Some((cuda_torch_index_url(), "CUDA"));
        }
        if detect_amd_rocm() {
            return Some((rocm_torch_index_url(), "ROCm"));
        }
        if detect_intel_xpu() {
            return Some((xpu_torch_index_url(), "Intel XPU"));
        }
    }

    None
}

/// Reinstall torch from a vendor wheel index, pinning the version already
/// resolved by the base install so the rest of the dependency tree stays
/// consistent. Non-fatal: on failure the CPU torch from the base install
/// remains and the sidecar falls back gracefully at runtime.
fn reinstall_torch_from_index(venv_python: &Path, index_url: &str, label: &str) {
    tracing::info!("{label} detected, installing {label}-enabled torch from {index_url}");

    let torch_version = read_installed_torch_version(venv_python);

    let (mut cmd, is_uv) = pip_install_command(venv_python);
    if is_uv {
        cmd.arg("--reinstall-package").arg("torch");
    } else {
        cmd.arg("--force-reinstall");
    }
    if let Some(ref ver) = torch_version {
        cmd.arg(format!("torch=={ver}"));
    } else {
        cmd.arg("torch");
    }
    cmd.arg("--index-url").arg(index_url);

    match run_command(&mut cmd, &format!("{label} torch install")) {
        Ok(()) => tracing::info!("{label}-enabled torch installed successfully"),
        Err(err) => {
            // Non-fatal: CPU torch still works, sidecar falls back gracefully.
            tracing::warn!("{label} torch install failed (CPU fallback available): {err:#}");
        }
    }
}

/// Build a `pip install` command, preferring `uv pip install` when available.
/// Returns the command and whether `uv` was selected (callers may need
/// uv-specific flags like `--reinstall-package`).
fn pip_install_command(venv_python: &Path) -> (Command, bool) {
    if command_exists(OsStr::new("uv")) {
        let mut cmd = Command::new("uv");
        cmd.arg("pip")
            .arg("install")
            .arg("--python")
            .arg(venv_python);
        (cmd, true)
    } else {
        let mut cmd = Command::new(venv_python);
        cmd.arg("-m")
            .arg("pip")
            .arg("install")
            .arg("--disable-pip-version-check");
        (cmd, false)
    }
}

fn run_command(command: &mut Command, error_context: &str) -> Result<()> {
    suppress_console_window(command);
    let program = command.get_program().to_string_lossy().to_string();
    let args = command
        .get_args()
        .map(OsStr::to_string_lossy)
        .collect::<Vec<_>>()
        .join(" ");

    let output = command.output().with_context(|| {
        if args.is_empty() {
            format!("{error_context}: failed to execute '{program}'")
        } else {
            format!("{error_context}: failed to execute '{program} {args}'")
        }
    })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let status = output.status;

    if stderr.trim().is_empty() {
        bail!("{error_context}: command exited with status {status}");
    }

    bail!(
        "{error_context}: command exited with status {status}: {}",
        stderr.trim()
    )
}
