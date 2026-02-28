use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};

const DEFAULT_SIDECAR_MODULE: &str = "sidecar.main";
const SIDECAR_ROOT_ENV: &str = "JULIE_EMBEDDING_SIDECAR_ROOT";
const SIDECAR_VENV_ENV: &str = "JULIE_EMBEDDING_SIDECAR_VENV";
const SIDECAR_PROGRAM_ENV: &str = "JULIE_EMBEDDING_SIDECAR_PROGRAM";
const SIDECAR_SCRIPT_ENV: &str = "JULIE_EMBEDDING_SIDECAR_SCRIPT";
const SIDECAR_MODULE_ENV: &str = "JULIE_EMBEDDING_SIDECAR_MODULE";
const SIDECAR_BOOTSTRAP_PYTHON_ENV: &str = "JULIE_EMBEDDING_SIDECAR_BOOTSTRAP_PYTHON";
const EMBEDDING_CACHE_DIR_ENV: &str = "JULIE_EMBEDDING_CACHE_DIR";
const INSTALL_MARKER: &str = ".julie-sidecar-install-root";
const INSTALL_MARKER_VERSION: &str = "v2-runtime-extras";
const RUNTIME_EDITABLE_REQUIREMENT: &str = ".[runtime]";

#[derive(Debug, Clone)]
pub struct SidecarLaunchConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(OsString, OsString)>,
}

pub fn build_sidecar_launch_config() -> Result<SidecarLaunchConfig> {
    let sidecar_root = sidecar_root_path();
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
        let args = launch_args(script_override.clone(), &module);
        let env = if script_override.is_some() {
            Vec::new()
        } else {
            vec![(
                OsString::from("PYTHONPATH"),
                build_pythonpath_with_root(&sidecar_root)
                    .context("sidecar bootstrap failed to prepare PYTHONPATH")?,
            )]
        };

        return Ok(SidecarLaunchConfig { program, args, env });
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

pub fn sidecar_root_path() -> PathBuf {
    if let Some(root_override) = std::env::var_os(SIDECAR_ROOT_ENV) {
        return PathBuf::from(root_override);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("python")
        .join("embeddings_sidecar")
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

fn ensure_venv_exists(venv_path: &Path) -> Result<()> {
    let venv_python = managed_venv_python_path(venv_path);
    if venv_python.exists() {
        return Ok(());
    }

    if let Some(parent) = venv_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("sidecar bootstrap failed to create '{}'", parent.display())
        })?;
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

    for candidate in python_interpreter_candidates() {
        if command_exists(&candidate) {
            return Ok(candidate);
        }
    }

    bail!(
        "sidecar bootstrap failed: no Python interpreter found on PATH (tried: {})",
        python_interpreter_candidates()
            .iter()
            .map(|candidate| candidate.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(", ")
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

    let mut install_cmd = Command::new(venv_python);
    install_cmd
        .arg("-m")
        .arg("pip")
        .arg("install")
        .arg("--disable-pip-version-check")
        .arg("--editable")
        .arg(RUNTIME_EDITABLE_REQUIREMENT)
        .current_dir(sidecar_root);

    run_command(
        &mut install_cmd,
        "sidecar bootstrap failed to install managed sidecar package",
    )?;

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
    use super::{install_marker_value, INSTALL_MARKER_VERSION, RUNTIME_EDITABLE_REQUIREMENT};
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
}
