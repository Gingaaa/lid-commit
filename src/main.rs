use anyhow::Result;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod ai;
mod config;
mod git;
pub mod gui;
mod ide_scanner;
mod lid_monitor;
mod notifier;

pub async fn trigger_manual_sync() {
    let dummy_config = match config::load().await {
        Ok(c) => c,
        Err(_) => config::Config::default(),
    };
    info!("⚡ Manual sync requested via GUI dashboard...");
    match handle_lid_close(&dummy_config).await {
        Ok(results) => {
            gui::record_history(&results);
            notifier::record_commit_status(&results, None);
            notifier::notify_results(&results).await;
        }
        Err(e) => {
            error!("Manual sync failed: {}", e);
            notifier::record_commit_status(&[], Some(&e.to_string()));
            notifier::notify_error(&e.to_string()).await;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("LIDCOMMIT_LOG")
                .unwrap_or_else(|_| EnvFilter::new("lid_commit=info")),
        )
        .init();

    // Load .env file if present
    dotenvy::dotenv().ok();

    info!("🔒 LidCommit daemon starting...");

    // Load config
    let config = config::load().await?;
    info!("✅ Config loaded");

    // Create event channel: lid monitor sends events, main loop processes them
    let (tx, mut rx) = tokio::sync::mpsc::channel::<LidEvent>(1);

    // Spawn the platform-specific lid monitor in a background task
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = lid_monitor::run(tx_clone).await {
            error!("Lid monitor error: {}", e);
        }
    });

    info!("👂 Listening for lid close events...");

    let mut last_close_time: Option<std::time::Instant> = None;
    let mut last_resume_time: Option<std::time::Instant> = None;
    let debounce_window = std::time::Duration::from_secs(3);

    // Check for CLI argument to launch headless or with GUI (default: with GUI)
    let has_headless_arg = std::env::args().any(|arg| arg == "--headless" || arg == "-d");

    // Spawn GUI on dedicated thread if not headless
    if !has_headless_arg {
        let config_clone = std::sync::Arc::new(tokio::sync::RwLock::new(config.clone()));
        let handle = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            if let Err(e) = gui::run_desktop_app(config_clone, handle) {
                tracing::error!("Desktop GUI error: {}", e);
            }
        });
    }

    // Main event loop
    while let Some(event) = rx.recv().await {
        if gui::is_daemon_paused() {
            info!("⏸️ Daemon is paused via GUI/tray. Skipping event: {:?}", event);
            continue;
        }

        match event {
            LidEvent::LidClosed => {
                if let Some(last) = last_close_time {
                    if last.elapsed() < debounce_window {
                        info!("⏳ Duplicate LidClosed event within debounce window, ignoring."); 
                        continue;
                    }
                }
                last_close_time = Some(std::time::Instant::now());

                info!("🔒 Lid close / sleep detected! Starting auto-commit process...");
                match handle_lid_close(&config).await {
                    Ok(results) => {
                        gui::record_history(&results);
                        let committed: Vec<_> = results.iter().filter(|r| r.committed).collect();
                        if committed.is_empty() {
                            info!("✅ No uncommitted changes found in any IDE project.");
                        } else {
                            info!("✅ Auto-committed {} project(s).", committed.len());
                            for result in &committed {
                                info!("  📁 {}: {}", result.project_path, result.commit_message);
                            }
                        }
                        // Record detailed status so it is shown in the notice popup when user returns
                        notifier::record_commit_status(&results, None);
                        notifier::notify_results(&results).await;
                    }
                    Err(e) => {
                        error!("Auto-commit failed: {}", e);
                        notifier::record_commit_status(&[], Some(&e.to_string()));
                        notifier::notify_error(&e.to_string()).await;
                    }
                }
            }
            LidEvent::LidResumed => {
                if let Some(last) = last_resume_time {
                    if last.elapsed() < debounce_window {
                        info!("⏳ Duplicate LidResumed event within debounce window, ignoring.");
                        continue;
                    }
                }
                last_resume_time = Some(std::time::Instant::now());

                info!("🔓 Laptop resumed / lid opened! Showing notice popup...");
                notifier::notify_lid_resumed().await;
            }
        }
    }

    Ok(())
}

/// Event types from the lid monitor
#[derive(Debug, Clone)]
pub enum LidEvent {
    LidClosed,
    LidResumed,
}

/// Result for one project's auto-commit attempt
#[derive(Debug, Clone)]
pub struct CommitResult {
    pub project_path: String,
    pub committed: bool,
    pub pushed: bool,
    pub commit_message: String,
    pub files_changed: usize,
    pub error: Option<String>,
}

/// Core logic: find IDE projects and auto-commit each one
async fn handle_lid_close(config: &config::Config) -> Result<Vec<CommitResult>> {
    let mut results = Vec::new();

    // 1. Scan for open IDE projects
    let projects = ide_scanner::find_open_projects(config).await?;

    if projects.is_empty() {
        info!("No IDE projects found open.");
        return Ok(results);
    }

    info!("Found {} open IDE project(s).", projects.len());

    // 2. For each project, attempt auto-commit
    for project in projects {
        info!("Processing project: {}", project.path);

        let result = process_project(config, &project).await;
        results.push(result);
    }

    Ok(results)
}

/// Process a single project: check for changes, generate commit message, commit & push
async fn process_project(
    config: &config::Config,
    project: &ide_scanner::Project,
) -> CommitResult {
    let path = project.path.clone();

    // Check git status
    match git::check_project(&path, config).await {
        Err(e) => {
            return CommitResult {
                project_path: path,
                committed: false,
                pushed: false,
                commit_message: String::new(),
                files_changed: 0,
                error: Some(e.to_string()),
            };
        }
        Ok(status) => {
            if !status.has_changes {
                info!("  ✅ Nothing to commit in {}", path);
                return CommitResult {
                    project_path: path,
                    committed: false,
                    pushed: false,
                    commit_message: "Nothing to commit".into(),
                    files_changed: 0,
                    error: None,
                };
            }

            // Safety checks
            if let Some(reason) = status.should_skip(config) {
                info!("  ⏭️  Skipping {}: {}", path, reason);
                return CommitResult {
                    project_path: path,
                    committed: false,
                    pushed: false,
                    commit_message: format!("Skipped: {}", reason),
                    files_changed: status.changed_files,
                    error: None,
                };
            }

            // Generate AI commit message
            let commit_msg = match ai::generate_commit_message(config, &status.diff_summary).await
            {
                Ok(msg) => msg,
                Err(e) => {
                    // Fallback to timestamp-based message
                    let now = chrono::Local::now();
                    let fallback = format!(
                        "{} auto-save at {}",
                        config.git.commit_prefix,
                        now.format("%Y-%m-%d %H:%M")
                    );
                    error!("AI message generation failed: {}, using fallback", e);
                    fallback
                }
            };

            // Stage, commit, and push
            match git::commit_and_push(&path, &commit_msg, config.git.push).await {
                Ok(pushed) => CommitResult {
                    project_path: path,
                    committed: true,
                    pushed,
                    commit_message: commit_msg,
                    files_changed: status.changed_files,
                    error: None,
                },
                Err(e) => CommitResult {
                    project_path: path,
                    committed: false,
                    pushed: false,
                    commit_message: commit_msg,
                    files_changed: status.changed_files,
                    error: Some(e.to_string()),
                },
            }
        }
    }
}
