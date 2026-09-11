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
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
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

/// Reports whether a process with this id is currently running.
pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        let state = stat
            .rsplit_once(')')
            .and_then(|(_, after_comm)| after_comm.split_whitespace().next())
            .unwrap_or("R");
        state != "Z"
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            let mut code: u32 = 0;
            let queried = GetExitCodeProcess(handle, &mut code) != 0;
            CloseHandle(handle);
            queried && code == 259
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}
