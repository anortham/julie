//! Sidecar binary discovery, SHA-256 verification, path derivation, and process launching.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

pub const DEFAULT_NATIVE_MODEL: &str = "bge-small-en-v1.5-f32";

/// Canonical filesystem paths for broker endpoints and lock coordination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrokerPaths {
    pub endpoint_str: String,
    #[cfg(unix)]
    pub endpoint_path: PathBuf,
    pub service_lock: PathBuf,
    pub accelerator_lock: PathBuf,
}

/// Derives deterministic IPC endpoint and advisory lock paths.
///
/// Truncates the SHA-256 discriminator to 16 characters to prevent exceeding
/// the Unix `sockaddr_un` limit (104 bytes on macOS, 108 bytes on Linux).
pub fn derive_broker_paths(
    cache_root: &Path,
    executable_sha256: &str,
    model_id: &str,
) -> Result<BrokerPaths> {
    let mut hasher = Sha256::new();
    #[cfg(windows)]
    {
        let normalized_cache = cache_root
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase();
        hasher.update(normalized_cache.as_bytes());
    }
    #[cfg(not(windows))]
    {
        hasher.update(cache_root.to_string_lossy().as_bytes());
    }
    hasher.update(b":");
    hasher.update(executable_sha256.as_bytes());
    hasher.update(b":");
    hasher.update(model_id.as_bytes());
    let discriminator = &hex::encode(hasher.finalize())[..16];

    let service_lock = cache_root.join(format!("b-{model_id}-{discriminator}.lock"));
    let accelerator_lock = cache_root.join("accelerator.lock");

    #[cfg(unix)]
    {
        let endpoint_path = cache_root.join(format!("b-{model_id}-{discriminator}.sock"));
        let endpoint_str = endpoint_path.to_string_lossy().to_string();
        if endpoint_str.len() >= 104 {
            bail!(
                "derived Unix domain socket path exceeds 104-byte limit: '{}' (len {})",
                endpoint_str,
                endpoint_str.len()
            );
        }
        Ok(BrokerPaths {
            endpoint_str,
            endpoint_path,
            service_lock,
            accelerator_lock,
        })
    }

    #[cfg(windows)]
    {
        let endpoint_str = format!(r"\\.\pipe\julie-semantic-broker-{model_id}-{discriminator}");
        Ok(BrokerPaths {
            endpoint_str,
            service_lock,
            accelerator_lock,
        })
    }
}

/// Locates the native sidecar binary and computes its SHA-256 checksum.
pub fn find_and_hash_sidecar_binary(explicit: Option<&Path>) -> Result<(PathBuf, String)> {
    let candidate = if let Some(path) = explicit {
        path.to_path_buf()
    } else if let Ok(env_path) = std::env::var("JULIE_NATIVE_SIDECAR_PROGRAM") {
        PathBuf::from(env_path)
    } else {
        find_default_sidecar_binary()?
    };

    if !candidate.is_file() {
        bail!(
            "NATIVE_SIDECAR_MISSING: sidecar executable not found at '{}'",
            candidate.display()
        );
    }

    let mut file = File::open(&candidate).map_err(|e| {
        anyhow::anyhow!(
            "NATIVE_SIDECAR_MISSING: failed to open sidecar binary '{}': {e}",
            candidate.display()
        )
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let sha256 = hex::encode(hasher.finalize());

    Ok((candidate, sha256))
}

fn find_default_sidecar_binary() -> Result<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join("julie-semantic-sidecar"));
            #[cfg(windows)]
            candidates.push(parent.join("julie-semantic-sidecar.exe"));
        }
    }
    candidates.push(PathBuf::from(
        "/home/murphy/source/julie-semantic-sidecar/target/release/julie-semantic-sidecar",
    ));
    candidates.push(PathBuf::from(
        "/home/murphy/source/julie-semantic-sidecar/target/debug/julie-semantic-sidecar",
    ));
    if let Some(cache) = dirs::cache_dir() {
        candidates.push(cache.join("julie-semantic/bin/julie-semantic-sidecar"));
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            candidates.push(dir.join("julie-semantic-sidecar"));
            #[cfg(windows)]
            candidates.push(dir.join("julie-semantic-sidecar.exe"));
        }
    }
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| {
        anyhow::anyhow!(
            "NATIVE_SIDECAR_MISSING: unable to locate julie-semantic-sidecar binary in PATH, cache, or target"
        )
    })
}

/// Executes `julie-semantic-sidecar prepare --model <model_id>` to ensure weights exist in cache.
pub fn run_prepare(executable: &Path, cache_root: &Path, model_id: &str) -> Result<()> {
    let mut cmd = Command::new(executable);
    cmd.arg("prepare")
        .arg("--model")
        .arg(model_id)
        .env("JULIE_EMBEDDING_CACHE_DIR", cache_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| {
        anyhow::anyhow!("NATIVE_SIDECAR_MISSING: failed to execute sidecar prepare: {e}")
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "MODEL_NOT_PREPARED: sidecar prepare failed with exit code {:?}: {stderr}",
            output.status.code()
        );
    }

    Ok(())
}

