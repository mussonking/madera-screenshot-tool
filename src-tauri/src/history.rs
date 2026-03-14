use base64::Engine;
use chrono::{DateTime, Utc};
use image::ImageFormat;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum HistoryError {
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("File system error: {0}")]
    FileSystemError(String),
    #[error("Image processing error: {0}")]
    ImageError(String),
    #[error("Duplicate content")]
    DuplicateContent,
}

/// Calculate a hash for content (used for duplicate detection)
fn calculate_content_hash(data: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

impl From<rusqlite::Error> for HistoryError {
    fn from(e: rusqlite::Error) -> Self {
        HistoryError::DatabaseError(e.to_string())
    }
}

impl From<std::io::Error> for HistoryError {
    fn from(e: std::io::Error) -> Self {
        HistoryError::FileSystemError(e.to_string())
    }
}

/// Type of history item
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryItemType {
    Screenshot,
    ClipboardText,
    ClipboardImage,
    ColorPick,
}

impl std::fmt::Display for HistoryItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HistoryItemType::Screenshot => write!(f, "screenshot"),
            HistoryItemType::ClipboardText => write!(f, "clipboard_text"),
            HistoryItemType::ClipboardImage => write!(f, "clipboard_image"),
            HistoryItemType::ColorPick => write!(f, "color_pick"),
        }
    }
}

impl std::str::FromStr for HistoryItemType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "screenshot" => Ok(HistoryItemType::Screenshot),
            "clipboard_text" => Ok(HistoryItemType::ClipboardText),
            "clipboard_image" => Ok(HistoryItemType::ClipboardImage),
            "color_pick" => Ok(HistoryItemType::ColorPick),
            _ => Err(format!("Unknown item type: {}", s)),
        }
    }
}

/// Unified history record for screenshots and clipboard items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,
    pub item_type: HistoryItemType,
    pub created_at: String,
    // For screenshots and clipboard images
    pub filename: Option<String>,
    pub thumbnail: Option<String>, // Base64 encoded thumbnail
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub saved_path: Option<String>,
    // For clipboard text
    pub text_content: Option<String>,
    pub text_preview: Option<String>, // First 100 chars
    // For color picks
    pub color_hex: Option<String>,
    pub color_rgb: Option<String>, // "r,g,b" format
    pub color_hsl: Option<String>, // "h,s,l" format
    // Metadata
    pub source_app: Option<String>,
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotRecord {
    pub id: String,
    pub filename: String,
    pub thumbnail: String, // Base64 encoded thumbnail
    pub created_at: String,
    pub width: u32,
    pub height: u32,
    pub saved_path: Option<String>,
}

pub struct HistoryManager {
    conn: Connection,
    data_dir: PathBuf,
    screenshots_dir: PathBuf,
    thumbnails_dir: PathBuf,
    clipboard_dir: PathBuf,
}

