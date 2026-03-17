mod capture;
mod clipboard;
mod clipboard_monitor;
mod color_picker;
mod history;
mod native_selection;
mod snippet_manager;
mod ssh_uploader;
mod window_layout;

use capture::CaptureManager;
use clipboard::ClipboardManager;
use clipboard_monitor::{ClipboardContent, ClipboardMonitor, ClipboardSettings};
use color_picker::{ColorFormat, ColorInfo, ColorPickSettings};
use history::{HistoryItem, HistoryItemType, HistoryManager, ScreenshotRecord};
use snippet_manager::{SnippetItem, SnippetManager};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::sync::mpsc::Sender;

// Global flag to track if system is sleeping (for auto-save)
static IS_SYSTEM_SLEEPING: AtomicBool = AtomicBool::new(false);
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Apply wlr-layer-shell overlay to a Tauri window (Wayland always-on-top).
/// Must be called BEFORE the GTK window is realized (i.e. before .show()).
/// Runs on the main thread as required by GTK.
#[cfg(target_os = "linux")]
fn apply_layer_shell_overlay(window: &tauri::WebviewWindow, width: i32) {
    let window_clone = window.clone();
    let _ = window.run_on_main_thread(move || {
        if let Ok(gtk_win) = window_clone.gtk_window() {
            use gtk::prelude::*;
            use gtk_layer_shell::LayerShell;
            let w = gtk_win.upcast_ref::<gtk::Window>();
            w.init_layer_shell();
            w.set_layer(gtk_layer_shell::Layer::Overlay);
            w.set_anchor(gtk_layer_shell::Edge::Top, true);
            w.set_anchor(gtk_layer_shell::Edge::Bottom, true);
            w.set_anchor(gtk_layer_shell::Edge::Right, true);
            w.set_anchor(gtk_layer_shell::Edge::Left, false);
            w.set_namespace("madera-tools-overlay");
            w.set_keyboard_interactivity(false);
            gtk_win.set_size_request(width, -1);
        }
    });
}

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
    pub last_paste_time: Mutex<Option<Instant>>,
    pub multi_paste_window_open: Mutex<bool>,
    pub auto_save_notifier: Mutex<Option<Sender<()>>>,
    pub snippet_manager: Arc<SnippetManager>,
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
    // SSH Upload settings (assumes SSH key already configured)
    pub ssh_enabled: bool,
    pub ssh_host: String,
    pub ssh_remote_path: String,
    #[serde(default)]
    pub ssh_passphrase: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Shift+S".to_string(),
            auto_copy: true,
            max_history: 150,
            max_image_width: Some(1568), // Optimal for Claude
            ssh_enabled: true,
            ssh_host: "mad@192.168.2.71".to_string(),
            ssh_remote_path: "/home/mad/.claude/downloads".to_string(),
            ssh_passphrase: String::new(),
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

#[tauri::command]
async fn upload_to_dev_server(
    state: State<'_, AppState>,
    image_data: String,
) -> Result<String, String> {
    use base64::Engine;
    
    // Scoped lock to avoid holding it during long network operation
    let (enabled, host, remote_path, passphrase) = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        (settings.ssh_enabled, settings.ssh_host.clone(), settings.ssh_remote_path.clone(), settings.ssh_passphrase.clone())
    };
    
    if !enabled {
        return Err("SSH upload not enabled".to_string());
    }
    
    if host.is_empty() {
        return Err("SSH host not configured".to_string());
    }

    if remote_path.is_empty() {
        return Err("SSH remote path not configured".to_string());
    }

    // Generate filename with timestamp
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("screenshot_{}.png", timestamp);
    
    // Build full remote path - ensure no double slashes if user added trailing slash
    let base_path = remote_path.trim_end_matches('/');
    let full_remote_path = format!("{}/{}", base_path, filename);
    
    // Decode image
    let data = base64::engine::general_purpose::STANDARD
        .decode(&image_data)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;
    
    // Upload via SSH (uses system's default SSH key)
    let uploader = ssh_uploader::SshUploader::new(host.clone());
    
    uploader.upload_file(&data, &full_remote_path, &passphrase)
        .map_err(|e| format!("SSH upload failed: {}", e))?;
    
    // Copy full path to clipboard
    {
        let mut clipboard_manager = state.clipboard_manager.lock().map_err(|e| e.to_string())?;
        clipboard_manager.copy_text_to_clipboard(&full_remote_path)
            .map_err(|e| e.to_string())?;
    }
    
    Ok(full_remote_path)
}

