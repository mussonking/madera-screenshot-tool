use base64::Engine;
use image::{imageops::FilterType, DynamicImage, ImageFormat};
use std::io::Cursor;
use thiserror::Error;
use xcap::Monitor;

use crate::{CaptureResult, MonitorInfo};

#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
#[cfg(windows)]
use windows::Win32::Foundation::POINT;

#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("Failed to capture screen: {0}")]
    CaptureFailure(String),
    #[error("Failed to encode image: {0}")]
    EncodingError(String),
    #[error("Failed to decode image: {0}")]
    DecodingError(String),
    #[error("No monitors found")]
    NoMonitors,
}

pub struct CaptureManager;

impl CaptureManager {
    pub fn new() -> Self {
        Self
    }

    pub fn get_monitors() -> Result<Vec<MonitorInfo>, CaptureError> {
        let monitors = Monitor::all().map_err(|e| CaptureError::CaptureFailure(e.to_string()))?;

        if monitors.is_empty() {
            return Err(CaptureError::NoMonitors);
        }

        Ok(monitors
            .iter()
            .map(|m| MonitorInfo {
                name: m.name().to_string(),
                x: m.x(),
                y: m.y(),
                width: m.width(),
                height: m.height(),
                is_primary: m.is_primary(),
            })
            .collect())
    }

    #[cfg(windows)]
    fn get_cursor_position() -> Option<(i32, i32)> {
        unsafe {
            let mut point = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point).is_ok() {
                Some((point.x, point.y))
            } else {
                None
            }
        }
    }

    #[cfg(not(windows))]
    fn get_cursor_position() -> Option<(i32, i32)> {
        None
    }

    fn find_monitor_at_cursor(monitors: &[Monitor]) -> Option<&Monitor> {
        if let Some((cursor_x, cursor_y)) = Self::get_cursor_position() {
            for monitor in monitors {
                let mx = monitor.x();
                let my = monitor.y();
                let mw = monitor.width() as i32;
                let mh = monitor.height() as i32;

                if cursor_x >= mx && cursor_x < mx + mw && cursor_y >= my && cursor_y < my + mh {
                    return Some(monitor);
                }
            }
        }
        None
    }

    pub fn capture_all_screens(&self) -> Result<Vec<CaptureResult>, CaptureError> {
        let monitors = Monitor::all().map_err(|e| CaptureError::CaptureFailure(e.to_string()))?;

        if monitors.is_empty() {
            return Err(CaptureError::NoMonitors);
        }

        // Find the monitor where the cursor is located
        let target_monitor = Self::find_monitor_at_cursor(&monitors)
            .or_else(|| monitors.iter().find(|m| m.is_primary()))
            .unwrap_or(&monitors[0]);

        // Only capture the monitor where the cursor is
        let image = target_monitor
            .capture_image()
            .map_err(|e| CaptureError::CaptureFailure(e.to_string()))?;

        let width = image.width();
        let height = image.height();

        // Convert to PNG and base64
        let mut buffer = Cursor::new(Vec::new());
        let dynamic_image = DynamicImage::ImageRgba8(image);
        dynamic_image
            .write_to(&mut buffer, ImageFormat::Png)
            .map_err(|e| CaptureError::EncodingError(e.to_string()))?;

        let base64_data = base64::engine::general_purpose::STANDARD.encode(buffer.get_ref());

        Ok(vec![CaptureResult {
            image_data: base64_data,
            width,
            height,
        }])
    }

    pub fn capture_primary_screen(&self) -> Result<CaptureResult, CaptureError> {
        let monitors = Monitor::all().map_err(|e| CaptureError::CaptureFailure(e.to_string()))?;

        let primary = monitors
            .into_iter()
            .find(|m| m.is_primary())
            .ok_or(CaptureError::NoMonitors)?;

        let image = primary
            .capture_image()
            .map_err(|e| CaptureError::CaptureFailure(e.to_string()))?;

        let width = image.width();
        let height = image.height();

        let mut buffer = Cursor::new(Vec::new());
        let dynamic_image = DynamicImage::ImageRgba8(image);
        dynamic_image
            .write_to(&mut buffer, ImageFormat::Png)
            .map_err(|e| CaptureError::EncodingError(e.to_string()))?;

        let base64_data = base64::engine::general_purpose::STANDARD.encode(buffer.get_ref());

        Ok(CaptureResult {
            image_data: base64_data,
            width,
            height,
        })
    }

    pub fn crop_region(
        &self,
        source_base64: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Result<CaptureResult, CaptureError> {
        // Decode base64 to image
        let image_data = base64::engine::general_purpose::STANDARD
            .decode(source_base64)
            .map_err(|e| CaptureError::DecodingError(e.to_string()))?;

        let img = image::load_from_memory(&image_data)
            .map_err(|e| CaptureError::DecodingError(e.to_string()))?;

        // Ensure coordinates are within bounds
        let x = x.max(0) as u32;
        let y = y.max(0) as u32;
        let width = width.min(img.width().saturating_sub(x));
        let height = height.min(img.height().saturating_sub(y));

        // Crop the image
        let cropped = img.crop_imm(x, y, width, height);

        // Encode back to PNG
        let mut buffer = Cursor::new(Vec::new());
        cropped
            .write_to(&mut buffer, ImageFormat::Png)
            .map_err(|e| CaptureError::EncodingError(e.to_string()))?;

        let base64_data = base64::engine::general_purpose::STANDARD.encode(buffer.get_ref());

        Ok(CaptureResult {
            image_data: base64_data,
            width,
            height,
        })
    }

    pub fn resize_image(
        &self,
        source_base64: &str,
        max_width: u32,
    ) -> Result<CaptureResult, CaptureError> {
        let image_data = base64::engine::general_purpose::STANDARD
            .decode(source_base64)
            .map_err(|e| CaptureError::DecodingError(e.to_string()))?;

        let img = image::load_from_memory(&image_data)
            .map_err(|e| CaptureError::DecodingError(e.to_string()))?;

        let (width, height) = if img.width() > max_width {
            let ratio = max_width as f64 / img.width() as f64;
            let new_height = (img.height() as f64 * ratio) as u32;
            (max_width, new_height)
        } else {
            (img.width(), img.height())
        };

        let resized = img.resize(width, height, FilterType::Lanczos3);

        let mut buffer = Cursor::new(Vec::new());
        resized
            .write_to(&mut buffer, ImageFormat::Png)
            .map_err(|e| CaptureError::EncodingError(e.to_string()))?;

        let base64_data = base64::engine::general_purpose::STANDARD.encode(buffer.get_ref());

        Ok(CaptureResult {
            image_data: base64_data,
            width,
            height,
        })
    }
}

impl Default for CaptureManager {
    fn default() -> Self {
        Self::new()
    }
}