impl HistoryManager {
    pub fn new() -> Result<Self, HistoryError> {
        // Get app data directory
        let data_dir = dirs::data_local_dir()
            .ok_or_else(|| HistoryError::FileSystemError("Cannot find app data dir".into()))?
            .join("screenshot-tool");

        let screenshots_dir = data_dir.join("screenshots");
        let thumbnails_dir = data_dir.join("thumbnails");
        let clipboard_dir = data_dir.join("clipboard");

        // Create directories
        fs::create_dir_all(&screenshots_dir)?;
        fs::create_dir_all(&thumbnails_dir)?;
        fs::create_dir_all(&clipboard_dir)?;

        // Open database
        let db_path = data_dir.join("history.db");
        let conn = Connection::open(&db_path)?;

        // Increase concurrency reliability to fix 'database is locked' errors
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        // Create screenshots table (legacy)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS screenshots (
                id TEXT PRIMARY KEY,
                filename TEXT NOT NULL,
                thumbnail_filename TEXT NOT NULL,
                created_at TEXT NOT NULL,
                width INTEGER NOT NULL,
                height INTEGER NOT NULL,
                saved_path TEXT
            )",
            [],
        )?;

        // Create unified history table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS history_items (
                id TEXT PRIMARY KEY,
                item_type TEXT NOT NULL,
                created_at TEXT NOT NULL,
                filename TEXT,
                thumbnail_filename TEXT,
                width INTEGER,
                height INTEGER,
                saved_path TEXT,
                text_content TEXT,
                text_preview TEXT,
                color_hex TEXT,
                color_rgb TEXT,
                color_hsl TEXT,
                source_app TEXT,
                is_pinned INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Add color columns if they don't exist (migration for existing DBs)
        let _ = conn.execute("ALTER TABLE history_items ADD COLUMN color_hex TEXT", []);
        let _ = conn.execute("ALTER TABLE history_items ADD COLUMN color_rgb TEXT", []);
        let _ = conn.execute("ALTER TABLE history_items ADD COLUMN color_hsl TEXT", []);

        // Add content_hash column for duplicate detection
        let _ = conn.execute("ALTER TABLE history_items ADD COLUMN content_hash TEXT", []);

        // Create index for faster queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_history_created_at ON history_items(created_at DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_history_type ON history_items(item_type)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_history_content_hash ON history_items(content_hash)",
            [],
        )?;

        Ok(Self {
            conn,
            data_dir,
            screenshots_dir,
            thumbnails_dir,
            clipboard_dir,
        })
    }

    /// Check if content with this hash already exists in history
    fn content_hash_exists(&self, hash: &str) -> Result<bool, HistoryError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM history_items WHERE content_hash = ?1",
            params![hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Get the most recent item with this hash (to return existing item instead of creating duplicate)
    fn get_item_by_hash(&self, hash: &str) -> Result<Option<HistoryItem>, HistoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, item_type, created_at, filename, thumbnail_filename, width, height, saved_path,
                    text_content, text_preview, color_hex, color_rgb, color_hsl, source_app, is_pinned
             FROM history_items WHERE content_hash = ?1 ORDER BY created_at DESC LIMIT 1",
        )?;

        let item = stmt
            .query_row(params![hash], |row| self.row_to_history_item(row))
            .optional()?;

        Ok(item)
    }

    pub fn save_screenshot(
        &mut self,
        base64_image: &str,
        width: u32,
        height: u32,
        max_history: usize,
    ) -> Result<ScreenshotRecord, HistoryError> {
        let id = Uuid::new_v4().to_string();
        let now: DateTime<Utc> = Utc::now();
        let timestamp = now.format("%Y-%m-%d_%H%M%S").to_string();
        let filename = format!("{}_{}.png", timestamp, &id[..8]);
        let thumbnail_filename = format!("thumb_{}_{}.jpg", timestamp, &id[..8]);

        // Decode image
        let image_bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_image)
            .map_err(|e| HistoryError::ImageError(e.to_string()))?;

        // Save full image
        let image_path = self.screenshots_dir.join(&filename);
        fs::write(&image_path, &image_bytes)?;

        // Generate and save thumbnail
        let img = image::load_from_memory(&image_bytes)
            .map_err(|e| HistoryError::ImageError(e.to_string()))?;

        let thumbnail = img.thumbnail(200, 200);
        let thumbnail_path = self.thumbnails_dir.join(&thumbnail_filename);

        let mut thumb_buffer = Cursor::new(Vec::new());
        thumbnail
            .write_to(&mut thumb_buffer, ImageFormat::Jpeg)
            .map_err(|e| HistoryError::ImageError(e.to_string()))?;

        fs::write(&thumbnail_path, thumb_buffer.get_ref())?;

        // Base64 encode thumbnail for quick display
        let thumbnail_base64 =
            base64::engine::general_purpose::STANDARD.encode(thumb_buffer.get_ref());

        // Insert into database
        let created_at = now.to_rfc3339();
        self.conn.execute(
            "INSERT INTO screenshots (id, filename, thumbnail_filename, created_at, width, height) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, filename, thumbnail_filename, created_at, width, height],
        )?;

        // Clean up old screenshots if over limit
        self.cleanup_old_screenshots(max_history)?;

        Ok(ScreenshotRecord {
            id,
            filename,
            thumbnail: thumbnail_base64,
            created_at,
            width,
            height,
            saved_path: None,
        })
    }

    pub fn get_all_screenshots(&self) -> Result<Vec<ScreenshotRecord>, HistoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, filename, thumbnail_filename, created_at, width, height, saved_path FROM screenshots ORDER BY created_at DESC",
        )?;

        let records = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let filename: String = row.get(1)?;
            let thumbnail_filename: String = row.get(2)?;
            let created_at: String = row.get(3)?;
            let width: u32 = row.get(4)?;
            let height: u32 = row.get(5)?;
            let saved_path: Option<String> = row.get(6)?;

            // Load thumbnail from file
            let thumbnail_path = self.thumbnails_dir.join(&thumbnail_filename);
            let thumbnail = if thumbnail_path.exists() {
                fs::read(&thumbnail_path)
                    .map(|bytes| base64::engine::general_purpose::STANDARD.encode(&bytes))
                    .unwrap_or_default()
            } else {
                String::new()
            };

            Ok(ScreenshotRecord {
                id,
                filename,
                thumbnail,
                created_at,
                width,
                height,
                saved_path,
            })
        })?;

        records
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| HistoryError::DatabaseError(e.to_string()))
    }

    pub fn get_screenshot_image(&self, id: &str) -> Result<Option<String>, HistoryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT filename FROM screenshots WHERE id = ?1")?;

        let filename: Option<String> = stmt.query_row(params![id], |row| row.get(0)).ok();

        if let Some(filename) = filename {
            let image_path = self.screenshots_dir.join(&filename);
            if image_path.exists() {
                let bytes = fs::read(&image_path)?;
                let base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                return Ok(Some(base64));
            }
        }

        Ok(None)
    }

    pub fn delete_screenshot(&mut self, id: &str) -> Result<(), HistoryError> {
        // Get filenames first
        let mut stmt = self
            .conn
            .prepare("SELECT filename, thumbnail_filename FROM screenshots WHERE id = ?1")?;

        let result: Option<(String, String)> = stmt
            .query_row(params![id], |row| Ok((row.get(0)?, row.get(1)?)))
            .ok();

        if let Some((filename, thumbnail_filename)) = result {
            // Delete files
            let image_path = self.screenshots_dir.join(&filename);
            let thumbnail_path = self.thumbnails_dir.join(&thumbnail_filename);

            let _ = fs::remove_file(image_path);
            let _ = fs::remove_file(thumbnail_path);
        }

        // Delete from database
        self.conn
            .execute("DELETE FROM screenshots WHERE id = ?1", params![id])?;

        Ok(())
    }

    pub fn clear_all(&mut self) -> Result<(), HistoryError> {
        // Delete all files
        if self.screenshots_dir.exists() {
            for entry in fs::read_dir(&self.screenshots_dir)? {
                if let Ok(entry) = entry {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }

        if self.thumbnails_dir.exists() {
            for entry in fs::read_dir(&self.thumbnails_dir)? {
                if let Ok(entry) = entry {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }

        // Clear database
        self.conn.execute("DELETE FROM screenshots", [])?;

        Ok(())
    }

    fn cleanup_old_screenshots(&mut self, max_count: usize) -> Result<(), HistoryError> {
        // Get count
        let count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM screenshots", [], |row| row.get(0))?;

        if count > max_count {
            // Get oldest screenshots to delete
            let to_delete = count - max_count;

            let mut stmt = self.conn.prepare(
                "SELECT id, filename, thumbnail_filename FROM screenshots ORDER BY created_at ASC LIMIT ?1",
            )?;

            let old_records: Vec<(String, String, String)> = stmt
                .query_map(params![to_delete], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .filter_map(|r| r.ok())
                .collect();

            for (id, filename, thumbnail_filename) in old_records {
                // Delete files
                let image_path = self.screenshots_dir.join(&filename);
                let thumbnail_path = self.thumbnails_dir.join(&thumbnail_filename);

                let _ = fs::remove_file(image_path);
                let _ = fs::remove_file(thumbnail_path);

                // Delete from database
                self.conn
                    .execute("DELETE FROM screenshots WHERE id = ?1", params![id])?;
            }
        }

        Ok(())
    }

    pub fn update_saved_path(&mut self, id: &str, path: &str) -> Result<(), HistoryError> {
        self.conn.execute(
            "UPDATE screenshots SET saved_path = ?1 WHERE id = ?2",
            params![path, id],
        )?;
        Ok(())
    }

    // ============================================
    // Unified History Methods (screenshots + clipboard)
    // ============================================

    /// Save a clipboard text item to history
    pub fn save_clipboard_text(
        &mut self,
        text: &str,
        source_app: Option<&str>,
        max_history: usize,
    ) -> Result<HistoryItem, HistoryError> {
        // Calculate content hash for duplicate detection
        let content_hash = calculate_content_hash(text.as_bytes());

        // Check if this exact text already exists
        if let Some(existing_item) = self.get_item_by_hash(&content_hash)? {
            // Return existing item instead of creating duplicate
            return Ok(existing_item);
        }

        let id = Uuid::new_v4().to_string();
        let now: DateTime<Utc> = Utc::now();
        let created_at = now.to_rfc3339();

        // Create preview (first 100 chars, trimmed)
        let text_preview = if text.len() > 100 {
            format!("{}...", text.chars().take(100).collect::<String>().trim())
        } else {
            text.trim().to_string()
        };

        self.conn.execute(
            "INSERT INTO history_items (id, item_type, created_at, text_content, text_preview, source_app, is_pinned, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
            params![
                id,
                HistoryItemType::ClipboardText.to_string(),
                created_at,
                text,
                text_preview,
                source_app,
                content_hash,
            ],
        )?;

        // Cleanup old items
        self.cleanup_old_history_items(max_history)?;

        Ok(HistoryItem {
            id,
            item_type: HistoryItemType::ClipboardText,
            created_at,
            filename: None,
            thumbnail: None,
            width: None,
            height: None,
            saved_path: None,
            text_content: Some(text.to_string()),
            text_preview: Some(text_preview),
            color_hex: None,
            color_rgb: None,
            color_hsl: None,
            source_app: source_app.map(|s| s.to_string()),
            is_pinned: false,
        })
    }

    /// Save a clipboard image to history
    pub fn save_clipboard_image(
        &mut self,
        base64_image: &str,
        width: u32,
        height: u32,
        source_app: Option<&str>,
        max_history: usize,
    ) -> Result<HistoryItem, HistoryError> {
        // Decode image first to calculate hash
        let image_bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_image)
            .map_err(|e| HistoryError::ImageError(e.to_string()))?;

        // Calculate content hash for duplicate detection
        let content_hash = calculate_content_hash(&image_bytes);

        // Check if this exact image already exists
        if let Some(existing_item) = self.get_item_by_hash(&content_hash)? {
            // Return existing item instead of creating duplicate
            return Ok(existing_item);
        }

        let id = Uuid::new_v4().to_string();
        let now: DateTime<Utc> = Utc::now();
        let timestamp = now.format("%Y-%m-%d_%H%M%S").to_string();
        let filename = format!("clip_{}_{}.png", timestamp, &id[..8]);
        let thumbnail_filename = format!("clip_thumb_{}_{}.jpg", timestamp, &id[..8]);

        // Save full image to clipboard directory
        let image_path = self.clipboard_dir.join(&filename);
        fs::write(&image_path, &image_bytes)?;

        // Generate and save thumbnail
        let img = image::load_from_memory(&image_bytes)
            .map_err(|e| HistoryError::ImageError(e.to_string()))?;

        let thumbnail = img.thumbnail(200, 200);
        let thumbnail_path = self.thumbnails_dir.join(&thumbnail_filename);

        let mut thumb_buffer = Cursor::new(Vec::new());
        thumbnail
            .write_to(&mut thumb_buffer, ImageFormat::Jpeg)
            .map_err(|e| HistoryError::ImageError(e.to_string()))?;

        fs::write(&thumbnail_path, thumb_buffer.get_ref())?;

        let thumbnail_base64 =
            base64::engine::general_purpose::STANDARD.encode(thumb_buffer.get_ref());

        let created_at = now.to_rfc3339();
        self.conn.execute(
            "INSERT INTO history_items (id, item_type, created_at, filename, thumbnail_filename, width, height, source_app, is_pinned, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)",
            params![
                id,
                HistoryItemType::ClipboardImage.to_string(),
                created_at,
                filename,
                thumbnail_filename,
                width,
                height,
                source_app,
                content_hash,
            ],
        )?;

        // Cleanup old items
        self.cleanup_old_history_items(max_history)?;

        Ok(HistoryItem {
            id,
            item_type: HistoryItemType::ClipboardImage,
            created_at,
            filename: Some(filename),
            thumbnail: Some(thumbnail_base64),
            width: Some(width),
            height: Some(height),
            saved_path: None,
            text_content: None,
            text_preview: None,
            color_hex: None,
            color_rgb: None,
            color_hsl: None,
            source_app: source_app.map(|s| s.to_string()),
            is_pinned: false,
        })
    }

    /// Save a color pick to history
    pub fn save_color_pick(
        &mut self,
        hex: &str,
        rgb: (u8, u8, u8),
        hsl: (u16, u8, u8),
        source_app: Option<&str>,
        max_history: usize,
    ) -> Result<HistoryItem, HistoryError> {
        let id = Uuid::new_v4().to_string();
        let now: DateTime<Utc> = Utc::now();
        let created_at = now.to_rfc3339();

        let color_rgb = format!("{},{},{}", rgb.0, rgb.1, rgb.2);
        let color_hsl = format!("{},{},{}", hsl.0, hsl.1, hsl.2);

        self.conn.execute(
            "INSERT INTO history_items (id, item_type, created_at, color_hex, color_rgb, color_hsl, source_app, is_pinned)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            params![
                id,
                HistoryItemType::ColorPick.to_string(),
                created_at,
                hex,
                color_rgb,
                color_hsl,
                source_app,
            ],
        )?;

        // Cleanup old items
        self.cleanup_old_history_items(max_history)?;

        Ok(HistoryItem {
            id,
            item_type: HistoryItemType::ColorPick,
            created_at,
            filename: None,
            thumbnail: None,
            width: None,
            height: None,
            saved_path: None,
            text_content: None,
            text_preview: None,
            color_hex: Some(hex.to_string()),
            color_rgb: Some(color_rgb),
            color_hsl: Some(color_hsl),
            source_app: source_app.map(|s| s.to_string()),
            is_pinned: false,
        })
    }

    /// Save a screenshot to the unified history table
    pub fn save_screenshot_to_unified(
        &mut self,
        base64_image: &str,
        width: u32,
        height: u32,
        max_history: usize,
    ) -> Result<HistoryItem, HistoryError> {
        // Decode image first to calculate hash for duplicate detection
        let image_bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_image)
            .map_err(|e| HistoryError::ImageError(e.to_string()))?;

        // Calculate content hash for duplicate detection
        let content_hash = calculate_content_hash(&image_bytes);

        // Check if this exact image already exists
        if let Some(existing_item) = self.get_item_by_hash(&content_hash)? {
            // Return existing item instead of creating duplicate
            return Ok(existing_item);
        }

        // First save to legacy table for backwards compatibility
        let record = self.save_screenshot(base64_image, width, height, max_history)?;

        // Also save to unified history with content_hash
        self.conn.execute(
            "INSERT INTO history_items (id, item_type, created_at, filename, thumbnail_filename, width, height, is_pinned, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
            params![
                record.id,
                HistoryItemType::Screenshot.to_string(),
                record.created_at,
                record.filename,
                format!("thumb_{}", record.filename.replace(".png", ".jpg")),
                width,
                height,
                content_hash,
            ],
        )?;

        // Cleanup old items from unified table
        self.cleanup_old_history_items(max_history)?;

        Ok(HistoryItem {
            id: record.id,
            item_type: HistoryItemType::Screenshot,
            created_at: record.created_at,
            filename: Some(record.filename),
            thumbnail: Some(record.thumbnail),
            width: Some(width),
            height: Some(height),
            saved_path: record.saved_path,
            text_content: None,
            text_preview: None,
            color_hex: None,
            color_rgb: None,
            color_hsl: None,
            source_app: None,
            is_pinned: false,
        })
    }

    /// Get all unified history items (screenshots + clipboard)
    pub fn get_all_history_items(
        &self,
        filter_type: Option<HistoryItemType>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<HistoryItem>, HistoryError> {
        let limit_val = limit.map(|l| l as i64).unwrap_or(-1); // -1 means no limit in SQLite
        let offset_val = offset.map(|o| o as i64).unwrap_or(0);

        let items: Vec<HistoryItem> = if let Some(ref item_type) = filter_type {
            let mut stmt = self.conn.prepare(
                "SELECT id, item_type, created_at, filename, thumbnail_filename, width, height, saved_path, text_content, text_preview, color_hex, color_rgb, color_hsl, source_app, is_pinned
                 FROM history_items WHERE item_type = ?1 ORDER BY is_pinned DESC, created_at DESC LIMIT ?2 OFFSET ?3"
            )?;
            let rows = stmt.query_map(
                params![item_type.to_string(), limit_val, offset_val],
                |row| self.row_to_history_item(row),
            )?;
            rows.filter_map(|r| r.ok()).collect()
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, item_type, created_at, filename, thumbnail_filename, width, height, saved_path, text_content, text_preview, color_hex, color_rgb, color_hsl, source_app, is_pinned
                 FROM history_items ORDER BY is_pinned DESC, created_at DESC LIMIT ?1 OFFSET ?2"
            )?;
            let rows = stmt.query_map(params![limit_val, offset_val], |row| {
                self.row_to_history_item(row)
            })?;
            rows.filter_map(|r| r.ok()).collect()
        };

        Ok(items)
    }

    fn row_to_history_item(&self, row: &rusqlite::Row) -> Result<HistoryItem, rusqlite::Error> {
        let id: String = row.get(0)?;
        let item_type_str: String = row.get(1)?;
        let created_at: String = row.get(2)?;
        let filename: Option<String> = row.get(3)?;
        let thumbnail_filename: Option<String> = row.get(4)?;
        let width: Option<u32> = row.get(5)?;
        let height: Option<u32> = row.get(6)?;
        let saved_path: Option<String> = row.get(7)?;
        let text_content: Option<String> = row.get(8)?;
        let text_preview: Option<String> = row.get(9)?;
        let color_hex: Option<String> = row.get(10)?;
        let color_rgb: Option<String> = row.get(11)?;
        let color_hsl: Option<String> = row.get(12)?;
        let source_app: Option<String> = row.get(13)?;
        let is_pinned: i32 = row.get(14)?;

        let item_type = item_type_str.parse().unwrap_or(HistoryItemType::Screenshot);

        // Load thumbnail if it's an image type
        let thumbnail = if let Some(ref thumb_filename) = thumbnail_filename {
            let thumbnail_path = self.thumbnails_dir.join(thumb_filename);
            if thumbnail_path.exists() {
                fs::read(&thumbnail_path)
                    .map(|bytes| base64::engine::general_purpose::STANDARD.encode(&bytes))
                    .ok()
            } else {
                None
            }
        } else {
            None
        };

        Ok(HistoryItem {
            id,
            item_type,
            created_at,
            filename,
            thumbnail,
            width,
            height,
            saved_path,
            text_content,
            text_preview,
            color_hex,
            color_rgb,
            color_hsl,
            source_app,
            is_pinned: is_pinned != 0,
        })
    }

    /// Get a single history item by ID
    pub fn get_history_item(&self, id: &str) -> Result<Option<HistoryItem>, HistoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, item_type, created_at, filename, thumbnail_filename, width, height, saved_path,
                    text_content, text_preview, color_hex, color_rgb, color_hsl, source_app, is_pinned
             FROM history_items WHERE id = ?1",
        )?;

        let item = stmt
            .query_row(params![id], |row| self.row_to_history_item(row))
            .optional()?;

        Ok(item)
    }

    /// Load image as base64 from filename
    pub fn load_image_base64(
        &self,
        filename: &str,
        item_type: HistoryItemType,
    ) -> Result<String, HistoryError> {
        let image_path = match item_type {
            HistoryItemType::Screenshot => self.screenshots_dir.join(filename),
            HistoryItemType::ClipboardImage => self.clipboard_dir.join(filename),
            _ => return Err(HistoryError::ImageError("Not an image type".to_string())),
        };

        if !image_path.exists() {
            return Err(HistoryError::FileSystemError(format!(
                "Image not found: {}",
                filename
            )));
        }

        let bytes = fs::read(&image_path)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
    }

    /// Get image data for a history item (screenshot or clipboard image)
    pub fn get_history_item_image(&self, id: &str) -> Result<Option<String>, HistoryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT item_type, filename FROM history_items WHERE id = ?1")?;

        let result: Option<(String, Option<String>)> = stmt
            .query_row(params![id], |row| Ok((row.get(0)?, row.get(1)?)))
            .ok();

        if let Some((item_type_str, filename)) = result {
            if let Some(filename) = filename {
                let item_type: HistoryItemType =
                    item_type_str.parse().unwrap_or(HistoryItemType::Screenshot);

                let image_path = match item_type {
                    HistoryItemType::Screenshot => self.screenshots_dir.join(&filename),
                    HistoryItemType::ClipboardImage => self.clipboard_dir.join(&filename),
                    HistoryItemType::ClipboardText | HistoryItemType::ColorPick => return Ok(None),
                };

                if image_path.exists() {
                    let bytes = fs::read(&image_path)?;
                    let base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    return Ok(Some(base64));
                }
            }
        }

        // Fallback to legacy screenshots table
        self.get_screenshot_image(id)
    }

    /// Delete a history item
    pub fn delete_history_item(&mut self, id: &str) -> Result<(), HistoryError> {
        // Get item info first using a scoped query
        let result: Option<(String, Option<String>, Option<String>)> = {
            let mut stmt = self.conn.prepare(
                "SELECT item_type, filename, thumbnail_filename FROM history_items WHERE id = ?1",
            )?;

            stmt.query_row(params![id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .ok()
        };

        let mut is_screenshot = false;

        if let Some((item_type_str, filename, thumbnail_filename)) = result {
            let item_type: HistoryItemType =
                item_type_str.parse().unwrap_or(HistoryItemType::Screenshot);

            // Delete files if they exist
            if let Some(filename) = filename {
                let image_path = match item_type {
                    HistoryItemType::Screenshot => self.screenshots_dir.join(&filename),
                    HistoryItemType::ClipboardImage => self.clipboard_dir.join(&filename),
                    HistoryItemType::ClipboardText | HistoryItemType::ColorPick => PathBuf::new(),
                };
                let _ = fs::remove_file(image_path);
            }

            if let Some(thumb_filename) = thumbnail_filename {
                let _ = fs::remove_file(self.thumbnails_dir.join(&thumb_filename));
            }

            is_screenshot = item_type == HistoryItemType::Screenshot;
        }

        // Delete from unified table
        self.conn
            .execute("DELETE FROM history_items WHERE id = ?1", params![id])?;

        // Also delete from legacy table if it's a screenshot
        if is_screenshot {
            let _ = self
                .conn
                .execute("DELETE FROM screenshots WHERE id = ?1", params![id]);
        }

        Ok(())
    }

    /// Toggle pin status
    pub fn toggle_pin(&mut self, id: &str) -> Result<bool, HistoryError> {
        // Get current pin status
        let is_pinned: i32 = self.conn.query_row(
            "SELECT is_pinned FROM history_items WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;

        let new_status = if is_pinned == 0 { 1 } else { 0 };

        self.conn.execute(
            "UPDATE history_items SET is_pinned = ?1 WHERE id = ?2",
            params![new_status, id],
        )?;

        Ok(new_status != 0)
    }

    /// Clear all unified history
    pub fn clear_all_unified(&mut self) -> Result<(), HistoryError> {
        // Clear legacy first
        self.clear_all()?;

        // Clear clipboard directory
        if self.clipboard_dir.exists() {
            for entry in fs::read_dir(&self.clipboard_dir)? {
                if let Ok(entry) = entry {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }

        // Clear unified table
        self.conn.execute("DELETE FROM history_items", [])?;

        Ok(())
    }

    /// Search text content in clipboard items
    pub fn search_history(&self, query: &str) -> Result<Vec<HistoryItem>, HistoryError> {
        let search_pattern = format!("%{}%", query);

        let mut stmt = self.conn.prepare(
            "SELECT id, item_type, created_at, filename, thumbnail_filename, width, height, saved_path, text_content, text_preview, color_hex, color_rgb, color_hsl, source_app, is_pinned
             FROM history_items
             WHERE text_content LIKE ?1 OR text_preview LIKE ?1 OR color_hex LIKE ?1
             ORDER BY is_pinned DESC, created_at DESC"
        )?;

        let rows = stmt.query_map(params![search_pattern], |row| self.row_to_history_item(row))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| HistoryError::DatabaseError(e.to_string()))
    }

    /// Get the last clipboard content hash for duplicate detection
    pub fn get_last_clipboard_hash(&self) -> Result<Option<String>, HistoryError> {
        let result: Option<(String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT item_type, text_content FROM history_items
             WHERE item_type IN ('clipboard_text', 'clipboard_image')
             ORDER BY created_at DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        if let Some((item_type, text_content)) = result {
            if item_type == "clipboard_text" {
                return Ok(text_content);
            }
            // For images, we'd need to store a hash - for now return None
        }

        Ok(None)
    }

    /// Cleanup old history items (keeps pinned items)
    fn cleanup_old_history_items(&mut self, max_count: usize) -> Result<(), HistoryError> {
        // Get count of non-pinned items
        let count: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM history_items WHERE is_pinned = 0",
            [],
            |row| row.get(0),
        )?;

        if count > max_count {
            let to_delete = count - max_count;

            // Get oldest non-pinned items to delete
            let mut stmt = self.conn.prepare(
                "SELECT id, item_type, filename, thumbnail_filename FROM history_items
                 WHERE is_pinned = 0 ORDER BY created_at ASC LIMIT ?1",
            )?;

            let old_items: Vec<(String, String, Option<String>, Option<String>)> = stmt
                .query_map(params![to_delete], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?
                .filter_map(|r| r.ok())
                .collect();

            for (id, item_type_str, filename, thumbnail_filename) in old_items {
                let item_type: HistoryItemType =
                    item_type_str.parse().unwrap_or(HistoryItemType::Screenshot);

                // Delete files
                if let Some(filename) = filename {
                    let image_path = match item_type {
                        HistoryItemType::Screenshot => self.screenshots_dir.join(&filename),
                        HistoryItemType::ClipboardImage => self.clipboard_dir.join(&filename),
                        HistoryItemType::ClipboardText | HistoryItemType::ColorPick => {
                            PathBuf::new()
                        }
                    };
                    let _ = fs::remove_file(image_path);
                }

                if let Some(thumb_filename) = thumbnail_filename {
                    let _ = fs::remove_file(self.thumbnails_dir.join(&thumb_filename));
                }

                // Delete from unified table
                self.conn
                    .execute("DELETE FROM history_items WHERE id = ?1", params![id])?;

                // Also delete from legacy if screenshot
                if item_type == HistoryItemType::Screenshot {
                    self.conn
                        .execute("DELETE FROM screenshots WHERE id = ?1", params![id])?;
                }
            }
        }

        Ok(())
    }
}
