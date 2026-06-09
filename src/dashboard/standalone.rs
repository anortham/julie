use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Instant;

use anyhow::Result;
use axum::Router;
use tokio::net::TcpListener;

use crate::dashboard::{DashboardConfig, create_router};
use crate::paths::RegistryPaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashboardLaunchOptions {
    pub open_browser: bool,
}

impl Default for DashboardLaunchOptions {
    fn default() -> Self {
        Self { open_browser: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardLaunch {
    pub url: String,
    pub local_addr: SocketAddr,
    pub browser_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DashboardServer {
    url: String,
    local_addr: SocketAddr,
}

static DASHBOARD_SERVER: OnceLock<Mutex<Option<DashboardServer>>> = OnceLock::new();

pub async fn launch_dashboard(options: DashboardLaunchOptions) -> Result<DashboardLaunch> {
    let server = ensure_background_server().await?;
    let browser_error = if options.open_browser {
        opener::open(&server.url)
            .err()
            .map(|error| error.to_string())
    } else {
        None
    };

    Ok(DashboardLaunch {
        url: server.url,
        local_addr: server.local_addr,
        browser_error,
    })
}

#[cfg(test)]
pub(crate) async fn launch_dashboard_for_paths(
    paths: RegistryPaths,
    options: DashboardLaunchOptions,
) -> Result<DashboardLaunch> {
    let server = spawn_background_server(paths).await?;
    let browser_error = if options.open_browser {
        opener::open(&server.url)
            .err()
            .map(|error| error.to_string())
    } else {
        None
    };

    Ok(DashboardLaunch {
        url: server.url,
        local_addr: server.local_addr,
        browser_error,
    })
}

pub async fn serve_dashboard_forever() -> Result<()> {
    let (listener, app, server) = build_dashboard_server(RegistryPaths::try_new()?).await?;

    println!("Dashboard URL: {}", server.url);
    if let Err(error) = opener::open(&server.url) {
        eprintln!("Failed to open browser: {error}");
    }

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

async fn ensure_background_server() -> Result<DashboardServer> {
    if let Some(server) = cached_server() {
        return Ok(server);
    }

    let (listener, app, server) = build_dashboard_server(RegistryPaths::try_new()?).await?;

    {
        let cache = DASHBOARD_SERVER.get_or_init(|| Mutex::new(None));
        let mut guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = guard.clone() {
            return Ok(existing);
        }
        *guard = Some(server.clone());
    }

    let url = server.url.clone();
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app.into_make_service()).await {
            tracing::warn!("Standalone dashboard server at {url} stopped with error: {error}");
        }
    });

    Ok(server)
}

fn cached_server() -> Option<DashboardServer> {
    DASHBOARD_SERVER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

#[cfg(test)]
async fn spawn_background_server(paths: RegistryPaths) -> Result<DashboardServer> {
    let (listener, app, server) = build_dashboard_server(paths).await?;

    let url = server.url.clone();
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app.into_make_service()).await {
            tracing::warn!("Standalone dashboard server at {url} stopped with error: {error}");
        }
    });

    Ok(server)
}

async fn build_dashboard_server(
    paths: RegistryPaths,
) -> Result<(TcpListener, Router, DashboardServer)> {
    paths.ensure_dirs()?;
    let registry = Arc::new(crate::registry::database::DaemonDatabase::open(
        &paths.registry_db(),
    )?);
    let recovery_markers = Arc::new(crate::registry::shutdown::read_recovery_markers(&paths));
    let state = crate::dashboard::state::DashboardState::new(
        Arc::new(crate::registry::session::SessionTracker::new()),
        Some(registry),
        Arc::new(RwLock::new(
            crate::registry::lifecycle::LifecyclePhase::Ready,
        )),
        Instant::now(),
        None,
        50,
    )
    .with_recovery_markers(recovery_markers);
    let app = create_router(state, DashboardConfig::default())?;
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let local_addr = listener.local_addr()?;
    let url = format!("http://{local_addr}");

    Ok((listener, app, DashboardServer { url, local_addr }))
}
