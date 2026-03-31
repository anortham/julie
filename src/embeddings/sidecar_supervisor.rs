use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use super::sidecar_bootstrap;

const DEFAULT_SIDECAR_MODULE: &str = "sidecar.main";
pub(crate) const SIDECAR_ROOT_ENV: &str = "JULIE_EMBEDDING_SIDECAR_ROOT";
const SIDECAR_VENV_ENV: &str = "JULIE_EMBEDDING_SIDECAR_VENV";
const SIDECAR_PROGRAM_ENV: &str = "JULIE_EMBEDDING_SIDECAR_PROGRAM";
const SIDECAR_RAW_PROGRAM_ENV: &str = "JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM";
const SIDECAR_SCRIPT_ENV: &str = "JULIE_EMBEDDING_SIDECAR_SCRIPT";
const SIDECAR_MODULE_ENV: &str = "JULIE_EMBEDDING_SIDECAR_MODULE";
const EMBEDDING_CACHE_DIR_ENV: &str = "JULIE_EMBEDDING_CACHE_DIR";
pub(crate) const INSTALL_MARKER_VERSION: &str = "v10-cuda-torch";
/// PyTorch publishes wheels for these minor versions (3.10 through 3.13).
pub(crate) const SUPPORTED_PYTHON_MINORS: [u32; 4] = [12, 13, 11, 10];
pub(crate) const RUNTIME_EDITABLE_REQUIREMENT: &str = ".[runtime]";

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
    sidecar_bootstrap::ensure_venv_exists(&venv_path)?;

    let venv_python = sidecar_bootstrap::managed_venv_python_path(&venv_path);
    sidecar_bootstrap::ensure_sidecar_package_installed(&venv_python, &venv_path, &sidecar_root)?;

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
    let extracted = super::sidecar_embedded::managed_sidecar_source_path();
    super::sidecar_embedded::extract_embedded_sidecar(&extracted)?;
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

pub(crate) fn managed_cache_base_dir() -> PathBuf {
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

fn launch_args(script_override: Option<String>, module: &str) -> Vec<String> {
    if let Some(script) = script_override {
        vec![script]
    } else {
        vec!["-m".to_string(), module.to_string()]
    }
}

pub(crate) fn build_program_override_launch_config(
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

pub(crate) fn is_truthy_env_flag(value: &str) -> bool {
    let normalized = value.trim();
    normalized == "1"
        || normalized.eq_ignore_ascii_case("true")
        || normalized.eq_ignore_ascii_case("on")
}

pub(crate) fn python_version_from_program(program: &std::ffi::OsStr) -> Option<(u32, u32)> {
    use std::process::Command;

    let mut cmd = Command::new(program);
    cmd.arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // On Windows, prevent a visible console window from flashing.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output().ok()?;

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

pub(crate) fn install_marker_value(sidecar_root: &Path) -> String {
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
