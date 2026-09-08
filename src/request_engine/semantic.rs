//! Semantic runtime abstraction, execution modes, requirements, and readiness reporting.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::embeddings::EmbeddingProvider;
use crate::paths::RegistryPaths;
use crate::request_engine::types::{RequestFailure, RequestReadiness, WorkspaceBinding};

pub const CURRENT_EMBEDDING_FORMAT_VERSION: u32 = 3;

/// Semantic retrieval execution mode across CLI and MCP transports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum SemanticMode {
    #[default]
    Auto,
    Off,
    Required,
}

/// Granular semantic requirement for a tool invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRequirement {
    #[default]
    None,
    Query,
    Symbols,
    QueryAndSymbols,
}

impl SemanticRequirement {
    pub fn requires_query(&self) -> bool {
        matches!(self, Self::Query | Self::QueryAndSymbols)
    }

    pub fn requires_symbols(&self) -> bool {
        matches!(self, Self::Symbols | Self::QueryAndSymbols)
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

/// Operational readiness evidence for semantic capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SemanticReadiness {
    Disabled,
    Starting,
    Ready {
        model_id: String,
        dimensions: usize,
        device: String,
    },
    Degraded {
        code: String,
        retryable: bool,
    },
}

impl SemanticReadiness {
    /// Convert semantic readiness to unified `RequestReadiness`.
    pub fn to_request_readiness(&self, mode: SemanticMode) -> RequestReadiness {
        match self {
            Self::Disabled => RequestReadiness {
                mode: SemanticMode::Off,
                status: "disabled".to_string(),
                coverage: None,
                canonical_revision: None,
                lexical_revision: None,
            },
            Self::Starting => RequestReadiness {
                mode,
                status: "starting".to_string(),
                coverage: Some("starting".to_string()),
                canonical_revision: None,
                lexical_revision: None,
            },
            Self::Ready { .. } => RequestReadiness {
                mode,
                status: "ready".to_string(),
                coverage: Some("full".to_string()),
                canonical_revision: None,
                lexical_revision: None,
            },
            Self::Degraded { code, .. } => RequestReadiness {
                mode,
                status: format!("degraded: {code}"),
                coverage: Some("missing".to_string()),
                canonical_revision: None,
                lexical_revision: None,
            },
        }
    }
}

/// Transport-independent runtime managing semantic readiness and health verification.
#[async_trait]
pub trait SemanticRuntime: Send + Sync {
    /// Ensure semantic capability is ready according to requirement and mode.
    async fn ensure_ready(
        &self,
        binding: &WorkspaceBinding,
        requirement: SemanticRequirement,
        mode: SemanticMode,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<SemanticReadiness, RequestFailure>;

    /// Retrieve the currently active or acquired embedding provider, if any.
    fn provider(&self) -> Option<Arc<dyn EmbeddingProvider>> {
        None
    }
}

/// Default no-op semantic runtime used for testing or disabled configurations.
#[derive(Debug, Clone, Default)]
pub struct NoopSemanticRuntime;

#[async_trait]
impl SemanticRuntime for NoopSemanticRuntime {
    async fn ensure_ready(
        &self,
        _binding: &WorkspaceBinding,
        requirement: SemanticRequirement,
        mode: SemanticMode,
        _deadline: Instant,
        _cancellation: &CancellationToken,
    ) -> Result<SemanticReadiness, RequestFailure> {
        if mode == SemanticMode::Off || requirement.is_none() {
            return Ok(SemanticReadiness::Disabled);
        }

        if mode == SemanticMode::Required {
            return Err(RequestFailure::new(
                "SEMANTICS_NOT_READY",
                "Semantics is required but no semantic runtime is configured",
                true,
                serde_json::json!({ "coverage": "missing", "reason": "noop_runtime" }),
            ));
        }

        Ok(SemanticReadiness::Degraded {
            code: "no_semantic_runtime".to_string(),
            retryable: false,
        })
    }
}

/// Standard implementation of `SemanticRuntime` backed by lazy acquisition and SQLite vector verification.
pub struct DefaultSemanticRuntime {
    registry_paths: Option<RegistryPaths>,
    provider_cache: Arc<tokio::sync::RwLock<Option<Arc<dyn EmbeddingProvider>>>>,
}

impl DefaultSemanticRuntime {
    pub fn new(provider: Option<Arc<dyn EmbeddingProvider>>) -> Self {
        Self {
            registry_paths: None,
            provider_cache: Arc::new(tokio::sync::RwLock::new(provider)),
        }
    }