/// Spawns a background sidecar broker process.
///
/// Retains child stdin open (`Stdio::piped()`) so the broker's `OwnerWatchdog`
/// does not terminate prematurely.
pub fn spawn_broker(
    executable: &Path,
    cache_root: &Path,
    model_id: &str,
    paths: &BrokerPaths,
) -> Result<Child> {
    if let Some(parent) = paths.service_lock.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }

    let mut cmd = Command::new(executable);
    cmd.arg("broker")
        .arg("--model")
        .arg(model_id)
        .arg("--endpoint")
        .arg(&paths.endpoint_str)
        .arg("--lock")
        .arg(&paths.service_lock)
        .arg("--accelerator-lock")
        .arg(&paths.accelerator_lock)
        .env("JULIE_EMBEDDING_CACHE_DIR", cache_root)
        .stdin(Stdio::piped()) // REQUIRED for OwnerWatchdog
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let child = cmd.spawn().map_err(|e| {
        anyhow::anyhow!("NATIVE_SIDECAR_MISSING: failed to spawn sidecar broker: {e}")
    })?;

    Ok(child)
}

/// Binds and verifies executable provenance against the launched child process.
///
/// Detects binary replacement races between initial hash calculation and spawn,
/// inspecting `/proc/{pid}/exe` on Linux where available and re-hashing immediately after launch.
pub fn verify_launched_child_sha(
    executable_path: &Path,
    child_pid: u32,
    expected_sha: &str,
) -> Result<String> {
    let mut header = [0u8; 2];
    if let Ok(mut f) = File::open(executable_path) {
        if f.read_exact(&mut header).is_ok() && &header == b"#!" {
            bail!(
                "NATIVE_SIDECAR_SCRIPTS_UNSUPPORTED: native sidecar broker must be a compiled binary executable, not a script"
            );
        }
    }

    #[cfg(target_os = "linux")]
    {
        let proc_exe = PathBuf::from(format!("/proc/{child_pid}/exe"));
        if !proc_exe.exists() {
            bail!(
                "LAUNCHED_EXECUTABLE_UNAVAILABLE: process {child_pid} executable evidence at /proc/{child_pid}/exe is unavailable"
            );
        }

        let (_, proc_sha) = find_and_hash_sidecar_binary(Some(&proc_exe)).map_err(|e| {
            anyhow::anyhow!(
                "LAUNCHED_EXECUTABLE_HASH_FAILED: failed to hash running process {child_pid} executable image: {e}"
            )
        })?;

        if proc_sha != expected_sha {
            bail!(
                "LAUNCHED_EXECUTABLE_SHA_MISMATCH: running process {child_pid} executable hash ({proc_sha}) does not match expected hash ({expected_sha})"
            );
        }

        Ok(proc_sha)
    }

    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn proc_pidpath(
                pid: libc::c_int,
                buffer: *mut libc::c_void,
                buffersize: u32,
            ) -> libc::c_int;
            fn proc_pidinfo(
                pid: libc::c_int,
                flavor: libc::c_int,
                arg: u64,
                buffer: *mut libc::c_void,
                buffersize: libc::c_int,
            ) -> libc::c_int;
        }
        let mut buf = vec![0u8; 4096];
        let ret = unsafe {
            proc_pidpath(
                child_pid as libc::c_int,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len() as u32,
            )
        };
        if ret <= 0 {
            bail!(
                "LAUNCHED_EXECUTABLE_UNAVAILABLE: process {child_pid} executable path unavailable on macOS"
            );
        }
        let nul_pos = buf.iter().position(|&b| b == 0).unwrap_or(ret as usize);
        let path_str = std::str::from_utf8(&buf[..nul_pos]).map_err(|e| {
            anyhow::anyhow!("LAUNCHED_EXECUTABLE_UNAVAILABLE: invalid utf8 in proc_pidpath: {e}")
        })?;
        let proc_exe = PathBuf::from(path_str);
        if !proc_exe.exists() {
            bail!(
                "LAUNCHED_EXECUTABLE_UNAVAILABLE: process {child_pid} executable image at {proc_exe:?} does not exist"
            );
        }

        let mut bsd_info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let expected_size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let info_ret = unsafe {
            proc_pidinfo(
                child_pid as libc::c_int,
                libc::PROC_PIDTBSDINFO,
                0,
                &mut bsd_info as *mut _ as *mut libc::c_void,
                expected_size,
            )
        };
        if info_ret < expected_size {
            bail!("LAUNCHED_EXECUTABLE_UNAVAILABLE: failed to get bsdinfo for process {child_pid}");
        }

        use std::os::unix::fs::MetadataExt;
        let mut proc_file = File::open(&proc_exe)?;
        let proc_meta = proc_file.metadata()?;

        // Subsecond comparison: file ctime/mtime vs kernel process start time.
        // Unchanged binaries launched within the same second have file timestamps <= process start,
        // while binaries replaced or touched after process launch will have timestamps > process start.
        let file_ctime_usec = (proc_meta.ctime().max(0) as u64)
            .saturating_mul(1_000_000)
            .saturating_add((proc_meta.ctime_nsec().max(0) / 1_000) as u64);
        let file_mtime_usec = (proc_meta.mtime().max(0) as u64)
            .saturating_mul(1_000_000)
            .saturating_add((proc_meta.mtime_nsec().max(0) / 1_000) as u64);
        let proc_start_usec = bsd_info
            .pbi_start_tvsec
            .saturating_mul(1_000_000)
            .saturating_add(bsd_info.pbi_start_tvusec);

        if file_ctime_usec > proc_start_usec || file_mtime_usec > proc_start_usec {
            bail!(
                "LAUNCHED_EXECUTABLE_REPLACED_AFTER_START: executable at {proc_exe:?} was modified or replaced after process {child_pid} started"
            );
        }

        // Hash the opened file descriptor directly to avoid TOCTOU pathname replacement
        let mut hasher = Sha256::new();
        let mut reader = std::io::BufReader::new(&mut proc_file);
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let proc_sha = format!("{:x}", hasher.finalize());

        if proc_sha != expected_sha {
            bail!(
                "LAUNCHED_EXECUTABLE_SHA_MISMATCH: running process {child_pid} executable hash ({proc_sha}) does not match expected hash ({expected_sha})"
            );
        }

        Ok(proc_sha)
    }

    #[cfg(target_os = "windows")]
    {
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        unsafe extern "system" {
            fn OpenProcess(
                dwDesiredAccess: u32,
                bInheritHandle: i32,
                dwProcessId: u32,
            ) -> *mut std::ffi::c_void;
            fn QueryFullProcessImageNameW(
                hProcess: *mut std::ffi::c_void,
                dwFlags: u32,
                lpExeName: *mut u16,
                lpdwSize: *mut u32,
            ) -> i32;
            fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
        }

        let h_process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, child_pid) };
        if h_process.is_null() {
            bail!(
                "LAUNCHED_EXECUTABLE_UNAVAILABLE: failed to open process {child_pid} for executable query"
            );
        }

        let mut buf = vec![0u16; 1024];
        let mut size = buf.len() as u32;
        let res = unsafe { QueryFullProcessImageNameW(h_process, 0, buf.as_mut_ptr(), &mut size) };
        unsafe { CloseHandle(h_process) };

        if res == 0 || size == 0 {
            bail!(
                "LAUNCHED_EXECUTABLE_UNAVAILABLE: QueryFullProcessImageNameW failed for process {child_pid}"
            );
        }

        use std::os::windows::ffi::OsStringExt;
        let os_str = std::ffi::OsString::from_wide(&buf[..size as usize]);
        let proc_exe = PathBuf::from(os_str);
        if !proc_exe.exists() {
            bail!(
                "LAUNCHED_EXECUTABLE_UNAVAILABLE: process {child_pid} executable image at {proc_exe:?} does not exist"
            );
        }

        let (_, proc_sha) = find_and_hash_sidecar_binary(Some(&proc_exe)).map_err(|e| {
            anyhow::anyhow!(
                "LAUNCHED_EXECUTABLE_HASH_FAILED: failed to hash running process {child_pid} executable image: {e}"
            )
        })?;

        if proc_sha != expected_sha {
            bail!(
                "LAUNCHED_EXECUTABLE_SHA_MISMATCH: running process {child_pid} executable hash ({proc_sha}) does not match expected hash ({expected_sha})"
            );
        }

        Ok(proc_sha)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        bail!(
            "UNSUPPORTED_PLATFORM: process-bound executable verification is not supported on this platform"
        );
    }
}

/// Resolved configuration for launching and attaching to a native sidecar broker.
#[derive(Clone, Debug)]
pub struct NativeLaunchConfig {
    pub executable_path: PathBuf,
    pub executable_sha256: String,
    pub model_id: String,
    pub cache_root: PathBuf,
    pub broker_paths: BrokerPaths,
}

impl NativeLaunchConfig {
    pub fn try_new(
        explicit_program: Option<&Path>,
        model_id: Option<&str>,
        custom_cache: Option<&Path>,
    ) -> Result<Self> {
        let (executable_path, executable_sha256) = find_and_hash_sidecar_binary(explicit_program)?;
        let model = model_id.unwrap_or(DEFAULT_NATIVE_MODEL).to_string();

        let cache_root = if let Some(custom) = custom_cache {
            custom.to_path_buf()
        } else if let Ok(env_cache) = std::env::var("JULIE_EMBEDDING_CACHE_DIR") {
            PathBuf::from(env_cache)
        } else {
            dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from(".cache"))
                .join("julie-semantic")
        };

        let broker_paths = derive_broker_paths(&cache_root, &executable_sha256, &model)?;

        Ok(Self {
            executable_path,
            executable_sha256,
            model_id: model,
            cache_root,
            broker_paths,
        })
    }
}
