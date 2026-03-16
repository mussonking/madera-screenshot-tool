use arboard::{Clipboard, ImageData};
use base64::Engine;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClipboardError {
    #[error("Failed to access clipboard: {0}")]
    AccessError(String),
    #[error("Failed to decode image: {0}")]
    DecodeError(String),
    #[error("Failed to copy to clipboard: {0}")]
    CopyError(String),
}

pub struct ClipboardManager;

impl ClipboardManager {
    pub fn new() -> Self {
        Self
    }

    pub fn copy_image_to_clipboard(&self, base64_image: &str) -> Result<(), ClipboardError> {
        // Decode base64 to bytes
        let image_bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_image)
            .map_err(|e| ClipboardError::DecodeError(e.to_string()))?;

        // Load image and convert to RGBA
        let img = image::load_from_memory(&image_bytes)
            .map_err(|e| ClipboardError::DecodeError(e.to_string()))?;

        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        let rgba_raw = rgba.into_raw();

        let mut clipboard =
            Clipboard::new().map_err(|e| ClipboardError::AccessError(e.to_string()))?;

        let mut last_err = String::new();
        // Fast retry loop: 20 tries x 25ms = 500ms max delay for lock contention evasion
        for _ in 0..20 {
            let image_data = ImageData {
                width: width as usize,
                height: height as usize,
                bytes: std::borrow::Cow::Borrowed(&rgba_raw),
            };

            match clipboard.set_image(image_data) {
                Ok(_) => return Ok(()),
                Err(e) => {
                    last_err = e.to_string();
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
            }
        }

        Err(ClipboardError::CopyError(format!(
            "Failed after retries: {}",
            last_err
        )))
    }

    pub fn copy_text_to_clipboard(&self, text: &str) -> Result<(), ClipboardError> {
        let mut clipboard =
            Clipboard::new().map_err(|e| ClipboardError::AccessError(e.to_string()))?;

        let mut last_err = String::new();
        for _ in 0..20 {
            match clipboard.set_text(text) {
                Ok(_) => return Ok(()),
                Err(e) => {
                    last_err = e.to_string();
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
            }
        }

        Err(ClipboardError::CopyError(format!(
            "Failed after retries: {}",
            last_err
        )))
    }
}

impl Default for ClipboardManager {
    fn default() -> Self {
        Self::new()
    }
}