    pub fn from_registry_paths(paths: RegistryPaths) -> Self {
        Self {
            registry_paths: Some(paths),
            provider_cache: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    pub fn provider(&self) -> Option<Arc<dyn EmbeddingProvider>> {
        self.provider_cache.try_read().ok().and_then(|g| g.clone())
    }

    async fn get_or_acquire_provider(
        &self,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<Option<Arc<dyn EmbeddingProvider>>, RequestFailure> {
        {
            let guard = self.provider_cache.read().await;
            if let Some(ref p) = *guard {
                return Ok(Some(Arc::clone(p)));
            }
        }

        let paths = match &self.registry_paths {
            Some(p) => p.clone(),
            None => return Ok(None),
        };

        let mut guard = self.provider_cache.write().await;
        if let Some(ref p) = *guard {
            return Ok(Some(Arc::clone(p)));
        }

        let acquire_fut = crate::server_in_process::acquire_in_process_embedding_provider(&paths);

        let acquired = tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                return Err(RequestFailure::cancelled("Semantic initialization cancelled"));
            }
            _ = tokio::time::sleep_until(deadline) => {
                return Err(RequestFailure::deadline_exceeded("Semantic initialization deadline exceeded"));
            }
            res = acquire_fut => res,
        };

        if let Some(ref p) = acquired {
            *guard = Some(Arc::clone(p));
        }

        Ok(acquired)
    }

    fn check_sqlite_vectors(
        db_path: &Path,
        provider: &dyn EmbeddingProvider,
        mode: SemanticMode,
    ) -> Result<SemanticReadiness, RequestFailure> {
        if !db_path.exists() {
            return match mode {
                SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                    "Workspace symbols database does not exist",
                    serde_json::json!({ "coverage": "missing" }),
                )),
                SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                    code: "DATABASE_MISSING".to_string(),
                    retryable: true,
                }),
                SemanticMode::Off => Ok(SemanticReadiness::Disabled),
            };
        }

