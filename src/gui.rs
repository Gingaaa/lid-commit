use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::WindowBuilder,
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIconBuilder,
};
use wry::WebViewBuilder;

use crate::config::Config;
use crate::CommitResult;

static DAEMON_PAUSED: AtomicBool = AtomicBool::new(false);
static PAUSED_PROJECTS: std::sync::LazyLock<Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));
static RECENT_HISTORY: Mutex<Vec<HistoryEntry>> = Mutex::new(Vec::new());

#[derive(Clone, serde::Serialize)]
pub struct HistoryEntry {
    pub project_name: String,
    pub commit_message: String,
    pub files_changed: usize,
    pub committed: bool,
    pub error: Option<String>,
    pub time: String,
}

pub fn is_daemon_paused() -> bool {
    DAEMON_PAUSED.load(Ordering::Relaxed)
}

pub fn is_project_paused(path: &str) -> bool {
    if let Ok(lock) = PAUSED_PROJECTS.lock() {
        let normalized = path.replace('/', "\\").to_lowercase();
        lock.iter().any(|p| p.replace('/', "\\").to_lowercase() == normalized)
    } else {
        false
    }
}

pub fn toggle_project_pause(path: &str) -> bool {
    if let Ok(mut lock) = PAUSED_PROJECTS.lock() {
        let normalized = path.to_string();
        let exists = lock.iter().any(|p| p.replace('/', "\\").to_lowercase() == normalized.replace('/', "\\").to_lowercase());
        if exists {
            lock.retain(|p| p.replace('/', "\\").to_lowercase() != normalized.replace('/', "\\").to_lowercase());
            false // Now active (not paused)
        } else {
            lock.insert(normalized);
            true // Now paused
        }
    } else {
        false
    }
}

pub fn record_history(results: &[CommitResult]) {
    let now = chrono::Local::now().format("%H:%M:%S").to_string();
    if let Ok(mut history) = RECENT_HISTORY.lock() {
        for r in results {
            let name = r
                .project_path
                .split(['/', '\\'])
                .last()
                .unwrap_or("project")
                .to_string();
            history.insert(
                0,
                HistoryEntry {
                    project_name: name,
                    commit_message: r.commit_message.clone(),
                    files_changed: r.files_changed,
                    committed: r.committed,
                    error: r.error.clone(),
                    time: now.clone(),
                },
            );
        }
        if history.len() > 20 {
            history.truncate(20);
        }
    }
}

pub enum UserEvent {
    ShowWindow,
    UpdateData,
    EvalScript(String),
}

