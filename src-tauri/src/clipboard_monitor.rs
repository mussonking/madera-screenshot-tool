use arboard::Clipboard;
use base64::Engine;
use regex::Regex;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClipboardMonitorError {
    #[error("Failed to access clipboard: {0}")]
    AccessError(String),
    #[error("Monitoring already running")]
    AlreadyRunning,
    #[error("Monitoring not running")]
    NotRunning,
}

#[derive(Debug, Clone)]
pub enum ClipboardContent {
    Text(String),
    Image {
        data: Vec<u8>,
        width: usize,
        height: usize,
    },
    Empty,
}

impl ClipboardContent {
    pub fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        match self {
            ClipboardContent::Text(text) => {
                "text".hash(&mut hasher);
                text.hash(&mut hasher);
            }
            ClipboardContent::Image {
                data,
                width,
                height,
            } => {
                "image".hash(&mut hasher);
                data.hash(&mut hasher);
                width.hash(&mut hasher);
                height.hash(&mut hasher);
            }
            ClipboardContent::Empty => {
                "empty".hash(&mut hasher);
            }
        }
        hasher.finish()
    }
}

#[derive(Debug, Clone)]
pub struct ClipboardSettings {
    pub enabled: bool,
    pub max_items: usize,
    pub excluded_apps: Vec<String>,
    pub auto_cleanup_days: Option<u32>,
}

impl Default for ClipboardSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_items: 200,
            excluded_apps: vec![
                "1Password".to_string(),
                "LastPass".to_string(),
                "Bitwarden".to_string(),
                "KeePass".to_string(),
                "Dashlane".to_string(),
            ],
            auto_cleanup_days: Some(30),
        }
    }
}

pub struct ClipboardMonitor {
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    skip_next: Arc<AtomicBool>,
    last_content_hash: Arc<std::sync::Mutex<u64>>,
    settings: Arc<std::sync::Mutex<ClipboardSettings>>,
}

