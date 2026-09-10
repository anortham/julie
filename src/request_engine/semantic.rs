//! Semantic runtime abstraction, execution modes, requirements, and readiness reporting.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::embeddings::{EmbeddingProvider, EncoderIdentity};
use crate::paths::RegistryPaths;
use crate::request_engine::types::{RequestFailure, RequestReadiness, WorkspaceBinding};

pub use julie_core::CURRENT_EMBEDDING_FORMAT_VERSION;

/// Semantic retrieval execution mode across CLI and MCP transports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum SemanticMode {
    #[default]
    Auto,
    Off,
    Required,
}

/// Determine whether a semantic execution mode requires an embedding provider.
///
/// Returns `false` for `Off` (zero provider work, zero sidecar launch),
/// and `true` for `Auto` and `Required`.
#[inline]
pub fn semantic_mode_needs_provider(mode: SemanticMode) -> bool {
    !matches!(mode, SemanticMode::Off)
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

/// Unified runtime provider state machine governing lifecycle, late sidecar attachment, and dynamic recovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RuntimeProviderState {
    /// Semantics explicitly disabled (mode=Off or JULIE_EMBEDDING_PROVIDER=none/off/disabled).
    Disabled,

    /// Provider is actively initializing or acquiring the sidecar child.
    Starting,

    /// Provider is active, connected, and ready to serve embeddings.
    Ready,

    /// Provider is degraded.
    /// retryable: true for transient network/socket/boot timeouts.
    /// retryable: false for permanent configuration/model incompatibilities.
    Degraded { reason: String, retryable: bool },
}

impl RuntimeProviderState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Starting => true,
            Self::Degraded { retryable, .. } => *retryable,
            _ => false,
        }
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
        #[serde(skip_serializing_if = "Option::is_none")]
        encoder_identity: Option<EncoderIdentity>,
        #[serde(skip_serializing_if = "Option::is_none")]
        vector_generation: Option<i64>,
        #[serde(default)]
        eligible_symbols: usize,
        #[serde(default)]
        embedded_symbols: usize,
    },
    Degraded {
        #[serde(alias = "code")]
        reason: String,
        retryable: bool,
    },
}

impl SemanticReadiness {
    /// Access degradation reason if degraded.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Degraded { reason, .. } => Some(reason),
            _ => None,
        }
    }

    /// Access code (alias for reason).
    pub fn code(&self) -> Option<&str> {
        self.reason()
    }

    /// Check if ready.
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    /// Convert semantic readiness to unified `RequestReadiness`.
    pub fn to_request_readiness(&self, mode: SemanticMode) -> RequestReadiness {
        let (mode, status, coverage) = match self {
            Self::Disabled => (SemanticMode::Off, "disabled".to_string(), None),
            Self::Starting => (mode, "starting".to_string(), Some("starting".to_string())),
            Self::Ready {
                eligible_symbols,
                embedded_symbols,
                ..
            } => {
                let cov = if *eligible_symbols == 0 || *embedded_symbols >= *eligible_symbols {
                    "full".to_string()
                } else {
                    format!("{}/{}", embedded_symbols, eligible_symbols)
                };
                (mode, "ready".to_string(), Some(cov))
            }
            Self::Degraded { reason, .. } => (
                mode,
                format!("degraded: {reason}"),
                Some("missing".to_string()),
            ),
        };
        RequestReadiness {
            mode,
            status,
            coverage,
            facts_revision: None,
            lexical_revision: None,
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

    /// Invalidate the current provider cache and mark runtime as Degraded.
    async fn invalidate_provider(&self, _reason: &str) {}
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
            reason: "no_semantic_runtime".to_string(),
            retryable: false,
        })
    }
}

type ProviderSender = tokio::sync::broadcast::Sender<Option<Arc<dyn EmbeddingProvider>>>;