pub fn run_desktop_app(
    _config: Arc<tokio::sync::RwLock<Config>>,
    handle: tokio::runtime::Handle,
) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    use tao::platform::windows::EventLoopBuilderExtWindows;

    #[allow(unused_mut)]
    let mut builder = EventLoopBuilder::<UserEvent>::with_user_event();
    #[cfg(target_os = "windows")]
    builder.with_any_thread(true);

    let event_loop = builder.build();
    let proxy = event_loop.create_proxy();

    // Build Native Tray Menu
    let tray_menu = Menu::new();
    let item_open = MenuItem::new("Open Dashboard", true, None);
    let item_sync = MenuItem::new("Sync Repositories Now", true, None);
    let item_toggle = MenuItem::new("Pause / Resume Monitoring", true, None);
    let item_quit = MenuItem::new("Quit LidCommit", true, None);

    let _ = tray_menu.append_items(&[&item_open, &item_sync, &item_toggle, &item_quit]);

    let icon_bytes = include_bytes!("../assets/app_icon.jpg");
    let tray_icon_obj = load_tray_icon(icon_bytes);
    let window_icon_obj = load_window_icon(icon_bytes);

    let mut tray_builder = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("LidCommit - Auto-Commit on Lid Close");

    if let Some(ic) = tray_icon_obj {
        tray_builder = tray_builder.with_icon(ic);
    }

    let _tray_icon = tray_builder.build()?;

    // Create Hidden or Visible main Window
    let mut win_builder = WindowBuilder::new()
        .with_title("LidCommit - Dashboard")
        .with_inner_size(tao::dpi::LogicalSize::new(880.0, 680.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(600.0, 500.0))
        .with_visible(true);

    if let Some(win_icon) = window_icon_obj {
        win_builder = win_builder.with_window_icon(Some(win_icon));
    }

    let window = win_builder.build(&event_loop)?;

    let html_content = include_str!("dashboard.html");

    let proxy_ipc = proxy.clone();
    let handle_ipc = handle.clone();
    let webview = WebViewBuilder::new()
        .with_html(html_content)
        .with_ipc_handler(move |req: wry::http::Request<String>| {
            let msg = req.into_body();
            handle_ipc_message(&msg, &proxy_ipc, &handle_ipc);
        })
        .build(&window)?;

    // Listen to Tray events in background
    let proxy_tray = proxy.clone();
    let open_id = item_open.id().clone();
    let sync_id = item_sync.id().clone();
    let toggle_id = item_toggle.id().clone();
    let quit_id = item_quit.id().clone();
    let handle_tray = handle.clone();

    std::thread::spawn(move || {
        let menu_channel = MenuEvent::receiver();
        let tray_channel = tray_icon::TrayIconEvent::receiver();

        loop {
            crossbeam_channel::select! {
                recv(menu_channel) -> event => {
                    if let Ok(event) = event {
                        if event.id == open_id {
                            let _ = proxy_tray.send_event(UserEvent::ShowWindow);
                        } else if event.id == sync_id {
                            handle_tray.spawn(async {
                                crate::trigger_manual_sync().await;
                            });
                        } else if event.id == toggle_id {
                            let was_paused = DAEMON_PAUSED.load(Ordering::Relaxed);
                            DAEMON_PAUSED.store(!was_paused, Ordering::Relaxed);
                            let _ = proxy_tray.send_event(UserEvent::UpdateData);
                        } else if event.id == quit_id {
                            std::process::exit(0);
                        }
                    }
                }
                recv(tray_channel) -> event => {
                    if let Ok(event) = event {
                        if matches!(event, tray_icon::TrayIconEvent::Click { .. }) {
                            let _ = proxy_tray.send_event(UserEvent::ShowWindow);
                        }
                    }
                }
            }
        }
    });

    let handle_loop = handle.clone();

    // Run the main event loop
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(UserEvent::ShowWindow) => {
                window.set_visible(true);
                window.set_focus();
                trigger_data_fetch(&proxy, &handle_loop);
            }
            Event::UserEvent(UserEvent::UpdateData) => {
                trigger_data_fetch(&proxy, &handle_loop);
            }
            Event::UserEvent(UserEvent::EvalScript(script)) => {
                let _ = webview.evaluate_script(&script);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // Minimize to tray instead of quitting
                window.set_visible(false);
            }
            _ => (),
        }
    });
}

fn handle_ipc_message(
    msg: &str,
    proxy: &EventLoopProxy<UserEvent>,
    handle: &tokio::runtime::Handle,
) {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg) {
        if let Some(action) = json.get("action").and_then(|v| v.as_str()) {
            match action {
                "pause" => {
                    DAEMON_PAUSED.store(true, Ordering::Relaxed);
                    let _ = proxy.send_event(UserEvent::UpdateData);
                }
                "resume" => {
                    DAEMON_PAUSED.store(false, Ordering::Relaxed);
                    let _ = proxy.send_event(UserEvent::UpdateData);
                }
                "sync_now" => {
                    handle.spawn(async {
                        crate::trigger_manual_sync().await;
                    });
                }
                "sync_project" => {
                    if let Some(path) = json.get("path").and_then(|v| v.as_str()) {
                        let path = path.to_string();
                        handle.spawn(async move {
                            crate::trigger_project_sync(&path).await;
                        });
                    }
                }
                "toggle_project_pause" => {
                    if let Some(path) = json.get("path").and_then(|v| v.as_str()) {
                        toggle_project_pause(path);
                        let _ = proxy.send_event(UserEvent::UpdateData);
                    }
                }
                "get_status" => {
                    let _ = proxy.send_event(UserEvent::UpdateData);
                }
                "save_settings" => {
                    let json_clone = json.clone();
                    let proxy_clone = proxy.clone();
                    handle.spawn(async move {
                        let mut current_config = crate::config::load().await.unwrap_or_default();
                        
                        if let Some(push) = json_clone.get("push").and_then(|v| v.as_bool()) {
                            current_config.git.push = push;
                            current_config.general.push_after_commit = push;
                        }
                        if let Some(notify) = json_clone.get("notify").and_then(|v| v.as_bool()) {
                            current_config.general.notify_on_success = notify;
                            current_config.general.notify_on_failure = notify;
                        }
                        if let Some(provider) = json_clone.get("ai_provider").and_then(|v| v.as_str()) {
                            current_config.ai.provider = provider.to_string();
                        }
                        if let Some(key) = json_clone.get("api_key").and_then(|v| v.as_str()) {
                            if !key.trim().is_empty() {
                                current_config.ai.api_key = Some(key.trim().to_string());
                            } else {
                                current_config.ai.api_key = None;
                            }
                        }
                        if let Some(url) = json_clone.get("ollama_url").and_then(|v| v.as_str()) {
                            if !url.trim().is_empty() {
                                current_config.ai.ollama_url = url.trim().to_string();
                            }
                        }
                        if let Some(model) = json_clone.get("model").and_then(|v| v.as_str()) {
                            if !model.trim().is_empty() {
                                current_config.ai.model = model.trim().to_string();
                            }
                        }
                        if let Some(prefix) = json_clone.get("commit_prefix").and_then(|v| v.as_str()) {
                            current_config.git.commit_prefix = prefix.to_string();
                        }
                        if let Some(max_files) = json_clone.get("max_files").and_then(|v| v.as_u64()) {
                            current_config.safety.max_files_changed = max_files as usize;
                        }

                        if let Err(e) = crate::config::save(&current_config).await {
                            tracing::error!("Failed to save settings: {}", e);
                        } else {
                            tracing::info!("Updated configuration saved successfully");
                        }
                        let _ = proxy_clone.send_event(UserEvent::UpdateData);
                    });
                }
                _ => {}
            }
        }
    }
}