// Monitor system power events (sleep/wake)
#[cfg(windows)]
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
        const PBT_APMSUSPEND: u32 = 4; // System is about to sleep
        const PBT_APMRESUMEAUTOMATIC: u32 = 18;
        const PBT_APMRESUMESUSPEND: u32 = 7;
        const PBT_POWERSETTINGCHANGE: u32 = 0x8013;

        if msg == WM_POWERBROADCAST {
            let event = wparam.0 as u32;
            let app_ptr = APP_HANDLE.load(Ordering::SeqCst);

            // System is ABOUT TO SLEEP - block auto-saves immediately
            if event == PBT_APMSUSPEND {
                IS_SYSTEM_SLEEPING.store(true, Ordering::SeqCst);
                println!("[Desktop Guardian] System going to sleep - blocking auto-saves!");
                if !app_ptr.is_null() {
                    let app = &*app_ptr;
                    let _ = app.emit("system-going-to-sleep", ());
                }
            }

            // Handle resume events
            if event == PBT_APMRESUMEAUTOMATIC || event == PBT_APMRESUMESUSPEND || event == PBT_POWERSETTINGCHANGE {
                if !app_ptr.is_null() {
                    let app = &*app_ptr;
                    let app_clone = app.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        IS_SYSTEM_SLEEPING.store(false, Ordering::SeqCst);
                        
                        // Check if Desktop Guardian is enabled before doing anything
                        let guardian_enabled = app_clone.path().app_config_dir()
                            .ok()
                            .and_then(|dir| std::fs::read_to_string(dir.join("settings.json")).ok())
                            .and_then(|content| serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(&content).ok())
                            .and_then(|data| data.get("guardian_settings").cloned())
                            .and_then(|v| v.get("autoSaveEnabled").and_then(|v| v.as_bool()))
                            .unwrap_or(true);
                        
                        if !guardian_enabled {
                            println!("[Desktop Guardian] Disabled - skipping wake actions");
                            return;
                        }
                        
                        let _ = app_clone.emit("system-wake-from-sleep", ());
                        println!("[Desktop Guardian] Wake from sleep detected - resuming auto-saves!");

                        // Signal the auto-save thread to wake up immediately
                        {
                            let state = app_clone.state::<AppState>();
                            let guard_result = state.auto_save_notifier.lock();
                            if let Ok(notifier) = guard_result {
                                if let Some(tx) = notifier.as_ref() {
                                    let _ = tx.send(());
                                }
                            }
                        };
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

#[cfg(not(windows))]
fn monitor_system_power_events(_app: AppHandle) {
    // Power event monitoring is Windows-only; no-op on other platforms.
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
    state.clipboard_monitor.pause();

    let res = (|| -> Result<(), String> {
        let manager = state.clipboard_manager.lock().map_err(|e| e.to_string())?;
        manager
            .copy_image_to_clipboard(&image_data)
            .map_err(|e| e.to_string())
    })();

    state.clipboard_monitor.skip_next_change();
    state.clipboard_monitor.resume();
    res
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
async fn update_settings(state: State<'_, AppState>, settings: AppSettings, app: AppHandle) -> Result<(), String> {
    let mut current = state.settings.lock().map_err(|e| e.to_string())?;
    *current = settings.clone();
    
    // Persist to file
    save_settings_to_file(&app, &settings)?;
    
    Ok(())
}

fn get_settings_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|p| p.join("settings.json"))
}

fn load_settings_from_file(app: &AppHandle) -> Result<AppSettings, String> {
    let path = get_settings_path(app).ok_or("Failed to get config dir")?;
    
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    // Read full settings object (HashMap)
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    
    // Extract "app_settings" field if it exists, otherwise try to parse the whole object (legacy)
    // For now, let's store it under "app_settings" key in the consolidated settings.json
    // But since we are using a single file for everything (auto-save layouts are there too),
    // we need to be careful not to overwrite other keys.
    
    if let Some(val) = json.get("app_settings") {
        serde_json::from_value(val.clone()).map_err(|e| e.to_string())
    } else {
        Ok(AppSettings::default())
    }
}

fn save_settings_to_file(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    use std::fs;
    let path = get_settings_path(app).ok_or("Failed to get config dir")?;
    
    // Ensure dir exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // Load existing JSON to preserve other keys (like desktop_layouts)
    let mut json_obj: serde_json::Map<String, serde_json::Value> = if path.exists() {
        let content = fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
        serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    } else {
        serde_json::Map::new()
    };
    
    // Update app_settings
    json_obj.insert(
        "app_settings".to_string(), 
        serde_json::to_value(settings).map_err(|e| e.to_string())?
    );
    
    let json_str = serde_json::to_string_pretty(&json_obj).map_err(|e| e.to_string())?;
    fs::write(path, json_str).map_err(|e| e.to_string())?;
    
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
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<HistoryItem>, String> {
    let manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    let filter = filter_type.and_then(|t| t.parse::<HistoryItemType>().ok());
    manager
        .get_all_history_items(filter, limit, offset)
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

/// Get the system idle time in seconds (time since last user input)
#[tauri::command]
async fn get_idle_time() -> Result<u64, String> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

        unsafe {
            let mut last_input = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };

            if GetLastInputInfo(&mut last_input).as_bool() {
                let tick_count = windows::Win32::System::SystemInformation::GetTickCount();
                let idle_ms = tick_count.saturating_sub(last_input.dwTime);
                Ok((idle_ms / 1000) as u64)
            } else {
                Err("Failed to get last input info".to_string())
            }
        }
    }

    #[cfg(not(windows))]
    {
        Ok(0)
    }
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

#[tauri::command]
async fn ensure_current_window_visible(window: tauri::Window) -> Result<(), String> {
    if window.is_minimized().unwrap_or(false) {
        window.unminimize().map_err(|e| e.to_string())?;
    }
    window.center().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
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

    // WINDOW LIMIT: Close oldest editor if we have 5 or more
    let editor_windows: Vec<String> = app.webview_windows()
        .iter()
        .filter_map(|(label, _)| {
            if label.starts_with("editor-") {
                Some(label.clone())
            } else {
                None
            }
        })
        .collect();

    if editor_windows.len() >= 5 {
        // Sort by timestamp (extract from label: editor-{timestamp})
        let mut sorted_windows = editor_windows.clone();
        sorted_windows.sort_by_key(|label| {
            label.strip_prefix("editor-")
                .and_then(|ts| ts.parse::<u128>().ok())
                .unwrap_or(0)
        });
        
        // Close the oldest one
        if let Some(oldest) = sorted_windows.first() {
            if let Some(window) = app.get_webview_window(oldest) {
                let _ = window.close();
                println!("[Window Limit] Closed oldest editor: {}", oldest);
            }
        }
    }

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

    // Get Work Area (Screen - Taskbar)
    let (work_x, work_y, work_w, work_h) = {
        #[cfg(windows)]
        {
            use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
            use windows::Win32::Graphics::Gdi::{MonitorFromPoint, GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTONEAREST};
            use windows::Win32::Foundation::POINT;

            unsafe {
                let mut point = POINT::default();
                let _ = GetCursorPos(&mut point);

                let hmonitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
                let mut monitor_info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };

                if GetMonitorInfoW(hmonitor, &mut monitor_info).as_bool() {
                    let rc = monitor_info.rcWork;
                    (rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top)
                } else {
                    (0, 0, 1920, 1080)
                }
            }
        }
        #[cfg(not(windows))]
        {
            // Use xcap to get primary monitor dimensions
            if let Ok(monitors) = xcap::Monitor::all() {
                if let Some(m) = monitors.iter().find(|m| m.is_primary()).or(monitors.first()) {
                    (m.x(), m.y(), m.width() as i32, m.height() as i32)
                } else {
                    (0, 0, 1920, 1080)
                }
            } else {
                (0, 0, 1920, 1080)
            }
        }
    };

    // Calculate window size (image size + toolbar space)
    let min_width = 900.0;
    let min_height = 400.0;
    
    // Constrain max dimensions to the work area (minus a small margin of 20px)
    let max_w = (work_w as f64 - 20.0).max(min_width);
    let max_h = (work_h as f64 - 20.0).max(min_height);

    let window_width = ((width + 40) as f64).max(min_width).min(max_w);
    let window_height = ((height + 160) as f64).max(min_height).min(max_h);

    let window = WebviewWindowBuilder::new(app, &window_id, WebviewUrl::App("index.html#editor".into()))
        .title("Screenshot Editor")
        .inner_size(window_width, window_height)
        .min_inner_size(min_width, min_height)
        .resizable(true)
        .decorations(true)
        .build()?;

    // Manually center to ensure we are in the work area
    // (native .center() sometimes considers full screen, not work area)
    let pos_x = work_x as f64 + (work_w as f64 - window_width) / 2.0;
    let pos_y = work_y as f64 + (work_h as f64 - window_height) / 2.0;
    
    window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
        x: pos_x as i32,
        y: pos_y as i32,
    }))?;

    window.set_focus()?;

    Ok(())
}

