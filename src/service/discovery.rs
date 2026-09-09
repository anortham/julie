use julie_core::paths::RegistryPaths;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ServiceRecord {
    pub port: u16,
    pub token: String,
    pub pid: u32,
    pub version: String,
    pub started_at: String,
}

pub fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

pub fn write_record(paths: &RegistryPaths, record: &ServiceRecord) -> std::io::Result<()> {
    let target = paths.service_json();
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = target.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    let content = serde_json::to_string_pretty(record).map_err(std::io::Error::other)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temp, &target)
}

pub fn read_record(paths: &RegistryPaths) -> std::io::Result<Option<ServiceRecord>> {
    read_record_at(&paths.service_json())
}

fn read_record_at(path: &Path) -> std::io::Result<Option<ServiceRecord>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn remove_record(paths: &RegistryPaths) -> std::io::Result<()> {
    match std::fs::remove_file(paths.service_json()) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}
