use anyhow::Result;
use tracing::info;

use crate::LidEvent;

/// Platform dispatcher: calls the correct OS-specific lid monitor
pub async fn run(tx: tokio::sync::mpsc::Sender<LidEvent>) -> Result<()> {
    info!("Starting lid monitor for: {}", std::env::consts::OS);

    #[cfg(target_os = "windows")]
    return windows::run(tx).await;

    #[cfg(target_os = "macos")]
    return macos::run(tx).await;

    #[cfg(target_os = "linux")]
    return linux::run(tx).await;

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    anyhow::bail!("Unsupported platform: {}", std::env::consts::OS);
}

// ─────────────────────────────────────────────────────────────────
// Windows implementation
// ─────────────────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
mod windows {
    use anyhow::Result;
    use tracing::{error, info};

    use crate::LidEvent;

    // Windows constants we need (from the Win32 SDK)
    // PBT_POWERSETTINGCHANGE = 0x8013
    const PBT_POWERSETTINGCHANGE: u32 = 0x8013;
    // PBT_APMSUSPEND = 0x0004
    const PBT_APMSUSPEND: u32 = 0x0004;
    // PBT_APMRESUMESUSPEND = 0x0007
    const PBT_APMRESUMESUSPEND: u32 = 0x0007;
    // PBT_APMRESUMEAUTOMATIC = 0x0012
    const PBT_APMRESUMEAUTOMATIC: u32 = 0x0012;
    // WM_POWERBROADCAST = 0x0218
    const WM_POWERBROADCAST: u32 = 0x0218;
    // DEVICE_NOTIFY_WINDOW_HANDLE = 0
    const DEVICE_NOTIFY_WINDOW_HANDLE: u32 = 0;

    // GUID_LIDSWITCH_STATE_CHANGE from WinNT.h: {BA3E0F4D-B817-4094-A2D1-D56379E6A0F3}
    // Fires on physical laptop lid open/close events:
    // Data = 0: lid closed
    // Data = 1: lid opened
    const GUID_LIDSWITCH_STATE_CHANGE: windows::core::GUID = windows::core::GUID::from_values(
        0xBA3E0F4D,
        0xB817,
        0x4094,
        [0xA2, 0xD1, 0xD5, 0x63, 0x79, 0xE6, 0xA0, 0xF3],
    );

    pub async fn run(tx: tokio::sync::mpsc::Sender<LidEvent>) -> Result<()> {
        info!("Windows lid monitor: starting message-only window...");

        // The Windows message pump is blocking, run it on a dedicated OS thread
        let (os_tx, mut os_rx) = tokio::sync::mpsc::channel::<LidEvent>(4);

        std::thread::spawn(move || {
            if let Err(e) = windows_message_loop(os_tx) {
                error!("Windows message loop error: {}", e);
            }
        });

        // Forward OS events into the main event channel
        while let Some(event) = os_rx.recv().await {
            let _ = tx.send(event).await;
        }

        Ok(())
    }

    fn windows_message_loop(tx: tokio::sync::mpsc::Sender<LidEvent>) -> anyhow::Result<()> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Power::{
            RegisterPowerSettingNotification, RegisterSuspendResumeNotification,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DispatchMessageW, GetMessageW, RegisterClassExW, HWND_MESSAGE, MSG,
            REGISTER_NOTIFICATION_FLAGS, WNDCLASSEXW,
        };
        use windows::core::PCWSTR;

