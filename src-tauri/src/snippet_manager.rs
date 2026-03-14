use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetItem {
    pub id: String,
    pub title: String,
    pub content_type: String, // "text" or "image"
    pub content: String,
}

pub struct SnippetManager {
    file_path: PathBuf,
    snippets: Mutex<Vec<SnippetItem>>,
}

impl SnippetManager {
    pub fn new() -> Result<Self, String> {
        let app_dir = dirs::data_local_dir()
            .ok_or_else(|| "Cannot find app data dir".to_string())?
            .join("screenshot-tool");
            
        fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;

        let file_path = app_dir.join("snippets.json");
        let snippets = if file_path.exists() {
            match fs::read_to_string(&file_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| Vec::new()),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        Ok(Self {
            file_path,
            snippets: Mutex::new(snippets),
        })
    }

    pub fn get_all(&self) -> Vec<SnippetItem> {
        let guard = self.snippets.lock().unwrap();
        guard.clone()
    }

    pub fn add(&self, title: String, content_type: String, content: String) -> SnippetItem {
        let new_item = SnippetItem {
            id: Uuid::new_v4().to_string(),
            title,
            content_type,
            content,
        };

        {
            let mut guard = self.snippets.lock().unwrap();
            guard.push(new_item.clone());
        }

        self.save();
        new_item
    }

    pub fn delete(&self, id: &str) -> bool {
        let mut guard = self.snippets.lock().unwrap();
        let initial_len = guard.len();
        guard.retain(|item| item.id != id);
        
        let changed = guard.len() < initial_len;
        if changed {
            drop(guard);
            self.save();
        }
        changed
    }

    pub fn update(&self, id: &str, title: String, content: String) -> bool {
        let mut guard = self.snippets.lock().unwrap();
        if let Some(item) = guard.iter_mut().find(|i| i.id == id) {
            item.title = title;
            item.content = content;
            drop(guard);
            self.save();
            true
        } else {
            false
        }
    }

    pub fn save(&self) {
        if let Ok(guard) = self.snippets.lock() {
            if let Ok(json) = serde_json::to_string_pretty(&*guard) {
                let _ = fs::write(&self.file_path, json);
            }
        }
    }
}
