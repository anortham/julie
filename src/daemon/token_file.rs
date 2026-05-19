//! Token file IO primitives for the daemon bearer token.
//!
//! The daemon writes a short-lived bearer token to `~/.julie/daemon.token` at
//! startup; the adapter reads it to authenticate HTTP requests to the daemon's
//! localhost endpoint.  The file is sensitive and must be readable only by the
//! owning user.
//!
//! # Security posture
//! * **POSIX**: mode `0600` is set both at `open()` time (via
//!   [`OpenOptionsExt::mode`]) and enforced with a post-write
//!   [`set_permissions`] call to defend against umask stripping.
//! * **Windows**: the file DACL is replaced with a protected ACL granting
//!   access only to the current user SID.

use std::io::Write as _;
use std::path::Path;

use anyhow::Context as _;

/// Write `token` to `path`, creating or truncating the file.
///
/// On POSIX the file is created with mode `0600` and permissions are
/// explicitly re-applied after the write to resist umask stripping.
///
/// On Windows the file DACL is restricted to the current user after writing.
///
/// Parent directories are created automatically if absent.
pub fn write_token(path: &Path, token: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent dirs for {}", path.display()))?;
    }

    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // Set mode at open time; this is the primary gate.
        options.mode(0o600);
    }

    let mut file = options
        .open(path)
        .with_context(|| format!("open token file {}", path.display()))?;

    // Post-write explicit permission set: defends against platforms / filesystems
    // where the open-time mode hint is silently ignored (e.g. some FUSE mounts,
    // exotic umask configurations).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("set_permissions 0600 on {}", path.display()))?;
    }

    writeln!(file, "{token}").with_context(|| format!("write token to {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("sync token file {}", path.display()))?;

    #[cfg(windows)]
    restrict_current_user_acl(path)
        .with_context(|| format!("restrict token file ACL on {}", path.display()))?;

    Ok(())
}

/// Read and return the token stored at `path`.
///
/// Trailing whitespace (including the newline written by [`write_token`]) is
/// stripped before returning.
pub fn read_token(path: &Path) -> anyhow::Result<String> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read token file {}", path.display()))?;
    Ok(raw.trim_end().to_owned())
}

#[cfg(windows)]
fn restrict_current_user_acl(path: &Path) -> anyhow::Result<()> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt as _;
    use std::ptr::{NonNull, null_mut};

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, SET_ACCESS, SetEntriesInAclW,
        SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetTokenInformation, NO_INHERITANCE,
        PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    struct LocalAllocPtr(NonNull<c_void>);
    impl Drop for LocalAllocPtr {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0.as_ptr());
            }
        }
    }

    let mut token_handle: HANDLE = std::ptr::null_mut();
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle) };
    if opened == 0 {
        return Err(std::io::Error::last_os_error()).context("OpenProcessToken");
    }
    let token_handle = Handle(token_handle);

    let mut token_len = 0u32;
    unsafe {
        GetTokenInformation(token_handle.0, TokenUser, null_mut(), 0, &mut token_len);
    }
    if token_len == 0 {
        return Err(std::io::Error::last_os_error()).context("GetTokenInformation size");
    }

    let mut token_buf = vec![0u8; token_len as usize];
    let got_token = unsafe {
        GetTokenInformation(
            token_handle.0,
            TokenUser,
            token_buf.as_mut_ptr().cast(),
            token_len,
            &mut token_len,
        )
    };
    if got_token == 0 {
        return Err(std::io::Error::last_os_error()).context("GetTokenInformation token user");
    }

    let token_user = unsafe { &*(token_buf.as_ptr() as *const TOKEN_USER) };
    let sid = token_user.User.Sid;
    if sid.is_null() {
        anyhow::bail!("current user token has null SID");
    }

    let mut explicit_access = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_ALL_ACCESS,
        grfAccessMode: SET_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: sid.cast(),
        },
    };

    let mut acl = null_mut();
    let set_acl = unsafe { SetEntriesInAclW(1, &mut explicit_access, null_mut(), &mut acl) };
    if set_acl != 0 {
        return Err(std::io::Error::from_raw_os_error(set_acl as i32)).context("SetEntriesInAclW");
    }
    let acl =
        LocalAllocPtr(NonNull::new(acl.cast()).context("SetEntriesInAclW returned null ACL")?);

    let mut wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let set_security = unsafe {
        SetNamedSecurityInfoW(
            wide_path.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            acl.0.as_ptr().cast(),
            null_mut(),
        )
    };
    if set_security != 0 {
        return Err(std::io::Error::from_raw_os_error(set_security as i32))
            .context("SetNamedSecurityInfoW");
    }

    Ok(())
}