        unsafe {
            // Store the sender in a thread-local so the wndproc can access it
            TX.with(|cell| {
                *cell.borrow_mut() = Some(tx);
            });

            let class_name: Vec<u16> = "LidCommitMsgWnd\0".encode_utf16().collect();

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wnd_proc),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };

            let atom = RegisterClassExW(&wc);
            if atom == 0 {
                anyhow::bail!("RegisterClassExW failed");
            }

            // HWND_MESSAGE = invisible message-only window (receives WM_ messages but no visible UI)
            let hwnd = CreateWindowExW(
                Default::default(),
                PCWSTR(class_name.as_ptr()),
                PCWSTR::null(),
                Default::default(),
                0, 0, 0, 0,
                HWND_MESSAGE,
                None, None, None,
            )?;

            // Register to receive GUID_LIDSWITCH_STATE_CHANGE power setting notifications.
            // This fires directly when the laptop lid opens or closes.
            let _notif = RegisterPowerSettingNotification(
                HANDLE(hwnd.0),
                &GUID_LIDSWITCH_STATE_CHANGE,
                REGISTER_NOTIFICATION_FLAGS(DEVICE_NOTIFY_WINDOW_HANDLE),
            )?;

            // Also register for system suspend/resume events so that if closing the lid
            // triggers sleep/modern standby, we catch the suspend broadcast as well.
            let _suspend_notif = RegisterSuspendResumeNotification(
                HANDLE(hwnd.0),
                REGISTER_NOTIFICATION_FLAGS(DEVICE_NOTIFY_WINDOW_HANDLE),
            );

            info!(
                "Windows lid monitor: registered for lid-switch and suspend notifications, entering message loop"
            );

            // Standard Win32 message loop (blocking)
            let mut msg = MSG::default();
            loop {
                let ret = GetMessageW(&mut msg, None, 0, 0);
                if ret.0 == 0 || ret.0 == -1 {
                    break;
                }
                DispatchMessageW(&msg);
            }

            Ok(())
        }
    }

    // Thread-local sender used inside the wndproc callback
    thread_local! {
        static TX: std::cell::RefCell<Option<tokio::sync::mpsc::Sender<LidEvent>>> =
            std::cell::RefCell::new(None);
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: windows::Win32::Foundation::HWND,
        msg: u32,
        wparam: windows::Win32::Foundation::WPARAM,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        use windows::Win32::System::Power::POWERBROADCAST_SETTING;
        use windows::Win32::UI::WindowsAndMessaging::DefWindowProcW;

        if msg == WM_POWERBROADCAST {
            let event_type = wparam.0 as u32;

            if event_type == PBT_POWERSETTINGCHANGE {
                let setting_ptr = lparam.0 as *const POWERBROADCAST_SETTING;
                if !setting_ptr.is_null() {
                    let setting = &*setting_ptr;
                    if setting.PowerSetting == GUID_LIDSWITCH_STATE_CHANGE {
                        // Data: 0 = closed, 1 = opened
                        let is_closed = if setting.DataLength > 0 {
                            let state_byte = *setting.Data.as_ptr();
                            state_byte == 0
                        } else {
                            true
                        };

                        if is_closed {
                            tracing::info!("🔒 Lid close event detected via GUID_LIDSWITCH_STATE_CHANGE");
                            send_lid_event(crate::LidEvent::LidClosed);
                        } else {
                            tracing::info!("🔓 Lid open detected via GUID_LIDSWITCH_STATE_CHANGE");
                            send_lid_event(crate::LidEvent::LidResumed);
                        }
                    }
                }
            } else if event_type == PBT_APMSUSPEND {
                tracing::info!("🔒 System suspend (sleep) event detected via PBT_APMSUSPEND");
                send_lid_event(crate::LidEvent::LidClosed);
            } else if event_type == PBT_APMRESUMESUSPEND || event_type == PBT_APMRESUMEAUTOMATIC {
                tracing::info!("🔓 System resume event detected via PBT_APMRESUME");
                send_lid_event(crate::LidEvent::LidResumed);
            }
        }

        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    fn send_lid_event(event: crate::LidEvent) {
        TX.with(|cell| {
            if let Some(tx) = &*cell.borrow() {
                let _ = tx.try_send(event);
            }
        });
    }
}

// ─────────────────────────────────────────────────────────────────
// macOS implementation
// ─────────────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
mod macos {
    use anyhow::Result;
    use tracing::info;

