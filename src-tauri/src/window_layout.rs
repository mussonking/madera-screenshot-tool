use serde::{Deserialize, Serialize};

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

// ============================================
// Windows implementation
// ============================================
#[cfg(windows)]
mod platform {
    use super::*;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowInfo, GetWindowPlacement, GetWindowRect, GetWindowTextLengthW,
        GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, SetWindowPlacement,
        SetWindowPos, ShowWindow, WINDOWINFO, WINDOWPLACEMENT, WINDOWPLACEMENT_FLAGS,
        SWP_NOZORDER, SWP_NOACTIVATE, SW_RESTORE, SW_MAXIMIZE, SW_MINIMIZE, SW_SHOWNORMAL,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// Get all monitors
    pub fn get_all_monitors() -> Vec<MonitorLayout> {
        use windows::Win32::Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
        };

        let mut monitors = Vec::new();

        unsafe {
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
                    let device_name: String = info
                        .szDevice
                        .iter()
                        .take_while(|&&c| c != 0)
                        .map(|&c| char::from_u32(c as u32).unwrap_or('?'))
                        .collect();

                    let rect = info.monitorInfo.rcMonitor;
                    let is_primary = (info.monitorInfo.dwFlags & 1) != 0;

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

                BOOL(1)
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
        0
    }

