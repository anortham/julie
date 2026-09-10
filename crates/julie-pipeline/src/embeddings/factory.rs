use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, bail};

use super::native::launch::find_and_hash_sidecar_binary;
use super::{EmbeddingBackend, EmbeddingProvider, NativeEmbeddingProvider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendResolverCapabilities {
    pub native_available: bool,
    pub target_os: &'static str,
    pub target_arch: &'static str,
}

impl BackendResolverCapabilities {
    /// Probe the host: native semantics are available when the
    /// `julie-semantic-sidecar` binary can be found.
    pub fn detect(native_program: Option<&Path>) -> Self {
        Self {
            native_available: find_and_hash_sidecar_binary(native_program).is_ok(),
            target_os: std::env::consts::OS,
            target_arch: std::env::consts::ARCH,
        }
    }

    pub fn current() -> Self {
        Self::detect(None)
    }
}

/// Runtime configuration for embedding provider selection.
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub cache_dir: Option<PathBuf>,
    pub native_program: Option<PathBuf>,
    pub native_model: Option<String>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "auto".to_string(),
            cache_dir: None,
            native_program: None,
            native_model: None,
        }
    }
}

pub fn parse_provider_preference(provider: &str) -> Result<EmbeddingBackend> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(EmbeddingBackend::Auto),
        "native" => Ok(EmbeddingBackend::Native),
        "ort" | "sidecar" => bail!(
            "Embedding backend '{}' has been removed. Use 'auto', 'native', or 'none' instead.",
            provider.trim()
        ),
        unknown => bail!(
            "Unknown embedding provider: {} (valid: auto|native|none)",
            unknown
        ),
    }
}

pub fn strict_acceleration_enabled_from_env_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "on"
    )
}

pub fn should_disable_for_strict_acceleration(
    strict_acceleration: bool,
    resolved_backend: &EmbeddingBackend,
    accelerated: bool,
    degraded_reason: Option<&str>,
) -> bool {
    strict_acceleration
        && (!accelerated
            || degraded_reason.is_some()
            || matches!(resolved_backend, EmbeddingBackend::Unresolved))
}

/// `auto` becomes `native` when the sidecar binary is found and an error
/// (no provider) otherwise. An explicit `native` request always resolves so
/// the provider constructor can report the precise launch failure.
pub fn resolve_backend_preference(
    requested_backend: EmbeddingBackend,
    capabilities: &BackendResolverCapabilities,
) -> Result<EmbeddingBackend> {
    match requested_backend {
        EmbeddingBackend::Auto if capabilities.native_available => Ok(EmbeddingBackend::Native),
        EmbeddingBackend::Auto => bail!(
            "No embedding backend available for platform {}-{}: julie-semantic-sidecar binary not found",
            capabilities.target_os,
            capabilities.target_arch
        ),
        EmbeddingBackend::Native => Ok(EmbeddingBackend::Native),
        EmbeddingBackend::Unresolved => {
            bail!("Cannot resolve embedding backend from unresolved preference")
        }
        EmbeddingBackend::Invalid(provider) => {
            bail!("Cannot resolve embedding backend from invalid preference: {provider}")
        }
    }
}

pub struct EmbeddingProviderFactory;

impl EmbeddingProviderFactory {
    pub fn create(config: &EmbeddingConfig) -> Result<Arc<dyn EmbeddingProvider>> {
        let requested_backend = parse_provider_preference(&config.provider)?;
        let capabilities = BackendResolverCapabilities::detect(config.native_program.as_deref());
        match resolve_backend_preference(requested_backend, &capabilities)? {
            EmbeddingBackend::Native => Ok(Arc::new(NativeEmbeddingProvider::try_new(config)?)),
            backend => unreachable!(
                "resolve_backend_preference returned unsupported backend: {}",
                backend.as_str()
            ),
        }
    }
}
