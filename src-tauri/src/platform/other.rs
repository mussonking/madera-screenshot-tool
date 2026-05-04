use tauri::AppHandle;

#[allow(dead_code)]
pub fn is_wayland_session() -> bool {
    false
}

pub fn apply_layer_shell_overlay(_window: &tauri::WebviewWindow, _width: i32) {}

pub fn toggle_panel_side(_app: &AppHandle, _window_label: &str) -> Result<String, String> {
    Ok("right".to_string())
}

pub fn toggle_panel_monitor(_app: &AppHandle, _window_label: &str) -> Result<(), String> {
    Ok(())
}

pub fn set_panel_keyboard(
    _app: &AppHandle,
    _window_label: &str,
    _enabled: bool,
) -> Result<(), String> {
    Ok(())
}

pub fn active_window_geometry() -> Option<(i32, i32, i32, i32)> {
    None
}

pub fn page_down_active_window() {}

pub fn work_area_near_cursor() -> (i32, i32, i32, i32) {
    (0, 0, 1920, 1080)
}

pub fn cursor_and_monitor_info() -> (i32, i32, i32, i32, i32, i32) {
    (100, 100, 0, 0, 1920, 1080)
}

pub fn snapshot_for_paste_panel() {}

pub fn start_focus_tracker() {}

pub fn start_paste_hook(
    _app: AppHandle,
    _open_quick_paste_history: fn(&AppHandle) -> Result<(), String>,
    _is_multi_paste_open: fn(&AppHandle) -> bool,
) {
}

pub fn paste_into_previous_window(_app: AppHandle) {}

#[allow(dead_code)]
pub fn simulate_paste() {}

#[allow(dead_code)]
pub fn type_text(_text: &str) {}

#[allow(dead_code)]
pub fn simulate_tabby_breakline() {}
