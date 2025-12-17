mod capture;
mod clipboard;
mod clipboard_monitor;
mod color_picker;
mod history;
mod native_selection;
mod window_layout;

use capture::CaptureManager;
use clipboard::ClipboardManager;
use clipboard_monitor::{ClipboardContent, ClipboardMonitor, ClipboardSettings};
use color_picker::{ColorFormat, ColorInfo, ColorPickSettings};
use history::{HistoryItem, HistoryItemType, HistoryManager, ScreenshotRecord};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct AppState {
    pub capture_manager: Mutex<CaptureManager>,
    pub clipboard_manager: Mutex<ClipboardManager>,
    pub clipboard_monitor: Arc<ClipboardMonitor>,
    pub history_manager: Mutex<HistoryManager>,
    pub pending_capture: Mutex<Option<PendingCapture>>,
    pub settings: Mutex<AppSettings>,
    pub clipboard_settings: Mutex<ClipboardSettings>,
    pub color_settings: Mutex<ColorPickSettings>,
    pub should_exit: Mutex<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingCapture {
    pub image_data: String, // Base64 encoded
    pub width: u32,
    pub height: u32,
    pub monitor_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub hotkey: String,
    pub auto_copy: bool,
    pub max_history: usize,
    pub max_image_width: Option<u32>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Shift+S".to_string(),
            auto_copy: true,
            max_history: 150,
            max_image_width: Some(1568), // Optimal for Claude
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureResult {
    pub image_data: String,
    pub width: u32,
    pub height: u32,
}

// Monitor system power events (sleep/wake)
fn monitor_system_power_events(app: AppHandle) {
    use windows::Win32::System::Power::RegisterPowerSettingNotification;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
        TranslateMessage, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
        WM_POWERBROADCAST,
    };
    use windows::Win32::Foundation::HWND;
    use windows::core::PCWSTR;
    use std::sync::atomic::{AtomicPtr, Ordering};

    // Store app handle in a static for the window proc
    static APP_HANDLE: AtomicPtr<AppHandle> = AtomicPtr::new(std::ptr::null_mut());

    unsafe extern "system" fn power_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: windows::Win32::Foundation::WPARAM,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        const WM_POWERBROADCAST: u32 = 0x0218;
        const PBT_APMRESUMEAUTOMATIC: u32 = 18;
        const PBT_APMRESUMESUSPEND: u32 = 7;
        const PBT_POWERSETTINGCHANGE: u32 = 0x8013;

        if msg == WM_POWERBROADCAST {
            let event = wparam.0 as u32;

            // Handle both regular resume events AND display power events
            if event == PBT_APMRESUMEAUTOMATIC || event == PBT_APMRESUMESUSPEND || event == PBT_POWERSETTINGCHANGE {
                // System resumed from sleep or display turned on - emit event after a delay
                let app_ptr = APP_HANDLE.load(Ordering::SeqCst);
                if !app_ptr.is_null() {
                    let app = &*app_ptr;

                    // Spawn thread to avoid blocking the message pump
                    let app_clone = app.clone();
                    std::thread::spawn(move || {
                        // Wait for displays to come back online
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        let _ = app_clone.emit("system-wake-from-sleep", ());
                        println!("[Desktop Guardian] Wake from sleep detected!");
                    });
                }
            }
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    unsafe {
        // Store app handle
        let app_box = Box::new(app);
        APP_HANDLE.store(Box::into_raw(app_box), Ordering::SeqCst);

        // Register window class
        let class_name: Vec<u16> = "PowerMonitorClass\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(power_wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        // Create message-only window
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE::default(),
            0, 0, 0, 0,
            HWND_MESSAGE,
            None,
            None,
            None,
        );

        if let Ok(hwnd) = hwnd {
            // GUID_CONSOLE_DISPLAY_STATE - notifies when displays turn on/off
            let guid_console_display = windows::core::GUID::from_values(
                0x6fe69556, 0x704a, 0x47a0, [0x8f, 0x24, 0xc2, 0x8d, 0x93, 0x6f, 0xda, 0x47]
            );
            use windows::Win32::UI::WindowsAndMessaging::REGISTER_NOTIFICATION_FLAGS;
            let _ = RegisterPowerSettingNotification(
                hwnd,
                &guid_console_display,
                REGISTER_NOTIFICATION_FLAGS(0), // DEVICE_NOTIFY_WINDOW_HANDLE
            );

            // Message loop
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

// Commands
#[tauri::command]
async fn get_monitors() -> Result<Vec<MonitorInfo>, String> {
    CaptureManager::get_monitors().map_err(|e| e.to_string())
}

#[tauri::command]
async fn capture_all_screens(state: State<'_, AppState>) -> Result<Vec<CaptureResult>, String> {
    let manager = state.capture_manager.lock().map_err(|e| e.to_string())?;
    manager.capture_all_screens().map_err(|e| e.to_string())
}

#[tauri::command]
async fn capture_region(
    state: State<'_, AppState>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    source_image: String,
) -> Result<CaptureResult, String> {
    let manager = state.capture_manager.lock().map_err(|e| e.to_string())?;
    manager
        .crop_region(&source_image, x, y, width, height)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn copy_to_clipboard(state: State<'_, AppState>, image_data: String) -> Result<(), String> {
    let manager = state.clipboard_manager.lock().map_err(|e| e.to_string())?;
    manager
        .copy_image_to_clipboard(&image_data)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_to_history(
    state: State<'_, AppState>,
    image_data: String,
    width: u32,
    height: u32,
) -> Result<ScreenshotRecord, String> {
    let mut manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    let clipboard_settings = state.clipboard_settings.lock().map_err(|e| e.to_string())?;

    // Save to unified history table (which also saves to legacy table for compatibility)
    let history_item = manager
        .save_screenshot_to_unified(&image_data, width, height, clipboard_settings.max_items)
        .map_err(|e| e.to_string())?;

    // Convert HistoryItem to ScreenshotRecord for backwards compatibility
    Ok(ScreenshotRecord {
        id: history_item.id,
        filename: history_item.filename.unwrap_or_default(),
        thumbnail: history_item.thumbnail.unwrap_or_default(),
        created_at: history_item.created_at,
        width: history_item.width.unwrap_or(width),
        height: history_item.height.unwrap_or(height),
        saved_path: history_item.saved_path,
    })
}

#[tauri::command]
async fn get_history(state: State<'_, AppState>) -> Result<Vec<ScreenshotRecord>, String> {
    let manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    manager.get_all_screenshots().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_screenshot_by_id(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<String>, String> {
    let manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    manager.get_screenshot_image(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_screenshot(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    manager.delete_screenshot(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    let mut manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    manager.clear_all().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.clone())
}

#[tauri::command]
async fn update_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    let mut current = state.settings.lock().map_err(|e| e.to_string())?;
    *current = settings;
    Ok(())
}

#[tauri::command]
async fn save_image_to_file(
    image_data: String,
    path: String,
) -> Result<(), String> {
    use base64::Engine;
    use std::fs;

    let data = base64::engine::general_purpose::STANDARD
        .decode(&image_data)
        .map_err(|e| e.to_string())?;

    fs::write(&path, &data).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn resize_image(
    image_data: String,
    max_width: u32,
) -> Result<CaptureResult, String> {
    let manager = CaptureManager::new();
    manager.resize_image(&image_data, max_width).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_default_save_path() -> Result<String, String> {
    // Get Pictures folder and create sstool subfolder
    let pictures_dir = dirs::picture_dir()
        .ok_or_else(|| "Cannot find Pictures folder".to_string())?;

    let sstool_dir = pictures_dir.join("sstool");

    // Create directory if it doesn't exist
    if !sstool_dir.exists() {
        std::fs::create_dir_all(&sstool_dir)
            .map_err(|e| format!("Failed to create sstool folder: {}", e))?;
    }

    // Generate filename with timestamp
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("screenshot_{}.png", timestamp);
    let full_path = sstool_dir.join(filename);

    Ok(full_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn toggle_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;

    let autostart_manager = app.autolaunch();

    if enabled {
        autostart_manager.enable().map_err(|e| e.to_string())?;
    } else {
        autostart_manager.disable().map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
async fn is_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;

    let autostart_manager = app.autolaunch();
    autostart_manager.is_enabled().map_err(|e| e.to_string())
}

// ============================================
// Unified History Commands (clipboard + screenshots)
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardSettingsDto {
    pub enabled: bool,
    pub max_items: usize,
    pub excluded_apps: Vec<String>,
    pub auto_cleanup_days: Option<u32>,
}

impl From<ClipboardSettings> for ClipboardSettingsDto {
    fn from(s: ClipboardSettings) -> Self {
        Self {
            enabled: s.enabled,
            max_items: s.max_items,
            excluded_apps: s.excluded_apps,
            auto_cleanup_days: s.auto_cleanup_days,
        }
    }
}

impl From<ClipboardSettingsDto> for ClipboardSettings {
    fn from(s: ClipboardSettingsDto) -> Self {
        Self {
            enabled: s.enabled,
            max_items: s.max_items,
            excluded_apps: s.excluded_apps,
            auto_cleanup_days: s.auto_cleanup_days,
        }
    }
}

#[tauri::command]
async fn get_unified_history(
    state: State<'_, AppState>,
    filter_type: Option<String>,
) -> Result<Vec<HistoryItem>, String> {
    let manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    let filter = filter_type.and_then(|t| t.parse::<HistoryItemType>().ok());
    manager
        .get_all_history_items(filter)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_history_item_image(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<String>, String> {
    let manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    manager
        .get_history_item_image(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_history_item(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    manager
        .delete_history_item(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_pin_item(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let mut manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    manager.toggle_pin(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_history(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<HistoryItem>, String> {
    let manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    manager.search_history(&query).map_err(|e| e.to_string())
}

#[tauri::command]
async fn clear_unified_history(state: State<'_, AppState>) -> Result<(), String> {
    let mut manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    manager.clear_all_unified().map_err(|e| e.to_string())
}

#[tauri::command]
async fn copy_text_to_clipboard(
    state: State<'_, AppState>,
    text: String,
) -> Result<(), String> {
    let manager = state.clipboard_manager.lock().map_err(|e| e.to_string())?;
    manager
        .copy_text_to_clipboard(&text)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_clipboard_settings(state: State<'_, AppState>) -> Result<ClipboardSettingsDto, String> {
    let settings = state.clipboard_settings.lock().map_err(|e| e.to_string())?;
    Ok(ClipboardSettingsDto::from(settings.clone()))
}

#[tauri::command]
async fn update_clipboard_settings(
    state: State<'_, AppState>,
    settings: ClipboardSettingsDto,
) -> Result<(), String> {
    let new_settings: ClipboardSettings = settings.into();

    // Update monitor settings
    state.clipboard_monitor.update_settings(new_settings.clone());

    // Update stored settings
    let mut stored = state.clipboard_settings.lock().map_err(|e| e.to_string())?;
    *stored = new_settings;

    Ok(())
}

#[tauri::command]
async fn is_clipboard_monitoring(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.clipboard_monitor.is_running())
}

#[tauri::command]
async fn start_clipboard_monitoring(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();

    if state.clipboard_monitor.is_running() {
        return Ok(());
    }

    let app_handle = app.clone();

    state
        .clipboard_monitor
        .start(move |content| {
            let app = app_handle.clone();

            // Process clipboard content in a separate task
            std::thread::spawn(move || {
                let state = app.state::<AppState>();
                let clipboard_settings = state.clipboard_settings.lock().unwrap().clone();

                if !clipboard_settings.enabled {
                    return;
                }

                let mut manager = match state.history_manager.lock() {
                    Ok(m) => m,
                    Err(_) => return,
                };

                match content {
                    ClipboardContent::Text(text) => {
                        // Check for duplicates
                        if let Ok(Some(last)) = manager.get_last_clipboard_hash() {
                            if last == text {
                                return;
                            }
                        }

                        let _ = manager.save_clipboard_text(
                            &text,
                            None, // source_app - could be implemented with Windows API
                            clipboard_settings.max_items,
                        );

                        // Emit event to frontend
                        let _ = app.emit("clipboard-changed", "text");
                    }
                    ClipboardContent::Image { data, width, height } => {
                        if let Some(base64) = ClipboardMonitor::image_to_base64(&data, width, height)
                        {
                            let _ = manager.save_clipboard_image(
                                &base64,
                                width as u32,
                                height as u32,
                                None,
                                clipboard_settings.max_items,
                            );

                            // Emit event to frontend
                            let _ = app.emit("clipboard-changed", "image");
                        }
                    }
                    ClipboardContent::Empty => {}
                }
            });
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_clipboard_monitoring(state: State<'_, AppState>) -> Result<(), String> {
    state
        .clipboard_monitor
        .stop()
        .map_err(|e| e.to_string())
}

// ============================================
// Color Picker Commands
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorPickSettingsDto {
    pub format: String,
    pub max_history: usize,
    pub magnifier_size: u8,
}

impl From<ColorPickSettings> for ColorPickSettingsDto {
    fn from(s: ColorPickSettings) -> Self {
        Self {
            format: match s.format {
                ColorFormat::HexUpper => "hex_upper".to_string(),
                ColorFormat::HexLower => "hex_lower".to_string(),
                ColorFormat::Rgb => "rgb".to_string(),
                ColorFormat::Hsl => "hsl".to_string(),
            },
            max_history: s.max_history,
            magnifier_size: s.magnifier_size,
        }
    }
}

impl From<ColorPickSettingsDto> for ColorPickSettings {
    fn from(s: ColorPickSettingsDto) -> Self {
        Self {
            format: match s.format.as_str() {
                "hex_lower" => ColorFormat::HexLower,
                "rgb" => ColorFormat::Rgb,
                "hsl" => ColorFormat::Hsl,
                _ => ColorFormat::HexUpper,
            },
            max_history: s.max_history,
            magnifier_size: s.magnifier_size,
        }
    }
}

#[tauri::command]
async fn get_pixel_color(x: i32, y: i32) -> Result<Option<ColorInfo>, String> {
    Ok(color_picker::get_pixel_color(x, y))
}

#[tauri::command]
async fn get_magnifier_region(
    center_x: i32,
    center_y: i32,
    radius: i32,
) -> Result<Option<Vec<Vec<(u8, u8, u8)>>>, String> {
    Ok(color_picker::get_magnifier_region(center_x, center_y, radius))
}

#[tauri::command]
async fn save_color_pick(
    state: State<'_, AppState>,
    hex: String,
    r: u8,
    g: u8,
    b: u8,
    h: u16,
    s: u8,
    l: u8,
    source_app: Option<String>,
) -> Result<HistoryItem, String> {
    let mut manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    let color_settings = state.color_settings.lock().map_err(|e| e.to_string())?;

    manager
        .save_color_pick(
            &hex,
            (r, g, b),
            (h, s, l),
            source_app.as_deref(),
            color_settings.max_history,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_color_settings(state: State<'_, AppState>) -> Result<ColorPickSettingsDto, String> {
    let settings = state.color_settings.lock().map_err(|e| e.to_string())?;
    Ok(ColorPickSettingsDto::from(settings.clone()))
}

#[tauri::command]
async fn update_color_settings(
    state: State<'_, AppState>,
    settings: ColorPickSettingsDto,
) -> Result<(), String> {
    let mut current = state.color_settings.lock().map_err(|e| e.to_string())?;
    *current = settings.into();
    Ok(())
}

// ============================================
// Desktop Guardian Commands (Window Layout Manager)
// ============================================

#[tauri::command]
async fn get_desktop_monitors() -> Result<Vec<window_layout::MonitorLayout>, String> {
    Ok(window_layout::get_all_monitors())
}

#[tauri::command]
async fn get_current_window_layout() -> Result<Vec<window_layout::WindowPosition>, String> {
    Ok(window_layout::get_all_windows())
}

#[tauri::command]
async fn save_window_layout(name: String) -> Result<window_layout::SavedLayout, String> {
    let windows = window_layout::get_all_windows();
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();

    Ok(window_layout::SavedLayout {
        id,
        name,
        created_at,
        windows,
        is_auto_save: false,
    })
}

#[tauri::command]
async fn restore_window_layout(
    layout: window_layout::SavedLayout,
) -> Result<Vec<(String, Option<String>)>, String> {
    let results = window_layout::match_and_restore_layout(&layout);
    Ok(results
        .into_iter()
        .map(|(name, result)| (name, result.err()))
        .collect())
}

#[tauri::command]
async fn open_desktop_guardian(app: AppHandle) -> Result<(), String> {
    // Check if window already exists
    if let Some(window) = app.get_webview_window("desktop-guardian") {
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        &app,
        "desktop-guardian",
        WebviewUrl::App("index.html".into()),
    )
    .title("Desktop Guardian")
    .inner_size(900.0, 700.0)
    .min_inner_size(700.0, 500.0)
    .center()
    .decorations(true)
    .resizable(true)
    .visible(true)
    .build()
    .map_err(|e| e.to_string())?;

    // Navigate to desktop guardian route
    window
        .eval("window.location.hash = '#/desktop-guardian'")
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn open_selection_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Use native Win32 selection - no WebView, no flash!
    // The native selection captures ALL monitors and returns the cropped image directly
    let app_handle = app.clone();

    std::thread::spawn(move || {
        // Show native selection overlay - it captures all screens internally
        if let Some(selection) = native_selection::show_native_selection() {
            // The selection result now includes the cropped image data
            if let Some(image_data) = selection.image_data {
                // Open editor with the cropped image from native selection
                let _ = open_editor_window(
                    &app_handle,
                    &image_data,
                    selection.width,
                    selection.height,
                );
            }
        }
    });

    Ok(())
}

fn open_editor_window(
    app: &AppHandle,
    image_data: &str,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    // DON'T close existing editor windows - allow multiple editors
    // Each new screenshot opens in a new editor window

    // Generate unique window ID using timestamp
    let window_id = format!("editor-{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis());

    // Store the image data for the editor
    let state = app.state::<AppState>();
    {
        let mut pending = state.pending_capture.lock().unwrap();
        *pending = Some(PendingCapture {
            image_data: image_data.to_string(),
            width,
            height,
            monitor_name: "cropped".to_string(),
        });
    }

    // Calculate window size (image size + toolbar space)
    // Minimum size to show all toolbar buttons
    let min_width = 900.0;
    let min_height = 400.0;
    let window_width = ((width + 40) as f64).max(min_width).min(1920.0);
    let window_height = ((height + 160) as f64).max(min_height).min(1080.0);

    let window = WebviewWindowBuilder::new(app, &window_id, WebviewUrl::App("/editor".into()))
        .title("Screenshot Editor")
        .inner_size(window_width, window_height)
        .min_inner_size(min_width, min_height)
        .center()
        .resizable(true)
        .decorations(true)
        .build()?;

    window.set_focus()?;

    Ok(())
}

fn open_main_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("main") {
        window.set_focus()?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("/".into()))
        .title("Madera.Tools")
        .inner_size(1200.0, 800.0)
        .center()
        .resizable(true)
        .decorations(true)
        .build()?;

    window.set_focus()?;
    Ok(())
}

fn open_history_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("history") {
        window.set_focus()?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(app, "history", WebviewUrl::App("/history".into()))
        .title("Madera.Tools - History")
        .inner_size(1200.0, 800.0)
        .center()
        .resizable(true)
        .decorations(true)
        .build()?;

    window.set_focus()?;
    Ok(())
}

fn open_desktop_guardian_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("desktop-guardian") {
        window.set_focus()?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        "desktop-guardian",
        WebviewUrl::App("index.html".into()),
    )
    .title("Desktop Guardian")
    .inner_size(900.0, 700.0)
    .min_inner_size(700.0, 500.0)
    .center()
    .decorations(true)
    .resizable(true)
    .visible(true)
    .build()?;

    // Navigate to desktop guardian route
    window.eval("window.location.hash = '#/desktop-guardian'")?;
    window.set_focus()?;
    Ok(())
}

fn open_color_picker_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Close existing color picker window if any
    if let Some(window) = app.get_webview_window("colorpicker") {
        let _ = window.close();
    }

    // Capture screen BEFORE opening window (so we capture the actual screen, not the picker window)
    let state = app.state::<AppState>();
    let capture_manager = state.capture_manager.lock().map_err(|e| e.to_string())?;
    let captures = capture_manager.capture_all_screens().map_err(|e| e.to_string())?;
    drop(capture_manager);

    if let Some(capture) = captures.first() {
        // Store the capture for the color picker to use
        let mut pending = state.pending_capture.lock().map_err(|e| e.to_string())?;
        *pending = Some(PendingCapture {
            image_data: capture.image_data.clone(),
            width: capture.width,
            height: capture.height,
            monitor_name: "colorpicker".to_string(),
        });
    }

    // Create fullscreen window for color picking
    let window = WebviewWindowBuilder::new(app, "colorpicker", WebviewUrl::App("/colorpicker".into()))
        .title("Color Picker")
        .fullscreen(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .build()?;

    window.set_focus()?;
    Ok(())
}

#[tauri::command]
async fn trigger_color_picker(app: AppHandle) -> Result<(), String> {
    open_color_picker_window(&app).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_pending_capture(state: State<'_, AppState>) -> Result<Option<PendingCapture>, String> {
    let pending = state.pending_capture.lock().map_err(|e| e.to_string())?;
    Ok(pending.clone())
}

#[tauri::command]
async fn open_editor_with_image(
    app: AppHandle,
    image_data: String,
    width: u32,
    height: u32,
) -> Result<(), String> {
    open_editor_window(&app, &image_data, width, height).map_err(|e| e.to_string())
}

#[tauri::command]
async fn close_window(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn trigger_capture(app: AppHandle) -> Result<(), String> {
    open_selection_window(&app).map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_history_panel(app: AppHandle) -> Result<(), String> {
    open_history_window(&app).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // When a second instance is launched, just focus existing window or show tray notification
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            } else {
                let _ = open_main_window(app);
            }
        }))
        .manage(AppState {
            capture_manager: Mutex::new(CaptureManager::new()),
            clipboard_manager: Mutex::new(ClipboardManager::new()),
            clipboard_monitor: Arc::new(ClipboardMonitor::new()),
            history_manager: Mutex::new(HistoryManager::new().expect("Failed to init history")),
            pending_capture: Mutex::new(None),
            settings: Mutex::new(AppSettings::default()),
            clipboard_settings: Mutex::new(ClipboardSettings::default()),
            color_settings: Mutex::new(ColorPickSettings::default()),
            should_exit: Mutex::new(false),
        })
        .setup(|app| {
            // Check if autostart is enabled to show correct menu state
            let autostart_enabled = {
                use tauri_plugin_autostart::ManagerExt;
                app.autolaunch().is_enabled().unwrap_or(false)
            };

            // Create tray icon
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let capture = MenuItem::with_id(app, "capture", "Capture (Ctrl+Shift+S)", true, None::<&str>)?;
            let colorpicker = MenuItem::with_id(app, "colorpicker", "Pick Color (Ctrl+Shift+X)", true, None::<&str>)?;
            let history = MenuItem::with_id(app, "history", "History (Ctrl+Shift+V)", true, None::<&str>)?;
            let desktop_guardian = MenuItem::with_id(app, "desktop_guardian", "Desktop Guardian", true, None::<&str>)?;
            let restore_layout = MenuItem::with_id(app, "restore_layout", "⚡ Restore Last Layout", true, None::<&str>)?;
            let clipboard_monitor = CheckMenuItem::with_id(app, "clipboard_monitor", "Clipboard Monitoring", true, true, None::<&str>)?;
            let autostart = CheckMenuItem::with_id(app, "autostart", "Start with Windows", true, autostart_enabled, None::<&str>)?;
            let menu = Menu::with_items(app, &[&capture, &colorpicker, &history, &desktop_guardian, &restore_layout, &clipboard_monitor, &autostart, &quit])?;

            // Load icon
            let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))
                .unwrap_or_else(|_| Image::from_bytes(include_bytes!("../icons/32x32.png")).unwrap());

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("Madera.Tools")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        // Set flag to allow exit
                        let state = app.state::<AppState>();
                        if let Ok(mut should_exit) = state.should_exit.lock() {
                            *should_exit = true;
                        }
                        app.exit(0);
                    }
                    "capture" => {
                        let _ = open_selection_window(app);
                    }
                    "colorpicker" => {
                        let _ = open_color_picker_window(app);
                    }
                    "history" => {
                        let _ = open_history_window(app);
                    }
                    "autostart" => {
                        use tauri_plugin_autostart::ManagerExt;
                        let autostart_manager = app.autolaunch();
                        let is_enabled = autostart_manager.is_enabled().unwrap_or(false);

                        if is_enabled {
                            let _ = autostart_manager.disable();
                        } else {
                            let _ = autostart_manager.enable();
                        }
                    }
                    "clipboard_monitor" => {
                        let state = app.state::<AppState>();
                        let mut settings = state.clipboard_settings.lock().unwrap();
                        settings.enabled = !settings.enabled;
                        state.clipboard_monitor.update_settings(settings.clone());
                    }
                    "desktop_guardian" => {
                        let _ = open_desktop_guardian_window(app);
                    }
                    "restore_layout" => {
                        // Emit event to trigger restore from frontend
                        let _ = app.emit("restore-last-layout", ());
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Handle left click on tray icon - open History panel
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = open_history_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // Start sleep/wake monitor
            let app_handle_sleep = app.handle().clone();
            std::thread::spawn(move || {
                monitor_system_power_events(app_handle_sleep);
            });

            // Register global shortcut for capture (Ctrl+Shift+S)
            let capture_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyS);
            let app_handle_capture = app.handle().clone();

            app.global_shortcut().on_shortcut(capture_shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let _ = open_selection_window(&app_handle_capture);
                }
            })?;

            // Register global shortcut for copy (Ctrl+P)
            let copy_shortcut = Shortcut::new(Some(Modifiers::CONTROL), Code::KeyP);
            let app_handle_copy = app.handle().clone();

            app.global_shortcut().on_shortcut(copy_shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    // Check if editor window exists and emit copy event to it
                    if let Some(editor_window) = app_handle_copy.get_webview_window("editor") {
                        let _ = editor_window.emit("global-copy", ());
                    }
                }
            })?;

            // Register global shortcut for history (Ctrl+Shift+V)
            let history_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyV);
            let app_handle_history = app.handle().clone();

            app.global_shortcut().on_shortcut(history_shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let _ = open_history_window(&app_handle_history);
                }
            })?;

            // Register global shortcut for color picker (Ctrl+Shift+X)
            let colorpicker_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyX);
            let app_handle_colorpicker = app.handle().clone();

            app.global_shortcut().on_shortcut(colorpicker_shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let _ = open_color_picker_window(&app_handle_colorpicker);
                }
            })?;

            // Start clipboard monitoring
            let app_handle_for_clipboard = app.handle().clone();
            let state = app.state::<AppState>();

            let _ = state.clipboard_monitor.start({
                let app = app_handle_for_clipboard.clone();
                move |content| {
                    let app = app.clone();
                    std::thread::spawn(move || {
                        let state = app.state::<AppState>();
                        let clipboard_settings = match state.clipboard_settings.lock() {
                            Ok(s) => s.clone(),
                            Err(_) => return,
                        };

                        if !clipboard_settings.enabled {
                            return;
                        }

                        let mut manager = match state.history_manager.lock() {
                            Ok(m) => m,
                            Err(_) => return,
                        };

                        match content {
                            ClipboardContent::Text(text) => {
                                // Check for duplicates
                                if let Ok(Some(last)) = manager.get_last_clipboard_hash() {
                                    if last == text {
                                        return;
                                    }
                                }

                                let _ = manager.save_clipboard_text(
                                    &text,
                                    None,
                                    clipboard_settings.max_items,
                                );

                                let _ = app.emit("clipboard-changed", "text");
                            }
                            ClipboardContent::Image { data, width, height } => {
                                if let Some(base64) =
                                    ClipboardMonitor::image_to_base64(&data, width, height)
                                {
                                    let _ = manager.save_clipboard_image(
                                        &base64,
                                        width as u32,
                                        height as u32,
                                        None,
                                        clipboard_settings.max_items,
                                    );

                                    let _ = app.emit("clipboard-changed", "image");
                                }
                            }
                            ClipboardContent::Empty => {}
                        }
                    });
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_monitors,
            capture_all_screens,
            capture_region,
            copy_to_clipboard,
            save_to_history,
            get_history,
            get_screenshot_by_id,
            delete_screenshot,
            clear_history,
            get_settings,
            update_settings,
            get_pending_capture,
            open_editor_with_image,
            close_window,
            trigger_capture,
            save_image_to_file,
            resize_image,
            get_default_save_path,
            toggle_autostart,
            is_autostart_enabled,
            // Unified history commands
            get_unified_history,
            get_history_item_image,
            delete_history_item,
            toggle_pin_item,
            search_history,
            clear_unified_history,
            copy_text_to_clipboard,
            // Clipboard monitoring commands
            get_clipboard_settings,
            update_clipboard_settings,
            is_clipboard_monitoring,
            start_clipboard_monitoring,
            stop_clipboard_monitoring,
            // Color picker commands
            get_pixel_color,
            get_magnifier_region,
            save_color_pick,
            get_color_settings,
            update_color_settings,
            trigger_color_picker,
            // Panel commands
            open_history_panel,
            // Desktop Guardian commands
            get_desktop_monitors,
            get_current_window_layout,
            save_window_layout,
            restore_window_layout,
            open_desktop_guardian,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Keep the app running in the system tray when all windows are closed
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                // Check if we should really exit
                let state = app_handle.state::<AppState>();
                let should_exit = state.should_exit.lock().map(|v| *v).unwrap_or(false);

                if !should_exit {
                    api.prevent_exit();
                }
            }
        });
}
