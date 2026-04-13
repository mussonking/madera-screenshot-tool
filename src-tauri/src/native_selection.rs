//! Native Win32 selection overlay - no WebView, no flash, instant display

#[cfg(windows)]
use std::sync::mpsc;
#[cfg(windows)]
use std::thread;
use base64::Engine;
#[cfg(windows)]
use image::{ImageBuffer, Rgba};

#[cfg(windows)]
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::Input::KeyboardAndMouse::*,
    Win32::UI::WindowsAndMessaging::*,
};

/// Extract a region from the screenshot DC and encode as base64 PNG
#[cfg(windows)]
unsafe fn extract_region_as_base64(
    screenshot_dc: HDC,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Option<String> {
    let width_i = width as i32;
    let height_i = height as i32;

    // Create a new DC and bitmap for the cropped region
    let crop_dc = CreateCompatibleDC(screenshot_dc);
    let crop_bitmap = CreateCompatibleBitmap(screenshot_dc, width_i, height_i);
    let old_bitmap = SelectObject(crop_dc, crop_bitmap);

    // Copy the region
    let _ = BitBlt(crop_dc, 0, 0, width_i, height_i, screenshot_dc, x, y, SRCCOPY);

    // Get bitmap info
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width_i,
            biHeight: -height_i, // Negative for top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD::default(); 1],
    };

    // Allocate buffer for pixel data
    let row_size = ((width * 4 + 3) & !3) as usize; // 4 bytes per pixel, aligned to 4 bytes
    let mut pixels: Vec<u8> = vec![0; row_size * height as usize];

    // Get the pixel data
    let result = GetDIBits(
        crop_dc,
        crop_bitmap,
        0,
        height,
        Some(pixels.as_mut_ptr() as *mut _),
        &mut bmi,
        DIB_RGB_COLORS,
    );

    // Cleanup GDI objects
    SelectObject(crop_dc, old_bitmap);
    let _ = DeleteObject(crop_bitmap);
    let _ = DeleteDC(crop_dc);

    if result == 0 {
        return None;
    }

    // Convert BGRA to RGBA
    let mut rgba_pixels: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let offset = y * row_size + x * 4;
            let b = pixels[offset];
            let g = pixels[offset + 1];
            let r = pixels[offset + 2];
            let a = 255u8; // Set alpha to fully opaque
            rgba_pixels.extend_from_slice(&[r, g, b, a]);
        }
    }

    // Create image and encode to PNG
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, rgba_pixels)?;

    let mut png_data: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_data);
    img.write_to(&mut cursor, image::ImageFormat::Png).ok()?;

    Some(base64::engine::general_purpose::STANDARD.encode(&png_data))
}

/// Result of a selection operation
#[derive(Debug, Clone)]
pub struct SelectionResult {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub cancelled: bool,
    /// Base64 encoded PNG of the selected region
    pub image_data: Option<String>,
}

/// State for the selection window
#[cfg(windows)]
struct SelectionState {
    // Screenshot bitmap
    screenshot_dc: HDC,
    screenshot_bitmap: HBITMAP,
    screen_width: i32,
    screen_height: i32,
    // Virtual screen offset (for multi-monitor - can be negative)
    virtual_x: i32,
    virtual_y: i32,

    // Selection coordinates
    is_selecting: bool,
    start_x: i32,
    start_y: i32,
    current_x: i32,
    current_y: i32,

    // Result
    result_sender: Option<mpsc::Sender<SelectionResult>>,
}

#[cfg(windows)]
impl Default for SelectionState {
    fn default() -> Self {
        Self {
            screenshot_dc: HDC::default(),
            screenshot_bitmap: HBITMAP::default(),
            screen_width: 0,
            screen_height: 0,
            virtual_x: 0,
            virtual_y: 0,
            is_selecting: false,
            start_x: 0,
            start_y: 0,
            current_x: 0,
            current_y: 0,
            result_sender: None,
        }
    }
}

#[cfg(windows)]
static mut SELECTION_STATE: Option<SelectionState> = None;

/// Capture the screen and show native selection overlay
/// Returns the selected region or None if cancelled
#[cfg(windows)]
pub fn show_native_selection() -> Option<SelectionResult> {
    let (tx, rx) = mpsc::channel();

    // Run the selection window in a separate thread
    thread::spawn(move || {
        unsafe {
            run_selection_window(tx);
        }
    });

    // Wait for result
    match rx.recv() {
        Ok(result) => {
            if result.cancelled {
                None
            } else {
                Some(result)
            }
        }
        Err(_) => None,
    }
}

