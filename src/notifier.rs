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

/// Show notification and popup when laptop wakes up / lid is reopened
pub async fn notify_lid_resumed() {
    #[cfg(target_os = "windows")]
    {
        use std::sync::atomic::Ordering;
        if POPUP_ACTIVE
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            std::thread::spawn(|| {
                use windows::core::w;
                use windows::Win32::UI::WindowsAndMessaging::{
                    MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MB_SYSTEMMODAL,
                };
                unsafe {
                    let _ = MessageBoxW(
                        None,
                        w!("Welcome back!\n\nThe laptop lid was off / laptop was in sleep mode."),
                        w!("LidCommit Notice"),
                        MB_OK | MB_ICONINFORMATION | MB_SYSTEMMODAL | MB_SETFOREGROUND,
                    );
                }
                POPUP_ACTIVE.store(false, Ordering::Release);
            });
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        show_notification(
            "🔒 LidCommit Alert",
            "Welcome back! The laptop lid was off / laptop was asleep.",
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
