use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Result};

#[cfg(feature = "embeddings-candle")]
use super::CandleEmbeddingProvider;
#[cfg(feature = "embeddings-ort")]
use super::OrtEmbeddingProvider;
#[cfg(feature = "embeddings-sidecar")]
use super::SidecarEmbeddingProvider;
use super::{EmbeddingBackend, EmbeddingProvider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendResolverCapabilities {
    pub sidecar_available: bool,
    pub ort_available: bool,
    pub candle_available: bool,
    pub target_os: &'static str,
    pub target_arch: &'static str,
}

impl BackendResolverCapabilities {
    pub fn current() -> Self {
        Self {
            sidecar_available: cfg!(feature = "embeddings-sidecar"),
            ort_available: cfg!(feature = "embeddings-ort"),
            candle_available: cfg!(feature = "embeddings-candle"),
            target_os: std::env::consts::OS,
            target_arch: std::env::consts::ARCH,
        }
    }

    fn is_available(self, backend: EmbeddingBackend) -> bool {
        match backend {
            EmbeddingBackend::Sidecar => self.sidecar_available,
            EmbeddingBackend::Ort => self.ort_available,
            EmbeddingBackend::Candle => self.candle_available,
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
        "ort" => Ok(EmbeddingBackend::Ort),
        "candle" => Ok(EmbeddingBackend::Candle),
        unknown => bail!(
            "Unknown embedding provider: {} (valid: auto|sidecar|ort|candle)",
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

pub fn fallback_backend_after_init_failure(
    requested_backend: EmbeddingBackend,
    resolved_backend: EmbeddingBackend,
    strict_acceleration: bool,
    capabilities: BackendResolverCapabilities,
) -> Option<EmbeddingBackend> {
    if strict_acceleration {
        return None;
    }

    if requested_backend == EmbeddingBackend::Auto {
        if resolved_backend == EmbeddingBackend::Sidecar {
            if capabilities.ort_available {
                return Some(EmbeddingBackend::Ort);
            }
            if capabilities.candle_available {
                return Some(EmbeddingBackend::Candle);
            }
        }
        if resolved_backend == EmbeddingBackend::Candle && capabilities.ort_available {
            return Some(EmbeddingBackend::Ort);
        }
        if resolved_backend == EmbeddingBackend::Ort && capabilities.candle_available {
            return Some(EmbeddingBackend::Candle);
        }
    }

    None
}

pub fn resolve_backend_preference(
    requested_backend: EmbeddingBackend,
    capabilities: &BackendResolverCapabilities,
) -> Result<EmbeddingBackend> {
    let resolved_backend = match requested_backend {
        EmbeddingBackend::Auto => {
            if capabilities.sidecar_available {
                EmbeddingBackend::Sidecar
            } else if capabilities.ort_available {
                EmbeddingBackend::Ort
            } else if capabilities.candle_available {
                EmbeddingBackend::Candle
            } else {
                bail!(
                    "No embedding backend available for platform {}-{}",
                    capabilities.target_os,
                    capabilities.target_arch
                )
            }
        }
        EmbeddingBackend::Sidecar => EmbeddingBackend::Sidecar,
        EmbeddingBackend::Ort => EmbeddingBackend::Ort,
        EmbeddingBackend::Candle => EmbeddingBackend::Candle,
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
            EmbeddingBackend::Ort => {
                #[cfg(feature = "embeddings-ort")]
                {
                    return Ok(Arc::new(OrtEmbeddingProvider::try_new(
                        config.cache_dir.clone(),
                    )?));
                }

                #[cfg(not(feature = "embeddings-ort"))]
                {
                    bail!("Embedding provider 'ort' is not available in this build");
                }
            }
            EmbeddingBackend::Candle => {
                #[cfg(feature = "embeddings-candle")]
                {
                    return Ok(Arc::new(CandleEmbeddingProvider::try_new(
                        config.cache_dir.clone(),
                    )?));
                }

                #[cfg(not(feature = "embeddings-candle"))]
                {
                    bail!("Embedding provider 'candle' is not available in this build");
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