/// Standard implementation of `SemanticRuntime` backed by lazy acquisition,
/// dynamic recovery, and SQLite vector verification.
pub struct DefaultSemanticRuntime {
    registry_paths: Option<RegistryPaths>,
    provider_cache: Arc<tokio::sync::RwLock<Option<Arc<dyn EmbeddingProvider>>>>,
    state: Arc<tokio::sync::RwLock<RuntimeProviderState>>,
    in_flight_init: Arc<tokio::sync::Mutex<Option<ProviderSender>>>,
}

impl DefaultSemanticRuntime {
    fn with_parts(
        paths: Option<RegistryPaths>,
        provider: Option<Arc<dyn EmbeddingProvider>>,
        state: RuntimeProviderState,
    ) -> Self {
        Self {
            registry_paths: paths,
            provider_cache: Arc::new(tokio::sync::RwLock::new(provider)),
            state: Arc::new(tokio::sync::RwLock::new(state)),
            in_flight_init: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    pub fn new(provider: Option<Arc<dyn EmbeddingProvider>>) -> Self {
        let state = if provider.is_some() {
            RuntimeProviderState::Ready
        } else {
            RuntimeProviderState::Disabled
        };
        Self::with_parts(None, provider, state)
    }

    pub fn from_registry_paths(paths: RegistryPaths) -> Self {
        Self::with_parts(Some(paths), None, RuntimeProviderState::Starting)
    }

    pub fn provider(&self) -> Option<Arc<dyn EmbeddingProvider>> {
        self.provider_cache.try_read().ok().and_then(|g| g.clone())
    }

    pub async fn runtime_state(&self) -> RuntimeProviderState {
        self.state.read().await.clone()
    }

    pub async fn update_provider(&self, provider: Option<Arc<dyn EmbeddingProvider>>) {
        let mut cache = self.provider_cache.write().await;
        let mut st = self.state.write().await;
        *cache = provider.clone();
        *st = match provider {
            Some(_) => RuntimeProviderState::Ready,
            None => RuntimeProviderState::Degraded {
                reason: "PROVIDER_UNAVAILABLE".to_string(),
                retryable: true,
            },
        };
    }

    pub async fn invalidate_provider(&self, reason: &str) {
        let mut cache = self.provider_cache.write().await;
        let mut st = self.state.write().await;
        *cache = None;
        *st = RuntimeProviderState::Degraded {
            reason: reason.to_string(),
            retryable: true,
        };
    }

    async fn get_or_acquire_provider(
        &self,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<Option<Arc<dyn EmbeddingProvider>>, RequestFailure> {
        // 1. Fast path: check cached provider and probe health
        {
            let cache = self.provider_cache.read().await;
            let st = self.state.read().await;
            if let (Some(p), RuntimeProviderState::Ready) = (cache.as_ref(), &*st) {
                let p_clone = Arc::clone(p);
                let remaining = deadline.saturating_duration_since(Instant::now());
                let probe_budget =
                    julie_core::embeddings_contract::EmbeddingRequestBudget::with_timeout(
                        Duration::from_millis(500).min(remaining),
                    );
                if tokio::task::spawn_blocking(move || p_clone.health_check(&probe_budget))
                    .await
                    .is_ok_and(|r| r.is_ok())
                {
                    return Ok(Some(Arc::clone(p)));
                }
                drop(cache);
                drop(st);
                self.invalidate_provider("HEALTH_PROBE_FAILED").await;
            } else if matches!(
                &*st,
                RuntimeProviderState::Degraded {
                    retryable: false,
                    ..
                }
            ) {
                return Ok(None);
            }
        }

        if self.registry_paths.is_none() {
            let cache = self.provider_cache.read().await;
            return Ok(cache.clone());
        }

        // 2. Single-flight shared initialization task
        let mut rx = {
            let mut init_guard = self.in_flight_init.lock().await;
            // Re-check after acquiring init lock
            {
                let cache = self.provider_cache.read().await;
                let st = self.state.read().await;
                if let (Some(p), RuntimeProviderState::Ready) = (cache.as_ref(), &*st) {
                    return Ok(Some(Arc::clone(p)));
                }
                if matches!(
                    &*st,
                    RuntimeProviderState::Degraded {
                        retryable: false,
                        ..
                    }
                ) {
                    return Ok(None);
                }
            }

            if let Some(ref tx) = *init_guard {
                tx.subscribe()
            } else {
                let (tx, rx) = tokio::sync::broadcast::channel(1);
                *init_guard = Some(tx.clone());

                {
                    let mut st = self.state.write().await;
                    *st = RuntimeProviderState::Starting;
                }

                let state_clone = Arc::clone(&self.state);
                let cache_clone = Arc::clone(&self.provider_cache);
                let in_flight_clone = Arc::clone(&self.in_flight_init);

                tokio::spawn(async move {
                    let provider = crate::embeddings::acquire_in_process_embedding_provider().await;
                    {
                        let mut cache = cache_clone.write().await;
                        let mut st = state_clone.write().await;
                        if let Some(ref p) = provider {
                            *cache = Some(Arc::clone(p));
                            *st = RuntimeProviderState::Ready;
                        } else {
                            *cache = None;
                            *st = RuntimeProviderState::Degraded {
                                reason: "PROVIDER_UNAVAILABLE".to_string(),
                                retryable: true,
                            };
                        }
                    }
                    let _ = tx.send(provider);
                    *in_flight_clone.lock().await = None;
                });

                rx
            }
        };

        // 3. Await result bounded by caller deadline and cancellation
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                Err(RequestFailure::cancelled("Semantic initialization cancelled"))
            }
            _ = tokio::time::sleep_until(deadline) => {
                Err(RequestFailure::deadline_exceeded("Semantic initialization deadline exceeded"))
            }
            res = rx.recv() => match res {
                Ok(provider) => Ok(provider),
                Err(_) => Ok(self.provider_cache.read().await.clone()),
            },
        }
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

        // Rule 1 & 2: Off mode or Requirement None performs zero work
        if mode == SemanticMode::Off || requirement.is_none() {
            return Ok(SemanticReadiness::Disabled);
        }

        // Rule 3: Provider readiness check
        let provider = match self.get_or_acquire_provider(deadline, cancellation).await? {
            Some(p) => p,
            None => {
                let st = self.state.read().await;
                let retryable = match &*st {
                    RuntimeProviderState::Degraded { retryable, .. } => *retryable,
                    RuntimeProviderState::Starting => true,
                    _ => false,
                };
                return match mode {
                    SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                        "No embedding provider configured or available",
                        serde_json::json!({ "coverage": "missing", "reason": "provider_unavailable" }),
                    )),
                    SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                        reason: "PROVIDER_UNAVAILABLE".to_string(),
                        retryable,
                    }),
                    SemanticMode::Off => Ok(SemanticReadiness::Disabled),
                };
            }
        };

        // If only Query is required, provider readiness is sufficient
        if requirement == SemanticRequirement::Query {
            let dev_info = provider.device_info();
            let encoder_identity = provider.encoder_identity().ok();
            return Ok(SemanticReadiness::Ready {
                model_id: dev_info.model_name,
                dimensions: provider.dimensions(),
                device: dev_info.device,
                encoder_identity,
                vector_generation: None,
                eligible_symbols: 0,
                embedded_symbols: 0,
            });
        }

        // Rule 4: Symbols or QueryAndSymbols requires vectors the provider can query
        let facts_path = binding
            .index_root
            .join(julie_index::checkout_store::FACTS_FILE);
        crate::request_engine::semantic_store::check_facts_vectors(
            &facts_path,
            provider.as_ref(),
            mode,
        )
    }

    fn provider(&self) -> Option<Arc<dyn EmbeddingProvider>> {
        self.provider_cache.try_read().ok().and_then(|g| g.clone())
    }

    async fn invalidate_provider(&self, reason: &str) {
        self.invalidate_provider(reason).await;
    }
}
