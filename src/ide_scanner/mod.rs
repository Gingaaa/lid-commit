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

    // Also check currently opened windows in running IDEs (VS Code, Antigravity, Cursor, etc.)
    if watch_vscode {
        let running_proc_names: Vec<String> = sys
            .processes()
            .values()
            .map(|p| p.name().to_string_lossy().to_lowercase())
            .collect();

        let vscode_projects = get_running_ide_active_workspaces(&running_proc_names).await;
        for (path, ide_name) in vscode_projects {
            if seen_paths.insert(path.clone()) {
                info!("  Active {} workspace: {}", ide_name, path);
                projects.push(Project {
                    path,
                    ide: ide_name,
                });
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

/// Check currently open windows across IDEs (VS Code, Antigravity, Cursor, etc.)
/// Only includes projects if that specific IDE process is currently running!
async fn get_running_ide_active_workspaces(running_procs: &[String]) -> Vec<(String, String)> {
    let mut results = Vec::new();

    let ide_configs = [
        ("Code", vec!["code", "code.exe"]),
        ("Code - Insiders", vec!["code-insiders", "code-insiders.exe"]),
        ("Antigravity IDE", vec!["antigravity ide", "antigravity.exe"]),
        ("Antigravity", vec!["antigravity", "antigravity.exe"]),
        ("Cursor", vec!["cursor", "cursor.exe"]),
        ("Windsurf", vec!["windsurf", "windsurf.exe"]),
        ("VSCodium", vec!["codium", "codium.exe"]),
    ];

    let config_dir = match dirs::config_dir() {
        Some(d) => d,
        None => return results,
    };

    for (app_dir_name, proc_signatures) in ide_configs {
        // Only inspect this IDE if its process is ACTUALLY running right now
        let is_running = running_procs.iter().any(|proc_name| {
            proc_signatures.iter().any(|&sig| proc_name.contains(sig))
        });

        if !is_running {
            continue;
        }

        let storage_path = config_dir
            .join(app_dir_name)
            .join("User")
            .join("globalStorage")
            .join("storage.json");

        if !storage_path.exists() {
            continue;
        }

        if let Ok(contents) = tokio::fs::read_to_string(&storage_path).await {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
                let parse_uri = |folder_uri: &str| -> Option<String> {
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
                        if final_path.starts_with('/')
                            && final_path.len() > 2
                            && final_path.chars().nth(2) == Some(':')
                        {
                            final_path = final_path[1..].to_string();
                        }
                        final_path = final_path.replace('/', "\\");
                    }
                    if std::path::Path::new(&final_path).is_dir() && is_git_repo(&final_path) {
                        Some(final_path)
                    } else {
                        None
                    }
                };

                // Look at `windowsState.openedWindows` — these are the windows that are currently OPEN in this running IDE
                if let Some(opened) = json
                    .get("windowsState")
                    .and_then(|v| v.get("openedWindows"))
                    .and_then(|v| v.as_array())
                {
                    for win in opened {
                        if let Some(folder_uri) = win.get("folder").and_then(|v| v.as_str()) {
                            if let Some(valid_path) = parse_uri(folder_uri) {
                                results.push((valid_path, app_dir_name.to_lowercase()));
                            }
                        }
                    }
                }
            }
        }
    }

    results
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
