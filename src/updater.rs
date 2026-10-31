use anyhow::Result;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tracing::{debug, info};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_API_LATEST_RELEASE: &str =
    "https://api.github.com/repos/Gingaaa/lid-commit/releases/latest";

static UPDATE_AVAILABLE: AtomicBool = AtomicBool::new(false);
static LATEST_RELEASE_INFO: Mutex<Option<UpdateInfo>> = Mutex::new(None);

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_name: String,
    pub html_url: String,
    pub body: String,
    pub installer_download_url: Option<String>,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    body: Option<String>,
    assets: Option<Vec<GitHubAsset>>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Query GitHub API to see if a newer version is released
pub async fn check_for_updates() -> Result<Option<UpdateInfo>> {
    debug!("Checking GitHub for latest LidCommit releases...");

    let client = reqwest::Client::builder()
        .user_agent(format!("LidCommit/{}", CURRENT_VERSION))
        .timeout(std::time::Duration::from_secs(8))
        .build()?;

    let response = client
        .get(GITHUB_API_LATEST_RELEASE)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await?;

    if !response.status().is_success() {
        debug!(
            "GitHub release check returned status: {}",
            response.status()
        );
        return Ok(None);
    }

    let release: GitHubRelease = response.json().await?;
    let remote_tag = release.tag_name.trim().trim_start_matches('v');

    if is_newer_version(remote_tag, CURRENT_VERSION) {
        let installer_url = release.assets.and_then(|assets| {
            assets
                .into_iter()
                .find(|a| a.name.ends_with(".exe"))
                .map(|a| a.browser_download_url)
        });

        let info = UpdateInfo {
            current_version: CURRENT_VERSION.to_string(),
            latest_version: remote_tag.to_string(),
            release_name: release
                .name
                .unwrap_or_else(|| format!("Version v{}", remote_tag)),
            html_url: release.html_url,
            body: release.body.unwrap_or_default(),
            installer_download_url: installer_url,
        };

        UPDATE_AVAILABLE.store(true, Ordering::Relaxed);
        if let Ok(mut lock) = LATEST_RELEASE_INFO.lock() {
            *lock = Some(info.clone());
        }

        info!(
            "🚀 New LidCommit update available: v{} (Current: v{})",
            remote_tag, CURRENT_VERSION
        );

        Ok(Some(info))
    } else {
        debug!(
            "LidCommit is up to date (current: v{}, remote: v{})",
            CURRENT_VERSION, remote_tag
        );
        Ok(None)
    }
}

pub fn get_available_update() -> Option<UpdateInfo> {
    if UPDATE_AVAILABLE.load(Ordering::Relaxed) {
        LATEST_RELEASE_INFO.lock().ok().and_then(|l| l.clone())
    } else {
        None
    }
}

/// Semantic version comparison: returns true if remote > current
fn is_newer_version(remote: &str, current: &str) -> bool {
    let parse_semver = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|part| {
                // Strip pre-release tags like -beta
                let clean = part.split('-').next().unwrap_or(part);
                clean.parse::<u32>().ok()
            })
            .collect()
    };

    let r_parts = parse_semver(remote);
    let c_parts = parse_semver(current);

    let max_len = r_parts.len().max(c_parts.len());
    for i in 0..max_len {
        let r = r_parts.get(i).copied().unwrap_or(0);
        let c = c_parts.get(i).copied().unwrap_or(0);
        if r > c {
            return true;
        } else if r < c {
            return false;
        }
    }

    false
}

/// Periodically run update checks every 24 hours in the background
pub async fn start_background_updater_loop() {
    // Initial check after 5 seconds of launch to allow network initialization
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    if let Ok(Some(update)) = check_for_updates().await {
        crate::notifier::notify_update_available(&update).await;
    }

    // Repeat every 24 hours
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(86400));
    loop {
        interval.tick().await;
        if let Ok(Some(update)) = check_for_updates().await {
            crate::notifier::notify_update_available(&update).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        assert!(is_newer_version("0.3.0", "0.2.0"));
        assert!(is_newer_version("0.2.1", "0.2.0"));
        assert!(is_newer_version("1.0.0", "0.9.9"));
        assert!(!is_newer_version("0.2.0", "0.2.0"));
        assert!(!is_newer_version("0.1.9", "0.2.0"));
        assert!(!is_newer_version("0.2.0-rc1", "0.2.0"));
    }
}