fn open_main_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("main") {
        window.set_focus()?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
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

    let window = WebviewWindowBuilder::new(app, "history", WebviewUrl::App("index.html#history".into()))
        .title("Madera.Tools - History")
        .inner_size(1200.0, 800.0)
        .center()
        .resizable(true)
        .decorations(true)
        .build()?;

    window.set_focus()?;
    Ok(())
}

fn open_settings_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("settings") {
        window.set_focus()?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html#settings".into()))
        .title("Settings")
        .inner_size(900.0, 700.0)
        .min_inner_size(700.0, 500.0)
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
    let window = WebviewWindowBuilder::new(app, "colorpicker", WebviewUrl::App("index.html#colorpicker".into()))
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

#[tauri::command]
async fn open_settings_panel(app: AppHandle) -> Result<(), String> {
    open_settings_window(&app).map_err(|e| e.to_string())
}

fn open_multi_paste_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Save the current foreground window BEFORE opening popup
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        unsafe {
            let hwnd = GetForegroundWindow();
            PREVIOUS_FOREGROUND_WINDOW.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
        }
    }

    // Close existing if any
    if let Some(window) = app.get_webview_window("multipaste") {
        window.close()?;
    }

    // Get cursor position and monitor info
    let (cursor_x, cursor_y, monitor_x, monitor_y, monitor_w, monitor_h) = get_cursor_and_monitor_info();
    let window_height = 500.0;
    let window_width = 400.0;

    // Position window above cursor, centered horizontally, but clamp to monitor bounds
    let mut pos_x = cursor_x as f64 - window_width / 2.0;
    let mut pos_y = cursor_y as f64 - window_height - 20.0; // 20px gap above cursor

    // Clamp to monitor bounds
    pos_x = pos_x.max(monitor_x as f64).min((monitor_x + monitor_w) as f64 - window_width);
    pos_y = pos_y.max(monitor_y as f64).min((monitor_y + monitor_h) as f64 - window_height);

    // If cursor is near top of screen, show below cursor instead
    if pos_y < monitor_y as f64 + 50.0 {
        pos_y = cursor_y as f64 + 20.0;
    }

    let window = WebviewWindowBuilder::new(app, "multipaste", WebviewUrl::App("index.html#multipaste".into()))
        .title("Quick Paste")
        .inner_size(window_width, window_height)
        .position(pos_x, pos_y)
        .resizable(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;

    #[cfg(target_os = "linux")]
    apply_layer_shell_overlay(&window, 400);

    window.show().map_err(|e| e.to_string())?;
    window.set_focus()?;

    // Mark window as open
    let state = app.state::<AppState>();
    if let Ok(mut open) = state.multi_paste_window_open.lock() {
        *open = true;
    }

    Ok(())
}

#[tauri::command]
async fn open_quick_paste_panel(app: tauri::AppHandle) -> Result<(), String> {
    open_quick_paste_window(&app)
}

/// Toggle layer-shell anchor between left and right for a given window
#[cfg(target_os = "linux")]
#[tauri::command]
async fn toggle_panel_side(app: AppHandle, window_label: String) -> Result<String, String> {
    let window = app.get_webview_window(&window_label)
        .ok_or("Window not found")?;
    let win = window.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    window.run_on_main_thread(move || {
        if let Ok(gtk_win) = win.gtk_window() {
            use gtk::prelude::*;
            use gtk_layer_shell::LayerShell;
            let w = gtk_win.upcast_ref::<gtk::Window>();
            let is_right = w.is_anchor(gtk_layer_shell::Edge::Right);
            w.set_anchor(gtk_layer_shell::Edge::Right, !is_right);
            w.set_anchor(gtk_layer_shell::Edge::Left, is_right);
            let _ = tx.send(if is_right { "left" } else { "right" });
        }
    }).map_err(|e| e.to_string())?;
    rx.recv().map(|s| s.to_string()).map_err(|e| e.to_string())
}

/// Cycle layer-shell monitor for a given window
#[cfg(target_os = "linux")]
#[tauri::command]
async fn toggle_panel_monitor(app: AppHandle, window_label: String) -> Result<(), String> {
    let window = app.get_webview_window(&window_label)
        .ok_or("Window not found")?;
    let win = window.clone();
    window.run_on_main_thread(move || {
        if let Ok(gtk_win) = win.gtk_window() {
            use gtk::prelude::*;
            use gtk_layer_shell::LayerShell;
            let w = gtk_win.upcast_ref::<gtk::Window>();
            let display = match gdk::Display::default() {
                Some(d) => d,
                None => return,
            };
            let n = display.n_monitors();
            if n <= 1 { return; }
            let current = w.monitor();
            let mut next_idx: i32 = 0;
            if let Some(ref cur) = current {
                for i in 0..n {
                    if display.monitor(i).as_ref() == Some(cur) {
                        next_idx = (i + 1) % n;
                        break;
                    }
                }
            }
            if let Some(next_mon) = display.monitor(next_idx) {
                w.set_monitor(&next_mon);
            }
        }
    }).map_err(|e| e.to_string())?;
    Ok(())
}

