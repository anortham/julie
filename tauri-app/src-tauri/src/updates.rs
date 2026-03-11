//! GitHub release checking for update notifications.
//!
//! Periodically polls the GitHub releases API to detect when a newer version
//! is available. Uses check-and-notify only — no auto-download or replacement.
//!
//! Rate limiting: GitHub allows 60 unauthenticated requests per hour.
//! We check at most once per 6 hours, staying well within limits.

use std::time::Duration;

use serde::Deserialize;
use tauri::AppHandle;

/// How often to check for updates.
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 3600); // 6 hours

/// Delay before first check (let the app settle).
const INITIAL_DELAY: Duration = Duration::from_secs(60);

/// GitHub owner/repo for Julie releases.
/// TODO: Update to the actual GitHub repo when ready.
const GITHUB_OWNER: &str = "anthropics";
const GITHUB_REPO: &str = "julie";

/// Current version of the tray app (synced with Julie releases).
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A GitHub release from the releases API.
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

/// Info about an available update.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
}

/// Check GitHub for a newer release. Returns `Some` if an update is available.
async fn check_for_update() -> Option<UpdateInfo> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        GITHUB_OWNER, GITHUB_REPO
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("julie-tray")
        .build()
        .ok()?;

    let release: GitHubRelease = client.get(&url).send().await.ok()?.json().await.ok()?;

    // Strip leading 'v' from tag (e.g., "v4.2.0" → "4.2.0")
    let remote_version = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name);

    if is_newer(remote_version, CURRENT_VERSION) {
        Some(UpdateInfo {
            version: remote_version.to_string(),
            url: release.html_url,
        })
    } else {
        None
    }
}

/// Simple semver comparison: is `remote` newer than `current`?
fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .filter_map(|part| part.parse().ok())
            .collect()
    };

    let r = parse(remote);
    let c = parse(current);

    // Compare component by component
    for i in 0..r.len().max(c.len()) {
        let rv = r.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if rv > cv {
            return true;
        }
        if rv < cv {
            return false;
        }
    }
    false
}

/// Run the periodic update checker. Spawned as an async task.
pub async fn check_periodically(app: AppHandle) {
    // Wait before first check
    tokio::time::sleep(INITIAL_DELAY).await;

    loop {
        match check_for_update().await {
            Some(update) => {
                // Show system notification
                use tauri_plugin_notification::NotificationExt;
                let _ = app
                    .notification()
                    .builder()
                    .title("Julie Update Available")
                    .body(format!("Version {} is ready to download", update.version))
                    .show();

                // Store update info for the tray menu to pick up
                // The health poller will rebuild the menu on its next cycle
                let mut guard = UPDATE_STATE.lock().unwrap();
                *guard = Some(update);
            }
            None => {
                // No update or check failed — clear any stale state
                // (keeps showing if already found, only clears on explicit re-check)
            }
        }

        tokio::time::sleep(CHECK_INTERVAL).await;
    }
}

/// Global update state — read by the tray menu builder.
static UPDATE_STATE: std::sync::Mutex<Option<UpdateInfo>> = std::sync::Mutex::new(None);

/// Get the current update info (if any update is available).
pub fn available_update() -> Option<UpdateInfo> {
    UPDATE_STATE.lock().ok()?.clone()
}

/// Get the URL for the available update (called from menu event handler).
pub fn update_url() -> Option<String> {
    available_update().map(|u| u.url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        assert!(is_newer("4.2.0", "4.1.7"));
        assert!(is_newer("4.1.8", "4.1.7"));
        assert!(is_newer("5.0.0", "4.9.9"));
        assert!(!is_newer("4.1.7", "4.1.7"));
        assert!(!is_newer("4.1.6", "4.1.7"));
        assert!(!is_newer("3.0.0", "4.1.7"));
    }

    #[test]
    fn test_is_newer_different_lengths() {
        assert!(is_newer("4.2", "4.1.7"));
        assert!(is_newer("4.1.7.1", "4.1.7"));
        assert!(!is_newer("4.1", "4.1.7"));
    }
}
