pub mod client;
pub mod discovery;
pub mod http;
pub mod mcp;
pub mod shim;
pub mod status;

use crate::registry::database::DaemonDatabase;
use crate::request_engine::{BindingResolver, RequestEngine, RuntimeFactory};
use crate::tools::workspace::commands::registry::cleanup::{
    CleanupSweepSummary, WorkspaceCleanupActivity, run_cleanup_sweep,
};
use crate::tools::workspace::commands::registry::registry_store_for;
use anyhow::Context;
use julie_core::paths::RegistryPaths;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

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
    router: axum::Router,
}

impl ServiceApp {
    pub fn new(config: ServiceConfig) -> anyhow::Result<Self> {
        let paths = config.registry_paths.clone();
        crate::logging::install_file_tracing(
            &paths.logs_dir(),
            crate::registry::project_log::SERVICE_LOG_PREFIX,
            "info",
        )?;
        let resolver = BindingResolver::new(None, false, paths.clone());
        let runtimes = Arc::new(RuntimeFactory::new(paths.clone()));
        let engine = Arc::new(RequestEngine::new(resolver, runtimes));
        let shutdown = tokio_util::sync::CancellationToken::new();
        let state = http::AppState {
            engine,
            status: Arc::new(status::StatusLog::new()),
            token: Arc::from(discovery::new_token()),
            request_timeout: Duration::from_secs(120),
            shutdown,
        };
        let dashboard = crate::dashboard::dashboard_router(&paths)?;
        let router = http::router(state.clone(), dashboard);
        Ok(Self {
            config,
            state,
            router,
        })
    }

    /// Returns a reference to the HTTP application state.
    pub fn state(&self) -> &http::AppState {
        &self.state
    }

    /// Returns a reference to the service request engine.
    pub fn engine(&self) -> &Arc<RequestEngine> {
        &self.state.engine
    }

    pub async fn serve(self, listener: tokio::net::TcpListener) -> anyhow::Result<()> {
        let port = listener.local_addr()?.port();
        self.state.engine.set_service_url(format!(
            "http://127.0.0.1:{port}/?token={}",
            self.state.token
        ));
        let record = discovery::ServiceRecord {
            port,
            token: self.state.token.to_string(),
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: status::now_rfc3339(),
        };
        discovery::write_record(&self.config.registry_paths, &record)?;

        tokio::spawn(sweep_registry(self.config.registry_paths.clone()));

        let shutdown = self.state.shutdown.clone();
        let maintenance = {
            let runtimes = Arc::clone(&self.state.engine.runtimes);
            let shutdown = shutdown.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = shutdown.cancelled() => return,
                        _ = tokio::time::sleep(Duration::from_secs(1)) => {
                            runtimes.retire_idle_runtimes(Instant::now()).await;
                        }
                    }
                }
            })
        };
        let graceful_shutdown = {
            let status = Arc::clone(&self.state.status);
            let shutdown = shutdown.clone();
            let idle = self.config.idle;
            async move {
                let Some(idle) = idle else {
                    shutdown.cancelled().await;
                    return;
                };
                loop {
                    tokio::select! {
                        _ = shutdown.cancelled() => return,
                        _ = tokio::time::sleep(idle.min(Duration::from_millis(250))) => {
                            if status.idle_for().is_some_and(|d| d >= idle) {
                                shutdown.cancel();
                                return;
                            }
                        }
                    }
                }
            }
        };
        let result = axum::serve(listener, self.router)
            .with_graceful_shutdown(graceful_shutdown)
            .await
            .map_err(anyhow::Error::from);
        shutdown.cancel();
        if let Err(error) = maintenance.await {
            warn!("Runtime maintenance task failed to join: {error}");
        }
        self.state.engine.runtimes.shutdown_all().await;
        let owned = discovery::read_record(&self.config.registry_paths)?
            .is_some_and(|r| r.pid == std::process::id());
        if owned {
            discovery::remove_record(&self.config.registry_paths)?;
        }
        result
    }
}

async fn sweep_registry(paths: RegistryPaths) {
    match cleanup_sweep(&paths).await {
        Ok(summary) => info!(
            pruned_workspaces = summary.pruned_workspaces.len(),
            pruned_orphan_dirs = summary.pruned_orphan_dirs.len(),
            blocked_workspaces = summary.blocked_workspaces.len(),
            "Cleanup sweep finished at service start"
        ),
        Err(error) => warn!("Cleanup sweep at service start failed: {error}"),
    }
}

async fn cleanup_sweep(paths: &RegistryPaths) -> anyhow::Result<CleanupSweepSummary> {
    let daemon_db = Arc::new(
        DaemonDatabase::open(&paths.registry_db()).context("open registry database for sweep")?,
    );
    let registry_store = registry_store_for(&daemon_db)?;
    run_cleanup_sweep(
        &registry_store,
        &WorkspaceCleanupActivity::new(HashSet::new()),
    )
    .await
}

pub(crate) fn acquire_service_lock(paths: &RegistryPaths) -> anyhow::Result<std::fs::File> {
    std::fs::create_dir_all(paths.julie_home())?;
    let path = paths.julie_home().join("service.lock");
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    file.try_lock()
        .with_context(|| format!("could not acquire service lock {}", path.display()))?;
    Ok(file)
}

pub async fn run_service(config: ServiceConfig) -> anyhow::Result<()> {
    let _service_lock = acquire_service_lock(&config.registry_paths)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind 127.0.0.1:0")?;
    ServiceApp::new(config)?.serve(listener).await
}
