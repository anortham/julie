use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use include_dir::{Dir, include_dir};
use tracing::warn;

const DEFAULT_SIDECAR_MODULE: &str = "sidecar.main";
pub(crate) const SIDECAR_ROOT_ENV: &str = "JULIE_EMBEDDING_SIDECAR_ROOT";
const SIDECAR_VENV_ENV: &str = "JULIE_EMBEDDING_SIDECAR_VENV";
const SIDECAR_PROGRAM_ENV: &str = "JULIE_EMBEDDING_SIDECAR_PROGRAM";
const SIDECAR_RAW_PROGRAM_ENV: &str = "JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM";
const SIDECAR_SCRIPT_ENV: &str = "JULIE_EMBEDDING_SIDECAR_SCRIPT";
const SIDECAR_MODULE_ENV: &str = "JULIE_EMBEDDING_SIDECAR_MODULE";
const SIDECAR_BOOTSTRAP_PYTHON_ENV: &str = "JULIE_EMBEDDING_SIDECAR_BOOTSTRAP_PYTHON";
const EMBEDDING_CACHE_DIR_ENV: &str = "JULIE_EMBEDDING_CACHE_DIR";
const INSTALL_MARKER: &str = ".julie-sidecar-install-root";
pub(crate) const INSTALL_MARKER_VERSION: &str = "v7-directml-simple";
/// PyTorch publishes wheels for these minor versions (3.10 through 3.13).
const SUPPORTED_PYTHON_MINORS: [u32; 4] = [12, 13, 11, 10];
const RUNTIME_EDITABLE_REQUIREMENT: &str = ".[runtime]";

static EMBEDDED_SIDECAR: Dir = include_dir!("$CARGO_MANIFEST_DIR/python/embeddings_sidecar");
const EMBEDDED_VERSION_MARKER: &str = ".embedded-version";

#[derive(Debug, Clone)]
pub struct SidecarLaunchConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(OsString, OsString)>,
}

pub fn build_sidecar_launch_config() -> Result<SidecarLaunchConfig> {
    let sidecar_root = sidecar_root_path()?;
    let script_override = std::env::var(SIDECAR_SCRIPT_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let module = std::env::var(SIDECAR_MODULE_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SIDECAR_MODULE.to_string());

    if let Some(program_override) = std::env::var_os(SIDECAR_PROGRAM_ENV) {
        let program = PathBuf::from(program_override);
        let raw_program_mode = std::env::var(SIDECAR_RAW_PROGRAM_ENV)
            .ok()
            .as_deref()
            .is_some_and(is_truthy_env_flag);

        return build_program_override_launch_config(
            program,
            script_override,
            &module,
            &sidecar_root,
            raw_program_mode,
        );
    }

    let venv_path = managed_venv_path();
    ensure_venv_exists(&venv_path)?;

    let venv_python = managed_venv_python_path(&venv_path);
    ensure_sidecar_package_installed(&venv_python, &venv_path, &sidecar_root)?;

    Ok(SidecarLaunchConfig {
        program: venv_python,
        args: launch_args(script_override, &module),
        env: Vec::new(),
    })
}

pub fn sidecar_root_path() -> Result<PathBuf> {
    // Priority 1: Env var override
    if let Some(root_override) = std::env::var_os(SIDECAR_ROOT_ENV) {
        return Ok(PathBuf::from(root_override));
    }

    // Priority 2: Adjacent to binary (for packagers)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let adjacent = exe_dir.join("python").join("embeddings_sidecar");
            if adjacent.join("pyproject.toml").exists() {
                return Ok(adjacent);
            }
        }
    }

    // Priority 3: Source checkout (dev mode)
    let source_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("python")
        .join("embeddings_sidecar");
    if source_dir.join("pyproject.toml").exists() {
        return Ok(source_dir);
    }

    // Priority 4: Extract embedded files to cache
    let extracted = managed_sidecar_source_path();
    extract_embedded_sidecar(&extracted)?;
    Ok(extracted)
}

pub fn managed_venv_path() -> PathBuf {
    if let Some(venv_override) = std::env::var_os(SIDECAR_VENV_ENV) {
        return PathBuf::from(venv_override);
    }

    managed_cache_base_dir()
        .join("embeddings")
        .join("sidecar")
        .join("venv")
}

fn managed_cache_base_dir() -> PathBuf {
    if let Some(configured_cache_dir) = std::env::var_os(EMBEDDING_CACHE_DIR_ENV) {
        return PathBuf::from(configured_cache_dir);
    }

    if let Some(xdg_cache_home) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(xdg_cache_home).join("julie");
    }

    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local_app_data).join("julie");
    }

    if let Some(app_data) = std::env::var_os("APPDATA") {
        return PathBuf::from(app_data).join("julie");
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".cache").join("julie");
    }

    std::env::temp_dir().join("julie")
}

fn managed_sidecar_source_path() -> PathBuf {
    managed_cache_base_dir()
        .join("embeddings")
        .join("sidecar")
        .join("source")
}

