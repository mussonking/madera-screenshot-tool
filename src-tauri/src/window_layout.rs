use serde::{Deserialize, Serialize};
use windows::core::PWSTR;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowInfo, GetWindowPlacement, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, SetWindowPlacement, SetWindowPos, ShowWindow,
    WINDOWINFO, WINDOWPLACEMENT, WINDOWPLACEMENT_FLAGS, SWP_NOZORDER, SWP_NOACTIVATE,
    SW_RESTORE, SW_MAXIMIZE, SW_MINIMIZE, SW_SHOWNORMAL,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Information about a window's position and state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowPosition {
    pub hwnd: isize,
    pub title: String,
    pub process_name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub monitor_index: i32,
    pub is_maximized: bool,
    pub is_minimized: bool,
}

/// A saved layout containing all window positions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedLayout {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub windows: Vec<WindowPosition>,
    pub is_auto_save: bool,
}

/// Monitor information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorLayout {
    pub index: i32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

/// Get all monitors
pub fn get_all_monitors() -> Vec<MonitorLayout> {
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
    };

    let mut monitors = Vec::new();

    unsafe {
        // Callback for EnumDisplayMonitors
        unsafe extern "system" fn monitor_callback(
            hmonitor: HMONITOR,
            _hdc: HDC,
            _rect: *mut RECT,
            lparam: LPARAM,
        ) -> BOOL {
            let monitors = &mut *(lparam.0 as *mut Vec<MonitorLayout>);

            let mut info: MONITORINFOEXW = std::mem::zeroed();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

            if GetMonitorInfoW(hmonitor, &mut info.monitorInfo).as_bool() {
                let device_name: String = info.szDevice
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| char::from_u32(c as u32).unwrap_or('?'))
                    .collect();

                let rect = info.monitorInfo.rcMonitor;
                let is_primary = (info.monitorInfo.dwFlags & 1) != 0; // MONITORINFOF_PRIMARY

                monitors.push(MonitorLayout {
                    index: monitors.len() as i32,
                    name: device_name,
                    x: rect.left,
                    y: rect.top,
                    width: (rect.right - rect.left) as u32,
                    height: (rect.bottom - rect.top) as u32,
                    is_primary,
                });
            }

            BOOL(1) // Continue enumeration
        }

        let monitors_ptr = &mut monitors as *mut Vec<MonitorLayout>;
        EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(monitor_callback),
            LPARAM(monitors_ptr as isize),
        );
    }

    monitors
}

/// Get the monitor index for a given position
fn get_monitor_for_position(x: i32, y: i32, monitors: &[MonitorLayout]) -> i32 {
    for monitor in monitors {
        if x >= monitor.x
            && x < monitor.x + monitor.width as i32
            && y >= monitor.y
            && y < monitor.y + monitor.height as i32
        {
            return monitor.index;
        }
    }
    0 // Default to primary
}

/// Get process name from process ID
fn get_process_name(pid: u32) -> String {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        if let Ok(handle) = handle {
            let mut buffer = [0u16; 260];
            let mut size = buffer.len() as u32;
            if QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut size).is_ok() {
                let path: String = buffer[..size as usize]
                    .iter()
                    .map(|&c| char::from_u32(c as u32).unwrap_or('?'))
                    .collect();
                // Extract just the filename
                if let Some(name) = path.rsplit('\\').next() {
                    return name.to_string();
                }
            }
        }
    }
    String::new()
}

/// Get window title
fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len == 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; (len + 1) as usize];
        let copied = GetWindowTextW(hwnd, &mut buffer);
        if copied == 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..copied as usize])
    }
}

/// Check if window should be tracked (visible, has title, not a system window)
fn should_track_window(hwnd: HWND) -> bool {
    unsafe {
        // Must be visible OR minimized (minimized windows are still trackable)
        // Don't skip minimized windows - they need to be restored too!
        let is_visible = IsWindowVisible(hwnd).as_bool();
        if !is_visible {
            // Check if it's minimized - if so, still track it
            let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
            placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
            if GetWindowPlacement(hwnd, &mut placement).is_ok() {
                // SW_SHOWMINIMIZED = 2
                if placement.showCmd != 2 {
                    return false; // Not visible and not minimized - skip it
                }
            } else {
                return false; // Can't get placement and not visible - skip it
            }
        }

        // Must have a title
        let title = get_window_title(hwnd);
        if title.is_empty() {
            return false;
        }

        // Skip certain system windows
        let skip_titles = [
            "Program Manager",
            "Windows Input Experience",
            "Microsoft Text Input Application",
            "Settings",
            "NVIDIA GeForce Overlay",
        ];
        if skip_titles.iter().any(|&t| title.contains(t)) {
            return false;
        }

        // Get window info to check style
        let mut info: WINDOWINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<WINDOWINFO>() as u32;
        if GetWindowInfo(hwnd, &mut info).is_ok() {
            // Skip tool windows and other special windows
            let ws_ex_toolwindow = 0x00000080u32;
            if (info.dwExStyle.0 & ws_ex_toolwindow) != 0 {
                return false;
            }
            // Window must have reasonable size
            let width = info.rcWindow.right - info.rcWindow.left;
            let height = info.rcWindow.bottom - info.rcWindow.top;
            if width < 100 || height < 50 {
                return false;
            }
        }

        true
    }
}