fn open_quick_paste_window(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("quickpaste") {
        window.close().map_err(|e| e.to_string())?;
    }

    let (cursor_x, _cursor_y, monitor_x, monitor_y, monitor_w, monitor_h) = get_cursor_and_monitor_info();
    let window_height = monitor_h as f64;
    let window_width = 400.0;

    let mut pos_x = cursor_x as f64 - window_width / 2.0;
    pos_x = pos_x.max(monitor_x as f64).min((monitor_x + monitor_w) as f64 - window_width);
    let pos_y = monitor_y as f64;

    let window = WebviewWindowBuilder::new(app, "quickpaste", WebviewUrl::App("index.html#quickpaste".into()))
        .title("Quick Prompt Snippets")
        .inner_size(window_width, window_height)
        .position(pos_x, pos_y)
        .resizable(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build().map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    apply_layer_shell_overlay(&window, 400);

    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_snippets(state: State<'_, AppState>) -> Result<Vec<SnippetItem>, String> {
    Ok(state.snippet_manager.get_all())
}

#[tauri::command]
async fn add_snippet(state: State<'_, AppState>, title: String, content_type: String, content: String) -> Result<SnippetItem, String> {
    Ok(state.snippet_manager.add(title, content_type, content))
}

#[tauri::command]
async fn delete_snippet(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    Ok(state.snippet_manager.delete(&id))
}

#[tauri::command]
async fn update_snippet(state: State<'_, AppState>, id: String, title: String, content: String) -> Result<bool, String> {
    Ok(state.snippet_manager.update(&id, title, content))
}

#[tauri::command]
async fn paste_snippet_item(app: AppHandle, item_id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    
    let snippet = {
        let items = state.snippet_manager.get_all();
        items.into_iter().find(|i| i.id == item_id).ok_or("Snippet not found")?
    };

    state.clipboard_monitor.pause();

    let copy_result = (|| -> Result<(), String> {
        let clipboard_manager = state.clipboard_manager.lock().map_err(|e| e.to_string())?;

        if snippet.content_type == "text" {
            clipboard_manager.copy_text_to_clipboard(&snippet.content).map_err(|e| e.to_string())?;
        } else {
            clipboard_manager.copy_image_to_clipboard(&snippet.content).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();

    state.clipboard_monitor.skip_next_change();
    state.clipboard_monitor.resume();
    copy_result?;

    let content = snippet.content.clone();
    let is_text = snippet.content_type == "text";
    std::thread::spawn(move || {
        #[cfg(windows)]
        {
            use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
            use windows::Win32::Foundation::HWND;

            let hwnd_val = PREVIOUS_FOREGROUND_WINDOW.load(std::sync::atomic::Ordering::Relaxed);
            if hwnd_val != 0 {
                unsafe {
                    let hwnd = HWND(hwnd_val as *mut _);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
        #[cfg(not(windows))]
        if is_text {
            type_text(&content);
        } else {
            simulate_paste();
        }
        #[cfg(windows)]
        simulate_paste();
    });

    Ok(())
}

#[tauri::command]
async fn copy_snippet_to_clipboard(app: AppHandle, item_id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    
    let snippet = {
        let items = state.snippet_manager.get_all();
        items.into_iter().find(|i| i.id == item_id).ok_or("Snippet not found")?
    };

    state.clipboard_monitor.pause();

    let copy_result = (|| -> Result<(), String> {
        let clipboard_manager = state.clipboard_manager.lock().map_err(|e| e.to_string())?;

        if snippet.content_type == "text" {
            clipboard_manager.copy_text_to_clipboard(&snippet.content).map_err(|e| e.to_string())?;
        } else {
            clipboard_manager.copy_image_to_clipboard(&snippet.content).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();

    state.clipboard_monitor.skip_next_change();
    state.clipboard_monitor.resume();
    copy_result?;

    Ok(())
}

fn get_cursor_and_monitor_info() -> (i32, i32, i32, i32, i32, i32) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
        use windows::Win32::Graphics::Gdi::{MonitorFromPoint, GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTONEAREST};
        use windows::Win32::Foundation::POINT;

        unsafe {
            let mut point = POINT::default();
            if GetCursorPos(&mut point).is_ok() {
                let hmonitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
                let mut monitor_info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };

                if GetMonitorInfoW(hmonitor, &mut monitor_info).as_bool() {
                    let rc = monitor_info.rcMonitor;
                    return (
                        point.x,
                        point.y,
                        rc.left,
                        rc.top,
                        rc.right - rc.left,
                        rc.bottom - rc.top,
                    );
                }

                (point.x, point.y, 0, 0, 1920, 1080)
            } else {
                (100, 100, 0, 0, 1920, 1080)
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Use X11 to get cursor position, fall back to xcap for monitor info
        let (cursor_x, cursor_y) = unsafe {
            let display = x11::xlib::XOpenDisplay(std::ptr::null());
            if display.is_null() {
                (100, 100)
            } else {
                let screen = x11::xlib::XDefaultScreen(display);
                let root = x11::xlib::XRootWindow(display, screen);
                let mut root_return = 0u64;
                let mut child_return = 0u64;
                let mut root_x = 0i32;
                let mut root_y = 0i32;
                let mut win_x = 0i32;
                let mut win_y = 0i32;
                let mut mask = 0u32;
                let result = x11::xlib::XQueryPointer(
                    display, root,
                    &mut root_return, &mut child_return,
                    &mut root_x, &mut root_y,
                    &mut win_x, &mut win_y, &mut mask,
                );
                x11::xlib::XCloseDisplay(display);
                if result != 0 {
                    (root_x, root_y)
                } else {
                    (100, 100)
                }
            }
        };

        // Try to get monitor info from xcap
        if let Ok(monitors) = xcap::Monitor::all() {
            for m in &monitors {
                let mx = m.x();
                let my = m.y();
                let mw = m.width() as i32;
                let mh = m.height() as i32;
                if cursor_x >= mx && cursor_x < mx + mw && cursor_y >= my && cursor_y < my + mh {
                    return (cursor_x, cursor_y, mx, my, mw, mh);
                }
            }
            // Fallback to first monitor
            if let Some(m) = monitors.first() {
                return (cursor_x, cursor_y, m.x(), m.y(), m.width() as i32, m.height() as i32);
            }
        }

        (cursor_x, cursor_y, 0, 0, 1920, 1080)
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        (100, 100, 0, 0, 1920, 1080)
    }
}

#[tauri::command]
async fn open_multi_paste_panel(app: AppHandle) -> Result<(), String> {
    open_multi_paste_window(&app).map_err(|e| e.to_string())
}

#[tauri::command]
async fn close_multi_paste(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("multipaste") {
        window.close().map_err(|e| e.to_string())?;
    }
    let state = app.state::<AppState>();
    if let Ok(mut open) = state.multi_paste_window_open.lock() {
        *open = false;
    }
    Ok(())
}

#[tauri::command]
async fn paste_history_item(app: AppHandle, item_id: String) -> Result<(), String> {
    let state = app.state::<AppState>();

    // Get the item from history
    let history_manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    let item = history_manager.get_history_item(&item_id).map_err(|e| e.to_string())?;
    drop(history_manager);

    let item = item.ok_or("Item not found")?;

    state.clipboard_monitor.pause();

    let copy_result = (|| -> Result<(), String> {
        let clipboard_manager = state.clipboard_manager.lock().map_err(|e| e.to_string())?;

        match item.item_type {
            HistoryItemType::ClipboardText => {
                if let Some(text) = &item.text_content {
                    clipboard_manager.copy_text_to_clipboard(text).map_err(|e| e.to_string())?;
                }
            }
            HistoryItemType::ClipboardImage | HistoryItemType::Screenshot => {
                if let Some(filename) = &item.filename {
                    let history_manager = state.history_manager.lock().map_err(|e| e.to_string())?;
                    let image_base64 = history_manager.load_image_base64(filename, item.item_type.clone()).map_err(|e| e.to_string())?;
                    drop(history_manager);
                    clipboard_manager.copy_image_to_clipboard(&image_base64).map_err(|e| e.to_string())?;
                }
            }
            HistoryItemType::ColorPick => {
                if let Some(hex) = &item.color_hex {
                    clipboard_manager.copy_text_to_clipboard(hex).map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    })();

    state.clipboard_monitor.skip_next_change();
    state.clipboard_monitor.resume();
    copy_result?;

    // Auto-paste: restore focus to previous window and simulate Ctrl+V
    // Keep popup open so user can paste multiple items
    std::thread::spawn(|| {
        #[cfg(windows)]
        {
            use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
            use windows::Win32::Foundation::HWND;

            let hwnd_val = PREVIOUS_FOREGROUND_WINDOW.load(std::sync::atomic::Ordering::Relaxed);

            if hwnd_val != 0 {
                unsafe {
                    let hwnd = HWND(hwnd_val as *mut _);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
        simulate_paste();
    });

    Ok(())
}

#[tauri::command]
async fn copy_history_item_to_clipboard(app: AppHandle, item_id: String) -> Result<(), String> {
    let state = app.state::<AppState>();

    // Get the item from history
    let history_manager = state.history_manager.lock().map_err(|e| e.to_string())?;
    let item = history_manager.get_history_item(&item_id).map_err(|e| e.to_string())?;
    drop(history_manager);

    let item = item.ok_or("Item not found")?;

    state.clipboard_monitor.pause();

    let copy_result = (|| -> Result<(), String> {
        let clipboard_manager = state.clipboard_manager.lock().map_err(|e| e.to_string())?;

        match item.item_type {
            HistoryItemType::ClipboardText => {
                if let Some(text) = &item.text_content {
                    clipboard_manager.copy_text_to_clipboard(text).map_err(|e| e.to_string())?;
                }
            }
            HistoryItemType::ClipboardImage | HistoryItemType::Screenshot => {
                if let Some(filename) = &item.filename {
                    let history_manager = state.history_manager.lock().map_err(|e| e.to_string())?;
                    let image_base64 = history_manager.load_image_base64(filename, item.item_type.clone()).map_err(|e| e.to_string())?;
                    drop(history_manager);
                    clipboard_manager.copy_image_to_clipboard(&image_base64).map_err(|e| e.to_string())?;
                }
            }
            HistoryItemType::ColorPick => {
                if let Some(hex) = &item.color_hex {
                    clipboard_manager.copy_text_to_clipboard(hex).map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    })();

    state.clipboard_monitor.skip_next_change();
    state.clipboard_monitor.resume();
    copy_result?;

    Ok(())
}

// Global state for keyboard hook - using atomics for lock-free performance
#[cfg(windows)]
static PASTE_HOOK_APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();
// Store last V press time as milliseconds (0 = never pressed)
#[cfg(windows)]
static LAST_V_PRESS_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(windows)]
static CTRL_HELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[allow(dead_code)]
static PREVIOUS_FOREGROUND_WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

#[cfg(windows)]
fn start_paste_hook(app: AppHandle) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowsHookExW, CallNextHookEx, GetMessageW, WH_KEYBOARD_LL, KBDLLHOOKSTRUCT, MSG,
        GetForegroundWindow, LLKHF_INJECTED,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_V, VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_RETURN, VK_SHIFT};
    use windows::Win32::Foundation::{WPARAM, LPARAM, LRESULT};

    let _ = PASTE_HOOK_APP.set(app);

    std::thread::spawn(|| {
        unsafe extern "system" fn keyboard_hook(
            code: i32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            use windows::Win32::UI::WindowsAndMessaging::{CallNextHookEx, HHOOK, HC_ACTION, GetForegroundWindow};

            if code == HC_ACTION as i32 {
                let kb = *(lparam.0 as *const KBDLLHOOKSTRUCT);
                let vk = kb.vkCode as u16;
                let key_down = wparam.0 == 0x100 || wparam.0 == 0x104; // WM_KEYDOWN or WM_SYSKEYDOWN
                let _key_up = wparam.0 == 0x101 || wparam.0 == 0x105; // WM_KEYUP or WM_SYSKEYUP

                // Track Ctrl key state
                if vk == VK_CONTROL.0 || vk == VK_LCONTROL.0 || vk == VK_RCONTROL.0 {
                    CTRL_HELD.store(key_down, std::sync::atomic::Ordering::SeqCst);
                }

                // Detect double-tap of Ctrl+V (two presses within 500ms)
                // Using lock-free atomics for performance in low-level hook
                if vk == VK_V.0 && key_down && CTRL_HELD.load(std::sync::atomic::Ordering::Relaxed) {
                    use std::sync::atomic::Ordering::Relaxed;

                    // Get current time in ms (using system time, wraps every ~584 million years)
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);

                    let last_ms = LAST_V_PRESS_MS.swap(now_ms, Relaxed);

                    // Check if double-tap (within 500ms and not first press)
                    let should_open = last_ms > 0 && now_ms.saturating_sub(last_ms) < 500;

                    if should_open {
                        // Reset to prevent triple-tap
                        LAST_V_PRESS_MS.store(0, Relaxed);

                        if let Some(app) = PASTE_HOOK_APP.get() {
                            // Use try_lock to avoid blocking - skip if can't get lock
                            let state = app.state::<AppState>();
                            let is_open = state.multi_paste_window_open.try_lock()
                                .map(|v| *v)
                                .unwrap_or(true); // Assume open if can't lock (safer)

                            if !is_open {
                                let _ = open_multi_paste_window(app);
                                // Block the second V keypress so it doesn't paste
                                return LRESULT(1);
                            }
                        }
                    }
                }

                // --- TABBY TERMINAL MACRO ---
                // Detect Shift+Enter
                let is_injected = (kb.flags.0 & LLKHF_INJECTED.0) != 0;
                
                if vk == VK_RETURN.0 && key_down {
                    let shift_down = unsafe { 
                        windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(VK_SHIFT.0 as i32) 
                    } as u16 & 0x8000 != 0;

                    if shift_down {
                        
                        if !is_injected {
                            let hwnd = GetForegroundWindow();
                            if !hwnd.0.is_null() {
                                use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, PROCESS_NAME_WIN32};
                                use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
                                
                                let mut pid = 0;
                                unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
                                
                                if pid != 0 {
                                    if let Ok(process_handle) = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
                                        let mut buffer = [0u16; 1024];
                                        let mut size = buffer.len() as u32;
                                        
                                        if unsafe { QueryFullProcessImageNameW(process_handle, PROCESS_NAME_WIN32, windows::core::PWSTR::from_raw(buffer.as_mut_ptr()), &mut size) }.is_ok() {
                                            let process_name = String::from_utf16_lossy(&buffer[..size as usize]);
                                            let name_lower = process_name.to_lowercase();
                                            
                                            if name_lower.ends_with("tabby.exe") || name_lower.contains("tabby") {
                                                let _ = unsafe { windows::Win32::Foundation::CloseHandle(process_handle) };
                                                
                                                std::thread::spawn(|| {
                                                    simulate_tabby_breakline();
                                                });
                                                
                                                return LRESULT(1);
                                            }
                                        }
                                        let _ = unsafe { windows::Win32::Foundation::CloseHandle(process_handle) };
                                    }
                                }
                            }
                        }
                    }
                }
                // --- END TABBY MACRO ---
            }

            // Pass the key to the next hook
            CallNextHookEx(HHOOK::default(), code, wparam, lparam)
        }

        unsafe {
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook),
                None,
                0,
            );

            if hook.is_ok() {
                // Message loop to keep the hook alive
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    // Process messages
                }
            }
        }
    });
}

#[cfg(not(windows))]
fn start_paste_hook(_app: AppHandle) {
    // Keyboard hooks are Windows-only; no-op on other platforms.
}

#[cfg(windows)]
fn simulate_paste() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
    };

    unsafe {
        let inputs = [
            // Press Ctrl
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Press V
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_V,
                        wScan: 0,
                        dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Release V
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_V,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Release Ctrl
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(not(windows))]
fn simulate_paste() {
    // Try wtype first (Wayland native), fall back to xdotool (X11)
    let wtype = std::process::Command::new("wtype")
        .args(["-M", "ctrl", "-P", "v", "-p", "v", "-m", "ctrl"])
        .spawn();
    if wtype.is_err() {
        let _ = std::process::Command::new("xdotool")
            .args(["key", "ctrl+v"])
            .spawn();
    }
}

#[cfg(not(windows))]
fn type_text(text: &str) {
    // Strip Windows \r line endings to avoid extra Enter keypresses
    let clean = text.replace('\r', "");
    let text = clean.as_str();
    // Type text directly via wtype (works on Wayland regardless of app's paste shortcut)
    let wtype = std::process::Command::new("wtype")
        .arg("--")
        .arg(text)
        .spawn();
    if wtype.is_err() {
        // Fallback: use xdotool type for X11
        let _ = std::process::Command::new("xdotool")
            .args(["type", "--clearmodifiers", "--", text])
            .spawn();
    }
}

#[cfg(windows)]
fn simulate_tabby_breakline() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_MENU, VK_RETURN,
    };

    unsafe {
        // We only need to send Alt+Enter since the user is already physically holding Shift.
        // The OS will combine the physical Shift with our simulated Alt+Enter.
        let inputs = [
            // Press Alt
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_MENU,
                        wScan: 0,
                        dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Press Enter
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_RETURN,
                        wScan: 0,
                        dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Release Enter
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_RETURN,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Release Alt
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_MENU,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn simulate_tabby_breakline() {
    // Tabby terminal macro is Windows-only; no-op on other platforms.
}

/// Settings for Desktop Guardian auto-save
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuardianSettings {
    #[serde(rename = "autoSaveEnabled")]
    auto_save_enabled: bool,
    #[serde(rename = "autoSaveInterval")]
    auto_save_interval: u64,
}

impl Default for GuardianSettings {
    fn default() -> Self {
        Self {
            auto_save_enabled: true,
            auto_save_interval: 5, // 5 minutes default
        }
    }
}

/// Perform auto-save of window layout (called from background thread)
/// Uses direct file I/O instead of store plugin for thread safety
fn perform_auto_save(app: &AppHandle) -> Result<(), String> {
    use std::fs;
    use std::io::Write;
    use std::collections::HashMap;

    // Helper to log to file for debugging
    fn log_to_file(app_data_dir: &std::path::Path, msg: &str) {
        let log_path = app_data_dir.join("autosave_debug.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] {}", timestamp, msg);
        }
    }

    // Get the app data directory
    let app_data_dir = app.path().app_config_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let settings_path = app_data_dir.join("settings.json");

    log_to_file(&app_data_dir, &format!("Settings path: {:?}", settings_path));
    println!("[Auto-save] Settings path: {:?}", settings_path);

    // Read existing settings file
    log_to_file(&app_data_dir, &format!("Settings exists: {}", settings_path.exists()));
    let mut settings_data: HashMap<String, serde_json::Value> = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)
            .map_err(|e| format!("Failed to read settings: {}", e))?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    };

    // Load guardian settings
    let guardian_settings: GuardianSettings = settings_data
        .get("guardian_settings")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    log_to_file(&app_data_dir, &format!("Guardian settings: enabled={}, interval={}",
        guardian_settings.auto_save_enabled, guardian_settings.auto_save_interval));

    // Check if auto-save is enabled
    if !guardian_settings.auto_save_enabled {
        log_to_file(&app_data_dir, "Auto-save disabled in settings, skipping");
        println!("[Auto-save] Disabled in settings, skipping");
        return Ok(());
    }

    // Get current windows
    let windows = window_layout::get_all_windows();
    log_to_file(&app_data_dir, &format!("Found {} windows", windows.len()));

    // Skip if no windows to save
    if windows.is_empty() {
        log_to_file(&app_data_dir, "No windows found, skipping");
        println!("[Auto-save] No windows found, skipping");
        return Ok(());
    }

    // Create new auto-save layout
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    let timestamp = chrono::Local::now().format("%b %d, %H:%M");

    let new_layout = window_layout::SavedLayout {
        id,
        name: format!("Auto-save {}", timestamp),
        created_at,
        windows,
        is_auto_save: true,
    };

    // Load existing layouts
    let mut layouts: Vec<window_layout::SavedLayout> = settings_data
        .get("desktop_layouts")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    // Remove old auto-saves (keep only last 4 so we can add the new one = 5 total)
    let mut auto_save_count = 0;
    layouts.retain(|l| {
        if l.is_auto_save {
            auto_save_count += 1;
            auto_save_count <= 4
        } else {
            true // Keep all manual saves
        }
    });

    // Add new layout at the beginning (most recent first)
    layouts.insert(0, new_layout);

    // Update layouts in settings
    settings_data.insert(
        "desktop_layouts".to_string(),
        serde_json::to_value(&layouts).map_err(|e| format!("Failed to serialize: {}", e))?,
    );

    // Write back to file
    let json_str = serde_json::to_string_pretty(&settings_data)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    log_to_file(&app_data_dir, &format!("Writing {} bytes to settings", json_str.len()));

    fs::write(&settings_path, json_str)
        .map_err(|e| format!("Failed to write settings: {}", e))?;

    log_to_file(&app_data_dir, &format!("SUCCESS: Saved layout with {} windows", layouts[0].windows.len()));
    println!("[Auto-save] Saved layout with {} windows", layouts[0].windows.len());

    // Emit event so frontend can refresh its layout list
    let _ = app.emit("layouts-updated", ());

    Ok(())
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
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // Check CLI arguments for action commands (used by COSMIC/Wayland shortcuts)
            let action = args.iter().find_map(|a| {
                match a.as_str() {
                    "--capture" => Some("capture"),
                    "--history" => Some("history"),
                    "--colorpicker" => Some("colorpicker"),
                    "--quickpaste" => Some("quickpaste"),
                    "--snippets" => Some("snippets"),
                    _ => None,
                }
            });

            match action {
                Some("capture") => { let _ = open_selection_window(app); },
                Some("history") => { let _ = open_history_window(app); },
                Some("colorpicker") => { let _ = open_color_picker_window(app); },
                Some("quickpaste") => { let _ = open_multi_paste_window(app); },
                Some("snippets") => { let _ = open_quick_paste_window(app); },
                _ => {
                    // No action flag: just focus existing window
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.set_focus();
                    } else {
                        let _ = open_main_window(app);
                    }
                }
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
            last_paste_time: Mutex::new(None),
            multi_paste_window_open: Mutex::new(false),
            auto_save_notifier: Mutex::new(None),
            snippet_manager: Arc::new(SnippetManager::new().expect("Failed to init snippet manager")),
        })
        .setup(|app| {
            // --- Background Focus Tracker (Windows only) ---
            // Continuously tracks the last active window that does NOT belong to Madera Tools
            #[cfg(windows)]
            std::thread::spawn(|| {
                use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
                use windows::Win32::System::Threading::GetCurrentProcessId;

                let my_pid = unsafe { GetCurrentProcessId() };
                loop {
                    unsafe {
                        let hwnd = GetForegroundWindow();
                        if hwnd.0 != std::ptr::null_mut() {
                            let mut pid = 0;
                            GetWindowThreadProcessId(hwnd, Some(&mut pid));
                            if pid != my_pid && pid != 0 {
                                PREVIOUS_FOREGROUND_WINDOW.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            });

            // Check if autostart is enabled to show correct menu state
            let autostart_enabled = {
                use tauri_plugin_autostart::ManagerExt;
                app.autolaunch().is_enabled().unwrap_or(false)
            };

            // Load settings from file (persistence) and get guardian_enabled state
            let guardian_auto_save_enabled = {
                let app_handle = app.handle().clone();
                if let Ok(loaded_settings) = load_settings_from_file(&app_handle) {
                    let state = app.state::<AppState>();
                    {
                        if let Ok(mut s) = state.settings.lock() {
                            *s = loaded_settings;
                        }
                    }
                    println!("[Settings] Loaded from file");
                }
                
                // Load guardian settings to get initial state
                app_handle.path().app_config_dir()
                    .ok()
                    .and_then(|dir| std::fs::read_to_string(dir.join("settings.json")).ok())
                    .and_then(|content| serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(&content).ok())
                    .and_then(|data| data.get("guardian_settings").cloned())
                    .and_then(|v| serde_json::from_value::<GuardianSettings>(v).ok())
                    .map(|s| s.auto_save_enabled)
                    .unwrap_or(true)
            };

            // Create tray icon
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let capture = MenuItem::with_id(app, "capture", "Capture (Ctrl+Shift+S)", true, None::<&str>)?;
            let colorpicker = MenuItem::with_id(app, "colorpicker", "Pick Color (Ctrl+Shift+X)", true, None::<&str>)?;
            let history = MenuItem::with_id(app, "history", "History (Ctrl+Shift+H)", true, None::<&str>)?;
            let settings_panel = MenuItem::with_id(app, "settings_panel", "⚙️ Settings", true, None::<&str>)?;
            let desktop_guardian = MenuItem::with_id(app, "desktop_guardian", "Desktop Guardian", true, None::<&str>)?;
            let quick_paste = MenuItem::with_id(app, "quick_paste", "⭐ Quick Prompt Snippets", true, None::<&str>)?;
            let restore_layout = MenuItem::with_id(app, "restore_layout", "⚡ Restore Last Layout", true, None::<&str>)?;
            let clipboard_monitor = CheckMenuItem::with_id(app, "clipboard_monitor", "Clipboard Monitoring", true, true, None::<&str>)?;
            let autostart_label = if cfg!(windows) { "Start with Windows" } else { "Start at Login" };
            let autostart = CheckMenuItem::with_id(app, "autostart", autostart_label, true, autostart_enabled, None::<&str>)?;
            let guardian_auto_save = CheckMenuItem::with_id(app, "guardian_auto_save", "Desktop Guardian", true, guardian_auto_save_enabled, None::<&str>)?;
            let menu = Menu::with_items(app, &[&capture, &colorpicker, &history, &quick_paste, &settings_panel, &desktop_guardian, &restore_layout, &clipboard_monitor, &guardian_auto_save, &autostart, &quit])?;

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
                    "settings_panel" => {
                        let _ = open_settings_window(app);
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
                    "quick_paste" => {
                        let _ = open_quick_paste_window(app);
                    }
                    "restore_layout" => {
                        // Emit event to trigger restore from frontend
                        let _ = app.emit("restore-last-layout", ());
                    }
                    "guardian_auto_save" => {
                        // Toggle Desktop Guardian auto-save
                        use std::fs;
                        use std::collections::HashMap;
                        
                        if let Ok(app_data_dir) = app.path().app_config_dir() {
                            let settings_path = app_data_dir.join("settings.json");
                            
                            // Read existing settings
                            let mut settings_data: HashMap<String, serde_json::Value> = if settings_path.exists() {
                                fs::read_to_string(&settings_path)
                                    .ok()
                                    .and_then(|content| serde_json::from_str(&content).ok())
                                    .unwrap_or_default()
                            } else {
                                HashMap::new()
                            };
                            
                            // Load current guardian settings
                            let mut guardian_settings: GuardianSettings = settings_data
                                .get("guardian_settings")
                                .and_then(|v| serde_json::from_value(v.clone()).ok())
                                .unwrap_or_default();
                            
                            // Toggle the setting
                            guardian_settings.auto_save_enabled = !guardian_settings.auto_save_enabled;
                            
                            // Save back
                            if let Ok(value) = serde_json::to_value(&guardian_settings) {
                                settings_data.insert("guardian_settings".to_string(), value);
                                if let Ok(json_str) = serde_json::to_string_pretty(&settings_data) {
                                    let _ = fs::write(&settings_path, json_str);
                                    println!("[Guardian] Auto-save toggled to: {}", guardian_settings.auto_save_enabled);
                                }
                            }
                        }
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

            // Start background auto-save for Desktop Guardian
            let app_handle_autosave = app.handle().clone();
            
            // Channel for waking up the auto-save thread
            let (tx, rx) = std::sync::mpsc::channel();
            
            // Store sender in state so power monitor can use it
            let state = app.state::<AppState>();
            if let Ok(mut notifier) = state.auto_save_notifier.lock() {
                *notifier = Some(tx);
            }

            std::thread::spawn(move || {
                use std::io::Write;

                // Helper to log to file for debugging
                fn thread_log(app: &AppHandle, msg: &str) {
                    if let Ok(app_data_dir) = app.path().app_config_dir() {
                        let log_path = app_data_dir.join("autosave_debug.log");
                        if let Ok(mut file) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&log_path)
                        {
                            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                            let _ = writeln!(file, "[{}] THREAD: {}", timestamp, msg);
                        }
                    }
                }

                thread_log(&app_handle_autosave, "Auto-save thread started");

                // Wait 30 seconds before first auto-save (give app time to start)
                std::thread::sleep(std::time::Duration::from_secs(30));
                thread_log(&app_handle_autosave, "Initial wait complete, starting auto-save loop");

                loop {
                    // Load interval from settings (default 5 minutes) using direct file I/O
                    let interval_minutes = app_handle_autosave
                        .path().app_config_dir()
                        .ok()
                        .and_then(|dir| std::fs::read_to_string(dir.join("settings.json")).ok())
                        .and_then(|content| serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(&content).ok())
                        .and_then(|data| data.get("guardian_settings").cloned())
                        .and_then(|v| serde_json::from_value::<GuardianSettings>(v).ok())
                        .map(|s| s.auto_save_interval)
                        .unwrap_or(5);

                    thread_log(&app_handle_autosave, &format!("Waiting {} minutes until next auto-save (or wake event)", interval_minutes));

                    // Wait for the configured interval OR a wake-up signal
                    match rx.recv_timeout(std::time::Duration::from_secs(interval_minutes * 60)) {
                        Ok(_) => {
                            thread_log(&app_handle_autosave, "Woke up by notification (System Wake)!");
                            println!("[Auto-save] Woke up by notification - triggering immediate save check");
                        },
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            thread_log(&app_handle_autosave, "Woke up by timeout (Normal Interval)");
                        },
                        Err(_) => {
                            thread_log(&app_handle_autosave, "Channel disconnected, stopping thread");
                            break;
                        }
                    }


                    // FAIL-SAFE: Check for user activity (Windows only)
                    // If the system thinks it's sleeping but the user is active, we missed a wake event!
                    #[cfg(windows)]
                    unsafe {
                        use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
                        use windows::Win32::System::SystemInformation::GetTickCount;

                        let mut last_input = LASTINPUTINFO {
                            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                            dwTime: 0,
                        };

                        if GetLastInputInfo(&mut last_input).as_bool() {
                            let tick_count = GetTickCount();
                            let idle_ms = tick_count.saturating_sub(last_input.dwTime);

                            // If user has been active in the last minute (60000ms)
                            if idle_ms < 60000 {
                                // And we think we are sleeping...
                                if IS_SYSTEM_SLEEPING.load(Ordering::SeqCst) {
                                    thread_log(&app_handle_autosave, "User activity detected while marked as sleeping - forcing wake state!");
                                    println!("[Auto-save] User activity detected while marked as sleeping - forcing wake state!");
                                    IS_SYSTEM_SLEEPING.store(false, Ordering::SeqCst);
                                }
                            }
                        }
                    }

                    // Check if system is sleeping (should be false if woke up by notification)
                    if IS_SYSTEM_SLEEPING.load(Ordering::SeqCst) {
                        thread_log(&app_handle_autosave, "Skipping - system is sleeping");
                        println!("[Auto-save] Skipping - system is sleeping");
                    } else {
                        thread_log(&app_handle_autosave, "Calling perform_auto_save...");
                        // Perform auto-save
                        if let Err(e) = perform_auto_save(&app_handle_autosave) {
                            thread_log(&app_handle_autosave, &format!("ERROR: {}", e));
                            eprintln!("[Auto-save] Error: {}", e);
                        }
                    }
                }
            });

            // Register global shortcut for capture (Ctrl+Shift+S)
            let capture_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyS);
            let app_handle_capture = app.handle().clone();

            app.global_shortcut().on_shortcut(capture_shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let _ = open_selection_window(&app_handle_capture);
                }
            })?;

            // NOTE: Removed global Ctrl+P shortcut - it was blocking print in all apps
            // The editor window can handle Ctrl+P locally if needed

            // Register global shortcut for history (Ctrl+Shift+H)
            let history_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyH);
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

            // Register global shortcut for Quick Paste popup (Ctrl+Alt+V)
            let quickpaste_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyV);
            let app_handle_quickpaste = app.handle().clone();

            app.global_shortcut().on_shortcut(quickpaste_shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let _ = open_multi_paste_window(&app_handle_quickpaste);
                }
            })?;

            // Register global shortcut for Quick Paste snippet manager (Ctrl+Alt+Shift+Q)
            let snippet_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT), Code::KeyQ);
            let app_handle_snippet = app.handle().clone();

            app.global_shortcut().on_shortcut(snippet_shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let _ = open_quick_paste_window(&app_handle_snippet);
                }
            })?;

            // Start keyboard hook for Ctrl+V double-tap detection
            let app_handle_paste = app.handle().clone();
            start_paste_hook(app_handle_paste);

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

            // Handle CLI action flags on first launch (e.g. `madera-tools --capture`)
            let cli_args: Vec<String> = std::env::args().collect();
            if cli_args.iter().any(|a| a == "--capture") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    // Small delay to let the app finish initializing
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let _ = open_selection_window(&h);
                });
            } else if cli_args.iter().any(|a| a == "--history") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let _ = open_history_window(&h);
                });
            } else if cli_args.iter().any(|a| a == "--colorpicker") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let _ = open_color_picker_window(&h);
                });
            } else if cli_args.iter().any(|a| a == "--quickpaste") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let _ = open_multi_paste_window(&h);
                });
            } else if cli_args.iter().any(|a| a == "--snippets") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let _ = open_quick_paste_window(&h);
                });
            }

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
            open_settings_panel,
            // Multi-paste commands
            open_multi_paste_panel,
            close_multi_paste,
            paste_history_item,
            copy_history_item_to_clipboard,
            // Desktop Guardian commands
            get_desktop_monitors,
            get_current_window_layout,
            save_window_layout,
            restore_window_layout,
            get_idle_time,
            open_desktop_guardian,
            ensure_current_window_visible,
            // SSH Upload command
            upload_to_dev_server,
            get_snippets,
            add_snippet,
            delete_snippet,
            update_snippet,
            paste_snippet_item,
            copy_snippet_to_clipboard,
            open_quick_paste_panel,
            toggle_panel_side,
            toggle_panel_monitor,
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
