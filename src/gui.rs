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

pub fn run_desktop_app(_config: Arc<tokio::sync::RwLock<Config>>) -> anyhow::Result<()> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // Build Native Tray Menu
    let tray_menu = Menu::new();
    let item_open = MenuItem::new("Open Dashboard", true, None);
    let item_sync = MenuItem::new("Sync Repositories Now", true, None);
    let item_toggle = MenuItem::new("Pause / Resume Monitoring", true, None);
    let item_quit = MenuItem::new("Quit LidCommit", true, None);

    let _ = tray_menu.append_items(&[&item_open, &item_sync, &item_toggle, &item_quit]);

    let icon_bytes = include_bytes!("../assets/icon.png");
    let icon = load_icon(icon_bytes);

    let mut tray_builder = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("LidCommit - Auto-Commit on Lid Close");

    if let Some(ic) = icon {
        tray_builder = tray_builder.with_icon(ic);
    }

    let _tray_icon = tray_builder.build()?;

    // Create Hidden or Visible main Window
    let window = WindowBuilder::new()
        .with_title("LidCommit - Dashboard")
        .with_inner_size(tao::dpi::LogicalSize::new(880.0, 680.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(600.0, 500.0))
        .with_visible(true)
        .build(&event_loop)?;

    let html_content = include_str!("dashboard.html");

    let proxy_ipc = proxy.clone();
    let webview = WebViewBuilder::new()
        .with_html(html_content)
        .with_ipc_handler(move |req: wry::http::Request<String>| {
            let msg = req.into_body();
            handle_ipc_message(&msg, &proxy_ipc);
        })
        .build(&window)?;

    // Listen to Tray events in background
    let proxy_tray = proxy.clone();
    let open_id = item_open.id().clone();
    let sync_id = item_sync.id().clone();
    let toggle_id = item_toggle.id().clone();
    let quit_id = item_quit.id().clone();

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
                            tokio::spawn(async {
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

    // Run the main event loop
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(UserEvent::ShowWindow) => {
                window.set_visible(true);
                window.set_focus();
                trigger_data_fetch(&proxy);
            }
            Event::UserEvent(UserEvent::UpdateData) => {
                trigger_data_fetch(&proxy);
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

fn handle_ipc_message(msg: &str, proxy: &EventLoopProxy<UserEvent>) {
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
                    tokio::spawn(async {
                        crate::trigger_manual_sync().await;
                    });
                }
                "get_status" => {
                    let _ = proxy.send_event(UserEvent::UpdateData);
                }
                _ => {}
            }
        }
    }
}

fn trigger_data_fetch(proxy: &EventLoopProxy<UserEvent>) {
    let paused = DAEMON_PAUSED.load(Ordering::Relaxed);
    let status = if paused { "paused" } else { "running" };

    let history = RECENT_HISTORY.lock().unwrap().clone();
    let proxy = proxy.clone();

    tokio::spawn(async move {
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

            project_payload.push(serde_json::json!({
                "name": name,
                "path": p.path,
                "ide": p.ide,
                "has_changes": has_changes,
            }));
        }

        let data = serde_json::json!({
            "status": status,
            "projects": project_payload,
            "history": history,
        });

        let script = format!("if (window.updateDashboard) {{ window.updateDashboard({}); }}", data);
        let _ = proxy.send_event(UserEvent::EvalScript(script));
    });
}

fn load_icon(bytes: &[u8]) -> Option<Icon> {
    if let Ok(img) = image::load_from_memory(bytes) {
        let rgba = img.into_rgba8();
        let (width, height) = rgba.dimensions();
        Icon::from_rgba(rgba.into_raw(), width, height).ok()
    } else {
        None
    }
}
