mod commands;
mod crypto;
mod vault;
mod api_server;
#[cfg(target_os = "windows")]
mod wts_monitor;

use commands::AppState;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Create shared state as Arc so the API server thread can access it
    let shared_state = Arc::new(AppState {
        vault_data: Mutex::new(None),
        master_password: Mutex::new(None),
        api_token: Mutex::new(None),
    });

    let api_state = shared_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(shared_state)
        .setup(move |app| {
            // Start API server for browser extension
            api_server::start_api_server(api_state);

            // Build tray menu
            let show_item = MenuItem::with_id(app, "show", "Show Vault", true, None::<&str>)?;
            let lock_item = MenuItem::with_id(app, "lock", "Lock Vault", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&show_item, &lock_item, &quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Vault")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            window.show().ok();
                            window.set_focus().ok();
                        }
                    }
                    "lock" => {
                        if let Some(window) = app.get_webview_window("main") {
                            window.eval("document.getElementById('lock-btn')?.click()").ok();
                            window.show().ok();
                            window.set_focus().ok();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            window.show().ok();
                            window.set_focus().ok();
                        }
                    }
                })
                .build(app)?;

            // Intercept window close to hide to tray instead
            let window = app.get_webview_window("main").unwrap();
            let window_clone = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    window_clone.hide().ok();
                }
            });

            // Start WTS session lock monitor (Windows only)
            #[cfg(target_os = "windows")]
            {
                let lock_window = app.get_webview_window("main").unwrap();
                wts_monitor::start_wts_monitor(move || {
                    lock_window
                        .eval("document.getElementById('lock-btn')?.click()")
                        .ok();
                });
            }

            // Register global hotkey: Ctrl+Shift+V to show/focus vault
            // We track the last foreground window title on a background poll
            // because by the time the shortcut fires, focus may have shifted.
            use std::sync::atomic::AtomicBool;
            static VAULT_FOCUSED: AtomicBool = AtomicBool::new(false);

            let last_title: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
            let poll_title = last_title.clone();

            std::thread::spawn(move || {
                use windows::Win32::Foundation::CloseHandle;
                use windows::Win32::System::Threading::{
                    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
                };
                use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

                loop {
                    let name = unsafe {
                        let hwnd = GetForegroundWindow();
                        let mut pid = 0u32;
                        GetWindowThreadProcessId(hwnd, Some(&mut pid));

                        if pid == 0 {
                            String::new()
                        } else {
                            match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                                Ok(handle) => {
                                    let mut buf = [0u16; 512];
                                    let mut size = buf.len() as u32;
                                    let result = QueryFullProcessImageNameW(
                                        handle,
                                        PROCESS_NAME_FORMAT(0),
                                        windows::core::PWSTR(buf.as_mut_ptr()),
                                        &mut size,
                                    );
                                    let _ = CloseHandle(handle);
                                    if result.is_ok() && size > 0 {
                                        let path = String::from_utf16_lossy(&buf[..size as usize]);
                                        // Extract just the filename without extension
                                        path.rsplit('\\')
                                            .next()
                                            .unwrap_or("")
                                            .trim_end_matches(".exe")
                                            .trim_end_matches(".EXE")
                                            .to_string()
                                    } else {
                                        String::new()
                                    }
                                }
                                Err(_) => String::new(),
                            }
                        }
                    };
                    // Only update if it's NOT the Vault process
                    if !name.eq_ignore_ascii_case("vault") && !name.is_empty() {
                        *poll_title.lock().unwrap() = name;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            });

            let hotkey_window = app.get_webview_window("main").unwrap();
            let hotkey_title = last_title.clone();
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyV);
            app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, _event| {
                let title = hotkey_title.lock().unwrap().clone();

                hotkey_window.show().ok();
                hotkey_window.set_focus().ok();

                // Pass the title to the frontend to pre-fill search
                if !title.is_empty() {
                    let escaped = title.replace('\\', "\\\\").replace('\'', "\\'");
                    hotkey_window.eval(&format!(
                        "window.__vaultLastApp = '{}'; document.getElementById('search-input').value = '{}'; document.getElementById('search-input').dispatchEvent(new Event('input'));",
                        escaped, escaped
                    )).ok();
                }
            }).ok();

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_exists,
            commands::create_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::is_unlocked,
            commands::list_entries,
            commands::get_entry,
            commands::add_entry,
            commands::edit_entry,
            commands::delete_entry,
            commands::list_categories,
            commands::generate_password,
            commands::check_vault_integrity,
            commands::check_breach,
            commands::get_vault_path,
            commands::export_vault,
            commands::import_vault,
            commands::import_csv,
            commands::get_api_token,
            commands::auto_type,
            commands::set_start_on_boot,
            commands::get_start_on_boot,
            commands::get_foreground_window_title,
            commands::get_sync_folder,
            commands::set_sync_folder,
            commands::sync_status,
            commands::sync_push,
            commands::sync_pull,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
