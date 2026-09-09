use crate::service::discovery::{self, ServiceRecord};
use julie_core::paths::RegistryPaths;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum ConnectError {
    VersionMismatch { service: String, client: String },
    Unavailable(String),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::VersionMismatch { service, client } => write!(
                f,
                "julie: service version {service} does not match client version {client}; run: julie-server service restart"
            ),
            ConnectError::Unavailable(why) => write!(f, "julie: service unavailable: {why}"),
        }
    }
}

impl std::error::Error for ConnectError {}

pub struct ServiceClient {
    pub base: String,
    pub token: String,
    pub version: String,
    http: reqwest::Client,
}

impl ServiceClient {
    fn from_record(record: &ServiceRecord) -> Self {
        Self {
            base: format!("http://127.0.0.1:{}", record.port),
            token: record.token.clone(),
            version: record.version.clone(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(130))
                .build()
                .expect("reqwest client"),
        }
    }

    pub async fn status(&self) -> reqwest::Result<serde_json::Value> {
        self.http.get(format!("{}/status", self.base)).bearer_auth(&self.token).send().await?.error_for_status()?.json().await
    }

    pub async fn post_mcp(&self, body: &[u8], method: &str) -> reqwest::Result<reqwest::Response> {
        self.http.post(format!("{}/mcp", self.base))
            .bearer_auth(&self.token)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .header("Mcp-Method", method)
            .body(body.to_vec())
            .send().await
    }

    pub async fn post_shutdown(&self) -> reqwest::Result<reqwest::Response> {
        self.http.post(format!("{}/shutdown", self.base)).bearer_auth(&self.token).send().await
    }
}

async fn try_connect(paths: &RegistryPaths) -> Result<Option<ServiceClient>, ConnectError> {
    let Some(record) =
        discovery::read_record(paths).map_err(|e| ConnectError::Unavailable(e.to_string()))?
    else {
        return Ok(None);
    };
    let client = ServiceClient::from_record(&record);
    match client.status().await {
        Ok(_) => {
            if record.version != env!("CARGO_PKG_VERSION") {
                return Err(ConnectError::VersionMismatch {
                    service: record.version,
                    client: env!("CARGO_PKG_VERSION").to_string(),
                });
            }
            Ok(Some(client))
        }
        Err(_) => {
            discovery::remove_record(paths)
                .map_err(|e| ConnectError::Unavailable(e.to_string()))?;
            Ok(None)
        }
    }
}

pub async fn connect_or_start(
    paths: &RegistryPaths,
    spawn: impl Fn() -> std::io::Result<()>,
) -> Result<ServiceClient, ConnectError> {
    if let Some(client) = try_connect(paths).await? {
        return Ok(client);
    }
    spawn().map_err(|e| ConnectError::Unavailable(format!("could not start service: {e}")))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(client) = try_connect(paths).await? {
            return Ok(client);
        }
    }
    Err(ConnectError::Unavailable(
        "service did not start within 10 s".into(),
    ))
}

pub fn spawn_detached_service() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut command = std::process::Command::new(exe);
    command
        .arg("service")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    command.spawn().map(|_| ())
}