        // Open read-only without acquiring write or exclusive locks
        let conn = rusqlite::Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| RequestFailure::internal(format!("Failed to open SQLite database: {e}")))?;

        // 1. Verify table presence
        let has_vectors: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='symbol_vectors'",
                [],
                |row| row.get::<_, i32>(0).map(|c| c > 0),
            )
            .unwrap_or(false);

        let has_config: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='embedding_config'",
                [],
                |row| row.get::<_, i32>(0).map(|c| c > 0),
            )
            .unwrap_or(false);

        let has_symbols: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='symbols'",
                [],
                |row| row.get::<_, i32>(0).map(|c| c > 0),
            )
            .unwrap_or(false);

        if !has_vectors || !has_config {
            return match mode {
                SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                    "Workspace vector storage tables missing",
                    serde_json::json!({ "coverage": "missing" }),
                )),
                SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                    code: "VECTORS_MISSING".to_string(),
                    retryable: true,
                }),
                SemanticMode::Off => Ok(SemanticReadiness::Disabled),
            };
        }

        // 2. Validate model & dimensions
        let config: Result<(String, usize, u32), _> = conn.query_row(
            "SELECT model_name, dimensions, format_version FROM embedding_config WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get::<_, i64>(1)? as usize,
                    row.get::<_, i64>(2)? as u32,
                ))
            },
        );

        let (stored_model, stored_dims, stored_fmt) = match config {
            Ok(c) => c,
            Err(_) => {
                return match mode {
                    SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                        "Embedding configuration row missing in SQLite",
                        serde_json::json!({ "coverage": "missing" }),
                    )),
                    SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                        code: "CONFIG_MISSING".to_string(),
                        retryable: true,
                    }),
                    SemanticMode::Off => Ok(SemanticReadiness::Disabled),
                };
            }
        };

        let dev_info = provider.device_info();
        let provider_dims = provider.dimensions();

        if stored_dims != provider_dims || stored_model != dev_info.model_name {
            return match mode {
                SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                    format!(
                        "Stored vectors are incompatible: stored {} ({}d) vs provider {} ({}d)",
                        stored_model, stored_dims, dev_info.model_name, provider_dims
                    ),
                    serde_json::json!({
                        "coverage": "incompatible",
                        "stored_model": stored_model,
                        "stored_dimensions": stored_dims,
                        "provider_model": dev_info.model_name,
                        "provider_dimensions": provider_dims,
                    }),
                )),
                SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                    code: "VECTORS_INCOMPATIBLE".to_string(),
                    retryable: true,
                }),
                SemanticMode::Off => Ok(SemanticReadiness::Disabled),
            };
        }

        // 3. Check vector count & staleness
        let vector_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbol_vectors", [], |row| row.get(0))
            .unwrap_or(0);

        let symbol_count: i64 = if has_symbols {
            conn.query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
                .unwrap_or(0)
        } else {
            0
        };

        if symbol_count > 0 && vector_count == 0 {
            return match mode {
                SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                    "Workspace has symbols but zero vector embeddings",
                    serde_json::json!({ "coverage": "missing" }),
                )),
                SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                    code: "VECTORS_MISSING".to_string(),
                    retryable: true,
                }),
                SemanticMode::Off => Ok(SemanticReadiness::Disabled),
            };
        }

        if stored_fmt < CURRENT_EMBEDDING_FORMAT_VERSION && symbol_count > 0 {
            return match mode {
                SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                    format!(
                        "Stored vectors are stale: format v{stored_fmt} < v{CURRENT_EMBEDDING_FORMAT_VERSION}"
                    ),
                    serde_json::json!({
                        "coverage": "stale",
                        "format_version": stored_fmt,
                        "expected_version": CURRENT_EMBEDDING_FORMAT_VERSION,
                    }),
                )),
                SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                    code: "VECTORS_STALE".to_string(),
                    retryable: true,
                }),
                SemanticMode::Off => Ok(SemanticReadiness::Disabled),
            };
        }

        Ok(SemanticReadiness::Ready {
            model_id: dev_info.model_name,
            dimensions: provider_dims,
            device: dev_info.device,
        })
    }
}

#[async_trait]
impl SemanticRuntime for DefaultSemanticRuntime {
    async fn ensure_ready(
        &self,
        binding: &WorkspaceBinding,
        requirement: SemanticRequirement,
        mode: SemanticMode,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<SemanticReadiness, RequestFailure> {
        if cancellation.is_cancelled() {
            return Err(RequestFailure::cancelled(
                "Semantic readiness check cancelled",
            ));
        }
        if Instant::now() >= deadline {
            return Err(RequestFailure::deadline_exceeded(
                "Semantic readiness deadline exceeded",
            ));
        }

        // Rule 1: Off mode performs zero provider/model/vector work
        if mode == SemanticMode::Off {
            return Ok(SemanticReadiness::Disabled);
        }

        // Rule 2: Requirement None performs zero work
        if requirement.is_none() {
            return Ok(SemanticReadiness::Disabled);
        }

        // Rule 3: Provider readiness check
        let provider = match self.get_or_acquire_provider(deadline, cancellation).await? {
            Some(p) => p,
            None => {
                return match mode {
                    SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                        "No embedding provider configured or available",
                        serde_json::json!({ "coverage": "missing", "reason": "provider_unavailable" }),
                    )),
                    SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                        code: "PROVIDER_UNAVAILABLE".to_string(),
                        retryable: true,
                    }),
                    SemanticMode::Off => Ok(SemanticReadiness::Disabled),
                };
            }
        };

        // If only Query is required, provider readiness is sufficient
        if requirement == SemanticRequirement::Query {
            let dev_info = provider.device_info();
            return Ok(SemanticReadiness::Ready {
                model_id: dev_info.model_name,
                dimensions: provider.dimensions(),
                device: dev_info.device,
            });
        }

        // Rule 4: Symbols or QueryAndSymbols requires SQLite vector compatibility
        let db_path = binding.index_root.join("db/symbols.db");
        Self::check_sqlite_vectors(&db_path, provider.as_ref(), mode)
    }

    fn provider(&self) -> Option<Arc<dyn EmbeddingProvider>> {
        self.provider_cache.try_read().ok().and_then(|g| g.clone())
    }
}
