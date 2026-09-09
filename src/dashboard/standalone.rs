use anyhow::Result;
use julie_core::paths::RegistryPaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DashboardLaunchOptions {
    pub open_browser: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardLaunch {
    pub url: String,
    pub local_addr: std::net::SocketAddr,
    pub browser_error: Option<String>,
}

pub async fn open_dashboard() -> Result<()> {
    let paths = RegistryPaths::try_new()?;
    let client = crate::service::client::connect_or_start(
        &paths,
        crate::service::client::spawn_detached_service,
    )
    .await?;
    let url = format!("{}/?token={}", client.base, client.token);
    println!("{url}");
    Ok(())
}

pub async fn launch_dashboard(_options: DashboardLaunchOptions) -> Result<DashboardLaunch> {
    let paths = RegistryPaths::try_new()?;
    let client = crate::service::client::connect_or_start(
        &paths,
        crate::service::client::spawn_detached_service,
    )
    .await?;
    let url = format!("{}/?token={}", client.base, client.token);
    Ok(DashboardLaunch {
        url,
        local_addr: "127.0.0.1:0".parse().unwrap(),
        browser_error: None,
    })
}