/// Get all visible windows with their positions
pub fn get_all_windows() -> Vec<WindowPosition> {
    let mut windows: Vec<WindowPosition> = Vec::new();
    let monitors = get_all_monitors();

    unsafe {
        unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let data = &mut *(lparam.0 as *mut (Vec<WindowPosition>, Vec<MonitorLayout>));
            let (windows, monitors) = data;

            if !should_track_window(hwnd) {
                return BOOL(1); // Continue
            }

            let title = get_window_title(hwnd);

            // Get process info
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let process_name = get_process_name(pid);

            // Get window placement for state (minimized/maximized)
            let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
            placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;

            if GetWindowPlacement(hwnd, &mut placement).is_ok() {
                // SW_SHOWMAXIMIZED = 3, SW_SHOWMINIMIZED = 2
                let is_maximized = placement.showCmd == 3;
                let is_minimized = placement.showCmd == 2;

                // For position/size, use different sources depending on window state:
                // - Minimized: use rcNormalPosition (GetWindowRect returns weird coords)
                // - Maximized/Normal: use GetWindowRect (actual current position)
                let rect = if is_minimized {
                    // For minimized windows, use the "normal" position
                    placement.rcNormalPosition
                } else {
                    // For visible/maximized windows, get ACTUAL current rect
                    let mut current_rect = RECT::default();
                    if GetWindowRect(hwnd, &mut current_rect).is_err() {
                        return BOOL(1); // Skip if can't get rect
                    }
                    current_rect
                };

                let x = rect.left;
                let y = rect.top;
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;

                let monitor_index = get_monitor_for_position(x, y, monitors);

                windows.push(WindowPosition {
                    hwnd: hwnd.0 as isize,
                    title,
                    process_name,
                    x,
                    y,
                    width,
                    height,
                    monitor_index,
                    is_maximized,
                    is_minimized,
                });
            }

            BOOL(1) // Continue enumeration
        }

        let mut data = (windows, monitors);
        let data_ptr = &mut data as *mut (Vec<WindowPosition>, Vec<MonitorLayout>);
        EnumWindows(Some(enum_callback), LPARAM(data_ptr as isize)).ok();
        windows = data.0;
    }

    windows
}

