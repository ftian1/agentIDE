//! Window control commands — called from the frontend via invoke().
//! Bypasses the @tauri-apps/api/window JS API which can fail in webviews.

use tauri::Window;

#[tauri::command]
pub fn minimize_window(window: Window) {
    let _ = window.minimize();
}

#[tauri::command]
pub fn toggle_maximize_window(window: Window) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

#[tauri::command]
pub fn close_window(window: Window) {
    let _ = window.close();
}
