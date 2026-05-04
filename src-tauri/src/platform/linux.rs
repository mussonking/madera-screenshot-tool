use tauri::{AppHandle, Manager};

pub fn is_wayland_session() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|v| v == "wayland")
        .unwrap_or(false)
}

pub fn apply_layer_shell_overlay(window: &tauri::WebviewWindow, width: i32) {
    if !is_wayland_session() {
        return;
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
            w.set_namespace("madera-ss-overlay");
            w.set_keyboard_interactivity(false);
            gtk_win.set_size_request(width, -1);
        }
    });
}

pub fn toggle_panel_side(app: &AppHandle, window_label: &str) -> Result<String, String> {
    if !is_wayland_session() {
        return Ok("right".to_string());
    }

    let window = app
        .get_webview_window(window_label)
        .ok_or("Window not found")?;
    let win = window.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    window
        .run_on_main_thread(move || {
            if let Ok(gtk_win) = win.gtk_window() {
                use gtk::prelude::*;
                use gtk_layer_shell::LayerShell;

                let w = gtk_win.upcast_ref::<gtk::Window>();
                let is_right = w.is_anchor(gtk_layer_shell::Edge::Right);
                w.set_anchor(gtk_layer_shell::Edge::Right, !is_right);
                w.set_anchor(gtk_layer_shell::Edge::Left, is_right);
                let _ = tx.send(if is_right { "left" } else { "right" });
            }
        })
        .map_err(|e| e.to_string())?;

    rx.recv().map(|s| s.to_string()).map_err(|e| e.to_string())
}

pub fn toggle_panel_monitor(app: &AppHandle, window_label: &str) -> Result<(), String> {
    if !is_wayland_session() {
        return Ok(());
    }

    let window = app
        .get_webview_window(window_label)
        .ok_or("Window not found")?;
    let win = window.clone();

    window
        .run_on_main_thread(move || {
            if let Ok(gtk_win) = win.gtk_window() {
                use gtk::prelude::*;
                use gtk_layer_shell::LayerShell;

                let w = gtk_win.upcast_ref::<gtk::Window>();
                let display = match gdk::Display::default() {
                    Some(d) => d,
                    None => return,
                };
                let n = display.n_monitors();
                if n <= 1 {
                    return;
                }
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
        })
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn set_panel_keyboard(
    app: &AppHandle,
    window_label: &str,
    enabled: bool,
) -> Result<(), String> {
    if !is_wayland_session() {
        return Ok(());
    }

    let window = app
        .get_webview_window(window_label)
        .ok_or("Window not found")?;
    let win = window.clone();

    window
        .run_on_main_thread(move || {
            if let Ok(gtk_win) = win.gtk_window() {
                use gtk::prelude::*;
                use gtk_layer_shell::LayerShell;

                let w = gtk_win.upcast_ref::<gtk::Window>();
                w.set_keyboard_interactivity(enabled);
            }
        })
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn active_window_geometry() -> Option<(i32, i32, i32, i32)> {
    let output = std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowgeometry", "--shell"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let geom = String::from_utf8_lossy(&output.stdout);
    let parse = |key: &str| -> i32 {
        geom.lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split('=').nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };

    Some((parse("X"), parse("Y"), parse("WIDTH"), parse("HEIGHT")))
}

pub fn page_down_active_window() {
    let _ = std::process::Command::new("xdotool")
        .args(["key", "Page_Down"])
        .output();
}

pub fn work_area_near_cursor() -> (i32, i32, i32, i32) {
    if let Ok(monitors) = xcap::Monitor::all() {
        if let Some(m) = monitors
            .iter()
            .find(|m| m.is_primary())
            .or(monitors.first())
        {
            return (m.x(), m.y(), m.width() as i32, m.height() as i32);
        }
    }

    (0, 0, 1920, 1080)
}

pub fn cursor_and_monitor_info() -> (i32, i32, i32, i32, i32, i32) {
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
                display,
                root,
                &mut root_return,
                &mut child_return,
                &mut root_x,
                &mut root_y,
                &mut win_x,
                &mut win_y,
                &mut mask,
            );
            x11::xlib::XCloseDisplay(display);
            if result != 0 {
                (root_x, root_y)
            } else {
                (100, 100)
            }
        }
    };

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

        if let Some(m) = monitors.first() {
            return (
                cursor_x,
                cursor_y,
                m.x(),
                m.y(),
                m.width() as i32,
                m.height() as i32,
            );
        }
    }

    (cursor_x, cursor_y, 0, 0, 1920, 1080)
}

pub fn snapshot_for_paste_panel() {
    crate::wayland_focus::snapshot_for_paste();

    if !is_wayland_session() {
        if let Ok(output) = std::process::Command::new("xdotool")
            .arg("getactivewindow")
            .output()
        {
            if let Ok(id_str) = String::from_utf8(output.stdout) {
                if let Ok(wid) = id_str.trim().parse::<u64>() {
                    crate::wayland_focus::set_x11_window_id(wid);
                }
            }
        }
    }
}

pub fn start_focus_tracker() {
    crate::wayland_focus::init();
}

pub fn start_paste_hook(
    _app: AppHandle,
    _open_quick_paste_history: fn(&AppHandle) -> Result<(), String>,
    _is_multi_paste_open: fn(&AppHandle) -> bool,
) {
}

pub fn paste_into_previous_window(app: AppHandle) {
    if !is_wayland_session() {
        for label in &["quickpaste", "multipaste"] {
            if let Some(w) = app.get_webview_window(label) {
                let _ = w.hide();
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    crate::wayland_focus::activate_last_focused();
    std::thread::sleep(std::time::Duration::from_millis(350));
    simulate_paste();

    if !is_wayland_session() {
        for label in &["quickpaste", "multipaste"] {
            if let Some(w) = app.get_webview_window(label) {
                let _ = w.close();
            }
        }
    }
}

fn is_active_window_terminal() -> bool {
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
        "wezterm",
        "gnome-terminal",
        "kitty",
        "alacritty",
        "xterm",
        "konsole",
        "tilix",
        "terminator",
        "xfce4-terminal",
        "sakura",
        "st-256color",
        "urxvt",
        "foot",
        "blackbox",
    ];
    TERMINALS.iter().any(|t| class.contains(t))
}

pub fn simulate_paste() {
    let use_terminal_paste = is_active_window_terminal();
    if use_terminal_paste {
        eprintln!("[paste] Terminal detected, using Shift+Ctrl+V");
    }

    let key = if use_terminal_paste {
        "shift+ctrl+v"
    } else {
        "ctrl+v"
    };

    let ok = std::process::Command::new("ydotool")
        .args(["key", "--delay", "0", key])
        .spawn()
        .is_ok();

    if !ok {
        if use_terminal_paste {
            let ok2 = std::process::Command::new("wtype")
                .args([
                    "-M", "shift", "-M", "ctrl", "-P", "v", "-p", "v", "-m", "ctrl", "-m", "shift",
                ])
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

#[allow(dead_code)]
pub fn type_text(text: &str) {
    let clean = text.replace('\r', "");
    let text = clean.as_str();

    let ok = std::process::Command::new("ydotool")
        .args(["type", "--delay", "0", "--key-delay", "2", "--", text])
        .spawn()
        .is_ok();
    if !ok {
        let ok2 = std::process::Command::new("wtype")
            .arg("--")
            .arg(text)
            .spawn()
            .is_ok();
        if !ok2 {
            let _ = std::process::Command::new("xdotool")
                .args(["type", "--clearmodifiers", "--", text])
                .spawn();
        }
    }
}

#[allow(dead_code)]
pub fn simulate_tabby_breakline() {}
