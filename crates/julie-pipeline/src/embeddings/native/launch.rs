//! Sidecar binary discovery, SHA-256 verification, and process launching.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

/// Default model identifier for native embeddings.
pub const DEFAULT_NATIVE_MODEL: &str = "bge-small-en-v1.5-f32";

/// Locates the native sidecar binary and computes its SHA-256 checksum.
pub fn find_and_hash_sidecar_binary(explicit: Option<&Path>) -> Result<(PathBuf, String)> {
    let candidate = if let Some(path) = explicit {
        path.to_path_buf()
    } else if let Ok(env_path) = std::env::var("JULIE_NATIVE_SIDECAR_PROGRAM") {
        PathBuf::from(env_path)
    } else {
        find_default_sidecar_binary()?
    };

    if !candidate.is_file() {
        bail!(
            "NATIVE_SIDECAR_MISSING: sidecar executable not found at '{}'",
            candidate.display()
        );
    }

    let mut file = File::open(&candidate).map_err(|e| {
        anyhow::anyhow!(
            "NATIVE_SIDECAR_MISSING: failed to open sidecar binary '{}': {e}",
            candidate.display()
        )
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let sha256 = hex::encode(hasher.finalize());

    Ok((candidate, sha256))
}

fn find_default_sidecar_binary() -> Result<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join("julie-semantic-sidecar"));
            #[cfg(windows)]
            candidates.push(parent.join("julie-semantic-sidecar.exe"));
        }
    }
    if let Some(cache) = dirs::cache_dir() {
        candidates.push(cache.join("julie-semantic/bin/julie-semantic-sidecar"));
        #[cfg(windows)]
        candidates.push(cache.join("julie-semantic/bin/julie-semantic-sidecar.exe"));
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            candidates.push(dir.join("julie-semantic-sidecar"));
            #[cfg(windows)]
            candidates.push(dir.join("julie-semantic-sidecar.exe"));
        }
    }
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| {
        anyhow::anyhow!(
            "NATIVE_SIDECAR_MISSING: unable to locate julie-semantic-sidecar binary in PATH, cache, or target"
        )
    })
}

/// Executes `julie-semantic-sidecar prepare --model <model_id>` to ensure weights exist in cache.
pub fn run_prepare(executable: &Path, cache_root: &Path, model_id: &str) -> Result<()> {
    let mut cmd = Command::new(executable);
    cmd.arg("prepare")
        .arg("--model")
        .arg(model_id)
        .env("JULIE_EMBEDDING_CACHE_DIR", cache_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| {
        anyhow::anyhow!("NATIVE_SIDECAR_MISSING: failed to execute sidecar prepare: {e}")
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "MODEL_NOT_PREPARED: sidecar prepare failed with exit code {:?}: {stderr}",
            output.status.code()
        );
    }

    Ok(())
}

/// Resolved configuration for launching a native sidecar child.
#[derive(Clone, Debug)]
pub struct NativeLaunchConfig {
    pub executable_path: PathBuf,
    pub executable_sha256: String,
    pub model_id: String,
    pub cache_root: PathBuf,
}

impl NativeLaunchConfig {
    /// Resolves executable, checksum, model identifier, and cache directory.
    pub fn try_new(
        explicit_program: Option<&Path>,
        model_id: Option<&str>,
        custom_cache: Option<&Path>,
    ) -> Result<Self> {
        let (executable_path, executable_sha256) = find_and_hash_sidecar_binary(explicit_program)?;
        let model = model_id.unwrap_or(DEFAULT_NATIVE_MODEL).to_string();

        let cache_root = if let Some(custom) = custom_cache {
            custom.to_path_buf()
        } else if let Ok(env_cache) = std::env::var("JULIE_EMBEDDING_CACHE_DIR") {
            PathBuf::from(env_cache)
        } else {
            dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from(".cache"))
                .join("julie-semantic")
        };

        Ok(Self {
            executable_path,
            executable_sha256,
            model_id: model,
            cache_root,
        })
    }
}