    use crate::LidEvent;

    pub async fn run(tx: tokio::sync::mpsc::Sender<LidEvent>) -> Result<()> {
        info!("macOS lid monitor: registering for system sleep notifications...");

        let (os_tx, mut os_rx) = tokio::sync::mpsc::channel::<LidEvent>(4);

        // IOKit + CFRunLoop must run on a dedicated thread
        std::thread::spawn(move || {
            macos_run_loop(os_tx);
        });

        while let Some(event) = os_rx.recv().await {
            let _ = tx.send(event).await;
        }

        Ok(())
    }

    fn macos_run_loop(tx: tokio::sync::mpsc::Sender<LidEvent>) {
        // kIOMessageSystemWillSleep = 0xe0000280
        // Fires before any sleep event (lid close on MacBook triggers this)
        use std::ffi::c_void;

        unsafe {
            let tx_box = Box::new(tx);
            let refcon = Box::into_raw(tx_box) as *mut c_void;

            extern "C" {
                fn IORegisterForSystemPower(
                    refcon: *mut c_void,
                    notify_port: *mut *mut c_void,
                    callback: unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut c_void),
                    notifier: *mut *mut c_void,
                ) -> u32;
                fn IONotificationPortGetRunLoopSource(notify: *mut c_void) -> *mut c_void;
                fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *const c_void);
                fn CFRunLoopGetCurrent() -> *mut c_void;
                fn CFRunLoopRun();
                static kCFRunLoopDefaultMode: *const c_void;
            }

            unsafe extern "C" fn sleep_callback(
                refcon: *mut c_void,
                _service: *mut c_void,
                message_type: u32,
                _message_argument: *mut c_void,
            ) {
                const K_IO_MESSAGE_SYSTEM_WILL_SLEEP: u32 = 0xe0000280;
                if message_type == K_IO_MESSAGE_SYSTEM_WILL_SLEEP {
                    tracing::info!("macOS: system will sleep (lid close)");
                    let tx = &*(refcon as *const tokio::sync::mpsc::Sender<LidEvent>);
                    let _ = tx.try_send(LidEvent::LidClosed);
                }
            }

            let mut notify_port: *mut c_void = std::ptr::null_mut();
            let mut notifier: *mut c_void = std::ptr::null_mut();

            IORegisterForSystemPower(refcon, &mut notify_port, sleep_callback, &mut notifier);

            let source = IONotificationPortGetRunLoopSource(notify_port);
            CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopDefaultMode);

            tracing::info!("macOS lid monitor: entering CFRunLoop");
            CFRunLoopRun();
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// Linux implementation
// ─────────────────────────────────────────────────────────────────
#[cfg(target_os = "linux")]
mod linux {
    use anyhow::Result;
    use futures_util::StreamExt;
    use tracing::info;
    use zbus::Connection;

    use crate::LidEvent;

    pub async fn run(tx: tokio::sync::mpsc::Sender<LidEvent>) -> Result<()> {
        info!("Linux lid monitor: subscribing to logind PrepareForSleep D-Bus signal...");

        let connection = Connection::system().await?;

        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.login1.Manager")?
            .member("PrepareForSleep")?
            .build();

        let mut stream =
            zbus::MessageStream::for_match_rule(rule, &connection, None).await?;

        info!("Linux lid monitor: waiting for PrepareForSleep signal...");

        while let Some(msg) = stream.next().await {
            match msg {
                Ok(msg) => {
                    // PrepareForSleep(bool): true = going to sleep, false = woke up
                    if let Ok((sleeping,)) = msg.body().deserialize::<(bool,)>() {
                        if sleeping {
                            info!("Linux: PrepareForSleep(true) — going to sleep");
                            let _ = tx.send(LidEvent::LidClosed).await;
                        }
                    }
                }
                Err(e) => tracing::error!("D-Bus stream error: {}", e),
            }
        }

        Ok(())
    }
}
