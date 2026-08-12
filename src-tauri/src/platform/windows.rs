use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use tauri::AppHandle;

static PREVIOUS_FOREGROUND_WINDOW: AtomicIsize = AtomicIsize::new(0);
static PASTE_HOOK_APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();
static OPEN_QUICK_PASTE_HISTORY: std::sync::OnceLock<fn(&AppHandle) -> Result<(), String>> =
    std::sync::OnceLock::new();
static IS_MULTI_PASTE_OPEN: std::sync::OnceLock<fn(&AppHandle) -> bool> =
    std::sync::OnceLock::new();
static LAST_V_PRESS_MS: AtomicU64 = AtomicU64::new(0);
static CTRL_HELD: AtomicBool = AtomicBool::new(false);
static SHIFT_HELD: AtomicBool = AtomicBool::new(false);
static TARGET_TERMINAL_FOREGROUND: AtomicBool = AtomicBool::new(false);
static MULTI_PASTE_OPEN_CACHED: AtomicBool = AtomicBool::new(false);
static QUICK_PASTE_REQUESTED: AtomicBool = AtomicBool::new(false);
static TABBY_BREAKLINE_REQUESTED: AtomicBool = AtomicBool::new(false);
static PASTE_HOOK_WORKERS_STARTED: AtomicBool = AtomicBool::new(false);

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
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

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

