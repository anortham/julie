pub mod discovery;
pub mod http;
pub mod status;

use crate::request_engine::{BindingResolver, RequestEngine, RuntimeFactory};
use anyhow::Context;
use julie_core::paths::RegistryPaths;
use std::sync::Arc;
use std::time::Duration;

pub struct ServiceConfig {
    pub idle: Option<Duration>,
    pub registry_paths: RegistryPaths,
}

impl ServiceConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let registry_paths = RegistryPaths::try_new().context("resolve Julie home")?;
        let idle = match std::env::var("JULIE_SERVICE_IDLE_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
        {
            Some(0) => None,
            Some(secs) => Some(Duration::from_secs(secs)),
            None => Some(Duration::from_secs(1800)),
        };
        Ok(Self {
            idle,
            registry_paths,
        })
    }
}

pub struct ServiceApp {
    config: ServiceConfig,
    state: http::AppState,
}

impl ServiceApp {
    pub fn new(config: ServiceConfig) -> anyhow::Result<Self> {
        let paths = config.registry_paths.clone();
        let resolver = BindingResolver::new(None, false, paths.clone());
        let runtimes = Arc::new(RuntimeFactory::new(paths));
        let engine = Arc::new(RequestEngine::new(resolver, runtimes));
        let state = http::AppState {
            engine,
            status: Arc::new(status::StatusLog::new()),
            token: Arc::from(discovery::new_token()),
            request_timeout: Duration::from_secs(120),
        };
        Ok(Self { config, state })
    }

    pub fn state(&self) -> &http::AppState {
        &self.state
    }

    pub async fn serve(self, listener: tokio::net::TcpListener) -> anyhow::Result<()> {
        let port = listener.local_addr()?.port();
        let record = discovery::ServiceRecord {
            port,
            token: self.state.token.to_string(),
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: status::now_rfc3339(),
        };
        discovery::write_record(&self.config.registry_paths, &record)?;

        let shutdown = tokio_util::sync::CancellationToken::new();
        let idle_watch = {
            let status = Arc::clone(&self.state.status);
            let shutdown = shutdown.clone();
            let idle = self.config.idle;
            async move {
                let Some(idle) = idle else {
                    std::future::pending::<()>().await;
                    return;
                };
                loop {
                    tokio::time::sleep(idle.min(Duration::from_millis(250))).await;
                    if status.idle_for().is_some_and(|d| d >= idle) {
                        shutdown.cancel();
                        return;
                    }
                }
            }
        };
        let router = http::router(self.state.clone());
        let server = axum::serve(listener, router).with_graceful_shutdown({
            let shutdown = shutdown.clone();
            async move { shutdown.cancelled().await }
        });
        let result = tokio::select! {
            r = server => r.map_err(anyhow::Error::from),
            _ = idle_watch => Ok(()),
        };
        discovery::remove_record(&self.config.registry_paths)?;
        result
    }
}

pub async fn run_service(config: ServiceConfig) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind 127.0.0.1:0")?;
    ServiceApp::new(config)?.serve(listener).await
}
