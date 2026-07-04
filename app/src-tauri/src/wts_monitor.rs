//! Windows Terminal Services session change monitor.
//! Detects when the Windows session is locked and triggers a callback.

use std::sync::Arc;
use std::thread;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    PostQuitMessage, RegisterClassW, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_DESTROY, WM_WTSSESSION_CHANGE, WNDCLASSW, WTS_SESSION_LOCK,
};

use windows::core::PCWSTR;

/// Callback type invoked when a session lock is detected.
type LockCallback = Arc<dyn Fn() + Send + Sync>;

/// Stores the callback in thread-local storage for the window proc.
static mut LOCK_CALLBACK: Option<LockCallback> = None;

/// Window procedure that handles WTS session change messages.
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_WTSSESSION_CHANGE => {
            if wparam.0 as u32 == WTS_SESSION_LOCK {
                if let Some(ref cb) = LOCK_CALLBACK {
                    cb();
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Start monitoring for Windows session lock events.
/// Spawns a background thread with a message-only window.
/// Calls `on_lock` when the session is locked.
pub fn start_wts_monitor<F>(on_lock: F)
where
    F: Fn() + Send + Sync + 'static,
{
    let callback = Arc::new(on_lock);

    thread::spawn(move || unsafe {
        // Store callback
        LOCK_CALLBACK = Some(callback);

        // Register window class
        let class_name: Vec<u16> = "VaultWtsMonitor\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };

        RegisterClassW(&wc);

        // Create message-only window
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        )
        .unwrap_or(HWND::default());

        if hwnd.0.is_null() {
            log::error!("Failed to create WTS monitor window");
            return;
        }

        // Register for session notifications
        let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, Some(hwnd), 0, 0).as_bool() {
            let _ = DispatchMessageW(&msg);
        }

        // Cleanup
        let _ = WTSUnRegisterSessionNotification(hwnd);
        let _ = DestroyWindow(hwnd);
    });
}