#[cfg(windows)]
unsafe fn run_selection_window(result_sender: mpsc::Sender<SelectionResult>) {
    // Get VIRTUAL screen dimensions (all monitors combined)
    let virtual_x = GetSystemMetrics(SM_XVIRTUALSCREEN);  // Can be negative!
    let virtual_y = GetSystemMetrics(SM_YVIRTUALSCREEN);  // Can be negative!
    let screen_width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let screen_height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

    // Capture the entire virtual screen (all monitors)
    let screen_dc = GetDC(HWND::default());
    let mem_dc = CreateCompatibleDC(screen_dc);
    let bitmap = CreateCompatibleBitmap(screen_dc, screen_width, screen_height);
    let old_bitmap = SelectObject(mem_dc, bitmap);

    // Copy entire virtual screen to bitmap (note: source starts at virtual_x, virtual_y)
    let _ = BitBlt(mem_dc, 0, 0, screen_width, screen_height, screen_dc, virtual_x, virtual_y, SRCCOPY);

    ReleaseDC(HWND::default(), screen_dc);

    // Initialize state
    SELECTION_STATE = Some(SelectionState {
        screenshot_dc: mem_dc,
        screenshot_bitmap: bitmap,
        screen_width,
        screen_height,
        virtual_x,
        virtual_y,
        is_selecting: false,
        start_x: 0,
        start_y: 0,
        current_x: 0,
        current_y: 0,
        result_sender: Some(result_sender),
    });

    // Register window class
    let class_name = w!("NativeSelectionOverlay");
    let hmodule = GetModuleHandleW(None).unwrap();
    let hinstance = HINSTANCE(hmodule.0);

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: hinstance,
        hCursor: LoadCursorW(None, IDC_CROSS).unwrap(),
        lpszClassName: class_name,
        ..Default::default()
    };

    RegisterClassExW(&wc);

    // Create fullscreen topmost window covering ALL monitors
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        class_name,
        w!("Selection"),
        WS_POPUP | WS_VISIBLE,
        virtual_x,  // Start at virtual screen origin (can be negative)
        virtual_y,
        screen_width,
        screen_height,
        None,
        None,
        hinstance,
        None,
    ).unwrap();

    // Show and update
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = UpdateWindow(hwnd);
    let _ = SetForegroundWindow(hwnd);

    // Message loop
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).into() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    // Cleanup
    SelectObject(mem_dc, old_bitmap);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(mem_dc);

    let _ = UnregisterClassW(class_name, hinstance);

    SELECTION_STATE = None;
}

