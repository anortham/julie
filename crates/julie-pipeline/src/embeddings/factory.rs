use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, bail};

#[cfg(feature = "embeddings-sidecar")]
use super::SidecarEmbeddingProvider;
use super::{EmbeddingBackend, EmbeddingProvider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendResolverCapabilities {
    pub sidecar_available: bool,
    pub target_os: &'static str,
    pub target_arch: &'static str,
}

impl BackendResolverCapabilities {
    pub fn current() -> Self {
        Self {
            sidecar_available: cfg!(feature = "embeddings-sidecar"),
            target_os: std::env::consts::OS,
            target_arch: std::env::consts::ARCH,
        }
    }

    fn is_available(self, backend: EmbeddingBackend) -> bool {
        match backend {
            EmbeddingBackend::Sidecar => self.sidecar_available,
            _ => false,
        }
    }
}

/// Runtime configuration for embedding provider selection.
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub cache_dir: Option<PathBuf>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "auto".to_string(),
            cache_dir: None,
        }
    }
}

pub fn parse_provider_preference(provider: &str) -> Result<EmbeddingBackend> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(EmbeddingBackend::Auto),
        "sidecar" => Ok(EmbeddingBackend::Sidecar),
        "ort" => bail!("ORT embedding backend has been removed. Use 'auto' or 'sidecar' instead."),
        unknown => bail!(
            "Unknown embedding provider: {} (valid: auto|sidecar)",
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

pub fn resolve_backend_preference(
    requested_backend: EmbeddingBackend,
    capabilities: &BackendResolverCapabilities,
) -> Result<EmbeddingBackend> {
    let resolved_backend = match requested_backend {
        EmbeddingBackend::Auto => {
            if capabilities.sidecar_available {
                EmbeddingBackend::Sidecar
            } else {
                bail!(
                    "No embedding backend available for platform {}-{}",
                    capabilities.target_os,
                    capabilities.target_arch
                )
            }
        }
        EmbeddingBackend::Sidecar => EmbeddingBackend::Sidecar,
        EmbeddingBackend::Unresolved => {
            bail!("Cannot resolve embedding backend from unresolved preference")
        }
        EmbeddingBackend::Invalid(provider) => {
            bail!("Cannot resolve embedding backend from invalid preference: {provider}")
        }
    };

    if !capabilities.is_available(resolved_backend.clone()) {
        bail!(
            "Embedding backend '{}' (requested '{}') is not available for platform {}-{} in this build",
            resolved_backend.as_str(),
            requested_backend.as_str(),
            capabilities.target_os,
            capabilities.target_arch,
        );
    }

    Ok(resolved_backend)
}

pub struct EmbeddingProviderFactory;

impl EmbeddingProviderFactory {
    pub fn create(config: &EmbeddingConfig) -> Result<Arc<dyn EmbeddingProvider>> {
        let requested_backend = parse_provider_preference(&config.provider)?;
        let resolved_backend =
            resolve_backend_preference(requested_backend, &BackendResolverCapabilities::current())?;

        match resolved_backend {
            EmbeddingBackend::Sidecar => {
                #[cfg(feature = "embeddings-sidecar")]
                {
                    return Ok(Arc::new(SidecarEmbeddingProvider::try_new()?));
                }

                #[cfg(not(feature = "embeddings-sidecar"))]
                {
                    bail!("Embedding provider 'sidecar' is not available in this build");
                }
            }
            backend => {
                unreachable!(
                    "resolve_backend_preference returned unsupported backend: {}",
                    backend.as_str()
                )
            }
        }
    }
}