/// Extract the compile-time embedded sidecar source to `target_dir`.
///
/// Skips extraction if the version marker already matches [`INSTALL_MARKER_VERSION`].
/// Directories named `tests` or `__pycache__` are excluded from extraction.
pub fn extract_embedded_sidecar(target_dir: &Path) -> Result<()> {
    let marker_path = target_dir.join(EMBEDDED_VERSION_MARKER);

    // Check version marker — skip if up-to-date
    if marker_path.is_file() {
        if let Ok(contents) = std::fs::read_to_string(&marker_path) {
            if contents.trim() == INSTALL_MARKER_VERSION {
                return Ok(());
            }
        }
    }

    // Extract all files recursively, skipping test/cache directories
    extract_dir_recursive(&EMBEDDED_SIDECAR, target_dir)
        .context("extracting embedded sidecar source")?;

    // Write version marker
    std::fs::write(&marker_path, INSTALL_MARKER_VERSION)
        .context("writing embedded version marker")?;

    Ok(())
}

/// Recursively extract files from an embedded `Dir`, skipping `tests` and `__pycache__`.
fn extract_dir_recursive(dir: &Dir, target: &Path) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(sub_dir) => {
                let dir_name = sub_dir
                    .path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                // Skip test and cache directories
                if dir_name == "tests" || dir_name == "__pycache__" || dir_name == ".pytest_cache"
                {
                    continue;
                }

                let sub_target = target.join(sub_dir.path());
                std::fs::create_dir_all(&sub_target).with_context(|| {
                    format!("creating directory {}", sub_target.display())
                })?;
                extract_dir_recursive(sub_dir, target)?;
            }
            include_dir::DirEntry::File(file) => {
                let file_target = target.join(file.path());
                if let Some(parent) = file_target.parent() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!("creating parent directory {}", parent.display())
                    })?;
                }
                std::fs::write(&file_target, file.contents()).with_context(|| {
                    format!("writing file {}", file_target.display())
                })?;
            }
        }
    }
    Ok(())
}

fn managed_venv_python_path(venv_path: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        venv_path.join("Scripts").join("python.exe")
    } else {
        venv_path.join("bin").join("python")
    }
}

fn launch_args(script_override: Option<String>, module: &str) -> Vec<String> {
    if let Some(script) = script_override {
        vec![script]
    } else {
        vec!["-m".to_string(), module.to_string()]
    }
}

fn build_program_override_launch_config(
    program: PathBuf,
    script_override: Option<String>,
    module: &str,
    sidecar_root: &Path,
    raw_program_mode: bool,
) -> Result<SidecarLaunchConfig> {
    if raw_program_mode {
        return Ok(SidecarLaunchConfig {
            program,
            args: Vec::new(),
            env: Vec::new(),
        });
    }

    let args = launch_args(script_override.clone(), module);
    let env = if script_override.is_some() {
        Vec::new()
    } else {
        vec![(
            OsString::from("PYTHONPATH"),
            build_pythonpath_with_root(sidecar_root)
                .context("sidecar bootstrap failed to prepare PYTHONPATH")?,
        )]
    };

    Ok(SidecarLaunchConfig { program, args, env })
}