/// Restore a window to its saved position
pub fn restore_window_position(pos: &WindowPosition) -> Result<(), String> {
    unsafe {
        let hwnd = HWND(pos.hwnd as *mut std::ffi::c_void);

        // Verify window still exists - check placement instead of visibility
        // (minimized windows return false for IsWindowVisible)
        let mut test_placement: WINDOWPLACEMENT = std::mem::zeroed();
        test_placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        if GetWindowPlacement(hwnd, &mut test_placement).is_err() {
            return Err(format!("Window '{}' no longer exists", pos.title));
        }

        // First, restore the window if it's maximized/minimized so we can move it
        let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;

        if GetWindowPlacement(hwnd, &mut placement).is_err() {
            return Err(format!("Cannot get placement for '{}'", pos.title));
        }

        // If window is currently maximized or minimized, restore it first
        if placement.showCmd == 2 || placement.showCmd == 3 {
            ShowWindow(hwnd, SW_RESTORE);
            // Small delay to let the window restore
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        // For maximized windows, we need to set the normal position first
        // so that when Windows maximizes, it uses the correct monitor
        if pos.is_maximized {
            // Set the rcNormalPosition to the saved position BEFORE maximizing
            // This ensures Windows maximizes on the correct monitor
            placement.rcNormalPosition = RECT {
                left: pos.x,
                top: pos.y,
                right: pos.x + pos.width,
                bottom: pos.y + pos.height,
            };
            placement.flags = WINDOWPLACEMENT_FLAGS(0);
            placement.showCmd = 3; // SW_SHOWMAXIMIZED

            if SetWindowPlacement(hwnd, &placement).is_err() {
                return Err(format!("Failed to maximize '{}' on correct monitor", pos.title));
            }
        } else {
            // Use SetWindowPos for non-maximized windows
            let result = SetWindowPos(
                hwnd,
                None, // No z-order change
                pos.x,
                pos.y,
                pos.width,
                pos.height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );

            if result.is_err() {
                return Err(format!("Failed to set position for '{}': {:?}", pos.title, result));
            }

            // Apply minimized state if needed
            if pos.is_minimized {
                ShowWindow(hwnd, SW_MINIMIZE);
            }
        }

        // For non-maximized windows, also update the placement for the "normal" position
        // (For maximized windows, we already did this above)
        if !pos.is_maximized {
            placement.rcNormalPosition = RECT {
                left: pos.x,
                top: pos.y,
                right: pos.x + pos.width,
                bottom: pos.y + pos.height,
            };
            placement.flags = WINDOWPLACEMENT_FLAGS(0);
            let _ = SetWindowPlacement(hwnd, &placement); // Best effort
        }
    }
    Ok(())
}

/// Calculate similarity score between two window titles (0-100)
fn title_similarity(saved_title: &str, current_title: &str) -> u32 {
    // Exact match = 100
    if saved_title == current_title {
        return 100;
    }

    // Normalize titles for comparison (lowercase, trim)
    let saved = saved_title.to_lowercase();
    let current = current_title.to_lowercase();

    // Exact match after normalization
    if saved == current {
        return 95;
    }

    // One contains the other completely
    if current.contains(&saved) || saved.contains(&current) {
        return 80;
    }

    // For apps like VSCode/Chrome, extract the meaningful part
    // "projet-A - Visual Studio Code" -> "projet-A"
    // "Gmail - Google Chrome" -> "Gmail"
    let saved_parts: Vec<&str> = saved.split(" - ").collect();
    let current_parts: Vec<&str> = current.split(" - ").collect();

    // Compare first parts (usually the document/tab name)
    if !saved_parts.is_empty() && !current_parts.is_empty() {
        let saved_main = saved_parts[0].trim();
        let current_main = current_parts[0].trim();

        if saved_main == current_main {
            return 90;
        }
        if current_main.contains(saved_main) || saved_main.contains(current_main) {
            return 70;
        }
    }

    // Check for common words
    let saved_words: std::collections::HashSet<&str> = saved.split_whitespace().collect();
    let current_words: std::collections::HashSet<&str> = current.split_whitespace().collect();
    let common = saved_words.intersection(&current_words).count();
    let total = saved_words.len().max(current_words.len());

    if total > 0 {
        let word_score = (common * 60 / total) as u32;
        if word_score > 30 {
            return word_score;
        }
    }

    // No meaningful match
    0
}

/// Try to match saved windows with current windows using smart matching
pub fn match_and_restore_layout(saved: &SavedLayout) -> Vec<(String, Result<(), String>)> {
    let current_windows = get_all_windows();
    let mut results = Vec::new();
    let mut used_hwnds: std::collections::HashSet<isize> = std::collections::HashSet::new();

    // First pass: Find best matches for each saved window
    // Sort saved windows by specificity (more unique titles first)
    let mut saved_with_scores: Vec<(&WindowPosition, Option<&WindowPosition>, u32)> = Vec::new();

    for saved_pos in &saved.windows {
        let mut best_match: Option<&WindowPosition> = None;
        let mut best_score: u32 = 0;

        for current in &current_windows {
            // Skip if already used
            if used_hwnds.contains(&current.hwnd) {
                continue;
            }

            // Must match process name
            if saved_pos.process_name.is_empty() || current.process_name != saved_pos.process_name {
                continue;
            }

            // Calculate title similarity
            let score = title_similarity(&saved_pos.title, &current.title);

            // Higher score = better match
            if score > best_score {
                best_score = score;
                best_match = Some(current);
            }
        }

        // Only accept matches with score >= 50 (decent title match)
        // Or score >= 0 if it's the only window of that process
        let same_process_count = current_windows.iter()
            .filter(|w| w.process_name == saved_pos.process_name && !used_hwnds.contains(&w.hwnd))
            .count();

        if let Some(matched) = best_match {
            // Accept if good score OR only one window of this process type
            if best_score >= 50 || same_process_count == 1 {
                used_hwnds.insert(matched.hwnd);
                saved_with_scores.push((saved_pos, Some(matched), best_score));
            } else {
                saved_with_scores.push((saved_pos, None, 0));
            }
        } else {
            saved_with_scores.push((saved_pos, None, 0));
        }
    }

    // Second pass: For unmatched windows, try fallback matching by process only
    for (saved_pos, matched, score) in &mut saved_with_scores {
        if matched.is_none() {
            // Try to find any unmatched window of the same process
            for current in &current_windows {
                if !used_hwnds.contains(&current.hwnd)
                    && !saved_pos.process_name.is_empty()
                    && current.process_name == saved_pos.process_name
                {
                    used_hwnds.insert(current.hwnd);
                    *matched = Some(current);
                    *score = 10; // Low score for fallback match
                    break;
                }
            }
        }
    }

    // Apply the matches
    for (saved_pos, matched, score) in saved_with_scores {
        if let Some(current) = matched {
            let restore_pos = WindowPosition {
                hwnd: current.hwnd,
                title: current.title.clone(),
                process_name: current.process_name.clone(),
                x: saved_pos.x,
                y: saved_pos.y,
                width: saved_pos.width,
                height: saved_pos.height,
                monitor_index: saved_pos.monitor_index,
                is_maximized: saved_pos.is_maximized,
                is_minimized: saved_pos.is_minimized,
            };
            let result = restore_window_position(&restore_pos);

            // Include match quality in success message
            let match_info = if score >= 90 {
                format!("{} (exact match)", saved_pos.title)
            } else if score >= 50 {
                format!("{} -> {} (similar)", saved_pos.title, current.title)
            } else {
                format!("{} -> {} (fallback)", saved_pos.title, current.title)
            };

            results.push((match_info, result));
        } else {
            results.push((
                saved_pos.title.clone(),
                Err(format!("Window '{}' ({}) not found", saved_pos.title, saved_pos.process_name)),
            ));
        }
    }

    results
}
