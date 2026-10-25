use anyhow::Result;
use sysinfo::{ProcessesToUpdate, System};
use tracing::{debug, info};

use crate::config::Config;

/// A discovered open IDE project
#[derive(Debug, Clone)]
pub struct Project {
    /// Absolute path to the project root directory
    pub path: String,
    /// Which IDE this was detected from
    pub ide: String,
}

/// Scan running processes to find open IDE projects
pub async fn find_open_projects(config: &Config) -> Result<Vec<Project>> {
    let mut projects = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    let watch_all = config.ide.watch.contains(&"any".to_string());
    let watch_vscode = watch_all || config.ide.watch.contains(&"vscode".to_string());
    let watch_jetbrains = watch_all || config.ide.watch.contains(&"jetbrains".to_string());

    // Refresh the system process list
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All);

    for (pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_lowercase();

        // VS Code detection
        if watch_vscode && is_vscode_process(&name) {
            debug!("Found VS Code process: {} (pid {})", name, pid);
            if let Some(path) = get_project_from_process(process).await {
                if seen_paths.insert(path.clone()) {
                    info!("  VS Code project: {}", path);
                    projects.push(Project {
                        path,
                        ide: "vscode".into(),
                    });
                }
            }
        }

        // JetBrains detection
        if watch_jetbrains && is_jetbrains_process(&name) {
            debug!("Found JetBrains process: {} (pid {})", name, pid);
            if let Some(path) = get_project_from_process(process).await {
                if seen_paths.insert(path.clone()) {
                    info!("  JetBrains project: {}", path);
                    projects.push(Project {
                        path,
                        ide: "jetbrains".into(),
                    });
                }
            }
        }

        // Generic: any process whose CWD is a git repo
        if watch_all {
            if let Some(cwd) = process.cwd() {
                let cwd_str = cwd.to_string_lossy().to_string();
                if !cwd_str.is_empty()
                    && !is_system_path(&cwd_str)
                    && !seen_paths.contains(&cwd_str)
                {
                    if is_git_repo(&cwd_str) {
                        seen_paths.insert(cwd_str.clone());
                        debug!("  Generic CWD git repo: {} (from {})", cwd_str, name);
                        projects.push(Project {
                            path: cwd_str,
                            ide: format!("generic({})", name),
                        });
                    }
                }
            }
        }
    }

    // Also check VS Code's recent workspace list from its config files
    // (catches the case where VS Code is open but args aren't visible)
    if watch_vscode {
        let vscode_is_running = sys.processes().values().any(|p| {
            is_vscode_process(&p.name().to_string_lossy().to_lowercase())
        });
        if vscode_is_running {
            let vscode_projects = get_vscode_recent_workspaces().await;
            for path in vscode_projects {
                if seen_paths.insert(path.clone()) {
                    info!("  VS Code recent workspace: {}", path);
                    projects.push(Project {
                        path,
                        ide: "vscode-recent".into(),
                    });
                }
            }
        }
    }

    Ok(projects)
}

fn is_vscode_process(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "code"
        || lower == "code.exe"
        || lower == "code-insiders"
        || lower == "code-insiders.exe"
        || lower == "codium"
        || lower == "codium.exe"
        || lower == "cursor"
        || lower == "cursor.exe"
        || lower.contains("antigravity")
        || lower.contains("windsurf")
}

fn is_jetbrains_process(name: &str) -> bool {
    let jetbrains_names = [
        "idea", "pycharm", "goland", "webstorm", "rider", "clion",
        "datagrip", "phpstorm", "rubymine", "appcode",
    ];
    jetbrains_names.iter().any(|&n| name.starts_with(n))
}

/// Try to extract the project path from a process (from CLI args or CWD)
async fn get_project_from_process(process: &sysinfo::Process) -> Option<String> {
    // Check CLI args: IDEs are typically launched with the folder as an argument
    // e.g., `code /path/to/project`
    for arg in process.cmd().iter().skip(1) {
        let arg_str = arg.to_string_lossy();
        let path = std::path::Path::new(arg_str.as_ref());
        if path.is_dir() && !is_system_path(&arg_str) {
            if is_git_repo(&arg_str) {
                return Some(arg_str.into_owned());
            }
        }
    }

    // Fallback: check CWD
    if let Some(cwd) = process.cwd() {
        let cwd_str = cwd.to_string_lossy().to_string();
        if !cwd_str.is_empty() && !is_system_path(&cwd_str) && is_git_repo(&cwd_str) {
            return Some(cwd_str);
        }
    }

    None
}

/// Read VS Code and derived editors' (Antigravity, Cursor, Windsurf) workspaces from storage.json
async fn get_vscode_recent_workspaces() -> Vec<String> {
    let mut paths = Vec::new();

    let config_dirs: Vec<std::path::PathBuf> = {
        let mut dirs = Vec::new();
        if let Some(config) = dirs::config_dir() {
            let app_names = [
                "Code",
                "Code - Insiders",
                "Antigravity IDE",
                "Antigravity",
                "Cursor",
                "Windsurf",
                "VSCodium",
            ];
            for app in app_names {
                dirs.push(
                    config
                        .join(app)
                        .join("User")
                        .join("globalStorage")
                        .join("storage.json"),
                );
            }
        }
        dirs
    };

    for storage_path in config_dirs {
        if storage_path.exists() {
            if let Ok(contents) = tokio::fs::read_to_string(&storage_path).await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
                    let mut extract_uri = |folder_uri: &str| {
                        let path = folder_uri
                            .strip_prefix("file://")
                            .unwrap_or(folder_uri)
                            .to_string();
                        let decoded = urlencoding::decode(&path)
                            .map(|s| s.into_owned())
                            .unwrap_or(path);
                        let mut final_path = decoded;
                        #[cfg(target_os = "windows")]
                        {
                            if final_path.starts_with('/') && final_path.len() > 2 && final_path.chars().nth(2) == Some(':') {
                                final_path = final_path[1..].to_string();
                            }
                            final_path = final_path.replace('/', "\\");
                        }
                        if std::path::Path::new(&final_path).is_dir() && is_git_repo(&final_path) {
                            paths.push(final_path);
                        }
                    };

                    // 1. Check openedPathsList.workspaces3
                    if let Some(workspaces) = json
                        .get("openedPathsList")
                        .and_then(|v| v.get("workspaces3"))
                        .and_then(|v| v.as_array())
                    {
                        for ws in workspaces {
                            if let Some(folder_uri) = ws.get("folderUri").and_then(|v| v.as_str()) {
                                extract_uri(folder_uri);
                            }
                        }
                    }

                    // 2. Check backupWorkspaces.folders (currently open windows in VS Code / Antigravity IDE)
                    if let Some(folders) = json
                        .get("backupWorkspaces")
                        .and_then(|v| v.get("folders"))
                        .and_then(|v| v.as_array())
                    {
                        for f in folders {
                            if let Some(folder_uri) = f.get("folderUri").and_then(|v| v.as_str()) {
                                extract_uri(folder_uri);
                            }
                        }
                    }
                }
            }
        }
    }

    paths
}

/// Quickly check if a directory is a git repo (has a .git folder/file)
fn is_git_repo(path: &str) -> bool {
    std::path::Path::new(path).join(".git").exists()
}

/// Filter out system directories we should never auto-commit
fn is_system_path(path: &str) -> bool {
    let system_prefixes = [
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/etc",
        "/proc",
        "/sys",
        "C:\\Windows",
        "C:\\Program Files",
        "/Applications",
    ];
    system_prefixes.iter().any(|p| path.starts_with(p))
}