impl ClipboardMonitor {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            skip_next: Arc::new(AtomicBool::new(false)),
            last_content_hash: Arc::new(std::sync::Mutex::new(0)),
            settings: Arc::new(std::sync::Mutex::new(ClipboardSettings::default())),
        }
    }

    pub fn skip_next_change(&self) {
        self.skip_next.store(true, Ordering::SeqCst);
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn get_settings(&self) -> ClipboardSettings {
        self.settings.lock().unwrap().clone()
    }

    pub fn update_settings(&self, settings: ClipboardSettings) {
        *self.settings.lock().unwrap() = settings;
    }

    pub fn start<F>(&self, callback: F) -> Result<(), ClipboardMonitorError>
    where
        F: Fn(ClipboardContent) + Send + 'static,
    {
        if self.running.load(Ordering::SeqCst) {
            return Err(ClipboardMonitorError::AlreadyRunning);
        }

        self.running.store(true, Ordering::SeqCst);

        let running = Arc::clone(&self.running);
        let paused = Arc::clone(&self.paused);
        let skip_next = Arc::clone(&self.skip_next);
        let last_hash = Arc::clone(&self.last_content_hash);
        let settings = Arc::clone(&self.settings);

        thread::spawn(move || {
            let mut clipboard = match Clipboard::new() {
                Ok(cb) => cb,
                Err(e) => {
                    eprintln!("Failed to create clipboard: {}", e);
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            while running.load(Ordering::SeqCst) {
                let current_settings = settings.lock().unwrap().clone();

                if !current_settings.enabled || paused.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }

                let content = Self::read_clipboard_content(&mut clipboard);
                let content_hash = content.hash();

                let mut last = last_hash.lock().unwrap();
                if content_hash != *last {
                    *last = content_hash;

                    if skip_next.swap(false, Ordering::SeqCst) {
                        drop(last);
                        thread::sleep(Duration::from_millis(300));
                        continue;
                    }

                    // Skip empty content
                    if !matches!(content, ClipboardContent::Empty) {
                        // Check if content should be excluded
                        if !Self::should_exclude(&content, &current_settings) {
                            callback(content);
                        }
                    }
                }
                drop(last);

                // Poll every 300ms - lightweight but responsive
                thread::sleep(Duration::from_millis(300));
            }
        });

        Ok(())
    }

    pub fn stop(&self) -> Result<(), ClipboardMonitorError> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(ClipboardMonitorError::NotRunning);
        }
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn read_clipboard_content(clipboard: &mut Clipboard) -> ClipboardContent {
        // On Wayland, arboard's background-thread clipboard reading is unreliable
        // because the Wayland event loop is not driven from this thread.
        // Use wl-paste (wl-clipboard) instead — it works from any process.
        #[cfg(target_os = "linux")]
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            return Self::read_clipboard_wayland(clipboard);
        }

        // X11 / Windows / macOS: use arboard directly
        Self::read_clipboard_arboard(clipboard)
    }

    #[cfg(target_os = "linux")]
    fn read_clipboard_wayland(clipboard: &mut Clipboard) -> ClipboardContent {
        // Try text via wl-paste (reliable from background threads)
        if let Ok(output) = std::process::Command::new("wl-paste")
            .args(["--no-newline"])
            .output()
        {
            if output.status.success() && !output.stdout.is_empty() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    if !text.is_empty() {
                        return ClipboardContent::Text(text);
                    }
                }
            }
        }

        // wl-paste failed (clipboard has image or is empty) — try arboard for image
        if let Ok(image) = clipboard.get_image() {
            if !image.bytes.is_empty() {
                return ClipboardContent::Image {
                    data: image.bytes.to_vec(),
                    width: image.width,
                    height: image.height,
                };
            }
        }

        ClipboardContent::Empty
    }

    fn read_clipboard_arboard(clipboard: &mut Clipboard) -> ClipboardContent {
        // Try to get text first (most common)
        if let Ok(text) = clipboard.get_text() {
            if !text.is_empty() {
                return ClipboardContent::Text(text);
            }
        }

        // Try to get image
        if let Ok(image) = clipboard.get_image() {
            if !image.bytes.is_empty() {
                return ClipboardContent::Image {
                    data: image.bytes.to_vec(),
                    width: image.width,
                    height: image.height,
                };
            }
        }

        ClipboardContent::Empty
    }

    fn should_exclude(content: &ClipboardContent, _settings: &ClipboardSettings) -> bool {
        if let ClipboardContent::Text(text) = content {
            // Check for sensitive content patterns
            if Self::is_sensitive_content(text) {
                return true;
            }

            // Check if text is too short (likely noise)
            if text.trim().is_empty() {
                return true;
            }
        }

        false
    }

    /// Detect sensitive content that should not be stored
    pub fn is_sensitive_content(text: &str) -> bool {
        // Credit card patterns (various formats)
        let credit_card_pattern = Regex::new(
            r"(?:^|\s)(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12}|(?:2131|1800|35\d{3})\d{11})(?:\s|$)"
        ).unwrap();

        // SSN pattern (US Social Security Number)
        let ssn_pattern = Regex::new(r"(?:^|\s)\d{3}-\d{2}-\d{4}(?:\s|$)").unwrap();

        // Common password manager clipboard patterns
        let password_patterns = [
            // 1Password format
            r"(?i)^[A-Za-z0-9!@#$%^&*()_+\-=\[\]{}|;:',.<>?/`~]{12,}$",
            // UUID-like patterns (often used for API keys)
            r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
            // Base64 encoded secrets (AWS keys, etc.)
            r"^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$",
            // AWS Access Key ID
            r"(?:^|\s)AKIA[0-9A-Z]{16}(?:\s|$)",
            // Private key markers
            r"-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----",
            // Bearer tokens
            r"(?i)bearer\s+[a-z0-9\-_.]+",
        ];

        // Check credit card
        if credit_card_pattern.is_match(text) {
            return true;
        }

        // Check SSN
        if ssn_pattern.is_match(text) {
            return true;
        }

        // Check password patterns
        for pattern in password_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(text) {
                    // Only flag as sensitive if it looks like a password/secret
                    // (high entropy, no spaces, certain length)
                    let trimmed = text.trim();
                    if !trimmed.contains(' ') && trimmed.len() >= 16 && trimmed.len() <= 256 {
                        // Calculate simple entropy check
                        let has_upper = trimmed.chars().any(|c| c.is_uppercase());
                        let has_lower = trimmed.chars().any(|c| c.is_lowercase());
                        let has_digit = trimmed.chars().any(|c| c.is_numeric());
                        let has_special = trimmed.chars().any(|c| !c.is_alphanumeric());

                        let complexity = [has_upper, has_lower, has_digit, has_special]
                            .iter()
                            .filter(|&&x| x)
                            .count();

                        if complexity >= 3 {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Convert RGBA image data to base64 PNG
    pub fn image_to_base64(data: &[u8], width: usize, height: usize) -> Option<String> {
        use image::{ImageBuffer, RgbaImage};
        use std::io::Cursor;

        let img: RgbaImage = ImageBuffer::from_raw(width as u32, height as u32, data.to_vec())?;

        let mut buffer = Cursor::new(Vec::new());
        img.write_to(&mut buffer, image::ImageFormat::Png).ok()?;

        Some(base64::engine::general_purpose::STANDARD.encode(buffer.get_ref()))
    }
}

impl Default for ClipboardMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_content_detection() {
        // Credit cards
        assert!(ClipboardMonitor::is_sensitive_content("4111111111111111"));
        assert!(ClipboardMonitor::is_sensitive_content(
            "5500 0000 0000 0004"
        ));

        // SSN
        assert!(ClipboardMonitor::is_sensitive_content("123-45-6789"));

        // AWS Key
        assert!(ClipboardMonitor::is_sensitive_content(
            "AKIAIOSFODNN7EXAMPLE"
        ));

        // Normal text should pass
        assert!(!ClipboardMonitor::is_sensitive_content("Hello, World!"));
        assert!(!ClipboardMonitor::is_sensitive_content(
            "This is a normal sentence."
        ));
    }

    #[test]
    fn test_content_hash() {
        let text1 = ClipboardContent::Text("hello".to_string());
        let text2 = ClipboardContent::Text("hello".to_string());
        let text3 = ClipboardContent::Text("world".to_string());

        assert_eq!(text1.hash(), text2.hash());
        assert_ne!(text1.hash(), text3.hash());
    }
}
