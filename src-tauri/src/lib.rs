mod capture;
mod clipboard;
mod clipboard_monitor;
mod color_picker;
mod history;
mod native_selection;
mod snippet_manager;
mod ssh_uploader;
#[cfg(target_os = "linux")]
mod wayland_focus;

use capture::CaptureManager;
use clipboard::ClipboardManager;
use clipboard_monitor::{ClipboardContent, ClipboardMonitor, ClipboardSettings};
use color_picker::{ColorFormat, ColorInfo, ColorPickSettings};
use history::{HistoryItem, HistoryItemType, HistoryManager, ScreenshotRecord};
use snippet_manager::{SnippetItem, SnippetManager};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Check if the current session is Wayland (vs X11)
#[cfg(target_os = "linux")]
fn is_wayland_session() -> bool {
    std::env::var("XDG_SESSION_TYPE").map(|v| v == "wayland").unwrap_or(false)
}

/// Apply wlr-layer-shell overlay to a Tauri window (Wayland always-on-top).
/// Must be called BEFORE the GTK window is realized (i.e. before .show()).
/// Runs on the main thread as required by GTK.
#[cfg(target_os = "linux")]
fn apply_layer_shell_overlay(window: &tauri::WebviewWindow, width: i32) {
    if !is_wayland_session() {
        return; // Layer-shell is Wayland-only; on X11 use normal window behavior
    }
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
    // SSH Upload settings
    pub ssh_enabled: bool,
    #[serde(default)]
    pub ssh_servers: Vec<SshServer>,
    // Legacy single-server fields (kept for migration from old settings files)
    #[serde(default)]
    pub ssh_host: String,
    #[serde(default)]
    pub ssh_remote_path: String,
    #[serde(default)]
    pub ssh_passphrase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshServer {
    pub id: String,
    pub name: String,
    pub host: String,
    pub remote_path: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Shift+S".to_string(),
            auto_copy: true,
            max_history: 100,
            max_image_width: Some(1568),
            ssh_enabled: true,
            ssh_servers: vec![SshServer {
                id: "default".to_string(),
                name: "Mac Mini".to_string(),
                host: "mad@192.168.2.71".to_string(),
                remote_path: "/home/mad/.claude/downloads".to_string(),
            }],
            ssh_host: String::new(),
            ssh_remote_path: String::new(),
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
    server_id: String,
) -> Result<String, String> {
    use base64::Engine;

    let (enabled, server, passphrase) = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        if !settings.ssh_enabled {
            return Err("SSH upload not enabled - enable it in Settings".to_string());
        }
        // Find the requested server; fall back to legacy single-server config
        let server = settings.ssh_servers.iter()
            .find(|s| s.id == server_id)
            .cloned()
            .or_else(|| {
                // Migrate legacy single-server config on the fly
                if !settings.ssh_host.is_empty() {
                    Some(SshServer {
                        id: "legacy".to_string(),
                        name: "Server".to_string(),
                        host: settings.ssh_host.clone(),
                        remote_path: settings.ssh_remote_path.clone(),
                    })
                } else {
                    None
                }
            });
        (settings.ssh_enabled, server, settings.ssh_passphrase.clone())
    };

    if !enabled {
        return Err("SSH upload not enabled - enable it in Settings".to_string());
    }

    let server = server.ok_or("SSH server not found - check your SSH settings")?;

    if server.host.is_empty() {
        return Err("SSH host not configured".to_string());
    }
    if server.remote_path.is_empty() {
        return Err("Remote path not configured".to_string());
    }

    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("screenshot_{}.png", timestamp);
    let base_path = server.remote_path.trim_end_matches('/');
    let full_remote_path = format!("{}/{}", base_path, filename);

    let data = base64::engine::general_purpose::STANDARD
        .decode(&image_data)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    let uploader = ssh_uploader::SshUploader::new(server.host.clone());
    uploader.upload_file(&data, &full_remote_path, &passphrase)
        .map_err(|e| format!("SSH upload failed: {}", e))?;

    {
        let mut clipboard_manager = state.clipboard_manager.lock().map_err(|e| e.to_string())?;
        clipboard_manager.copy_text_to_clipboard(&full_remote_path)
            .map_err(|e| e.to_string())?;
    }

    Ok(full_remote_path)
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
        let mut settings: AppSettings = serde_json::from_value(val.clone()).map_err(|e| e.to_string())?;
        // Migrate legacy single-server config to ssh_servers list
        if settings.ssh_servers.is_empty() && !settings.ssh_host.is_empty() {
            settings.ssh_servers = vec![SshServer {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Server".to_string(),
                host: settings.ssh_host.clone(),
                remote_path: settings.ssh_remote_path.clone(),
            }];
            settings.ssh_host = String::new();
            settings.ssh_remote_path = String::new();
        }
        Ok(settings)
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

fn open_selection_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[lib] open_selection_window called");
    // Use native Win32 selection - no WebView, no flash!
    // The native selection captures ALL monitors and returns the cropped image directly
    let app_handle = app.clone();

    std::thread::spawn(move || {
        eprintln!("[lib] open_selection_window thread started");
        // Show native selection overlay - it captures all screens internally
        if let Some(selection) = native_selection::show_native_selection() {
            eprintln!("[lib] native_selection returned Some, opening editor");
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
        } else {
            eprintln!("[lib] native_selection returned None");
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

    // Force size via Physical pixels (Wayland compositors may ignore inner_size)
    window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width: window_width as u32,
        height: window_height as u32,
    }))?;

    // Center in work area
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

    #[cfg(target_os = "linux")]
    {
        wayland_focus::snapshot_for_paste();
        if !is_wayland_session() {
            if let Ok(output) = std::process::Command::new("xdotool")
                .arg("getactivewindow")
                .output()
            {
                if let Ok(id_str) = String::from_utf8(output.stdout) {
                    if let Ok(wid) = id_str.trim().parse::<u64>() {
                        wayland_focus::set_x11_window_id(wid);
                    }
                }
            }
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
async fn open_quick_paste_panel(app: tauri::AppHandle, tab: Option<String>) -> Result<(), String> {
    open_quick_paste_window(&app, tab.as_deref())
}

/// Toggle layer-shell anchor between left and right for a given window
#[cfg(target_os = "linux")]
#[tauri::command]
async fn toggle_panel_side(app: AppHandle, window_label: String) -> Result<String, String> {
    if !is_wayland_session() {
        return Ok("right".to_string()); // Layer-shell not available on X11
    }
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
    if !is_wayland_session() {
        return Ok(()); // Layer-shell not available on X11
    }
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

#[cfg(target_os = "linux")]
#[tauri::command]
async fn set_panel_keyboard(app: AppHandle, window_label: String, enabled: bool) -> Result<(), String> {
    if !is_wayland_session() {
        return Ok(()); // On X11 the window has normal keyboard focus
    }
    let window = app.get_webview_window(&window_label)
        .ok_or("Window not found")?;
    let win = window.clone();
    window.run_on_main_thread(move || {
        if let Ok(gtk_win) = win.gtk_window() {
            use gtk::prelude::*;
            use gtk_layer_shell::LayerShell;
            let w = gtk_win.upcast_ref::<gtk::Window>();
            w.set_keyboard_interactivity(enabled);
        }
    }).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command]
async fn set_panel_keyboard(_app: AppHandle, _window_label: String, _enabled: bool) -> Result<(), String> {
    Ok(())
}

fn open_quick_paste_window(app: &tauri::AppHandle, tab: Option<&str>) -> Result<(), String> {
    // Snapshot the active window NOW, before the panel opens and COSMIC potentially
    // shifts focus to the window behind it. This snapshot is used by activate_last_focused().
    #[cfg(target_os = "linux")]
    wayland_focus::snapshot_for_paste();

    #[cfg(target_os = "linux")]
    if !is_wayland_session() {
        if let Ok(output) = std::process::Command::new("xdotool")
            .arg("getactivewindow")
            .output()
        {
            if let Ok(id_str) = String::from_utf8(output.stdout) {
                if let Ok(wid) = id_str.trim().parse::<u64>() {
                    wayland_focus::set_x11_window_id(wid);
                }
            }
        }
    }

    if let Some(window) = app.get_webview_window("quickpaste") {
        window.close().map_err(|e| e.to_string())?;
    }

    let (cursor_x, _cursor_y, monitor_x, monitor_y, monitor_w, monitor_h) = get_cursor_and_monitor_info();
    eprintln!("[quickpaste] cursor=({},{}) monitor=({},{} {}x{})", cursor_x, _cursor_y, monitor_x, monitor_y, monitor_w, monitor_h);
    let window_height = monitor_h as f64;
    let window_width = 400.0;

    // Position: right edge of the monitor the cursor is on
    // If cursor is in the right half, anchor right; left half, anchor left
    let monitor_center_x = monitor_x as f64 + monitor_w as f64 / 2.0;
    let pos_x = if (cursor_x as f64) >= monitor_center_x {
        (monitor_x + monitor_w) as f64 - window_width
    } else {
        monitor_x as f64
    };
    let pos_y = monitor_y as f64;

    // Pass tab selection via URL hash query
    let tab_param = tab.unwrap_or("snippets");
    let url = format!("index.html#quickpaste?tab={}", tab_param);

    let window = WebviewWindowBuilder::new(app, "quickpaste", WebviewUrl::App(url.into()))
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
async fn add_snippet_from_clipboard(state: State<'_, AppState>) -> Result<SnippetItem, String> {
    let text = {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        clipboard.get_text().map_err(|e| e.to_string())?
    };
    if text.trim().is_empty() {
        return Err("Clipboard is empty".to_string());
    }
    let preview = text.chars().take(40).collect::<String>();
    let title = if preview.len() < text.len() {
        format!("{}...", preview)
    } else {
        preview
    };
    Ok(state.snippet_manager.add(title, "text".to_string(), text))
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
async fn update_snippet_category(state: State<'_, AppState>, id: String, category: String) -> Result<bool, String> {
    Ok(state.snippet_manager.update_category(&id, category))
}

#[tauri::command]
async fn reorder_snippets(state: State<'_, AppState>, ordered_ids: Vec<String>) -> Result<bool, String> {
    Ok(state.snippet_manager.reorder(ordered_ids))
}

#[tauri::command]
async fn get_snippet_categories(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.snippet_manager.get_categories())
}

#[tauri::command]
async fn rename_snippet_category(state: State<'_, AppState>, old_name: String, new_name: String) -> Result<bool, String> {
    Ok(state.snippet_manager.rename_category(&old_name, &new_name))
}

#[tauri::command]
async fn delete_snippet_category(state: State<'_, AppState>, name: String) -> Result<(), String> {
    state.snippet_manager.delete_category(&name);
    Ok(())
}

#[tauri::command]
async fn add_snippet_with_category(state: State<'_, AppState>, title: String, content_type: String, content: String, category: String) -> Result<SnippetItem, String> {
    Ok(state.snippet_manager.add_with_category(title, content_type, content, category))
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
    let app_clone = app.clone();
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
            std::thread::sleep(std::time::Duration::from_millis(100));
            simulate_paste();
        }

        #[cfg(not(windows))]
        {
            // On X11, close the paste panel first so it doesn't steal focus back
            #[cfg(target_os = "linux")]
            if !is_wayland_session() {
                for label in &["quickpaste", "multipaste"] {
                    if let Some(w) = app_clone.get_webview_window(label) {
                        let _ = w.hide();
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            wayland_focus::activate_last_focused();
            std::thread::sleep(std::time::Duration::from_millis(350));
            simulate_paste();
            // On X11, close the panel after paste completes
            #[cfg(target_os = "linux")]
            if !is_wayland_session() {
                for label in &["quickpaste", "multipaste"] {
                    if let Some(w) = app_clone.get_webview_window(label) {
                        let _ = w.close();
                    }
                }
            }
        }
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

    // Auto-paste: restore focus to previous window and simulate paste
    let is_text = matches!(item.item_type, HistoryItemType::ClipboardText | HistoryItemType::ColorPick);
    let paste_content = if is_text {
        item.text_content.clone().or_else(|| item.color_hex.clone()).unwrap_or_default()
    } else {
        String::new()
    };
    let app_clone = app.clone();
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
            std::thread::sleep(std::time::Duration::from_millis(100));
            simulate_paste();
        }

        #[cfg(not(windows))]
        {
            // On X11, hide the paste panel first so it doesn't steal focus back
            #[cfg(target_os = "linux")]
            if !is_wayland_session() {
                for label in &["quickpaste", "multipaste"] {
                    if let Some(w) = app_clone.get_webview_window(label) {
                        let _ = w.hide();
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            wayland_focus::activate_last_focused();
            std::thread::sleep(std::time::Duration::from_millis(350));
            simulate_paste();
            // On X11, close the panel after paste completes
            #[cfg(target_os = "linux")]
            if !is_wayland_session() {
                for label in &["quickpaste", "multipaste"] {
                    if let Some(w) = app_clone.get_webview_window(label) {
                        let _ = w.close();
                    }
                }
            }
        }
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
                                let _ = open_quick_paste_window(app, Some("history"));
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
fn is_active_window_terminal() -> bool {
    // On X11, check WM_CLASS of the focused window to detect terminals
    let window_id = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok();
    let window_id = match window_id {
        Some(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => return false,
    };
    let xprop = std::process::Command::new("xprop")
        .args(["-id", &window_id, "WM_CLASS"])
        .output()
        .ok();
    let class = match xprop {
        Some(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_lowercase(),
        _ => return false,
    };
    const TERMINALS: &[&str] = &[
        "wezterm", "gnome-terminal", "kitty", "alacritty", "xterm",
        "konsole", "tilix", "terminator", "xfce4-terminal", "sakura",
        "st-256color", "urxvt", "foot", "blackbox",
    ];
    TERMINALS.iter().any(|t| class.contains(t))
}

#[cfg(not(windows))]
fn simulate_paste() {
    // Detect terminal to use Shift+Ctrl+V instead of Ctrl+V
    let use_terminal_paste = is_active_window_terminal();
    if use_terminal_paste {
        eprintln!("[paste] Terminal detected, using Shift+Ctrl+V");
    }

    let key = if use_terminal_paste { "shift+ctrl+v" } else { "ctrl+v" };

    // ydotool injects via kernel uinput — bypasses Wayland focus entirely.
    // Falls back to wtype (Wayland protocol) then xdotool (X11).
    let ok = std::process::Command::new("ydotool")
        .args(["key", "--delay", "0", key])
        .spawn()
        .is_ok();
    if !ok {
        if use_terminal_paste {
            let ok2 = std::process::Command::new("wtype")
                .args(["-M", "shift", "-M", "ctrl", "-P", "v", "-p", "v", "-m", "ctrl", "-m", "shift"])
                .spawn()
                .is_ok();
            if !ok2 {
                let _ = std::process::Command::new("xdotool")
                    .args(["key", "shift+ctrl+v"])
                    .spawn();
            }
        } else {
            let ok2 = std::process::Command::new("wtype")
                .args(["-M", "ctrl", "-P", "v", "-p", "v", "-m", "ctrl"])
                .spawn()
                .is_ok();
            if !ok2 {
                let _ = std::process::Command::new("xdotool")
                    .args(["key", "ctrl+v"])
                    .spawn();
            }
        }
    }
}

#[cfg(not(windows))]
fn type_text(text: &str) {
    // Strip Windows \r line endings to avoid extra Enter keypresses
    let clean = text.replace('\r', "");
    let text = clean.as_str();
    // ydotool injects via kernel uinput — bypasses Wayland focus entirely.
    let ok = std::process::Command::new("ydotool")
        .args(["type", "--delay", "0", "--key-delay", "2", "--", text])
        .spawn()
        .is_ok();
    if !ok {
        // Fallback: wtype (Wayland protocol)
        let ok2 = std::process::Command::new("wtype")
            .arg("--")
            .arg(text)
            .spawn()
            .is_ok();
        if !ok2 {
            // Last resort: xdotool (X11/XWayland)
            let _ = std::process::Command::new("xdotool")
                .args(["type", "--clearmodifiers", "--", text])
                .spawn();
        }
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
                Some("quickpaste") => { let _ = open_quick_paste_window(app, Some("history")); },
                Some("snippets") => { let _ = open_quick_paste_window(app, Some("snippets")); },
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

            // --- Background Focus Tracker (Linux/Wayland) ---
            // Tracks the last active non-Madera toplevel via zcosmic_toplevel_info_v1
            // so we can re-activate it before paste (equivalent to Windows SetForegroundWindow)
            #[cfg(target_os = "linux")]
            wayland_focus::init();

            // Check if autostart is enabled to show correct menu state
            let autostart_enabled = {
                use tauri_plugin_autostart::ManagerExt;
                app.autolaunch().is_enabled().unwrap_or(false)
            };

            // Load settings from file
            {
                let app_handle = app.handle().clone();
                if let Ok(loaded_settings) = load_settings_from_file(&app_handle) {
                    let state = app.state::<AppState>();
                    if let Ok(mut s) = state.settings.lock() {
                        *s = loaded_settings;
                    }
                    println!("[Settings] Loaded from file");
                }
            }

            // Create tray icon
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let capture = MenuItem::with_id(app, "capture", "Capture (Ctrl+Shift+S)", true, None::<&str>)?;
            let colorpicker = MenuItem::with_id(app, "colorpicker", "Pick Color (Ctrl+Shift+X)", true, None::<&str>)?;
            let history = MenuItem::with_id(app, "history", "History (Ctrl+Shift+H)", true, None::<&str>)?;
            let settings_panel = MenuItem::with_id(app, "settings_panel", "⚙️ Settings", true, None::<&str>)?;
            let quick_paste = MenuItem::with_id(app, "quick_paste", "⭐ Quick Prompt Snippets", true, None::<&str>)?;
            let clipboard_monitor = CheckMenuItem::with_id(app, "clipboard_monitor", "Clipboard Monitoring", true, true, None::<&str>)?;
            let autostart_label = if cfg!(windows) { "Start with Windows" } else { "Start at Login" };
            let autostart = CheckMenuItem::with_id(app, "autostart", autostart_label, true, autostart_enabled, None::<&str>)?;
            let menu = Menu::with_items(app, &[&capture, &colorpicker, &history, &quick_paste, &settings_panel, &clipboard_monitor, &autostart, &quit])?;

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
                    "quick_paste" => {
                        let _ = open_quick_paste_window(app, Some("history"));
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
                    let _ = open_quick_paste_window(&app_handle_quickpaste, Some("history"));
                }
            })?;

            // Register global shortcut for Quick Paste snippet manager (Ctrl+Alt+Shift+Q)
            let snippet_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT), Code::KeyQ);
            let app_handle_snippet = app.handle().clone();

            app.global_shortcut().on_shortcut(snippet_shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let _ = open_quick_paste_window(&app_handle_snippet, Some("snippets"));
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
                    let _ = open_quick_paste_window(&h, Some("history"));
                });
            } else if cli_args.iter().any(|a| a == "--snippets") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let _ = open_quick_paste_window(&h, Some("snippets"));
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
            paste_history_item,
            copy_history_item_to_clipboard,
            // SSH Upload command
            upload_to_dev_server,
            get_snippets,
            add_snippet,
            add_snippet_with_category,
            add_snippet_from_clipboard,
            delete_snippet,
            update_snippet,
            update_snippet_category,
            reorder_snippets,
            get_snippet_categories,
            rename_snippet_category,
            delete_snippet_category,
            paste_snippet_item,
            copy_snippet_to_clipboard,
            open_quick_paste_panel,
            toggle_panel_side,
            toggle_panel_monitor,
            set_panel_keyboard,
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
