//! Sidecar venv creation, Python detection, and package installation.
//!
//! This module handles the entire bootstrap lifecycle for the managed sidecar
//! environment: finding a compatible Python interpreter, creating a virtual
//! environment (preferring `uv` over `python -m venv`), and installing the
//! sidecar package via pip.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use tracing::warn;

use super::sidecar_supervisor::{RUNTIME_EDITABLE_REQUIREMENT, SUPPORTED_PYTHON_MINORS};

const SIDECAR_BOOTSTRAP_PYTHON_ENV: &str = "JULIE_EMBEDDING_SIDECAR_BOOTSTRAP_PYTHON";
const INSTALL_MARKER: &str = ".julie-sidecar-install-root";

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
        let status = cmd.status();
        if matches!(status, Ok(s) if s.success()) {
            return Some(Ok(()));
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
    let install_ok = install_cmd
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

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

    run_command(
        Command::new(&bootstrap_python)
            .arg("-m")
            .arg("venv")
            .arg(venv_path),
        "sidecar bootstrap failed to create managed venv",
    )
}

fn detect_bootstrap_python_interpreter() -> Result<OsString> {
    if let Some(override_value) = std::env::var_os(SIDECAR_BOOTSTRAP_PYTHON_ENV) {
        if !override_value.is_empty() {
            return Ok(override_value);
        }
    }

    // Try uv-managed Python installations first. uv maintains its own
    // Python cache and can locate versions that aren't on system PATH.
    // This is critical on Windows where the py launcher defaults to
    // the latest installed Python (which may be too new for PyTorch).
    for minor in SUPPORTED_PYTHON_MINORS {
        if let Some(path) = uv_python_find(minor) {
            return Ok(path);
        }
    }

    // Fall back to system Python candidates, but verify the version is
    // supported. Without this check, `py` on Windows picks 3.14+ which
    // has no PyTorch wheels.
    for candidate in python_interpreter_candidates() {
        if !command_exists(&candidate) {
            continue;
        }
        if let Some((_major, minor)) = super::sidecar_supervisor::python_version_from_program(&candidate) {
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

fn python_interpreter_candidates() -> Vec<OsString> {
    // Prefer 3.12 for best cross-platform compatibility (macOS, Windows, Linux)
    // with PyTorch and sentence-transformers. Fall back to newer/older versions.
    if cfg!(target_os = "windows") {
        vec![
            OsString::from("py"),
            OsString::from("python3.12"),
            OsString::from("python3.13"),
            OsString::from("python3.11"),
            OsString::from("python"),
            OsString::from("python3"),
        ]
    } else {
        vec![
            OsString::from("python3.12"),
            OsString::from("python3.13"),
            OsString::from("python3.11"),
            OsString::from("python3"),
            OsString::from("python"),
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
    //
    // Prefer `uv pip install` when available — it's faster and doesn't
    // require pip to be bundled in the venv (uv venv omits pip by design).
    if command_exists(OsStr::new("uv")) {
        run_command(
            Command::new("uv")
                .arg("pip")
                .arg("install")
                .arg("--python")
                .arg(venv_python)
                .arg("--editable")
                .arg(RUNTIME_EDITABLE_REQUIREMENT)
                .current_dir(sidecar_root),
            "sidecar bootstrap failed to install managed sidecar package",
        )?;
    } else {
        run_command(
            Command::new(venv_python)
                .arg("-m")
                .arg("pip")
                .arg("install")
                .arg("--disable-pip-version-check")
                .arg("--editable")
                .arg(RUNTIME_EDITABLE_REQUIREMENT)
                .current_dir(sidecar_root),
            "sidecar bootstrap failed to install managed sidecar package",
        )?;
    }

    std::fs::write(&marker_path, expected_marker).with_context(|| {
        format!(
            "sidecar bootstrap failed to update install marker '{}'",
            marker_path.display()
        )
    })?;

    Ok(())
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
