use tracing::warn;

use crate::CommitResult;

/// Show OS notification for the results of auto-commit
pub async fn notify_results(results: &[CommitResult]) {
    let committed: Vec<_> = results.iter().filter(|r| r.committed).collect();
    let failed: Vec<_> = results
        .iter()
        .filter(|r| r.error.is_some())
        .collect();
    let _skipped: Vec<_> = results
        .iter()
        .filter(|r| !r.committed && r.error.is_none())
        .collect();

    if committed.is_empty() && failed.is_empty() {
        // Nothing to notify
        return;
    }

    let title = "🔒 LidCommit";

    let body = if !committed.is_empty() && failed.is_empty() {
        // All good
        if committed.len() == 1 {
            let r = &committed[0];
            format!(
                "Auto-committed {} file(s) to {}\n\"{}\"",
                r.files_changed,
                r.project_path
                    .split(['/', '\\'])
                    .last()
                    .unwrap_or("project"),
                r.commit_message
            )
        } else {
            format!(
                "Auto-committed {} project(s) — {} files total",
                committed.len(),
                committed.iter().map(|r| r.files_changed).sum::<usize>()
            )
        }
    } else if committed.is_empty() && !failed.is_empty() {
        format!(
            "⚠️ Auto-commit failed for {} project(s). Check logs.",
            failed.len()
        )
    } else {
        format!(
            "Committed {} project(s), {} failed. Check logs.",
            committed.len(),
            failed.len()
        )
    };

    show_notification(title, &body).await;
}

/// Show OS notification for an error
pub async fn notify_error(message: &str) {
    show_notification("🔒 LidCommit Error", message).await;
}

#[cfg(target_os = "windows")]
static POPUP_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static LAST_STATUS_MESSAGE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Store the outcome of the lid close auto-commit process
pub fn record_commit_status(results: &[CommitResult], err: Option<&str>) {
    let committed: Vec<_> = results.iter().filter(|r| r.committed).collect();
    let failed: Vec<_> = results.iter().filter(|r| r.error.is_some()).collect();

    let msg = if let Some(e) = err {
        format!("⚠️ Auto-commit encountered an error:\n{}", e)
    } else if !committed.is_empty() {
        if committed.len() == 1 {
            let r = &committed[0];
            let repo_name = r.project_path.split(['/', '\\']).last().unwrap_or("project");
            format!(
                "✅ Successfully auto-committed changes!\n\n📁 Project: {}\n📝 Files: {}\n💬 Message: \"{}\"",
                repo_name, r.files_changed, r.commit_message
            )
        } else {
            let mut summary = format!(
                "✅ Successfully auto-committed {} project(s):\n",
                committed.len()
            );
            for r in &committed {
                let repo_name = r.project_path.split(['/', '\\']).last().unwrap_or("project");
                summary.push_str(&format!("\n• {}: \"{}\"", repo_name, r.commit_message));
            }
            summary
        }
    } else if !failed.is_empty() {
        format!(
            "⚠️ Auto-commit failed for {} project(s). Please check git logs.",
            failed.len()
        )
    } else {
        "ℹ️ No uncommitted changes were detected in your open projects.".to_string()
    };

    if let Ok(mut lock) = LAST_STATUS_MESSAGE.lock() {
        *lock = Some(msg);
    }
}

/// Show notification and popup when laptop wakes up / lid is reopened
pub async fn notify_lid_resumed() {
    let status_detail = {
        LAST_STATUS_MESSAGE
            .lock()
            .ok()
            .and_then(|mut lock| lock.take())
            .unwrap_or_else(|| "Welcome back!\n\nThe laptop lid was closed.".to_string())
    };

    #[cfg(target_os = "windows")]
    {
        use std::sync::atomic::Ordering;
        if POPUP_ACTIVE
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            let popup_text = format!("Welcome back!\n\n{}", status_detail);
            let toast_body = status_detail.clone();

            // Also fire an OS Toast notification so it is visible immediately in the notification center
            tokio::spawn(async move {
                show_notification("🔒 LidCommit Status", &toast_body).await;
            });

            std::thread::spawn(move || {
                use windows::core::HSTRING;
                use windows::Win32::UI::WindowsAndMessaging::{
                    MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SERVICE_NOTIFICATION,
                    MB_SETFOREGROUND, MB_TOPMOST,
                };
                let text_wide = HSTRING::from(popup_text.as_str());
                let title_wide = HSTRING::from("LidCommit Status");
                unsafe {
                    let _ = MessageBoxW(
                        None,
                        &text_wide,
                        &title_wide,
                        MB_OK
                            | MB_ICONINFORMATION
                            | MB_TOPMOST
                            | MB_SETFOREGROUND
                            | MB_SERVICE_NOTIFICATION,
                    );
                }
                POPUP_ACTIVE.store(false, Ordering::Release);
            });
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        show_notification(
            "🔒 LidCommit Status",
            &status_detail,
        )
        .await;
    }
}

async fn show_notification(title: &str, body: &str) {
    // notify-rust handles cross-platform notifications
    // On Windows: Toast notifications via Windows Runtime
    // On macOS: NSUserNotification / UNUserNotificationCenter
    // On Linux: libnotify / D-Bus

    #[cfg(not(target_os = "linux"))]
    {
        use notify_rust::Notification;
        if let Err(e) = Notification::new()
            .summary(title)
            .body(body)
            .icon("dialog-information")
            .timeout(notify_rust::Timeout::Milliseconds(6000))
            .show()
        {
            warn!("Failed to show notification: {}", e);
        }
    }

    // On Linux, use notify-send via subprocess for reliability
    #[cfg(target_os = "linux")]
    {
        let _ = tokio::process::Command::new("notify-send")
            .arg("-t")
            .arg("6000")
            .arg(title)
            .arg(body)
            .output()
            .await;
    }
}