fn trigger_data_fetch(
    proxy: &EventLoopProxy<UserEvent>,
    handle: &tokio::runtime::Handle,
) {
    let paused = DAEMON_PAUSED.load(Ordering::Relaxed);
    let status = if paused { "paused" } else { "running" };

    let history = RECENT_HISTORY.lock().unwrap().clone();
    let proxy = proxy.clone();

    handle.spawn(async move {
        let dummy_config = crate::config::load().await.unwrap_or_default();
        let projects = crate::ide_scanner::find_open_projects(&dummy_config)
            .await
            .unwrap_or_default();

        let mut project_payload = Vec::new();
        for p in projects {
            let name = p
                .path
                .split(['/', '\\'])
                .last()
                .unwrap_or("project")
                .to_string();

            let has_changes = match crate::git::check_project(&p.path, &dummy_config).await {
                Ok(status) => status.has_changes,
                Err(_) => false,
            };

            let is_paused = is_project_paused(&p.path);

            project_payload.push(serde_json::json!({
                "name": name,
                "path": p.path,
                "ide": p.ide,
                "has_changes": has_changes,
                "is_paused": is_paused,
            }));
        }

        let config_payload = serde_json::json!({
            "push": dummy_config.git.push,
            "notify": dummy_config.general.notify_on_success,
            "ai_provider": dummy_config.ai.provider,
            "api_key": dummy_config.ai.api_key.unwrap_or_default(),
            "ollama_url": dummy_config.ai.ollama_url,
            "model": dummy_config.ai.model,
            "commit_prefix": dummy_config.git.commit_prefix,
            "max_files": dummy_config.safety.max_files_changed,
        });

        let data = serde_json::json!({
            "status": status,
            "projects": project_payload,
            "history": history,
            "config": config_payload,
        });

        let script = format!("if (window.updateDashboard) {{ window.updateDashboard({}); }}", data);
        let _ = proxy.send_event(UserEvent::EvalScript(script));
    });
}

fn load_tray_icon(bytes: &[u8]) -> Option<Icon> {
    if let Ok(img) = image::load_from_memory(bytes) {
        let resized = img.resize_exact(32, 32, image::imageops::FilterType::Lanczos3);
        let rgba = resized.into_rgba8();
        let (width, height) = rgba.dimensions();
        Icon::from_rgba(rgba.into_raw(), width, height).ok()
    } else {
        None
    }
}

fn load_window_icon(bytes: &[u8]) -> Option<tao::window::Icon> {
    if let Ok(img) = image::load_from_memory(bytes) {
        let resized = img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
        let rgba = resized.into_rgba8();
        let (width, height) = rgba.dimensions();
        tao::window::Icon::from_rgba(rgba.into_raw(), width, height).ok()
    } else {
        None
    }
}