    fn get_process_name(pid: u32) -> String {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
            if let Ok(handle) = handle {
                let mut buffer = [0u16; 260];
                let mut size = buffer.len() as u32;
                if QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_WIN32,
                    PWSTR(buffer.as_mut_ptr()),
                    &mut size,
                )
                .is_ok()
                {
                    let path: String = buffer[..size as usize]
                        .iter()
                        .map(|&c| char::from_u32(c as u32).unwrap_or('?'))
                        .collect();
                    if let Some(name) = path.rsplit('\\').next() {
                        return name.to_string();
                    }
                }
            }
        }
        String::new()
    }

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

    fn should_track_window(hwnd: HWND) -> bool {
        unsafe {
            let is_visible = IsWindowVisible(hwnd).as_bool();
            if !is_visible {
                let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
                placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
                if GetWindowPlacement(hwnd, &mut placement).is_ok() {
                    if placement.showCmd != 2 {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            let title = get_window_title(hwnd);
            if title.is_empty() {
                return false;
            }

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

            let mut info: WINDOWINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<WINDOWINFO>() as u32;
            if GetWindowInfo(hwnd, &mut info).is_ok() {
                let ws_ex_toolwindow = 0x00000080u32;
                if (info.dwExStyle.0 & ws_ex_toolwindow) != 0 {
                    return false;
                }
                let width = info.rcWindow.right - info.rcWindow.left;
                let height = info.rcWindow.bottom - info.rcWindow.top;
                if width < 100 || height < 50 {
                    return false;
                }
            }

            true
        }
    }

    pub fn get_all_windows() -> Vec<WindowPosition> {
        let mut windows: Vec<WindowPosition> = Vec::new();
        let monitors = get_all_monitors();

        unsafe {
            unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
                let data = &mut *(lparam.0 as *mut (Vec<WindowPosition>, Vec<MonitorLayout>));
                let (windows, monitors) = data;

                if !should_track_window(hwnd) {
                    return BOOL(1);
                }

                let title = get_window_title(hwnd);

                let mut pid: u32 = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                let process_name = get_process_name(pid);

                let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
                placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;

                if GetWindowPlacement(hwnd, &mut placement).is_ok() {
                    let is_minimized = placement.showCmd == 2;

                    let rect = if is_minimized {
                        placement.rcNormalPosition
                    } else {
                        let mut current_rect = RECT::default();
                        if GetWindowRect(hwnd, &mut current_rect).is_err() {
                            return BOOL(1);
                        }
                        current_rect
                    };

                    let x = rect.left;
                    let y = rect.top;
                    let width = rect.right - rect.left;
                    let height = rect.bottom - rect.top;

                    let monitor_index = get_monitor_for_position(x, y, monitors);

                    let is_maximized = if is_minimized {
                        false
                    } else {
                        let showCmd_maximized = placement.showCmd == 3;

                        let monitor = monitors.get(monitor_index as usize);
                        let visually_maximized = if let Some(mon) = monitor {
                            let mon_width = mon.width as i32;
                            let mon_height = mon.height as i32;
                            let width_matches = (width - mon_width).abs() <= 20;
                            let height_matches = (height - mon_height).abs() <= 20;
                            width_matches && height_matches
                        } else {
                            false
                        };

                        showCmd_maximized || visually_maximized
                    };

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

                BOOL(1)
            }

            let mut data = (windows, monitors);
            let data_ptr = &mut data as *mut (Vec<WindowPosition>, Vec<MonitorLayout>);
            EnumWindows(Some(enum_callback), LPARAM(data_ptr as isize)).ok();
            windows = data.0;
        }

        windows
    }

    pub fn restore_window_position(pos: &WindowPosition) -> Result<(), String> {
        unsafe {
            let hwnd = HWND(pos.hwnd as *mut std::ffi::c_void);

            let mut test_placement: WINDOWPLACEMENT = std::mem::zeroed();
            test_placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
            if GetWindowPlacement(hwnd, &mut test_placement).is_err() {
                return Err(format!("Window '{}' no longer exists", pos.title));
            }

            let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
            placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;

            if GetWindowPlacement(hwnd, &mut placement).is_err() {
                return Err(format!("Cannot get placement for '{}'", pos.title));
            }

            if placement.showCmd == 2 || placement.showCmd == 3 {
                ShowWindow(hwnd, SW_RESTORE);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            if pos.is_maximized {
                placement.rcNormalPosition = RECT {
                    left: pos.x,
                    top: pos.y,
                    right: pos.x + pos.width,
                    bottom: pos.y + pos.height,
                };
                placement.flags = WINDOWPLACEMENT_FLAGS(0);
                placement.showCmd = 3;

                if SetWindowPlacement(hwnd, &placement).is_err() {
                    return Err(format!(
                        "Failed to maximize '{}' on correct monitor",
                        pos.title
                    ));
                }
            } else {
                let result = SetWindowPos(
                    hwnd,
                    None,
                    pos.x,
                    pos.y,
                    pos.width,
                    pos.height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );

                if result.is_err() {
                    return Err(format!(
                        "Failed to set position for '{}': {:?}",
                        pos.title, result
                    ));
                }

                if pos.is_minimized {
                    ShowWindow(hwnd, SW_MINIMIZE);
                }
            }

            if !pos.is_maximized {
                placement.rcNormalPosition = RECT {
                    left: pos.x,
                    top: pos.y,
                    right: pos.x + pos.width,
                    bottom: pos.y + pos.height,
                };
                placement.flags = WINDOWPLACEMENT_FLAGS(0);
                let _ = SetWindowPlacement(hwnd, &placement);
            }
        }
        Ok(())
    }

    fn title_similarity(saved_title: &str, current_title: &str) -> u32 {
        if saved_title == current_title {
            return 100;
        }

        let saved = saved_title.to_lowercase();
        let current = current_title.to_lowercase();

        if saved == current {
            return 95;
        }

        if current.contains(&saved) || saved.contains(&current) {
            return 80;
        }

        let saved_parts: Vec<&str> = saved.split(" - ").collect();
        let current_parts: Vec<&str> = current.split(" - ").collect();

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

        0
    }

    pub fn match_and_restore_layout(saved: &SavedLayout) -> Vec<(String, Result<(), String>)> {
        let current_windows = get_all_windows();
        let mut results = Vec::new();
        let mut used_hwnds: std::collections::HashSet<isize> = std::collections::HashSet::new();

        let mut saved_with_scores: Vec<(&WindowPosition, Option<&WindowPosition>, u32)> =
            Vec::new();

        for saved_pos in &saved.windows {
            let mut best_match: Option<&WindowPosition> = None;
            let mut best_score: u32 = 0;

            for current in &current_windows {
                if used_hwnds.contains(&current.hwnd) {
                    continue;
                }

                if saved_pos.process_name.is_empty()
                    || current.process_name != saved_pos.process_name
                {
                    continue;
                }

                let score = title_similarity(&saved_pos.title, &current.title);

                if score > best_score {
                    best_score = score;
                    best_match = Some(current);
                }
            }

            let same_process_count = current_windows
                .iter()
                .filter(|w| {
                    w.process_name == saved_pos.process_name && !used_hwnds.contains(&w.hwnd)
                })
                .count();

            if let Some(matched) = best_match {
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

        for (saved_pos, matched, score) in &mut saved_with_scores {
            if matched.is_none() {
                for current in &current_windows {
                    if !used_hwnds.contains(&current.hwnd)
                        && !saved_pos.process_name.is_empty()
                        && current.process_name == saved_pos.process_name
                    {
                        used_hwnds.insert(current.hwnd);
                        *matched = Some(current);
                        *score = 10;
                        break;
                    }
                }
            }
        }

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
                    Err(format!(
                        "Window '{}' ({}) not found",
                        saved_pos.title, saved_pos.process_name
                    )),
                ));
            }
        }

        results
    }
}

// ============================================
// Non-Windows (Linux) stubs
// ============================================
#[cfg(not(windows))]
mod platform {
    use super::*;

    pub fn get_all_monitors() -> Vec<MonitorLayout> {
        Vec::new()
    }

    pub fn get_all_windows() -> Vec<WindowPosition> {
        Vec::new()
    }

    pub fn match_and_restore_layout(_saved: &SavedLayout) -> Vec<(String, Result<(), String>)> {
        vec![("Desktop Guardian".to_string(), Err("Desktop Guardian is not supported on this platform".to_string()))]
    }
}

// Re-export platform functions
pub fn get_all_monitors() -> Vec<MonitorLayout> {
    platform::get_all_monitors()
}

pub fn get_all_windows() -> Vec<WindowPosition> {
    platform::get_all_windows()
}

pub fn match_and_restore_layout(saved: &SavedLayout) -> Vec<(String, Result<(), String>)> {
    platform::match_and_restore_layout(saved)
}