pub fn cursor_and_monitor_info() -> (i32, i32, i32, i32, i32, i32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

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

pub fn snapshot_for_paste_panel() {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    unsafe {
        let hwnd = GetForegroundWindow();
        if !hwnd.0.is_null() {
            PREVIOUS_FOREGROUND_WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
        }
    }
}

pub fn start_focus_tracker() {
    std::thread::spawn(|| {
        use windows::Win32::System::Threading::GetCurrentProcessId;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };

        let my_pid = unsafe { GetCurrentProcessId() };
        loop {
            unsafe {
                let hwnd = GetForegroundWindow();
                if !hwnd.0.is_null() {
                    let mut pid = 0;
                    GetWindowThreadProcessId(hwnd, Some(&mut pid));
                    if pid != 0 {
                        TARGET_TERMINAL_FOREGROUND.store(is_tabby_process(pid), Ordering::Relaxed);
                        if pid != my_pid {
                            PREVIOUS_FOREGROUND_WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
                        }
                    } else {
                        TARGET_TERMINAL_FOREGROUND.store(false, Ordering::Relaxed);
                    }
                } else {
                    TARGET_TERMINAL_FOREGROUND.store(false, Ordering::Relaxed);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
}

pub fn start_paste_hook(
    app: AppHandle,
    open_quick_paste_history: fn(&AppHandle) -> Result<(), String>,
    is_multi_paste_open: fn(&AppHandle) -> bool,
) {
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_CONTROL, VK_LCONTROL, VK_LSHIFT, VK_RCONTROL, VK_RETURN, VK_RSHIFT, VK_SHIFT, VK_V,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG,
        WH_KEYBOARD_LL,
    };

    let _ = PASTE_HOOK_APP.set(app);
    let _ = OPEN_QUICK_PASTE_HISTORY.set(open_quick_paste_history);
    let _ = IS_MULTI_PASTE_OPEN.set(is_multi_paste_open);
    start_paste_hook_workers();

    std::thread::spawn(|| {
        unsafe extern "system" fn keyboard_hook(
            code: i32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            use windows::Win32::UI::WindowsAndMessaging::{CallNextHookEx, HC_ACTION, HHOOK};

            if code == HC_ACTION as i32 {
                let kb = *(lparam.0 as *const KBDLLHOOKSTRUCT);
                let vk = kb.vkCode as u16;
                let key_down = wparam.0 == 0x100 || wparam.0 == 0x104;
                let key_up = wparam.0 == 0x101 || wparam.0 == 0x105;

                if vk == VK_CONTROL.0 || vk == VK_LCONTROL.0 || vk == VK_RCONTROL.0 {
                    CTRL_HELD.store(key_down && !key_up, Ordering::Relaxed);
                }

                if vk == VK_SHIFT.0 || vk == VK_LSHIFT.0 || vk == VK_RSHIFT.0 {
                    SHIFT_HELD.store(key_down && !key_up, Ordering::Relaxed);
                }

                if vk == VK_V.0 && key_down && CTRL_HELD.load(Ordering::Relaxed) {
                    let now_ms = kb.time as u64;
                    let last_ms = LAST_V_PRESS_MS.swap(now_ms, Ordering::Relaxed);
                    let should_open = last_ms > 0 && now_ms.saturating_sub(last_ms) < 500;

                    if should_open && !MULTI_PASTE_OPEN_CACHED.load(Ordering::Relaxed) {
                        LAST_V_PRESS_MS.store(0, Ordering::Relaxed);
                        QUICK_PASTE_REQUESTED.store(true, Ordering::Relaxed);
                        return LRESULT(1);
                    }
                }

                let is_injected = (kb.flags.0 & LLKHF_INJECTED.0) != 0;
                if vk == VK_RETURN.0
                    && key_down
                    && SHIFT_HELD.load(Ordering::Relaxed)
                    && TARGET_TERMINAL_FOREGROUND.load(Ordering::Relaxed)
                    && !is_injected
                {
                    TABBY_BREAKLINE_REQUESTED.store(true, Ordering::Relaxed);
                    return LRESULT(1);
                }
            }

            CallNextHookEx(HHOOK::default(), code, wparam, lparam)
        }

        unsafe {
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0);

            if let Ok(hook) = hook {
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
                let _ = UnhookWindowsHookEx(hook);
            }
        }
    });
}

fn start_paste_hook_workers() {
    if PASTE_HOOK_WORKERS_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    std::thread::spawn(|| loop {
        if let (Some(app), Some(is_open)) = (PASTE_HOOK_APP.get(), IS_MULTI_PASTE_OPEN.get()) {
            MULTI_PASTE_OPEN_CACHED.store(is_open(app), Ordering::Relaxed);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    });

    std::thread::spawn(|| loop {
        if QUICK_PASTE_REQUESTED.swap(false, Ordering::Relaxed) {
            if let (Some(app), Some(open)) = (PASTE_HOOK_APP.get(), OPEN_QUICK_PASTE_HISTORY.get()) {
                if !MULTI_PASTE_OPEN_CACHED.load(Ordering::Relaxed) {
                    let _ = open(app);
                }
            }
        }

        if TABBY_BREAKLINE_REQUESTED.swap(false, Ordering::Relaxed) {
            simulate_tabby_breakline();
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    });
}

fn is_tabby_process(pid: u32) -> bool {
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let Ok(process_handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };

        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let query_ok = QueryFullProcessImageNameW(
            process_handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR::from_raw(buffer.as_mut_ptr()),
            &mut size,
        )
        .is_ok();

        let _ = windows::Win32::Foundation::CloseHandle(process_handle);

        if !query_ok {
            return false;
        }

        let process_name = String::from_utf16_lossy(&buffer[..size as usize]);
        let name_lower = process_name.to_lowercase();
        name_lower.ends_with("tabby.exe") || name_lower.contains("tabby")
    }
}

pub fn paste_into_previous_window(_app: AppHandle) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    let hwnd_val = PREVIOUS_FOREGROUND_WINDOW.load(Ordering::Relaxed);
    if hwnd_val != 0 {
        unsafe {
            let hwnd = HWND(hwnd_val as *mut _);
            let _ = SetForegroundWindow(hwnd);
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    simulate_paste();
}

pub fn simulate_paste() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
    };

    unsafe {
        let inputs = [
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

        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[allow(dead_code)]
pub fn type_text(_text: &str) {}

pub fn simulate_tabby_breakline() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_MENU, VK_RETURN,
    };

    unsafe {
        // We only need to send Alt+Enter since the user is already physically holding Shift.
        let inputs = [
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

        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}