#[cfg(windows)]
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            if let Some(state) = &SELECTION_STATE {
                // Draw the screenshot
                let _ = BitBlt(
                    hdc,
                    0, 0,
                    state.screen_width,
                    state.screen_height,
                    state.screenshot_dc,
                    0, 0,
                    SRCCOPY,
                );

                // Draw dark overlay
                let overlay_brush = CreateSolidBrush(COLORREF(0x00000000));
                let full_rect = RECT {
                    left: 0,
                    top: 0,
                    right: state.screen_width,
                    bottom: state.screen_height,
                };

                // Use alpha blending for overlay
                let blend_dc = CreateCompatibleDC(hdc);
                let blend_bitmap = CreateCompatibleBitmap(hdc, state.screen_width, state.screen_height);
                let old_blend = SelectObject(blend_dc, blend_bitmap);

                // Fill with semi-transparent black
                let _ = BitBlt(blend_dc, 0, 0, state.screen_width, state.screen_height, state.screenshot_dc, 0, 0, SRCCOPY);
                FillRect(blend_dc, &full_rect, overlay_brush);

                // Blend
                let blend_func = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 128, // 50% opacity
                    AlphaFormat: 0,
                };
                let _ = GdiAlphaBlend(
                    hdc, 0, 0, state.screen_width, state.screen_height,
                    blend_dc, 0, 0, state.screen_width, state.screen_height,
                    blend_func,
                );

                SelectObject(blend_dc, old_blend);
                let _ = DeleteObject(blend_bitmap);
                let _ = DeleteDC(blend_dc);
                let _ = DeleteObject(overlay_brush);

                // Draw selection rectangle if selecting
                if state.is_selecting {
                    let sel_x = state.start_x.min(state.current_x);
                    let sel_y = state.start_y.min(state.current_y);
                    let sel_w = (state.current_x - state.start_x).abs();
                    let sel_h = (state.current_y - state.start_y).abs();

                    if sel_w > 0 && sel_h > 0 {
                        // Clear selection area to show original image
                        let _ = BitBlt(
                            hdc,
                            sel_x, sel_y,
                            sel_w, sel_h,
                            state.screenshot_dc,
                            sel_x, sel_y,
                            SRCCOPY,
                        );

                        // Draw selection border (red)
                        let pen = CreatePen(PS_SOLID, 2, COLORREF(0x004560E9)); // BGR format - red/pink
                        let old_pen = SelectObject(hdc, pen);
                        let null_brush = GetStockObject(NULL_BRUSH);
                        let old_brush = SelectObject(hdc, null_brush);

                        Rectangle(hdc, sel_x, sel_y, sel_x + sel_w, sel_y + sel_h);

                        SelectObject(hdc, old_pen);
                        SelectObject(hdc, old_brush);
                        let _ = DeleteObject(pen);

                        // Draw dimensions text
                        let dim_text = format!("{} × {}", sel_w, sel_h);
                        let text_wide: Vec<u16> = dim_text.encode_utf16().chain(std::iter::once(0)).collect();

                        // Background for text
                        let text_bg = CreateSolidBrush(COLORREF(0x004560E9));
                        let text_rect = RECT {
                            left: sel_x,
                            top: sel_y - 22,
                            right: sel_x + (dim_text.len() as i32 * 8) + 16,
                            bottom: sel_y - 2,
                        };
                        FillRect(hdc, &text_rect, text_bg);
                        let _ = DeleteObject(text_bg);

                        // Draw text
                        SetBkMode(hdc, TRANSPARENT);
                        SetTextColor(hdc, COLORREF(0x00FFFFFF)); // White
                        TextOutW(hdc, sel_x + 8, sel_y - 20, &text_wide[..text_wide.len()-1]);
                    }
                }

                // Draw instructions at top
                let instructions = "Click and drag to select • ESC to cancel";
                let instr_wide: Vec<u16> = instructions.encode_utf16().chain(std::iter::once(0)).collect();

                let instr_bg = CreateSolidBrush(COLORREF(0x00000000));
                let instr_rect = RECT {
                    left: state.screen_width / 2 - 180,
                    top: 15,
                    right: state.screen_width / 2 + 180,
                    bottom: 45,
                };
                FillRect(hdc, &instr_rect, instr_bg);
                let _ = DeleteObject(instr_bg);

                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(0x00FFFFFF));
                let _ = SetTextAlign(hdc, TA_CENTER);
                TextOutW(hdc, state.screen_width / 2, 20, &instr_wide[..instr_wide.len()-1]);
            }

            EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        WM_LBUTTONDOWN => {
            if let Some(state) = &mut SELECTION_STATE {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

                state.is_selecting = true;
                state.start_x = x;
                state.start_y = y;
                state.current_x = x;
                state.current_y = y;

                SetCapture(hwnd);
            }
            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            if let Some(state) = &mut SELECTION_STATE {
                if state.is_selecting {
                    let x = (lparam.0 & 0xFFFF) as i16 as i32;
                    let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

                    state.current_x = x;
                    state.current_y = y;

                    // Redraw
                    let _ = InvalidateRect(hwnd, None, false);
                }
            }
            LRESULT(0)
        }

        WM_LBUTTONUP => {
            if let Some(state) = &mut SELECTION_STATE {
                if state.is_selecting {
                    let _ = ReleaseCapture();
                    state.is_selecting = false;

                    let sel_x = state.start_x.min(state.current_x);
                    let sel_y = state.start_y.min(state.current_y);
                    let sel_w = (state.current_x - state.start_x).abs();
                    let sel_h = (state.current_y - state.start_y).abs();

                    // Minimum selection size
                    if sel_w > 10 && sel_h > 10 {
                        // Extract the selected region as base64 PNG
                        let image_data = extract_region_as_base64(
                            state.screenshot_dc,
                            sel_x,
                            sel_y,
                            sel_w as u32,
                            sel_h as u32,
                        );

                        if let Some(sender) = state.result_sender.take() {
                            let _ = sender.send(SelectionResult {
                                x: sel_x,
                                y: sel_y,
                                width: sel_w as u32,
                                height: sel_h as u32,
                                cancelled: false,
                                image_data,
                            });
                        }
                        PostQuitMessage(0);
                    }
                }
            }
            LRESULT(0)
        }

        WM_KEYDOWN => {
            if wparam.0 == VK_ESCAPE.0 as usize {
                if let Some(state) = &mut SELECTION_STATE {
                    if let Some(sender) = state.result_sender.take() {
                        let _ = sender.send(SelectionResult {
                            x: 0,
                            y: 0,
                            width: 0,
                            height: 0,
                            cancelled: true,
                            image_data: None,
                        });
                    }
                }
                PostQuitMessage(0);
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

#[cfg(not(windows))]
fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|s| s == "wayland")
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn show_native_selection() -> Option<SelectionResult> {
    eprintln!("[native_selection] Starting screenshot capture");
    eprintln!("[native_selection] Session type: XDG_SESSION_TYPE={:?}", std::env::var("XDG_SESSION_TYPE").ok());

    if is_wayland() {
        // Wayland: use slurp for interactive region selection + xcap for capture
        let slurp_available = std::process::Command::new("which")
            .arg("slurp")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if slurp_available {
            return try_slurp_xcap();
        }
    } else {
        // X11: use slop for interactive region selection + scrot or xcap for capture
        if let Some(result) = try_slop_x11() {
            return Some(result);
        }
    }

    // Fallback: full screen capture
    try_xcap_fullscreen()
}

/// X11: use slop for interactive region selection, then xcap + crop for capture
#[cfg(not(windows))]
fn try_slop_x11() -> Option<SelectionResult> {
    use std::process::Command;
    use xcap::Monitor;

    let slop_available = Command::new("which")
        .arg("slop")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !slop_available {
        eprintln!("[native_selection] slop not installed, falling back to fullscreen");
        return None;
    }

    // slop lets user draw a selection rectangle on X11, returns geometry
    let slop_output = Command::new("slop")
        .args(["-f", "%x %y %w %h"])
        .output()
        .ok()?;

    if !slop_output.status.success() {
        eprintln!("[native_selection] slop cancelled or failed");
        return None; // User cancelled (ESC)
    }

    let output_str = String::from_utf8_lossy(&slop_output.stdout).trim().to_string();
    eprintln!("[native_selection] slop output: {}", output_str);
    let vals: Vec<i32> = output_str.split_whitespace().filter_map(|s| s.parse().ok()).collect();
    if vals.len() != 4 {
        eprintln!("[native_selection] slop returned unexpected format");
        return None;
    }
    let (sel_x, sel_y, sel_w, sel_h) = (vals[0], vals[1], vals[2], vals[3]);

    if sel_w <= 0 || sel_h <= 0 {
        return None;
    }

    // Capture the monitor that contains the selection using xcap
    let monitors = Monitor::all().ok()?;

    let center_x = sel_x + sel_w / 2;
    let center_y = sel_y + sel_h / 2;

    let target_monitor = monitors.iter().find(|m| {
        let mx = m.x();
        let my = m.y();
        let mw = m.width() as i32;
        let mh = m.height() as i32;
        center_x >= mx && center_x < mx + mw && center_y >= my && center_y < my + mh
    }).or_else(|| monitors.iter().find(|m| m.is_primary()))
      .or(monitors.first())?;

    let mon_x = target_monitor.x();
    let mon_y = target_monitor.y();

    let screen_image = target_monitor.capture_image().ok()?;

    // Crop to selection (coordinates relative to monitor)
    let crop_x = (sel_x - mon_x).max(0) as u32;
    let crop_y = (sel_y - mon_y).max(0) as u32;
    let crop_w = (sel_w as u32).min(screen_image.width().saturating_sub(crop_x));
    let crop_h = (sel_h as u32).min(screen_image.height().saturating_sub(crop_y));

    if crop_w == 0 || crop_h == 0 {
        return None;
    }

    let dynamic_image = image::DynamicImage::ImageRgba8(screen_image);
    let cropped = dynamic_image.crop_imm(crop_x, crop_y, crop_w, crop_h);

    let mut buffer = std::io::Cursor::new(Vec::new());
    cropped.write_to(&mut buffer, image::ImageFormat::Png).ok()?;

    let base64_data = base64::engine::general_purpose::STANDARD.encode(buffer.get_ref());

    eprintln!("[native_selection] X11 capture OK: {}x{}", crop_w, crop_h);

    Some(SelectionResult {
        x: sel_x,
        y: sel_y,
        width: crop_w,
        height: crop_h,
        cancelled: false,
        image_data: Some(base64_data),
    })
}

/// Wayland: use slurp for interactive region selection, then xcap + crop for the actual capture
#[cfg(not(windows))]
fn try_slurp_xcap() -> Option<SelectionResult> {
    use std::process::Command;
    use xcap::Monitor;

    // slurp lets user draw a selection rectangle, returns geometry
    let slurp_output = Command::new("slurp")
        .arg("-f")
        .arg("%x %y %w %h")
        .output()
        .ok()?;

    if !slurp_output.status.success() {
        return None; // User cancelled (ESC) or slurp not available
    }

    let output_str = String::from_utf8_lossy(&slurp_output.stdout).trim().to_string();
    let vals: Vec<i32> = output_str.split_whitespace().filter_map(|s| s.parse().ok()).collect();
    if vals.len() != 4 {
        return None;
    }
    let (sel_x, sel_y, sel_w, sel_h) = (vals[0], vals[1], vals[2], vals[3]);

    if sel_w <= 0 || sel_h <= 0 {
        return None;
    }

    // Capture the monitor that contains the selection using xcap
    let monitors = Monitor::all().ok()?;

    // Find which monitor contains the selection center
    let center_x = sel_x + sel_w / 2;
    let center_y = sel_y + sel_h / 2;

    let target_monitor = monitors.iter().find(|m| {
        let mx = m.x();
        let my = m.y();
        let mw = m.width() as i32;
        let mh = m.height() as i32;
        center_x >= mx && center_x < mx + mw && center_y >= my && center_y < my + mh
    }).or_else(|| monitors.iter().find(|m| m.is_primary()))
      .or(monitors.first())?;

    let mon_x = target_monitor.x();
    let mon_y = target_monitor.y();

    let screen_image = target_monitor.capture_image().ok()?;

    // Crop to selection (coordinates relative to monitor)
    let crop_x = (sel_x - mon_x).max(0) as u32;
    let crop_y = (sel_y - mon_y).max(0) as u32;
    let crop_w = (sel_w as u32).min(screen_image.width().saturating_sub(crop_x));
    let crop_h = (sel_h as u32).min(screen_image.height().saturating_sub(crop_y));

    if crop_w == 0 || crop_h == 0 {
        return None;
    }

    let dynamic_image = image::DynamicImage::ImageRgba8(screen_image);
    let cropped = dynamic_image.crop_imm(crop_x, crop_y, crop_w, crop_h);

    let mut buffer = std::io::Cursor::new(Vec::new());
    cropped.write_to(&mut buffer, image::ImageFormat::Png).ok()?;

    let base64_data = base64::engine::general_purpose::STANDARD.encode(buffer.get_ref());

    Some(SelectionResult {
        x: sel_x,
        y: sel_y,
        width: crop_w,
        height: crop_h,
        cancelled: false,
        image_data: Some(base64_data),
    })
}

#[cfg(not(windows))]
fn try_xcap_fullscreen() -> Option<SelectionResult> {
    use xcap::Monitor;

    let monitors = Monitor::all().ok()?;
    let primary = monitors
        .into_iter()
        .find(|m| m.is_primary())
        .or_else(|| Monitor::all().ok().and_then(|m| m.into_iter().next()))?;

    let image = primary.capture_image().ok()?;
    let width = image.width();
    let height = image.height();

    let dynamic_image = image::DynamicImage::ImageRgba8(image);
    let mut buffer = std::io::Cursor::new(Vec::new());
    dynamic_image
        .write_to(&mut buffer, image::ImageFormat::Png)
        .ok()?;

    let base64_data = base64::engine::general_purpose::STANDARD.encode(buffer.get_ref());

    Some(SelectionResult {
        x: 0,
        y: 0,
        width,
        height,
        cancelled: false,
        image_data: Some(base64_data),
    })
}