fn is_truthy_env_flag(value: &str) -> bool {
    let normalized = value.trim();
    normalized == "1"
        || normalized.eq_ignore_ascii_case("true")
        || normalized.eq_ignore_ascii_case("on")
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
        let status = Command::new("uv")
            .arg("venv")
            .arg("--python")
            .arg(format!("3.{minor}"))
            .arg(venv_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .status();
        if matches!(status, Ok(s) if s.success()) {
            return Some(Ok(()));
        }
    }

    // No compatible Python available — auto-install the preferred version.
    let preferred = SUPPORTED_PYTHON_MINORS[0]; // 3.12
    tracing::info!("No Python 3.10-3.13 found — installing Python 3.{preferred} via uv");
    let install_ok = Command::new("uv")
        .arg("python")
        .arg("install")
        .arg(format!("3.{preferred}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !install_ok {
        warn!("uv python install 3.{preferred} failed");
        return None;
    }

    // Retry venv creation with the freshly installed interpreter.
    let status = Command::new("uv")
        .arg("venv")
        .arg("--python")
        .arg(format!("3.{preferred}"))
        .arg(venv_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status();
    if matches!(status, Ok(s) if s.success()) {
        return Some(Ok(()));
    }

    warn!("uv venv --python 3.{preferred} failed even after installing");
    None
}

fn ensure_venv_exists(venv_path: &Path) -> Result<()> {
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
        if let Some((_major, minor)) = python_version_from_program(&candidate) {
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
    Command::new(program)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Ask `uv python find 3.{minor}` for a Python interpreter path.
/// Returns `None` if uv isn't installed or doesn't have that version.
fn uv_python_find(minor: u32) -> Option<OsString> {
    let output = Command::new("uv")
        .arg("python")
        .arg("find")
        .arg(format!("3.{minor}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

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
    python_version_from_program(python_path.as_os_str())
}

fn python_version_from_program(program: &OsStr) -> Option<(u32, u32)> {
    let output = Command::new(program)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    // Python prints to stdout (3.4+) or stderr (older).
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    // Parse "Python 3.12.1" → (3, 12)
    let version_str = text.trim().strip_prefix("Python ")?;
    let mut parts = version_str.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    Some((major, minor))
}

fn ensure_sidecar_package_installed(
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
    let expected_marker = install_marker_value(sidecar_root);
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

fn install_marker_value(sidecar_root: &Path) -> String {
    format!(
        "version={}\nroot={}\n",
        INSTALL_MARKER_VERSION,
        sidecar_root.to_string_lossy()
    )
}

fn build_pythonpath_with_root(sidecar_root: &Path) -> Result<OsString> {
    let mut entries = vec![sidecar_root.to_path_buf()];
    if let Some(existing) = std::env::var_os("PYTHONPATH") {
        entries.extend(std::env::split_paths(&existing));
    }

    std::env::join_paths(entries).map_err(|err| {
        anyhow!(
            "sidecar bootstrap failed while constructing PYTHONPATH with root '{}': {}",
            sidecar_root.display(),
            err
        )
    })
}

fn run_command(command: &mut Command, error_context: &str) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::{
        INSTALL_MARKER_VERSION, RUNTIME_EDITABLE_REQUIREMENT, SUPPORTED_PYTHON_MINORS,
        build_program_override_launch_config, install_marker_value, is_truthy_env_flag,
        python_version_from_program,
    };
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn test_install_marker_value_includes_version_and_root() {
        let marker = install_marker_value(Path::new("/tmp/sidecar"));
        assert!(marker.contains(&format!("version={INSTALL_MARKER_VERSION}")));
        assert!(marker.contains("root=/tmp/sidecar"));
    }

    #[test]
    fn test_runtime_editable_requirement_targets_runtime_extras() {
        assert_eq!(RUNTIME_EDITABLE_REQUIREMENT, ".[runtime]");
    }

    #[test]
    fn test_supported_python_minors_covers_pytorch_range() {
        // PyTorch supports 3.10-3.13 as of early 2026.
        assert!(SUPPORTED_PYTHON_MINORS.contains(&10));
        assert!(SUPPORTED_PYTHON_MINORS.contains(&11));
        assert!(SUPPORTED_PYTHON_MINORS.contains(&12));
        assert!(SUPPORTED_PYTHON_MINORS.contains(&13));
        // 3.14+ is not supported yet.
        assert!(!SUPPORTED_PYTHON_MINORS.contains(&14));
    }

    #[test]
    fn test_python_version_parses_current_interpreter() {
        // Use whatever Python is available in CI/dev to smoke-test parsing.
        let candidates: &[&str] = if cfg!(target_os = "windows") {
            &["py", "python"]
        } else {
            &["python3", "python"]
        };
        for &name in candidates {
            if let Some((major, minor)) = python_version_from_program(OsStr::new(name)) {
                assert_eq!(major, 3, "Expected Python 3.x");
                assert!(minor >= 10, "Expected Python 3.10+, got 3.{minor}");
                return;
            }
        }
        // No Python found — skip rather than fail (CI might not have Python).
    }

    #[test]
    fn test_program_override_raw_mode_uses_no_implicit_args() {
        let launch = build_program_override_launch_config(
            Path::new("/usr/bin/env").to_path_buf(),
            None,
            "custom.module",
            Path::new("/tmp/sidecar"),
            true,
        )
        .expect("raw override launch should build");

        assert_eq!(launch.program, Path::new("/usr/bin/env"));
        assert!(
            launch.args.is_empty(),
            "raw mode should not add implicit args: {:?}",
            launch.args
        );
        assert!(
            launch.env.is_empty(),
            "raw mode should not inject env vars: {:?}",
            launch.env
        );
    }

    #[test]
    fn test_program_override_without_raw_mode_keeps_python_entrypoint_args() {
        let launch = build_program_override_launch_config(
            Path::new("/usr/bin/env").to_path_buf(),
            None,
            "custom.module",
            Path::new("/tmp/sidecar"),
            false,
        )
        .expect("override launch should build");

        assert_eq!(launch.program, Path::new("/usr/bin/env"));
        assert_eq!(
            launch.args,
            vec!["-m".to_string(), "custom.module".to_string()]
        );
        assert_eq!(launch.env.len(), 1, "expected PYTHONPATH to be injected");
        assert_eq!(launch.env[0].0, "PYTHONPATH");
    }

    #[test]
    fn test_is_truthy_env_flag_accepts_expected_values() {
        for value in ["1", " true ", "TRUE", "on", "On"] {
            assert!(
                is_truthy_env_flag(value),
                "expected value '{value}' to be truthy"
            );
        }
    }

    #[test]
    fn test_is_truthy_env_flag_rejects_non_truthy_values() {
        for value in ["", "0", "false", "off", "yes"] {
            assert!(
                !is_truthy_env_flag(value),
                "expected value '{value}' to be non-truthy"
            );
        }
    }
}
